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

pub mod env;
pub mod group;
pub mod invite;
pub mod org;
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

/// An environment (`POST/GET /v1/projects/{project}/environments`).
#[derive(Deserialize)]
pub struct Environment {
    pub id: String,
    pub name: String,
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
