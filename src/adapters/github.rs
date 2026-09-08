/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   github.rs                                            :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! GitHub Releases as the distribution channel for `42ctl update` (D11). Resolves the
//! newest release of `Univers42/42ctl` by following the `releases/latest` redirect — no
//! API token, no rate-limited endpoint — and downloads release assets (`42ctl-<target>`,
//! `SHA256SUMS`) into memory. Nothing here trusts bytes: `checksum` verifies them.

use anyhow::{bail, Context};
use semver::Version;

/// The GitHub repository releases are published from.
pub const REPO: &str = "Univers42/42ctl";

/// One published release: its git tag (`vX.Y.Z`) and the semver it carries.
#[derive(Debug)]
pub struct Release {
    pub tag: String,
    pub version: Version,
}

/// The two HTTP clients discovery needs: `probe` never follows redirects (so the
/// `releases/latest` Location header can be read), `fetch` does (asset downloads hop to
/// the GitHub CDN). Built once per command and threaded down — no globals.
pub struct GitHub {
    probe: reqwest::Client,
    fetch: reqwest::Client,
}

impl Release {
    /// Parse a `vX.Y.Z` (or bare `X.Y.Z`) tag into a release.
    pub fn from_tag(tag: &str) -> anyhow::Result<Self> {
        let version = Version::parse(tag.trim().trim_start_matches('v'))
            .with_context(|| format!("'{tag}' is not a vX.Y.Z release tag"))?;
        Ok(Self {
            tag: format!("v{version}"),
            version,
        })
    }
}

impl GitHub {
    /// Construct both clients with a `42ctl/<version>` user agent.
    pub fn new() -> anyhow::Result<Self> {
        let agent = concat!("42ctl/", env!("CARGO_PKG_VERSION"));
        let probe = reqwest::Client::builder()
            .user_agent(agent)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let fetch = reqwest::Client::builder().user_agent(agent).build()?;
        Ok(Self { probe, fetch })
    }

    /// The newest published release, read from where `releases/latest` redirects to.
    pub async fn latest(&self) -> anyhow::Result<Release> {
        let url = format!("https://github.com/{REPO}/releases/latest");
        let response = self
            .probe
            .get(&url)
            .send()
            .await
            .context("cannot reach github.com — check your connection")?;
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let tag = location.rsplit_once("/releases/tag/").map(|(_, tag)| tag);
        let tag = tag.with_context(|| {
            format!("no release has been published yet — see https://github.com/{REPO}/releases")
        })?;
        Release::from_tag(tag)
    }

    /// The release's `SHA256SUMS` manifest as text (missing → no such release).
    pub async fn checksums(&self, tag: &str) -> anyhow::Result<String> {
        let bytes = self.asset(tag, "SHA256SUMS").await?;
        String::from_utf8(bytes).context("SHA256SUMS is not UTF-8")
    }

    /// Download one release asset fully into memory (a static binary is ~10 MB).
    pub async fn asset(&self, tag: &str, name: &str) -> anyhow::Result<Vec<u8>> {
        let url = format!("https://github.com/{REPO}/releases/download/{tag}/{name}");
        let response = self
            .fetch
            .get(&url)
            .send()
            .await
            .with_context(|| format!("download failed: {name}"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            bail!("release {tag} has no asset '{name}' (does that release exist?)");
        }
        let body = response.error_for_status()?.bytes().await?;
        Ok(body.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_parses_with_or_without_v() {
        assert_eq!(Release::from_tag("v1.2.3").unwrap().tag, "v1.2.3");
        assert_eq!(Release::from_tag("1.2.3").unwrap().tag, "v1.2.3");
        assert_eq!(
            Release::from_tag(" v0.4.0\n").unwrap().version,
            Version::new(0, 4, 0)
        );
    }

    #[test]
    fn non_semver_tag_is_an_error() {
        assert!(Release::from_tag("latest").is_err());
        assert!(Release::from_tag("v1.2").is_err());
    }
}
