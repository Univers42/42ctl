/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   help.rs                                              :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl help [TOPIC]` (and bare `42ctl`) — the guided walkthrough. No topic prints the
//! overview (a version card, what the tool is, the first commands, command groups, topic
//! index); a topic name prints that topic; `commands` prints the reference generated from the
//! parser; a command name falls through to clap's own `--help` for it. The page is rendered
//! into one string and emitted once, so `42ctl help | less` is plain and pipe-safe.

use super::help_commands;
use super::help_topics::{OVERVIEW, TOPICS};
use crate::cli::Cli;
use crate::ui;
use clap::CommandFactory;
use std::fmt::Write;

/// The generated reference's line in the topic index; it has no static body to render.
const COMMANDS: (&str, &str) = ("commands", "every command and its arguments, in one list");

/// Print the overview, a topic, the command reference, or a command's help — or list the
/// topics on a miss.
pub fn run(topic: Option<&str>) -> anyhow::Result<()> {
    let Some(name) = topic.map(canonical) else {
        return ui::emit(&overview());
    };
    if name == COMMANDS.0 {
        return ui::emit(&format!("\n{}\n", help_commands::page()));
    }
    if let Some((_, summary, body)) = TOPICS.iter().find(|(n, _, _)| *n == name) {
        return ui::emit(&topic_page(name, summary, body));
    }
    command_help(name)
}

/// Map a retired topic name to the one that replaced it. `quickstart` grew into `kickoff`,
/// and the README, older installers and muscle memory all still say the old name.
fn canonical(name: &str) -> &str {
    match name {
        "quickstart" => "kickoff",
        other => other,
    }
}

/// The bare `42ctl` page: the version card, the overview text, and the topic index.
fn overview() -> String {
    let build = format!("{} · {}", env!("FT_TARGET"), env!("FT_GIT_SHA"));
    let mut page = String::from("\n");
    page.push_str(&ui::boxed(
        concat!("42ctl ", env!("CARGO_PKG_VERSION")),
        &["zero-knowledge secrets & identity for the 42 stack", &build],
    ));
    page.push('\n');
    render(&mut page, OVERVIEW);
    let index = TOPICS.iter().map(|(name, summary, _)| (*name, *summary));
    for (name, summary) in index.chain(std::iter::once(COMMANDS)) {
        let _ = writeln!(
            page,
            "    {}  {summary}",
            ui::accent(&format!("{name:<11}"))
        );
    }
    page.push_str("\n  Read one:   42ctl help <topic>\n  Any verb:   42ctl <command> --help\n\n");
    page
}

/// One topic: its title and summary, then its rendered body.
fn topic_page(name: &str, summary: &str, body: &str) -> String {
    let mut page = format!(
        "\n  {}  {}\n\n",
        ui::title(&format!("42ctl help {name}")),
        ui::dim(&format!("— {summary}"))
    );
    render(&mut page, body);
    page.push('\n');
    page
}

/// Render the topic markup: `## ` sections, `$ ` commands with dim `# …` comments,
/// `! ` warnings, and plain prose — every line indented two spaces. A blank line before
/// a section is absorbed (the section brings its own).
fn render(page: &mut String, body: &str) {
    let mut blank = false;
    for line in body.lines() {
        if line.is_empty() {
            blank = true;
            continue;
        }
        if let Some(section) = line.strip_prefix("## ") {
            page.push_str(&ui::section(section));
        } else {
            if blank {
                page.push('\n');
            }
            page.push_str(&line_of(line));
        }
        blank = false;
    }
}

/// One non-section line: a `$` command (bold, comment dimmed), a `!` warning, or prose.
fn line_of(line: &str) -> String {
    if let Some(command) = line.strip_prefix("$ ") {
        let (cmd, comment) = command.split_at(command.find("  #").unwrap_or(command.len()));
        format!(
            "    {} {}{}\n",
            ui::accent("$"),
            ui::bold(cmd),
            ui::dim(comment)
        )
    } else if let Some(warning) = line.strip_prefix("! ") {
        format!("    {} {warning}\n", ui::warn("!"))
    } else {
        format!("  {line}\n")
    }
}

/// Show help for a subcommand named `name`, or list the topics.
///
/// A command with subcommands gets the same grouped page `42ctl <name> --help` prints, so the
/// two doors onto one command's help do not show two different things.
fn command_help(name: &str) -> anyhow::Result<()> {
    if let Some(page) = super::help_grouped::requested(&["42ctl".to_string(), name.to_string()]) {
        return ui::emit(&page);
    }
    let mut root = Cli::command();
    match root.find_subcommand_mut(name) {
        Some(sub) => Ok(sub.print_long_help()?),
        None => {
            let mut names: Vec<&str> = TOPICS.iter().map(|(n, _, _)| *n).collect();
            names.push(COMMANDS.0);
            anyhow::bail!(
                "no topic or command '{name}' — topics: {}",
                names.join(", ")
            )
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use clap::error::ErrorKind;
    use clap::Parser;

    /// Every page of static help text, with a label for the failure message.
    fn bodies() -> Vec<(&'static str, &'static str)> {
        let topics = TOPICS.iter().map(|(name, _, body)| (*name, *body));
        std::iter::once(("overview", OVERVIEW))
            .chain(topics)
            .collect()
    }

    /// Split a shell line into words, honouring single quotes — all the quoting help uses.
    pub(in crate::cmd) fn words(line: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut word = String::new();
        let mut quoted = false;
        let mut started = false;
        for ch in line.chars() {
            if ch == '\'' {
                quoted = !quoted;
                started = true;
            } else if ch == ' ' && !quoted {
                if started {
                    words.push(std::mem::take(&mut word));
                }
                started = false;
            } else {
                word.push(ch);
                started = true;
            }
        }
        if started {
            words.push(word);
        }
        words
    }

    /// The `42ctl …` invocation a `$ ` line runs — after any pipe, before any comment — or
    /// `None` for a line that runs something else (`cd`, `curl`, `export`).
    fn invocation(line: &str) -> Option<&str> {
        let command = line.strip_prefix("$ ")?;
        let command = &command[..command.find("  #").unwrap_or(command.len())];
        let last = command.rsplit(" | ").next()?.trim();
        last.starts_with("42ctl ").then_some(last)
    }

    /// Split `$( … )` command substitutions out of a line: the line with each replaced by one
    /// plain word, plus the inner invocations, so both halves of a composed example are
    /// checked.
    ///
    /// `42ctl vault rm $(42ctl vault ls dev/ -q)` is two commands, and the outer one sees a
    /// single argument where the substitution stands. Parsing the line as written hands `rm`
    /// the inner command's flags, which is not what any shell does.
    pub(in crate::cmd) fn substitutions(line: &str) -> (String, Vec<String>) {
        let mut outer = String::with_capacity(line.len());
        let mut inner = Vec::new();
        let mut rest = line;
        while let Some(start) = rest.find("$(") {
            outer.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let Some(end) = after.find(')') else {
                outer.push_str(&rest[start..]);
                return (outer, inner);
            };
            outer.push_str("substituted");
            inner.push(after[..end].trim().to_string());
            rest = &after[end + 1..];
        }
        outer.push_str(rest);
        (outer, inner)
    }

    /// A composed example is two commands, and both are checked: the substitution collapses
    /// to one word for the outer parse, and the inner command is parsed on its own.
    #[test]
    fn a_command_substitution_is_parsed_as_two_commands() {
        let (outer, inner) = substitutions("42ctl vault rm $(42ctl vault ls dev/ -q)");
        assert_eq!(outer, "42ctl vault rm substituted");
        assert_eq!(inner, vec!["42ctl vault ls dev/ -q"]);

        let (plain, none) = substitutions("42ctl vault ls");
        assert_eq!(plain, "42ctl vault ls");
        assert!(none.is_empty(), "a line with no substitution is unchanged");

        let (broken, _) = substitutions("42ctl vault rm $(unterminated");
        assert!(
            broken.ends_with("$(unterminated"),
            "an unclosed one is left alone"
        );
    }

    /// Every example the help prints is a command the parser accepts. A renamed flag, a
    /// dropped verb, a missing required argument or a `…` standing in for one fails here
    /// instead of on a reader's first paste. It proves the SHAPE only: a sequence that parses
    /// but is refused at run time (a contract asked for before any session) is not caught.
    /// The count proves the haystack: a test that parsed nothing would also pass.
    #[test]
    fn every_example_command_parses() {
        let mut parsed = 0;
        for (page, body) in bodies() {
            for line in body.lines().filter_map(invocation) {
                let (outer, inner) = substitutions(line);
                for command in std::iter::once(outer).chain(inner) {
                    if !command.starts_with("42ctl ") {
                        continue;
                    }
                    match Cli::try_parse_from(words(&command)) {
                        Ok(_) => {}
                        Err(error) if error.kind() == ErrorKind::DisplayHelp => {}
                        Err(error) => panic!(
                            "`42ctl help {page}` shows a command that does not parse:\n  {line}\n  → {command}\n{error}"
                        ),
                    }
                    parsed += 1;
                }
            }
        }
        assert!(
            parsed >= 100,
            "only {parsed} example commands found — is the markup still `$ `?"
        );
    }

    #[test]
    fn every_topic_is_advertised_by_help() {
        let root = Cli::command();
        let after = root
            .get_after_help()
            .map(ToString::to_string)
            .unwrap_or_default();
        let help = root.find_subcommand("help").expect("help verb");
        let arg = help.get_positionals().next().expect("topic argument");
        let doc = arg.get_help().map(ToString::to_string).unwrap_or_default();
        for name in TOPICS.iter().map(|(n, _, _)| *n).chain([COMMANDS.0]) {
            assert!(
                after.contains(name),
                "`42ctl --help` does not name topic `{name}`"
            );
            assert!(
                doc.contains(name),
                "`42ctl help --help` does not name topic `{name}`"
            );
        }
    }

    /// The retired name still opens the page that replaced it, and that page exists.
    #[test]
    fn quickstart_still_opens_the_kickoff() {
        assert_eq!(canonical("quickstart"), "kickoff");
        assert!(TOPICS.iter().any(|(name, _, _)| *name == "kickoff"));
    }
}
