/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope_secret.rs                                     :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `vault set-env` / `get-env` — shared per-environment secrets. `set-env` seals stdin to the
//! env's PUBLIC scope key (so any wrapped member can later read it, but the caller need not be
//! one) and pushes the opaque envelope with optimistic concurrency. `get-env` recovers the env
//! scope SECRET from the caller's own wrap and decrypts the fetched envelope locally with it.
//! Zero-knowledge throughout: the server stores only the scope-sealed blob and never a DEK.

use crate::adapters::api::Session;
use crate::adapters::compose::{self, ScopeSeal};
use crate::adapters::scope as crypto;
use crate::adapters::scope_env_grpc::EnvSecretPut;
use crate::adapters::{decrypt, derive};
use crate::cmd::scope::Ctx;
use crate::cmd::scope_recover::recover_scope_secret;
use crate::cmd::vault::read_input;
use crate::ui;
use vault42_core::ReadScope;

/// Seal stdin to the env's scope public key at `path` and store it at the next version. The
/// scope is the sole recipient, so the caller seals without holding the scope secret.
pub async fn set_env(session: &mut Session, ctx: &Ctx, path: &str) -> anyhow::Result<()> {
    let scope_id = crypto::scope_id(&ctx.project, &ctx.env_name)?;
    let epoch = ctx.scope_epoch.max(1);
    let owner = hex::encode(scope_id);
    let scope_pub = scope_public(ctx)?;
    let plaintext = read_input(None)?;
    let current = head_version(session, &owner, epoch, path).await?;
    let envelope = compose::scope_envelope(
        &session.identity,
        &ScopeSeal {
            owner: &owner,
            vault_path: path,
            project_id: &owner,
            scope_pub,
            rev: current + 1,
            plaintext: plaintext.as_slice(),
        },
    )?;
    let version = session
        .put_env_secret(EnvSecretPut {
            scope_id: &owner,
            epoch,
            path,
            envelope,
            expected_prev_rev: current,
        })
        .await?;
    ui::success(&format!("set-env {path} (v{version})"));
    Ok(())
}

/// Recover the scope secret, fetch the env secret at `path`, decrypt it with the scope
/// secret, and write the plaintext to stdout. Errors if the path has no version.
pub async fn get_env(session: &mut Session, ctx: &Ctx, path: &str) -> anyhow::Result<()> {
    let scope_id = crypto::scope_id(&ctx.project, &ctx.env_name)?;
    let epoch = ctx.scope_epoch.max(1);
    let owner = hex::encode(scope_id);
    let secret = recover_scope_secret(session, scope_id, epoch).await?;
    let (envelope, author) = session
        .get_env_secret(&owner, epoch, path)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no env secret '{path}' for this env"))?;
    let expected = derive::secret_id(&owner, path);
    let scope = ReadScope {
        secret_id: &expected,
        min_rev: 0,
    };
    let plaintext = decrypt::open_env_envelope(&secret, &envelope, &author, scope)?;
    use std::io::Write;
    std::io::stdout().write_all(&plaintext)?;
    Ok(())
}

/// Decode the env's published base64 scope public key into a wrap target, erroring when the
/// env has no scope key yet (run `vault env-init` first).
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

/// The current head version of `path` within `(scope_id, epoch)`, or 0 if it does not exist
/// yet — the optimistic-concurrency token for the next put.
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
