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
mod invite;
mod keys;
mod notes;
mod org;
mod project;
mod scope;
mod scope_init;
mod scope_pubkey;
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

/// Route a parsed CLI invocation. Offline verbs run synchronously; the network verbs
/// (auth/vault/db) run on a multi-thread tokio runtime.
pub fn dispatch(cli: &Cli) -> anyhow::Result<()> {
    match &cli.command {
        Command::Version => version::run(),
        Command::Update => update::run(),
        Command::Unseal => unseal::run(&cli.profile),
        Command::Config(cmd) => config::run(cmd, &cli.profile),
        _ => block_on_net(cli),
    }
}

/// Drive the async network verbs on a fresh runtime.
fn block_on_net(cli: &Cli) -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(net(cli))
}

/// The async dispatch for the network verbs.
async fn net(cli: &Cli) -> anyhow::Result<()> {
    match &cli.command {
        Command::Auth(cmd) => auth::run(cmd, &cli.profile).await,
        Command::Keys(cmd) => keys::run(cmd, &cli.profile).await,
        Command::Vault(cmd) => vault::run(cmd, &cli.profile).await,
        Command::Db(cmd) => db::run(cmd, &cli.profile).await,
        Command::Note(cmd) => notes::run(cmd, &cli.profile).await,
        Command::Org(cmd) => org::run(cmd, &cli.profile).await,
        Command::Team(cmd) => team::run(cmd, &cli.profile).await,
        Command::Group(cmd) => group::run(cmd, &cli.profile).await,
        Command::Env(cmd) => env::run(cmd, &cli.profile).await,
        Command::Project(cmd) => project::run(cmd, &cli.profile).await,
        Command::Invite(cmd) => invite::run(cmd, &cli.profile).await,
        Command::Push { project } => sync::push(&cli.profile, project.as_deref()).await,
        Command::Pull {
            project,
            apply,
            force,
            backup,
        } => sync::pull(&cli.profile, project.as_deref(), *apply, *force, *backup).await,
        _ => unreachable!("offline verbs are handled before block_on_net"),
    }
}
