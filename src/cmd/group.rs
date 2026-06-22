/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   group.rs                                            :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl group` — project group operations: create a project's group (the server derives
//! its name), add members, invite by email (prints the one-time token). Authenticates with
//! the grobase session token from `auth login --github`.

use crate::adapters::rbac::group;
use crate::adapters::session;
use crate::cli::Group;
use crate::ui;

/// Dispatch a `group` subcommand for `profile`.
pub async fn run(cmd: &Group, profile: &str) -> anyhow::Result<()> {
    let (grobase, token) = session::connect(profile)?;
    match cmd {
        Group::Create { project } => create(&grobase, &token, project).await,
        Group::AddMember { group, user } => add_member(&grobase, &token, group, user).await,
        Group::Invite { group, email } => invite(&grobase, &token, group, email).await,
    }
}

/// Create a project's group and print its id.
async fn create(grobase: &str, token: &str, project: &str) -> anyhow::Result<()> {
    let g = group::create(grobase, token, project).await?;
    ui::field("id", &g.id);
    ui::success(&format!(
        "created group '{}' for project '{project}'",
        g.name
    ));
    Ok(())
}

/// Add a user to a group.
async fn add_member(grobase: &str, token: &str, group_id: &str, user: &str) -> anyhow::Result<()> {
    group::add_member(grobase, token, group_id, user).await?;
    ui::success(&format!("added {user} to group '{group_id}'"));
    Ok(())
}

/// Invite an email to a group and print the one-time token.
async fn invite(grobase: &str, token: &str, group_id: &str, email: &str) -> anyhow::Result<()> {
    let inv = group::invite(grobase, token, group_id, email).await?;
    ui::field("invite_id", &inv.id);
    ui::field("token", &inv.token);
    ui::success(&format!("invited {email} to group '{group_id}'"));
    Ok(())
}
