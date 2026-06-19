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

/// Register `author_pubkey_hex` as `tenant` with the authority, sending the invite
/// `token` when required, and returning the issued contract token.
pub async fn register(
    authority_url: &str,
    author_pubkey_hex: &str,
    tenant: &str,
    token: Option<&str>,
) -> anyhow::Result<String> {
    let url = format!("{}/v1/register", authority_url.trim_end_matches('/'));
    let response = reqwest::Client::new()
        .post(url)
        .json(&RegisterReq {
            tenant,
            author_pubkey: author_pubkey_hex,
            token,
        })
        .send()
        .await?;
    if !response.status().is_success() {
        anyhow::bail!("registration failed: HTTP {}", response.status().as_u16());
    }
    Ok(response.json::<RegisterResp>().await?.contract)
}
