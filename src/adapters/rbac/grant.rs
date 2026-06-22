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
//! authorized (`grants`), which grants still lack a wrap (`fulfilled`), and recording a wrap
//! once vault42 has stored it (`record_wrap`). All carry the grobase JWT.

use crate::adapters::rbac::{self, Fulfilled, ProjectGrant};
use serde_json::json;

/// List a project's live grants (`GET /v1/orgs/{org}/projects/{proj}/grants`).
pub async fn list(
    grobase: &str,
    token: &str,
    org: &str,
    project: &str,
) -> anyhow::Result<Vec<ProjectGrant>> {
    let path = format!("/v1/orgs/{org}/projects/{project}/grants");
    rbac::get_json(grobase, token, &path).await
}

/// Report whether `grant_id`'s scope key is wrapped to every effective member
/// (`GET /v1/orgs/{org}/projects/{proj}/grants/{grantId}/fulfilled`).
pub async fn fulfilled(
    grobase: &str,
    token: &str,
    ids: (&str, &str),
    grant_id: &str,
) -> anyhow::Result<Fulfilled> {
    let (org, project) = ids;
    let path = format!("/v1/orgs/{org}/projects/{project}/grants/{grant_id}/fulfilled");
    rbac::get_json(grobase, token, &path).await
}

/// Record that `user` now has a wrap for `grant_id`
/// (`POST /v1/orgs/{org}/projects/{proj}/grants/{grantId}/wraps`).
pub async fn record_wrap(
    grobase: &str,
    token: &str,
    ids: (&str, &str),
    grant_id: &str,
    user: &str,
) -> anyhow::Result<()> {
    let (org, project) = ids;
    let path = format!("/v1/orgs/{org}/projects/{project}/grants/{grant_id}/wraps");
    rbac::post_unit(grobase, token, &path, &json!({ "user_id": user })).await
}
