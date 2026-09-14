/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   help_manual.rs                                       :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The manual in `docs/manual` is tested with the code it describes.
//!
//! A manual drifts silently: the old `docs/vault.md` showed `42ctl project grant --org …`, a
//! command that never parsed, for as long as it existed. These tests embed every chapter at compile
//! time, so a renamed command, a removed flag or a missing required argument in any `$ 42ctl …`
//! example fails the build, and the command index must name every command the parser accepts.

use crate::cli::Cli;
use crate::cmd::help::tests::{substitutions, words};
use crate::cmd::help_commands::leaves;
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};

/// A chapter of the manual: its file name, and its text embedded at compile time.
macro_rules! chapter {
    ($name:literal) => {
        ($name, include_str!(concat!("../../docs/manual/", $name)))
    };
}

/// Every chapter of the manual, by file name.
fn chapters() -> Vec<(&'static str, &'static str)> {
    vec![
        chapter!("README.md"),
        chapter!("01-overview.md"),
        chapter!("02-installing.md"),
        chapter!("03-first-session.md"),
        chapter!("04-configuration.md"),
        chapter!("05-identity-and-accounts.md"),
        chapter!("06-personal-secrets.md"),
        chapter!("07-project-trees.md"),
        chapter!("08-organisations.md"),
        chapter!("09-environments.md"),
        chapter!("10-credentials.md"),
        chapter!("11-output-and-scripting.md"),
        chapter!("12-cloud.md"),
        chapter!("13-walkthrough.md"),
        chapter!("14-security-model.md"),
        chapter!("15-diagnostics.md"),
        chapter!("A-commands.md"),
        chapter!("B-files-and-variables.md"),
        chapter!("C-glossary.md"),
    ]
}

/// The 42ctl invocations a `$ ` line of the manual runs.
///
/// A line is split at `|`, `&&`, `||` and `;`, each piece loses its redirections and any leading
/// `NAME=value` assignments, and `$( … )` substitutions are checked as commands of their own.
/// Pieces that do not start with `42ctl` belong to other programs and are left alone.
fn invocations(line: &str) -> Vec<String> {
    let Some(body) = line.trim_start().strip_prefix("$ ") else {
        return Vec::new();
    };
    let body = &body[..body.find("  #").unwrap_or(body.len())];
    let (outer, inner) = substitutions(body);
    std::iter::once(outer)
        .chain(inner)
        .flat_map(|chunk| pieces(&chunk))
        .filter(|piece| piece.starts_with("42ctl "))
        .collect()
}

/// One shell chunk cut at its operators, each piece stripped of redirections and assignments.
fn pieces(chunk: &str) -> Vec<String> {
    let mut parts = vec![chunk.to_string()];
    for operator in [" | ", " && ", " || ", "; "] {
        parts = parts
            .iter()
            .flat_map(|part| part.split(operator).map(str::to_string).collect::<Vec<_>>())
            .collect();
    }
    parts
        .iter()
        .map(|part| unassigned(unredirected(part)))
        .collect()
}

/// A piece up to its first redirection.
fn unredirected(piece: &str) -> &str {
    [" < ", " > ", " >> ", " 2>"]
        .iter()
        .filter_map(|redirection| piece.find(redirection))
        .min()
        .map_or(piece, |at| &piece[..at])
}

/// A piece without the `NAME=value` assignments in front of its command.
fn unassigned(piece: &str) -> String {
    piece
        .trim()
        .split(' ')
        .skip_while(|word| {
            word.split_once('=').is_some_and(|(name, _)| {
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every `$ 42ctl …` example in the manual parses. The count proves the haystack: a test that
/// found no example would also pass.
#[test]
fn every_manual_example_command_parses() {
    let mut parsed = 0;
    for (chapter, body) in chapters() {
        for line in body.lines() {
            for command in invocations(line) {
                match Cli::try_parse_from(words(&command)) {
                    Ok(_) => {}
                    Err(error) if error.kind() == ErrorKind::DisplayHelp => {}
                    Err(error) => panic!(
                        "docs/manual/{chapter} shows a command that does not parse:\n  {line}\n  → {command}\n{error}"
                    ),
                }
                parsed += 1;
            }
        }
    }
    assert!(
        parsed >= 250,
        "only {parsed} manual examples found — is the markup still `$ `?"
    );
}

/// The command index names every runnable command, measured against the parser.
#[test]
fn the_command_index_lists_every_command() {
    let index = chapters()
        .into_iter()
        .find(|(name, _)| *name == "A-commands.md")
        .map(|(_, body)| body)
        .expect("appendix A");
    let root = Cli::command();
    let paths: Vec<String> = leaves(&root, &[])
        .iter()
        .map(|(path, _)| path.join(" "))
        .collect();
    assert!(
        paths.len() > 80,
        "the parser walk found only {} commands",
        paths.len()
    );
    for path in paths {
        let listed =
            index.contains(&format!("| `{path} ")) || index.contains(&format!("| `{path}`"));
        assert!(listed, "`{path}` is missing from docs/manual/A-commands.md");
    }
}

/// The README's table of contents links every chapter, so no chapter is unreachable.
#[test]
fn the_contents_link_every_chapter() {
    let chapters = chapters();
    let readme = chapters[0].1;
    for (name, _) in chapters.iter().skip(1) {
        assert!(
            readme.contains(&format!("]({name})")),
            "README.md does not link {name}"
        );
    }
}

/// Shell operators, redirections and assignments are cut away, and substitutions are checked.
#[test]
fn a_manual_line_yields_the_42ctl_commands_it_runs() {
    assert_eq!(invocations("$ 42ctl version"), vec!["42ctl version"]);
    assert_eq!(
        invocations("$ printf x | 42ctl vault set a  # comment"),
        vec!["42ctl vault set a"]
    );
    assert_eq!(
        invocations("$ FT_S3_KEY=k FT_S3_SECRET=s 42ctl push"),
        vec!["42ctl push"]
    );
    assert_eq!(
        invocations("$ 42ctl vault get k > out.txt && echo done"),
        vec!["42ctl vault get k"]
    );
    assert_eq!(
        invocations("$ 42ctl vault rm $(42ctl vault ls dev/ -q)"),
        vec!["42ctl vault rm substituted", "42ctl vault ls dev/ -q"]
    );
    assert!(
        invocations("42ctl version").is_empty(),
        "output lines are not commands"
    );
}
