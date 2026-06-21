/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   manifest.rs                                          :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The per-project encrypted manifest — the ONLY place the real relative paths live.
//! It is sealed like any secret (kind=Manifest), so the server holds only its
//! ciphertext: the blob entries it can see carry opaque vault paths, never the real
//! file paths. Maps each file's `relative_path` → its opaque `vault_path` + Unix mode.

use serde::{Deserialize, Serialize};

/// The project manifest (plaintext shape, only ever sealed before it leaves the host).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub project_id: String,
    pub entries: Vec<Entry>,
}

/// One file in the manifest: the real path, its opaque server (vault) path, mode, and
/// `kind` (the `vault42_core::Kind` repr — 1=EnvFile, 2=Note; defaults to 0 for
/// pre-`kind` manifests, which `pull` still treats as a non-note file).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub relative_path: String,
    pub vault_path: String,
    pub mode: u32,
    #[serde(default)]
    pub kind: u8,
}

impl Manifest {
    /// A fresh empty manifest for `project_id`.
    pub fn new(project_id: &str) -> Self {
        Self {
            version: 1,
            project_id: project_id.to_string(),
            entries: Vec::new(),
        }
    }

    /// Insert or replace the entry for its relative path, keeping entries path-sorted
    /// (deterministic ciphertext).
    pub fn upsert(&mut self, entry: Entry) {
        match self
            .entries
            .iter_mut()
            .find(|e| e.relative_path == entry.relative_path)
        {
            Some(slot) => *slot = entry,
            None => self.entries.push(entry),
        }
        self.entries
            .sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    }

    /// Drop the entry for `relative_path`; returns the removed entry if it was present.
    pub fn remove(&mut self, relative_path: &str) -> Option<Entry> {
        let idx = self
            .entries
            .iter()
            .position(|e| e.relative_path == relative_path)?;
        Some(self.entries.remove(idx))
    }

    /// Serialize to canonical JSON bytes (sealed by the caller).
    pub fn to_bytes(&self) -> anyhow::Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    /// Parse from decrypted JSON bytes.
    pub fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}
