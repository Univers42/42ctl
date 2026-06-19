/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   profile.rs                                           :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Profiles & endpoints — the multi-org/environment config, stored as JSON at
//! `$FT_CONFIG` or `~/.config/42ctl/config.json`. No globals: a caller loads a `Config`,
//! resolves a profile to its `Endpoint`, and threads it down. The default profile points
//! at the public duo (vault42 + grobase-nano).

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A profile's endpoints: the vault42 data plane + the contract authority.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Endpoint {
    pub server: String,
    pub authority: String,
}

/// The active profile name plus the named profiles.
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub current: String,
    pub profiles: BTreeMap<String, Endpoint>,
}

impl Default for Config {
    fn default() -> Self {
        let mut profiles = BTreeMap::new();
        profiles.insert(
            "default".to_string(),
            Endpoint {
                server: "https://vault42.fly.dev".to_string(),
                authority: "https://grobase-nano.fly.dev".to_string(),
            },
        );
        Self {
            current: "default".to_string(),
            profiles,
        }
    }
}

/// The config file path (`$FT_CONFIG` or the per-user default).
pub fn config_path() -> anyhow::Result<PathBuf> {
    if let Ok(custom) = std::env::var("FT_CONFIG") {
        return Ok(PathBuf::from(custom));
    }
    let base = dirs::config_dir().context("no config directory")?;
    Ok(base.join("42ctl").join("config.json"))
}

impl Config {
    /// Load the config, or the built-in default if none exists yet.
    pub fn load() -> anyhow::Result<Self> {
        match std::fs::read(config_path()?) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(_) => Ok(Self::default()),
        }
    }

    /// Persist the config, creating parent directories.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    /// Resolve `profile` to its endpoints.
    pub fn endpoint(&self, profile: &str) -> anyhow::Result<Endpoint> {
        self.profiles.get(profile).cloned().with_context(|| {
            format!("unknown profile '{profile}' (create it with `42ctl config profile {profile}`)")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_points_at_the_public_duo() {
        let endpoint = Config::default()
            .endpoint("default")
            .expect("default profile");
        assert!(endpoint.server.contains("vault42"));
        assert!(endpoint.authority.contains("grobase-nano"));
    }

    #[test]
    fn unknown_profile_is_an_error() {
        assert!(Config::default().endpoint("nope").is_err());
    }

    #[test]
    fn config_json_round_trips() {
        let bytes = serde_json::to_vec(&Config::default()).expect("serialize");
        let back: Config = serde_json::from_slice(&bytes).expect("deserialize");
        assert_eq!(back.current, "default");
        assert!(back.profiles.contains_key("default"));
    }
}
