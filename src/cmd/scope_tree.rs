/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope_tree.rs                                        :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `vault push-env` / `pull-env` — a whole project tree shared with an environment.
//!
//! `set-env` carries one value under one name. This carries a FILE TREE: every scanned file
//! sealed to the environment's public scope key, plus a manifest holding the real relative
//! paths and modes. A member the authority has authorised holds a wrap of that key and can
//! restore the tree exactly as it stood; anybody else holds ciphertext.
//!
//! The access control is the same as `get-env`'s and is cryptographic rather than advisory:
//! being refused means being unable to decrypt, not being told no. That is what makes a
//! grant through a team meaningful — the authority decides who gets a wrap, and everything
//! else follows from holding one.
//!
//! The server sees opaque env-secret paths and never the real ones, exactly as the personal
//! `push` does: real paths live only inside the sealed manifest.

use crate::adapters::api::Session;
use crate::adapters::compose::{self, ScopeSeal};
use crate::adapters::scope as crypto;
use crate::adapters::scope_env_grpc::EnvSecretPut;
use crate::adapters::{decrypt, derive};
use crate::cmd::scope::Ctx;
use crate::cmd::scope_chunks;
use crate::cmd::scope_private::{self, Pick, Scanned};
use crate::cmd::scope_recover::recover_scope_secret;
use crate::core::chunk::{self, ChunkSet, Naming};
use crate::core::manifest::{Entry, Manifest};
use crate::core::materialize::Opts;
use crate::core::{materialize, project, projpath};
use crate::ui;
use std::collections::BTreeMap;
use std::path::Path;
use vault42_core::{ReadScope, RecipientPublicKey};
use zeroize::Zeroizing;

/// The reserved env-secret path holding the tree manifest.
pub(super) const TREE_MANIFEST: &str = "__42ctl/tree";

/// Which key opens an environment secret: the environment's recovered secret for a shared
/// file, or the caller's own identity for one sealed to them alone.
pub(super) enum Key<'a> {
    Scope(&'a Zeroizing<[u8; 32]>),
    Me,
}

/// The operator's push flags, parsed: extra private patterns and the labels for every file.
pub struct PushRules {
    pub private: Vec<String>,
    pub labels: BTreeMap<String, String>,
}

/// A push in progress: what every file of it shares — the environment's owner id, the
/// project it belongs to, the operator's flags, and the chunk naming, derived on first need.
struct Push<'a> {
    owner: String,
    project_id: String,
    scope_id: [u8; 16],
    naming: Option<Naming>,
    rules: &'a PushRules,
}

/// Scan the project at the working directory and seal every file to the environment — or,
/// for the private ones, to the pusher alone (`scope_private`).
///
/// Each manifest goes last, so an interrupted push leaves files nobody references rather than
/// a manifest naming files that are not there — the same commit-point rule the chunked-object
/// path uses, for the same reason. An oversized private file is refused before anything is
/// uploaded, so a push is never half done.
pub async fn push_env(session: &mut Session, ctx: &Ctx, rules: &PushRules) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let (proj, _) = project::open(&cwd, None)?;
    let files = project::scan(&proj)?.files;
    let (shared, private) = scope_private::partition(&files, &proj.root, &rules.private)?;
    scope_private::refuse_oversized(&private)?;
    let scope_id = crypto::scope_id(&ctx.project, &ctx.env_name)?;
    let mut push = Push {
        owner: hex::encode(scope_id),
        project_id: proj.project_id.clone(),
        scope_id,
        naming: None,
        rules,
    };
    seal_tree(session, ctx, &mut push, &shared).await?;
    let ids = (push.owner.as_str(), proj.project_id.as_str());
    let mine = scope_private::push(session, ctx, ids, (&private, &rules.labels)).await?;
    ui::success(&format!(
        "pushed {} shared and {mine} private file(s) to environment '{}'",
        shared.len(),
        ctx.env_name
    ));
    Ok(())
}

/// Seal every shared file and commit their manifest last, so an interrupted push leaves
/// files nobody references rather than a manifest naming files that are not there.
async fn seal_tree(
    session: &mut Session,
    ctx: &Ctx,
    push: &mut Push<'_>,
    shared: &[Scanned],
) -> anyhow::Result<()> {
    let mut manifest = Manifest::new(&push.project_id);
    for (file, rel) in shared {
        let entry = seal_scanned(session, ctx, push, (rel, file)).await?;
        manifest.upsert(labelled(entry, file, &push.rules.labels));
    }
    let to = scope_public(ctx)?;
    let at = (push.owner.as_str(), TREE_MANIFEST, to);
    put_one(session, ctx, at, &manifest.to_bytes()?).await?;
    Ok(())
}

/// Seal one shared file — whole, or chunked to the object store above the ceiling.
async fn seal_scanned(
    session: &mut Session,
    ctx: &Ctx,
    push: &mut Push<'_>,
    at: (&str, &Path),
) -> anyhow::Result<Entry> {
    let (rel, file) = at;
    let plaintext = Zeroizing::new(std::fs::read(file)?);
    if !chunk::needs_chunking(plaintext.len() as u64) {
        return seal_one(session, ctx, (&push.owner, rel), &plaintext).await;
    }
    if push.naming.is_none() {
        push.naming = Some(scope_chunks::environment_naming(session, ctx, push.scope_id).await?);
    }
    let named = push.naming.as_ref().expect("just derived");
    scope_chunks::seal_large(session, ctx, (&push.owner, rel, named), &plaintext).await
}

/// Finish an entry with the file's on-disk mode and this push's labels.
fn labelled(entry: Entry, file: &Path, labels: &BTreeMap<String, String>) -> Entry {
    Entry {
        mode: file_mode(file),
        labels: labels.clone(),
        ..entry
    }
}

/// Recover the environment's key, fetch the manifests, and restore every file they name.
///
/// The shared manifest opens with the scope secret; the caller's private one, if any, with
/// their own identity, and a private file wins over a shared file at the same path. Dry-run
/// unless `opts.apply`. Every stored path is validated before any filesystem call, so a
/// manifest that has been tampered with cannot write outside the project root.
pub async fn pull_env(
    session: &mut Session,
    ctx: &Ctx,
    opts: &Opts,
    only: &[String],
) -> anyhow::Result<()> {
    let root = std::env::current_dir()?;
    let scope_id = crypto::scope_id(&ctx.project, &ctx.env_name)?;
    let owner = hex::encode(scope_id);
    let secret =
        recover_scope_secret(session, scope_id, ctx.epoch(), ctx.scope_pubkey.as_deref()).await?;
    let (shared, mine) = scope_private::both_manifests(session, ctx, (&owner, &secret)).await?;
    let merged = scope_private::merge(&shared, mine.as_ref());
    scope_private::report_shadows(&merged.shadowed);
    let selected: Vec<&Pick> = merged
        .picks
        .iter()
        .filter(|p| selects(&p.entry.relative_path, only))
        .collect();
    refuse_empty_selection(selected.len(), only, &ctx.env_name)?;
    restore_all(session, ctx, (&owner, &secret), (&root, &selected, opts)).await?;
    report(selected.len(), merged.picks.len(), &ctx.env_name);
    Ok(())
}

/// Restore every selected file, opening each with the key its manifest came from.
async fn restore_all(
    session: &mut Session,
    ctx: &Ctx,
    key: (&str, &Zeroizing<[u8; 32]>),
    what: (&Path, &[&Pick<'_>], &Opts),
) -> anyhow::Result<()> {
    let (owner, secret) = key;
    let (root, selected, opts) = what;
    if !opts.apply {
        ui::field("pull-env", "dry-run — re-run with --apply to write");
    }
    for pick in selected {
        let key = if pick.private {
            Key::Me
        } else {
            Key::Scope(secret)
        };
        restore_one(session, ctx, (owner, &key), (root, pick.entry, opts)).await?;
    }
    Ok(())
}

/// Whether a stored path is selected: everything when no pattern is given, else any match.
fn selects(relative_path: &str, only: &[String]) -> bool {
    only.is_empty()
        || only
            .iter()
            .any(|pattern| project::glob_match(relative_path, pattern))
}

/// Refuse a selection that matched nothing, naming the patterns that missed.
///
/// An empty restore is otherwise indistinguishable from a clean one: `--only 'secret/*'` for
/// `secrets/` writes no files, exits 0, and reads as "nothing to do". The whole reason to
/// select a subset is that the rest matters too much to touch, so a typo has to stop.
fn refuse_empty_selection(selected: usize, only: &[String], env: &str) -> anyhow::Result<()> {
    if selected > 0 || only.is_empty() {
        return Ok(());
    }
    anyhow::bail!(
        "no file in environment '{env}' matches {} — nothing was restored",
        only.join(", ")
    )
}

/// Report what was restored, and what was deliberately left behind.
///
/// Naming the remainder matters on a partial restore: a tree that is complete and a tree that
/// is one selection of several look identical on disk afterwards.
fn report(selected: usize, total: usize, env: &str) {
    if selected == total {
        ui::success(&format!("{selected} file(s) from environment '{env}'"));
        return;
    }
    ui::success(&format!(
        "{selected} of {total} file(s) from environment '{env}' — {} not selected",
        total - selected
    ));
}

/// Seal one file to the environment and return the manifest entry describing it.
async fn seal_one(
    session: &mut Session,
    ctx: &Ctx,
    at: (&str, &str),
    plaintext: &[u8],
) -> anyhow::Result<Entry> {
    let (owner, rel) = at;
    let vault_path = tree_path(owner, rel);
    let to = scope_public(ctx)?;
    let rev = put_one(session, ctx, (owner, &vault_path, to), plaintext).await?;
    Ok(Entry::file(rel, vault_path, rev, plaintext.len() as u64))
}

/// Seal `plaintext` to `to` — the environment's key, or the caller's own for a private
/// file — and store it at `path` under the environment's owner.
pub(super) async fn put_one(
    session: &mut Session,
    ctx: &Ctx,
    at: (&str, &str, RecipientPublicKey),
    plaintext: &[u8],
) -> anyhow::Result<u64> {
    let (owner, path, to) = at;
    let epoch = ctx.epoch();
    let current = head_version(session, owner, epoch, path).await?;
    let envelope = compose::scope_envelope(
        &session.identity,
        &ScopeSeal {
            owner,
            vault_path: path,
            project_id: owner,
            scope_pub: to,
            rev: current + 1,
            plaintext,
        },
    )?;
    session
        .put_env_secret(EnvSecretPut {
            scope_id: owner,
            epoch,
            path,
            envelope,
            expected_prev_rev: current,
        })
        .await
}

/// Fetch and decrypt one env secret, or `None` when the environment holds nothing at `path`.
pub(super) async fn fetch_one(
    session: &mut Session,
    ctx: &Ctx,
    at: (&str, &str),
    key: &Key<'_>,
) -> anyhow::Result<Option<Zeroizing<Vec<u8>>>> {
    let (owner, path) = at;
    let Some((envelope, author)) = session.get_env_secret(owner, ctx.epoch(), path).await? else {
        return Ok(None);
    };
    let expected = derive::secret_id(owner, path);
    let scope = ReadScope {
        secret_id: &expected,
        min_rev: 0,
    };
    let plain = match key {
        Key::Scope(secret) => decrypt::open_env_envelope(secret, &envelope, &author, scope)?,
        Key::Me => decrypt::open_private_envelope(&session.identity, &envelope, &author, scope)?,
    };
    Ok(Some(plain))
}

/// Fetch and decrypt one env secret that has to be there.
pub(super) async fn open_one(
    session: &mut Session,
    ctx: &Ctx,
    at: (&str, &str),
    key: &Key<'_>,
) -> anyhow::Result<Zeroizing<Vec<u8>>> {
    fetch_one(session, ctx, at, key)
        .await?
        .ok_or_else(|| anyhow::anyhow!("environment '{}' holds no tree", ctx.env_name))
}

/// Restore one file from the manifest, validating its stored path before touching disk.
async fn restore_one(
    session: &mut Session,
    ctx: &Ctx,
    key: (&str, &Key<'_>),
    what: (&Path, &Entry, &Opts),
) -> anyhow::Result<()> {
    let (owner, key) = key;
    let (root, entry, opts) = what;
    let rel = projpath::validate_stored(&entry.relative_path)?; // sec: validate before any FS op
    let stored = open_one(session, ctx, (owner, &entry.vault_path), key).await?;
    let bytes = match (entry.chunked, key) {
        (false, _) => stored,
        (true, Key::Scope(secret)) => {
            let set = ChunkSet::from_bytes(&stored)?;
            scope_chunks::open_large(session, ctx, (owner, secret), &set).await?
        }
        (true, Key::Me) => anyhow::bail!(
            "private file '{}' claims to be chunked, which this client never writes",
            entry.relative_path
        ),
    };
    if !opts.apply {
        ui::field(rel.as_str(), &format!("{} byte(s)", bytes.len()));
        return Ok(());
    }
    materialize::write_one(root, &rel, &bytes, owner_only(entry.mode), opts.backup)?;
    ui::field(rel.as_str(), "restored");
    Ok(())
}

/// Narrow a mode from the manifest to the owner alone.
///
/// The manifest is written by whoever may write the environment, which on a team is not
/// necessarily the person pulling. A hostile entry asking for 0777 on a private key restores
/// it readable by every process on the machine, and nothing about the BYTES is wrong, so no
/// integrity check notices. Clamping rather than refusing on purpose: refusing would let any
/// writer deny the whole tree to everybody with one entry, while clamping restores the file
/// safely and cannot be widened by anyone.
///
/// The cost is that a legitimately group-readable file comes back owner-only. For a tree of
/// credentials that is the right default, and it is the direction that cannot hurt.
fn owner_only(mode: u32) -> u32 {
    let narrowed = mode & 0o700;
    if narrowed == 0 {
        0o600
    } else {
        narrowed
    }
}

/// The opaque env-secret path for one real relative path (the real path never appears here).
pub(super) fn tree_path(owner: &str, rel: &str) -> String {
    format!("__42ctl/f/{}", derive::secret_id(owner, rel))
}

/// The file's Unix mode (low 9 bits), or 0o600 on non-Unix.
pub(super) fn file_mode(file: &Path) -> u32 {
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

/// Decode the env's published scope public key, erroring when the env has no keyset yet.
pub(super) fn scope_public(ctx: &Ctx) -> anyhow::Result<RecipientPublicKey> {
    let b64 = ctx
        .scope_pubkey
        .as_deref()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "env '{}' has no scope key — run `vault env-init` first",
                ctx.env_name
            )
        })?;
    crypto::x25519_pub(b64)
}

/// The current head version of `path` within `(scope_id, epoch)`, or 0 if absent.
pub(super) async fn head_version(
    session: &mut Session,
    owner: &str,
    epoch: u32,
    path: &str,
) -> anyhow::Result<u64> {
    Ok(session
        .list_env_secrets(owner, epoch)
        .await?
        .into_iter()
        .find(|entry| entry.path == path)
        .map(|entry| entry.version)
        .unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The server sees this path. It must not contain the real one, or the manifest hiding
    /// paths would be pointless — the same rule the personal push follows.
    #[test]
    fn the_stored_path_never_contains_the_real_one() {
        let path = tree_path("deadbeef", "srcs/.env.production");
        assert!(!path.contains("srcs"), "{path}");
        assert!(!path.contains(".env"), "{path}");
        assert!(path.starts_with("__42ctl/f/"), "{path}");
    }

    /// Deterministic, so a second push of the same file updates it rather than adding a
    /// second copy under a fresh name.
    #[test]
    fn the_same_file_maps_to_the_same_stored_path() {
        assert_eq!(tree_path("o", "srcs/.env"), tree_path("o", "srcs/.env"));
    }

    /// The mode in a shared manifest is hostile input: a teammate may write it. 0777 on a
    /// private key restores it readable by everything on the box, with correct bytes, so
    /// nothing downstream would notice.
    #[test]
    fn a_mode_from_the_manifest_never_reaches_group_or_other() {
        for asked in [0o777, 0o666, 0o644, 0o755, 0o707, 0o604] {
            let got = owner_only(asked);
            assert_eq!(got & 0o077, 0, "0{asked:o} restored as 0{got:o}");
        }
    }

    /// And the ordinary case is untouched: a 0600 key comes back 0600, not narrowed further
    /// and not widened.
    #[test]
    fn an_owner_only_mode_survives_unchanged() {
        assert_eq!(owner_only(0o600), 0o600);
        assert_eq!(owner_only(0o700), 0o700);
    }

    /// A manifest asking for nothing at all must not produce a file its owner cannot read.
    #[test]
    fn a_zero_mode_becomes_readable_by_its_owner() {
        assert_eq!(owner_only(0), 0o600);
        assert_eq!(owner_only(0o044), 0o600);
    }

    /// Two files must not collide, and two ENVIRONMENTS must not either: the owner is the
    /// scope id, so the same path in a different environment is a different secret.
    #[test]
    fn different_files_and_different_environments_do_not_collide() {
        assert_ne!(tree_path("o", "srcs/.env"), tree_path("o", "srcs/.env.dev"));
        assert_ne!(
            tree_path("prod-scope", "srcs/.env"),
            tree_path("dev-scope", "srcs/.env")
        );
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    /// No pattern means the whole tree, which is what every existing caller relies on.
    #[test]
    fn an_empty_selection_takes_everything() {
        assert!(selects("secrets/db_password.txt", &[]));
        assert!(selects("srcs/.env", &[]));
    }

    /// The three shapes an operator actually types: a directory, an extension, an exact file.
    #[test]
    fn a_pattern_selects_by_directory_extension_or_exact_path() {
        let dir = vec!["secrets/*".to_string()];
        assert!(selects("secrets/ca.key", &dir));
        assert!(
            !selects("srcs/.env", &dir),
            "a sibling must not be selected"
        );

        let ext = vec!["*.crt".to_string()];
        assert!(selects("secrets/server.crt", &ext));
        assert!(!selects("secrets/server.key", &ext));

        let exact = vec!["srcs/.env".to_string()];
        assert!(selects("srcs/.env", &exact));
        assert!(
            !selects("srcs/.env.example", &exact),
            "an exact pattern must not match a longer path"
        );
    }

    /// Repeating the flag unions the selections rather than narrowing them.
    #[test]
    fn several_patterns_union() {
        let both = vec!["secrets/ca.*".to_string(), "srcs/.env".to_string()];
        assert!(selects("secrets/ca.key", &both));
        assert!(selects("srcs/.env", &both));
        assert!(!selects("secrets/server.key", &both));
    }

    /// A pattern that matches nothing must stop, not report a clean restore of no files.
    #[test]
    fn a_selection_matching_nothing_is_refused() {
        let only = vec!["secret/*".to_string()];
        let err = refuse_empty_selection(0, &only, "prod")
            .expect_err("a typo must not look like success");
        let said = err.to_string();
        assert!(said.contains("secret/*"), "must name the pattern: {said}");
        assert!(said.contains("nothing was restored"), "{said}");
    }

    /// An empty ENVIRONMENT with no pattern is a legitimate no-op, not an error.
    #[test]
    fn an_empty_environment_without_a_pattern_is_not_an_error() {
        refuse_empty_selection(0, &[], "prod").expect("no pattern means no selection to miss");
    }
}
