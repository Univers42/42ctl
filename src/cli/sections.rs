/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   sections.rs                                          :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Which section of the root `--help` each top-level command belongs to.
//!
//! Only the root needs a table. "What you reach for daily" is a judgement no property of the
//! command tree can supply, so it is written down here and tested against the parser: every
//! visible top-level command appears in exactly one section, and every name here exists.
//! Below the root the split is computed instead — a command with children of its own manages
//! a resource, one without runs — so a new verb lands in the right place with no edit.

/// The root's sections, in the order `--help` prints them.
pub const SECTIONS: &[(&str, &[&str])] = &[
    (
        "Common Commands",
        &["auth", "push", "pull", "vault", "version"],
    ),
    (
        "Management Commands",
        &[
            "account", "config", "db", "env", "group", "invite", "keys", "note", "org", "project",
            "team",
        ],
    ),
    ("Commands", &["help", "update", "unseal"]),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::CommandFactory;

    /// A command missing from the table would be invisible in the grouped help, and one
    /// listed twice would print twice. Both are silent, so the parser is the referee.
    #[test]
    fn every_top_level_command_is_in_exactly_one_section() {
        let root = Cli::command();
        for cmd in root.get_subcommands().filter(|c| !c.is_hide_set()) {
            let homes: Vec<&str> = SECTIONS
                .iter()
                .filter(|(_, names)| names.contains(&cmd.get_name()))
                .map(|(title, _)| *title)
                .collect();
            assert_eq!(
                homes.len(),
                1,
                "`{}` is in {homes:?}, it belongs in exactly one section",
                cmd.get_name()
            );
        }
    }

    /// A name left behind by a rename would print a section entry for a command that no
    /// longer exists, or silently nothing at all.
    #[test]
    fn every_name_in_the_table_is_a_real_command() {
        let root = Cli::command();
        for (title, names) in SECTIONS {
            for name in *names {
                assert!(
                    root.get_subcommands().any(|cmd| cmd.get_name() == *name),
                    "`{name}` is listed under {title} but is not a command"
                );
            }
        }
    }
}
