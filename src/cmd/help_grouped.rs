/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   help_grouped.rs                                      :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Grouped `--help`, the way docker's reads: commands split into sections instead of one flat
//! list, at the root and under every noun.
//!
//! clap prints exactly one `Commands:` heading and offers no way to split it — `help_template`
//! does not reach subcommands and `next_help_heading` groups arguments, not commands. So the
//! help request is intercepted: `main` hands the failed parse here, this module resolves which
//! command was asked about, and renders the page itself when that command has children. A leaf
//! is handed straight back to clap, which keeps its exact option rendering, its styling and
//! its `-h` / `--help` distinction for every page where grouping would add nothing.

use crate::cli::sections::SECTIONS;
use crate::cli::Cli;
use crate::ui;
use clap::{Command, CommandFactory};

/// Column the one-line summaries are clipped at.
const WIDTH: usize = 96;

/// The grouped help `argv` asked for, or `None` when clap should answer instead.
///
/// `None` means one of two things, and both belong to clap: the path names a leaf, whose help
/// clap already renders better than a hand-rolled page would, or it names nothing at all, in
/// which case clap's "did you mean" is the right answer and inventing one here would lose it.
pub fn requested(argv: &[String]) -> Option<String> {
    let mut root = Cli::command();
    root.build();
    let path = command_path(argv, &root);
    let node = resolve(&root, &path)?;
    node.has_subcommands().then(|| page(node, &path))
}

/// The tokens that name a command: options and their values dropped, everything after `--`
/// ignored, so `42ctl --profile staging org --help` still resolves to `org`.
fn command_path(argv: &[String], root: &Command) -> Vec<String> {
    let valued = flags_taking_values(root);
    let mut path = Vec::new();
    let mut skip = false;
    for token in argv.iter().skip(1) {
        if token == "--" {
            break;
        }
        if std::mem::replace(&mut skip, false) {
            continue;
        }
        if token.starts_with('-') {
            skip = valued.iter().any(|flag| flag == token);
            continue;
        }
        path.push(token.clone());
    }
    path
}

/// Every spelling of a global option that consumes the token after it.
///
/// Without this `--profile staging org --help` would try to resolve `staging` as a command and
/// find nothing, so the page would silently fall back to clap's flat list.
fn flags_taking_values(root: &Command) -> Vec<String> {
    root.get_arguments()
        .filter(|arg| arg.is_global_set() && arg.get_action().takes_values())
        .flat_map(|arg| {
            let long = arg.get_long().map(|name| format!("--{name}"));
            let short = arg.get_short().map(|name| format!("-{name}"));
            long.into_iter().chain(short)
        })
        .collect()
}

/// Walk `path` down from `root`, matching a name or any of its aliases.
fn resolve<'a>(root: &'a Command, path: &[String]) -> Option<&'a Command> {
    path.iter().try_fold(root, |node, name| {
        node.get_subcommands()
            .find(|cmd| cmd.get_name() == name || cmd.get_all_aliases().any(|a| a == name))
    })
}

/// The page: usage, what the command is for, its commands by section, the global options, and
/// where to go next.
fn page(node: &Command, path: &[String]) -> String {
    let spelled = std::iter::once("42ctl")
        .chain(path.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    let mut out = format!(
        "\n{} {}\n",
        ui::title("Usage:"),
        ui::bold(&format!("{spelled} [OPTIONS] COMMAND"))
    );
    if let Some(about) = node.get_about() {
        out.push_str(&format!("\n{}\n", clip(&about.to_string(), WIDTH)));
    }
    for (title, commands) in grouped(node, path.is_empty()) {
        out.push_str(&list(&title, &commands));
    }
    out.push_str(&options(node));
    out.push_str(&footer(&spelled, path.is_empty()));
    out
}

/// The sections this node's children fall into.
///
/// The root reads its table; below it the split is computed, so a noun's page reads like
/// `docker container --help` with nothing to keep in step by hand.
fn grouped(node: &Command, is_root: bool) -> Vec<(String, Vec<&Command>)> {
    let visible: Vec<&Command> = node
        .get_subcommands()
        .filter(|cmd| !cmd.is_hide_set())
        .collect();
    if is_root {
        return SECTIONS
            .iter()
            .map(|(title, names)| {
                let picked = names
                    .iter()
                    .filter_map(|name| visible.iter().find(|cmd| cmd.get_name() == *name).copied())
                    .collect();
                ((*title).to_string(), picked)
            })
            .filter(|(_, picked): &(String, Vec<&Command>)| !picked.is_empty())
            .collect();
    }
    let (managed, verbs): (Vec<_>, Vec<_>) = visible.into_iter().partition(|c| c.has_subcommands());
    [("Management Commands", managed), ("Commands", verbs)]
        .into_iter()
        .filter(|(_, picked)| !picked.is_empty())
        .map(|(title, picked)| (title.to_string(), picked))
        .collect()
}

/// One titled section: each command's name, padded, then its one-line summary.
fn list(title: &str, commands: &[&Command]) -> String {
    let pad = commands
        .iter()
        .map(|cmd| cmd.get_name().chars().count())
        .max()
        .unwrap_or(0);
    let mut out = format!("\n{}\n", ui::title(&format!("{title}:")));
    for cmd in commands {
        let name = format!("{:<pad$}", cmd.get_name());
        let about = cmd.get_about().map(ToString::to_string).unwrap_or_default();
        out.push_str(&format!(
            "  {}  {}\n",
            ui::accent(&name),
            clip(&about, WIDTH.saturating_sub(pad + 4))
        ));
    }
    out
}

/// The options that apply everywhere, listed once at the bottom the way docker lists them.
///
/// `help` and `version` are generated by clap rather than declared, so they only exist once
/// the command has been built — which is why `requested` builds the tree before walking it.
fn options(node: &Command) -> String {
    let args: Vec<&clap::Arg> = node
        .get_arguments()
        .filter(|arg| arg.is_global_set() || arg.get_id() == "help" || arg.get_id() == "version")
        .collect();
    if args.is_empty() {
        return String::new();
    }
    let spellings: Vec<String> = args.iter().map(|arg| spelling(arg)).collect();
    let pad = spellings
        .iter()
        .map(|s| s.chars().count())
        .max()
        .unwrap_or(0);
    let mut out = format!("\n{}\n", ui::title("Global Options:"));
    for (arg, spelled) in args.iter().zip(&spellings) {
        let help = arg.get_help().map(ToString::to_string).unwrap_or_default();
        out.push_str(&format!(
            "  {:<pad$}  {}\n",
            spelled,
            clip(&help, WIDTH.saturating_sub(pad + 4))
        ));
    }
    out
}

/// One option as it is typed: `-p, --profile <NAME>`.
fn spelling(arg: &clap::Arg) -> String {
    let short = arg.get_short().map(|c| format!("-{c}, "));
    let long = arg
        .get_long()
        .map(|name| format!("--{name}"))
        .unwrap_or_default();
    let value = if arg.get_action().takes_values() {
        arg.get_value_names()
            .and_then(|names| names.first())
            .map_or_else(|| " <VALUE>".to_string(), |name| format!(" <{name}>"))
    } else {
        String::new()
    };
    format!("{}{long}{value}", short.unwrap_or_default())
}

/// Where to go next: the deeper help, and at the root the guided topics as well.
fn footer(spelled: &str, is_root: bool) -> String {
    let next = format!(
        "\n{}\n",
        ui::dim(&format!(
            "Run '{spelled} COMMAND --help' for more information on a command."
        ))
    );
    if !is_root {
        return next;
    }
    format!(
        "{next}{}\n",
        ui::dim(
            "Start here: 42ctl help kickoff · every command: 42ctl help commands · topics: 42ctl help"
        )
    )
}

/// Keep a summary to one line and inside `room` columns.
fn clip(text: &str, room: usize) -> String {
    let line = text.lines().next().unwrap_or_default();
    if line.chars().count() <= room {
        return line.to_string();
    }
    line.chars()
        .take(room.saturating_sub(1))
        .collect::<String>()
        + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Words that look like commands in `argv` but are not, dropped.
    #[test]
    fn a_global_option_and_its_value_are_not_mistaken_for_a_command() {
        let mut root = Cli::command();
        root.build();
        let path = |line: &str| {
            let argv: Vec<String> = line.split_whitespace().map(ToString::to_string).collect();
            command_path(&argv, &root)
        };
        assert_eq!(path("42ctl org --help"), vec!["org"]);
        assert_eq!(
            path("42ctl --profile staging org --help"),
            vec!["org"],
            "the value after a global option is not a command"
        );
        assert_eq!(
            path("42ctl --profile=staging org member --help"),
            vec!["org", "member"]
        );
        assert_eq!(
            path("42ctl org -- --help"),
            vec!["org"],
            "nothing after -- is a command"
        );
        assert!(
            path("42ctl --help").is_empty(),
            "the root has an empty path"
        );
    }

    /// A group is answered here; a leaf and a name that is not a command are handed to clap,
    /// whose option rendering and "did you mean" are better than anything rebuilt here.
    #[test]
    fn only_a_command_with_subcommands_is_answered_here() {
        let page = |line: &str| {
            let argv: Vec<String> = line.split_whitespace().map(ToString::to_string).collect();
            requested(&argv)
        };
        assert!(
            page("42ctl --help").is_some(),
            "the root groups its commands"
        );
        assert!(
            page("42ctl org --help").is_some(),
            "a noun groups its verbs"
        );
        assert!(
            page("42ctl org github --help").is_some(),
            "so does a nested one"
        );
        assert!(
            page("42ctl secrets --help").is_some(),
            "an alias resolves too"
        );
        assert!(
            page("42ctl org members --help").is_none(),
            "a leaf belongs to clap"
        );
        assert!(
            page("42ctl bogus --help").is_none(),
            "so does a name that is not one"
        );
    }

    /// Every visible command is reachable by reading: the root page names each top-level
    /// command, and each group's page names each of its children. A command that no section
    /// claims would exist and be undiscoverable.
    #[test]
    fn every_visible_command_is_named_on_its_parents_page() {
        let mut root = Cli::command();
        root.build();
        check_children(&root, &[]);
    }

    /// Assert `node`'s page lists each visible child, then recurse into the children that are
    /// themselves groups.
    fn check_children(node: &Command, path: &[String]) {
        if !node.has_subcommands() {
            return;
        }
        let rendered = plain(&page(node, path));
        for child in node.get_subcommands().filter(|c| !c.is_hide_set()) {
            assert!(
                rendered.contains(child.get_name()),
                "`42ctl {} --help` does not name `{}`",
                path.join(" "),
                child.get_name()
            );
            let mut deeper = path.to_vec();
            deeper.push(child.get_name().to_string());
            check_children(child, &deeper);
        }
    }

    /// Strip ANSI so an assertion reads the text, not the styling.
    fn plain(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars();
        while let Some(ch) = chars.next() {
            if ch != '\x1b' {
                out.push(ch);
                continue;
            }
            for escape in chars.by_ref() {
                if escape == 'm' {
                    break;
                }
            }
        }
        out
    }

    /// A summary longer than the room available is cut with an ellipsis rather than wrapping
    /// into the next command's line, which is what turns a section into a wall of text.
    #[test]
    fn a_long_summary_is_clipped_to_one_line() {
        assert_eq!(clip("short", 10), "short");
        assert_eq!(clip("first\nsecond", 20), "first", "only the first line");
        assert_eq!(clip("abcdefghij", 5), "abcd…");
    }
}
