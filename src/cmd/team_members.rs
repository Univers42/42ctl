/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   team_members.rs                                     :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl team {add-member,invite,grant-project}` — the team membership + grant verbs, split
//! out of `team.rs` to keep each file within the 42 norm. Invites print the one-time token.

use crate::adapters::rbac::team;
use crate::cli::Team;
use crate::ui;

/// Run a team membership/grant verb already resolved to its grobase base + session token.
pub async fn run(cmd: &Team, grobase: &str, token: &str) -> anyhow::Result<()> {
    match cmd {
        Team::AddMember {
            org,
            team,
            user,
            role,
        } => add_member(grobase, token, (org, team), (user, role)).await,
        Team::Invite {
            org,
            team,
            email,
            role,
        } => invite(grobase, token, (org, team), (email, role)).await,
        Team::GrantProject {
            org,
            team,
            project,
            role,
            env,
        } => grant(grobase, token, (org, team, project), (role, env.as_deref())).await,
        _ => unreachable!("create/list are handled in team::run"),
    }
}

/// Add a user to a team with a role.
async fn add_member(
    grobase: &str,
    token: &str,
    ids: (&str, &str),
    member: (&str, &str),
) -> anyhow::Result<()> {
    let (user, role) = member;
    team::add_member(grobase, token, ids, user, role).await?;
    ui::success(&format!("added {user} to team '{}' as '{role}'", ids.1));
    Ok(())
}

/// Invite an email to a team and print the one-time token.
async fn invite(
    grobase: &str,
    token: &str,
    ids: (&str, &str),
    spec: (&str, &str),
) -> anyhow::Result<()> {
    let (email, role) = spec;
    let inv = team::invite(grobase, token, ids, email, role).await?;
    ui::field("invite_id", &inv.id);
    ui::field("token", &inv.token);
    ui::success(&format!("invited {email} to team '{}' as '{role}'", ids.1));
    Ok(())
}

/// Grant a team a project role, optionally scoped to an environment.
async fn grant(
    grobase: &str,
    token: &str,
    ids: (&str, &str, &str),
    spec: (&str, Option<&str>),
) -> anyhow::Result<()> {
    let (org, team_id, project) = ids;
    let (role, env) = spec;
    let g = team::grant_project(grobase, token, (org, project, team_id), role, env).await?;
    ui::field("grant_id", &g.id);
    ui::success(&format!(
        "granted team '{team_id}' '{role}' on project '{project}'"
    ));
    Ok(())
}
