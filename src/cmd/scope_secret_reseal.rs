/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope_secret_reseal.rs                              :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The two heavy halves of `rotate-scope`: re-seal every env secret from the OLD scope key to
//! the NEW one (`reseal_all`), and re-wrap the new scope key to the env's remaining authorized
//! members (`rewrap_remaining`). The old/new scope secrets stay in `Zeroizing` buffers; only
//! opaque envelopes + AEAD-wrapped grants ever leave. A member grobase no longer reports as
//! authorized is simply absent from the rewraps, so it cannot reach the new epoch.

use crate::adapters::api::Session;
use crate::adapters::compose::{self, ScopeSeal};
use crate::adapters::rbac::pubkey;
use crate::adapters::scope;
use crate::adapters::scope_env_grpc::EnvSecretPut;
use crate::adapters::{decrypt, derive};
use crate::cmd::scope::{self as orch, Ctx};
use crate::cmd::scope_pubkey;
use vault42_core::{grant_scope_key, ReadScope, ScopeKeyset};
use vault42_proto::vault::v1::WrapScopeKeyRequest;
use zeroize::Zeroizing;

/// The rotation's fixed identity: the scope id, both epochs, the recovered OLD secret (to open
/// existing secrets), the freshly generated keyset (its public key is the new seal target), and
/// the NEW secret (to wrap the new scope key to each remaining member).
pub struct RotateState<'a> {
    pub scope_id: [u8; 16],
    pub old_epoch: u32,
    pub new_epoch: u32,
    pub old_secret: &'a Zeroizing<[u8; 32]>,
    pub new_secret: &'a Zeroizing<[u8; 32]>,
    pub keyset: &'a ScopeKeyset,
}

/// Re-seal every old-epoch env secret to the new scope public key at the new epoch, returning
/// how many were re-sealed. Each is opened with the OLD scope secret and sealed to the NEW key.
pub async fn reseal_all(session: &mut Session, state: &RotateState<'_>) -> anyhow::Result<usize> {
    let owner = hex::encode(state.scope_id);
    let paths: Vec<String> = session
        .list_env_secrets(&owner, state.old_epoch)
        .await?
        .into_iter()
        .map(|entry| entry.path)
        .collect();
    let mut resealed = 0usize;
    for path in &paths {
        reseal_one(session, state, &owner, path).await?;
        resealed += 1;
    }
    Ok(resealed)
}

/// Open one old-epoch secret with the OLD scope secret and re-seal it to the NEW scope key at
/// the new epoch (create, `expected_prev_rev=0`).
async fn reseal_one(
    session: &mut Session,
    state: &RotateState<'_>,
    owner: &str,
    path: &str,
) -> anyhow::Result<()> {
    let plaintext = open_old(session, state, owner, path).await?;
    let envelope = compose::scope_envelope(
        &session.identity,
        &ScopeSeal {
            owner,
            vault_path: path,
            project_id: owner,
            scope_pub: state.keyset.public,
            rev: 1,
            plaintext: plaintext.as_slice(),
        },
    )?;
    session
        .put_env_secret(EnvSecretPut {
            scope_id: owner,
            epoch: state.new_epoch,
            path,
            envelope,
            expected_prev_rev: 0,
        })
        .await?;
    Ok(())
}

/// Fetch and decrypt one old-epoch env secret with the OLD scope secret.
async fn open_old(
    session: &mut Session,
    state: &RotateState<'_>,
    owner: &str,
    path: &str,
) -> anyhow::Result<Zeroizing<Vec<u8>>> {
    let (envelope, author) = session
        .get_env_secret(owner, state.old_epoch, path)
        .await?
        .ok_or_else(|| anyhow::anyhow!("env secret '{path}' vanished mid-rotation"))?;
    let expected = derive::secret_id(owner, path);
    let read = ReadScope {
        secret_id: &expected,
        min_rev: 0,
    };
    decrypt::open_env_envelope(state.old_secret, &envelope, &author, read)
}

/// Re-wrap the new scope key to the env's remaining authorized members (grobase effective minus
/// revoked) and deposit them via one `RotateScope`, returning how many the server stored.
pub async fn rewrap_remaining(
    session: &mut Session,
    ctx: &Ctx,
    state: &RotateState<'_>,
) -> anyhow::Result<usize> {
    let mut rewraps: Vec<WrapScopeKeyRequest> = Vec::new();
    // ponytail: authorized set = grobase env_pending (the listable effective members); a
    // member already wrapped but no longer pending is re-added by a follow-up sync-keys.
    for (user, _grant) in orch::env_pending(ctx).await? {
        if let Some(rewrap) = build_rewrap(session, ctx, state, &user).await? {
            rewraps.push(rewrap);
        }
    }
    let count = rewraps.len();
    session
        .rotate_scope(&hex::encode(state.scope_id), state.new_epoch, rewraps)
        .await?;
    Ok(count)
}

/// Build one member's new-epoch rewrap: fetch + verify their pubkey, wrap the new scope secret
/// to their X25519 key signed by the caller. Returns `None` for an unenrolled/unverifiable key.
async fn build_rewrap(
    session: &Session,
    ctx: &Ctx,
    state: &RotateState<'_>,
    user: &str,
) -> anyhow::Result<Option<WrapScopeKeyRequest>> {
    let Ok(pk) = pubkey::get(&ctx.grobase, &ctx.token, &ctx.org, user).await else {
        return Ok(None);
    };
    if !scope_pubkey::verify_member(&pk, &ctx.org) {
        return Ok(None);
    }
    let member_pub = scope::x25519_pub(&pk.x25519_pub)?;
    let granter = session.identity.signing_key();
    let grant = grant_scope_key(
        state.new_secret,
        &member_pub,
        granter,
        state.scope_id,
        state.new_epoch,
    )?;
    Ok(Some(WrapScopeKeyRequest {
        member_id: scope::member_id(&pk.ed25519_pub)?,
        scope_id: hex::encode(state.scope_id),
        epoch: state.new_epoch,
        granted_blob: grant.to_bytes()?,
        granter_pubkey: session.identity.author_public().to_bytes().to_vec(),
    }))
}
