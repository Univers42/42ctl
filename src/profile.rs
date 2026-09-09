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
//! resolves a profile to its `Endpoint`, and threads it down.
//!
//! The default profile points at a DUO, not a trio. vault42 serves secrets, and
//! vault42-authority serves everything else: contract issuance, accounts, orgs, teams, groups,
//! grants, member keys, one-time codes and escrow. grobase used to be a third host and is
//! rejected — its stack cannot run inside the fly.io budget it was meant to protect.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A profile's endpoints: the vault42 data plane and the authority that serves everything else.
///
/// `grobase` survives as a field only so a saved config written before the cutover still loads.
/// It is `#[serde(default)]`, and both empty and a retired grobase host resolve to the authority.
/// Do not add a new caller: `otp_base` is the only reader and it exists to retire this field.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Endpoint {
    pub server: String,
    pub authority: String,
    #[serde(default)]
    pub grobase: String,
}

/// Hosts that used to serve the control plane and no longer answer.
///
/// A config saved before the cutover still names one of these. Treating it as unset sends the
/// caller to the authority instead of at a host that is switched off, which is the difference
/// between the CLI working and the CLI reporting a connection error on every org verb.
///
/// THIS LIST IS A MIGRATION ARTEFACT, NOT A FEATURE. It carries configs written before the
/// cutover and should be deleted once none remain, realistically once the operator has run
/// `42ctl config endpoint` again. Do not extend it into a general blocklist: a CLI that silently
/// declines to talk to a host the user named is worse than a stale pointer, and the only thing
/// justifying these two entries is that we are the ones who switched them off.
const RETIRED_CONTROL_PLANE: [&str; 2] = ["grobase-stack.fly.dev", "grobase-nano.fly.dev"];

impl Endpoint {
    /// The base URL of the control plane: the authority, unless a profile overrides it.
    ///
    /// An override that names a retired grobase host is ignored rather than honoured, because
    /// grobase is rejected and that value can only have come from a config written before the
    /// cutover. A genuine override to some other host is still respected.
    pub fn otp_base(&self) -> &str {
        if self.grobase.is_empty() || retired(&self.grobase) {
            &self.authority
        } else {
            &self.grobase
        }
    }
}

/// Whether `url`'s host is one that used to serve the control plane.
///
/// Compares the PARSED host for equality rather than searching the string. `contains` also
/// matched `https://grobase-stack.fly.dev.example.com` and a query string merely mentioning the
/// name, which would reroute a URL the user meant. The failure direction was safe, since the
/// fallback is our own authority and never somebody else's host, but redirecting a request the
/// user deliberately made is wrong however safe the destination.
///
/// A URL that does not parse is not retired: an unparseable override is the user's to see rather
/// than something to quietly reroute.
fn retired(url: &str) -> bool {
    match reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
    {
        Some(host) => RETIRED_CONTROL_PLANE.contains(&host.as_str()),
        None => false,
    }
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
                authority: "https://vault42-authority.fly.dev".to_string(),
                grobase: String::new(),
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

    /// The default names the authority for the control plane and no grobase at all.
    #[test]
    fn default_points_at_the_public_duo() {
        let endpoint = Config::default()
            .endpoint("default")
            .expect("default profile");
        assert!(endpoint.server.contains("vault42.fly.dev"));
        assert!(endpoint.authority.contains("vault42-authority"));
        assert!(
            endpoint.grobase.is_empty(),
            "grobase is retired, not repointed"
        );
        assert_eq!(endpoint.otp_base(), endpoint.authority);
    }

    /// A config saved before the cutover still names grobase, and must not be sent there.
    ///
    /// This is the upgrade path for the only person who has one. Without it every org, team,
    /// grant and one-time-code call would go to a host that is switched off, and the CLI would
    /// report a connection error rather than reaching the authority that now serves those routes.
    #[test]
    fn a_saved_config_naming_a_retired_grobase_falls_back_to_the_authority() {
        for stale in [
            "https://grobase-stack.fly.dev",
            "https://grobase-nano.fly.dev",
            "http://grobase-stack.fly.dev:8000",
        ] {
            let endpoint = Endpoint {
                server: "https://vault42.fly.dev".into(),
                authority: "https://vault42-authority.fly.dev".into(),
                grobase: stale.into(),
            };
            assert_eq!(
                endpoint.otp_base(),
                "https://vault42-authority.fly.dev",
                "{stale} is retired and must not be honoured"
            );
        }
    }

    /// Only the retired host itself is redirected, never a URL that merely mentions it.
    ///
    /// `contains` matched a look-alike host and a query string carrying the name, which reroutes
    /// a request the user deliberately made. Safe destination, wrong behaviour.
    #[test]
    fn a_lookalike_host_is_not_treated_as_retired() {
        for honoured in [
            "https://grobase-stack.fly.dev.example.com",
            "https://example.com/?next=grobase-stack.fly.dev",
            "https://not-grobase-stack.fly.dev",
            "not a url at all",
        ] {
            let endpoint = Endpoint {
                server: "https://vault42.fly.dev".into(),
                authority: "https://vault42-authority.fly.dev".into(),
                grobase: honoured.into(),
            };
            assert_eq!(
                endpoint.otp_base(),
                honoured,
                "{honoured} is not a retired host and must be honoured as written"
            );
        }
    }

    /// A deliberate override to a host that is not retired is still respected.
    #[test]
    fn a_real_override_is_still_honoured() {
        let endpoint = Endpoint {
            server: "https://vault42.fly.dev".into(),
            authority: "https://vault42-authority.fly.dev".into(),
            grobase: "http://127.0.0.1:8444".into(),
        };
        assert_eq!(endpoint.otp_base(), "http://127.0.0.1:8444");
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
