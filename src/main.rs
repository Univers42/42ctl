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
//! Errors print with their cause chain; nothing sensitive is ever logged.

mod adapters;
mod cli;
mod cmd;
mod core;
mod ops;
mod profile;
mod trace;
mod ui;

use clap::error::ErrorKind;
use clap::{CommandFactory, FromArgMatches};
use std::process::ExitCode;

/// Entry point: parse, dispatch, map errors to an exit code.
fn main() -> ExitCode {
    quiet_on_a_closed_pipe();
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            ui::report_error(&error);
            ExitCode::FAILURE
        }
    }
}

/// End quietly, as a shell does, when whoever reads the output has gone.
///
/// `42ctl vault ls | head -1` closes the pipe after one line, and the next `println!` answers
/// with a panic: `failed printing to stdout: Broken pipe` plus a backtrace hint, on stderr, for
/// what is an ordinary way to read a listing. A closed pipe exits 141 and prints nothing; every
/// other panic still reaches the default hook.
fn quiet_on_a_closed_pipe() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or_default();
        if message.contains("Broken pipe") {
            std::process::exit(141);
        }
        default(info);
    }));
}

/// Parse the CLI and dispatch to the command layer, answering a help request for a command
/// that has subcommands ourselves so its verbs print in sections rather than one flat list.
fn run() -> anyhow::Result<()> {
    let argv = current_spelling(std::env::args().collect());
    match cli::Cli::command().try_get_matches_from(&argv) {
        Ok(matches) => {
            trace::record(&matches);
            cmd::dispatch(&cli::Cli::from_arg_matches(&matches)?)
        }
        Err(error) => help_or_exit(&error, &argv),
    }
}

/// `argv` with a retired command path rewritten to the one that replaced it.
///
/// The notice goes to a terminal only. A script, a Makefile or the QA battery reading this
/// process's stderr must see exactly what it saw before the rename, or the rename breaks the
/// very callers the old spelling is being kept for.
fn current_spelling(argv: Vec<String>) -> Vec<String> {
    use std::io::IsTerminal;
    let Some(done) = cli::legacy::rewrite(&argv) else {
        return argv;
    };
    if std::io::stderr().is_terminal() {
        eprintln!(
            "{}",
            ui::dim(&format!(
                "note: `42ctl {}` is now `42ctl {}` — the old spelling still works, for now",
                done.old, done.new
            ))
        );
    }
    done.argv
}

/// Render the grouped page when the failed parse was a help request for a command group;
/// otherwise let clap print its own message and choose the exit code.
///
/// clap keeps every leaf page and every real parse error, which is deliberate: its option
/// rendering and its "did you mean" are better than a hand-rolled substitute, and grouping
/// adds nothing to a command that has no subcommands to group.
fn help_or_exit(error: &clap::Error, argv: &[String]) -> anyhow::Result<()> {
    let asked_for_help = matches!(
        error.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    );
    let Some(page) = asked_for_help
        .then(|| cmd::help_grouped::requested(argv))
        .flatten()
    else {
        error.exit()
    };
    if error.kind() == ErrorKind::DisplayHelp {
        return ui::emit(&page);
    }
    eprint!("{page}");
    std::process::exit(2)
}
