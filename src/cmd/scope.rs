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
use crate::adapters::rbac::{grant, org, pubkey, GrantScope};
use crate::adapters::session;
use crate::cli::Vault;
use crate::cmd::{scope_init, scope_rotate, scope_secret, scope_status, scope_sync, scope_tree};

/// The resolved orchestration context: the base URL + session token and the project/env
/// identifiers a scope verb operates on.
///
/// `org` is the reference the user typed and is what goes into REST paths, which accept either
/// a slug or an id. `org_id` is the canonical id, resolved once here, and is the ONLY value
/// valid in a proof-of-possession message: the authority verifies against the id, so signing
/// or verifying over a slug produces bytes it never builds.
pub struct Ctx {
    pub grobase: String,
    pub token: String,
    pub org: String,
    pub org_id: String,
    pub project: String,
    pub env_id: String,
    pub env_name: String,
    pub scope_epoch: u32,
    pub scope_pubkey: Option<String>,
}

impl Ctx {
    /// The env's current scope epoch, never below 1: an env whose keyset predates epoch
    /// bookkeeping reports 0, and every scope verb means epoch 1 by that.
    pub fn epoch(&self) -> u32 {
        self.scope_epoch.max(1)
    }

    /// Address this env's grants at `epoch`. The epoch is explicit because rotation asks
    /// about the epoch it is creating while every other verb asks about the current one.
    pub fn grant_scope(&self, epoch: u32) -> GrantScope<'_> {
        GrantScope {
            grobase: &self.grobase,
            token: &self.token,
            org: &self.org,
            project: &self.project,
            env_id: &self.env_id,
            epoch,
        }
    }
}

/// Route the scope-key verbs, resolving their shared context first.
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
        Vault::SetEnv {
            org,
            project,
            env,
            path,
        } => {
            let ctx = resolve(profile, org, project, env).await?;
            scope_secret::set_env(session, &ctx, path).await
        }
        Vault::GetEnv {
            org,
            project,
            env,
            path,
        } => {
            let ctx = resolve(profile, org, project, env).await?;
            scope_secret::get_env(session, &ctx, path).await
        }
        Vault::PushEnv { org, project, env } => {
            scope_tree::push_env(session, &resolve(profile, org, project, env).await?).await
        }
        Vault::PullEnv {
            org,
            project,
            env,
            apply,
            backup,
        } => {
            let ctx = resolve(profile, org, project, env).await?;
            let opts = crate::core::materialize::Opts {
                apply: *apply,
                force: false,
                backup: *backup,
            };
            scope_tree::pull_env(session, &ctx, &opts).await
        }
        Vault::RotateScope { org, project, env } => {
            scope_rotate::rotate_scope(session, &resolve(profile, org, project, env).await?).await
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
    let org_id = org::show(&grobase, &token, org).await?.id;
    Ok(Ctx {
        grobase,
        token,
        org: org.to_string(),
        org_id,
        project: project.to_string(),
        env_id: found.id,
        env_name: env.to_string(),
        scope_epoch: found.scope_epoch,
        scope_pubkey: found.scope_pubkey,
    })
}

/// The env's members as the control plane sees them, grouped `(user_id, [grant_id…])`.
///
/// `pending` and `authorized` are NOT the same set and choosing between them is the whole
/// point of returning both. `pending` shrinks to nothing as members are provisioned, so it is
/// a worklist. `authorized` is everyone the env's grants cover, so it is the set that must be
/// re-wrapped when the scope key changes. Rotating from the worklist re-wraps nobody once
/// provisioning has converged, which strands the environment.
pub struct EnvMembers {
    pub pending: Vec<Member>,
    pub authorized: Vec<Member>,
}

/// One member of an environment: who they are, which grants put them there, and the
/// strongest project role any of those grants carries.
///
/// The role travels with the member because it decides what their WRAP will say they may do,
/// and a member under two grants must get the stronger of the two rather than whichever was
/// read last.
pub struct Member {
    pub user: String,
    pub grant_ids: Vec<String>,
    pub project_role: String,
}

/// Read the env's grants and ask each one who it authorizes and who still lacks a wrap at the
/// env's current epoch. A grant applies when it targets this env or is project-wide
/// (`env_id = None`). A user under several grants appears once, carrying every grant id, so
/// one vault42 wrap can be recorded against all of them.
pub async fn env_members(ctx: &Ctx) -> anyhow::Result<EnvMembers> {
    let scope = ctx.grant_scope(ctx.epoch());
    let grants = grant::list(&scope).await?;
    let (mut pending, mut authorized) = (Vec::new(), Vec::new());
    for g in grants.iter().filter(|g| applies(g, &ctx.env_id)) {
        let f = grant::fulfilled(&scope, &g.id).await?;
        let role = g.project_role.clone();
        pending.extend(
            f.missing
                .into_iter()
                .map(|user| (g.id.clone(), user, role.clone())),
        );
        authorized.extend(
            f.members
                .into_iter()
                .map(|user| (g.id.clone(), user, role.clone())),
        );
    }
    Ok(EnvMembers {
        pending: group_by_user(pending),
        authorized: group_by_user(authorized),
    })
}

/// Group `(grant_id, user_id, role)` triples by user, preserving first occurrence — so one
/// vault42 wrap per user is recorded against each of that user's grants, carrying the
/// STRONGEST role any of them gives. A member holding both a read grant and a write grant is
/// a writer; taking whichever arrived last would make that depend on row order.
fn group_by_user(triples: Vec<(String, String, String)>) -> Vec<Member> {
    let mut grouped: Vec<Member> = Vec::new();
    for (grant_id, user, role) in triples {
        match grouped.iter_mut().find(|m| m.user == user) {
            Some(member) => {
                member.grant_ids.push(grant_id);
                member.project_role = stronger(&member.project_role, &role).to_string();
            }
            None => grouped.push(Member {
                user,
                grant_ids: vec![grant_id],
                project_role: role,
            }),
        }
    }
    grouped
}

/// The stronger of two project roles. Anything unrecognised ranks below `read`, so an
/// unknown role can never outrank a known one and widen what a member is wrapped for.
fn stronger<'a>(a: &'a str, b: &'a str) -> &'a str {
    if rank(a) >= rank(b) {
        a
    } else {
        b
    }
}

/// Where a project role sits in the ordering. Unknown is 0 on purpose.
fn rank(role: &str) -> u8 {
    match role {
        "admin" => 3,
        "write" => 2,
        "read" => 1,
        _ => 0,
    }
}

/// Whether a grant applies to `env_id`: a grant scoped to this env, or a project-wide grant.
fn applies(g: &crate::adapters::rbac::ProjectGrant, env_id: &str) -> bool {
    match &g.env_id {
        Some(id) => id == env_id,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A user authorized by several grants collapses into one entry carrying every grant id,
    /// in first-seen order, so one vault42 wrap is recorded against all of them.
    #[test]
    fn a_user_under_several_grants_is_grouped_once() {
        let triples = vec![
            ("g1".to_string(), "alice".to_string(), "read".to_string()),
            ("g2".to_string(), "bob".to_string(), "read".to_string()),
            ("g3".to_string(), "alice".to_string(), "read".to_string()),
        ];
        let grouped = group_by_user(triples);
        assert_eq!(grouped.len(), 2, "one entry per distinct user");
        assert_eq!(grouped[0].user, "alice", "first-seen order is preserved");
        assert_eq!(
            grouped[0].grant_ids,
            vec!["g1".to_string(), "g3".to_string()]
        );
        assert_eq!(grouped[1].grant_ids, vec!["g2".to_string()]);
    }

    /// A member under both a read grant and a write grant is a WRITER, whichever order the
    /// rows arrive in. Taking the last one read would make what a member may do depend on
    /// how the control plane happened to sort its answer.
    #[test]
    fn the_strongest_role_wins_in_either_order() {
        for pair in [("read", "write"), ("write", "read")] {
            let grouped = group_by_user(vec![
                ("g1".to_string(), "alice".to_string(), pair.0.to_string()),
                ("g2".to_string(), "alice".to_string(), pair.1.to_string()),
            ]);
            assert_eq!(grouped[0].project_role, "write", "{pair:?}");
        }
    }

    /// An unknown role never outranks a known one. A control plane inventing a role must not
    /// be able to widen a member's wrap by naming something this client cannot interpret.
    #[test]
    fn an_unknown_role_never_outranks_a_known_one() {
        let grouped = group_by_user(vec![
            ("g1".to_string(), "alice".to_string(), "read".to_string()),
            (
                "g2".to_string(),
                "alice".to_string(),
                "superuser".to_string(),
            ),
        ]);
        assert_eq!(grouped[0].project_role, "read");
        assert_eq!(stronger("admin", "write"), "admin");
        assert_eq!(stronger("write", "admin"), "admin");
    }

    /// An env-scoped grant reaches only its own environment; a project-wide grant reaches
    /// every one. Inverting this hands somebody a key they were never granted.
    #[test]
    fn a_grant_applies_to_its_own_env_or_to_all_of_them() {
        let scoped = crate::adapters::rbac::ProjectGrant {
            id: "g1".into(),
            env_id: Some("env-prod".into()),
            project_role: "read".into(),
        };
        let wide = crate::adapters::rbac::ProjectGrant {
            id: "g2".into(),
            env_id: None,
            project_role: "write".into(),
        };
        assert!(applies(&scoped, "env-prod"));
        assert!(!applies(&scoped, "env-staging"));
        assert!(applies(&wide, "env-prod") && applies(&wide, "env-staging"));
    }
}
