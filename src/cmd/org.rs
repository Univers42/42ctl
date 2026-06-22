/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   org.rs                                              :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl org` — org-scoped RBAC (create / members / invite / accept-invite) plus the
//! GitHub App verbs. Each call sends the grobase session token (from `auth login --github`)
//! to grobase, which RBAC-checks it. Invites print the one-time cleartext token.

use crate::adapters::rbac::org;
use crate::adapters::{github_org, session};
use crate::cli::{Org, OrgGithub};
use crate::profile::Config;
use crate::ui;
use anyhow::Context;

/// Dispatch an `org` subcommand for `profile`.
pub async fn run(cmd: &Org, profile: &str) -> anyhow::Result<()> {
    match cmd {
        Org::Github(gh) => github(gh, profile).await,
        _ => rbac(cmd, profile).await,
    }
}

/// Run an org RBAC verb against grobase using the saved session token.
async fn rbac(cmd: &Org, profile: &str) -> anyhow::Result<()> {
    let (grobase, token) = session::connect(profile)?;
    match cmd {
        Org::Create { slug, name } => {
            let o = org::create(&grobase, &token, slug, name).await?;
            ui::field("id", &o.id);
            ui::success(&format!("created org '{}' ({})", o.name, o.slug));
        }
        Org::Members { org } => members(&grobase, &token, org).await?,
        Org::Invite { org, email, role } => {
            let inv = org::invite(&grobase, &token, org, email, role).await?;
            ui::field("invite_id", &inv.id);
            ui::field("token", &inv.token);
            ui::success(&format!("invited {email} to org '{org}' as '{role}'"));
        }
        Org::AcceptInvite { token: invite } => {
            org::accept_invite(&grobase, &token, invite).await?;
            ui::success("accepted org invite");
        }
        Org::Github(_) => unreachable!("github is handled before rbac"),
    }
    Ok(())
}

/// Print an org's members as a `user_id role joined` table.
async fn members(grobase: &str, token: &str, org_id: &str) -> anyhow::Result<()> {
    let list = org::members(grobase, token, org_id).await?;
    let rows: Vec<Vec<String>> = list
        .iter()
        .map(|m| vec![m.user_id.clone(), m.role.clone(), m.created_at.clone()])
        .collect();
    ui::table(&["user_id", "role", "joined"], &rows);
    Ok(())
}

/// Run an `org github` verb against grobase using the saved session token.
async fn github(cmd: &OrgGithub, profile: &str) -> anyhow::Result<()> {
    let endpoint = Config::load()?.endpoint(profile)?;
    let grobase = endpoint.otp_base().to_string();
    let token = session::load(profile)
        .context("not logged in to grobase — run `42ctl auth login --github` first")?;
    github_dispatch(cmd, &grobase, &token).await
}

/// Execute one resolved `org github` verb and report its result.
async fn github_dispatch(cmd: &OrgGithub, grobase: &str, token: &str) -> anyhow::Result<()> {
    match cmd {
        OrgGithub::Connect { org } => {
            let res = github_org::connect_start(grobase, token, org).await?;
            ui::field("install_url", &res.install_url);
            ui::field("nonce", &res.nonce);
            ui::success("open the install URL, then run `org github sync` after installing");
        }
        OrgGithub::Link { org, github_org } => {
            github_org::link(grobase, token, org, github_org).await?;
            ui::success(&format!("linked GitHub org '{github_org}' to '{org}'"));
        }
        OrgGithub::Sync { org } => {
            let s = github_org::sync(grobase, token, org).await?;
            ui::field("repos", &s.repos.to_string());
            ui::field("teams", &s.teams.to_string());
            ui::field("members", &s.members.to_string());
            ui::field("roles_seeded", &s.roles_seeded.to_string());
            ui::success(&format!("synced GitHub → org '{org}'"));
        }
    }
    Ok(())
}
