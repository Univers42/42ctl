/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope_wrap.rs                                       :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Provision one member: fetch their registered pubkey, verify proof-of-possession, wrap
//! the scope secret to their X25519 key (signed by the reconciling admin), deposit the wrap
//! at vault42, and record it against each pending grant in grobase. A member with no/invalid
//! pubkey is skipped (pending-enrollment) — never wrapped to. Zero-knowledge: the scope
//! secret stays in a `Zeroizing` buffer; only the AEAD-wrapped grant leaves.

use crate::adapters::api::Session;
use crate::adapters::rbac::{grant, pubkey};
use crate::adapters::scope;
use crate::adapters::scope_grpc::ScopeDeposit;
use crate::cmd::scope::Ctx;
use crate::cmd::scope_pubkey;
use vault42_core::grant_scope_key;
use zeroize::Zeroizing;

/// The scope identity a wrap is bound to (keeps `provision` ≤4 args).
pub struct ScopeRef<'a> {
    pub secret: &'a Zeroizing<[u8; 32]>,
    pub id: [u8; 16],
    pub epoch: u32,
}

/// Provision `user`: fetch + verify their pubkey, then wrap the scope secret to it and
/// deposit it at vault42. Returns `false` (skipped) when the member has no registered or
/// no verifiable pubkey; `true` once the wrap is deposited.
pub async fn provision(
    session: &mut Session,
    ctx: &Ctx,
    sref: &ScopeRef<'_>,
    user: &str,
) -> anyhow::Result<bool> {
    let Some(pk) = fetch_pubkey(ctx, user).await? else {
        return Ok(false);
    };
    if !scope_pubkey::verify_member(&pk, &ctx.org) {
        return Ok(false);
    }
    deposit(session, sref, &pk).await?;
    Ok(true)
}

/// Wrap the scope secret to `pk`'s X25519 key (signed by the admin) and deposit the grant
/// at vault42 under the member's storage id.
async fn deposit(
    session: &mut Session,
    sref: &ScopeRef<'_>,
    pk: &crate::adapters::rbac::MemberPubkey,
) -> anyhow::Result<()> {
    let member_pub = scope::x25519_pub(&pk.x25519_pub)?;
    let granter = session.identity.signing_key();
    let g = grant_scope_key(sref.secret, &member_pub, granter, sref.id, sref.epoch)?;
    let granter_pubkey = session.identity.author_public().to_bytes().to_vec();
    session
        .wrap_scope_key(ScopeDeposit {
            member_id: &scope::member_id(&pk.ed25519_pub)?,
            scope_id: &hex::encode(sref.id),
            epoch: sref.epoch,
            granted_blob: g.to_bytes()?,
            granter_pubkey,
        })
        .await
}

/// Record a member's now-deposited wrap against every grant id in `grant_ids` (grobase
/// `POST .../grants/{grantId}/wraps`).
pub async fn record(ctx: &Ctx, user: &str, grant_ids: &[String]) -> anyhow::Result<()> {
    for grant_id in grant_ids {
        grant::record_wrap(
            &ctx.grobase,
            &ctx.token,
            (&ctx.org, &ctx.project),
            grant_id,
            user,
        )
        .await?;
    }
    Ok(())
}

/// Fetch a member's registered pubkey, returning `None` when none is registered (a 404 →
/// pending-enrollment) rather than an error.
async fn fetch_pubkey(
    ctx: &Ctx,
    user: &str,
) -> anyhow::Result<Option<crate::adapters::rbac::MemberPubkey>> {
    match pubkey::get(&ctx.grobase, &ctx.token, &ctx.org, user).await {
        Ok(pk) => Ok(Some(pk)),
        // ponytail: any fetch failure ⇒ pending-enrollment — distinguish 404 from a transport
        // error (a typed rbac error) if a flaky network must not be reported as "not enrolled".
        Err(_) => Ok(None),
    }
}
