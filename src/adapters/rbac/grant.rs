/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   grant.rs                                            :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Grant-fulfilment REST calls — the control-plane half of the scope-key bridge: who is
//! authorized (`grants`), who is provisioned for one environment at one epoch (`fulfilled`),
//! and recording a wrap once vault42 has stored it (`record_wrap`). All carry the grobase JWT.
//!
//! Every call is addressed by a `GrantScope`, which pins the environment and epoch. A wrap
//! only ever means "this member holds THIS environment's key at THIS epoch": a project-wide
//! grant spans several environments and a rotation replaces the key, so an answer that omits
//! either coordinate reports members as provisioned in scopes they cannot read.

use crate::adapters::rbac::{self, Fulfilled, GrantScope, ProjectGrant};
use serde_json::json;

/// List a project's live grants (`GET /v1/orgs/{org}/projects/{proj}/grants`).
pub async fn list(scope: &GrantScope<'_>) -> anyhow::Result<Vec<ProjectGrant>> {
    let path = format!("/v1/orgs/{}/projects/{}/grants", scope.org, scope.project);
    rbac::get_json(scope.grobase, scope.token, &path).await
}

/// Report the grant's authorized members and which of them still lack a wrap for the scope's
/// environment and epoch (`GET /v1/orgs/{org}/projects/{proj}/grants/{grantId}/fulfilled`).
pub async fn fulfilled(scope: &GrantScope<'_>, grant_id: &str) -> anyhow::Result<Fulfilled> {
    let path = format!(
        "/v1/orgs/{}/projects/{}/grants/{grant_id}/fulfilled?env_id={}&epoch={}",
        scope.org, scope.project, scope.env_id, scope.epoch
    );
    rbac::get_json(scope.grobase, scope.token, &path).await
}

/// Record that `user` now holds a wrap for `grant_id` at the scope's environment and epoch
/// (`POST /v1/orgs/{org}/projects/{proj}/grants/{grantId}/wraps`).
pub async fn record_wrap(scope: &GrantScope<'_>, grant_id: &str, user: &str) -> anyhow::Result<()> {
    let path = format!(
        "/v1/orgs/{}/projects/{}/grants/{grant_id}/wraps",
        scope.org, scope.project
    );
    let body = json!({ "user_id": user, "env_id": scope.env_id, "epoch": scope.epoch });
    rbac::post_unit(scope.grobase, scope.token, &path, &body).await
}

/// Revoke `grant_id` on a project
/// (`DELETE /v1/orgs/{org}/projects/{project}/grants/{grant}`).
///
/// Soft on the server: the row stays with `revoked_at` set, because "who used to be able to
/// read this" outlives the grant. Every read filters it out, so it authorizes nobody from now
/// on and rotation passes it by.
pub async fn revoke(
    grobase: &str,
    token: &str,
    ids: (&str, &str),
    grant_id: &str,
) -> anyhow::Result<rbac::Removed> {
    let (org, project) = ids;
    let path = format!("/v1/orgs/{org}/projects/{project}/grants/{grant_id}");
    rbac::delete_json(grobase, token, &path).await
}
