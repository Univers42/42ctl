/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   cloud_volume.rs                                      :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `cloud volume` — the disks vault42's databases live on.
//!
//! A vault42 volume is the only copy of its SQLite database, and the authority's also holds
//! the contract signing key. So the listing leads with what you would want before doing
//! anything: whether it is encrypted, what it is attached to, and how old the newest snapshot
//! is. That last column is the one that says how much a mistake would cost.

use crate::adapters::fly;
use crate::adapters::flyctl::Flyctl;
use crate::cli::{CloudVolume, Output};
use crate::cmd::cloud::{one_target, targets};
use crate::profile::Endpoint;
use crate::ui;

/// Route a `cloud volume` invocation.
pub async fn run(cmd: &CloudVolume, fly: &Flyctl, endpoint: &Endpoint) -> anyhow::Result<()> {
    match cmd {
        CloudVolume::Ls { app, out } => ls(fly, endpoint, app.as_deref(), out).await,
        CloudVolume::Inspect { ids, app } => inspect(fly, endpoint, app.as_deref(), ids).await,
        CloudVolume::Snapshots { id, app, out } => {
            snapshots(fly, endpoint, app.as_deref(), id, out).await
        }
        CloudVolume::Snapshot { id, app, dry_run } => {
            snapshot(fly, endpoint, app.as_deref(), id, *dry_run).await
        }
    }
}

/// Every volume of every target app.
async fn ls(
    fly: &Flyctl,
    endpoint: &Endpoint,
    app: Option<&str>,
    out: &Output,
) -> anyhow::Result<()> {
    let mut rows = Vec::new();
    for name in targets(endpoint, app)? {
        for volume in fly::volumes(fly, &name).await? {
            rows.push(serde_json::json!({
                "ID": volume.id, "App": name, "Name": volume.name, "State": volume.state,
                "Size": format!("{}GB", volume.size_gb), "Region": volume.region,
                "Encrypted": volume.encrypted,
                "Attached": volume.attached_machine_id.clone().unwrap_or_default(),
                "Snapshots": volume.snapshot_retention,
            }));
        }
    }
    ui::render(
        &[
            "ID",
            "App",
            "Name",
            "State",
            "Size",
            "Region",
            "Encrypted",
            "Attached",
            "Snapshots",
        ],
        rows,
        out.shape(),
    )
}

/// The named volumes' untouched JSON.
async fn inspect(
    fly: &Flyctl,
    endpoint: &Endpoint,
    app: Option<&str>,
    ids: &[String],
) -> anyhow::Result<()> {
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for id in super::bulk::targets(ids) {
        match raw_volume(fly, endpoint, app, &id).await? {
            Some(value) => found.push(value),
            None => missing.push(id),
        }
    }
    println!("{}", serde_json::to_string_pretty(&found)?);
    for id in &missing {
        super::bulk::failure(id, &anyhow::anyhow!("no such volume"));
    }
    super::bulk::report(&missing, ids.len())
}

/// One volume's untouched JSON, searched across every target app.
async fn raw_volume(
    fly: &Flyctl,
    endpoint: &Endpoint,
    app: Option<&str>,
    id: &str,
) -> anyhow::Result<Option<serde_json::Value>> {
    for name in targets(endpoint, app)? {
        let all: Vec<serde_json::Value> = fly.json(&fly::args(&["volumes", "list"], &name)).await?;
        if let Some(found) = all
            .into_iter()
            .find(|v| v.get("id").and_then(serde_json::Value::as_str) == Some(id))
        {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// One volume's snapshots, newest first.
async fn snapshots(
    fly: &Flyctl,
    endpoint: &Endpoint,
    app: Option<&str>,
    id: &str,
    out: &Output,
) -> anyhow::Result<()> {
    let name = one_target(endpoint, app)?;
    let mut all = fly::snapshots(fly, &name, id).await?;
    all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let rows = all
        .iter()
        .map(|snapshot| {
            serde_json::json!({
                "ID": snapshot.id, "Status": snapshot.status,
                "Size": snapshot.size, "Created": snapshot.created_at,
                "Retention": snapshot.retention_days,
            })
        })
        .collect();
    ui::render(
        &["ID", "Status", "Size", "Created", "Retention"],
        rows,
        out.shape(),
    )
}

/// Take a snapshot now.
async fn snapshot(
    fly: &Flyctl,
    endpoint: &Endpoint,
    app: Option<&str>,
    id: &str,
    dry_run: bool,
) -> anyhow::Result<()> {
    let name = one_target(endpoint, app)?;
    let args: Vec<String> = ["volumes", "snapshots", "create", id, "--app", &name]
        .iter()
        .map(ToString::to_string)
        .collect();
    eprintln!("{}", ui::dim(&format!("+ {}", fly.rendered(&args))));
    if dry_run {
        return Ok(());
    }
    fly.stream(&args).await
}
