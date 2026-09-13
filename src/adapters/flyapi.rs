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

/// Overrides `HOST`, for the QA battery's stand-in and nothing else a user needs.
///
/// It carries the same trust as `FT_FLYCTL`, which already runs any program with the token: an
/// environment that can set either can already read the token itself.
const HOST_OVERRIDE: &str = "FT_FLY_MACHINES_API";

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
    let base = base(std::env::var(HOST_OVERRIDE).ok())?;
    let url = format!("{base}/v1/apps/{app}/machines/{machine}/ps");
    let response = reqwest::Client::new()
        .get(&url)
        .bearer_auth(&token)
        .send()
        .await?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    read_ps(status, &body, &token, (app, machine))
}

/// The Machines API base: `override_base` when one is given and is an http(s) URL, else Fly's.
fn base(override_base: Option<String>) -> anyhow::Result<String> {
    match override_base {
        None => Ok(HOST.to_string()),
        Some(url) if url.starts_with("https://") || url.starts_with("http://") => {
            Ok(url.trim_end_matches('/').to_string())
        }
        Some(url) => anyhow::bail!("{HOST_OVERRIDE} must be an http(s) URL, got {url:?}"),
    }
}

/// Turn `/ps`'s answer into a process table or a refusal an operator can act on.
///
/// A stopped machine answers `412 failed_precondition`, which is state rather than breakage and
/// is said so; any other failure carries Fly's body with the token scrubbed out of it.
fn read_ps(
    status: u16,
    body: &str,
    token: &str,
    target: (&str, &str),
) -> anyhow::Result<Vec<Process>> {
    let (app, machine) = target;
    if status == 412 {
        anyhow::bail!("machine {machine} is not running — `42ctl cloud machine start {machine}`");
    }
    if !(200..300).contains(&status) {
        anyhow::bail!(
            "fly answered {status} for {app}/{machine}: {}",
            scrub(body, token)
        );
    }
    serde_json::from_str(body).map_err(|error| anyhow::anyhow!("could not read /ps: {error}"))
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

    /// A stopped machine is reported as state, with the command that changes it.
    #[test]
    fn a_stopped_machine_is_told_how_to_start() {
        let error = read_ps(412, r#"{"error":"failed_precondition"}"#, "t", ("a", "81"))
            .err()
            .expect("412 is a refusal")
            .to_string();
        assert!(error.contains("machine 81 is not running"), "{error}");
        assert!(error.contains("42ctl cloud machine start 81"), "{error}");
    }

    /// Any other failure names the status and never the token, even when Fly echoes it.
    #[test]
    fn a_failure_names_the_status_and_scrubs_the_token() {
        let error = read_ps(500, "boom for FlyV1 secret", "FlyV1 secret", ("a", "81"))
            .err()
            .expect("500 is a failure")
            .to_string();
        assert!(error.contains("fly answered 500 for a/81"), "{error}");
        assert!(!error.contains("FlyV1 secret"), "{error}");
    }

    /// A success is the table.
    #[test]
    fn a_success_is_parsed_into_processes() {
        let rows = read_ps(
            200,
            r#"[{"pid":1,"command":"/fly/init"}]"#,
            "t",
            ("a", "81"),
        )
        .expect("parsed");
        assert_eq!(rows[0].command, "/fly/init");
    }

    /// The override must be an http(s) URL; anything else is refused rather than guessed at.
    #[test]
    fn the_host_override_is_an_http_url_or_refused() {
        assert_eq!(base(None).expect("default"), HOST);
        assert_eq!(
            base(Some("http://stub:8080/".to_string())).expect("override"),
            "http://stub:8080"
        );
        assert!(base(Some("stub:8080".to_string())).is_err());
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
