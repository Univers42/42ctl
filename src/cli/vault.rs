/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   vault.rs                                             :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `vault` / `secrets` — YOUR secrets, sealed to your own identity. What a team shares through
//! an environment is under `env` (`env secret`, `env push`, `env keys` …). Every seal/open is
//! local; the server only ever sees ciphertext.

use clap::Subcommand;

/// `vault` / `secrets` subcommands.
#[derive(Subcommand)]
pub enum Vault {
    /// Fetch a secret and decrypt it locally to stdout
    Get {
        /// Secret path, e.g. `app/DATABASE_URL`
        path: String,
        /// Read this version instead of the latest (0 = latest)
        #[arg(long, default_value_t = 0, value_name = "N")]
        version: u64,
    },
    /// Seal a value (stdin, or --file) and store it at PATH
    Set {
        /// Secret path, e.g. `app/DATABASE_URL`
        path: String,
        /// Read the value from this file instead of stdin
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
    },
    /// List your secrets under an optional prefix
    ///
    /// 42ctl keeps its own records in the vault too — notes, and a push's manifest and chunk
    /// lists, under `__42ctl/`. They are left out unless --all, because this listing feeds
    /// `vault rm`, and removing one of them loses the notes or the pushed tree it holds.
    #[command(visible_alias = "list")]
    Ls {
        /// Only paths starting with this prefix
        #[arg(default_value = "", value_name = "PREFIX")]
        prefix: String,
        /// Also list 42ctl's own records under `__42ctl/`
        #[arg(short, long)]
        all: bool,
        /// Output shaping: --format / --filter
        #[command(flatten)]
        out: super::Output,
    },
    /// Remove one or more secrets
    ///
    /// Every path is attempted even if an earlier one fails, and the command fails once at
    /// the end naming what did not go — so `42ctl vault rm $(42ctl vault ls -q)` tells you
    /// exactly which paths to retry rather than stopping at the first stale one. A path under
    /// `__42ctl/` is 42ctl's own record and is refused: `note rm` removes a note.
    Rm {
        /// Secret paths
        #[arg(required = true, num_args = 1.., value_name = "PATH")]
        paths: Vec<String>,
    },
    /// Remove stored chunks that no version of any manifest still references
    ///
    /// A dry run unless --apply, and never touches a chunk younger than the grace period:
    /// an interrupted push has already uploaded chunks no manifest names yet, and resuming
    /// it is exactly what collecting them too early would make impossible.
    Gc {
        /// Actually delete (without this flag, only report what would be collected)
        #[arg(long)]
        apply: bool,
        /// Hours a chunk must have existed before it can be collected (default 24)
        #[arg(long, value_name = "HOURS")]
        grace_hours: Option<i64>,
    },
    /// Re-seal a secret under a fresh data key
    Rotate {
        /// Secret path
        path: String,
    },
    /// Re-seal a secret so another identity can read it
    Share {
        /// Secret path
        path: String,
        /// Recipient's public address (from their `42ctl keys export-pub`)
        #[arg(long, value_name = "ADDRESS")]
        to: String,
    },
    /// Stream this identity's tamper-evident audit chain
    Audit {
        /// Only entries after this Unix timestamp
        #[arg(long, default_value_t = 0, value_name = "EPOCH")]
        since: i64,
    },
    /// Import a .env file, sealing each KEY=VALUE as KEY, or as PREFIX/KEY with --prefix
    ///
    /// Without a prefix every key lands at the top of your vault, so two files that both define
    /// `DATABASE_URL` overwrite each other's; `--prefix app` keeps them apart and is what
    /// `vault export --prefix app` reads back.
    Import {
        /// Path to the .env file
        #[arg(value_name = "FILE")]
        source: String,
        /// Store each key under this prefix, as PREFIX/KEY
        #[arg(long, default_value = "", value_name = "PREFIX")]
        prefix: String,
    },
    /// Export your secrets under a prefix as KEY=value lines
    Export {
        /// Only paths starting with this prefix
        #[arg(long, default_value = "", value_name = "PREFIX")]
        prefix: String,
    },
}

#[cfg(test)]
mod tests {
    use crate::cli::Cli;
    use clap::Parser;

    /// `vault import` stores under a prefix only when asked, and `--prefix` is what
    /// `vault export --prefix` reads back.
    #[test]
    fn vault_import_takes_a_prefix() {
        let parses = |line: &str| Cli::try_parse_from(line.split_whitespace()).is_ok();
        assert!(parses("42ctl vault import srcs/.env"));
        assert!(parses("42ctl vault import srcs/.env --prefix inception"));
    }
}
