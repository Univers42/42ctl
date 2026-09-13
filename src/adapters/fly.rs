/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   fly.rs                                               :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! What flyctl's `--json` hands back, typed.
//!
//! Every struct is `#[serde(default)]` throughout: flyctl adds fields between releases, and a
//! listing that refuses to decode because one grew is worse than a listing missing a column.
//! Only the fields 42ctl renders or decides on are named — `inspect` passes the raw value
//! through instead, so nothing is lost where everything is wanted.

use crate::adapters::flyctl::Flyctl;
use serde::Deserialize;

/// One Fly machine.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Machine {
    pub id: String,
    pub name: String,
    pub state: String,
    pub region: String,
    pub private_ip: String,
    pub created_at: String,
    pub updated_at: String,
    pub host_status: String,
    pub cordoned: bool,
    pub image_ref: ImageRef,
    pub config: MachineConfig,
    pub events: Vec<Event>,
    pub checks: Vec<Check>,
}

/// The image a machine runs.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ImageRef {
    pub repository: String,
    pub tag: String,
    pub digest: String,
}

/// The parts of a machine's config 42ctl decides on.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct MachineConfig {
    pub image: String,
    pub env: std::collections::BTreeMap<String, String>,
    pub guest: Guest,
    pub mounts: Vec<Mount>,
    pub services: Vec<Service>,
}

/// A machine's size.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Guest {
    pub cpu_kind: String,
    pub cpus: u32,
    pub memory_mb: u32,
}

/// A volume as the machine mounts it.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Mount {
    pub volume: String,
    pub name: String,
    pub path: String,
    pub size_gb: u32,
    pub encrypted: bool,
}

/// A port mapping.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Service {
    pub protocol: String,
    pub internal_port: u32,
    pub autostart: bool,
    pub autostop: serde_json::Value,
    pub min_machines_running: u32,
    pub ports: Vec<Port>,
}

/// One published port on a service.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Port {
    pub port: u32,
    pub handlers: Vec<String>,
    pub force_https: bool,
}

/// One thing that happened to a machine, newest first in flyctl's output.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Event {
    #[serde(rename = "type")]
    pub kind: String,
    pub status: String,
    pub source: String,
    pub timestamp: i64,
}

/// A health check's last result.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Check {
    pub name: String,
    pub status: String,
    pub output: String,
    pub updated_at: String,
}

/// One volume.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Volume {
    pub id: String,
    pub name: String,
    pub state: String,
    pub size_gb: u32,
    pub region: String,
    pub zone: String,
    pub encrypted: bool,
    pub attached_machine_id: Option<String>,
    pub created_at: String,
    pub snapshot_retention: u32,
    pub auto_backup_enabled: bool,
    pub host_status: String,
}

/// One volume snapshot.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    pub id: String,
    pub size: u64,
    pub status: String,
    pub created_at: String,
    pub retention_days: u32,
}

/// One app secret — its NAME and a digest, never a value. Fly does not hand values back and
/// 42ctl must never ask for one.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Secret {
    pub name: String,
    pub digest: String,
    pub status: String,
}

/// One allocated IP address.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Ip {
    #[serde(alias = "Address")]
    pub address: String,
    #[serde(rename = "Type", alias = "type")]
    pub kind: String,
    #[serde(alias = "Region")]
    pub region: String,
    #[serde(alias = "CreatedAt")]
    pub created_at: String,
}

/// One TLS certificate.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Cert {
    #[serde(alias = "Hostname")]
    pub hostname: String,
    #[serde(alias = "ClientStatus")]
    pub client_status: String,
    #[serde(alias = "CreatedAt")]
    pub created_at: String,
}

/// Every machine of `app`.
pub async fn machines(fly: &Flyctl, app: &str) -> anyhow::Result<Vec<Machine>> {
    fly.json(&args(&["machine", "list"], app)).await
}

/// Every volume of `app`.
pub async fn volumes(fly: &Flyctl, app: &str) -> anyhow::Result<Vec<Volume>> {
    fly.json(&args(&["volumes", "list"], app)).await
}

/// Every snapshot of one volume.
pub async fn snapshots(fly: &Flyctl, app: &str, volume: &str) -> anyhow::Result<Vec<Snapshot>> {
    fly.json(&args(&["volumes", "snapshots", "list", volume], app))
        .await
}

/// Every secret NAME set on `app`.
pub async fn secrets(fly: &Flyctl, app: &str) -> anyhow::Result<Vec<Secret>> {
    fly.json(&args(&["secrets", "list"], app)).await
}

/// Every IP allocated to `app`.
pub async fn ips(fly: &Flyctl, app: &str) -> anyhow::Result<Vec<Ip>> {
    fly.json(&args(&["ips", "list"], app)).await
}

/// Every certificate on `app`.
pub async fn certs(fly: &Flyctl, app: &str) -> anyhow::Result<Vec<Cert>> {
    fly.json(&args(&["certs", "list"], app)).await
}

/// A flyctl invocation for `app`, asking for JSON.
pub fn args(verb: &[&str], app: &str) -> Vec<String> {
    verb.iter()
        .map(ToString::to_string)
        .chain(["--app".to_string(), app.to_string(), "--json".to_string()])
        .collect()
}

impl Machine {
    /// The volume ids this machine mounts.
    pub fn volumes(&self) -> Vec<&str> {
        self.config
            .mounts
            .iter()
            .map(|mount| mount.volume.as_str())
            .collect()
    }

    /// The machine's overall check verdict: the worst status any check reported.
    pub fn check_state(&self) -> String {
        let passing = self
            .checks
            .iter()
            .filter(|check| check.status == "passing")
            .count();
        format!("{passing}/{}", self.checks.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A machine's JSON decodes even when flyctl adds fields, and the derived views read the
    /// nesting the table would otherwise have to reach into itself.
    #[test]
    fn a_machine_decodes_and_summarises_its_nesting() {
        let raw = serde_json::json!({
            "id": "837243f799de98", "name": "sparkling-darkness-6809", "state": "stopped",
            "region": "cdg", "something_new_in_a_later_flyctl": 42,
            "config": {
                "mounts": [{"volume": "vol_x", "name": "vault42_data", "path": "/data",
                            "size_gb": 1, "encrypted": true}],
                "services": [{"protocol": "tcp", "internal_port": 8443,
                              "ports": [{"port": 80}, {"port": 443}]}],
                "guest": {"cpu_kind": "shared", "cpus": 1, "memory_mb": 256}
            },
            "checks": [{"name": "grpc_port", "status": "warning"}]
        });
        let machine: Machine = serde_json::from_value(raw).expect("an unknown field is tolerated");
        assert_eq!(machine.id, "837243f799de98");
        assert_eq!(machine.volumes(), vec!["vol_x"]);
        assert_eq!(machine.config.services[0].ports.len(), 2);
        assert_eq!(machine.check_state(), "0/1", "a warning is not a pass");
        assert_eq!(machine.config.guest.memory_mb, 256);
    }

    /// An empty object still decodes, so a machine flyctl reports with almost nothing on it
    /// lists as a row rather than failing the whole listing.
    #[test]
    fn a_sparse_machine_still_decodes() {
        let machine: Machine = serde_json::from_value(serde_json::json!({})).expect("sparse");
        assert!(machine.id.is_empty());
        assert!(machine.config.services.is_empty());
        assert_eq!(machine.check_state(), "0/0");
    }

    /// Every invocation names the app and asks for JSON, which is what the decoders assume.
    #[test]
    fn an_invocation_names_the_app_and_asks_for_json() {
        assert_eq!(
            args(&["machine", "list"], "vault42-server"),
            vec!["machine", "list", "--app", "vault42-server", "--json"]
        );
    }
}
