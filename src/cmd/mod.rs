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
mod keys;
mod unseal;
mod update;
mod vault;
mod version;

use crate::cli::{Cli, Command};

/// Route a parsed CLI invocation to the right handler.
pub fn dispatch(cli: &Cli) -> anyhow::Result<()> {
    match &cli.command {
        Command::Version => version::run(),
        Command::Update => update::run(),
        Command::Unseal => unseal::run(&cli.profile),
        Command::Config(cmd) => config::run(cmd, &cli.profile),
        Command::Auth(cmd) => auth::run(cmd, &cli.profile),
        Command::Keys(cmd) => keys::run(cmd, &cli.profile),
        Command::Vault(cmd) => vault::run(cmd, &cli.profile),
        Command::Db(cmd) => db::run(cmd, &cli.profile),
    }
}
