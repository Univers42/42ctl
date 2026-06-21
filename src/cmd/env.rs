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
        Env::List { project } => list(&grobase, &token, project).await,
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

/// List a project's environments as an `id name` table.
async fn list(grobase: &str, token: &str, project: &str) -> anyhow::Result<()> {
    let envs = env::list(grobase, token, project).await?;
    let rows: Vec<Vec<String>> = envs
        .iter()
        .map(|e| vec![e.id.clone(), e.name.clone()])
        .collect();
    ui::table(&["id", "name"], &rows);
    Ok(())
}
