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

//! `42ctl project` — create and list projects, and grant a USER a project role (optionally
//! scoped to an environment). Authenticates with the grobase session token from
//! `auth login --github` or `auth login --password`.

use crate::adapters::rbac::{org, project};
use crate::adapters::session;
use crate::cli::Project;
use crate::ui;

/// Dispatch a `project` subcommand for `profile`.
pub async fn run(cmd: &Project, profile: &str) -> anyhow::Result<()> {
    let (grobase, token) = session::connect(profile)?;
    match cmd {
        Project::Create { org, slug, name } => create(&grobase, &token, org, slug, name).await,
        Project::List { org } => list(&grobase, &token, org).await,
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

/// Create a project under an org and print its id.
async fn create(
    grobase: &str,
    token: &str,
    org: &str,
    slug: &str,
    name: &str,
) -> anyhow::Result<()> {
    let p = project::create(grobase, token, org, slug, name).await?;
    ui::field("id", &p.id);
    ui::success(&format!(
        "created project '{}' ({}) in org '{org}'",
        p.name, p.slug
    ));
    Ok(())
}

/// List an org's projects as an `id slug name` table.
async fn list(grobase: &str, token: &str, org: &str) -> anyhow::Result<()> {
    let projects = project::list(grobase, token, org).await?;
    let rows: Vec<Vec<String>> = projects
        .iter()
        .map(|p| vec![p.id.clone(), p.slug.clone(), p.name.clone()])
        .collect();
    ui::table(&["id", "slug", "name"], &rows);
    Ok(())
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
