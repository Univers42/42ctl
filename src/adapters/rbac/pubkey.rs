/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   pubkey.rs                                           :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The member-pubkey registry + env-scopekey REST calls. Members register their PUBLIC
//! keys (`put`); the admin reads a member's keys to wrap to (`get`), publishes the env's
//! scope public key (`put_scopekey`), and enumerates a project's environments (`list`).
//! All PUBLIC material; carries the grobase JWT.

use crate::adapters::rbac::{self, Environment, MemberPubkey, ScopeKeyRequest};
use serde::Serialize;

/// Read a member's registered public keys (`GET /v1/orgs/{org}/users/{userId}/pubkey`).
pub async fn get(
    grobase: &str,
    token: &str,
    org: &str,
    user: &str,
) -> anyhow::Result<MemberPubkey> {
    let path = format!("/v1/orgs/{org}/users/{user}/pubkey");
    rbac::get_json(grobase, token, &path).await
}

/// Register the caller's own public keys (`PUT /v1/orgs/{org}/pubkey`). `user_id` is taken
/// from the JWT server-side, never the body.
pub async fn put<B: Serialize>(
    grobase: &str,
    token: &str,
    org: &str,
    body: &B,
) -> anyhow::Result<MemberPubkey> {
    let path = format!("/v1/orgs/{org}/pubkey");
    rbac::put_json(grobase, token, &path, body).await
}

/// Publish (or rotate) the env's scope public key
/// (`PUT /v1/projects/{proj}/environments/{env}/scopekey`).
pub async fn put_scopekey(
    grobase: &str,
    token: &str,
    ids: (&str, &str),
    req: &ScopeKeyRequest,
) -> anyhow::Result<Environment> {
    let (project, env) = ids;
    let path = format!("/v1/projects/{project}/environments/{env}/scopekey");
    rbac::put_json(grobase, token, &path, req).await
}

/// List a project's environments incl. their scope pubkey/epoch
/// (`GET /v1/projects/{proj}/environments`).
pub async fn list_environments(
    grobase: &str,
    token: &str,
    project: &str,
) -> anyhow::Result<Vec<Environment>> {
    let path = format!("/v1/projects/{project}/environments");
    rbac::get_json(grobase, token, &path).await
}
