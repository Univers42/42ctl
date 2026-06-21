/* ************************************************************************** */
/*                                                                            */
/*                                                        :::      ::::::::   */
/*   escrow.rs                                          :+:      :+:    :+:   */
/*                                                    +:+ +:+         +:+     */
/*   By: dlesieur <dlesieur@student.42.fr>          +#+  +:+       +#+        */
/*                                                +#+#+#+#+#+   +#+           */
/*   Created: 2026/06/21 04:26:06 by dlesieur          #+#    #+#             */
/*   Updated: 2026/06/21 04:26:10 by dlesieur         ###   ########.fr       */
/*                                                                            */
/* ************************************************************************** */

//! The multi-device keystore-escrow client. `put` uploads the passphrase-wrapped
//! keystore (already ciphertext) to grobase after an email-OTP proof; `fetch` retrieves
//! it on a second device after its own proof. The server only ever holds ciphertext —
//! the passphrase that unlocks it never leaves this machine (zero-knowledge).

use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct FetchResp {
    blob: String,
}

/// Upload the base64 keystore `blob` for `email`, authorized by the OTP `proof`.
pub async fn put(grobase_url: &str, email: &str, proof: &str, blob: &str) -> anyhow::Result<()> {
    let url = format!("{}/v1/auth/escrow", grobase_url.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .put(url)
        .json(&json!({ "email": email, "proof": proof, "blob": blob }))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("escrow upload failed: HTTP {}", resp.status().as_u16());
    }
    Ok(())
}

/// Fetch the base64 keystore blob for `email`, authorized by the OTP `proof`.
pub async fn fetch(grobase_url: &str, email: &str, proof: &str) -> anyhow::Result<String> {
    let url = format!("{}/v1/auth/escrow/fetch", grobase_url.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .post(url)
        .json(&json!({ "email": email, "proof": proof }))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("escrow fetch failed: HTTP {}", resp.status().as_u16());
    }
    Ok(resp.json::<FetchResp>().await?.blob)
}
