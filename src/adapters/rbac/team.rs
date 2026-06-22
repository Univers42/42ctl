/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   team.rs                                             :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Team-scoped RBAC calls within an org: create/list teams, add a member, invite by email,
//! and grant a team a project role. All authenticate with the grobase session JWT.

use crate::adapters::rbac::{self, GrantRequest, IssuedInvite, Team};
use serde_json::json;

/// Create a team `slug`/`name` under `org` → the created `Team`
/// (`POST /v1/orgs/{org}/teams`).
pub async fn create(
    grobase: &str,
    token: &str,
    org: &str,
    slug: &str,
    name: &str,
) -> anyhow::Result<Team> {
    let path = format!("/v1/orgs/{org}/teams");
    let body = json!({ "slug": slug, "name": name });
    rbac::post_json(grobase, token, &path, &body).await
}

/// List `org`'s teams (`GET /v1/orgs/{org}/teams`).
pub async fn list(grobase: &str, token: &str, org: &str) -> anyhow::Result<Vec<Team>> {
    let path = format!("/v1/orgs/{org}/teams");
    rbac::get_json(grobase, token, &path).await
}

/// Add `user` to `team` (within `org`) with `role` (`POST .../teams/{team}/members`).
pub async fn add_member(
    grobase: &str,
    token: &str,
    ids: (&str, &str),
    user: &str,
    role: &str,
) -> anyhow::Result<()> {
    let (org, team) = ids;
    let path = format!("/v1/orgs/{org}/teams/{team}/members");
    let body = json!({ "user_id": user, "team_role": role });
    rbac::post_unit(grobase, token, &path, &body).await
}

/// Invite `email` to `team` with `role` → the issued invite + one-time token
/// (`POST .../teams/{team}/invites`).
pub async fn invite(
    grobase: &str,
    token: &str,
    ids: (&str, &str),
    email: &str,
    role: &str,
) -> anyhow::Result<IssuedInvite> {
    let (org, team) = ids;
    let path = format!("/v1/orgs/{org}/teams/{team}/invites");
    let body = json!({ "email": email, "role": role });
    rbac::post_json(grobase, token, &path, &body).await
}

/// Grant `team` a `project_role` on `project` (optionally scoped to `env`)
/// (`POST /v1/orgs/{org}/projects/{project}/grants`).
pub async fn grant_project(
    grobase: &str,
    token: &str,
    ids: (&str, &str, &str),
    project_role: &str,
    env: Option<&str>,
) -> anyhow::Result<rbac::Grant> {
    let (org, project, team) = ids;
    let path = format!("/v1/orgs/{org}/projects/{project}/grants");
    let body = GrantRequest {
        grantee_kind: "team".to_string(),
        grantee_id: team.to_string(),
        project_role: project_role.to_string(),
        env_id: env.map(str::to_string),
    };
    rbac::post_json(grobase, token, &path, &body).await
}
