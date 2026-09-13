/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   env.rs                                               :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/09/13 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/09/13 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `env` — a project's environments, and everything shared through one.
//!
//! Creating and listing environments sits beside what an environment is FOR: its key
//! (`init`, then `keys ls|sync|rotate`), its single secrets (`secret set|get`) and the whole
//! file tree a team shares (`push`, `pull`, `files`). These used to be `vault set-env`,
//! `vault push-env` and seven more, which put the team's shared data under the noun for your
//! PERSONAL secrets; `cli/legacy.rs` still accepts those spellings.

use clap::{Args, Subcommand};

/// `env` subcommands.
#[derive(Subcommand)]
pub enum Env {
    /// Create an environment under a project
    Create {
        /// Project name
        #[arg(long, value_name = "NAME")]
        project: String,
        /// Environment name (dev, staging, prod …)
        #[arg(long, value_name = "NAME")]
        name: String,
    },
    /// List a project's environments
    #[command(visible_alias = "list")]
    Ls {
        /// Project name
        #[arg(long, value_name = "NAME")]
        project: String,
        /// Output shaping: --format / --filter
        #[command(flatten)]
        out: super::Output,
    },
    /// [admin] Bootstrap an environment's shared key: generate, publish, self-wrap
    Init(Scope),
    /// Push the project's file tree to an environment, shared with everyone granted it
    ///
    /// Every scanned file is sealed to the environment's key, so a member the authority
    /// authorised can restore the tree at its original paths and modes. Unlike `push`,
    /// which seals to your own identity and is readable by nobody else.
    Push {
        #[command(flatten)]
        scope: Scope,
        /// Also seal files matching PATTERN to you alone — repeatable, same grammar as --only
        ///
        /// `*.local` is ALWAYS private, flag or not: a `.env.local` never reaches a teammate,
        /// as bytes or as a path. Your own `env pull` restores it and your own `env files`
        /// lists it; to every other member the environment does not contain it. A private
        /// file is sealed whole, so one above the chunking ceiling is refused before anything
        /// uploads.
        #[arg(long, value_name = "PATTERN")]
        private: Vec<String>,
        /// Label every file of this push, KEY=VALUE — repeatable; `env files --filter label=K=V`
        #[arg(long, value_name = "KEY=VALUE")]
        label: Vec<String>,
    },
    /// Restore an environment's file tree here — a dry-run until you pass --apply
    ///
    /// Without --apply this is the PREVIEW: it lists exactly what would be written, at the
    /// paths it would be written to, and touches nothing. With --only it previews only the
    /// selection, so what you see is what --apply then does.
    Pull {
        #[command(flatten)]
        scope: Scope,
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
    /// List an environment's files from its manifests — no file is fetched
    ///
    /// Columns: Path Size Mode Kind Private Labels. Your private files show Private=true; a
    /// teammate's private files are not listed at all, for anyone. Shapes like the other
    /// lists: `--format '{{.Path}} {{.Labels.app}}'`, `--format json`, `--filter Private=true`.
    Files {
        #[command(flatten)]
        scope: Scope,
        /// Output shaping: --format / --filter
        #[command(flatten)]
        out: super::Output,
    },
    /// One secret sealed to the environment's key: set, get
    #[command(subcommand)]
    Secret(EnvSecret),
    /// Who holds the environment's key: ls, sync, rotate
    #[command(subcommand)]
    Keys(EnvKeys),
}

/// `env secret` subcommands.
#[derive(Subcommand)]
pub enum EnvSecret {
    /// Seal a value (stdin) to the environment's shared key and store it at PATH
    Set {
        #[command(flatten)]
        scope: Scope,
        /// Secret path within the environment
        path: String,
    },
    /// Fetch + decrypt an environment secret at PATH to stdout
    Get {
        #[command(flatten)]
        scope: Scope,
        /// Secret path within the environment
        path: String,
    },
}

/// `env keys` subcommands.
#[derive(Subcommand)]
pub enum EnvKeys {
    /// Show each member's key state: active / pending-provision / pending-enrollment
    Ls {
        #[command(flatten)]
        scope: Scope,
        /// Output shaping: --format / --filter
        #[command(flatten)]
        out: super::Output,
    },
    /// [admin] Wrap the environment's key to every authorized member still missing it
    Sync(Scope),
    /// [admin] Rotate the environment's key: re-seal everything, re-wrap to current members
    ///
    /// Run it after removing somebody. Removal takes away AUTHORIZATION; a key they already
    /// hold stays readable until the environment is re-keyed without them.
    Rotate(Scope),
}

/// The environment a verb acts on: which org, which project, which environment.
#[derive(Args)]
pub struct Scope {
    /// Org slug
    #[arg(long, value_name = "SLUG")]
    pub org: String,
    /// Project slug or id
    #[arg(long, value_name = "NAME")]
    pub project: String,
    /// Environment name (dev, staging, prod …)
    #[arg(long, value_name = "NAME")]
    pub env: String,
}
