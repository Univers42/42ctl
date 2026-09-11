/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   env.rs                                              :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl env` — per-project environments (the key-bearing scope grants can target):
//! create and list. Authenticates with the grobase session token from `auth login --github`.

use crate::adapters::rbac::env;
use crate::adapters::session;
use crate::cli::Env;
use crate::ui;

/// Dispatch an `env` subcommand for `profile`.
pub async fn run(cmd: &Env, profile: &str) -> anyhow::Result<()> {
    let (grobase, token) = session::connect(profile)?;
    match cmd {
        Env::Create { project, name } => create(&grobase, &token, project, name).await,
        Env::List { project, out } => list(&grobase, &token, project, out).await,
    }
}

/// Create an environment under a project and print its id.
async fn create(grobase: &str, token: &str, project: &str, name: &str) -> anyhow::Result<()> {
    let e = env::create(grobase, token, project, name).await?;
    ui::field("id", &e.id);
    ui::success(&format!(
        "created environment '{}' in project '{project}'",
        e.name
    ));
    Ok(())
}

/// List a project's environments: `ID Name`.
async fn list(
    grobase: &str,
    token: &str,
    project: &str,
    out: &crate::cli::Output,
) -> anyhow::Result<()> {
    let rows = env::list(grobase, token, project)
        .await?
        .iter()
        .map(|e| serde_json::json!({"ID": e.id, "Name": e.name}))
        .collect();
    ui::render(&["ID", "Name"], rows, out.format.as_deref(), &out.filter)
}
