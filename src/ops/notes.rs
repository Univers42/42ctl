/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   notes.rs                                            :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `note add|get|ls|rm` — project notes stored as `kind=Note` entries in the SAME
//! encrypted manifest as `push`/`pull` (the only place real paths live). Notes reuse the
//! env tree's traversal defense and byte-exact crypto, but `pull` skips notes and only
//! `note ls` selects them, so the two families never clobber each other. The server sees
//! only opaque ciphertext — never the note's text and never its real path.

use crate::adapters::api::Session;
use crate::adapters::compose::{self, ProjectSeal};
use crate::adapters::derive;
use crate::core::manifest::{Entry, Manifest};
use crate::core::{project, projpath};
use crate::ops::sync::MAX_BLOB;
use crate::ui;
use std::io::Write;
use vault42_core::Kind;

impl Session {
    /// Seal `bytes` as a Note at `rel_raw` and record it in the project manifest.
    pub async fn cmd_note_add(&mut self, explicit_id: Option<&str>, rel_raw: &str, bytes: &[u8]) -> anyhow::Result<()> {
        let (proj, created) = project::open(&std::env::current_dir()?, explicit_id)?;
        if created {
            ui::field("project", &proj.project_id);
        }
        let rel = projpath::validate_stored(rel_raw)?; // sec: notes are path-addressed too
        if bytes.len() > MAX_BLOB {
            anyhow::bail!("note exceeds the 64 MiB blob ceiling");
        }
        let id = derive::secret_id(&self.principal, &format!("{}/note/{}", proj.project_id, rel.as_str()));
        let vault_path = format!("{}/nb/{}/{}", projpath::RESERVED_PREFIX, proj.project_id, id);
        let mut manifest = self
            .load_manifest(&proj.project_id)
            .await?
            .unwrap_or_else(|| Manifest::new(&proj.project_id));
        let rev = self.current_version(&vault_path).await?;
        let env = compose::project_envelope(
            &self.identity,
            &ProjectSeal {
                owner: &self.principal,
                vault_path: &vault_path,
                project_id: &proj.project_id,
                kind: Kind::Note,
                mode: 0o600,
                rev: rev + 1,
                plaintext: bytes,
            },
        )?;
        self.push_blob(&vault_path, env, rev, "/vault.v1.Vault/Push").await?;
        manifest.upsert(Entry {
            relative_path: rel.as_str().to_string(),
            vault_path,
            mode: 0o600,
            kind: Kind::Note as u8,
        });
        self.push_manifest(&proj.project_id, &manifest).await?;
        ui::success(&format!("note {} saved in project {}", rel.as_str(), proj.project_id));
        Ok(())
    }

    /// Fetch + decrypt the note at `rel_raw` and write it to stdout (byte-exact).
    pub async fn cmd_note_get(&mut self, explicit_id: Option<&str>, rel_raw: &str) -> anyhow::Result<()> {
        let (proj, _) = project::open(&std::env::current_dir()?, explicit_id)?;
        let manifest = self.load_manifest(&proj.project_id).await?.ok_or_else(|| {
            anyhow::anyhow!("project {} has no notes yet", proj.project_id)
        })?;
        let entry = find_note(&manifest, rel_raw)?;
        let bytes = self.fetch_blob(&entry.vault_path).await?;
        std::io::stdout().write_all(&bytes)?;
        Ok(())
    }

    /// List the project's notes (the `kind=Note` manifest entries).
    pub async fn cmd_note_ls(&mut self, explicit_id: Option<&str>) -> anyhow::Result<()> {
        let (proj, _) = project::open(&std::env::current_dir()?, explicit_id)?;
        let manifest = self
            .load_manifest(&proj.project_id)
            .await?
            .unwrap_or_else(|| Manifest::new(&proj.project_id));
        let rows: Vec<Vec<String>> = manifest
            .entries
            .iter()
            .filter(|e| e.kind == Kind::Note as u8)
            .map(|e| vec![e.relative_path.clone()])
            .collect();
        ui::table(&["note"], &rows);
        Ok(())
    }

    /// Remove the note's manifest entry, making it unreachable (ZK: no name↔blob link left).
    pub async fn cmd_note_rm(&mut self, explicit_id: Option<&str>, rel_raw: &str) -> anyhow::Result<()> {
        let (proj, _) = project::open(&std::env::current_dir()?, explicit_id)?;
        let mut manifest = self.load_manifest(&proj.project_id).await?.ok_or_else(|| {
            anyhow::anyhow!("project {} has no notes yet", proj.project_id)
        })?;
        find_note(&manifest, rel_raw)?;
        manifest.remove(rel_raw); // ponytail: ciphertext blob left orphaned — a GC sweep RPC could reclaim it
        self.push_manifest(&proj.project_id, &manifest).await?;
        ui::success(&format!("note {rel_raw} removed"));
        Ok(())
    }
}

/// Find the `kind=Note` entry for `rel`, or error if there is no such note.
fn find_note<'a>(manifest: &'a Manifest, rel: &str) -> anyhow::Result<&'a Entry> {
    manifest
        .entries
        .iter()
        .find(|e| e.relative_path == rel && e.kind == Kind::Note as u8)
        .ok_or_else(|| anyhow::anyhow!("no note {rel} in this project"))
}
