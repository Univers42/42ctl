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

//! `42ctl org github connect|link|sync` — org-scoped GitHub App operations. Each verb
//! sends the grobase session token (from `auth login --github`) to grobase, which
//! RBAC-checks it. Connect prints the install URL the operator opens; sync echoes the
//! upsert counts.

use crate::adapters::{github_org, session};
use crate::cli::{Org, OrgGithub};
use crate::profile::Config;
use crate::ui;
use anyhow::Context;

/// Dispatch an `org` subcommand for `profile`.
pub async fn run(cmd: &Org, profile: &str) -> anyhow::Result<()> {
    match cmd {
        Org::Github(gh) => github(gh, profile).await,
    }
}

/// Run an `org github` verb against grobase using the saved session token.
async fn github(cmd: &OrgGithub, profile: &str) -> anyhow::Result<()> {
    let endpoint = Config::load()?.endpoint(profile)?;
    let grobase = endpoint.otp_base().to_string();
    let token = session::load(profile)
        .context("not logged in to grobase — run `42ctl auth login --github` first")?;
    match cmd {
        OrgGithub::Connect { org } => {
            let res = github_org::connect_start(&grobase, &token, org).await?;
            ui::field("install_url", &res.install_url);
            ui::field("nonce", &res.nonce);
            ui::success("open the install URL, then run `org github sync` after installing");
        }
        OrgGithub::Link { org, github_org } => {
            github_org::link(&grobase, &token, org, github_org).await?;
            ui::success(&format!("linked GitHub org '{github_org}' to '{org}'"));
        }
        OrgGithub::Sync { org } => {
            let s = github_org::sync(&grobase, &token, org).await?;
            ui::field("repos", &s.repos.to_string());
            ui::field("teams", &s.teams.to_string());
            ui::field("members", &s.members.to_string());
            ui::field("roles_seeded", &s.roles_seeded.to_string());
            ui::success(&format!("synced GitHub → org '{org}'"));
        }
    }
    Ok(())
}
