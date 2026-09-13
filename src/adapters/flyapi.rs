/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   flyapi.rs                                            :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The one Fly route flyctl does not cover: a machine's process table.
//!
//! Every other cloud verb delegates to flyctl, deliberately — see `adapters/flyctl.rs`. This
//! module is the documented exception, and it stays one route wide on purpose. `fly machine
//! status` reports the machine; nothing in flyctl reports what is RUNNING INSIDE it, which is
//! what `cloud machine top` answers, so the Machines REST `/ps` route is called directly.
//!
//! The sibling exception the plan named — Prometheus for `cloud machine stats` — is NOT here,
//! because it was measured rather than assumed: `GET /prometheus/personal/api/v1/query` answers
//! `401 something went wrong resolving organization` for the deploy-scoped token that reaches
//! the Machines API fine. A verb that always fails is worse than an absent one, so `stats` is
//! absent and `docs/cloud.md` says why.
//!
//! The token is read per call from the environment, never from 42ctl's config, and scrubbed
//! out of anything that could reach an error chain — the same discipline `flyctl.rs` keeps.

use serde::Deserialize;

/// The Machines API host. Fly's own tooling talks to this name, not to `api.fly.io`.
const HOST: &str = "https://api.machines.dev";

/// The variable holding the Fly token, named once so a refusal can spell it.
const TOKEN: &str = "FLY_API_TOKEN";

/// One process inside a machine, as `/ps` reports it.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Process {
    pub pid: i64,
    pub command: String,
    pub directory: String,
    /// Accumulated CPU, in the units Fly reports (not a percentage).
    pub cpu: i64,
    /// Resident set size in bytes.
    pub rss: i64,
    /// Seconds of system time.
    pub stime: i64,
    /// Seconds since the process started.
    pub rtime: i64,
}

/// The process table of one machine.
///
/// A stopped machine answers `412 failed_precondition`, which is a true answer rather than an
/// error to paper over: there is no process table because there is no machine running. It is
/// turned into a refusal naming the machine so an operator reads it as state, not breakage.
pub async fn machine_ps(app: &str, machine: &str) -> anyhow::Result<Vec<Process>> {
    let token = token()?;
    let url = format!("{HOST}/v1/apps/{app}/machines/{machine}/ps");
    let response = reqwest::Client::new()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status == reqwest::StatusCode::PRECONDITION_FAILED {
        anyhow::bail!("machine {machine} is not running — `42ctl cloud machine start {machine}`");
    }
    if !status.is_success() {
        anyhow::bail!(
            "fly answered {status} for {app}/{machine}: {}",
            scrub(&body, &token)
        );
    }
    serde_json::from_str(&body).map_err(|error| anyhow::anyhow!("could not read /ps: {error}"))
}

/// The Fly token, or a refusal that names the variable.
fn token() -> anyhow::Result<String> {
    std::env::var(TOKEN).map_err(|_| {
        anyhow::anyhow!(
            "{TOKEN} is not set — export it (`export {TOKEN}=$(fly auth token)`) before a cloud verb"
        )
    })
}

/// Replace the token with a marker wherever it appears, before the text can reach an error.
fn scrub(text: &str, token: &str) -> String {
    if token.is_empty() {
        return text.to_string();
    }
    text.replace(token, "<FLY_API_TOKEN>")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `/ps` reports empty commands for kernel-side pids, so a reader must tolerate them
    /// rather than assume every process is named. Measured against the live route.
    #[test]
    fn a_process_row_survives_the_fields_fly_leaves_blank() {
        let raw = r#"[{"pid":1,"stime":0,"rtime":7,"command":"/fly/init","directory":"/",
                       "cpu":0,"rss":5181440,"listen_sockets":[]},
                      {"pid":2,"stime":0,"rtime":7,"command":"","directory":"/",
                       "cpu":0,"rss":0,"listen_sockets":[]}]"#;
        let rows: Vec<Process> = serde_json::from_str(raw).expect("parse");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].command, "/fly/init");
        assert_eq!(rows[0].rss, 5_181_440);
        assert!(rows[1].command.is_empty(), "a blank command is a real row");
    }

    /// A field Fly adds later must not break the reader, and one it drops must not either.
    #[test]
    fn an_unknown_field_is_ignored_and_a_missing_one_defaults() {
        let rows: Vec<Process> =
            serde_json::from_str(r#"[{"pid":9,"something_new":true}]"#).expect("parse");
        assert_eq!(rows[0].pid, 9);
        assert_eq!(rows[0].rss, 0);
        assert!(rows[0].command.is_empty());
    }

    /// The token is removed from a captured body, so a 500 carrying it back cannot be folded
    /// into an error chain and printed by the reporter at the top of main.
    #[test]
    fn the_token_never_survives_into_a_reported_body() {
        let secret = "FlyV1 fm2_notarealtoken";
        let body = format!("upstream rejected {secret}");
        let clean = scrub(&body, secret);
        assert!(!clean.contains(secret), "the value is gone: {clean}");
        assert!(clean.contains("<FLY_API_TOKEN>"), "and is marked: {clean}");
        assert_eq!(scrub("nothing to hide", ""), "nothing to hide");
    }
}
