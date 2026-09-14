/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope_private.rs                                     :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Private files inside a shared environment — `env push --private` and the `*.local` default.
//!
//! A private file lives in the SAME environment as the shared tree, under the same scope
//! owner, but is sealed to the pusher's own identity rather than to the environment's key,
//! and is named only in a second manifest that is sealed the same way. A teammate who pulls
//! the environment gets neither its bytes nor its path: to them the environment simply does
//! not contain it. Nothing on the server distinguishes the two kinds of blob, so the privacy
//! is cryptographic rather than a permission somebody could misconfigure.
//!
//! Reading a private file requires that its AUTHOR be the reader: a writer of the environment
//! can store bytes sealed to any published public key, so "sealed to me" alone does not mean
//! "written by me" (`decrypt::open_private_envelope`). Private files are always sealed whole;
//! one above the chunking ceiling is refused before anything is uploaded.

use crate::adapters::api::Session;
use crate::adapters::derive;
use crate::cmd::scope::Ctx;
use crate::cmd::scope_store::{self, Dest, Heads, Key, Slot};
use crate::cmd::scope_tree;
use crate::core::manifest::{Entry, Manifest};
use crate::core::{chunk, project, projpath};
use crate::ui;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use vault42_core::Kind;
use zeroize::Zeroizing;

/// Every file matching this is private whether or not `--private` was given. A `.env.local`
/// is by convention one person's overrides, and a push that shared it with the team would
/// be the surprise, not the default.
pub const DEFAULT_PRIVATE: &str = "*.local";

/// A scanned file and the relative path it is stored under.
pub type Scanned = (PathBuf, String);

/// Whether a stored path is sealed to the pusher alone: the default pattern or any extra one.
pub fn is_private(rel: &str, extra: &[String]) -> bool {
    project::glob_match(rel, DEFAULT_PRIVATE)
        || extra
            .iter()
            .any(|pattern| project::glob_match(rel, pattern))
}

/// Split the scan into `(shared, private)` by stored path.
pub fn partition(
    files: &[PathBuf],
    root: &Path,
    extra: &[String],
) -> anyhow::Result<(Vec<Scanned>, Vec<Scanned>)> {
    let (mut shared, mut private) = (Vec::new(), Vec::new());
    for file in files {
        let rel = projpath::canonicalize_for_storage(file, root)?;
        let scanned = (file.clone(), rel.as_str().to_string());
        if is_private(rel.as_str(), extra) {
            private.push(scanned);
        } else {
            shared.push(scanned);
        }
    }
    Ok((shared, private))
}

/// Refuse any private file above the whole-object ceiling, BEFORE anything is uploaded.
pub fn refuse_oversized(private: &[Scanned]) -> anyhow::Result<()> {
    for (file, rel) in private {
        let len = std::fs::metadata(file)?.len();
        if chunk::needs_chunking(len) {
            anyhow::bail!(
                "private file '{rel}' is {len} bytes, above the {}-byte ceiling — private \
                 files are sealed whole; split it, or push it shared",
                chunk::CHUNK_BYTES
            );
        }
    }
    Ok(())
}

/// `KEY=VALUE` flags into labels. A repeated key keeps its last value, as Docker does.
pub fn parse_labels(flags: &[String]) -> anyhow::Result<BTreeMap<String, String>> {
    let mut labels = BTreeMap::new();
    for flag in flags {
        match flag.split_once('=') {
            Some((key, value)) if !key.is_empty() => {
                labels.insert(key.to_string(), value.to_string());
            }
            _ => anyhow::bail!("label '{flag}' is not KEY=VALUE"),
        }
    }
    Ok(labels)
}

/// Where every private manifest and file is stored, whoever it belongs to.
pub(super) const PRIVATE_PREFIX: &str = "__42ctl/p/";

/// The private manifest's path for `principal` — one per member, under the shared owner.
fn manifest_path(principal: &str) -> String {
    format!("{PRIVATE_PREFIX}{principal}/tree")
}

/// Where a private file is stored. The principal is part of the derivation, so a private
/// `.env.local` can never land on the stored path of a teammate's shared `.env.local`.
fn blob_path(owner: &str, principal: &str, rel: &str) -> String {
    let id = derive::secret_id(owner, &format!("p/{principal}/{rel}"));
    format!("{PRIVATE_PREFIX}{principal}/f/{id}")
}

/// Seal every private file to the pusher and commit their manifest last. Returns the count.
///
/// `at` is the environment's owner, the project id, and the heads the push read before
/// writing anything, which every write here is conditional on as the shared tree's are.
pub async fn push(
    session: &mut Session,
    ctx: &Ctx,
    at: (&str, &str, &Heads),
    what: (&[Scanned], &BTreeMap<String, String>),
) -> anyhow::Result<usize> {
    let (owner, project_id, heads) = at;
    let (files, labels) = what;
    let principal = session.principal.clone();
    let dest = Dest {
        owner,
        epoch: ctx.epoch(),
        to: session.identity.encryption_public(),
    };
    let mut manifest = Manifest::new(project_id);
    for (file, rel) in files {
        let plaintext = Zeroizing::new(std::fs::read(file)?);
        let vault_path = blob_path(owner, &principal, rel);
        let after = heads.of(&vault_path);
        let rev = scope_store::put_one(session, dest, (&vault_path, after), &plaintext).await?;
        let entry = Entry::file(rel, vault_path, rev, plaintext.len() as u64);
        manifest.upsert(Entry {
            mode: scope_tree::file_mode(file),
            labels: labels.clone(),
            ..entry
        });
    }
    commit(session, dest, &manifest, heads).await?;
    Ok(files.len())
}

/// Write the private manifest — unless it is empty and there never was one, in which case
/// there is nothing to replace and nothing worth recording.
///
/// "Never" reaches back past a rotation: the one a pull reads may sit in an earlier epoch, and
/// skipping the empty write then would bring files the owner has since deleted back.
async fn commit(
    session: &mut Session,
    dest: Dest<'_>,
    manifest: &Manifest,
    heads: &Heads,
) -> anyhow::Result<()> {
    let path = manifest_path(&session.principal);
    let after = heads.of(&path);
    if manifest.entries.is_empty() && after == 0 && !kept_earlier(session, dest, &path).await? {
        return Ok(());
    }
    scope_store::put_one(session, dest, (&path, after), &manifest.to_bytes()?).await?;
    Ok(())
}

/// Whether an epoch before `dest.epoch` holds anything at `path`.
async fn kept_earlier(session: &mut Session, dest: Dest<'_>, path: &str) -> anyhow::Result<bool> {
    for epoch in (1..dest.epoch).rev() {
        if session
            .get_env_secret(dest.owner, epoch, path, 0)
            .await?
            .is_some()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The caller's private manifest, and the epoch it was found in.
pub struct Mine {
    pub manifest: Manifest,
    pub epoch: u32,
}

/// The shared manifest and, if the caller ever pushed one here, their private manifest.
pub async fn both_manifests(
    session: &mut Session,
    ctx: &Ctx,
    key: (&str, &Zeroizing<[u8; 32]>),
) -> anyhow::Result<(Manifest, Option<Mine>)> {
    let (owner, secret) = key;
    let shared = Slot {
        owner,
        epoch: ctx.epoch(),
        path: scope_tree::TREE_MANIFEST,
        rev: 0,
    };
    let raw = scope_store::fetch_one(session, shared, &Key::Scope(secret))
        .await?
        .ok_or_else(|| anyhow::anyhow!("environment '{}' holds no tree", ctx.env_name))?;
    let mine = newest_mine(session, owner, ctx.epoch()).await?;
    Ok((Manifest::parse(&raw)?, mine))
}

/// The caller's private manifest from the newest epoch holding one, back from `current`.
///
/// A rotation cannot carry private files forward. They are sealed to their owner, and the
/// server takes a write only from the envelope's author, so the administrator can neither open
/// nor re-author them; they stay in the epoch they were written in until their owner pushes
/// again. Their bytes are sealed to the owner's identity rather than to any epoch's key, so an
/// earlier epoch opens exactly as the current one does, and the author check still applies.
async fn newest_mine(
    session: &mut Session,
    owner: &str,
    current: u32,
) -> anyhow::Result<Option<Mine>> {
    let path = manifest_path(&session.principal);
    for epoch in (1..=current).rev() {
        let slot = Slot {
            owner,
            epoch,
            path: &path,
            rev: 0,
        };
        if let Some(raw) = scope_store::fetch_one(session, slot, &Key::Me).await? {
            let manifest = Manifest::parse(&raw)?;
            return Ok(Some(Mine { manifest, epoch }));
        }
    }
    Ok(None)
}

/// One file to restore and which key opens it.
pub struct Pick<'a> {
    pub entry: &'a Entry,
    pub private: bool,
}

/// What a pull restores, and the shared paths a private copy hid.
pub struct Merged<'a> {
    pub picks: Vec<Pick<'a>>,
    pub shadowed: Vec<String>,
}

/// Every shared file plus the caller's private ones, path-sorted. On a path both name, the
/// private copy wins and the shared one is reported rather than silently dropped. Notes in
/// the shared manifest are not files and are left out, as the pull always has.
pub fn merge<'a>(shared: &'a Manifest, mine: Option<&'a Manifest>) -> Merged<'a> {
    let private: Vec<&Entry> = mine.map(|m| m.entries.iter().collect()).unwrap_or_default();
    let (mut picks, mut shadowed) = (Vec::new(), Vec::new());
    for entry in &shared.entries {
        if entry.kind == Kind::Note as u8 {
            continue;
        }
        if private
            .iter()
            .any(|p| p.relative_path == entry.relative_path)
        {
            shadowed.push(entry.relative_path.clone());
        } else {
            picks.push(Pick {
                entry,
                private: false,
            });
        }
    }
    picks.extend(private.into_iter().map(|entry| Pick {
        entry,
        private: true,
    }));
    picks.sort_by(|a, b| a.entry.relative_path.cmp(&b.entry.relative_path));
    Merged { picks, shadowed }
}

/// Name every shared file a private one hid, on stderr so a `--format json` stays parseable.
pub fn report_shadows(shadowed: &[String]) {
    for path in shadowed {
        eprintln!(
            "{}",
            ui::warn(&format!("{path}: your private copy shadows the shared one"))
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(rel: &str) -> Entry {
        Entry::file(rel, format!("__42ctl/f/{rel}"), 1, 3)
    }

    fn manifest(paths: &[&str]) -> Manifest {
        let mut m = Manifest::new("p");
        for path in paths {
            m.upsert(entry(path));
        }
        m
    }

    /// `.env.local` is private with no flag at all, wherever it sits; `.env` beside it is not.
    #[test]
    fn dot_local_is_private_by_default_and_dot_env_is_not() {
        assert!(is_private("srcs/.env.local", &[]));
        assert!(is_private(".env.local", &[]));
        assert!(!is_private("srcs/.env", &[]));
        assert!(!is_private("secrets/db_password.txt", &[]));
    }

    /// `--private` adds to the default rather than replacing it.
    #[test]
    fn an_extra_pattern_adds_to_the_default() {
        let extra = vec!["secrets/me.*".to_string()];
        assert!(is_private("secrets/me.key", &extra));
        assert!(
            is_private("srcs/.env.local", &extra),
            "the default must survive"
        );
        assert!(!is_private("secrets/ca.key", &extra));
    }

    /// Labels parse as KEY=VALUE, keep the first `=` as the split, and refuse anything else.
    #[test]
    fn labels_parse_or_are_refused_by_name() {
        let ok = parse_labels(&["app=wordpress".into(), "note=a=b".into()]).expect("valid");
        assert_eq!(ok.get("app").map(String::as_str), Some("wordpress"));
        assert_eq!(ok.get("note").map(String::as_str), Some("a=b"));
        let last = parse_labels(&["app=a".into(), "app=b".into()]).expect("repeat");
        assert_eq!(last.get("app").map(String::as_str), Some("b"));
        for bad in ["app", "=x", ""] {
            let err = parse_labels(&[bad.to_string()]).expect_err(bad);
            assert!(err.to_string().contains("KEY=VALUE"), "{err}");
        }
    }

    /// A private file at a shared path wins, and the shared one is named as shadowed —
    /// a restore that quietly picked either would be wrong for somebody.
    #[test]
    fn a_private_copy_shadows_the_shared_one_and_says_so() {
        let shared = manifest(&["srcs/.env", "srcs/.env.local"]);
        let mine = manifest(&["srcs/.env.local"]);
        let merged = merge(&shared, Some(&mine));
        assert_eq!(merged.shadowed, vec!["srcs/.env.local".to_string()]);
        assert_eq!(merged.picks.len(), 2);
        let local = merged
            .picks
            .iter()
            .find(|p| p.entry.relative_path == "srcs/.env.local")
            .expect("present once");
        assert!(local.private, "the private copy is the one restored");
        assert!(!merged.picks[0].private, "srcs/.env stays shared");
    }

    /// Without a private manifest the merge is the shared tree, notes excluded, as before.
    #[test]
    fn no_private_manifest_means_the_shared_tree_alone() {
        let mut shared = manifest(&["b", "a"]);
        shared.upsert(Entry {
            kind: Kind::Note as u8,
            ..entry("notes/x.md")
        });
        let merged = merge(&shared, None);
        let paths: Vec<&str> = merged
            .picks
            .iter()
            .map(|p| p.entry.relative_path.as_str())
            .collect();
        assert_eq!(paths, vec!["a", "b"], "sorted, note left out");
        assert!(merged.shadowed.is_empty());
        assert!(merged.picks.iter().all(|p| !p.private));
    }

    /// A private blob never shares a stored path with the shared copy of the same file, nor
    /// with another member's private copy — the principal is in the derivation.
    #[test]
    fn private_paths_collide_with_nobody() {
        let owner = "0123456789abcdef0123456789abcdef";
        let mine = blob_path(owner, "alice", "srcs/.env.local");
        assert_ne!(mine, scope_tree::tree_path(owner, "srcs/.env.local"));
        assert_ne!(mine, blob_path(owner, "bob", "srcs/.env.local"));
        assert!(mine.starts_with("__42ctl/p/alice/f/"));
        assert!(
            !mine.contains(".env"),
            "the real path never reaches the server"
        );
    }

    /// A private file above the ceiling is refused, by name, with the ceiling stated.
    #[test]
    fn an_oversized_private_file_is_refused_by_name() {
        let path = std::env::temp_dir().join(format!("42ctl-private-{}.bin", std::process::id()));
        std::fs::File::create(&path)
            .and_then(|f| f.set_len(chunk::CHUNK_BYTES as u64 + 1))
            .expect("a sparse file");
        let err =
            refuse_oversized(&[(path.clone(), "big.local".into())]).expect_err("above the ceiling");
        let _ = std::fs::remove_file(&path);
        let said = err.to_string();
        assert!(
            said.contains("big.local") && said.contains("sealed whole"),
            "{said}"
        );
    }
}
