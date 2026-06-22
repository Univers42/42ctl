/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope_status.rs                                     :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `vault scope-status` — a glanceable table of each member's scope-key state. Members the
//! server still reports as `missing` a wrap are classified by whether they have a registered
//! pubkey: `pending-provision` (has one, awaiting `sync-keys`) vs `pending-enrollment` (none
//! yet). Members already wrapped (vault42 `list_scope_members`) show as `active`. Read-only:
//! it never wraps, deposits, or records — just reports what `sync-keys` would do.

use crate::adapters::address;
use crate::adapters::api::Session;
use crate::adapters::rbac::pubkey;
use crate::adapters::scope as crypto;
use crate::cmd::scope::{self as orch, Ctx};
use crate::ui;

/// Print the env's scope-key status table: a row per pending member (classified by pubkey
/// presence) and a row per already-provisioned (active) member.
pub async fn scope_status(session: &mut Session, ctx: &Ctx) -> anyhow::Result<()> {
    let scope_id = crypto::scope_id(&ctx.project, &ctx.env_name)?;
    let epoch = ctx.scope_epoch.max(1);
    let mut rows: Vec<Vec<String>> = Vec::new();
    for user in dedup_users(orch::env_pending(ctx).await?) {
        rows.push(pending_row(ctx, &user).await?);
    }
    for member in session
        .list_scope_members(&hex::encode(scope_id), epoch)
        .await?
    {
        let id = address::short(&member);
        rows.push(vec![id, "yes".into(), "yes".into(), "active".into()]);
    }
    ui::table(&["member", "pubkey", "provisioned", "state"], &rows);
    Ok(())
}

/// Build one pending member's row: `pending-provision` when they have a registered pubkey,
/// `pending-enrollment` when they do not (so an admin sees who must run `keys init`/register).
async fn pending_row(ctx: &Ctx, user: &str) -> anyhow::Result<Vec<String>> {
    let registered = pubkey::get(&ctx.grobase, &ctx.token, &ctx.org, user)
        .await
        .is_ok();
    let state = if registered {
        "pending-provision"
    } else {
        "pending-enrollment"
    };
    let has = if registered { "yes" } else { "no" };
    Ok(vec![
        address::short(user),
        has.into(),
        "no".into(),
        state.into(),
    ])
}

/// Dedup the pending `(grant_id, user)` pairs down to the distinct user ids, preserving order.
fn dedup_users(pairs: Vec<(String, String)>) -> Vec<String> {
    let mut users: Vec<String> = Vec::new();
    for (_, user) in pairs {
        if !users.contains(&user) {
            users.push(user);
        }
    }
    users
}
