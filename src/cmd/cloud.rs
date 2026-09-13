/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   cloud.rs                                             :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `cloud` handlers: resolve which fly apps this profile means, then delegate to flyctl.
//!
//! The app names are derived from the profile's endpoints rather than configured, so a
//! deployment that follows the convention needs no setup at all: a profile pointing at
//! `https://vault42-server.fly.dev` is bound to the app `vault42-server`. `--app` overrides it
//! for a deployment that does not, and a URL that names no fly app is a refusal that says so
//! rather than a guess.

use crate::adapters::fly;
use crate::adapters::flyctl::Flyctl;
use crate::cli::{Cloud, CloudNet, CloudSecret};
use crate::profile::{Config, Endpoint};
use crate::ui;

/// The suffix every fly app answers on.
const FLY_HOST: &str = ".fly.dev";

/// Route a `cloud` invocation.
///
/// flyctl is discovered per verb rather than up front: `cloud apps` reads nothing but the
/// profile, and refusing it because no flyctl is installed would be a refusal with no reason.
pub async fn run(command: &Cloud, profile: &str) -> anyhow::Result<()> {
    let endpoint = Config::load()?.endpoint(profile)?;
    match command {
        Cloud::Apps { out } => apps(&endpoint, out),
        Cloud::Status { app, out } => {
            let fly = Flyctl::discover()?;
            status(&fly, &targets(&endpoint, app.as_deref())?, out).await
        }
        Cloud::Health { no_wake, out } => {
            super::cloud_health::run(&Flyctl::discover()?, &endpoint, *no_wake, out).await
        }
        Cloud::Machine(cmd) => {
            super::cloud_machine::run(cmd, &Flyctl::discover()?, &endpoint).await
        }
        Cloud::Volume(cmd) => super::cloud_volume::run(cmd, &Flyctl::discover()?, &endpoint).await,
        Cloud::Secret(CloudSecret::Ls { app, out }) => {
            let fly = Flyctl::discover()?;
            secrets(&fly, &targets(&endpoint, app.as_deref())?, out).await
        }
        Cloud::Net(cmd) => net(cmd, &Flyctl::discover()?, &endpoint).await,
    }
}

/// The apps a profile is bound to, and where each came from.
fn apps(endpoint: &Endpoint, out: &crate::cli::Output) -> anyhow::Result<()> {
    let rows = [
        ("server", &endpoint.server),
        ("authority", &endpoint.authority),
    ]
    .iter()
    .map(|(role, url)| {
        serde_json::json!({
            "App": app_of(url).unwrap_or_default(), "Role": *role, "Endpoint": url,
        })
    })
    .collect();
    ui::render(&["App", "Role", "Endpoint"], rows, out.shape())
}

/// Both apps of a profile, or the one `--app` names.
///
/// An endpoint that is not a `*.fly.dev` host has no app to derive, so this refuses and names
/// the flag rather than inventing a name that would address somebody else's deployment.
pub fn targets(endpoint: &Endpoint, explicit: Option<&str>) -> anyhow::Result<Vec<String>> {
    if let Some(app) = explicit {
        return Ok(vec![app.to_string()]);
    }
    let found: Vec<String> = [&endpoint.server, &endpoint.authority]
        .iter()
        .filter_map(|url| app_of(url))
        .collect();
    if found.is_empty() {
        anyhow::bail!(
            "this profile's endpoints name no fly app ({}, {}) — pass --app <NAME>",
            endpoint.server,
            endpoint.authority
        );
    }
    let mut unique = found;
    unique.dedup();
    Ok(unique)
}

/// Exactly one app: the one `--app` names, or the single one a profile derives.
pub fn one_target(endpoint: &Endpoint, explicit: Option<&str>) -> anyhow::Result<String> {
    let found = targets(endpoint, explicit)?;
    match found.len() {
        1 => Ok(found[0].clone()),
        _ => anyhow::bail!(
            "this profile names {} apps ({}) — say which with --app <NAME>",
            found.len(),
            found.join(", ")
        ),
    }
}

/// The fly app a URL addresses: the host, minus `.fly.dev`.
fn app_of(url: &str) -> Option<String> {
    let host = url
        .rsplit("://")
        .next()?
        .split('/')
        .next()?
        .split(':')
        .next()?;
    host.strip_suffix(FLY_HOST)
        .filter(|app| !app.is_empty() && !app.contains('.'))
        .map(ToString::to_string)
}

/// Each app's deployment status.
async fn status(fly: &Flyctl, apps: &[String], out: &crate::cli::Output) -> anyhow::Result<()> {
    let mut rows = Vec::new();
    for app in apps {
        let machines = fly::machines(fly, app).await?;
        for machine in machines {
            rows.push(serde_json::json!({
                "App": app, "ID": machine.id, "State": machine.state,
                "Region": machine.region, "Checks": machine.check_state(),
                "Image": machine.image_ref.tag, "Updated": machine.updated_at,
            }));
        }
    }
    ui::render(
        &["App", "ID", "State", "Region", "Checks", "Image", "Updated"],
        rows,
        out.shape(),
    )
}

/// Each app's secret NAMES and digests — never a value.
async fn secrets(fly: &Flyctl, apps: &[String], out: &crate::cli::Output) -> anyhow::Result<()> {
    let mut rows = Vec::new();
    for app in apps {
        for secret in fly::secrets(fly, app).await? {
            rows.push(serde_json::json!({
                "Name": secret.name, "App": app, "Digest": secret.digest,
                "Status": secret.status,
            }));
        }
    }
    ui::render(&["Name", "App", "Digest", "Status"], rows, out.shape())
}

/// Addresses and certificates.
async fn net(cmd: &CloudNet, fly: &Flyctl, endpoint: &Endpoint) -> anyhow::Result<()> {
    match cmd {
        CloudNet::Ips { app, out } => {
            let mut rows = Vec::new();
            for name in targets(endpoint, app.as_deref())? {
                for ip in fly::ips(fly, &name).await? {
                    rows.push(serde_json::json!({
                        "Address": ip.address, "App": name, "Type": ip.kind,
                        "Region": ip.region, "Created": ip.created_at,
                    }));
                }
            }
            ui::render(
                &["Address", "App", "Type", "Region", "Created"],
                rows,
                out.shape(),
            )
        }
        CloudNet::Certs { app, out } => {
            let mut rows = Vec::new();
            for name in targets(endpoint, app.as_deref())? {
                for cert in fly::certs(fly, &name).await? {
                    rows.push(serde_json::json!({
                        "Hostname": cert.hostname, "App": name, "Status": cert.client_status,
                        "Created": cert.created_at,
                    }));
                }
            }
            ui::render(&["Hostname", "App", "Status", "Created"], rows, out.shape())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The app name is the fly host, and nothing else is treated as one — a look-alike host
    /// would address somebody else's deployment with this profile's credentials.
    #[test]
    fn only_a_real_fly_host_yields_an_app_name() {
        assert_eq!(
            app_of("https://vault42-server.fly.dev").as_deref(),
            Some("vault42-server")
        );
        assert_eq!(
            app_of("https://vault42-authority.fly.dev/").as_deref(),
            Some("vault42-authority")
        );
        assert_eq!(
            app_of("http://vault42-server.fly.dev:8443").as_deref(),
            Some("vault42-server")
        );
        assert_eq!(
            app_of("http://127.0.0.1:8443"),
            None,
            "a local stack has no app"
        );
        assert_eq!(
            app_of("https://vault42.example.com"),
            None,
            "another host is not fly"
        );
        assert_eq!(
            app_of("https://evil.vault42-server.fly.dev"),
            None,
            "a subdomain of the fly zone is not the app"
        );
        assert_eq!(app_of("https://.fly.dev"), None, "an empty name is no name");
    }

    /// A profile pointing at the public deployment binds to both its apps with no config; a
    /// local one is refused by name rather than guessed at.
    #[test]
    fn a_profile_binds_to_both_apps_or_says_why_it_cannot() {
        let public = Endpoint {
            server: "https://vault42-server.fly.dev".into(),
            authority: "https://vault42-authority.fly.dev".into(),
            ..Default::default()
        };
        assert_eq!(
            targets(&public, None).expect("derived"),
            vec!["vault42-server", "vault42-authority"]
        );
        assert_eq!(
            targets(&public, Some("other")).expect("explicit"),
            vec!["other"]
        );
        assert!(
            one_target(&public, None).is_err(),
            "two apps means the verb must be told which"
        );

        let local = Endpoint {
            server: "http://127.0.0.1:8443".into(),
            authority: "http://127.0.0.1:8444".into(),
            ..Default::default()
        };
        let error = targets(&local, None).expect_err("no fly app").to_string();
        assert!(error.contains("--app"), "names the way out: {error}");
    }
}
