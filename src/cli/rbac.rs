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

//! The RBAC verbs over grobase: `org`, `team`, `group`, `env`, `project`, `invite`. All
//! of them need a grobase session (`42ctl auth login --github`) and act on the
//! org / project the flags name.

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
    /// List an org's members
    Members {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
    },
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
    /// Remove a member from an org, with every membership derived from it
    ///
    /// Administrators, or the member themselves — leaving is always allowed, so nobody can be
    /// trapped in an organisation. Their teams, groups, published public key and direct grants
    /// go with them.
    ///
    /// This removes AUTHORIZATION, not access already held: a scope key they hold stays
    /// readable until you `vault rotate-scope` the environments they could read.
    RemoveMember {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// User id or email
        #[arg(long, value_name = "USER")]
        user: String,
    },
    /// Accept an org invite with its one-time token
    AcceptInvite {
        /// The token printed by `org invite`
        #[arg(long, value_name = "TOKEN")]
        token: String,
    },
    /// GitHub App connect / link / sync for an org (needs `auth login --github`)
    #[command(subcommand)]
    Github(OrgGithub),
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
    #[command(visible_alias = "ls")]
    List {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
    },
    /// Add a user to a team
    AddMember {
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
    /// Remove a member from a team, leaving their org membership intact
    ///
    /// Only the team's grants stop reaching them; a grant held directly still does.
    RemoveMember {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Team slug
        #[arg(long, value_name = "SLUG")]
        team: String,
        /// User id or email
        #[arg(long, value_name = "USER")]
        user: String,
    },
    /// Grant a team a role on a project (optionally one environment only)
    GrantProject {
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

/// `group` subcommands — project group operations.
#[derive(Subcommand)]
pub enum Group {
    /// Create a project's group (the server derives the name)
    Create {
        /// Project name
        #[arg(long, value_name = "NAME")]
        project: String,
    },
    /// Add a user to a group
    AddMember {
        /// Group id
        #[arg(long, value_name = "ID")]
        group: String,
        /// User id or email
        #[arg(long, value_name = "USER")]
        user: String,
    },
    /// Invite an email to a group (prints the one-time token)
    Invite {
        /// Group id
        #[arg(long, value_name = "ID")]
        group: String,
        /// Invitee's email
        #[arg(long, value_name = "EMAIL")]
        email: String,
    },
    /// Remove a member from a group
    RemoveMember {
        /// Group id
        #[arg(long, value_name = "ID")]
        group: String,
        /// User id or email
        #[arg(long, value_name = "USER")]
        user: String,
    },
}

/// `env` subcommands — per-project environments.
#[derive(Subcommand)]
pub enum Env {
    /// Create an environment under a project
    Create {
        /// Project name
        #[arg(long, value_name = "NAME")]
        project: String,
        /// Environment name (dev, staging, prod …)
        #[arg(long, value_name = "NAME")]
        name: String,
    },
    /// List a project's environments
    #[command(visible_alias = "ls")]
    List {
        /// Project name
        #[arg(long, value_name = "NAME")]
        project: String,
    },
}

/// `project` subcommands — projects themselves, and user-scoped grants on them.
#[derive(Subcommand)]
pub enum Project {
    /// [admin] Create a project under an org
    ///
    /// A project is the parent every environment, group and grant hangs off. Until one
    /// exists, `env create`, `team grant-project` and every scope-key verb answer 404.
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
    #[command(visible_alias = "ls")]
    List {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
    },
    /// List a project's live grants, with the ids `revoke-grant` takes
    Grants {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Project slug or id
        #[arg(long, value_name = "NAME")]
        project: String,
    },
    /// Revoke a grant, so it authorizes nobody from now on
    ///
    /// Find the id with `project grants`. The row is kept with a revocation time, because
    /// "who used to be able to read this" outlives the grant; every read filters it out.
    ///
    /// This removes AUTHORIZATION, not access already held — rotate the environment if a
    /// key they already hold matters.
    RevokeGrant {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Project slug or id
        #[arg(long, value_name = "NAME")]
        project: String,
        /// Grant id, from `project grants`
        #[arg(long, value_name = "ID")]
        grant: String,
    },
    /// Grant a user a role on a project (optionally one environment only)
    Grant {
        /// Org slug
        #[arg(long, value_name = "SLUG")]
        org: String,
        /// Project name
        #[arg(long, value_name = "NAME")]
        project: String,
        /// User id or email
        #[arg(long, value_name = "USER")]
        user: String,
        /// Project role: admin, write or read
        #[arg(long, value_name = "ROLE")]
        role: String,
        /// Restrict the grant to this environment
        #[arg(long, value_name = "NAME")]
        env: Option<String>,
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

/// `org github` subcommands.
#[derive(Subcommand)]
pub enum OrgGithub {
    /// Start connecting a GitHub App installation to ORG (prints the install URL + nonce)
    Connect {
        /// Org slug
        #[arg(value_name = "ORG")]
        org: String,
    },
    /// Link a GitHub organisation login to ORG
    Link {
        /// Org slug
        #[arg(value_name = "ORG")]
        org: String,
        /// The GitHub organisation's login name
        #[arg(value_name = "GITHUB_ORG")]
        github_org: String,
    },
    /// Sync GitHub teams / members / repos into ORG's RBAC
    Sync {
        /// Org slug
        #[arg(value_name = "ORG")]
        org: String,
    },
}
