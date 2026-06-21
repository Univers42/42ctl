/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   team.rs                                             :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl team` — team RBAC within an org. This file owns the dispatch + the create/list
//! verbs; the membership/grant verbs live in `team_members.rs`. Authenticates with the
//! grobase session token from `auth login --github`.

use crate::adapters::rbac::team;
use crate::adapters::session;
use crate::cli::Team;
use crate::cmd::team_members;
use crate::ui;

/// Dispatch a `team` subcommand for `profile`.
pub async fn run(cmd: &Team, profile: &str) -> anyhow::Result<()> {
    let (grobase, token) = session::connect(profile)?;
    match cmd {
        Team::Create { org, slug, name } => create(&grobase, &token, (org, slug, name)).await,
        Team::List { org } => list(&grobase, &token, org).await,
        _ => team_members::run(cmd, &grobase, &token).await,
    }
}

/// Create a team and print its id.
async fn create(grobase: &str, token: &str, spec: (&str, &str, &str)) -> anyhow::Result<()> {
    let (org, slug, name) = spec;
    let created = team::create(grobase, token, org, slug, name).await?;
    ui::field("id", &created.id);
    ui::success(&format!(
        "created team '{}' ({})",
        created.name, created.slug
    ));
    Ok(())
}

/// List an org's teams as an `id slug name` table.
async fn list(grobase: &str, token: &str, org: &str) -> anyhow::Result<()> {
    let teams = team::list(grobase, token, org).await?;
    let rows: Vec<Vec<String>> = teams
        .iter()
        .map(|t| vec![t.id.clone(), t.slug.clone(), t.name.clone()])
        .collect();
    ui::table(&["id", "slug", "name"], &rows);
    Ok(())
}
