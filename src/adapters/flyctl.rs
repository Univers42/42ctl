/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   flyctl.rs                                            :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! flyctl, driven — not Fly's API, re-implemented.
//!
//! Every cloud verb shells out to the same `fly` the operator would type, so 42ctl inherits
//! its behaviour, its flags and its fixes instead of maintaining a parallel client that drifts
//! from them. `vault42`'s own deploy pipeline already drives flyctl this way.
//!
//! Discovery prefers what is installed (`FT_FLYCTL`, then `fly`, then `flyctl` on PATH) and
//! falls back to a container pinned **by digest**: a program handed an org-wide token is not
//! something to resolve by a mutable tag.
//!
//! The token is read from the environment at call time and, for the container, passed by NAME
//! (`-e FLY_API_TOKEN`) so its value never reaches an argument list, `ps`, or `docker inspect`.
//! Captured output is scrubbed of it before it can be folded into an error chain.

use anyhow::Context;
use serde::de::DeserializeOwned;
use std::path::PathBuf;
use std::process::Stdio;

/// The container used when no flyctl is installed — the same version `vault42`'s deploy
/// workflow pins, by digest rather than by tag.
const IMAGE: &str =
    "flyio/flyctl@sha256:39a83e2c4129b896df502d72027dde6e436b5cf8f74b994b144857e0e22c0d7b";

/// The variable holding the Fly token. Named here once so every refusal can spell it.
const TOKEN: &str = "FLY_API_TOKEN";

/// Command words of a flyctl invocation that deletes, detaches, releases or scales something
/// away. No 42ctl verb ever runs one — see `refuse_destructive`.
const DESTRUCTIVE: &[&str] = &[
    "destroy", "delete", "remove", "rm", "release", "unset", "scale", "detach", "revoke",
];

/// How flyctl is reached on this machine.
enum Program {
    /// An installed binary, run directly.
    Native(PathBuf),
    /// The pinned container, run through docker.
    Docker,
}

/// A discovered flyctl, ready to run.
pub struct Flyctl {
    program: Program,
}

impl Flyctl {
    /// Find flyctl, or explain what to install.
    ///
    /// `FT_FLYCTL` comes first so an operator can pin a specific binary and a test can point
    /// at a stub without a container anywhere near it.
    pub fn discover() -> anyhow::Result<Self> {
        if let Some(path) = std::env::var_os("FT_FLYCTL").map(PathBuf::from) {
            return Ok(Self {
                program: Program::Native(path),
            });
        }
        for name in ["fly", "flyctl"] {
            if let Some(path) = on_path(name) {
                return Ok(Self {
                    program: Program::Native(path),
                });
            }
        }
        if on_path("docker").is_some() {
            return Ok(Self {
                program: Program::Docker,
            });
        }
        anyhow::bail!(
            "no flyctl and no docker — install flyctl (https://fly.io/docs/flyctl/install/), \
             or set FT_FLYCTL to one"
        )
    }

    /// The invocation as a person would type it, for `--dry-run` and for the line echoed
    /// before anything that changes the deployment.
    pub fn rendered(&self, args: &[String]) -> String {
        let (program, prefix) = self.spelling();
        let all = prefix.iter().chain(args).cloned().collect::<Vec<_>>();
        format!("{program} {}", all.join(" "))
    }

    /// Run flyctl and decode its `--json` output.
    pub async fn json<T: DeserializeOwned>(&self, args: &[String]) -> anyhow::Result<T> {
        let out = self.capture(args).await?;
        serde_json::from_str(&out).with_context(|| {
            format!(
                "could not read `{}` — is flyctl's output shape still what this expects?",
                self.rendered(args)
            )
        })
    }

    /// Run flyctl and return its stdout, failing with its stderr when it does.
    pub async fn capture(&self, args: &[String]) -> anyhow::Result<String> {
        let token = token()?;
        let output = self
            .command(args)?
            .stdin(Stdio::null())
            .output()
            .await
            .with_context(|| format!("could not run `{}`", self.rendered(args)))?;
        let stdout = scrub(&String::from_utf8_lossy(&output.stdout), &token);
        if output.status.success() {
            return Ok(stdout);
        }
        let stderr = scrub(&String::from_utf8_lossy(&output.stderr), &token);
        anyhow::bail!("{} failed: {}", self.rendered(args), stderr.trim())
    }

    /// Run a flyctl command that CHANGES the deployment, printing what it reports.
    ///
    /// Captured rather than streamed so a refusal can be read: a read-only token is turned
    /// away by Fly itself with a lease error that names neither the token nor what to do, and
    /// this is the one place that knows both. Fly's refusal is the enforcement — a member
    /// holding a read-only token cannot stop a machine however they call flyctl — so 42ctl
    /// only has to say so plainly.
    pub async fn change(&self, args: &[String]) -> anyhow::Result<()> {
        let token = token()?;
        let output = self
            .command(args)?
            .stdin(Stdio::null())
            .output()
            .await
            .with_context(|| format!("could not run `{}`", self.rendered(args)))?;
        print!(
            "{}",
            scrub(&String::from_utf8_lossy(&output.stdout), &token)
        );
        if output.status.success() {
            return Ok(());
        }
        let stderr = scrub(&String::from_utf8_lossy(&output.stderr), &token);
        if stderr.contains("unauthorized") {
            anyhow::bail!(not_allowed(&args.join(" ")));
        }
        anyhow::bail!("{} failed: {}", self.rendered(args), stderr.trim())
    }

    /// Run flyctl with its output going straight to the terminal, for the streaming verbs
    /// (`logs`, `machine wait`) where waiting for the end would defeat the point.
    pub async fn stream(&self, args: &[String]) -> anyhow::Result<()> {
        let _ = token()?;
        let status = self
            .command(args)?
            .stdin(Stdio::null())
            .status()
            .await
            .with_context(|| format!("could not run `{}`", self.rendered(args)))?;
        if status.success() {
            return Ok(());
        }
        anyhow::bail!("{} exited with {status}", self.rendered(args))
    }

    /// Build the process, with the token passed by name rather than by value.
    ///
    /// Every invocation is built here, which is why the refusal of destructive commands is
    /// here: a verb added later cannot delete anything by forgetting to check.
    fn command(&self, args: &[String]) -> anyhow::Result<tokio::process::Command> {
        refuse_destructive(args)?;
        let (program, prefix) = self.spelling();
        let mut command = tokio::process::Command::new(program);
        command.args(prefix).args(args).kill_on_drop(true);
        Ok(command)
    }

    /// The program to run and the arguments that come before flyctl's own.
    fn spelling(&self) -> (String, Vec<String>) {
        match &self.program {
            Program::Native(path) => (path.display().to_string(), Vec::new()),
            Program::Docker => (
                "docker".to_string(),
                ["run", "--rm", "-i", "-e", TOKEN, IMAGE]
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            ),
        }
    }
}

/// The Fly token, or a refusal that names the variable.
///
/// Deliberately read here, per call, and never written to 42ctl's config: that file is plain
/// JSON the user is invited to read and share, and a token with org-wide reach has no business
/// in it.
fn token() -> anyhow::Result<String> {
    std::env::var(TOKEN).map_err(|_| {
        anyhow::anyhow!(
            "{TOKEN} is not set — export it (`export {TOKEN}=$(fly auth token)`) before a cloud verb"
        )
    })
}

/// Refuse any flyctl command that deletes, detaches, releases or scales something away.
///
/// 42ctl controls a deployment and never takes one apart. A machine, a volume, an app, a
/// secret, an address or a certificate is removed with flyctl by somebody who means to, not
/// through a verb here. Only the COMMAND words are checked — the leading arguments before the
/// first flag — so an app or a secret that happens to be named `delete` is not mistaken for one.
fn refuse_destructive(args: &[String]) -> anyhow::Result<()> {
    let found = args
        .iter()
        .take_while(|arg| !arg.starts_with('-'))
        .take(3)
        .find(|word| DESTRUCTIVE.contains(&word.as_str()));
    if let Some(word) = found {
        anyhow::bail!(
            "42ctl never runs `fly … {word}` — deleting or scaling away cloud resources is done \
             with flyctl directly, on purpose"
        );
    }
    Ok(())
}

/// What an operator is told when Fly refuses the token for `command` (flyctl's own words).
fn not_allowed(command: &str) -> String {
    format!(
        "fly refused this {TOKEN} for `fly {command}` — changing a machine or a volume needs an \
         administrator's token; a read-only token (`fly tokens create readonly`) can still ls, \
         inspect, logs and health"
    )
}

/// Replace the token with a marker wherever it appears, before the text can reach an error.
fn scrub(text: &str, token: &str) -> String {
    if token.is_empty() {
        return text.to_string();
    }
    text.replace(token, "<FLY_API_TOKEN>")
}

/// The first `name` on `PATH` that is there, if any.
fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The container form passes the token by name, never by value: an argument list is
    /// readable by every other process on the machine.
    #[test]
    fn the_container_form_names_the_token_but_never_carries_it() {
        let fly = Flyctl {
            program: Program::Docker,
        };
        let rendered = fly.rendered(&["machine".to_string(), "list".to_string()]);
        assert!(rendered.starts_with("docker run --rm -i -e FLY_API_TOKEN "));
        assert!(
            rendered.contains("@sha256:"),
            "the image is pinned by digest"
        );
        assert!(rendered.ends_with(" machine list"));
    }

    /// A native flyctl runs with no wrapper, so the echoed line is what the operator could
    /// paste back.
    #[test]
    fn a_native_flyctl_is_spelled_as_it_would_be_typed() {
        let fly = Flyctl {
            program: Program::Native(PathBuf::from("/usr/bin/fly")),
        };
        assert_eq!(
            fly.rendered(&["status".to_string(), "--json".to_string()]),
            "/usr/bin/fly status --json"
        );
    }

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(ToString::to_string).collect()
    }

    /// Every flyctl command that takes something apart is refused before a process exists.
    #[test]
    fn a_destructive_command_is_refused_whatever_asks_for_it() {
        for line in [
            "machine destroy 8151d9a99540e8 --app vault42-authority --force",
            "machine rm 8151d9a99540e8",
            "volumes destroy vol_x --app vault42-server --yes",
            "volumes snapshots delete snap_x",
            "apps destroy vault42-server --yes",
            "secrets unset VAULT42_CONTRACT_PUBKEY --app vault42-server",
            "ips release 1.2.3.4 --app vault42-server",
            "certs remove vault.example --app vault42-server",
            "scale count 0 --app vault42-server",
            "volumes detach vol_x",
            "tokens revoke tok_x",
        ] {
            let refused = refuse_destructive(&words(line));
            assert!(refused.is_err(), "must be refused: {line}");
        }
    }

    /// The controller keeps every verb it is meant to have, lifecycle included.
    #[test]
    fn reading_and_lifecycle_commands_are_allowed() {
        for line in [
            "machine list --app vault42-server --json",
            "machine stop 8151d9a99540e8 --app vault42-authority",
            "machine start 8151d9a99540e8 --app vault42-authority",
            "machine restart 8151d9a99540e8 --app vault42-authority",
            "machine suspend 8151d9a99540e8 --app vault42-authority",
            "machine wait 8151d9a99540e8 --app vault42-authority --state started",
            "volumes list --app vault42-server --json",
            "volumes snapshots create vol_x --app vault42-server",
            "logs --app vault42-server --no-tail",
            "secrets list --app vault42-server --json",
            "ips list --app vault42-server --json",
        ] {
            assert!(refuse_destructive(&words(line)).is_ok(), "must run: {line}");
        }
    }

    /// Only command words count: a resource merely NAMED like a destructive verb is not one.
    #[test]
    fn a_value_named_like_a_destructive_verb_is_not_a_command() {
        assert!(refuse_destructive(&words("logs --app delete --no-tail")).is_ok());
        assert!(refuse_destructive(&words("status --app rm --json")).is_ok());
    }

    /// The refusal names the token, the command, and the kind of token that would do.
    #[test]
    fn a_refused_change_says_whose_token_would_work() {
        let message = not_allowed("machine stop 81 --app a");
        assert!(message.contains("FLY_API_TOKEN"), "{message}");
        assert!(message.contains("fly machine stop 81"), "{message}");
        assert!(message.contains("administrator"), "{message}");
    }

    /// The token is removed from anything captured, so it cannot be folded into an error
    /// chain and printed by the reporter at the top of main.
    #[test]
    fn the_token_never_survives_into_captured_output() {
        let secret = "FlyV1 fm2_notarealtoken";
        let text = format!("error: bad auth for {secret} while listing");
        let clean = scrub(&text, secret);
        assert!(!clean.contains(secret), "the value is gone: {clean}");
        assert!(clean.contains("<FLY_API_TOKEN>"), "and is marked: {clean}");
        assert_eq!(scrub("nothing to hide", ""), "nothing to hide");
    }
}
