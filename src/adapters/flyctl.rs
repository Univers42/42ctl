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
            .command(args)
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

    /// Run flyctl with its output going straight to the terminal, for the streaming verbs
    /// (`logs -f`, `deploy`) where waiting for the end would defeat the point.
    pub async fn stream(&self, args: &[String]) -> anyhow::Result<()> {
        let _ = token()?;
        let status = self
            .command(args)
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
    fn command(&self, args: &[String]) -> tokio::process::Command {
        let (program, prefix) = self.spelling();
        let mut command = tokio::process::Command::new(program);
        command.args(prefix).args(args).kill_on_drop(true);
        command
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
