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

//! `env keys ls` — a glanceable table of each member's scope-key state.
//!
//! Every row is a member the environment's grants authorize, keyed by ACCOUNT ID so the column
//! composes with every other `--user` flag. The authority reports two sets per grant: `members`,
//! everyone authorized, and `missing`, those still without a recorded wrap. `missing` members
//! are `pending-provision` when they have a registered pubkey (awaiting `env keys sync`) and
//! `pending-enrollment` when they do not; everyone else is `active`.
//!
//! The active rows used to come from vault42's `list_scope_members`, which is owner-scoped on
//! purpose — it returns the CALLER's own wrap and never the cross-member set. So the table showed
//! pending members and the admin alone, and every member already provisioned vanished from it:
//! exactly the people an admin runs this to see. The caller still gets a row of their own when
//! the server holds their wrap and no grant lists them, as the creator of the key does.
//! Read-only: it never wraps, deposits, or records.

use crate::adapters::api::Session;
use crate::adapters::rbac::{account, pubkey};
use crate::adapters::scope as crypto;
use crate::cmd::scope::{self as orch, Ctx, EnvMembers};
use crate::ui;

/// Print the env's scope-key status table.
pub async fn scope_status(
    session: &mut Session,
    ctx: &Ctx,
    out: &crate::cli::Output,
) -> anyhow::Result<()> {
    let members = orch::env_members(ctx).await?;
    let mut rows: Vec<serde_json::Value> = Vec::new();
    for member in &members.pending {
        rows.push(pending_row(ctx, &member.user).await?);
    }
    for user in active(&members) {
        rows.push(status_row(user, true, true, "active"));
    }
    if let Some(own) = own_row_if_unlisted(session, ctx, &members).await? {
        rows.push(own);
    }
    ui::render(
        &["Member", "Pubkey", "Provisioned", "State"],
        rows,
        out.shape(),
    )
}

/// The authorized members that are not missing a wrap.
fn active(members: &EnvMembers) -> impl Iterator<Item = &str> {
    members
        .authorized
        .iter()
        .map(|member| member.user.as_str())
        .filter(|user| !members.pending.iter().any(|pending| pending.user == *user))
}

/// The caller's own row, when the server holds their wrap and no grant already lists them.
async fn own_row_if_unlisted(
    session: &mut Session,
    ctx: &Ctx,
    members: &EnvMembers,
) -> anyhow::Result<Option<serde_json::Value>> {
    let scope_id = hex::encode(crypto::scope_id(&ctx.project, &ctx.env_name)?);
    if session
        .list_scope_members(&scope_id, ctx.epoch())
        .await?
        .is_empty()
    {
        return Ok(None);
    }
    let me = account::me(&ctx.grobase, &ctx.token).await?.account_id;
    let listed = members.authorized.iter().any(|member| member.user == me);
    Ok((!listed).then(|| status_row(&me, true, true, "active")))
}

/// One status row. Booleans stay booleans so `--filter Provisioned=false` reads naturally.
fn status_row(member: &str, pubkey: bool, provisioned: bool, state: &str) -> serde_json::Value {
    serde_json::json!({
        "Member": member, "Pubkey": pubkey, "Provisioned": provisioned, "State": state,
    })
}

/// Build one pending member's row: `pending-provision` when they have a registered pubkey,
/// `pending-enrollment` when they do not (so an admin sees who must run `keys init`/register).
async fn pending_row(ctx: &Ctx, user: &str) -> anyhow::Result<serde_json::Value> {
    let registered = pubkey::get(&ctx.grobase, &ctx.token, &ctx.org, user)
        .await
        .is_ok();
    let state = if registered {
        "pending-provision"
    } else {
        "pending-enrollment"
    };
    Ok(status_row(user, registered, false, state))
}
