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
//! the NEW one (`reseal_all`), and re-wrap the new scope key to the env's authorized members
//! (`rewrap_remaining`). The old/new scope secrets stay in `Zeroizing` buffers; only opaque
//! envelopes + AEAD-wrapped grants ever leave. A member the control plane no longer reports as
//! authorized is simply absent from the rewraps, so it cannot reach the new epoch.
//!
//! Two rules make that revocation-by-absence safe rather than terminal. The re-wrap set is the
//! grant's AUTHORIZED members, never the provisioning worklist — the worklist is empty once
//! everyone is provisioned, so rotating from it re-wraps nobody and the new key, which lives
//! only in this process, is destroyed with it. And the rotating administrator is always
//! re-wrapped, so a rotation stays repairable even when every other member is skipped.

use crate::adapters::api::Session;
use crate::adapters::compose::{self, ScopeSeal};
use crate::adapters::rbac::{grant, pubkey};
use crate::adapters::scope;
use crate::adapters::scope_env_grpc::EnvSecretPut;
use crate::adapters::{decrypt, derive};
use crate::cmd::scope::{self as orch, Ctx};
use crate::cmd::scope_pubkey;
use vault42_core::{grant_scope_key, Identity, ReadScope, ScopeKeyset};
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

/// Re-wrap the new scope key to every member the env's grants authorize, always including the
/// rotating administrator, and deposit them via one `RotateScope`. Records each re-wrapped
/// member against the control plane at the new epoch and returns how many were stored.
pub async fn rewrap_remaining(
    session: &mut Session,
    ctx: &Ctx,
    state: &RotateState<'_>,
) -> anyhow::Result<usize> {
    let mut rewraps: Vec<WrapScopeKeyRequest> = Vec::new();
    let mut wrapped: Vec<(String, Vec<String>)> = Vec::new();
    for (user, grant_ids) in orch::env_members(ctx).await?.authorized {
        if let Some(rewrap) = build_rewrap(session, ctx, state, &user).await? {
            rewraps.push(rewrap);
            wrapped.push((user, grant_ids));
        }
    }
    push_self_rewrap(&session.identity, &session.principal, state, &mut rewraps)?;
    let count = rewraps.len();
    session
        .rotate_scope(&hex::encode(state.scope_id), state.new_epoch, rewraps)
        .await?;
    record_new_epoch(ctx, state.new_epoch, &wrapped).await?;
    Ok(count)
}

/// Append the rotating administrator's own new-epoch wrap unless a member rewrap already
/// covers her.
///
/// She holds the only copy of the new scope secret, in this process, in a `Zeroizing` buffer.
/// Without her own wrap she can neither read the secrets she has just re-sealed nor run the
/// reconcile that would provision anybody else, and an epoch never regresses, so there is no
/// way back. `member_id` is the caller's own key fingerprint, the same value a member rewrap
/// carries, so this only adds a wrap that is not already there.
fn push_self_rewrap(
    identity: &Identity,
    principal: &str,
    state: &RotateState<'_>,
    rewraps: &mut Vec<WrapScopeKeyRequest>,
) -> anyhow::Result<()> {
    if rewraps.iter().any(|r| r.member_id == principal) {
        return Ok(());
    }
    let grant = grant_scope_key(
        state.new_secret,
        &identity.encryption_public(),
        identity.signing_key(),
        state.scope_id,
        state.new_epoch,
    )?;
    rewraps.push(WrapScopeKeyRequest {
        member_id: principal.to_string(),
        scope_id: hex::encode(state.scope_id),
        epoch: state.new_epoch,
        granted_blob: grant.to_bytes()?,
        granter_pubkey: identity.author_public().to_bytes().to_vec(),
    });
    Ok(())
}

/// Record each re-wrapped member's wrap at the NEW epoch, so `scope-status` and the next
/// reconcile describe the rotated epoch instead of the one it replaced. The administrator's
/// self-wrap is recorded only when a grant covers her, since a wrap needs a grant to hang on.
async fn record_new_epoch(
    ctx: &Ctx,
    new_epoch: u32,
    wrapped: &[(String, Vec<String>)],
) -> anyhow::Result<()> {
    let scope = ctx.grant_scope(new_epoch);
    for (user, grant_ids) in wrapped {
        for grant_id in grant_ids {
            grant::record_wrap(&scope, grant_id, user).await?;
        }
    }
    Ok(())
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
    if !scope_pubkey::verify_member(&pk, &ctx.org_id) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use vault42_core::generate_keyset;

    /// A rotation state over a throwaway keyset, as `rotate_scope` builds it.
    fn state(scope_id: [u8; 16]) -> (Zeroizing<[u8; 32]>, ScopeKeyset, Zeroizing<[u8; 32]>) {
        let (_old_keyset, old) = generate_keyset(scope_id, 1);
        let (keyset, new) = generate_keyset(scope_id, 2);
        (old, keyset, new)
    }

    /// One `WrapScopeKeyRequest` naming `member`, with no real grant bytes — the dedup rule
    /// looks only at `member_id`.
    fn rewrap_for(member: &str) -> WrapScopeKeyRequest {
        WrapScopeKeyRequest {
            member_id: member.to_string(),
            scope_id: String::new(),
            epoch: 2,
            granted_blob: Vec::new(),
            granter_pubkey: Vec::new(),
        }
    }

    /// A rotation that re-wraps nobody else still re-wraps the administrator running it.
    ///
    /// This is the defect that stranded whole environments: the re-wrap set was read from the
    /// provisioning worklist, which is empty once everyone is provisioned, so rotation wrapped
    /// the new key to nobody and it died with the process.
    #[test]
    fn a_rotation_with_no_other_member_still_wraps_the_administrator() {
        let identity = Identity::generate();
        let scope_id = [3u8; 16];
        let (old, keyset, new) = state(scope_id);
        let rotate = RotateState {
            scope_id,
            old_epoch: 1,
            new_epoch: 2,
            old_secret: &old,
            new_secret: &new,
            keyset: &keyset,
        };
        let mut rewraps = Vec::new();
        push_self_rewrap(&identity, "admin-fingerprint", &rotate, &mut rewraps).expect("self wrap");
        assert_eq!(rewraps.len(), 1, "the rotator must never be left out");
        assert_eq!(rewraps[0].member_id, "admin-fingerprint");
        assert_eq!(
            rewraps[0].epoch, 2,
            "the self-wrap is bound to the new epoch"
        );
        assert!(!rewraps[0].granted_blob.is_empty());
    }

    /// An administrator already covered by a member rewrap is not wrapped twice: her
    /// `member_id` is her key fingerprint, the same value a member rewrap carries.
    #[test]
    fn an_administrator_already_in_the_rewrap_set_is_not_duplicated() {
        let identity = Identity::generate();
        let scope_id = [4u8; 16];
        let (old, keyset, new) = state(scope_id);
        let rotate = RotateState {
            scope_id,
            old_epoch: 1,
            new_epoch: 2,
            old_secret: &old,
            new_secret: &new,
            keyset: &keyset,
        };
        let mut rewraps = vec![rewrap_for("other"), rewrap_for("admin-fingerprint")];
        push_self_rewrap(&identity, "admin-fingerprint", &rotate, &mut rewraps).expect("self wrap");
        assert_eq!(rewraps.len(), 2, "an already-covered rotator adds nothing");
    }
}
