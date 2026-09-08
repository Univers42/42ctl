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

//! The command layer: maps each parsed subcommand to its handler. Thin by design — the
//! real logic lives in the per-group modules and (as it grows) the `core` use-cases.

mod auth;
mod config;
mod db;
mod env;
mod group;
mod help;
mod help_topics;
mod invite;
mod keys;
mod notes;
mod org;
mod project;
mod scope;
mod scope_init;
mod scope_pubkey;
mod scope_recover;
mod scope_rotate;
mod scope_secret;
mod scope_secret_reseal;
mod scope_status;
mod scope_sync;
mod scope_wrap;
mod sync;
mod team;
mod team_members;
mod unseal;
mod update;
mod vault;
mod version;

use crate::cli::{Cli, Command};

/// Route a parsed CLI invocation. No verb shows the guided overview; offline verbs run
/// synchronously; the network verbs (auth/vault/db/update/…) run on a tokio runtime.
pub fn dispatch(cli: &Cli) -> anyhow::Result<()> {
    let Some(command) = &cli.command else {
        return help::run(None);
    };
    match command {
        Command::Version => version::run(),
        Command::Help { topic } => help::run(topic.as_deref()),
        Command::Unseal => unseal::run(&cli.profile),
        Command::Config(cmd) => config::run(cmd, &cli.profile),
        _ => block_on_net(command, &cli.profile),
    }
}

/// Drive the async network verbs on a fresh runtime.
fn block_on_net(command: &Command, profile: &str) -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(net(command, profile))
}

/// The async dispatch for the network verbs.
async fn net(command: &Command, profile: &str) -> anyhow::Result<()> {
    match command {
        Command::Auth(cmd) => auth::run(cmd, profile).await,
        Command::Keys(cmd) => keys::run(cmd, profile).await,
        Command::Vault(cmd) => vault::run(cmd, profile).await,
        Command::Db(cmd) => db::run(cmd, profile).await,
        Command::Note(cmd) => notes::run(cmd, profile).await,
        Command::Org(cmd) => org::run(cmd, profile).await,
        Command::Team(cmd) => team::run(cmd, profile).await,
        Command::Group(cmd) => group::run(cmd, profile).await,
        Command::Env(cmd) => env::run(cmd, profile).await,
        Command::Project(cmd) => project::run(cmd, profile).await,
        Command::Invite(cmd) => invite::run(cmd, profile).await,
        Command::Push { project } => sync::push(profile, project.as_deref()).await,
        Command::Pull {
            project,
            apply,
            force,
            backup,
        } => sync::pull(profile, project.as_deref(), *apply, *force, *backup).await,
        Command::Update { check, version } => update::run(*check, version.as_deref()).await,
        _ => unreachable!("offline verbs are handled before block_on_net"),
    }
}
