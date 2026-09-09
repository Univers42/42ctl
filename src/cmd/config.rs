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

use crate::cli::{Config as ConfigCmd, EndpointArgs};
use crate::profile::Config;
use crate::ui;
use anyhow::Context;

/// Dispatch a `config` subcommand.
pub fn run(cmd: &ConfigCmd, profile: &str) -> anyhow::Result<()> {
    match cmd {
        ConfigCmd::Show => show(profile),
        ConfigCmd::Profile { name } => profile_cmd(name.as_deref()),
        ConfigCmd::Endpoint(args) => set_endpoint(profile, args),
    }
}

/// Print the endpoints resolved for `profile`.
fn show(profile: &str) -> anyhow::Result<()> {
    let endpoint = Config::load()?.endpoint(profile)?;
    ui::field("profile", profile);
    ui::field("server", &endpoint.server);
    ui::field("authority", &endpoint.authority);
    ui::field("control-plane", endpoint.otp_base());
    if endpoint.blobs.is_set() {
        ui::field("object-store", &endpoint.blobs.endpoint);
        ui::field("bucket", &endpoint.blobs.bucket);
        ui::field("region", endpoint.blobs.signing_region());
    }
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

/// Apply every endpoint the caller named, leaving the rest of the profile as it was.
///
/// Each setting is optional and independent, so `config endpoint --bucket x` changes the
/// bucket and nothing else. That matters because the QA battery and real operators both
/// call this repeatedly to add one setting at a time.
fn set_endpoint(profile: &str, args: &EndpointArgs) -> anyhow::Result<()> {
    let mut cfg = Config::load()?;
    let endpoint = cfg
        .profiles
        .get_mut(profile)
        .with_context(|| format!("unknown profile '{profile}'"))?;
    assign(&mut endpoint.server, args.server.as_deref());
    assign(&mut endpoint.authority, args.authority.as_deref());
    assign(&mut endpoint.grobase, args.grobase.as_deref());
    assign(&mut endpoint.blobs.endpoint, args.blobstore.as_deref());
    assign(&mut endpoint.blobs.bucket, args.bucket.as_deref());
    assign(&mut endpoint.blobs.region, args.region.as_deref());
    cfg.save()?;
    ui::success(&format!("updated endpoints for '{profile}'"));
    Ok(())
}

/// Overwrite `slot` only when the caller supplied a value.
fn assign(slot: &mut String, value: Option<&str>) {
    if let Some(value) = value {
        *slot = value.to_string();
    }
}
