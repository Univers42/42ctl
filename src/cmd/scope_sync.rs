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
use crate::cmd::scope_recover::recover_scope_secret;
use crate::cmd::scope_wrap::{self, ScopeRef};
use crate::ui;

/// Reconcile every authorized member's wrap for the env scope. Prints how many members were
/// newly provisioned and how many were skipped (pending-enrollment).
pub async fn sync_keys(session: &mut Session, ctx: &Ctx) -> anyhow::Result<()> {
    let scope_id = crypto::scope_id(&ctx.project, &ctx.env_name)?;
    let advertised = ctx.scope_pubkey.as_deref();
    let secret = recover_scope_secret(session, scope_id, ctx.epoch(), advertised).await?;
    let sref = ScopeRef {
        secret: &secret,
        id: scope_id,
        epoch: ctx.epoch(),
    };
    let (mut provisioned, mut skipped) = (0usize, 0usize);
    for (user, grant_ids) in orch::env_members(ctx).await?.pending {
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
