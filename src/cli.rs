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

/// The complete how-to, shown on `42ctl --help` (after_long_help).
const HOWTO: &str = "\
HOW TO USE 42ctl — the zero-knowledge secrets + identity CLI for the 42 stack.

42ctl gives you ONE local identity (an X25519+Ed25519 keypair, sealed by a passphrase),
logs you in to the platform with email-OTP, and pushes/pulls your project's *.env tree to
the vault — encrypted on YOUR machine, so the server stores only opaque blobs (zero-knowledge).

FIRST RUN (a fresh machine):
  # 1. Point the profile at your platform (these are the live endpoints):
  42ctl config endpoint \\
      --server    https://vault42.fly.dev \\        # vault42-server (gRPC store)
      --authority https://grobase-nano.fly.dev \\   # contract authority (issues login contracts)
      --grobase   https://grobase-stack.fly.dev     # grobase (mails the OTP, escrow)
  42ctl config show

  # 2. Create your local zero-knowledge identity (prompts for a NEW passphrase):
  42ctl keys init

  # 3. Log in — a 6-digit code is emailed to you; enter it at the prompt:
  42ctl auth login --tenant <your-tenant> --email you@example.com
  42ctl auth whoami           # principal + address + 'contract: bound'

EVERY DAY — sync your secrets:
  cd <project>
  42ctl push --project <name>           # seal *.env tree + upload (path-aware, byte-exact)
  42ctl pull --project <name>           # DRY-RUN: shows what would change
  42ctl pull --project <name> --apply   # materialize the tree (add --backup to keep current)

A SECOND MACHINE (carry your identity, no file copy):
  # on machine A:  42ctl keys escrow  --email you@example.com   # OTP -> uploads sealed keystore
  # on machine B:  42ctl keys recover --email you@example.com   # OTP -> fetch + unlock w/ passphrase

NOTES (small encrypted project notes that ride the same vault):
  42ctl note add --project <name> <title>     # then note get/ls/rm

ENV KNOBS:
  FT_PROFILE      select an org/environment (also --profile)
  FT_PASSPHRASE   non-interactive passphrase (CI); otherwise prompted, never echoed
  FT_CONFIG       config path (default ~/.config/42ctl/config.json); tokens sit beside it

SECURITY: all plaintext crypto is LOCAL. The server never sees a key or a plaintext secret.
Lose the passphrase -> the data is unrecoverable by design. See RUNBOOK.md / SECURITY.md.";

/// 42ctl — one CLI for the 42 stack. `--profile` selects an org/environment.
#[derive(Parser)]
#[command(
    name = "42ctl",
    version,
    about = "42ctl — the umbrella CLI for the 42 stack",
    after_long_help = HOWTO
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
    /// Project notes (kind=Note) — encrypted, path-addressed, riding the project manifest.
    #[command(subcommand)]
    Note(Note),
    /// Profiles and endpoints (orgs / environments).
    #[command(subcommand)]
    Config(Config),
    /// Org-scoped operations (create, members, invites, GitHub App connect / link / sync).
    #[command(subcommand)]
    Org(Org),
    /// Team-scoped RBAC (create/list teams, members, invites, project grants).
    #[command(subcommand)]
    Team(Team),
    /// Project group operations (create, members, invites).
    #[command(subcommand)]
    Group(Group),
    /// Per-project environments (create, list).
    #[command(subcommand)]
    Env(Env),
    /// Project-role grants for a user.
    #[command(subcommand)]
    Project(Project),
    /// Generalized invite operations (accept by token, show by id).
    #[command(subcommand)]
    Invite(Invite),
    /// Push the project's *.env* tree to the vault (encrypted, path-aware).
    Push {
        #[arg(long)]
        project: Option<String>,
        /// Mirror the tree: also REMOVE manifest entries whose file is no longer
        /// scanned (prune stale / now-ignored paths). Run from the project ROOT.
        #[arg(long)]
        prune: bool,
    },
    /// Pull the project's encrypted tree back (dry-run unless --apply).
    Pull {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        apply: bool,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        backup: bool,
    },
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
    /// Register/log in and obtain a contract for this identity. With `--github`, log in to
    /// grobase via the GitHub device flow instead (saves a session token, no contract).
    Login {
        #[arg(long, required_unless_present = "github")]
        tenant: Option<String>,
        #[arg(long, env = "FT_REGISTER_TOKEN")]
        token: Option<String>,
        /// Account email — when set, require an email OTP (6-digit code) before login.
        #[arg(long, env = "FT_LOGIN_EMAIL")]
        email: Option<String>,
        /// Log in to grobase via the GitHub device flow (no browser callback).
        #[arg(long)]
        github: bool,
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
    /// Publish this identity's public keys to grobase (`PUT /v1/orgs/{org}/pubkey`) so a
    /// scope admin's `sync-keys` can wrap environment keys to you. Run once per org after
    /// joining; the private key never leaves the machine.
    Enroll {
        #[arg(long)]
        org: String,
    },
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
    /// Admin bootstrap: generate an env scope keyset, publish its public key to grobase,
    /// and self-wrap the scope secret to the admin (so it can later reconcile members).
    EnvInit {
        #[arg(long)]
        org: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        env: String,
    },
    /// Admin reconcile: recover the env scope secret, then wrap it to every authorized
    /// member still missing a wrap (skipping members with no registered pubkey).
    SyncKeys {
        #[arg(long)]
        org: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        env: String,
    },
    /// Show each authorized member's scope-key state: active / pending-provision /
    /// pending-enrollment (no registered pubkey).
    ScopeStatus {
        #[arg(long)]
        org: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        env: String,
    },
    /// Seal a value (stdin) to the env's scope public key and store it at PATH — any wrapped
    /// member of the env can read it back, but the server never sees the plaintext.
    SetEnv {
        #[arg(long)]
        org: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        env: String,
        path: String,
    },
    /// Recover the env scope secret, then fetch + decrypt the env secret at PATH to stdout.
    GetEnv {
        #[arg(long)]
        org: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        env: String,
        path: String,
    },
    /// Forward-secure rotation: re-seal every env secret to a fresh scope keyset and re-wrap
    /// the new scope key to the remaining authorized members (a removed member loses access).
    RotateScope {
        #[arg(long)]
        org: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        env: String,
    },
}

/// `note` subcommands — project-scoped (resolve the project from `.42ctl` or `--project`).
#[derive(Subcommand)]
pub enum Note {
    /// Seal a note (stdin or --file) at PATH within the project.
    Add {
        path: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        file: Option<String>,
    },
    /// Fetch and decrypt the note at PATH to stdout.
    Get {
        path: String,
        #[arg(long)]
        project: Option<String>,
    },
    /// List the project's notes.
    Ls {
        #[arg(long)]
        project: Option<String>,
    },
    /// Remove the note at PATH.
    Rm {
        path: String,
        #[arg(long)]
        project: Option<String>,
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

/// `org` subcommands — org-scoped operations (RBAC + provider integrations).
#[derive(Subcommand)]
pub enum Org {
    /// Create an org from a slug + display name.
    Create {
        #[arg(long)]
        slug: String,
        #[arg(long)]
        name: String,
    },
    /// List an org's members.
    Members {
        #[arg(long)]
        org: String,
    },
    /// Invite an email to an org with a role (prints the one-time token).
    Invite {
        #[arg(long)]
        org: String,
        #[arg(long)]
        email: String,
        #[arg(long)]
        role: String,
    },
    /// Accept an org invite by its one-time token.
    AcceptInvite {
        #[arg(long)]
        token: String,
    },
    /// GitHub App connect / link / sync for an org (needs `auth login --github` first).
    #[command(subcommand)]
    Github(OrgGithub),
}

/// `team` subcommands — team RBAC within an org.
#[derive(Subcommand)]
pub enum Team {
    /// Create a team under an org.
    Create {
        #[arg(long)]
        org: String,
        #[arg(long)]
        slug: String,
        #[arg(long)]
        name: String,
    },
    /// List an org's teams.
    List {
        #[arg(long)]
        org: String,
    },
    /// Add a user to a team with a role.
    AddMember {
        #[arg(long)]
        org: String,
        #[arg(long)]
        team: String,
        #[arg(long)]
        user: String,
        #[arg(long, default_value = "member")]
        role: String,
    },
    /// Invite an email to a team (prints the one-time token).
    Invite {
        #[arg(long)]
        org: String,
        #[arg(long)]
        team: String,
        #[arg(long)]
        email: String,
        #[arg(long, default_value = "member")]
        role: String,
    },
    /// Grant a team a project role (optionally scoped to an environment).
    GrantProject {
        #[arg(long)]
        org: String,
        #[arg(long)]
        team: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        role: String,
        #[arg(long)]
        env: Option<String>,
    },
}

/// `group` subcommands — project group operations.
#[derive(Subcommand)]
pub enum Group {
    /// Create a project's group (the server derives the name).
    Create {
        #[arg(long)]
        project: String,
    },
    /// Add a user to a group.
    AddMember {
        #[arg(long)]
        group: String,
        #[arg(long)]
        user: String,
    },
    /// Invite an email to a group (prints the one-time token).
    Invite {
        #[arg(long)]
        group: String,
        #[arg(long)]
        email: String,
    },
}

/// `env` subcommands — per-project environments.
#[derive(Subcommand)]
pub enum Env {
    /// Create an environment under a project.
    Create {
        #[arg(long)]
        project: String,
        #[arg(long)]
        name: String,
    },
    /// List a project's environments.
    List {
        #[arg(long)]
        project: String,
    },
}

/// `project` subcommands — user-scoped project grants.
#[derive(Subcommand)]
pub enum Project {
    /// Grant a user a project role (optionally scoped to an environment).
    Grant {
        #[arg(long)]
        org: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        user: String,
        #[arg(long)]
        role: String,
        #[arg(long)]
        env: Option<String>,
    },
}

/// `invite` subcommands — generalized invite operations.
#[derive(Subcommand)]
pub enum Invite {
    /// Accept an invite by its one-time token.
    Accept {
        #[arg(long)]
        token: String,
    },
    /// Show an invite by its id.
    Show {
        #[arg(long)]
        id: String,
    },
}

/// `org github` subcommands.
#[derive(Subcommand)]
pub enum OrgGithub {
    /// Begin connecting a GitHub App installation to ORG (prints the install URL + nonce).
    Connect { org: String },
    /// Link a GitHub org login to ORG.
    Link { org: String, github_org: String },
    /// Sync GitHub teams/members/repos into ORG's RBAC.
    Sync { org: String },
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
