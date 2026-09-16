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

use crate::core::projpath;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// The manifest shape this client understands and writes.
///
/// Version 1 held only the path map. Version 2 adds `chunked` and `rev`, which change what
/// the vault blob at a path MEANS: for a chunked entry it is a chunk list and never the
/// file. A reader that ignores those fields writes the list to disk as the file and reports
/// success, so a newer manifest has to be refused rather than read past.
///
/// `labels` and `size` were added WITHOUT a bump, and deliberately: they change nothing about
/// what the blob at a path holds. An older reader that drops them still fetches the right
/// bytes and writes them to the right place — it merely cannot show a label or a size. That is
/// the test the comment above sets, and they pass it in the direction a bump exists to catch.
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
///
/// `size` is the plaintext length recorded at push, so a listing can say how big a file is
/// without fetching and decrypting it. `labels` are `key=value` metadata the pusher attached
/// (`--label app=wordpress`), the handle a listing filters on. Both default to empty and are
/// omitted from the wire when empty, so a manifest without them serializes byte-for-byte as
/// it did before.
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
    #[serde(default, skip_serializing_if = "is_zero")]
    pub size: u64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
}

/// Whether a REGULAR file exists at `path`, without following a final symlink.
///
/// The scan skips symlinks, so a symlink left where a file used to be is not that file
/// coming back; neither is a directory at the same path.
fn is_regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file())
}

/// `skip_serializing_if` needs a function, and a zero size is "not recorded".
impl Entry {
    /// A whole file at the default owner-only mode, carrying no labels yet.
    pub fn file(rel: &str, vault_path: String, rev: u64, size: u64) -> Self {
        Entry {
            relative_path: rel.to_string(),
            vault_path,
            mode: 0o600,
            kind: vault42_core::Kind::EnvFile as u8,
            chunked: false,
            rev,
            size,
            labels: BTreeMap::new(),
        }
    }
}

fn is_zero(size: &u64) -> bool {
    *size == 0
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

    /// Drop entries the scan did not produce AND whose file is genuinely gone.
    ///
    /// `scanned` is what the scan FOUND, which is a lower bound on the tree rather than a
    /// census of it, and never evidence that a file was deleted. The scan declines whole
    /// directories — a vendored tree that is not its own repository — and the probe that
    /// reports a declined directory gives up past its entry limit, so the largest omission
    /// is the silent one. Pruning on "not scanned" therefore deleted live secrets from the
    /// vault and exited 0: measured at 33 of 39 files scanned, six entries destroyed.
    ///
    /// Absence from the FILESYSTEM is the evidence. The failure direction inverts with it:
    /// the worst case becomes a stale entry that was kept, not a secret that was destroyed.
    ///
    /// Returns how many were pruned, and the paths kept because their file is still on disk
    /// (or their stored path could not be validated) — an incomplete tree the caller must
    /// name, because this is the run that would otherwise have deleted them.
    pub fn prune_to_scanned(
        &mut self,
        scanned: &HashSet<String>,
        root: &Path,
    ) -> (usize, Vec<String>) {
        let before = self.entries.len();
        let mut kept = Vec::new();
        self.entries.retain(|entry| {
            if scanned.contains(&entry.relative_path) {
                return true;
            }
            // A note has no file and never had one, and the scan cannot produce its path,
            // so every prune would delete every note in the project.
            if entry.kind == vault42_core::Kind::Note as u8 {
                return true;
            }
            // sec: validate before any FS op — an unvalidatable path is never joined and
            // never stat'd, and is kept rather than deleted on a check that did not run.
            match projpath::validate_stored(&entry.relative_path) {
                Ok(rel) if !is_regular_file(&projpath::to_native(root, &rel)) => false,
                _ => {
                    kept.push(entry.relative_path.clone());
                    true
                }
            }
        });
        (before - self.entries.len(), kept)
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
    use std::collections::HashSet;

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
            size: 0,
            labels: BTreeMap::new(),
        });
        let back = Manifest::parse(&manifest.to_bytes().expect("encode")).expect("decode");
        assert!(back.entries[0].chunked);
        assert_eq!(back.entries[0].rev, 7);
    }

    /// Labels and size survive a round trip and read back typed.
    #[test]
    fn labels_and_size_round_trip() {
        let mut manifest = Manifest::new("p");
        manifest.upsert(Entry {
            relative_path: ".env.local".into(),
            vault_path: "__42ctl/p/me/f/id".into(),
            mode: 0o600,
            kind: 1,
            chunked: false,
            rev: 1,
            size: 1314,
            labels: BTreeMap::from([("app".into(), "wordpress".into())]),
        });
        let back = Manifest::parse(&manifest.to_bytes().expect("encode")).expect("decode");
        assert_eq!(back.entries[0].size, 1314);
        assert_eq!(
            back.entries[0].labels.get("app").map(String::as_str),
            Some("wordpress")
        );
    }

    /// A six-field manifest written before these fields existed still parses, with both
    /// defaulted — the reason this needed no version bump.
    #[test]
    fn a_pre_label_manifest_still_parses_with_defaults() {
        let old = br#"{"version":2,"project_id":"p","entries":[{"relative_path":"srcs/.env",
            "vault_path":"__42ctl/f/x","mode":420,"kind":1,"chunked":false,"rev":3}]}"#;
        let back = Manifest::parse(old).expect("an old manifest must still parse");
        assert_eq!(back.entries[0].size, 0);
        assert!(back.entries[0].labels.is_empty());
    }

    /// An entry with nothing recorded serializes WITHOUT the new keys, so a manifest that never
    /// used them is byte-for-byte what it was before — same ciphertext, same old-client view.
    #[test]
    fn empty_labels_and_zero_size_stay_off_the_wire() {
        let mut manifest = Manifest::new("p");
        manifest.upsert(Entry {
            relative_path: "a".into(),
            vault_path: "b".into(),
            mode: 0o600,
            kind: 1,
            chunked: false,
            rev: 1,
            size: 0,
            labels: BTreeMap::new(),
        });
        let json = String::from_utf8(manifest.to_bytes().expect("encode")).expect("utf8");
        assert!(!json.contains("\"size\""), "{json}");
        assert!(!json.contains("\"labels\""), "{json}");
    }

    /// A scratch root for the presence checks, named by pid so parallel tests do not collide.
    fn scratch(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("v42-prune-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        root
    }

    fn entry_at(rel: &str, kind: u8) -> Entry {
        let mut e = Entry::file(rel, "__42ctl/b/p/id".into(), 1, 0);
        e.kind = kind;
        e
    }

    /// A note has no file on disk and the scan can never produce its path, so a prune that
    /// asks only "was this scanned?" deletes every note in the project.
    #[test]
    fn a_note_is_never_pruned() {
        let root = scratch("note");
        let mut manifest = Manifest::new("p");
        manifest.upsert(entry_at("notes/todo", vault42_core::Kind::Note as u8));
        let (pruned, kept) = manifest.prune_to_scanned(&HashSet::new(), &root);
        assert_eq!(pruned, 0, "a note was pruned");
        assert!(
            kept.is_empty(),
            "a note is not an incomplete-tree warning: {kept:?}"
        );
        assert_eq!(manifest.entries.len(), 1);
    }

    /// The case prune exists for: the file really is gone, so the entry goes too.
    #[test]
    fn an_entry_whose_file_is_gone_is_pruned() {
        let root = scratch("gone");
        let mut manifest = Manifest::new("p");
        manifest.upsert(entry_at(".env.removed", 1));
        let (pruned, kept) = manifest.prune_to_scanned(&HashSet::new(), &root);
        assert_eq!(pruned, 1);
        assert!(kept.is_empty());
        assert!(manifest.entries.is_empty());
    }

    /// The bug this fixes. The scan declines whole directories, so "not scanned" is a LOWER
    /// BOUND on the tree — never evidence the file was deleted. Measured: pushing from a tree
    /// whose vendor/ dirs were not repositories scanned 33 of 39 files and, with --prune,
    /// deleted the other six from the vault while reporting success.
    #[test]
    fn an_unscanned_entry_still_on_disk_is_kept_and_named() {
        let root = scratch("present");
        std::fs::create_dir_all(root.join("vendor/qa")).expect("mkdir");
        std::fs::write(root.join("vendor/qa/.env"), b"KEY=v").expect("write");
        let mut manifest = Manifest::new("p");
        manifest.upsert(entry_at("vendor/qa/.env", 1));
        let (pruned, kept) = manifest.prune_to_scanned(&HashSet::new(), &root);
        assert_eq!(pruned, 0, "a file that still exists was pruned");
        assert_eq!(kept, vec!["vendor/qa/.env".to_string()]);
    }

    /// The scan skips symlinks, so a symlink left at the path is not the file coming back.
    #[cfg(unix)]
    #[test]
    fn a_symlink_is_not_presence() {
        let root = scratch("symlink");
        std::fs::write(root.join("real"), b"x").expect("write");
        std::os::unix::fs::symlink(root.join("real"), root.join(".env.link")).expect("symlink");
        let mut manifest = Manifest::new("p");
        manifest.upsert(entry_at(".env.link", 1));
        let (pruned, _) = manifest.prune_to_scanned(&HashSet::new(), &root);
        assert_eq!(pruned, 1, "a symlink counted as the file being present");
    }

    /// A directory sitting where a file was is not that file.
    #[test]
    fn a_directory_is_not_presence() {
        let root = scratch("dir");
        std::fs::create_dir_all(root.join(".env.d")).expect("mkdir");
        let mut manifest = Manifest::new("p");
        manifest.upsert(entry_at(".env.d", 1));
        let (pruned, _) = manifest.prune_to_scanned(&HashSet::new(), &root);
        assert_eq!(pruned, 1, "a directory counted as the file being present");
    }

    /// An entry whose stored path escapes the root is never joined and never stat'd — the
    /// module's invariant is validate-before-any-FS-op. It is kept, so a manifest that cannot
    /// be checked is never deleted on the strength of a check that did not happen.
    #[test]
    fn a_path_that_escapes_the_root_is_kept_and_never_stated() {
        let root = scratch("escape");
        let mut manifest = Manifest::new("p");
        for bad in ["../outside/.env", "/etc/passwd"] {
            manifest.entries.push(entry_at(bad, 1));
        }
        let (pruned, kept) = manifest.prune_to_scanned(&HashSet::new(), &root);
        assert_eq!(pruned, 0, "an unvalidatable path was pruned");
        assert_eq!(kept.len(), 2, "both should be reported: {kept:?}");
    }
}
