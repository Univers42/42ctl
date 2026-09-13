/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   github_org.rs                                       :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Org-scoped GitHub verbs against grobase `/v1/orgs/{orgId}/github/*`. Each call carries
//! the session token as a Bearer. `connect_start` returns the App install URL + a single-use
//! nonce, `link` binds a GitHub org login, and `sync` upserts GitHub teams/members/repos into
//! the org's RBAC.
//!
//! **vault42's authority serves none of these routes** — only grobase did, and grobase is not
//! this product's control plane. Against vault42 every call answers 404, and `failed` says that
//! in words rather than as a status that reads like a typo in the org name.

use serde::Deserialize;
use serde_json::json;

/// The org-connect handoff: a single-use nonce + the GitHub App install URL.
#[derive(Deserialize)]
pub struct ConnectStart {
    pub nonce: String,
    pub install_url: String,
}

/// What a sync upserted (echoed back for the operator).
#[derive(Deserialize)]
pub struct SyncSummary {
    pub teams: i64,
    pub members: i64,
    pub repos: i64,
    pub roles_seeded: i64,
}

/// Begin an org-scoped connect → `{nonce, install_url}`.
pub async fn connect_start(
    grobase: &str,
    token: &str,
    org_id: &str,
) -> anyhow::Result<ConnectStart> {
    let url = format!(
        "{}/v1/orgs/{org_id}/github/connect/start",
        grobase.trim_end_matches('/')
    );
    let resp = reqwest::Client::new()
        .post(url)
        .bearer_auth(token)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(failed("connect start", resp.status().as_u16()));
    }
    Ok(resp.json::<ConnectStart>().await?)
}

/// Link `github_org_login` to the vault42 org `org_id`.
pub async fn link(
    grobase: &str,
    token: &str,
    org_id: &str,
    github_org_login: &str,
) -> anyhow::Result<()> {
    let url = format!(
        "{}/v1/orgs/{org_id}/github/link",
        grobase.trim_end_matches('/')
    );
    let resp = reqwest::Client::new()
        .post(url)
        .bearer_auth(token)
        .json(&json!({ "github_org_login": github_org_login }))
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(failed("link", resp.status().as_u16()));
    }
    Ok(())
}

/// Run the GitHub→vault42 sync for `org_id`, returning the upsert summary.
pub async fn sync(grobase: &str, token: &str, org_id: &str) -> anyhow::Result<SyncSummary> {
    let url = format!(
        "{}/v1/orgs/{org_id}/github/sync",
        grobase.trim_end_matches('/')
    );
    let resp = reqwest::Client::new()
        .post(url)
        .bearer_auth(token)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(failed("sync", resp.status().as_u16()));
    }
    Ok(resp.json::<SyncSummary>().await?)
}

/// A failed `org github` call. A 404 means the control plane has no such route at all — the
/// vault42 authority has none — which is a different fact from "your org was not found".
fn failed(verb: &str, status: u16) -> anyhow::Error {
    if status == 404 {
        return anyhow::anyhow!(
            "org github {verb}: this authority does not serve GitHub organisation routes \
             (HTTP 404) — the vault42 authority has none; manage members with `org invite` \
             and `team member add`"
        );
    }
    anyhow::anyhow!("org github {verb} failed: HTTP {status}")
}
