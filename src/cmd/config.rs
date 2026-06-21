/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   config.rs                                            :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl config` — manage profiles and endpoints (orgs / environments). A new profile
//! inherits the active profile's endpoints; `endpoint` edits the named profile in place;
//! `show` prints the resolved endpoints. The config is a plain JSON file (no secrets).

use crate::cli::Config as ConfigCmd;
use crate::profile::Config;
use crate::ui;
use anyhow::Context;

/// Dispatch a `config` subcommand.
pub fn run(cmd: &ConfigCmd, profile: &str) -> anyhow::Result<()> {
    match cmd {
        ConfigCmd::Show => show(profile),
        ConfigCmd::Profile { name } => profile_cmd(name.as_deref()),
        ConfigCmd::Endpoint {
            server,
            authority,
            grobase,
        } => set_endpoint(profile, server.as_deref(), authority.as_deref(), grobase.as_deref()),
    }
}

/// Print the endpoints resolved for `profile`.
fn show(profile: &str) -> anyhow::Result<()> {
    let endpoint = Config::load()?.endpoint(profile)?;
    ui::field("profile", profile);
    ui::field("server", &endpoint.server);
    ui::field("authority", &endpoint.authority);
    ui::field("grobase", endpoint.otp_base());
    Ok(())
}

/// List profiles (no name) or switch to / create `name` (inheriting current endpoints).
fn profile_cmd(name: Option<&str>) -> anyhow::Result<()> {
    let mut cfg = Config::load()?;
    let Some(name) = name else {
        for profile in cfg.profiles.keys() {
            let marker = if *profile == cfg.current { "*" } else { " " };
            let name = if *profile == cfg.current {
                ui::accent(profile)
            } else {
                profile.to_string()
            };
            println!("{marker} {name}");
        }
        return Ok(());
    };
    if !cfg.profiles.contains_key(name) {
        let base = cfg.endpoint(&cfg.current)?;
        cfg.profiles.insert(name.to_string(), base);
    }
    cfg.current = name.to_string();
    cfg.save()?;
    ui::success(&format!("active profile: {name}"));
    Ok(())
}

/// Set `profile`'s server/authority/grobase endpoints in place.
fn set_endpoint(
    profile: &str,
    server: Option<&str>,
    authority: Option<&str>,
    grobase: Option<&str>,
) -> anyhow::Result<()> {
    let mut cfg = Config::load()?;
    let endpoint = cfg
        .profiles
        .get_mut(profile)
        .with_context(|| format!("unknown profile '{profile}'"))?;
    if let Some(server) = server {
        endpoint.server = server.to_string();
    }
    if let Some(authority) = authority {
        endpoint.authority = authority.to_string();
    }
    if let Some(grobase) = grobase {
        endpoint.grobase = grobase.to_string();
    }
    cfg.save()?;
    ui::success(&format!("updated endpoints for '{profile}'"));
    Ok(())
}
