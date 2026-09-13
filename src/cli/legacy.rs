/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   legacy.rs                                            :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/09/13 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/09/13 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The spellings the command tree used before it was reshaped into nouns and verbs.
//!
//! `vault get-env` became `env secret get`, `org remove-member` became `org member rm`, and so
//! on. Scripts, Makefiles and the QA battery were written against the old names, and a rename
//! that breaks them on upgrade is a rename nobody can adopt — so for a while both are
//! accepted. The old form is rewritten into the new one BEFORE clap sees the arguments.
//!
//! A rewrite rather than clap aliases, because an alias cannot change a command's depth:
//! `vault get-env` is two words and `env secret get` is three, under a different noun.
//! Rewriting also keeps the old names out of every help page, so nobody learns them anew.
//!
//! Every rename here is a pure change of PATH: the flags and the handler are the ones the old
//! spelling had, and the only output that differs is text that used to name an old verb. That is
//! what makes a rewrite safe — anything that also changed behaviour would not belong here.

/// Old command path → the path that replaced it. Flags are unchanged in every row.
const RENAMED: &[(&[&str], &[&str])] = &[
    (&["vault", "env-init"], &["env", "init"]),
    (&["vault", "sync-keys"], &["env", "keys", "sync"]),
    (&["vault", "scope-status"], &["env", "keys", "ls"]),
    (&["vault", "rotate-scope"], &["env", "keys", "rotate"]),
    (&["vault", "set-env"], &["env", "secret", "set"]),
    (&["vault", "get-env"], &["env", "secret", "get"]),
    (&["vault", "push-env"], &["env", "push"]),
    (&["vault", "pull-env"], &["env", "pull"]),
    (&["vault", "ls-env"], &["env", "files"]),
    (&["org", "members"], &["org", "member", "ls"]),
    (&["org", "remove-member"], &["org", "member", "rm"]),
    (&["team", "add-member"], &["team", "member", "add"]),
    (&["team", "remove-member"], &["team", "member", "rm"]),
    (&["team", "grant-project"], &["team", "grant"]),
    (&["group", "add-member"], &["group", "member", "add"]),
    (&["group", "remove-member"], &["group", "member", "rm"]),
    (&["project", "grants"], &["project", "grant", "ls"]),
    (&["project", "revoke-grant"], &["project", "grant", "rm"]),
];

/// The one global option that takes a separate value, so its value is not read as a command.
const VALUED_GLOBAL: &str = "--profile";

/// An old spelling, rewritten.
pub struct Rewritten {
    /// The arguments to parse instead.
    pub argv: Vec<String>,
    /// The command path as it was typed, e.g. `vault get-env`.
    pub old: String,
    /// The command path it now is, e.g. `env secret get`.
    pub new: String,
}

/// Rewrite `argv` when it uses an old command path; `None` when it does not.
pub fn rewrite(argv: &[String]) -> Option<Rewritten> {
    let at = positionals(argv, 2);
    let [noun, verb] = at[..] else {
        return None;
    };
    let typed = [canonical_noun(&argv[noun]), argv[verb].as_str()];
    if typed == ["project", "grant"] {
        return project_grant(argv, verb);
    }
    let (_, new) = RENAMED.iter().find(|(old, _)| **old == typed)?;
    Some(Rewritten {
        argv: spliced(argv, (noun, verb), new),
        old: format!("{} {}", argv[noun], argv[verb]),
        new: new.join(" "),
    })
}

/// `project grant --user …` was the verb; `project grant` is now a noun whose verbs are
/// `add`, `ls` and `rm`. A flag straight after `grant` can only be the old form.
///
/// `--help` is left alone: in the new tree it asks for the `grant` page, which is the answer.
fn project_grant(argv: &[String], verb: usize) -> Option<Rewritten> {
    let next = argv.get(verb + 1)?;
    if !next.starts_with('-') || next == "--help" || next == "-h" {
        return None;
    }
    let mut out = argv.to_vec();
    out.insert(verb + 1, "add".to_string());
    Some(Rewritten {
        argv: out,
        old: "project grant".to_string(),
        new: "project grant add".to_string(),
    })
}

/// `secrets` is `vault`'s alias, and an alias must rewrite like the name it stands for.
fn canonical_noun(noun: &str) -> &str {
    if noun == "secrets" {
        return "vault";
    }
    noun
}

/// The indices of the first `wanted` command words, stepping over options and `--profile`'s
/// value, and stopping at `--` so nothing after it is ever read as a command.
fn positionals(argv: &[String], wanted: usize) -> Vec<usize> {
    let mut found = Vec::new();
    let mut value_next = false;
    for (index, arg) in argv.iter().enumerate().skip(1) {
        if value_next {
            value_next = false;
        } else if arg == "--" || found.len() == wanted {
            break;
        } else if arg == VALUED_GLOBAL {
            value_next = true;
        } else if !arg.starts_with('-') {
            found.push(index);
        }
    }
    found
}

/// `argv` with the noun at `at.0` and the verb at `at.1` replaced by `new`, keeping whatever
/// the operator put between and after them in place.
fn spliced(argv: &[String], at: (usize, usize), new: &[&str]) -> Vec<String> {
    let (noun, verb) = at;
    let owned = |words: &[&str]| words.iter().map(ToString::to_string).collect::<Vec<_>>();
    let mut out = argv[..noun].to_vec();
    out.extend(owned(&new[..1]));
    out.extend_from_slice(&argv[noun + 1..verb]);
    out.extend(owned(&new[1..]));
    out.extend_from_slice(&argv[verb + 1..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(ToString::to_string).collect()
    }

    fn rewritten(line: &str) -> Option<String> {
        rewrite(&words(line)).map(|done| done.argv.join(" "))
    }

    #[test]
    fn every_old_path_becomes_its_new_one_with_the_flags_in_place() {
        for (old, new) in RENAMED {
            let line = format!("42ctl {} --org acme PATH", old.join(" "));
            let want = format!("42ctl {} --org acme PATH", new.join(" "));
            assert_eq!(rewritten(&line).as_deref(), Some(want.as_str()), "{line}");
        }
    }

    #[test]
    fn the_notice_names_both_spellings() {
        let done = rewrite(&words("42ctl vault get-env --org a x")).expect("rewritten");
        assert_eq!(done.old, "vault get-env");
        assert_eq!(done.new, "env secret get");
    }

    #[test]
    fn the_profile_flag_and_its_value_are_stepped_over_wherever_they_sit() {
        assert_eq!(
            rewritten("42ctl --profile prod vault set-env --org a x").as_deref(),
            Some("42ctl --profile prod env secret set --org a x")
        );
        assert_eq!(
            rewritten("42ctl vault --profile prod set-env --org a x").as_deref(),
            Some("42ctl env --profile prod secret set --org a x")
        );
        assert_eq!(
            rewritten("42ctl --profile=prod org members --org a").as_deref(),
            Some("42ctl --profile=prod org member ls --org a")
        );
    }

    #[test]
    fn a_profile_named_like_a_command_is_not_mistaken_for_one() {
        assert_eq!(
            rewritten("42ctl --profile vault get-env --org a x").as_deref(),
            None,
            "`vault` is the profile's value here, so the command is `get-env`, which is no noun"
        );
    }

    #[test]
    fn the_new_spellings_and_unrelated_commands_are_left_alone() {
        for line in [
            "42ctl env secret get --org a x",
            "42ctl org member rm --org a --user b",
            "42ctl vault get app/KEY",
            "42ctl vault get get-env",
            "42ctl push",
            "42ctl",
        ] {
            assert!(rewritten(line).is_none(), "{line}");
        }
    }

    #[test]
    fn the_secrets_alias_rewrites_like_vault() {
        assert_eq!(
            rewritten("42ctl secrets ls-env --org a").as_deref(),
            Some("42ctl env files --org a")
        );
    }

    #[test]
    fn nothing_after_a_double_dash_is_read_as_a_command() {
        assert!(rewritten("42ctl -- vault get-env").is_none());
    }

    /// A rewrite is only worth something if what it produces parses. Each line is an old
    /// invocation carrying every flag its command takes, spelled the way scripts spell it.
    #[test]
    fn every_old_invocation_parses_once_rewritten() {
        use clap::Parser;
        let scope = "--org acme --project api --env prod";
        let lines = [
            format!("vault env-init {scope}"),
            format!("vault sync-keys {scope}"),
            format!("vault scope-status {scope} --format json"),
            format!("vault rotate-scope {scope}"),
            format!("vault set-env {scope} db/URL"),
            format!("vault get-env {scope} db/URL"),
            format!("vault push-env {scope} --private *.key --label app=web"),
            format!("vault pull-env {scope} --only secrets/* --apply --backup"),
            format!("vault ls-env {scope} --filter Private=true"),
            format!("secrets ls-env {scope} -q"),
            "org members --org acme -q".to_string(),
            "org remove-member --org acme --user a@x.io b@x.io".to_string(),
            "team add-member --org acme --team core --user a@x.io --role member".to_string(),
            "team remove-member --org acme --team core --user a@x.io".to_string(),
            "team grant-project --org acme --team core --project api --role read --env prod"
                .to_string(),
            "group add-member --group g1 --user a@x.io".to_string(),
            "group remove-member --group g1 --user a@x.io b@x.io".to_string(),
            "project grants --org acme --project api --format json".to_string(),
            "project revoke-grant --org acme --project api --grant g1 g2".to_string(),
            "project grant --org acme --project api --user a@x.io --role write --env prod"
                .to_string(),
            format!("--profile prod vault get-env {scope} db/URL"),
        ];
        for line in lines {
            let argv = words(&format!("42ctl {line}"));
            let done = rewrite(&argv).unwrap_or_else(|| panic!("not rewritten: {line}"));
            if let Err(error) = crate::cli::Cli::try_parse_from(&done.argv) {
                panic!("{line}\n  → {}\n{error}", done.argv.join(" "));
            }
        }
    }

    #[test]
    fn project_grant_gains_add_only_where_it_was_the_old_verb() {
        assert_eq!(
            rewritten("42ctl project grant --org a --user u --role read").as_deref(),
            Some("42ctl project grant add --org a --user u --role read")
        );
        for line in [
            "42ctl project grant ls --org a",
            "42ctl project grant add --org a",
            "42ctl project grant --help",
            "42ctl project grant -h",
            "42ctl project grant",
        ] {
            assert!(rewritten(line).is_none(), "{line}");
        }
    }
}
