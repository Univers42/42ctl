/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   creds.rs                                             :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The per-profile contract token file — the credential the authority issued at login.
//! It sits beside the active config file as `contract-<profile>.tok` (or `$FT_CONTRACT`)
//! and is sent as `x-v42-contract` on every request when present. It
//! carries no secret (a signed, public claim), so it is stored in the clear, one file
//! per profile so distinct orgs/environments keep distinct contracts.

use std::path::PathBuf;

/// The contract file path for `profile`: `$FT_CONTRACT` overrides; else
/// `contract-<profile>.tok` beside the active config file (so `$FT_CONFIG` isolates it too).
pub fn contract_path(profile: &str) -> PathBuf {
    if let Ok(custom) = std::env::var("FT_CONTRACT") {
        return PathBuf::from(custom);
    }
    token_dir().join(format!("contract-{profile}.tok"))
}

/// The directory per-profile tokens live in — the directory of the active config file,
/// falling back to `~/.config/42ctl` when the config path has no parent. Shared by the
/// contract and the grobase session token (`session.rs`).
pub(crate) fn token_dir() -> PathBuf {
    if let Ok(config) = crate::profile::config_path() {
        if let Some(parent) = config.parent() {
            return parent.to_path_buf();
        }
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("42ctl")
}

/// Load the saved contract token for `profile`, if any.
pub fn load(profile: &str) -> Option<String> {
    std::fs::read_to_string(contract_path(profile))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Persist a contract token for `profile`, creating parent directories.
pub fn save(profile: &str, contract: &str) -> anyhow::Result<()> {
    let path = contract_path(profile);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, contract)?;
    Ok(())
}

/// Remove the saved contract token for `profile` (no error if it never existed).
pub fn clear(profile: &str) -> anyhow::Result<()> {
    match std::fs::remove_file(contract_path(profile)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
