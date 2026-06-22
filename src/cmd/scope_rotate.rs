/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope_rotate.rs                                     :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `vault rotate-scope` — forward-secure scope rotation. Recover the CURRENT scope secret,
//! generate a fresh keyset at `epoch+1`, re-seal every env secret to the new scope public key
//! (opened with the old secret, sealed to the new), re-wrap the new scope key to the REMAINING
//! authorized members (grobase effective minus revoked — a removed member gets no new-epoch
//! wrap, so it loses access by absence), and publish the new public key/epoch to grobase.

use crate::adapters::api::Session;
use crate::adapters::rbac::{pubkey, ScopeKeyRequest};
use crate::adapters::scope as crypto;
use crate::cmd::scope::Ctx;
use crate::cmd::scope_recover::recover_scope_secret;
use crate::cmd::scope_secret_reseal::{self, RotateState};
use crate::ui;
use vault42_core::generate_keyset;

/// Rotate the env scope one epoch forward: re-seal all secrets, re-wrap to remaining members,
/// publish the new public key. Prints the new epoch and the re-seal / re-wrap counts.
pub async fn rotate_scope(session: &mut Session, ctx: &Ctx) -> anyhow::Result<()> {
    let scope_id = crypto::scope_id(&ctx.project, &ctx.env_name)?;
    let old_epoch = ctx.scope_epoch.max(1);
    let new_epoch = old_epoch + 1;
    let old_secret = recover_scope_secret(session, scope_id, old_epoch).await?;
    let (keyset, new_secret) = generate_keyset(scope_id, new_epoch);
    let state = RotateState {
        scope_id,
        old_epoch,
        new_epoch,
        old_secret: &old_secret,
        new_secret: &new_secret,
        keyset: &keyset,
    };
    let resealed = scope_secret_reseal::reseal_all(session, &state).await?;
    let rewrapped = scope_secret_reseal::rewrap_remaining(session, ctx, &state).await?;
    publish(ctx, keyset.public.to_bytes(), new_epoch).await?;
    report(new_epoch, resealed, rewrapped);
    Ok(())
}

/// Publish the rotated scope PUBLIC key (base64) + new epoch to grobase
/// (`PUT /v1/projects/{proj}/environments/{env}/scopekey`).
async fn publish(ctx: &Ctx, public: [u8; 32], new_epoch: u32) -> anyhow::Result<()> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    let req = ScopeKeyRequest {
        scope_pubkey: STANDARD.encode(public),
        scope_epoch: new_epoch,
    };
    pubkey::put_scopekey(&ctx.grobase, &ctx.token, (&ctx.project, &ctx.env_id), &req).await?;
    Ok(())
}

/// Print the rotation summary: new epoch, secrets re-sealed, members re-wrapped.
fn report(new_epoch: u32, resealed: usize, rewrapped: usize) {
    ui::field("new_epoch", &new_epoch.to_string());
    ui::field("resealed", &resealed.to_string());
    ui::field("rewrapped", &rewrapped.to_string());
    ui::success("rotated scope (revoked members lose access by absence at the new epoch)");
}
