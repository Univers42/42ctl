/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   project.rs                                          :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl project grant` — grant a USER a project role (optionally scoped to an environment).
//! Authenticates with the grobase session token from `auth login --github`.

use crate::adapters::rbac::org;
use crate::adapters::session;
use crate::cli::Project;
use crate::ui;

/// Dispatch a `project` subcommand for `profile`.
pub async fn run(cmd: &Project, profile: &str) -> anyhow::Result<()> {
    let (grobase, token) = session::connect(profile)?;
    match cmd {
        Project::Grant {
            org: org_id,
            project,
            user,
            role,
            env,
        } => {
            grant(
                &grobase,
                &token,
                (org_id, project, user),
                (role, env.as_deref()),
            )
            .await
        }
    }
}

/// Grant a user a project role and print the grant id.
async fn grant(
    grobase: &str,
    token: &str,
    ids: (&str, &str, &str),
    spec: (&str, Option<&str>),
) -> anyhow::Result<()> {
    let (org_id, project, user) = ids;
    let (role, env) = spec;
    let g = org::grant_user(grobase, token, (org_id, project, user), role, env).await?;
    ui::field("grant_id", &g.id);
    ui::success(&format!("granted {user} '{role}' on project '{project}'"));
    Ok(())
}
