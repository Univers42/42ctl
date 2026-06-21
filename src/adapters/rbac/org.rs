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

//! Org-scoped RBAC calls: create an org, list its members, issue/accept org invites, and
//! grant a USER a project role. Each carries the grobase session JWT and RBAC-checks against
//! the org-system routes.

use crate::adapters::rbac::{self, GrantRequest, IssuedInvite, Member, Org};
use serde_json::json;

/// Create an org from `slug` + `name` → the created `Org` (`POST /v1/orgs`).
pub async fn create(grobase: &str, token: &str, slug: &str, name: &str) -> anyhow::Result<Org> {
    let body = json!({ "slug": slug, "name": name });
    rbac::post_json(grobase, token, "/v1/orgs", &body).await
}

/// List `org`'s members (`GET /v1/orgs/{org}/members`).
pub async fn members(grobase: &str, token: &str, org: &str) -> anyhow::Result<Vec<Member>> {
    let path = format!("/v1/orgs/{org}/members");
    rbac::get_json(grobase, token, &path).await
}

/// Invite `email` to `org` with `role` → the issued invite incl. its one-time token
/// (`POST /v1/orgs/{org}/invites`).
pub async fn invite(
    grobase: &str,
    token: &str,
    org: &str,
    email: &str,
    role: &str,
) -> anyhow::Result<IssuedInvite> {
    let path = format!("/v1/orgs/{org}/invites");
    let body = json!({ "email": email, "role": role });
    rbac::post_json(grobase, token, &path, &body).await
}

/// Accept an org invite by its one-time `invite_token` (`POST /v1/orgs/invites/accept`).
pub async fn accept_invite(grobase: &str, token: &str, invite_token: &str) -> anyhow::Result<()> {
    let body = json!({ "token": invite_token });
    rbac::post_unit(grobase, token, "/v1/orgs/invites/accept", &body).await
}

/// Grant `user` a `project_role` on `project` (optionally scoped to `env`)
/// (`POST /v1/orgs/{org}/projects/{project}/grants`).
pub async fn grant_user(
    grobase: &str,
    token: &str,
    ids: (&str, &str, &str),
    project_role: &str,
    env: Option<&str>,
) -> anyhow::Result<rbac::Grant> {
    let (org, project, user) = ids;
    let path = format!("/v1/orgs/{org}/projects/{project}/grants");
    let body = GrantRequest {
        grantee_kind: "user".to_string(),
        grantee_id: user.to_string(),
        project_role: project_role.to_string(),
        env_id: env.map(str::to_string),
    };
    rbac::post_json(grobase, token, &path, &body).await
}
