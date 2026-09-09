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
//! overview (a version card, what the tool is, the three-command start, command groups,
//! topic index); a topic name prints that topic; a command name falls through to clap's
//! own `--help` for it. The page is rendered into one string and emitted once, so
//! `42ctl help | less` is plain and pipe-safe.

use super::help_topics::{OVERVIEW, TOPICS};
use crate::cli::Cli;
use crate::ui;
use clap::CommandFactory;
use std::fmt::Write;

/// Print the overview, a topic, or a command's help — or list the topics on a miss.
pub fn run(topic: Option<&str>) -> anyhow::Result<()> {
    let mut page = String::from("\n");
    let Some(name) = topic else {
        let build = format!("{} · {}", env!("FT_TARGET"), env!("FT_GIT_SHA"));
        page.push_str(&ui::boxed(
            concat!("42ctl ", env!("CARGO_PKG_VERSION")),
            &["zero-knowledge secrets & identity for the 42 stack", &build],
        ));
        page.push('\n');
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
        let _ = writeln!(
            page,
            "  {}  {}\n",
            ui::title(&format!("42ctl help {name}")),
            ui::dim(&format!("— {summary}"))
        );
        render(&mut page, body);
        page.push('\n');
        return ui::emit(&page);
    }
    command_help(name)
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
