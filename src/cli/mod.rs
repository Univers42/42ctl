/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   mod.rs                                               :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The clap command surface for `42ctl` — the umbrella CLI. Command *groups* mirror the
//! stack: `auth` (grobase/contract), `keys` + `vault`/`secrets` (zero-knowledge, all
//! plaintext crypto local), `push`/`pull`/`note` (project sync), the RBAC verbs (`rbac`),
//! `config` (profiles), `version`, `update` (verify-before-swap), `help` (the guided
//! walkthrough), and operator-only `unseal`. Types only — handlers live under `cmd/`.

mod rbac;
mod vault;

pub use rbac::{Env, Group, Invite, Org, OrgGithub, Project, Team};
pub use vault::{Db, Note, Vault};

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Parser, Subcommand};

/// The `--help` colour scheme: cyan headings, green literals, yellow placeholders.
const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Yellow.on_default())
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::Yellow.on_default());

const ABOUT: &str = "Zero-knowledge secrets & identity for the 42 stack";

const LONG_ABOUT: &str = "\
42ctl is the one CLI for the 42 stack (grobase + vault42). It gives you a local identity,
logs you in to the platform, and pushes/pulls your project's *.env tree to the vault —
encrypted on YOUR machine, so the server only ever stores opaque ciphertext.";

const AFTER_HELP: &str = "\
Guided walkthrough:  42ctl help            (or `42ctl help <topic>`)
Topics:              quickstart  sync  keys  teams  scopes  notes  config  security  update
Per-command help:    42ctl <command> --help";

/// 42ctl — one CLI for the 42 stack. `--profile` selects an org/environment.
#[derive(Parser)]
#[command(
    name = "42ctl",
    version,
    about = ABOUT,
    long_about = LONG_ABOUT,
    after_help = AFTER_HELP,
    styles = STYLES,
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Profile (org / environment) to act on — see `42ctl config profile`.
    #[arg(
        long,
        env = "FT_PROFILE",
        default_value = "default",
        global = true,
        value_name = "NAME"
    )]
    pub profile: String,
    /// The verb to run; none at all shows the guided overview (`42ctl help`).
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Top-level command groups, in the order `--help` lists them.
#[derive(Subcommand)]
pub enum Command {
    /// Log in / out of the platform and inspect who you are
    #[command(subcommand)]
    Auth(Auth),
    /// Your local zero-knowledge identity: create, export, enroll, escrow, recover
    #[command(subcommand)]
    Keys(Keys),
    /// Secrets sealed on your machine — get, set, ls, share, rotate, import, export …
    #[command(subcommand, visible_alias = "secrets")]
    Vault(Vault),
    /// Upload the project's *.env tree to the vault (encrypted, path-aware, byte-exact)
    Push {
        /// Project name (default: the `.42ctl/` marker in the current directory)
        #[arg(long, value_name = "NAME")]
        project: Option<String>,
    },
    /// Download the project's tree back — a dry-run until you pass --apply
    Pull {
        /// Project name (default: the `.42ctl/` marker in the current directory)
        #[arg(long, value_name = "NAME")]
        project: Option<String>,
        /// Write the files (without this flag, only report what would change)
        #[arg(long)]
        apply: bool,
        /// Take the remote version even where a local edit conflicts
        #[arg(long)]
        force: bool,
        /// Keep a `.bak` copy of every file that is overwritten
        #[arg(long)]
        backup: bool,
    },
    /// Small encrypted notes that travel with the project
    #[command(subcommand)]
    Note(Note),
    /// RBAC-checked encrypted records, decrypted client-side
    #[command(subcommand)]
    Db(Db),
    /// Organisations: create, members, invites, GitHub App connect / link / sync
    #[command(subcommand)]
    Org(Org),
    /// Teams inside an org: create, list, members, invites, project grants
    #[command(subcommand)]
    Team(Team),
    /// Project groups: create, members, invites
    #[command(subcommand)]
    Group(Group),
    /// Environments inside a project (dev / staging / prod …)
    #[command(subcommand)]
    Env(Env),
    /// Grant a user a role on a project
    #[command(subcommand)]
    Project(Project),
    /// Accept or inspect an invite by token / id
    #[command(subcommand)]
    Invite(Invite),
    /// Profiles and endpoints (one per org / environment)
    #[command(subcommand)]
    Config(Config),
    /// Print the version, commit and build target
    Version,
    /// Update 42ctl to the latest GitHub release (SHA-256 verified before the swap)
    Update {
        /// Only report whether a newer release exists; change nothing
        #[arg(long)]
        check: bool,
        /// Install this exact release instead of the latest (e.g. 0.3.1)
        #[arg(long, value_name = "X.Y.Z")]
        version: Option<String>,
    },
    /// The guided walkthrough — `42ctl help <topic>` for one subject
    Help {
        /// quickstart · sync · keys · teams · scopes · notes · config · security · update
        #[arg(value_name = "TOPIC")]
        topic: Option<String>,
    },
    /// Operator-only: unseal the vault after a restart
    Unseal,
}

/// `auth` subcommands.
#[derive(Subcommand)]
pub enum Auth {
    /// Register / log in and obtain a contract for this identity
    ///
    /// With `--email`, a 6-digit code is emailed and asked for first. With `--github`,
    /// log in to grobase via the GitHub device flow instead (saves a session token).
    Login {
        /// Tenant to log in to (required unless --github)
        #[arg(long, value_name = "NAME", required_unless_present = "github")]
        tenant: Option<String>,
        /// One-time registration token, if your tenant requires one
        #[arg(long, env = "FT_REGISTER_TOKEN", value_name = "TOKEN")]
        token: Option<String>,
        /// Account email — enables the email OTP step
        #[arg(long, env = "FT_LOGIN_EMAIL", value_name = "EMAIL")]
        email: Option<String>,
        /// Log in to grobase with the GitHub device flow (no browser callback)
        #[arg(long)]
        github: bool,
    },
    /// Forget the saved contract / session for this profile
    Logout,
    /// Show the current principal, tenant and address
    Whoami,
    /// Show whether this profile is logged in
    Status,
}

/// `keys` subcommands.
#[derive(Subcommand)]
pub enum Keys {
    /// Generate a new local identity (X25519 + Ed25519), sealed by a passphrase
    Init {
        /// Overwrite an existing keystore (the old identity is unrecoverable)
        #[arg(long)]
        force: bool,
    },
    /// Print this identity's shareable public address
    ExportPub,
    /// Publish your public keys to an org so its admins can share env keys with you
    ///
    /// Run once per org after joining; the private key never leaves the machine.
    Enroll {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
    },
    /// Back up the passphrase-sealed keystore to grobase (for a second machine)
    ///
    /// Gated by an email OTP. The server stores only ciphertext — your passphrase
    /// never leaves this machine.
    Escrow {
        /// Account email (receives the OTP)
        #[arg(long, env = "FT_LOGIN_EMAIL", value_name = "EMAIL")]
        email: String,
    },
    /// Restore the keystore on a new machine: email OTP → fetch → unlock locally
    Recover {
        /// Account email (receives the OTP)
        #[arg(long, env = "FT_LOGIN_EMAIL", value_name = "EMAIL")]
        email: String,
    },
}

/// `config` subcommands.
#[derive(Subcommand)]
pub enum Config {
    /// List profiles, or switch to / create NAME (inherits the current endpoints)
    Profile {
        /// Profile to switch to (created if missing)
        #[arg(value_name = "NAME")]
        name: Option<String>,
    },
    /// Set this profile's endpoints
    Endpoint {
        /// vault42 server URL (the gRPC store)
        #[arg(long, value_name = "URL")]
        server: Option<String>,
        /// Contract authority URL (issues login contracts)
        #[arg(long, value_name = "URL")]
        authority: Option<String>,
        /// grobase URL (email OTP, escrow, RBAC)
        #[arg(long, value_name = "URL")]
        grobase: Option<String>,
    },
    /// Print the resolved configuration for this profile
    Show,
}
