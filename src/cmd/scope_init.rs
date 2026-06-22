/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope_init.rs                                       :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `vault env-init` — the admin bootstrap for an environment scope. Generate the scope
//! keyset (epoch 1), publish its PUBLIC key to grobase (so members seal secrets to it),
//! register the admin's own pubkey, and self-wrap the scope SECRET to the admin (so a later
//! `sync-keys` can recover it). The scope secret never leaves a `Zeroizing` buffer and is
//! never persisted in cleartext — only the AEAD-wrapped grant is deposited at vault42.

use crate::adapters::api::Session;
use crate::adapters::rbac::{pubkey, ScopeKeyRequest};
use crate::adapters::scope;
use crate::adapters::scope_grpc::ScopeDeposit;
use crate::cmd::scope::Ctx;
use crate::cmd::scope_pubkey;
use crate::ui;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use vault42_core::{generate_keyset, grant_scope_key, ScopeKeyset};
use zeroize::Zeroizing;

/// Bootstrap the env scope at epoch 1: derive the scope id, generate the keyset, publish
/// its public key, register self, self-wrap the secret, and print the scope id + epoch.
/// Refuses to clobber an already-bootstrapped env (a fresh keyset would orphan every
/// member's wrap); rotation is a future, distinct verb.
pub async fn env_init(session: &mut Session, ctx: &Ctx) -> anyhow::Result<()> {
    if ctx.scope_pubkey.as_deref().is_some_and(|k| !k.is_empty()) {
        anyhow::bail!(
            "env '{}' already has a scope key (epoch {}) — re-init would orphan member wraps",
            ctx.env_name,
            ctx.scope_epoch
        );
    }
    let scope_id = scope::scope_id(&ctx.project, &ctx.env_name)?;
    let (keyset, scope_secret) = generate_keyset(scope_id, 1);
    publish_keyset(ctx, &keyset).await?;
    scope_pubkey::register_self(&ctx.grobase, &ctx.token, &ctx.org, &session.identity).await?;
    self_wrap(session, &scope_secret, scope_id).await?;
    ui::field("scope_id", &hex::encode(scope_id));
    ui::field("epoch", "1");
    ui::success(&format!(
        "bootstrapped scope for env '{}' (self-wrap deposited; secret never persisted)",
        ctx.env_name
    ));
    Ok(())
}

/// Publish the scope PUBLIC key (base64) + epoch 1 to grobase
/// (`PUT /v1/projects/{proj}/environments/{env}/scopekey`).
async fn publish_keyset(ctx: &Ctx, keyset: &ScopeKeyset) -> anyhow::Result<()> {
    let req = ScopeKeyRequest {
        scope_pubkey: STANDARD.encode(keyset.public.to_bytes()),
        scope_epoch: 1,
    };
    pubkey::put_scopekey(&ctx.grobase, &ctx.token, (&ctx.project, &ctx.env_id), &req).await?;
    Ok(())
}

/// Wrap the scope secret to the admin's OWN X25519 key and deposit it under the admin's
/// principal, so `sync-keys` can later recover the scope secret to reconcile members.
async fn self_wrap(
    session: &mut Session,
    scope_secret: &Zeroizing<[u8; 32]>,
    scope_id: [u8; 16],
) -> anyhow::Result<()> {
    let member_pub = session.identity.encryption_public();
    let grant = grant_scope_key(
        scope_secret,
        &member_pub,
        session.identity.signing_key(),
        scope_id,
        1,
    )?;
    let principal = session.principal.clone();
    let granter_pubkey = session.identity.author_public().to_bytes().to_vec();
    session
        .wrap_scope_key(ScopeDeposit {
            member_id: &principal,
            scope_id: &hex::encode(scope_id),
            epoch: 1,
            granted_blob: grant.to_bytes()?,
            granter_pubkey,
        })
        .await
}
