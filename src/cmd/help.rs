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

//! `42ctl help [TOPIC]` — the guided walkthrough. No topic prints the overview (what the
//! tool is, the three-command start, command groups, topic index); a topic name prints
//! that topic; a command name falls through to clap's own `--help` for it. The page is
//! rendered into one string and emitted once, so `42ctl help | less` is plain and
//! pipe-safe.

use super::help_topics::{OVERVIEW, TOPICS};
use crate::cli::Cli;
use crate::ui;
use clap::CommandFactory;
use std::fmt::Write;

/// Print the overview, a topic, or a command's help — or list the topics on a miss.
pub fn run(topic: Option<&str>) -> anyhow::Result<()> {
    let mut page = String::new();
    let Some(name) = topic else {
        let title = format!("42ctl {}", env!("CARGO_PKG_VERSION"));
        heading(
            &mut page,
            &title,
            "zero-knowledge secrets & identity for the 42 stack",
        );
        render(&mut page, OVERVIEW);
        for (name, summary, _) in TOPICS {
            let _ = writeln!(
                page,
                "    {}  {summary}",
                ui::accent(&format!("{name:<11}"))
            );
        }
        page.push_str(
            "\n  Read one:   42ctl help <topic>\n  Any verb:   42ctl <command> --help\n\n",
        );
        return ui::emit(&page);
    };
    if let Some((_, summary, body)) = TOPICS.iter().find(|(n, _, _)| *n == name) {
        heading(&mut page, &format!("42ctl help {name}"), summary);
        render(&mut page, body);
        page.push('\n');
        return ui::emit(&page);
    }
    command_help(name)
}

/// A bold title with a dim subtitle, surrounded by blank lines.
fn heading(page: &mut String, title: &str, subtitle: &str) {
    let _ = writeln!(
        page,
        "\n  {}  {}\n",
        ui::bold(title),
        ui::dim(&format!("— {subtitle}"))
    );
}

/// Render the topic markup: `## ` sections, `$ ` commands with dim `# …` comments,
/// `! ` warnings, and plain prose — every line indented two spaces.
fn render(page: &mut String, body: &str) {
    for line in body.lines() {
        let rendered = if let Some(section) = line.strip_prefix("## ") {
            format!("  {}", ui::accent(&section.to_uppercase()))
        } else if let Some(command) = line.strip_prefix("$ ") {
            format!("    {} {}", ui::dim("$"), command_line(command))
        } else if let Some(warning) = line.strip_prefix("! ") {
            format!("    {} {warning}", ui::warn("!"))
        } else if line.is_empty() {
            String::new()
        } else {
            format!("  {line}")
        };
        page.push_str(&rendered);
        page.push('\n');
    }
}

/// A command line with its trailing `# comment` dimmed, alignment preserved.
fn command_line(command: &str) -> String {
    match command.find("  #") {
        Some(at) => format!("{}{}", &command[..at], ui::dim(&command[at..])),
        None => command.to_string(),
    }
}

/// Fall through to clap's help for a subcommand named `name`, or list the topics.
fn command_help(name: &str) -> anyhow::Result<()> {
    let mut root = Cli::command();
    match root.find_subcommand_mut(name) {
        Some(sub) => Ok(sub.print_long_help()?),
        None => {
            let names: Vec<&str> = TOPICS.iter().map(|(n, _, _)| *n).collect();
            anyhow::bail!(
                "no topic or command '{name}' — topics: {}",
                names.join(", ")
            )
        }
    }
}
