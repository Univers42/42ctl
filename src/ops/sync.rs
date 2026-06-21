/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   sync.rs                                              :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `push` / `pull` — path-aware project sync. `push` scans the project's `*.env*` tree,
//! seals each file under an OPAQUE vault path, and records the real paths only in the
//! encrypted manifest. `pull` fetches the manifest, validates every path (Zip-Slip
//! guard), decrypts each blob, and materializes the tree byte-exact (dry-run by default).
//! The server sees neither plaintext nor real paths.

use crate::adapters::api::Session;
use crate::adapters::compose::{self, ProjectSeal};
use crate::adapters::{decrypt, derive};
use crate::core::manifest::{Entry, Manifest};
use crate::core::{materialize, project, projpath};
use crate::ui;
use tonic::{Code, Request};
use vault42_core::Kind;
use vault42_proto::vault::v1::{GetRequest, PushRequest};
use zeroize::Zeroizing;

pub(crate) const MAX_BLOB: usize = 64 * 1024 * 1024;

impl Session {
    /// Scan the project, seal + push each matched file under an opaque vault path, and
    /// push the encrypted manifest mapping real paths → those vault paths.
    pub async fn cmd_push(&mut self, explicit_id: Option<&str>) -> anyhow::Result<()> {
        let cwd = std::env::current_dir()?;
        let (proj, created) = project::open(&cwd, explicit_id)?;
        if created {
            ui::field("project", &proj.project_id);
        }
        let files = project::scan(&proj)?;
        let mut manifest = self
            .load_manifest(&proj.project_id)
            .await?
            .unwrap_or_else(|| Manifest::new(&proj.project_id));
        for file in &files {
            let rel = projpath::canonicalize_for_storage(file, &proj.root)?;
            let vault_path = blob_path(&proj.project_id, &self.principal, rel.as_str());
            let mode = file_mode(file);
            let plaintext = Zeroizing::new(std::fs::read(file)?);
            if plaintext.len() > MAX_BLOB {
                anyhow::bail!("{} exceeds the 64 MiB blob ceiling", rel.as_str());
            }
            let rev = self.current_version(&vault_path).await?;
            let env = compose::project_envelope(
                &self.identity,
                &ProjectSeal {
                    owner: &self.principal,
                    vault_path: &vault_path,
                    project_id: &proj.project_id,
                    kind: Kind::EnvFile,
                    mode,
                    rev: rev + 1,
                    plaintext: plaintext.as_slice(),
                },
            )?;
            self.push_blob(&vault_path, env, rev, "/vault.v1.Vault/Push").await?;
            manifest.upsert(Entry {
                relative_path: rel.as_str().to_string(),
                vault_path,
                mode,
                kind: Kind::EnvFile as u8,
            });
        }
        self.push_manifest(&proj.project_id, &manifest).await?;
        ui::success(&format!(
            "pushed {} file(s) + manifest for project {}",
            files.len(),
            proj.project_id
        ));
        Ok(())
    }

    /// Fetch the manifest, validate every path, decrypt each blob, and materialize the
    /// tree (dry-run unless `apply`).
    pub async fn cmd_pull(
        &mut self,
        explicit_id: Option<&str>,
        opts: materialize::Opts,
    ) -> anyhow::Result<()> {
        let cwd = std::env::current_dir()?;
        let (proj, _) = project::open(&cwd, explicit_id)?;
        let manifest = self.load_manifest(&proj.project_id).await?.ok_or_else(|| {
            anyhow::anyhow!("no manifest for project {} (push first, or pass --project)", proj.project_id)
        })?;
        let mut plans = Vec::new();
        for entry in manifest.entries.iter().filter(|e| e.kind != Kind::Note as u8) {
            let rel = projpath::validate_stored(&entry.relative_path)?; // sec: validate before any FS op
            let bytes = self.fetch_blob(&entry.vault_path).await?;
            plans.push(materialize::Plan { rel, bytes, mode: entry.mode });
        }
        if !opts.apply {
            ui::field("pull", "dry-run — re-run with --apply to write");
        }
        materialize::materialize(&proj.root, plans, &opts)
    }

    /// Push one opaque envelope at `vault_path` with optimistic concurrency.
    pub(crate) async fn push_blob(&mut self, vault_path: &str, envelope: Vec<u8>, rev: u64, method: &str) -> anyhow::Result<()> {
        let mut request = Request::new(PushRequest {
            path: vault_path.to_string(),
            envelope,
            expected_prev_rev: rev,
        });
        self.authorize(&mut request, method)?;
        self.client.push(request).await?;
        Ok(())
    }

    /// Seal + push the manifest (kind=Manifest) at the project's reserved manifest path.
    pub(crate) async fn push_manifest(&mut self, project_id: &str, manifest: &Manifest) -> anyhow::Result<()> {
        let vault_path = manifest_path(project_id);
        let rev = self.current_version(&vault_path).await?;
        let bytes = manifest.to_bytes()?;
        let env = compose::project_envelope(
            &self.identity,
            &ProjectSeal {
                owner: &self.principal,
                vault_path: &vault_path,
                project_id,
                kind: Kind::Manifest,
                mode: 0o600,
                rev: rev + 1,
                plaintext: &bytes,
            },
        )?;
        self.push_blob(&vault_path, env, rev, "/vault.v1.Vault/Push").await
    }

    /// Fetch + decrypt the manifest (None when the project has nothing pushed yet).
    pub(crate) async fn load_manifest(&mut self, project_id: &str) -> anyhow::Result<Option<Manifest>> {
        let vault_path = manifest_path(project_id);
        match self.get_blob(&vault_path).await {
            Ok(bytes) => Ok(Some(Manifest::parse(&bytes)?)),
            Err(status) if status.code() == Code::NotFound => Ok(None),
            Err(status) => Err(status.into()),
        }
    }

    /// Fetch + decrypt the blob at `vault_path` (anyhow error on any failure).
    pub(crate) async fn fetch_blob(&mut self, vault_path: &str) -> anyhow::Result<Zeroizing<Vec<u8>>> {
        Ok(self.get_blob(vault_path).await?)
    }

    /// The raw Get → decrypt, surfacing the tonic Status so callers can match NotFound.
    async fn get_blob(&mut self, vault_path: &str) -> Result<Zeroizing<Vec<u8>>, tonic::Status> {
        let expected = derive::secret_id(&self.principal, vault_path);
        let mut request = Request::new(GetRequest {
            path: vault_path.to_string(),
            version: 0,
        });
        self.authorize(&mut request, "/vault.v1.Vault/Get")
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        let resp = self.client.get(request).await?.into_inner();
        decrypt::open_envelope(&self.identity, &resp, &expected, 0)
            .map_err(|e| tonic::Status::internal(e.to_string()))
    }
}

/// The opaque server path for a project file's blob (the real path never appears here).
fn blob_path(project_id: &str, principal: &str, rel: &str) -> String {
    let id = derive::secret_id(principal, &format!("{project_id}/{rel}"));
    format!("{}/b/{project_id}/{id}", projpath::RESERVED_PREFIX)
}

/// The reserved server path for a project's manifest.
fn manifest_path(project_id: &str) -> String {
    format!("{}/m/{project_id}", projpath::RESERVED_PREFIX)
}

/// The file's Unix mode (low 9 bits), or 0o600 on non-Unix.
fn file_mode(file: &std::path::Path) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(file).map(|m| m.mode() & 0o777).unwrap_or(0o600)
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        0o600
    }
}
