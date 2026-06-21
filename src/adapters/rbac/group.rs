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

//! Group-scoped RBAC calls: create a project's group (the server derives its name), add a
//! member by user id, and invite by email. All authenticate with the grobase session JWT.

use crate::adapters::rbac::{self, Group, IssuedInvite};
use serde_json::json;

/// Create `project`'s group (`POST /v1/projects/{project}/groups`; server names it).
pub async fn create(grobase: &str, token: &str, project: &str) -> anyhow::Result<Group> {
    let path = format!("/v1/projects/{project}/groups");
    rbac::post_json(grobase, token, &path, &json!({})).await
}

/// Add `user` to `group` (`POST /v1/groups/{group}/members`).
pub async fn add_member(grobase: &str, token: &str, group: &str, user: &str) -> anyhow::Result<()> {
    let path = format!("/v1/groups/{group}/members");
    let body = json!({ "user_id": user });
    rbac::post_unit(grobase, token, &path, &body).await
}

/// Invite `email` to `group` → the issued invite + one-time token
/// (`POST /v1/groups/{group}/invites`).
pub async fn invite(
    grobase: &str,
    token: &str,
    group: &str,
    email: &str,
) -> anyhow::Result<IssuedInvite> {
    let path = format!("/v1/groups/{group}/invites");
    let body = json!({ "email": email });
    rbac::post_json(grobase, token, &path, &body).await
}
