/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   main.rs                                              :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! 42ctl — the umbrella platform CLI for the 42 stack (grobase + vault42). One binary,
//! subcommand groups, multi-profile, zero-knowledge (all plaintext crypto is client-side).
//! P0 is the scaffold: `version` + `config` are real; the network/crypto verbs are wired
//! across P1–P3. Errors print with their cause chain; nothing sensitive is ever logged.

mod adapters;
mod cli;
mod cmd;
mod core;
mod ops;
mod profile;
mod ui;

use clap::Parser;
use std::process::ExitCode;

/// Entry point: parse, dispatch, map errors to an exit code.
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            ui::report_error(&error);
            ExitCode::FAILURE
        }
    }
}

/// Parse the CLI and dispatch to the command layer.
fn run() -> anyhow::Result<()> {
    cmd::dispatch(&cli::Cli::parse())
}
