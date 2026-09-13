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

//! `cloud` — the vault42 deployment: the machines it runs on, their volumes, network and
//! secrets, and whether the whole thing is actually working.
//!
//! Every verb here delegates to flyctl rather than to Fly's API. What 42ctl adds is the part
//! flyctl cannot know: which two apps make up a vault42 deployment, and what "healthy" means
//! for them — the authority answering, a 64-hex contract key, the server pinned to it, an
//! encrypted volume attached, a recent snapshot, the scope-key flag on.

use clap::Subcommand;

/// `cloud` subcommands.
#[derive(Subcommand)]
pub enum Cloud {
    /// The fly apps this profile's endpoints point at
    ///
    /// Derived from the server and authority URLs, so there is nothing to configure: a
    /// profile naming `https://vault42-server.fly.dev` is bound to the app `vault42-server`.
    Apps {
        /// Output shaping: --format / --filter / -q
        #[command(flatten)]
        out: super::Output,
    },
    /// Deployment status for both apps
    Status {
        /// One app only (default: both of this profile's)
        #[arg(long, value_name = "NAME")]
        app: Option<String>,
        /// Output shaping: --format / --filter / -q
        #[command(flatten)]
        out: super::Output,
    },
    /// Check the whole deployment: endpoints, keys, machines, volumes, snapshots
    ///
    /// Exits non-zero if any check fails, so it works as a gate. The HTTP probes WAKE a
    /// scale-to-zero machine; pass --no-wake to check only what flyctl can answer cold.
    #[command(alias = "doctor")]
    Health {
        /// Skip the probes that would start a stopped machine
        #[arg(long)]
        no_wake: bool,
        /// Output shaping: --format / --filter / -q
        #[command(flatten)]
        out: super::Output,
    },
    /// The machines running vault42
    #[command(subcommand)]
    Machine(CloudMachine),
    /// The volumes vault42's data lives on
    #[command(subcommand)]
    Volume(CloudVolume),
    /// App secrets — names and digests, never values
    #[command(subcommand)]
    Secret(CloudSecret),
    /// Addresses and certificates
    #[command(subcommand)]
    Net(CloudNet),
}

/// `cloud machine` subcommands.
#[derive(Subcommand)]
pub enum CloudMachine {
    /// List the machines of both apps
    Ls {
        /// One app only (default: both of this profile's)
        #[arg(long, value_name = "NAME")]
        app: Option<String>,
        /// Output shaping: --format / --filter / -q
        #[command(flatten)]
        out: super::Output,
    },
    /// Everything fly knows about one or more machines, as JSON
    Inspect {
        /// Machine ids
        #[arg(required = true, num_args = 1.., value_name = "ID")]
        ids: Vec<String>,
        /// One app only (default: both of this profile's)
        #[arg(long, value_name = "NAME")]
        app: Option<String>,
    },
    /// The published ports of each machine
    Ports {
        /// One app only (default: both of this profile's)
        #[arg(long, value_name = "NAME")]
        app: Option<String>,
        /// Output shaping: --format / --filter / -q
        #[command(flatten)]
        out: super::Output,
    },
    /// The processes running inside a machine
    ///
    /// The one place a cloud verb calls Fly's API instead of flyctl: `fly machine status`
    /// reports the machine, and nothing in flyctl reports what is running inside it. A
    /// stopped machine has no process table and says so.
    Top {
        /// Machine id
        #[arg(value_name = "ID")]
        id: String,
        /// The app the machine belongs to
        #[arg(long, value_name = "NAME")]
        app: Option<String>,
        /// Output shaping: --format / --filter / -q
        #[command(flatten)]
        out: super::Output,
    },
    /// What has happened to a machine, newest first
    Events {
        /// Machine id
        #[arg(value_name = "ID")]
        id: String,
        /// The app the machine belongs to
        #[arg(long, value_name = "NAME")]
        app: Option<String>,
        /// Output shaping: --format / --filter / -q
        #[command(flatten)]
        out: super::Output,
    },
    /// Stream a machine's logs
    Logs {
        /// Machine id (default: every machine of the app)
        #[arg(value_name = "ID")]
        id: Option<String>,
        /// The app to read
        #[arg(long, value_name = "NAME")]
        app: Option<String>,
        /// Print what is there and stop, instead of following
        #[arg(long)]
        no_tail: bool,
    },
    /// [admin] Start one or more stopped machines
    Start(Lifecycle),
    /// [admin] Stop one or more machines
    Stop(Lifecycle),
    /// [admin] Restart one or more machines
    Restart(Lifecycle),
    /// [admin] Suspend one or more machines, keeping their memory
    ///
    /// The pause of `docker pause`: the machine keeps its state and resumes faster than a
    /// cold start. `cloud machine start` is what brings it back.
    Suspend(Lifecycle),
    /// Wait until a machine reaches a state
    Wait {
        /// Machine id
        #[arg(value_name = "ID")]
        id: String,
        /// The state to wait for
        #[arg(long, default_value = "started", value_name = "STATE")]
        state: String,
        /// The app the machine belongs to
        #[arg(long, value_name = "NAME")]
        app: Option<String>,
    },
}

/// The arguments every machine lifecycle verb takes.
///
/// Administrators only, and Fly is what enforces it: a read-only token is refused a lease on
/// the machine, so a member cannot stop one through 42ctl or around it. Nothing here deletes a
/// machine — `adapters/flyctl.rs` refuses every destructive flyctl command outright.
#[derive(clap::Args)]
pub struct Lifecycle {
    /// Machine ids
    #[arg(required = true, num_args = 1.., value_name = "ID")]
    pub ids: Vec<String>,
    /// The app the machines belong to
    #[arg(long, value_name = "NAME")]
    pub app: Option<String>,
    /// Print the flyctl command that would run, and stop
    #[arg(long)]
    pub dry_run: bool,
}

/// `cloud volume` subcommands.
#[derive(Subcommand)]
pub enum CloudVolume {
    /// List the volumes of both apps
    Ls {
        /// One app only (default: both of this profile's)
        #[arg(long, value_name = "NAME")]
        app: Option<String>,
        /// Output shaping: --format / --filter / -q
        #[command(flatten)]
        out: super::Output,
    },
    /// Everything fly knows about one or more volumes, as JSON
    Inspect {
        /// Volume ids
        #[arg(required = true, num_args = 1.., value_name = "ID")]
        ids: Vec<String>,
        /// The app the volumes belong to
        #[arg(long, value_name = "NAME")]
        app: Option<String>,
    },
    /// List a volume's snapshots, newest first
    ///
    /// A volume is the only copy of vault42's database. The snapshot age is what says how
    /// much you would lose, so this is the listing to check before anything destructive.
    Snapshots {
        /// Volume id
        #[arg(value_name = "ID")]
        id: String,
        /// The app the volume belongs to
        #[arg(long, value_name = "NAME")]
        app: Option<String>,
        /// Output shaping: --format / --filter / -q
        #[command(flatten)]
        out: super::Output,
    },
    /// [admin] Take a snapshot of a volume now
    Snapshot {
        /// Volume id
        #[arg(value_name = "ID")]
        id: String,
        /// The app the volume belongs to
        #[arg(long, value_name = "NAME")]
        app: Option<String>,
        /// Print the flyctl command that would run, and stop
        #[arg(long)]
        dry_run: bool,
    },
}

/// `cloud secret` subcommands.
#[derive(Subcommand)]
pub enum CloudSecret {
    /// List an app's secret NAMES and digests
    ///
    /// Fly never hands a value back and 42ctl never asks for one. A digest changing is how
    /// you tell a secret was rotated.
    Ls {
        /// One app only (default: both of this profile's)
        #[arg(long, value_name = "NAME")]
        app: Option<String>,
        /// Output shaping: --format / --filter / -q
        #[command(flatten)]
        out: super::Output,
    },
}

/// `cloud net` subcommands.
#[derive(Subcommand)]
pub enum CloudNet {
    /// The addresses an app answers on
    Ips {
        /// One app only (default: both of this profile's)
        #[arg(long, value_name = "NAME")]
        app: Option<String>,
        /// Output shaping: --format / --filter / -q
        #[command(flatten)]
        out: super::Output,
    },
    /// The certificates an app serves
    Certs {
        /// One app only (default: both of this profile's)
        #[arg(long, value_name = "NAME")]
        app: Option<String>,
        /// Output shaping: --format / --filter / -q
        #[command(flatten)]
        out: super::Output,
    },
}
