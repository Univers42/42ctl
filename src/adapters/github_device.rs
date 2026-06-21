/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   github_device.rs                                    :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! GitHub device-flow login through grobase (no browser callback). `device_login`
//! requests a device code, shows the user the verification URL + code, then polls
//! `/v1/github/device/poll` until grobase returns a minted session JWT or the code
//! expires. The OAuth token is exchanged + discarded server-side; only the session JWT
//! (the grobase login subject) reaches the CLI.

use crate::ui;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

/// The device-code grant grobase proxies back from GitHub.
#[derive(Deserialize)]
pub struct DeviceStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: i64,
    pub interval: i64,
}

#[derive(Deserialize)]
struct PollResp {
    #[serde(default)]
    access_token: Option<String>,
}

/// Run the full device flow against `grobase`, returning the minted session JWT. Prints
/// the verification URL + user code, then polls at the server-advertised interval until a
/// token arrives or the grant expires.
pub async fn device_login(grobase: &str) -> anyhow::Result<String> {
    let start = device_start(grobase).await?;
    println!(
        "{}",
        ui::accent(&format!(
            "Open {} and enter code: {}",
            start.verification_uri, start.user_code
        ))
    );
    let interval = start.interval.max(1) as u64;
    let attempts = (start.expires_in.max(1) as u64 / interval) + 1;
    for _ in 0..attempts {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        if let Some(token) = device_poll(grobase, &start.device_code).await? {
            return Ok(token);
        }
    }
    anyhow::bail!("device login timed out — re-run `42ctl auth login --github`")
}

/// Begin the device flow: ask grobase for a device + user code.
async fn device_start(grobase: &str) -> anyhow::Result<DeviceStart> {
    let url = format!("{}/v1/github/device/start", grobase.trim_end_matches('/'));
    let resp = reqwest::Client::new().post(url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("device start failed: HTTP {}", resp.status().as_u16());
    }
    Ok(resp.json::<DeviceStart>().await?)
}

/// Poll once: `Some(jwt)` when authorized, `None` while still pending.
async fn device_poll(grobase: &str, device_code: &str) -> anyhow::Result<Option<String>> {
    let url = format!("{}/v1/github/device/poll", grobase.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .post(url)
        .json(&json!({ "device_code": device_code }))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("device poll failed: HTTP {}", resp.status().as_u16());
    }
    Ok(resp.json::<PollResp>().await?.access_token)
}
