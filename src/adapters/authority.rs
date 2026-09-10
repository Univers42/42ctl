/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   authority.rs                                         :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The contract authority client. `register` claims a tenant with a contract authority
//! and returns the signed contract. The CLI sends only its PUBLIC author key; the
//! authority signs a contract binding that key to the tenant and returns it. The contract
//! is then attached to every vault42 request. No secret or private key ever leaves the
//! machine.

use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct RegisterReq<'a> {
    tenant: &'a str,
    author_pubkey: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<&'a str>,
}

#[derive(Deserialize)]
struct RegisterResp {
    contract: String,
}

/// The author identity + the caller's session + optional invite token and email-OTP proof a
/// register call carries (keeps `register` within the ≤4-param limit).
///
/// `session` is what authenticates the call: the authority issues a contract to an ACCOUNT,
/// so a register without one is refused. It is separate from `token`, which is the shared
/// invite string an older authority checked here and a current one checks at signup.
pub struct RegisterSpec<'a> {
    pub author_pubkey_hex: &'a str,
    pub tenant: &'a str,
    pub session: Option<&'a str>,
    pub token: Option<&'a str>,
}

/// Register the author key as `tenant` with the authority, carrying the invite `token`
/// and (when OTP is enforced) the `email` + `otp_proof`, returning the issued contract.
pub async fn register(authority_url: &str, spec: &RegisterSpec<'_>) -> anyhow::Result<String> {
    let url = format!("{}/v1/register", authority_url.trim_end_matches('/'));
    let mut request = reqwest::Client::new().post(url);
    if let Some(session) = spec.session {
        request = request.bearer_auth(session);
    }
    let response = request
        .json(&RegisterReq {
            tenant: spec.tenant,
            author_pubkey: spec.author_pubkey_hex,
            token: spec.token,
        })
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!(
            "registration refused: the authority issues a contract to an ACCOUNT, so sign in \
             first with `42ctl auth signup --email <mail>` then \
             `42ctl auth login --password --email <mail>`"
        );
    }
    if response.status() == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("registration refused: this account already holds its maximum tenant names");
    }
    if !response.status().is_success() {
        anyhow::bail!("registration failed: HTTP {}", response.status().as_u16());
    }
    Ok(response.json::<RegisterResp>().await?.contract)
}
