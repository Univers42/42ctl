/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   trace.rs                                             :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The command trace the QA battery measures itself with.
//!
//! When `FT_TRACE_COMMANDS` names a file, every invocation that parsed appends one line: its
//! command path, a tab, and the flags it was given (`env files\t--filter --format`). The
//! battery reads this to know which verbs AND which flags it actually RAN, rather than which
//! ones a spec happens to mention: a verb spelled in a spec may never execute, one reached
//! through a helper is invisible to a text search, and counting commands alone said nothing
//! about whether `--filter` was ever tried on a given listing.
//!
//! Only NAMES are written — a command's, a flag's — never a value or a positional argument, so
//! no path, email, token or secret can reach the file. Old spellings are recorded as the path
//! they were rewritten to. Failures are ignored: switching on a measurement must never change
//! what the command does or how it exits.

use clap::parser::ValueSource;

/// Append this invocation's trace line when `FT_TRACE_COMMANDS` is set.
///
/// The command tree is rebuilt here, and only when tracing is on, so an ordinary run pays
/// nothing for a measurement it did not ask for.
pub fn record(matches: &clap::ArgMatches) {
    let Some(file) = std::env::var_os("FT_TRACE_COMMANDS") else {
        return;
    };
    let root = <crate::cli::Cli as clap::CommandFactory>::command();
    let Some(line) = line(&root, matches) else {
        return;
    };
    use std::io::Write;
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .and_then(|mut out| writeln!(out, "{line}"));
}

/// `path\tflags` for a parsed invocation, or nothing for a bare `42ctl`.
///
/// A flag counts only when it came from the command line: a default value or an environment
/// variable standing in for it is not the flag being exercised.
fn line(root: &clap::Command, matches: &clap::ArgMatches) -> Option<String> {
    let mut path = Vec::new();
    let (mut command, mut at) = (root, matches);
    while let Some((name, sub)) = at.subcommand() {
        command = command.find_subcommand(name)?;
        path.push(name.to_string());
        at = sub;
    }
    if path.is_empty() {
        return None;
    }
    let flags: Vec<String> = command
        .get_arguments()
        .filter(|arg| !arg.is_positional())
        .filter(|arg| at.value_source(arg.get_id().as_str()) == Some(ValueSource::CommandLine))
        .filter_map(flag_name)
        .collect();
    Some(format!("{}\t{}", path.join(" "), flags.join(" ")))
}

/// The spelling `help commands` prints for a flag: its long name, else its short one.
fn flag_name(arg: &clap::Arg) -> Option<String> {
    arg.get_long()
        .map(|long| format!("--{long}"))
        .or_else(|| arg.get_short().map(|short| format!("-{short}")))
}

#[cfg(test)]
mod tests {
    use super::line;
    use clap::CommandFactory;

    fn traced(argv: &str) -> Option<String> {
        let root = crate::cli::Cli::command();
        let matches = root
            .clone()
            .try_get_matches_from(argv.split_whitespace())
            .expect("parses");
        line(&root, &matches)
    }

    /// The path, then the flags given — and no value, whatever it was.
    #[test]
    fn a_trace_line_names_the_command_and_its_flags_but_no_value() {
        let got = traced("42ctl org member ls --org acme-secret -q --filter Role=member").unwrap();
        assert_eq!(got, "org member ls\t--org --filter --quiet");
        assert!(
            !got.contains("acme-secret") && !got.contains("Role"),
            "{got}"
        );
    }

    /// A positional argument is data, not a flag, and is never written.
    #[test]
    fn a_positional_argument_is_not_recorded() {
        assert_eq!(traced("42ctl vault get app/STRIPE").unwrap(), "vault get\t");
    }
}
