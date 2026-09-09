/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   rbac.rs                                              :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The grobase RBAC REST client — org / team / group / environment / invite verbs against
//! `/v1/orgs/*`, `/v1/projects/*`, `/v1/groups/*`, `/v1/invites/*`. Every call carries the
//! grobase session JWT (from `auth login --github`, in `session.rs`) as a Bearer; grobase
//! RBAC-checks it. This file owns the shared HTTP helpers + typed shapes; the per-domain
//! verbs live in the `org`/`team`/`group`/`env`/`invite` submodules, one capability each.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub mod account;
pub mod env;
pub mod grant;
pub mod group;
pub mod invite;
pub mod org;
pub mod pubkey;
pub mod team;

/// An org as returned by grobase (`POST /v1/orgs`).
#[derive(Deserialize)]
pub struct Org {
    pub id: String,
    pub slug: String,
    pub name: String,
}

/// An org membership row (`GET /v1/orgs/{org}/members`).
#[derive(Deserialize)]
pub struct Member {
    pub user_id: String,
    pub role: String,
    #[serde(default)]
    pub created_at: String,
}

/// An invite plus the cleartext token grobase returns ONCE at issue time.
#[derive(Deserialize)]
pub struct IssuedInvite {
    pub id: String,
    pub token: String,
}

/// A full invite projection (`GET /v1/invites/{id}`).
#[derive(Deserialize)]
pub struct Invite {
    pub id: String,
    pub scope_kind: String,
    pub scope_id: String,
    pub email: String,
    pub role: String,
    pub status: String,
    #[serde(default)]
    pub expires_at: String,
}

/// A team (`POST/GET /v1/orgs/{org}/teams`).
#[derive(Deserialize)]
pub struct Team {
    pub id: String,
    pub slug: String,
    pub name: String,
}

/// A group (`POST /v1/projects/{project}/groups`).
#[derive(Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
}

/// An environment (`POST/GET /v1/projects/{project}/environments`). `scope_pubkey` is the
/// env's vault42 X25519 scope PUBLIC key (base64); `scope_epoch` is its forward-secrecy
/// generation (0 = not yet bootstrapped). Both default for older/unbootstrapped envs.
#[derive(Deserialize)]
pub struct Environment {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub scope_pubkey: Option<String>,
    #[serde(default)]
    pub scope_epoch: u32,
}

/// A project-role grant (`POST /v1/orgs/{org}/projects/{project}/grants`). `env_id` is
/// omitted from the request body when `None` so a missing scope means project-wide.
#[derive(Serialize)]
pub struct GrantRequest {
    pub grantee_kind: String,
    pub grantee_id: String,
    pub project_role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_id: Option<String>,
}

/// A grant as echoed back by grobase (only the id is surfaced to the operator).
#[derive(Deserialize)]
pub struct Grant {
    pub id: String,
}

/// A live project grant row (`GET .../grants`). `env_id` is omitted for project-wide grants;
/// only the fields the scope-key orchestration consumes are projected.
#[derive(Deserialize)]
pub struct ProjectGrant {
    pub id: String,
    #[serde(default)]
    pub env_id: Option<String>,
}

/// A grant's fulfilment for ONE environment at ONE epoch (`GET .../fulfilled`).
///
/// The two lists answer different questions and are not interchangeable. `missing` is the
/// provisioning worklist and empties as members are wrapped, so it is what `sync-keys` reads.
/// `members` is everyone the grant authorizes and does not empty, so it is what rotation must
/// read: re-wrapping from `missing` re-wraps to nobody once provisioning has converged, which
/// is exactly the state an environment is in when someone rotates it.
#[derive(Deserialize)]
pub struct Fulfilled {
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub missing: Vec<String>,
}

/// Everything a grant call needs to address one environment's scope: where the control plane
/// is, who is asking, and which project/environment/epoch the question is about.
///
/// The epoch is not optional. A wrap is only meaningful for the scope key it wrapped, and
/// every rotation replaces that key, so a fulfilment answer without an epoch describes a
/// scope that may no longer exist.
pub struct GrantScope<'a> {
    pub grobase: &'a str,
    pub token: &'a str,
    pub org: &'a str,
    pub project: &'a str,
    pub env_id: &'a str,
    pub epoch: u32,
}

/// A member's registered public keys (`GET .../users/{userId}/pubkey`). All PUBLIC material;
/// only the fields proof-of-possession + wrapping need are projected.
#[derive(Clone, Deserialize)]
pub struct MemberPubkey {
    pub user_id: String,
    pub x25519_pub: String,
    pub ed25519_pub: String,
    pub pubkey_sig: String,
}

/// The env scope public key as published to grobase (`PUT .../scopekey`).
#[derive(Serialize)]
pub struct ScopeKeyRequest {
    pub scope_pubkey: String,
    pub scope_epoch: u32,
}

/// The calling account, as the authority reports it (`GET /v1/auth/me`).
///
/// This is how the caller learns its own user id. It replaces decoding a `sub` claim out of
/// the session token: the identity is asserted by the server that issued the session, not
/// parsed by the client out of a credential it cannot verify.
#[derive(Deserialize)]
pub struct Me {
    pub account_id: String,
}

/// Fetch the calling account's identity.
pub async fn me(base: &str, token: &str) -> anyhow::Result<Me> {
    get_json(base, token, "/v1/auth/me").await
}

/// POST `path` (relative to `grobase`) with `body`, Bearer `token`, decoding the JSON reply.
pub async fn post_json<B: Serialize, R: DeserializeOwned>(
    grobase: &str,
    token: &str,
    path: &str,
    body: &B,
) -> anyhow::Result<R> {
    let resp = reqwest::Client::new()
        .post(url(grobase, path))
        .bearer_auth(token)
        .json(body)
        .send()
        .await?;
    fail_on_error(&resp, path)?;
    Ok(resp.json::<R>().await?)
}

/// POST `path` with `body` for its side effect only — error on a non-2xx, ignore the body.
pub async fn post_unit<B: Serialize>(
    grobase: &str,
    token: &str,
    path: &str,
    body: &B,
) -> anyhow::Result<()> {
    let resp = reqwest::Client::new()
        .post(url(grobase, path))
        .bearer_auth(token)
        .json(body)
        .send()
        .await?;
    fail_on_error(&resp, path)
}

/// PUT `path` (relative to `grobase`) with `body`, Bearer `token`, decoding the JSON reply.
pub async fn put_json<B: Serialize, R: DeserializeOwned>(
    grobase: &str,
    token: &str,
    path: &str,
    body: &B,
) -> anyhow::Result<R> {
    let resp = reqwest::Client::new()
        .put(url(grobase, path))
        .bearer_auth(token)
        .json(body)
        .send()
        .await?;
    fail_on_error(&resp, path)?;
    Ok(resp.json::<R>().await?)
}

/// GET `path` (relative to `grobase`) with Bearer `token`, decoding the JSON reply.
pub async fn get_json<R: DeserializeOwned>(
    grobase: &str,
    token: &str,
    path: &str,
) -> anyhow::Result<R> {
    let resp = reqwest::Client::new()
        .get(url(grobase, path))
        .bearer_auth(token)
        .send()
        .await?;
    fail_on_error(&resp, path)?;
    Ok(resp.json::<R>().await?)
}

/// POST `path` with `body` on a route that takes no credential, decoding the JSON reply.
///
/// Separate from `post_json` rather than passing an empty token: `bearer_auth("")` sends a
/// malformed Authorization header, which a stricter server is entitled to reject and which
/// would fail as an authentication error on a route that never wanted one.
pub async fn post_public<B: Serialize, R: DeserializeOwned>(
    base: &str,
    path: &str,
    body: &B,
) -> anyhow::Result<R> {
    let resp = reqwest::Client::new()
        .post(url(base, path))
        .json(body)
        .send()
        .await?;
    fail_on_error(&resp, path)?;
    Ok(resp.json::<R>().await?)
}

/// DELETE `path` for its side effect only — error on a non-2xx, ignore the body.
pub async fn delete_unit(grobase: &str, token: &str, path: &str) -> anyhow::Result<()> {
    let resp = reqwest::Client::new()
        .delete(url(grobase, path))
        .bearer_auth(token)
        .send()
        .await?;
    fail_on_error(&resp, path)
}

/// Join `grobase` and `path` into one URL, collapsing a trailing slash on the base.
fn url(grobase: &str, path: &str) -> String {
    format!("{}{path}", grobase.trim_end_matches('/'))
}

/// Bail with the route + HTTP status when `resp` is not a success.
fn fail_on_error(resp: &reqwest::Response, path: &str) -> anyhow::Result<()> {
    if resp.status().is_success() {
        return Ok(());
    }
    anyhow::bail!("{path} failed: HTTP {}", resp.status().as_u16())
}
