/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   org.rs                                              :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl org` — org-scoped RBAC (create / member ls|rm / invite / accept-invite). Each call
//! sends the session token (from `auth login`) to the authority, which RBAC-checks it. Invites
//! print the one-time cleartext token.

use crate::adapters::rbac::org;
use crate::adapters::session;
use crate::cli::{Org, OrgMember};
use crate::cmd::bulk;
use crate::ui;

/// Run an `org` subcommand for `profile` against the authority, with the saved session token.
pub async fn run(cmd: &Org, profile: &str) -> anyhow::Result<()> {
    let (grobase, token) = session::connect(profile)?;
    match cmd {
        Org::Create { slug, name } => {
            let o = org::create(&grobase, &token, slug, name).await?;
            ui::field("id", &o.id);
            ui::success(&format!("created org '{}' ({})", o.name, o.slug));
        }
        Org::Member(OrgMember::Ls { org, out }) => members(&grobase, &token, org, out).await?,
        Org::Invite { org, email, role } => {
            let inv = org::invite(&grobase, &token, org, email, role).await?;
            ui::field("invite_id", &inv.id);
            ui::field("token", &inv.token);
            ui::success(&format!("invited {email} to org '{org}' as '{role}'"));
        }
        Org::Member(OrgMember::Rm { org, user }) => {
            remove_member(&grobase, &token, org, user).await?
        }
        Org::AcceptInvite { token: invite } => {
            org::accept_invite(&grobase, &token, invite).await?;
            ui::success("accepted org invite");
        }
    }
    Ok(())
}

/// Print an org's members: `UserID Role Joined`.
async fn members(
    grobase: &str,
    token: &str,
    org_id: &str,
    out: &crate::cli::Output,
) -> anyhow::Result<()> {
    let rows = org::members(grobase, token, org_id)
        .await?
        .iter()
        .map(|m| {
            serde_json::json!({
                "UserID": m.user_id, "Role": m.role, "Joined": ui::reltime(m.created_at),
            })
        })
        .collect();
    ui::render(&["UserID", "Role", "Joined"], rows, out.shape())
}

/// Remove a member from an org and report what the removal did NOT do.
///
/// The authority answers with `rotate_required` and a sentence explaining it, and this prints
/// that sentence rather than swallowing it. An operator who reads "removed" as "locked out"
/// has been misled at the worst possible moment: removal stops the person being RE-wrapped, it
/// cannot reach into their machine and take back a scope key they already hold.
async fn remove_member(
    grobase: &str,
    token: &str,
    org: &str,
    users: &[String],
) -> anyhow::Result<()> {
    let attempt = bulk::targets(users);
    let mut failed = Vec::new();
    for user in &attempt {
        if let Err(error) = remove_one(grobase, token, org, user).await {
            bulk::failure(user, &error);
            failed.push(user.clone());
        }
    }
    bulk::report(&failed, attempt.len())
}

/// Remove one member from `org` and report what their removal left reachable.
async fn remove_one(grobase: &str, token: &str, org: &str, user: &str) -> anyhow::Result<()> {
    let removed = org::remove_member(grobase, token, org, user).await?;
    ui::success(&format!("removed {user} from org '{org}'"));
    warn_rotation(&removed);
    Ok(())
}

/// Print the authority's own account of what a removal left reachable.
pub fn warn_rotation(removed: &crate::adapters::rbac::Removed) {
    if !removed.rotate_required || removed.detail.is_empty() {
        return;
    }
    println!("{}", ui::warn(&removed.detail));
}
