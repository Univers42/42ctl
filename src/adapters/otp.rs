/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   otp.rs                                                :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/20 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/20 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The email login-OTP second factor (Bitwarden-style). Before completing login the
//! CLI asks the authority to mail a 6-digit code to the account address, then WAITS —
//! with a timeout — for the operator to type it back, and verifies it. The code never
//! comes from the server to the terminal; only the user, holding the mailbox, closes
//! the loop. Returns the authority's short proof on success.

use crate::ui;
use serde::Deserialize;
use serde_json::json;
use std::io::Write;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// How long the terminal waits for the code before giving up (matches the server TTL).
const OTP_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Deserialize)]
struct VerifyResp {
    proof: Option<String>,
}

/// Run the email-OTP factor against `authority_url` for `email`: request a code, wait
/// for it (timeout), verify it. Returns the proof token on success; errors if the code
/// is wrong/expired or no code is entered in time.
pub async fn email_otp(authority_url: &str, email: &str) -> anyhow::Result<String> {
    let base = authority_url.trim_end_matches('/');
    let client = reqwest::Client::new();
    client
        .post(format!("{base}/v1/auth/otp/request"))
        .json(&json!({ "email": email }))
        .send()
        .await?; // always 200 (no email-enumeration oracle)
    println!("{}", ui::accent(&format!("A 6-digit code was sent to {email}.")));
    let code = prompt_code()?;
    let resp = client
        .post(format!("{base}/v1/auth/otp/verify"))
        .json(&json!({ "email": email, "code": code }))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("verification code rejected (HTTP {})", resp.status().as_u16());
    }
    Ok(resp.json::<VerifyResp>().await?.proof.unwrap_or_default())
}

/// Prompt for the code on stdin and wait up to OTP_TIMEOUT for it (the terminal blocks,
/// then gives up). The reader runs on a thread so the timeout is enforced.
fn prompt_code() -> anyhow::Result<String> {
    print!(
        "{}",
        ui::dim(&format!("Enter the code (waiting {}s): ", OTP_TIMEOUT.as_secs()))
    );
    std::io::stdout().flush().ok();
    let (tx, rx) = mpsc::channel();
    // ponytail: the reader thread parks on stdin if it times out — the process exits right after, so it is reaped
    thread::spawn(move || {
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line).is_ok() {
            let _ = tx.send(line.trim().to_string());
        }
    });
    match rx.recv_timeout(OTP_TIMEOUT) {
        Ok(code) if !code.is_empty() => Ok(code),
        Ok(_) => anyhow::bail!("no code entered"),
        Err(_) => anyhow::bail!("timed out waiting for the verification code"),
    }
}
