/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope.rs                                            :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl vault env-init|sync-keys|scope-status` — the scope-key orchestration that bridges
//! grobase (membership + member pubkeys) and vault42 (the scope-key wraps). This file owns
//! the dispatch and the shared resolution: the grobase REST session, the env lookup, and the
//! per-grant pending-provision set (the union of every env grant's `missing` list). The
//! init flow lives in `scope_init`, the sync/status flows in `scope_sync`.

use crate::adapters::api::Session;
use crate::adapters::rbac::{grant, pubkey};
use crate::adapters::session;
use crate::cli::Vault;
use crate::cmd::{scope_init, scope_status, scope_sync};

/// The resolved orchestration context: the grobase base URL + session token and the
/// project/env identifiers a scope verb operates on (org id, project UUID, env id, name).
pub struct Ctx {
    pub grobase: String,
    pub token: String,
    pub org: String,
    pub project: String,
    pub env_id: String,
    pub env_name: String,
    pub scope_epoch: u32,
    pub scope_pubkey: Option<String>,
}

/// Route the three scope-key verbs, resolving their shared context first.
pub async fn run(session: &mut Session, cmd: &Vault, profile: &str) -> anyhow::Result<()> {
    match cmd {
        Vault::EnvInit { org, project, env } => {
            scope_init::env_init(session, &resolve(profile, org, project, env).await?).await
        }
        Vault::SyncKeys { org, project, env } => {
            scope_sync::sync_keys(session, &resolve(profile, org, project, env).await?).await
        }
        Vault::ScopeStatus { org, project, env } => {
            scope_status::scope_status(session, &resolve(profile, org, project, env).await?).await
        }
        _ => unreachable!("scope::run only handles the scope-key verbs"),
    }
}

/// Resolve the grobase session and the env (by name) into a `Ctx`. Errors if the env does
/// not exist under the project.
async fn resolve(profile: &str, org: &str, project: &str, env: &str) -> anyhow::Result<Ctx> {
    let (grobase, token) = session::connect(profile)?;
    let environments = pubkey::list_environments(&grobase, &token, project).await?;
    let found = environments
        .into_iter()
        .find(|e| e.name == env)
        .ok_or_else(|| anyhow::anyhow!("no environment '{env}' in project '{project}'"))?;
    Ok(Ctx {
        grobase,
        token,
        org: org.to_string(),
        project: project.to_string(),
        env_id: found.id,
        env_name: env.to_string(),
        scope_epoch: found.scope_epoch,
        scope_pubkey: found.scope_pubkey,
    })
}

/// Collect the env's pending-provision `(grant_id, user_id)` pairs: per env grant, the users
/// the server reports `missing` a wrap. A grant applies to the env when it targets this env
/// or is project-wide (`env_id = None`). The same user can appear under several grants (the
/// caller deposits one vault42 wrap per user but records it against each pending grant).
pub async fn env_pending(ctx: &Ctx) -> anyhow::Result<Vec<(String, String)>> {
    let grants = grant::list(&ctx.grobase, &ctx.token, &ctx.org, &ctx.project).await?;
    let mut pending: Vec<(String, String)> = Vec::new();
    for g in grants.iter().filter(|g| applies(g, &ctx.env_id)) {
        let f = grant::fulfilled(&ctx.grobase, &ctx.token, (&ctx.org, &ctx.project), &g.id).await?;
        for user in f.missing {
            pending.push((g.id.clone(), user));
        }
    }
    Ok(pending)
}

/// Whether a grant applies to `env_id`: a grant scoped to this env, or a project-wide grant.
fn applies(g: &crate::adapters::rbac::ProjectGrant, env_id: &str) -> bool {
    match &g.env_id {
        Some(id) => id == env_id,
        None => true,
    }
}
