/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   cloud_machine.rs                                     :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `cloud machine` — the machines vault42 runs on: list, inspect, ports, events, logs, and
//! the lifecycle verbs.
//!
//! Reading is a decoded `machine list --json`, because that one call already carries the
//! config, the events and the checks that `inspect`, `ports` and `events` would otherwise ask
//! for separately. Changing anything echoes the exact flyctl command to stderr first, so the
//! operator sees what ran and could paste it back; `--dry-run` prints that line and stops.

use crate::adapters::fly::{self, Machine};
use crate::adapters::flyctl::Flyctl;
use crate::cli::{CloudMachine, Lifecycle, Output};
use crate::cmd::cloud::{one_target, targets};
use crate::profile::Endpoint;
use crate::ui;

/// Route a `cloud machine` invocation.
pub async fn run(cmd: &CloudMachine, fly: &Flyctl, endpoint: &Endpoint) -> anyhow::Result<()> {
    match cmd {
        CloudMachine::Ls { app, out } => ls(fly, endpoint, app.as_deref(), out).await,
        CloudMachine::Inspect { ids, app } => inspect(fly, endpoint, app.as_deref(), ids).await,
        CloudMachine::Ports { app, out } => ports(fly, endpoint, app.as_deref(), out).await,
        CloudMachine::Events { id, app, out } => {
            events(fly, endpoint, app.as_deref(), id, out).await
        }
        CloudMachine::Logs { id, app, no_tail } => {
            logs(fly, endpoint, app.as_deref(), id.as_deref(), *no_tail).await
        }
        CloudMachine::Start(args) => lifecycle(fly, endpoint, args, "start", &[]).await,
        CloudMachine::Stop(args) => lifecycle(fly, endpoint, args, "stop", &[]).await,
        CloudMachine::Restart(args) => lifecycle(fly, endpoint, args, "restart", &[]).await,
        CloudMachine::Suspend(args) => lifecycle(fly, endpoint, args, "suspend", &[]).await,
        CloudMachine::Wait { id, state, app } => {
            wait(fly, endpoint, app.as_deref(), id, state).await
        }
    }
}

/// Every machine of every target app, with the app it belongs to on the row.
async fn ls(
    fly: &Flyctl,
    endpoint: &Endpoint,
    app: Option<&str>,
    out: &Output,
) -> anyhow::Result<()> {
    let mut rows = Vec::new();
    for (name, machine) in collect(fly, endpoint, app).await? {
        rows.push(serde_json::json!({
            "ID": machine.id, "App": name, "Name": machine.name, "State": machine.state,
            "Region": machine.region, "Checks": machine.check_state(),
            "Size": format!("{}-{}x:{}MB", machine.config.guest.cpu_kind,
                            machine.config.guest.cpus, machine.config.guest.memory_mb),
            "Volume": machine.volumes().join(","),
        }));
    }
    ui::render(
        &[
            "ID", "App", "Name", "State", "Region", "Checks", "Size", "Volume",
        ],
        rows,
        out.shape(),
    )
}

/// Everything fly knows about the named machines, as raw JSON.
///
/// Raw, not a projection: `inspect` is where an operator goes for the field no column shows,
/// and dropping anything on the way defeats the only verb that was supposed to keep it.
async fn inspect(
    fly: &Flyctl,
    endpoint: &Endpoint,
    app: Option<&str>,
    ids: &[String],
) -> anyhow::Result<()> {
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for id in super::bulk::targets(ids) {
        let raw = raw_machine(fly, endpoint, app, &id).await?;
        match raw {
            Some(value) => found.push(value),
            None => missing.push(id),
        }
    }
    println!("{}", serde_json::to_string_pretty(&found)?);
    for id in &missing {
        super::bulk::failure(id, &anyhow::anyhow!("no such machine"));
    }
    super::bulk::report(&missing, ids.len())
}

/// One machine's untouched JSON, searched across every target app.
async fn raw_machine(
    fly: &Flyctl,
    endpoint: &Endpoint,
    app: Option<&str>,
    id: &str,
) -> anyhow::Result<Option<serde_json::Value>> {
    for name in targets(endpoint, app)? {
        let all: Vec<serde_json::Value> = fly.json(&fly::args(&["machine", "list"], &name)).await?;
        if let Some(found) = all
            .into_iter()
            .find(|m| m.get("id").and_then(serde_json::Value::as_str) == Some(id))
        {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// Every published port of every machine.
async fn ports(
    fly: &Flyctl,
    endpoint: &Endpoint,
    app: Option<&str>,
    out: &Output,
) -> anyhow::Result<()> {
    let mut rows = Vec::new();
    for (name, machine) in collect(fly, endpoint, app).await? {
        for service in &machine.config.services {
            for port in &service.ports {
                rows.push(serde_json::json!({
                    "Port": format!("{}/{}", port.port, service.protocol),
                    "App": name, "ID": machine.id, "Internal": service.internal_port,
                    "Handlers": port.handlers.join(","), "ForceHTTPS": port.force_https,
                }));
            }
        }
    }
    ui::render(
        &["Port", "App", "ID", "Internal", "Handlers", "ForceHTTPS"],
        rows,
        out.shape(),
    )
}

/// What has happened to one machine, newest first.
async fn events(
    fly: &Flyctl,
    endpoint: &Endpoint,
    app: Option<&str>,
    id: &str,
    out: &Output,
) -> anyhow::Result<()> {
    let machine = collect(fly, endpoint, app)
        .await?
        .into_iter()
        .find(|(_, machine)| machine.id == id)
        .map(|(_, machine)| machine)
        .ok_or_else(|| anyhow::anyhow!("no machine {id} in this profile's apps"))?;
    let rows = machine
        .events
        .iter()
        .map(|event| {
            serde_json::json!({
                "Event": event.kind, "Status": event.status, "Source": event.source,
                "When": ui::reltime(event.timestamp / 1000),
            })
        })
        .collect();
    ui::render(&["Event", "Status", "Source", "When"], rows, out.shape())
}

/// Stream an app's logs, or one machine's.
async fn logs(
    fly: &Flyctl,
    endpoint: &Endpoint,
    app: Option<&str>,
    id: Option<&str>,
    no_tail: bool,
) -> anyhow::Result<()> {
    let name = one_target(endpoint, app)?;
    let mut args = vec!["logs".to_string(), "--app".to_string(), name];
    if let Some(machine) = id {
        args.extend(["--machine".to_string(), machine.to_string()]);
    }
    if no_tail {
        args.push("--no-tail".to_string());
    }
    fly.stream(&args).await
}

/// One lifecycle verb over every named machine, continuing past one that fails.
async fn lifecycle(
    fly: &Flyctl,
    endpoint: &Endpoint,
    args: &Lifecycle,
    verb: &str,
    extra: &[&str],
) -> anyhow::Result<()> {
    let app = one_target(endpoint, args.app.as_deref())?;
    let attempt = super::bulk::targets(&args.ids);
    let mut failed = Vec::new();
    for id in &attempt {
        let invocation = machine_args(verb, id, &app, extra);
        eprintln!("{}", ui::dim(&format!("+ {}", fly.rendered(&invocation))));
        if args.dry_run {
            continue;
        }
        if let Err(error) = fly.stream(&invocation).await {
            super::bulk::failure(id, &error);
            failed.push(id.clone());
        }
    }
    super::bulk::report(&failed, attempt.len())
}

/// `machine <verb> <id> --app <app>` plus whatever the verb adds.
fn machine_args(verb: &str, id: &str, app: &str, extra: &[&str]) -> Vec<String> {
    ["machine", verb, id, "--app", app]
        .iter()
        .chain(extra)
        .map(ToString::to_string)
        .collect()
}

/// Block until a machine reaches `state`.
async fn wait(
    fly: &Flyctl,
    endpoint: &Endpoint,
    app: Option<&str>,
    id: &str,
    state: &str,
) -> anyhow::Result<()> {
    let name = one_target(endpoint, app)?;
    let args = machine_args("wait", id, &name, &["--state", state]);
    eprintln!("{}", ui::dim(&format!("+ {}", fly.rendered(&args))));
    fly.stream(&args).await
}

/// Every machine of every target app, paired with the app's name.
async fn collect(
    fly: &Flyctl,
    endpoint: &Endpoint,
    app: Option<&str>,
) -> anyhow::Result<Vec<(String, Machine)>> {
    let mut all = Vec::new();
    for name in targets(endpoint, app)? {
        for machine in fly::machines(fly, &name).await? {
            all.push((name.clone(), machine));
        }
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The echoed line is the command the operator could paste back, with the app named
    /// explicitly so it cannot land on a different one than the one they were reading.
    #[test]
    fn a_lifecycle_invocation_names_the_machine_and_its_app() {
        assert_eq!(
            machine_args("stop", "abc123", "vault42-server", &[]),
            vec!["machine", "stop", "abc123", "--app", "vault42-server"]
        );
        assert_eq!(
            machine_args("wait", "abc123", "vault42-server", &["--state", "started"]),
            vec![
                "machine",
                "wait",
                "abc123",
                "--app",
                "vault42-server",
                "--state",
                "started"
            ]
        );
    }
}
