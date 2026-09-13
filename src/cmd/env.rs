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

//! `42ctl env` — per-project environments (the key-bearing scope grants can target), and what
//! a team shares through one.
//!
//! `create` and `ls` are control-plane records and need only a session. Every other verb seals
//! or opens something, so it unlocks the identity FIRST and then resolves the environment —
//! the order these verbs had when they were `vault set-env` and its siblings, kept so the
//! error a half-configured machine sees first is the one it always saw.

use crate::adapters::rbac::env;
use crate::adapters::session;
use crate::cli::Env;
use crate::ui;

/// Dispatch an `env` subcommand for `profile`.
pub async fn run(cmd: &Env, profile: &str) -> anyhow::Result<()> {
    match cmd {
        Env::Create { project, name } => {
            let (grobase, token) = session::connect(profile)?;
            create(&grobase, &token, project, name).await
        }
        Env::Ls { project, out } => {
            let (grobase, token) = session::connect(profile)?;
            list(&grobase, &token, project, out).await
        }
        _ => {
            let mut vault = crate::cmd::vault::open_session(profile).await?;
            crate::cmd::scope::run(&mut vault, cmd, profile).await
        }
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
    ui::render(&["ID", "Name"], rows, out.shape())
}
