/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   vault.rs                                             :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The data verbs: `vault`/`secrets` (personal secrets + the env-scope orchestration),
//! `note` (project notes riding the encrypted manifest), and `db` (RBAC-checked
//! records). Every seal/open is local; the server only ever sees ciphertext.

use clap::Subcommand;

/// `vault` / `secrets` subcommands.
#[derive(Subcommand)]
pub enum Vault {
    /// Fetch a secret and decrypt it locally to stdout
    Get {
        /// Secret path, e.g. `app/DATABASE_URL`
        path: String,
        /// Read this version instead of the latest (0 = latest)
        #[arg(long, default_value_t = 0, value_name = "N")]
        version: u64,
    },
    /// Seal a value (stdin, or --file) and store it at PATH
    Set {
        /// Secret path, e.g. `app/DATABASE_URL`
        path: String,
        /// Read the value from this file instead of stdin
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
    },
    /// List your secrets under an optional prefix
    #[command(visible_alias = "list")]
    Ls {
        /// Only paths starting with this prefix
        #[arg(default_value = "", value_name = "PREFIX")]
        prefix: String,
    },
    /// Remove a secret
    Rm {
        /// Secret path
        path: String,
    },
    /// Remove stored chunks that no version of any manifest still references
    ///
    /// A dry run unless --apply, and never touches a chunk younger than the grace period:
    /// an interrupted push has already uploaded chunks no manifest names yet, and resuming
    /// it is exactly what collecting them too early would make impossible.
    Gc {
        /// Actually delete (without this flag, only report what would be collected)
        #[arg(long)]
        apply: bool,
        /// Hours a chunk must have existed before it can be collected (default 24)
        #[arg(long, value_name = "HOURS")]
        grace_hours: Option<i64>,
    },
    /// Re-seal a secret under a fresh data key
    Rotate {
        /// Secret path
        path: String,
    },
    /// Re-seal a secret so another identity can read it
    Share {
        /// Secret path
        path: String,
        /// Recipient's public address (from their `42ctl keys export-pub`)
        #[arg(long, value_name = "ADDRESS")]
        to: String,
    },
    /// Stream this identity's tamper-evident audit chain
    Audit {
        /// Only entries after this Unix timestamp
        #[arg(long, default_value_t = 0, value_name = "EPOCH")]
        since: i64,
    },
    /// Import a .env file, sealing each KEY=VALUE as <prefix>/KEY
    Import {
        /// Path to the .env file
        #[arg(value_name = "FILE")]
        source: String,
    },
    /// Export your secrets under a prefix as KEY=value lines
    Export {
        /// Only paths starting with this prefix
        #[arg(long, default_value = "", value_name = "PREFIX")]
        prefix: String,
    },
    /// [admin] Bootstrap an environment's shared key: generate, publish, self-wrap
    EnvInit {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Project name
        #[arg(long, value_name = "NAME")]
        project: String,
        /// Environment name (dev, staging, prod …)
        #[arg(long, value_name = "NAME")]
        env: String,
    },
    /// [admin] Wrap the environment's key to every authorized member still missing it
    SyncKeys {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Project name
        #[arg(long, value_name = "NAME")]
        project: String,
        /// Environment name
        #[arg(long, value_name = "NAME")]
        env: String,
    },
    /// Show each member's key state: active / pending-provision / pending-enrollment
    ScopeStatus {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Project name
        #[arg(long, value_name = "NAME")]
        project: String,
        /// Environment name
        #[arg(long, value_name = "NAME")]
        env: String,
    },
    /// Seal a value (stdin) to the environment's shared key and store it at PATH
    SetEnv {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Project name
        #[arg(long, value_name = "NAME")]
        project: String,
        /// Environment name
        #[arg(long, value_name = "NAME")]
        env: String,
        /// Secret path within the environment
        path: String,
    },
    /// Fetch + decrypt an environment secret at PATH to stdout
    GetEnv {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Project name
        #[arg(long, value_name = "NAME")]
        project: String,
        /// Environment name
        #[arg(long, value_name = "NAME")]
        env: String,
        /// Secret path within the environment
        path: String,
    },
    /// Push the project's file tree to an environment, shared with everyone granted it
    ///
    /// Every scanned file is sealed to the environment's key, so a member the authority
    /// authorised can restore the tree at its original paths and modes. Unlike `push`,
    /// which seals to your own identity and is readable by nobody else.
    PushEnv {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Project id
        #[arg(long, value_name = "NAME")]
        project: String,
        /// Environment name (dev, staging, prod …)
        #[arg(long, value_name = "NAME")]
        env: String,
    },
    /// Restore an environment's file tree here — a dry-run until you pass --apply
    ///
    /// Without --apply this is the PREVIEW: it lists exactly what would be written, at the
    /// paths it would be written to, and touches nothing. With --only it previews only the
    /// selection, so what you see is what --apply then does.
    PullEnv {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Project id
        #[arg(long, value_name = "NAME")]
        project: String,
        /// Environment name (dev, staging, prod …)
        #[arg(long, value_name = "NAME")]
        env: String,
        /// Restore only paths matching PATTERN — repeatable; default is the whole tree
        ///
        /// Matches the stored RELATIVE PATH with one optional leading and/or trailing `*`:
        ///   --only 'secrets/*'      just the secrets directory
        ///   --only 'secrets/ca.*'   just the CA material
        ///   --only 'srcs/.env'      one exact file
        ///   --only '*.crt'          every certificate, wherever it lives
        /// Repeat the flag to union several selections. A pattern that matches nothing is an
        /// error rather than an empty success, so a typo cannot look like a clean restore.
        #[arg(long, value_name = "PATTERN")]
        only: Vec<String>,
        /// Write the files (without this flag, only report what would be restored)
        #[arg(long)]
        apply: bool,
        /// Keep a `.bak` copy of every file that is overwritten
        #[arg(long)]
        backup: bool,
    },
    /// [admin] Rotate the environment's key: re-seal everything, re-wrap to current members
    RotateScope {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Project name
        #[arg(long, value_name = "NAME")]
        project: String,
        /// Environment name
        #[arg(long, value_name = "NAME")]
        env: String,
    },
}

/// `note` subcommands — project-scoped (resolve the project from `.42ctl` or `--project`).
#[derive(Subcommand)]
pub enum Note {
    /// Seal a note (stdin, or --file) at PATH within the project
    Add {
        /// Note path, e.g. `onboarding.md`
        path: String,
        /// Project name (default: the `.42ctl/` marker in the current directory)
        #[arg(long, value_name = "NAME")]
        project: Option<String>,
        /// Read the note from this file instead of stdin
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
    },
    /// Fetch and decrypt the note at PATH to stdout
    Get {
        /// Note path
        path: String,
        /// Project name (default: the `.42ctl/` marker in the current directory)
        #[arg(long, value_name = "NAME")]
        project: Option<String>,
    },
    /// List the project's notes
    #[command(visible_alias = "list")]
    Ls {
        /// Project name (default: the `.42ctl/` marker in the current directory)
        #[arg(long, value_name = "NAME")]
        project: Option<String>,
    },
    /// Remove the note at PATH
    Rm {
        /// Note path
        path: String,
        /// Project name (default: the `.42ctl/` marker in the current directory)
        #[arg(long, value_name = "NAME")]
        project: Option<String>,
    },
}

/// `db` subcommands.
#[derive(Subcommand)]
pub enum Db {
    /// Read one encrypted record and decrypt it locally
    Get {
        /// Record path
        path: String,
    },
    /// List readable records under a prefix
    #[command(visible_alias = "list")]
    Ls {
        /// Only paths starting with this prefix
        #[arg(default_value = "", value_name = "PREFIX")]
        prefix: String,
    },
}
