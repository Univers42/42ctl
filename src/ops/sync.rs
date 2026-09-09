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
use crate::core::chunk::{self, ChunkSet};
use crate::core::manifest::{Entry, Manifest};
use crate::core::syncstate::{self, SyncState};
use crate::core::{materialize, merge, project, projpath};
use crate::ops::{largeobj, reconcile};
use crate::ui;
use tonic::{Code, Request};
use vault42_core::Kind;
use vault42_proto::vault::v1::{GetRequest, PushRequest};
use zeroize::Zeroizing;

/// The largest plaintext one envelope can carry, which is the chunk size.
///
/// The old value here was 64 MiB, sixteen times what the wire accepts: the server decodes
/// with tonic's 4 MiB default and never raises it, so anything between the two passed the
/// client guard, sealed, and died at the transport with a message about the protocol rather
/// than about the file. A file above this goes to the object store as chunks instead.
pub(crate) const MAX_BLOB: usize = chunk::CHUNK_BYTES;

impl Session {
    /// Scan the project, seal + push each matched file under an opaque vault path, and
    /// push the encrypted manifest mapping real paths → those vault paths.
    pub async fn cmd_push(&mut self, explicit_id: Option<&str>, prune: bool) -> anyhow::Result<()> {
        let cwd = std::env::current_dir()?;
        let (proj, created) = project::open(&cwd, explicit_id)?;
        if created {
            ui::field("project", &proj.project_id);
        }
        let files = project::scan(&proj)?;
        let mut scanned: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut manifest = self
            .load_manifest(&proj.project_id)
            .await?
            .unwrap_or_else(|| Manifest::new(&proj.project_id));
        let mut state = SyncState::load(&proj.root);
        for file in &files {
            let rel = projpath::canonicalize_for_storage(file, &proj.root)?;
            scanned.insert(rel.as_str().to_string());
            let entry = self
                .push_file(&proj, rel.as_str(), file, &mut state)
                .await?;
            manifest.upsert(entry);
        }
        let pruned = if prune {
            let before = manifest.entries.len();
            manifest
                .entries
                .retain(|e| scanned.contains(&e.relative_path));
            before - manifest.entries.len()
        } else {
            0
        };
        self.push_manifest(&proj.project_id, &manifest).await?;
        state.save(&proj.root)?;
        ui::success(&format!(
            "pushed {} file(s){} + manifest for project {}",
            files.len(),
            if pruned > 0 {
                format!(", pruned {pruned} stale")
            } else {
                String::new()
            },
            proj.project_id
        ));
        Ok(())
    }

    /// Seal and upload one file, record its new merge base, and return its manifest entry.
    ///
    /// A file above the transport ceiling goes to the object store as chunks and the vault
    /// receives only the chunk list, so a volume never crosses a gRPC message limit. The
    /// merge base is the hash of the file's own bytes either way, so a chunked file
    /// reconciles on a later pull exactly as a small one does.
    async fn push_file(
        &mut self,
        proj: &project::Project,
        rel: &str,
        file: &std::path::Path,
        state: &mut SyncState,
    ) -> anyhow::Result<Entry> {
        let vault_path = blob_path(&proj.project_id, &self.principal, rel);
        let mode = file_mode(file);
        let plaintext = Zeroizing::new(std::fs::read(file)?);
        let hash = syncstate::hash(&plaintext);
        let chunked = chunk::needs_chunking(plaintext.len() as u64);
        let body = if chunked {
            self.upload_chunks(&proj.project_id, rel, &plaintext)
                .await?
        } else {
            plaintext
        };
        let rev = self
            .seal_and_push(&vault_path, &proj.project_id, mode, &body)
            .await?;
        state.set(rel, rev, hash);
        Ok(Entry {
            relative_path: rel.to_string(),
            vault_path,
            mode,
            kind: Kind::EnvFile as u8,
            chunked,
        })
    }

    /// Send a file's chunks to the object store and return the list that stands in for it.
    ///
    /// Without a configured store the file is refused, naming what to configure. Refusing is
    /// the right answer rather than a fallback: there is nowhere else for the bytes to go,
    /// and a push that reports success while carrying nothing is discovered only at restore.
    async fn upload_chunks(
        &self,
        project_id: &str,
        rel: &str,
        plaintext: &[u8],
    ) -> anyhow::Result<Zeroizing<Vec<u8>>> {
        let store = self.store.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "{rel} is {} bytes, above the {MAX_BLOB} byte transport ceiling, and this \
                 profile names no object store — set one with `42ctl config endpoint \
                 --blobstore <url> --bucket <name>` and export FT_S3_KEY and FT_S3_SECRET",
                plaintext.len()
            )
        })?;
        store.ensure_bucket().await?;
        let owner = largeobj::Owner {
            identity: &self.identity,
            principal: &self.principal,
        };
        let object = blob_id(project_id, &self.principal, rel);
        let set = largeobj::put_object(&owner, store, &object, plaintext).await?;
        ui::field(
            rel,
            &format!("{} chunk(s) in the object store", set.chunks.len()),
        );
        Ok(Zeroizing::new(set.to_bytes()?))
    }

    /// Seal `plaintext` as a project file and push it at `vault_path`; returns the new rev.
    async fn seal_and_push(
        &mut self,
        vault_path: &str,
        project_id: &str,
        mode: u32,
        plaintext: &[u8],
    ) -> anyhow::Result<u64> {
        let rev = self.current_version(vault_path).await?;
        let env = compose::project_envelope(
            &self.identity,
            &ProjectSeal {
                owner: &self.principal,
                vault_path,
                project_id,
                kind: Kind::EnvFile,
                mode,
                rev: rev + 1,
                plaintext,
            },
        )?;
        self.push_blob(vault_path, env, rev, "/vault.v1.Vault/Push")
            .await?;
        Ok(rev + 1)
    }

    /// Fetch the manifest, then reconcile each file 3-way (local-on-disk vs remote-in-vault
    /// vs the last-synced base): fast-forward when local is unchanged, keep local when only
    /// local moved, and write git-style conflict markers when both diverged. Dry-run unless
    /// `apply`.
    pub async fn cmd_pull(
        &mut self,
        explicit_id: Option<&str>,
        opts: materialize::Opts,
    ) -> anyhow::Result<()> {
        let cwd = std::env::current_dir()?;
        let (proj, _) = project::open(&cwd, explicit_id)?;
        let manifest = self.load_manifest(&proj.project_id).await?.ok_or_else(|| {
            anyhow::anyhow!(
                "no manifest for project {} (push first, or pass --project)",
                proj.project_id
            )
        })?;
        let mut state = SyncState::load(&proj.root);
        if !opts.apply {
            ui::field("pull", "dry-run — re-run with --apply to write");
        }
        let mut conflicts = 0usize;
        for entry in manifest
            .entries
            .iter()
            .filter(|e| e.kind != Kind::Note as u8)
        {
            if self
                .reconcile_entry(&proj.root, entry, &mut state, &opts)
                .await?
            {
                conflicts += 1;
            }
        }
        if opts.apply {
            state.save(&proj.root)?;
        }
        if conflicts > 0 {
            println!(
                "{}",
                ui::warn(&format!(
                    "{conflicts} file(s) in conflict — resolve, then push"
                ))
            );
        }
        Ok(())
    }

    /// Fetch one entry's remote, classify it against local + base, apply the resolution,
    /// and record the new base (on apply). Returns whether the file ended in conflict.
    async fn reconcile_entry(
        &mut self,
        root: &std::path::Path,
        entry: &Entry,
        state: &mut SyncState,
        opts: &materialize::Opts,
    ) -> anyhow::Result<bool> {
        let rel = projpath::validate_stored(&entry.relative_path)?; // sec: validate before any FS op
        let remote = self.fetch_entry(entry).await?;
        let rev = self.current_version(&entry.vault_path).await?;
        let local = std::fs::read(projpath::to_native(root, &rel)).ok();
        let base = state.bases.get(rel.as_str());
        let action = merge::decide(opts.force, local.as_deref(), &remote, rev, base);
        let conflict = reconcile::write_action(root, &rel, &action, entry.mode, opts)?;
        if opts.apply {
            reconcile::update_base(state, rel.as_str(), &action, &remote, rev);
        }
        Ok(conflict)
    }

    /// The entry's real bytes: the vault blob itself, or the object its chunk list names.
    ///
    /// A chunked entry's vault blob is a list, never the file, so returning it unread would
    /// materialise a few hundred bytes of JSON in place of the archive. The list is parsed
    /// before a single chunk is fetched, which is what turns a store that dropped or
    /// reordered a chunk into an error rather than into plausible wrong bytes.
    async fn fetch_entry(&mut self, entry: &Entry) -> anyhow::Result<Zeroizing<Vec<u8>>> {
        let stored = self.fetch_blob(&entry.vault_path).await?;
        if !entry.chunked {
            return Ok(stored);
        }
        let set = ChunkSet::from_bytes(&stored)?;
        let store = self.store.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "{} is stored as {} chunk(s) and this profile names no object store",
                entry.relative_path,
                set.chunks.len()
            )
        })?;
        largeobj::get_object(&self.identity, &self.principal, store, &set).await
    }

    /// Push one opaque envelope at `vault_path` with optimistic concurrency.
    pub(crate) async fn push_blob(
        &mut self,
        vault_path: &str,
        envelope: Vec<u8>,
        rev: u64,
        method: &str,
    ) -> anyhow::Result<()> {
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
    pub(crate) async fn push_manifest(
        &mut self,
        project_id: &str,
        manifest: &Manifest,
    ) -> anyhow::Result<()> {
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
        self.push_blob(&vault_path, env, rev, "/vault.v1.Vault/Push")
            .await
    }

    /// Fetch + decrypt the manifest (None when the project has nothing pushed yet).
    pub(crate) async fn load_manifest(
        &mut self,
        project_id: &str,
    ) -> anyhow::Result<Option<Manifest>> {
        let vault_path = manifest_path(project_id);
        match self.get_blob(&vault_path).await {
            Ok(bytes) => Ok(Some(Manifest::parse(&bytes)?)),
            Err(status) if status.code() == Code::NotFound => Ok(None),
            Err(status) => Err(status.into()),
        }
    }

    /// Fetch + decrypt the blob at `vault_path` (anyhow error on any failure).
    pub(crate) async fn fetch_blob(
        &mut self,
        vault_path: &str,
    ) -> anyhow::Result<Zeroizing<Vec<u8>>> {
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
    let id = blob_id(project_id, principal, rel);
    format!("{}/b/{project_id}/{id}", projpath::RESERVED_PREFIX)
}

/// The opaque id a project file is known by, on the server and in the object store alike.
fn blob_id(project_id: &str, principal: &str, rel: &str) -> String {
    derive::secret_id(principal, &format!("{project_id}/{rel}"))
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
        std::fs::metadata(file)
            .map(|m| m.mode() & 0o777)
            .unwrap_or(0o600)
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        0o600
    }
}
