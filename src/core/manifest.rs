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

/// The manifest shape this client understands and writes.
///
/// Version 1 held only the path map. Version 2 adds `chunked` and `rev`, which change what
/// the vault blob at a path MEANS: for a chunked entry it is a chunk list and never the
/// file. A reader that ignores those fields writes the list to disk as the file and reports
/// success, so a newer manifest has to be refused rather than read past.
pub const VERSION: u32 = 2;

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
///
/// `chunked` says what the vault blob at `vault_path` actually holds. False, the default,
/// means the file's own bytes, which is every manifest written before large objects
/// existed. True means a chunk list, and the bytes are in the object store. Defaulting to
/// false is what lets a new client read an old manifest unchanged.
///
/// `rev` is the blob revision this manifest version was written against, and it is what
/// makes an older version restorable: without it, reading an old manifest still fetches
/// today's bytes for every path it names, which reproduces a tree that never existed.
/// Zero means "whatever is latest", which is every manifest written before this field.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub relative_path: String,
    pub vault_path: String,
    pub mode: u32,
    #[serde(default)]
    pub kind: u8,
    #[serde(default)]
    pub chunked: bool,
    #[serde(default)]
    pub rev: u64,
}

impl Manifest {
    /// A fresh empty manifest for `project_id`.
    pub fn new(project_id: &str) -> Self {
        Self {
            version: VERSION,
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

    /// Parse from decrypted JSON bytes, refusing a manifest this client cannot fully read.
    ///
    /// Serde drops an unknown field silently, so a newer manifest parses cleanly and the
    /// reader carries on with a partial understanding of what each entry means. That is the
    /// dangerous direction here rather than a decode error: a chunked entry read by a client
    /// that does not know the flag writes a few hundred bytes of chunk list to disk in place
    /// of the file and reports it restored.
    ///
    /// Older manifests are read as they always were — the point is refusing what is AHEAD.
    pub fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
        let manifest: Self = serde_json::from_slice(bytes)?;
        if manifest.version > VERSION {
            anyhow::bail!(
                "this project's manifest is version {}, and this 42ctl understands up to {VERSION} \
                 — update 42ctl rather than pulling, because reading it with what is known here \
                 would restore some files with the wrong contents and report success",
                manifest.version
            );
        }
        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A manifest from a client that knows something this one does not must be REFUSED, not
    /// read past. The field it does not understand is dropped silently by serde, so the
    /// reader carries on and materialises whatever it made of the rest — which for a chunked
    /// entry means writing the chunk list to disk as the file and reporting success.
    ///
    /// This is the last version skew that can be closed. `version` has been written since
    /// the first manifest and read by nothing, so no already-released client can be made to
    /// fail safely on anything written from now on. A field with no reader is a gate that
    /// cannot fail.
    #[test]
    fn a_manifest_from_a_newer_client_is_refused() {
        let ahead = br#"{"version":99,"project_id":"p","entries":[]}"#;
        let said = Manifest::parse(ahead)
            .expect_err("a newer manifest must not be read past")
            .to_string();
        assert!(
            said.contains("99"),
            "the refusal must name the version: {said}"
        );
        assert!(
            said.to_lowercase().contains("update") || said.to_lowercase().contains("upgrade"),
            "the refusal must say what to do: {said}"
        );
    }

    /// The version this client writes must be one it also accepts, or it refuses its own
    /// manifests the moment the constant moves.
    #[test]
    fn this_client_accepts_what_it_writes() {
        let mine = Manifest::new("p").to_bytes().expect("encode");
        Manifest::parse(&mine).expect("a client must read back its own manifest");
    }

    /// A manifest written before large objects existed must still load, and its entries
    /// must read as ordinary files rather than as chunk lists. Getting this backwards would
    /// send `pull` to an object store for a file whose bytes are in the vault.
    #[test]
    fn a_manifest_without_the_chunked_flag_still_loads_as_whole_files() {
        let legacy = br#"{"version":1,"project_id":"p","entries":[
            {"relative_path":".env","vault_path":"__42ctl/b/p/id","mode":384}]}"#;
        let manifest = Manifest::parse(legacy).expect("a pre-chunking manifest must load");
        assert_eq!(manifest.entries.len(), 1);
        assert!(!manifest.entries[0].chunked);
        assert_eq!(manifest.entries[0].kind, 0);
        assert_eq!(
            manifest.entries[0].rev, 0,
            "a manifest with no recorded revision must read as latest, not as revision zero \
             of nothing"
        );
    }

    /// The flag survives a round trip, so a chunked entry is still chunked after a pull.
    #[test]
    fn the_chunked_flag_round_trips() {
        let mut manifest = Manifest::new("p");
        manifest.upsert(Entry {
            relative_path: "big.bin".into(),
            vault_path: "__42ctl/b/p/id".into(),
            mode: 0o600,
            kind: 1,
            chunked: true,
            rev: 7,
        });
        let back = Manifest::parse(&manifest.to_bytes().expect("encode")).expect("decode");
        assert!(back.entries[0].chunked);
        assert_eq!(back.entries[0].rev, 7);
    }
}
