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

use crate::adapters::rbac::{grant as grants_api, org, project, GrantScope};
use crate::adapters::session;
use crate::cli::Project;
use crate::ui;

/// Dispatch a `project` subcommand for `profile`.
pub async fn run(cmd: &Project, profile: &str) -> anyhow::Result<()> {
    let (grobase, token) = session::connect(profile)?;
    match cmd {
        Project::Create { org, slug, name } => create(&grobase, &token, org, slug, name).await,
        Project::List { org } => list(&grobase, &token, org).await,
        Project::Grants { org, project } => grants(&grobase, &token, org, project).await,
        Project::RevokeGrant {
            org,
            project,
            grant,
        } => revoke(&grobase, &token, (org, project), grant).await,
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

/// List a project's live grants as a `grant_id role env_id` table.
///
/// The ids are the point: `revoke-grant` needs one, and until this existed there was no way to
/// see a grant's id at all, so the revoke route was unreachable in practice even for somebody
/// who knew it was there.
async fn grants(grobase: &str, token: &str, org: &str, project: &str) -> anyhow::Result<()> {
    let scope = GrantScope {
        grobase,
        token,
        org,
        project,
        env_id: "",
        epoch: 1,
    };
    let live = grants_api::list(&scope).await?;
    let rows: Vec<Vec<String>> = live
        .iter()
        .map(|g| {
            vec![
                g.id.clone(),
                g.project_role.clone(),
                g.env_id.clone().unwrap_or_else(|| "(project-wide)".into()),
            ]
        })
        .collect();
    ui::table(&["grant_id", "role", "env"], &rows);
    Ok(())
}

/// Revoke a grant so it authorizes nobody from now on.
async fn revoke(
    grobase: &str,
    token: &str,
    ids: (&str, &str),
    grant_id: &str,
) -> anyhow::Result<()> {
    let (org, project) = ids;
    let removed = grants_api::revoke(grobase, token, (org, project), grant_id).await?;
    ui::success(&format!("revoked grant {grant_id} on project '{project}'"));
    crate::cmd::org::warn_rotation(&removed);
    Ok(())
}
