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
use crate::cmd::scope_recover::recover_scope_secret;
use crate::core::manifest::{Entry, Manifest};
use crate::core::materialize::Opts;
use crate::core::{materialize, project, projpath};
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
    let owner = hex::encode(crypto::scope_id(&ctx.project, &ctx.env_name)?);
    let mut manifest = Manifest::new(&proj.project_id);
    for file in &files {
        let rel = projpath::canonicalize_for_storage(file, &proj.root)?;
        let plaintext = Zeroizing::new(std::fs::read(file)?);
        let entry = seal_one(session, ctx, (&owner, rel.as_str()), &plaintext).await?;
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
pub async fn pull_env(session: &mut Session, ctx: &Ctx, opts: &Opts) -> anyhow::Result<()> {
    let root = std::env::current_dir()?;
    let scope_id = crypto::scope_id(&ctx.project, &ctx.env_name)?;
    let owner = hex::encode(scope_id);
    let epoch = ctx.scope_epoch.max(1);
    let secret =
        recover_scope_secret(session, scope_id, epoch, ctx.scope_pubkey.as_deref()).await?;
    let raw = open_one(session, ctx, (&owner, TREE_MANIFEST), &secret).await?;
    let manifest = Manifest::parse(&raw)?;
    if !opts.apply {
        ui::field("pull-env", "dry-run — re-run with --apply to write");
    }
    for entry in manifest
        .entries
        .iter()
        .filter(|e| e.kind != Kind::Note as u8)
    {
        restore_one(session, ctx, (&owner, &secret), (&root, entry, opts)).await?;
    }
    ui::success(&format!(
        "{} file(s) from environment '{}'",
        manifest.entries.len(),
        ctx.env_name
    ));
    Ok(())
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
    let bytes = open_one(session, ctx, (owner, &entry.vault_path), secret).await?;
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
