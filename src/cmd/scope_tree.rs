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
use crate::adapters::blobstore::BlobStore;
use crate::adapters::compose::{self, ScopeChunkSeal, ScopeSeal};
use crate::adapters::scope as crypto;
use crate::adapters::scope_env_grpc::EnvSecretPut;
use crate::adapters::{decrypt, derive};
use crate::cmd::scope::Ctx;
use crate::cmd::scope_recover::recover_scope_secret;
use crate::core::chunk::{self, ChunkSet, Naming};
use crate::core::manifest::{Entry, Manifest};
use crate::core::materialize::Opts;
use crate::core::{materialize, project, projpath};
use crate::ops::largeobj;
use crate::ui;
use vault42_core::{Kind, ReadScope};
use zeroize::Zeroizing;

/// The reserved env-secret path holding the tree manifest.
const TREE_MANIFEST: &str = "__42ctl/tree";

/// Scan the project at the working directory and seal every file to the environment.
///
/// The manifest goes last, so an interrupted push leaves files nobody references rather than
/// a manifest naming files that are not there — the same commit-point rule the chunked-object
/// path uses, for the same reason.
pub async fn push_env(session: &mut Session, ctx: &Ctx) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let (proj, _) = project::open(&cwd, None)?;
    let files = project::scan(&proj)?.files;
    let scope_id = crypto::scope_id(&ctx.project, &ctx.env_name)?;
    let owner = hex::encode(scope_id);
    let mut manifest = Manifest::new(&proj.project_id);
    let mut naming: Option<Naming> = None;
    for file in &files {
        let rel = projpath::canonicalize_for_storage(file, &proj.root)?;
        let plaintext = Zeroizing::new(std::fs::read(file)?);
        let entry = if chunk::needs_chunking(plaintext.len() as u64) {
            if naming.is_none() {
                naming = Some(environment_naming(session, ctx, scope_id).await?);
            }
            let named = naming.as_ref().expect("just derived");
            seal_large(session, ctx, (&owner, rel.as_str(), named), &plaintext).await?
        } else {
            seal_one(session, ctx, (&owner, rel.as_str()), &plaintext).await?
        };
        manifest.upsert(Entry {
            mode: file_mode(file),
            ..entry
        });
    }
    put_one(session, ctx, &owner, TREE_MANIFEST, &manifest.to_bytes()?).await?;
    ui::success(&format!(
        "pushed {} file(s) to environment '{}'",
        files.len(),
        ctx.env_name
    ));
    Ok(())
}

/// Recover the environment's key, fetch the manifest, and restore every file it names.
///
/// Dry-run unless `opts.apply`. Every stored path is validated before any filesystem call,
/// so a manifest that has been tampered with cannot write outside the project root.
pub async fn pull_env(
    session: &mut Session,
    ctx: &Ctx,
    opts: &Opts,
    only: &[String],
) -> anyhow::Result<()> {
    let root = std::env::current_dir()?;
    let scope_id = crypto::scope_id(&ctx.project, &ctx.env_name)?;
    let owner = hex::encode(scope_id);
    let epoch = ctx.scope_epoch.max(1);
    let secret =
        recover_scope_secret(session, scope_id, epoch, ctx.scope_pubkey.as_deref()).await?;
    let raw = open_one(session, ctx, (&owner, TREE_MANIFEST), &secret).await?;
    let manifest = Manifest::parse(&raw)?;
    let files: Vec<&Entry> = manifest
        .entries
        .iter()
        .filter(|e| e.kind != Kind::Note as u8)
        .collect();
    let selected: Vec<&&Entry> = files
        .iter()
        .filter(|e| selects(&e.relative_path, only))
        .collect();
    refuse_empty_selection(&selected, only, &ctx.env_name)?;
    if !opts.apply {
        ui::field("pull-env", "dry-run — re-run with --apply to write");
    }
    for entry in &selected {
        restore_one(session, ctx, (&owner, &secret), (&root, entry, opts)).await?;
    }
    report(selected.len(), files.len(), &ctx.env_name);
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
fn refuse_empty_selection(selected: &[&&Entry], only: &[String], env: &str) -> anyhow::Result<()> {
    if !selected.is_empty() || only.is_empty() {
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
    let rev = put_one(session, ctx, owner, &vault_path, plaintext).await?;
    Ok(Entry {
        relative_path: rel.to_string(),
        vault_path,
        mode: 0o600,
        kind: Kind::EnvFile as u8,
        chunked: false,
        rev,
    })
}

/// Seal `plaintext` to the environment's public key and store it at `path`.
async fn put_one(
    session: &mut Session,
    ctx: &Ctx,
    owner: &str,
    path: &str,
    plaintext: &[u8],
) -> anyhow::Result<u64> {
    let epoch = ctx.scope_epoch.max(1);
    let current = head_version(session, owner, epoch, path).await?;
    let envelope = compose::scope_envelope(
        &session.identity,
        &ScopeSeal {
            owner,
            vault_path: path,
            project_id: owner,
            scope_pub: scope_public(ctx)?,
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

/// Fetch and decrypt one env secret with the recovered scope secret.
async fn open_one(
    session: &mut Session,
    ctx: &Ctx,
    at: (&str, &str),
    secret: &Zeroizing<[u8; 32]>,
) -> anyhow::Result<Zeroizing<Vec<u8>>> {
    let (owner, path) = at;
    let epoch = ctx.scope_epoch.max(1);
    let (envelope, author) = session
        .get_env_secret(owner, epoch, path)
        .await?
        .ok_or_else(|| anyhow::anyhow!("environment '{}' holds no tree", ctx.env_name))?;
    let expected = derive::secret_id(owner, path);
    let scope = ReadScope {
        secret_id: &expected,
        min_rev: 0,
    };
    decrypt::open_env_envelope(secret, &envelope, &author, scope)
}

/// Restore one file from the manifest, validating its stored path before touching disk.
async fn restore_one(
    session: &mut Session,
    ctx: &Ctx,
    key: (&str, &Zeroizing<[u8; 32]>),
    what: (&std::path::Path, &Entry, &Opts),
) -> anyhow::Result<()> {
    let (owner, secret) = key;
    let (root, entry, opts) = what;
    let rel = projpath::validate_stored(&entry.relative_path)?; // sec: validate before any FS op
    let stored = open_one(session, ctx, (owner, &entry.vault_path), secret).await?;
    let bytes = if entry.chunked {
        open_large(
            session,
            ctx,
            (owner, secret),
            &ChunkSet::from_bytes(&stored)?,
        )
        .await?
    } else {
        stored
    };
    if !opts.apply {
        ui::field(rel.as_str(), &format!("{} byte(s)", bytes.len()));
        return Ok(());
    }
    materialize::write_one(root, &rel, &bytes, owner_only(entry.mode), opts.backup)?;
    ui::field(rel.as_str(), "restored");
    Ok(())
}

/// Split a large file into chunks in the object store and keep only the list in the vault.
///
/// The chunks are sealed to the ENVIRONMENT rather than to the pusher, so every member who
/// holds a wrap can open them. Sealing them to one identity would put the team's archive
/// somewhere only its author can read, which is the failure the shared tree exists to avoid.
async fn seal_large(
    session: &mut Session,
    ctx: &Ctx,
    at: (&str, &str, &Naming),
    plaintext: &[u8],
) -> anyhow::Result<Entry> {
    let (owner, rel, naming) = at;
    let vault_path = tree_path(owner, rel);
    let store = store_for(session, rel, plaintext.len())?;
    store.ensure_bucket().await?;
    let scope_pub = scope_public(ctx)?;
    let identity = &session.identity;
    let author = identity.author_public().to_bytes();
    let set = largeobj::put_chunks(store, (&vault_path, naming), plaintext, |part, bytes| {
        let envelope = compose::scope_chunk_envelope(
            identity,
            &ScopeChunkSeal {
                scope_owner: owner,
                name: &part.name,
                scope_pub,
                plaintext: bytes,
            },
        )?;
        Ok(compose::chunkframe::wrap(&author, &envelope))
    })
    .await?;
    let rev = put_one(session, ctx, owner, &vault_path, &set.to_bytes()?).await?;
    ui::field(
        rel,
        &format!("{} chunk(s) in the object store", set.chunks.len()),
    );
    Ok(Entry {
        relative_path: rel.to_string(),
        vault_path,
        mode: 0o600,
        kind: Kind::EnvFile as u8,
        chunked: true,
        rev,
    })
}

/// Fetch and reassemble a chunked entry, opening each chunk with the environment's key.
async fn open_large(
    session: &mut Session,
    ctx: &Ctx,
    key: (&str, &Zeroizing<[u8; 32]>),
    set: &ChunkSet,
) -> anyhow::Result<Zeroizing<Vec<u8>>> {
    let (owner, secret) = key;
    let naming = Naming::new(owner, secret.as_slice());
    let store = store_for(session, &ctx.env_name, set.total_len as usize)?;
    largeobj::get_chunks(store, set, |name, stored| {
        let (author, envelope) = compose::chunkframe::unwrap(stored)?;
        let expected = derive::secret_id(owner, name);
        let scope = ReadScope {
            secret_id: &expected,
            min_rev: 0,
        };
        let plain = decrypt::open_env_envelope(secret, envelope, &author, scope)?;
        verify_name(&naming, name, &plain)?;
        Ok(plain)
    })
    .await
}

/// Require a chunk's name to be the keyed hash of the bytes it turned out to hold.
///
/// Deduplication is what makes this necessary. A writer whose chunk name already exists skips
/// the upload and points at what is there, so a member who seals unrelated bytes under a name
/// they computed dishonestly poisons every later writer of that content: the honest writer
/// stores nothing, and their restore returns the poisoner's bytes. Nothing else notices —
/// the envelope is validly sealed, validly signed, and bound to the right secret id.
///
/// Checked on READ rather than on write, because the write side is where the dishonest party
/// stands. Recomputing costs one keyed hash per chunk against bytes already in memory.
fn verify_name(naming: &Naming, name: &str, plaintext: &[u8]) -> anyhow::Result<()> {
    if chunk::chunk_name(naming, plaintext) == name {
        return Ok(());
    }
    anyhow::bail!("chunk {name} does not hold the bytes its name says it does")
}

/// The configured object store, or a refusal naming what to configure.
fn store_for<'a>(session: &'a Session, what: &str, len: usize) -> anyhow::Result<&'a BlobStore> {
    session.store.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "{what} is {len} bytes, above the transport ceiling, and this profile names no \
             object store — set one with `42ctl config endpoint --blobstore <url> --bucket \
             <name>` and export FT_S3_KEY and FT_S3_SECRET"
        )
    })
}

/// How this environment names its chunks: a prefix and key derived from the scope secret.
///
/// Derived from the SCOPE rather than from a person, so two members holding the same bytes
/// compute the same name and the second one stores nothing. That is the deduplication a team
/// actually wants, and it needs no convergent ciphertext: identical names plus the
/// already-present check mean the first copy is the only copy, and every member can open it
/// because it is sealed to the environment.
async fn environment_naming(
    session: &mut Session,
    ctx: &Ctx,
    scope_id: [u8; 16],
) -> anyhow::Result<Naming> {
    let epoch = ctx.scope_epoch.max(1);
    let secret =
        recover_scope_secret(session, scope_id, epoch, ctx.scope_pubkey.as_deref()).await?;
    Ok(Naming::new(&hex::encode(scope_id), secret.as_slice()))
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
fn tree_path(owner: &str, rel: &str) -> String {
    format!("__42ctl/f/{}", derive::secret_id(owner, rel))
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

/// Decode the env's published scope public key, erroring when the env has no keyset yet.
fn scope_public(ctx: &Ctx) -> anyhow::Result<vault42_core::RecipientPublicKey> {
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
async fn head_version(
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

    /// A chunk whose plaintext does not hash to the name it was fetched under is refused.
    ///
    /// This is the poisoning case, and it is the one a substituted OBJECT cannot show: copying
    /// a whole stored chunk over another's name is caught earlier, by the secret id bound to
    /// the name it was sealed for. What this catches is a dishonest client that seals
    /// correctly FOR a name while putting unrelated bytes inside — which every later writer of
    /// those real bytes then points at, storing nothing and restoring the poisoner's content.
    #[test]
    fn a_chunk_holding_other_bytes_than_its_name_says_is_refused() {
        let naming = Naming::new("scope-id", &[3u8; 32]);
        let honest = b"the real chunk bytes";
        let name = chunk::chunk_name(&naming, honest);
        verify_name(&naming, &name, honest).expect("the honest chunk must open");
        assert!(
            verify_name(&naming, &name, b"unrelated bytes").is_err(),
            "a chunk must not hold bytes other than the ones its name names"
        );
    }

    /// And the check is keyed: a different environment's key must not validate this one's
    /// chunk, or the poisoning defence would be portable between environments.
    #[test]
    fn the_name_check_is_bound_to_the_environment_key() {
        let bytes = b"shared content";
        let mine = Naming::new("scope-id", &[3u8; 32]);
        let theirs = Naming::new("scope-id", &[9u8; 32]);
        let name = chunk::chunk_name(&mine, bytes);
        assert!(verify_name(&theirs, &name, bytes).is_err());
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
        let err = refuse_empty_selection(&[], &only, "prod")
            .expect_err("a typo must not look like success");
        let said = err.to_string();
        assert!(said.contains("secret/*"), "must name the pattern: {said}");
        assert!(said.contains("nothing was restored"), "{said}");
    }

    /// An empty ENVIRONMENT with no pattern is a legitimate no-op, not an error.
    #[test]
    fn an_empty_environment_without_a_pattern_is_not_an_error() {
        refuse_empty_selection(&[], &[], "prod").expect("no pattern means no selection to miss");
    }
}
