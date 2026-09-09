/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   gc.rs                                                :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `vault gc` — removing chunks that no version of any manifest still references.
//!
//! Content-addressed chunks are never overwritten, so every edit to a large file leaves its
//! predecessors in the store. That is what makes an older version still restorable, and it
//! is also why the store only ever grows without this verb.
//!
//! Collection is the operation in this product that can destroy data silently, so it is
//! built to refuse rather than to guess. It walks EVERY version of every manifest, not just
//! the current one, because a chunk that only an older version references is history rather
//! than garbage. It refuses outright if it found no manifests at all, since "nothing is
//! referenced" and "nothing was read" look identical from the deletion side. It never
//! touches an object younger than a grace period, because an interrupted push leaves chunks
//! in the store before the list that names them exists. And it is a dry run unless the
//! caller says otherwise.

use crate::adapters::api::Session;
use crate::adapters::sigv4;
use crate::core::chunk::ChunkSet;
use crate::core::manifest::Manifest;
use crate::core::projpath;
use crate::ops::largeobj;
use crate::ui;
use std::collections::HashSet;
use tonic::Request;
use vault42_proto::vault::v1::LsRequest;

/// Chunks younger than this are never collected, however unreferenced they look.
///
/// An interrupted push has already uploaded chunks that no manifest names yet; resuming it
/// is what makes a gigabyte survivable, and collecting them mid-flight is what would make
/// resuming impossible. Hours rather than minutes because a genuinely large upload can run
/// for a long time.
const DEFAULT_GRACE_HOURS: i64 = 24;

/// The row count at which a secret listing may have been silently truncated.
///
/// Today's SQLite store applies the prefix in SQL and caps nothing, so it cannot produce a
/// truncated listing. The grobase-backed store can: it fetches a flat 500-row page and
/// applies the prefix afterwards, so a short list is indistinguishable from a complete one.
/// That backend is off unless `VAULT42_STORE=grobase` is set exactly, which is precisely why
/// this check stays — every manifest missing from a listing is a manifest whose chunks look
/// like garbage, and collection deletes on the strength of it. Refusing at the cap costs
/// nothing against a store that never reaches it, and is the difference between collection
/// being safe and being safe by accident of which backend someone configured.
const LISTING_CAP: usize = 500;

impl Session {
    /// Delete every stored chunk that no manifest version references. Dry run unless `apply`.
    pub async fn cmd_gc(&mut self, grace_hours: Option<i64>, apply: bool) -> anyhow::Result<()> {
        let grace = grace_hours.unwrap_or(DEFAULT_GRACE_HOURS).max(0);
        let referenced = self.referenced_chunks().await?;
        let owner = largeobj::Owner {
            identity: &self.identity,
            principal: &self.principal,
        };
        let naming = largeobj::naming(&owner);
        let store = self.store.as_ref().ok_or_else(|| {
            anyhow::anyhow!("this profile names no object store, so there is nothing to collect")
        })?;
        let cutoff = sigv4::instant_ago(grace * 3600)?;
        let stored = store.list(&naming.prefix()).await?;
        let mut removed = 0usize;
        for object in &stored {
            if referenced.contains(&object.name) || object.last_modified >= cutoff {
                continue;
            }
            removed += 1;
            if apply {
                store.delete(&object.name).await?;
            }
            ui::field(if apply { "removed" } else { "would remove" }, &object.name);
        }
        report(stored.len(), referenced.len(), removed, apply);
        Ok(())
    }

    /// Every chunk name any version of any of this identity's manifests still names.
    ///
    /// Refuses when it found no manifest at all. An empty reference set makes every stored
    /// chunk look like garbage, so the one case where a wrong answer deletes everything is
    /// the case that must not be allowed to proceed quietly.
    async fn referenced_chunks(&mut self) -> anyhow::Result<HashSet<String>> {
        let manifests = self.manifest_paths().await?;
        if manifests.is_empty() {
            anyhow::bail!(
                "no manifests found, so nothing could be proved referenced — refusing to collect"
            );
        }
        let mut referenced = HashSet::new();
        for (path, latest) in manifests {
            for version in 1..=latest {
                self.chunks_of_manifest(&path, version, &mut referenced)
                    .await?;
            }
        }
        Ok(referenced)
    }

    /// Every manifest path this identity owns, with its latest version.
    ///
    /// Lists WITHOUT a prefix and filters here, because the server applies the prefix after
    /// its own page limit: asking for the manifests alone would hand back a short list that
    /// looks complete. The unfiltered count is the only thing that says whether it is.
    async fn manifest_paths(&mut self) -> anyhow::Result<Vec<(String, u64)>> {
        let mut request = Request::new(LsRequest {
            prefix: String::new(),
        });
        self.authorize(&mut request, "/vault.v1.Vault/Ls")?;
        let secrets = self.client.ls(request).await?.into_inner().secrets;
        ensure_listing_complete(secrets.len())?;
        let prefix = format!("{}/m/", projpath::RESERVED_PREFIX);
        Ok(secrets
            .into_iter()
            .filter(|s| s.path.starts_with(&prefix))
            .map(|s| (s.path, s.version))
            .collect())
    }

    /// Add every chunk named by one manifest version, across every version of its entries.
    ///
    /// A missing version is skipped rather than fatal: a tombstoned or pruned revision is
    /// ordinary, and refusing to collect because one old version is gone would mean the
    /// store never shrinks again.
    async fn chunks_of_manifest(
        &mut self,
        path: &str,
        version: u64,
        into: &mut HashSet<String>,
    ) -> anyhow::Result<()> {
        let Ok(bytes) = self.fetch_version(path, version).await else {
            return Ok(());
        };
        let Ok(manifest) = Manifest::parse(&bytes) else {
            return Ok(());
        };
        for entry in manifest.entries.iter().filter(|e| e.chunked) {
            let latest = self.current_version(&entry.vault_path).await.unwrap_or(0);
            for rev in 1..=latest {
                if let Ok(raw) = self.fetch_version(&entry.vault_path, rev).await {
                    if let Ok(set) = ChunkSet::from_bytes(&raw) {
                        into.extend(set.chunks.into_iter().map(|c| c.name));
                    }
                }
            }
        }
        Ok(())
    }
}

/// Refuse a listing that may have been truncated, since collection deletes on its strength.
fn ensure_listing_complete(rows: usize) -> anyhow::Result<()> {
    if rows >= LISTING_CAP {
        anyhow::bail!(
            "the vault returned {rows} secrets, at or above the {LISTING_CAP}-row listing \
             limit, so some manifests may be missing — refusing to collect"
        );
    }
    Ok(())
}

/// Print what collection found, so a dry run is worth reading on its own.
fn report(stored: usize, referenced: usize, removed: usize, apply: bool) {
    ui::field("stored chunks", &stored.to_string());
    ui::field("still referenced", &referenced.to_string());
    if removed == 0 {
        ui::success("nothing to collect");
        return;
    }
    if apply {
        ui::success(&format!("collected {removed} unreferenced chunk(s)"));
    } else {
        ui::success(&format!(
            "{removed} chunk(s) would be collected — re-run with --apply"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The boundary is the whole point: one row below the cap is a listing that is provably
    /// complete, and one at it is a listing that might not be. Collection must refuse the
    /// second, because every manifest it did not see is a manifest whose chunks it would
    /// delete as garbage.
    #[test]
    fn a_listing_at_the_page_limit_is_refused() {
        assert!(ensure_listing_complete(LISTING_CAP - 1).is_ok());
        assert!(ensure_listing_complete(LISTING_CAP).is_err());
        assert!(ensure_listing_complete(LISTING_CAP + 1).is_err());
    }

    #[test]
    fn an_empty_listing_is_complete_as_far_as_truncation_goes() {
        assert!(ensure_listing_complete(0).is_ok());
    }
}
