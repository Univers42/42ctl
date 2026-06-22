/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   session.rs                                          :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The per-profile grobase session token — the GoTrue JWT minted by `auth login --github`
//! (the device flow). It is the Bearer the org GitHub verbs send to `/v1/orgs/{id}/github/*`
//! (a JWT-bound, RBAC-checked credential, distinct from the vault42 contract in `creds.rs`).
//! Stored beside the config as `session-<profile>.tok` (or `$FT_SESSION`).

use crate::profile::Config;
use anyhow::Context;
use std::path::PathBuf;

/// Resolve `profile` to its grobase base URL and saved session token — the pair every RBAC
/// verb needs. Errors if the profile is unknown or there is no saved grobase session.
pub fn connect(profile: &str) -> anyhow::Result<(String, String)> {
    let grobase = Config::load()?.endpoint(profile)?.otp_base().to_string();
    let token = load(profile)
        .context("not logged in to grobase — run `42ctl auth login --github` first")?;
    Ok((grobase, token))
}

/// The session-token path for `profile`: `$FT_SESSION` overrides; else
/// `session-<profile>.tok` beside the active config file.
pub fn session_path(profile: &str) -> PathBuf {
    if let Ok(custom) = std::env::var("FT_SESSION") {
        return PathBuf::from(custom);
    }
    crate::adapters::creds::token_dir().join(format!("session-{profile}.tok"))
}

/// Load the saved grobase session token for `profile`, if any.
pub fn load(profile: &str) -> Option<String> {
    std::fs::read_to_string(session_path(profile))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Persist a grobase session token for `profile`, creating parent directories.
pub fn save(profile: &str, token: &str) -> anyhow::Result<()> {
    let path = session_path(profile);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, token)?;
    Ok(())
}

/// Remove the saved session token for `profile` (no error if it never existed).
pub fn clear(profile: &str) -> anyhow::Result<()> {
    match std::fs::remove_file(session_path(profile)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
