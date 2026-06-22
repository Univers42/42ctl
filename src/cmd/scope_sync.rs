/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope_sync.rs                                       :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `vault sync-keys` — the admin reconcile. Recover the scope secret from the admin's own
//! wrap (the two-hop unwrap), enumerate every authorized member still missing a wrap, and
//! provision each that has a verifiable pubkey (wrap → deposit → record). Members with no
//! pubkey are skipped (pending-enrollment). Re-running converges: an already-wrapped member
//! drops out of grobase's `missing` list. The scope secret stays in a `Zeroizing` buffer.

use crate::adapters::api::Session;
use crate::adapters::scope as crypto;
use crate::cmd::scope::{self as orch, Ctx};
use crate::cmd::scope_wrap::{self, ScopeRef};
use crate::ui;
use vault42_core::{open_scope_key, AuthorPublicKey, GrantedScopeKey};
use zeroize::Zeroizing;

/// Reconcile every authorized member's wrap for the env scope. Prints how many members were
/// newly provisioned and how many were skipped (pending-enrollment).
pub async fn sync_keys(session: &mut Session, ctx: &Ctx) -> anyhow::Result<()> {
    let scope_id = crypto::scope_id(&ctx.project, &ctx.env_name)?;
    let secret = recover_scope_secret(session, ctx, scope_id).await?;
    let sref = ScopeRef {
        secret: &secret,
        id: scope_id,
        epoch: ctx.scope_epoch.max(1),
    };
    let (mut provisioned, mut skipped) = (0usize, 0usize);
    for (user, grant_ids) in group_by_user(orch::env_pending(ctx).await?) {
        if scope_wrap::provision(session, ctx, &sref, &user).await? {
            scope_wrap::record(ctx, &user, &grant_ids).await?;
            provisioned += 1;
        } else {
            skipped += 1;
        }
    }
    ui::field("provisioned", &provisioned.to_string());
    ui::field("skipped", &format!("{skipped} (no registered pubkey)"));
    ui::success(&format!("reconciled scope keys for env '{}'", ctx.env_name));
    Ok(())
}

/// Recover the scope secret from the admin's OWN wrap: fetch it, deserialize the grant, pin
/// the granter (the admin's own Ed25519 key), and two-hop unwrap with the admin's X25519 key.
async fn recover_scope_secret(
    session: &mut Session,
    ctx: &Ctx,
    scope_id: [u8; 16],
) -> anyhow::Result<Zeroizing<[u8; 32]>> {
    let epoch = ctx.scope_epoch.max(1);
    let (blob, granter) = session
        .get_scope_key(&hex::encode(scope_id), epoch)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no scope key for this env — run `vault env-init` first"))?;
    let grant = GrantedScopeKey::from_bytes(&blob)
        .map_err(|_| anyhow::anyhow!("stored scope-key grant is malformed"))?;
    let granter_pub = granter_key(&granter)?;
    let member_secret = session.identity.encryption_secret();
    open_scope_key(&grant, member_secret, &granter_pub).map_err(|_| {
        anyhow::anyhow!("could not open the scope key — are you the bootstrapping admin?")
    })
}

/// Group `(grant_id, user_id)` pending pairs into `(user_id, [grant_id…])`, preserving first
/// occurrence — so one vault42 wrap per user is recorded against each of that user's grants.
fn group_by_user(pairs: Vec<(String, String)>) -> Vec<(String, Vec<String>)> {
    let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
    for (grant_id, user) in pairs {
        match grouped.iter_mut().find(|(u, _)| *u == user) {
            Some((_, ids)) => ids.push(grant_id),
            None => grouped.push((user, vec![grant_id])),
        }
    }
    grouped
}

/// Rebuild the granter's Ed25519 verifying key from the 32 raw bytes the wire returned,
/// rejecting a wrong length or a non-canonical encoding.
fn granter_key(bytes: &[u8]) -> anyhow::Result<AuthorPublicKey> {
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("granter pubkey must be 32 bytes"))?;
    AuthorPublicKey::from_bytes(&arr)
        .map_err(|_| anyhow::anyhow!("granter pubkey is not a valid key"))
}
