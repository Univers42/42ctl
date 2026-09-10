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
//! keyset, publish its PUBLIC key to grobase (so members seal secrets to it),
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
use vault42_core::{generate_keyset, grant_scope_key, GrantTerms, ScopeKeyset, ScopeRole};
use zeroize::Zeroizing;

/// Bootstrap the env scope: derive the scope id, generate the keyset, publish its public
/// key, register self, self-wrap the secret, and print the scope id + epoch. Normally epoch
/// 1; `resume_epoch` decides, and refuses to clobber an env whose wraps a fresh keyset would
/// orphan. Rotating a LIVE scope is `rotate-scope`, a distinct verb.
pub async fn env_init(session: &mut Session, ctx: &Ctx) -> anyhow::Result<()> {
    let scope_id = scope::scope_id(&ctx.project, &ctx.env_name)?;
    let epoch = resume_epoch(session, ctx, scope_id).await?;
    let (keyset, scope_secret) = generate_keyset(scope_id, epoch);
    publish_keyset(ctx, &keyset, epoch).await?;
    scope_pubkey::register_self(&ctx.grobase, &ctx.token, &ctx.org, &session.identity).await?;
    self_wrap(session, &scope_secret, scope_id, epoch).await?;
    ui::field("scope_id", &hex::encode(scope_id));
    ui::field("epoch", &epoch.to_string());
    ui::success(&format!(
        "bootstrapped scope for env '{}' (self-wrap deposited; secret never persisted)",
        ctx.env_name
    ));
    Ok(())
}

/// The epoch to bootstrap at, refusing only when a real bootstrap already exists.
///
/// The guard protects member wraps, and wraps live in VAULT42, so that is where it has to
/// look. Asking the authority's advertised public key instead conflated "bootstrapped" with
/// "half-bootstrapped": `env_init` publishes the public key over REST and deposits the
/// self-wrap over gRPC, and anything failing between the two — an expired contract is enough —
/// left the environment advertising a key nobody ever held. Every route out was then closed:
/// `env-init` refused because a key existed, while `sync-keys` and `rotate-scope` both refused
/// because none did.
///
/// So when vault42 holds no wrap for the scope there is nothing to orphan and the interrupted
/// bootstrap is completed rather than refused. It resumes at the NEXT epoch because the
/// authority refuses a `scope_epoch` that does not advance, and asks about members at the
/// advertised epoch because that is the one any stranded wrap would sit at.
async fn resume_epoch(session: &mut Session, ctx: &Ctx, scope_id: [u8; 16]) -> anyhow::Result<u32> {
    if ctx.scope_pubkey.as_deref().is_none_or(str::is_empty) {
        return Ok(1);
    }
    let members = session
        .list_scope_members(&hex::encode(scope_id), ctx.epoch())
        .await?;
    if !members.is_empty() {
        anyhow::bail!(
            "env '{}' already has a scope key (epoch {}) with {} provisioned member(s) — \
             re-init would orphan their wraps; use `vault rotate-scope` instead",
            ctx.env_name,
            ctx.epoch(),
            members.len()
        );
    }
    println!(
        "{}",
        ui::warn(&format!(
            "env '{}' advertises a scope key at epoch {} that vault42 never received — \
             completing the interrupted bootstrap at epoch {}",
            ctx.env_name,
            ctx.epoch(),
            ctx.epoch() + 1
        ))
    );
    Ok(ctx.epoch() + 1)
}

/// Publish the scope PUBLIC key (base64) + its epoch to grobase
/// (`PUT /v1/projects/{proj}/environments/{env}/scopekey`).
async fn publish_keyset(ctx: &Ctx, keyset: &ScopeKeyset, epoch: u32) -> anyhow::Result<()> {
    let req = ScopeKeyRequest {
        scope_pubkey: STANDARD.encode(keyset.public.to_bytes()),
        scope_epoch: epoch,
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
    epoch: u32,
) -> anyhow::Result<()> {
    let member_pub = session.identity.encryption_public();
    // The admin creating the scope is a writer by construction: they hold the secret because
    // they generated it, so a Reader wrap here would only lock them out of what they made.
    let grant = grant_scope_key(
        scope_secret,
        &member_pub,
        session.identity.signing_key(),
        GrantTerms {
            scope_id,
            epoch,
            role: ScopeRole::Writer,
        },
    )?;
    let principal = session.principal.clone();
    let granter_pubkey = session.identity.author_public().to_bytes().to_vec();
    session
        .wrap_scope_key(ScopeDeposit {
            member_id: &principal,
            scope_id: &hex::encode(scope_id),
            epoch,
            granted_blob: grant.to_bytes()?,
            granter_pubkey,
        })
        .await
}
