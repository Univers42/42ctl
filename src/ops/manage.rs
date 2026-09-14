/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   manage.rs                                            :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `ls`, `rm`, and `rotate`. Listing and removal are owner-scoped by the server to the
//! calling identity; `rotate` re-seals a secret under a fresh DEK and pushes it at the
//! next version. All of it is signed + contract-bound and, for rotate, sealed locally.

use crate::adapters::api::Session;
use crate::adapters::compose::{self, SelfSeal};
use crate::cmd::bulk;
use crate::ui;
use tonic::Request;
use vault42_proto::vault::v1::{LsRequest, PushRequest, RmRequest};

impl Session {
    /// List the caller's secrets under `prefix`: `Path Version Updated`. `scope` is the prefix
    /// and whether 42ctl's own records are included (`vault ls --all`, and always for `db ls`,
    /// which is the record-level view).
    ///
    /// `--format`/`--filter` take precedence. Without them a pipe still gets the historical
    /// tab-separated `path version updated_at` lines, so scripts written against that keep
    /// working, and an empty vault on a terminal still gets its hint.
    pub async fn cmd_ls(
        &mut self,
        scope: (&str, bool),
        shape: ui::Shape<'_>,
    ) -> anyhow::Result<()> {
        let (prefix, all) = scope;
        let mut request = Request::new(LsRequest {
            prefix: prefix.to_string(),
        });
        self.authorize(&mut request, "/vault.v1.Vault/Ls")?;
        let mut secrets = self.client.ls(request).await?.into_inner().secrets;
        secrets.retain(|secret| all || !is_own_record(&secret.path));
        let unshaped = shape.is_default();
        if unshaped && !ui::styled() {
            for secret in &secrets {
                println!("{}\t{}\t{}", secret.path, secret.version, secret.updated_at);
            }
            return Ok(());
        }
        if unshaped && secrets.is_empty() {
            println!(
                "{}",
                ui::dim("no secrets yet — add one with `42ctl vault set <path>`")
            );
            return Ok(());
        }
        let rows = secrets
            .iter()
            .map(|s| {
                serde_json::json!({
                    "Path": s.path, "Version": s.version, "Updated": ui::reltime(s.updated_at),
                })
            })
            .collect();
        ui::render(&["Path", "Version", "Updated"], rows, shape)
    }

    /// Remove every version of each path, continuing past one that fails.
    pub async fn cmd_rm(&mut self, paths: &[String]) -> anyhow::Result<()> {
        let attempt = bulk::targets(paths);
        let mut failed = Vec::new();
        for path in &attempt {
            if let Err(error) = self.rm_one(path).await {
                bulk::failure(path, &error);
                failed.push(path.clone());
            }
        }
        bulk::report(&failed, attempt.len())
    }

    /// Remove every version of one `path`. A path that is not there is reported, not an error:
    /// the vault holds nothing under it either way, which is what the caller asked for.
    async fn rm_one(&mut self, path: &str) -> anyhow::Result<()> {
        if is_own_record(path) {
            anyhow::bail!(
                "this is 42ctl's own record (a note, or a push's manifest or chunk list) — \
                 removing it loses what it holds; `42ctl note rm` removes a note"
            );
        }
        let mut request = Request::new(RmRequest {
            path: path.to_string(),
            version: 0,
        });
        self.authorize(&mut request, "/vault.v1.Vault/Rm")?;
        let tombstoned = self.client.rm(request).await?.into_inner().tombstoned;
        if tombstoned {
            ui::success(&format!("removed {path}"));
        } else {
            println!("{}", ui::warn(&format!("{path}: not found")));
        }
        Ok(())
    }

    /// Re-seal the secret at `path` under a fresh DEK and push it at the next version.
    pub async fn cmd_rotate(&mut self, path: &str) -> anyhow::Result<()> {
        let current = self.current_version(path).await?;
        if current == 0 {
            anyhow::bail!("no secret at {path} to rotate");
        }
        let plaintext = self.fetch_plaintext(path).await?;
        let envelope = compose::self_envelope(
            &self.identity,
            &SelfSeal {
                owner: &self.principal,
                path,
                rev: current + 1,
                plaintext: plaintext.as_slice(),
            },
        )?;
        let mut request = Request::new(PushRequest {
            path: path.to_string(),
            envelope,
            expected_prev_rev: current,
        });
        self.authorize(&mut request, "/vault.v1.Vault/Rotate")?;
        let version = self.client.rotate(request).await?.into_inner().version;
        ui::success(&format!("rotated {path} to v{version}"));
        Ok(())
    }
}

/// Whether `path` is one of 42ctl's own records — notes, and a push's manifest and chunk
/// lists — rather than a secret somebody stored. Those live under the reserved prefix that no
/// project path may use, so a secret can never be mistaken for one.
pub(super) fn is_own_record(path: &str) -> bool {
    path.split('/').next() == Some(crate::core::projpath::RESERVED_PREFIX) && path.contains('/')
}

#[cfg(test)]
mod tests {
    use super::is_own_record;

    /// Only the reserved first segment marks a record; a secret merely NAMED like it is not one.
    #[test]
    fn only_the_reserved_prefix_is_a_record() {
        assert!(is_own_record("__42ctl/m/project"));
        assert!(is_own_record("__42ctl/nb/project/8ce9"));
        assert!(!is_own_record("app/__42ctl/m/project"));
        assert!(!is_own_record("__42ctl_backup/x"));
        assert!(!is_own_record("__42ctl"));
        assert!(!is_own_record("app/DB_URL"));
    }
}
