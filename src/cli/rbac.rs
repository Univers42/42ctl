/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   rbac.rs                                              :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The RBAC verbs over the authority: `org`, `team`, `group`, `project`, `invite`. All of them
//! need a session (`42ctl auth login --password --email <mail>`, or `--github`) and act on the
//! org / project the flags name.
//!
//! Shaped the way docker shapes its tree: a thing that has members has a `member` noun with
//! `ls`, `add` and `rm` under it, and a project's grants are a `grant` noun of their own. The
//! compound verbs they replace (`add-member`, `revoke-grant` …) are still accepted through
//! `cli/legacy.rs`.

use clap::Subcommand;

/// `org` subcommands — org-scoped operations (RBAC + provider integrations).
#[derive(Subcommand)]
pub enum Org {
    /// Create an org
    Create {
        /// URL-safe identifier, e.g. `acme`
        #[arg(long, value_name = "SLUG")]
        slug: String,
        /// Display name
        #[arg(long, value_name = "TEXT")]
        name: String,
    },
    /// An org's members: ls, rm
    #[command(subcommand)]
    Member(OrgMember),
    /// Invite an email to an org with a role (prints the one-time token)
    Invite {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Invitee's email
        #[arg(long, value_name = "EMAIL")]
        email: String,
        /// Role to grant (owner, admin, member …)
        #[arg(long, value_name = "ROLE")]
        role: String,
    },
    /// Accept an org invite with its one-time token — `invite accept` is the same door
    #[command(hide = true)]
    AcceptInvite {
        /// The token printed by `org invite`
        #[arg(long, value_name = "TOKEN")]
        token: String,
    },
}

/// `org member` subcommands.
#[derive(Subcommand)]
pub enum OrgMember {
    /// List an org's members
    #[command(visible_alias = "list")]
    Ls {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Output shaping: --format / --filter
        #[command(flatten)]
        out: super::Output,
    },
    /// Remove members from an org, with every membership derived from it
    ///
    /// Administrators, or the member themselves — leaving is always allowed, so nobody can be
    /// trapped in an organisation. Their teams, groups, published public key and direct grants
    /// go with them.
    ///
    /// This removes AUTHORIZATION, not access already held: a scope key they hold stays
    /// readable until you `env keys rotate` the environments they could read.
    Rm {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// User ids or emails — repeat the flag, or list several after it
        ///
        /// `42ctl org member rm --org acme --user $(42ctl org member ls --org acme -q
        /// --filter Role=member)` removes everyone who is only a member. Each removal is
        /// authorised on its own; one refusal does not stop the rest.
        #[arg(long, required = true, num_args = 1.., value_name = "USER")]
        user: Vec<String>,
    },
}

/// `team` subcommands — team RBAC within an org.
#[derive(Subcommand)]
pub enum Team {
    /// Create a team under an org
    Create {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// URL-safe team identifier
        #[arg(long, value_name = "SLUG")]
        slug: String,
        /// Display name
        #[arg(long, value_name = "TEXT")]
        name: String,
    },
    /// List an org's teams
    #[command(visible_alias = "list")]
    Ls {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Output shaping: --format / --filter
        #[command(flatten)]
        out: super::Output,
    },
    /// A team's members: add, rm
    #[command(subcommand)]
    Member(TeamMember),
    /// Invite an email to a team (prints the one-time token)
    Invite {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Team slug
        #[arg(long, value_name = "SLUG")]
        team: String,
        /// Invitee's email
        #[arg(long, value_name = "EMAIL")]
        email: String,
        /// Role inside the team
        #[arg(long, default_value = "member", value_name = "ROLE")]
        role: String,
    },
    /// Grant a team a role on a project (optionally one environment only)
    Grant {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Team slug
        #[arg(long, value_name = "SLUG")]
        team: String,
        /// Project name
        #[arg(long, value_name = "NAME")]
        project: String,
        /// Project role: admin, write or read
        #[arg(long, value_name = "ROLE")]
        role: String,
        /// Restrict the grant to this environment
        #[arg(long, value_name = "NAME")]
        env: Option<String>,
    },
}

/// `team member` subcommands.
#[derive(Subcommand)]
pub enum TeamMember {
    /// Add a user to a team
    Add {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Team slug
        #[arg(long, value_name = "SLUG")]
        team: String,
        /// User id or email
        #[arg(long, value_name = "USER")]
        user: String,
        /// Role inside the team
        #[arg(long, default_value = "member", value_name = "ROLE")]
        role: String,
    },
    /// Remove members from a team, leaving their org membership intact
    ///
    /// Only the team's grants stop reaching them; a grant held directly still does.
    Rm {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Team slug
        #[arg(long, value_name = "SLUG")]
        team: String,
        /// User ids or emails — repeat the flag, or list several after it
        #[arg(long, required = true, num_args = 1.., value_name = "USER")]
        user: Vec<String>,
    },
}

/// `group` subcommands — project group operations.
#[derive(Subcommand)]
pub enum Group {
    /// Create a project's group (the server derives the name)
    Create {
        /// Project name
        #[arg(long, value_name = "NAME")]
        project: String,
    },
    /// A group's members: add, rm
    #[command(subcommand)]
    Member(GroupMember),
    /// Invite an email to a group (prints the one-time token)
    Invite {
        /// Group id
        #[arg(long, value_name = "ID")]
        group: String,
        /// Invitee's email
        #[arg(long, value_name = "EMAIL")]
        email: String,
    },
}

/// `group member` subcommands.
#[derive(Subcommand)]
pub enum GroupMember {
    /// Add a user to a group
    Add {
        /// Group id
        #[arg(long, value_name = "ID")]
        group: String,
        /// User id or email
        #[arg(long, value_name = "USER")]
        user: String,
    },
    /// Remove members from a group
    Rm {
        /// Group id
        #[arg(long, value_name = "ID")]
        group: String,
        /// User ids or emails — repeat the flag, or list several after it
        #[arg(long, required = true, num_args = 1.., value_name = "USER")]
        user: Vec<String>,
    },
}

/// `project` subcommands — projects themselves, and the grants on them.
#[derive(Subcommand)]
pub enum Project {
    /// [admin] Create a project under an org
    ///
    /// A project is the parent every environment, group and grant hangs off. Until one
    /// exists, `env create`, `team grant` and every `env` key verb answer 404.
    Create {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// URL-safe project identifier, e.g. `inception`
        #[arg(long, value_name = "SLUG")]
        slug: String,
        /// Display name
        #[arg(long, value_name = "NAME")]
        name: String,
    },
    /// List an org's projects
    #[command(visible_alias = "list")]
    Ls {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Output shaping: --format / --filter
        #[command(flatten)]
        out: super::Output,
    },
    /// A project's grants: ls, add, rm
    #[command(subcommand)]
    Grant(ProjectGrant),
}

/// `project grant` subcommands.
#[derive(Subcommand)]
pub enum ProjectGrant {
    /// List a project's live grants, with the ids `project grant rm` takes
    #[command(visible_alias = "list")]
    Ls {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Project slug or id
        #[arg(long, value_name = "NAME")]
        project: String,
        /// Output shaping: --format / --filter
        #[command(flatten)]
        out: super::Output,
    },
    /// Grant a user, or one of the project's groups, a role on a project (optionally one
    /// environment only)
    ///
    /// Name exactly one grantee. A group grant reaches the group's CURRENT members, and ends for
    /// anyone who leaves the group or the organization. A team is granted with `team grant`.
    #[command(group(clap::ArgGroup::new("grantee").required(true).args(["user", "group"])))]
    Add {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Project name
        #[arg(long, value_name = "NAME")]
        project: String,
        /// User id or email
        #[arg(long, value_name = "USER")]
        user: Option<String>,
        /// Group id (from `group create`), which must belong to this project
        #[arg(long, value_name = "ID")]
        group: Option<String>,
        /// Project role: admin, write or read
        #[arg(long, value_name = "ROLE")]
        role: String,
        /// Restrict the grant to this environment
        #[arg(long, value_name = "NAME")]
        env: Option<String>,
    },
    /// Revoke grants, so they authorize nobody from now on
    ///
    /// Find the ids with `project grant ls`. The row is kept with a revocation time, because
    /// "who used to be able to read this" outlives the grant; every read filters it out.
    ///
    /// This removes AUTHORIZATION, not access already held — `env keys rotate` the
    /// environment if a key they already hold matters.
    Rm {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Project slug or id
        #[arg(long, value_name = "NAME")]
        project: String,
        /// Grant ids, from `project grant ls` — repeat the flag, or list several after it
        #[arg(long, required = true, num_args = 1.., value_name = "ID")]
        grant: Vec<String>,
    },
}

/// `invite` subcommands — generalized invite operations.
#[derive(Subcommand)]
pub enum Invite {
    /// Accept an invite with its one-time token
    Accept {
        /// The token you were sent
        #[arg(long, value_name = "TOKEN")]
        token: String,
    },
    /// Show an invite by its id
    Show {
        /// Invite id
        #[arg(long, value_name = "ID")]
        id: String,
    },
}

#[cfg(test)]
mod tests {
    use crate::cli::Cli;
    use clap::Parser;

    fn parses(line: &str) -> bool {
        Cli::try_parse_from(line.split_whitespace()).is_ok()
    }

    /// A grant names exactly one grantee: a user or a group, never both, never neither.
    #[test]
    fn a_project_grant_names_exactly_one_grantee() {
        let base = "42ctl project grant add --org acme --project api --role read";
        assert!(parses(&format!("{base} --user a@x.io")));
        assert!(parses(&format!("{base} --group g1 --env prod")));
        assert!(
            !parses(&format!("{base} --user a@x.io --group g1")),
            "two grantees in one grant would leave one of them silently ignored"
        );
        assert!(!parses(base), "a grant to nobody is refused at the parser");
    }
}
