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
        Project::List { org, out } => list(&grobase, &token, org, out).await,
        Project::Grants { org, project, out } => {
            grants(&grobase, &token, (org, project), out).await
        }
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

/// List an org's projects: `ID Slug Name`.
async fn list(
    grobase: &str,
    token: &str,
    org: &str,
    out: &crate::cli::Output,
) -> anyhow::Result<()> {
    let rows = project::list(grobase, token, org)
        .await?
        .iter()
        .map(|p| serde_json::json!({"ID": p.id, "Slug": p.slug, "Name": p.name}))
        .collect();
    ui::render(
        &["ID", "Slug", "Name"],
        rows,
        out.format.as_deref(),
        &out.filter,
    )
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
async fn grants(
    grobase: &str,
    token: &str,
    ids: (&str, &str),
    out: &crate::cli::Output,
) -> anyhow::Result<()> {
    let (org, project) = ids;
    let scope = GrantScope {
        grobase,
        token,
        org,
        project,
        env_id: "",
        epoch: 1,
    };
    let rows = grants_api::list(&scope)
        .await?
        .iter()
        .map(grant_row)
        .collect();
    ui::render(
        &["GrantID", "Role", "Env"],
        rows,
        out.format.as_deref(),
        &out.filter,
    )
}

/// One grant row. A grant with no environment reaches every one, which is what the whole
/// project's name in that column says.
fn grant_row(g: &crate::adapters::rbac::ProjectGrant) -> serde_json::Value {
    serde_json::json!({
        "GrantID": g.id, "Role": g.project_role,
        "Env": g.env_id.clone().unwrap_or_else(|| "(project-wide)".into()),
    })
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
