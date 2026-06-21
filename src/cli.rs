/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   cli.rs                                               :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The clap command surface for `42ctl` — the umbrella CLI. Command *groups* mirror the
//! stack: `auth` (grobase/contract), `keys` + `vault`/`secrets` (zero-knowledge, all
//! plaintext crypto local), `db` (RBAC-checked encrypted records), `config` (profiles),
//! `version`, `update` (verify-before-swap), and operator-only `unseal`. This file is
//! types only — handlers live under `cmd/`.

use clap::{Parser, Subcommand};

/// 42ctl — one CLI for the 42 stack. `--profile` selects an org/environment.
#[derive(Parser)]
#[command(
    name = "42ctl",
    version,
    about = "42ctl — the umbrella CLI for the 42 stack"
)]
pub struct Cli {
    #[arg(long, env = "FT_PROFILE", default_value = "default", global = true)]
    pub profile: String,
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level command groups.
#[derive(Subcommand)]
pub enum Command {
    /// Authenticate against the platform (login / token lease / revoke).
    #[command(subcommand)]
    Auth(Auth),
    /// Manage your local zero-knowledge identity keypair.
    #[command(subcommand)]
    Keys(Keys),
    /// Zero-knowledge secrets (alias: `secrets`). All plaintext crypto is local.
    #[command(subcommand, alias = "secrets")]
    Vault(Vault),
    /// Read RBAC-checked encrypted records; decrypt client-side.
    #[command(subcommand)]
    Db(Db),
    /// Profiles and endpoints (orgs / environments).
    #[command(subcommand)]
    Config(Config),
    /// Print the version and commit.
    Version,
    /// Self-update (verifies signature + provenance before swapping the binary).
    Update,
    /// Operator-only: unseal the vault.
    Unseal,
}

/// `auth` subcommands.
#[derive(Subcommand)]
pub enum Auth {
    /// Register/log in and obtain a contract for this identity.
    Login {
        #[arg(long)]
        tenant: String,
        #[arg(long, env = "FT_REGISTER_TOKEN")]
        token: Option<String>,
        /// Account email — when set, require an email OTP (6-digit code) before login.
        #[arg(long, env = "FT_LOGIN_EMAIL")]
        email: Option<String>,
    },
    /// Clear the saved contract/token for this profile.
    Logout,
    /// Show the current principal + tenant.
    Whoami,
    /// Show authentication status for this profile.
    Status,
}

/// `keys` subcommands.
#[derive(Subcommand)]
pub enum Keys {
    /// Generate a new local identity + keystore.
    Init {
        #[arg(long)]
        force: bool,
    },
    /// Print this identity's shareable public address.
    ExportPub,
    /// Escrow the passphrase-wrapped keystore to grobase (multi-device), gated by an
    /// email OTP. The server stores only ciphertext — your passphrase never leaves.
    Escrow {
        #[arg(long, env = "FT_LOGIN_EMAIL")]
        email: String,
    },
    /// Recover the keystore on a new machine: email OTP → fetch the escrow → unlock
    /// locally with your passphrase.
    Recover {
        #[arg(long, env = "FT_LOGIN_EMAIL")]
        email: String,
    },
}

/// `vault` / `secrets` subcommands.
#[derive(Subcommand)]
pub enum Vault {
    /// Fetch and locally decrypt a secret to stdout.
    Get {
        path: String,
        #[arg(long, default_value_t = 0)]
        version: u64,
    },
    /// Seal a secret (stdin or --file) and store it.
    Set {
        path: String,
        #[arg(long)]
        file: Option<String>,
    },
    /// List secrets under an optional prefix.
    Ls {
        #[arg(default_value = "")]
        prefix: String,
    },
    /// Remove a secret.
    Rm { path: String },
    /// Re-seal a secret under a fresh data key.
    Rotate { path: String },
    /// Re-seal a secret for another identity's address.
    Share {
        path: String,
        #[arg(long)]
        to: String,
    },
    /// Stream this identity's tamper-evident audit chain.
    Audit {
        #[arg(long, default_value_t = 0)]
        since: i64,
    },
    /// Import a `.env` file, sealing each `KEY=VALUE` as `<prefix>/KEY`.
    Import { source: String },
    /// Export the caller's secrets under a prefix as `KEY=value` lines.
    Export {
        #[arg(long, default_value = "")]
        prefix: String,
    },
}

/// `db` subcommands.
#[derive(Subcommand)]
pub enum Db {
    /// Read one encrypted record and decrypt it locally.
    Get { path: String },
    /// List readable records under a prefix.
    Ls {
        #[arg(default_value = "")]
        prefix: String,
    },
}

/// `config` subcommands.
#[derive(Subcommand)]
pub enum Config {
    /// Show or switch/create the active profile.
    Profile { name: Option<String> },
    /// Set this profile's endpoints.
    Endpoint {
        #[arg(long)]
        server: Option<String>,
        #[arg(long)]
        authority: Option<String>,
        /// grobase URL that serves the email-OTP routes (for `auth login --email`).
        #[arg(long)]
        grobase: Option<String>,
    },
    /// Print the resolved configuration.
    Show,
}
