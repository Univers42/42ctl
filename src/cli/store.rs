/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   store.rs                                             :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The personal-store verbs: `note` (sealed project notes) and `db` (encrypted records).

use clap::Subcommand;

/// `note` subcommands — project-scoped (resolve the project from `.42ctl` or `--project`).
#[derive(Subcommand)]
pub enum Note {
    /// Seal a note (stdin, or --file) at PATH within the project
    Add {
        /// Note path, e.g. `onboarding.md`
        path: String,
        /// Project name (default: the `.42ctl/` marker in the current directory)
        #[arg(long, value_name = "NAME")]
        project: Option<String>,
        /// Read the note from this file instead of stdin
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
    },
    /// Fetch and decrypt the note at PATH to stdout
    Get {
        /// Note path
        path: String,
        /// Project name (default: the `.42ctl/` marker in the current directory)
        #[arg(long, value_name = "NAME")]
        project: Option<String>,
    },
    /// List the project's notes
    #[command(visible_alias = "list")]
    Ls {
        /// Project name (default: the `.42ctl/` marker in the current directory)
        #[arg(long, value_name = "NAME")]
        project: Option<String>,
        /// Output shaping: --format / --filter
        #[command(flatten)]
        out: super::Output,
    },
    /// Remove the note at PATH
    Rm {
        /// Note path
        path: String,
        /// Project name (default: the `.42ctl/` marker in the current directory)
        #[arg(long, value_name = "NAME")]
        project: Option<String>,
    },
}

/// `db` subcommands.
#[derive(Subcommand)]
pub enum Db {
    /// Read one encrypted record and decrypt it locally
    Get {
        /// Record path
        path: String,
    },
    /// List readable records under a prefix
    #[command(visible_alias = "list")]
    Ls {
        /// Only paths starting with this prefix
        #[arg(default_value = "", value_name = "PREFIX")]
        prefix: String,
        /// Output shaping: --format / --filter
        #[command(flatten)]
        out: super::Output,
    },
}
