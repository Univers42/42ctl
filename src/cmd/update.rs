/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   update.rs                                            :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl update` — self-update from GitHub Releases (D11). Resolves the newest (or
//! `--version`) release, downloads the static `42ctl-<target>` binary, verifies its SHA-256
//! against the release's `SHA256SUMS`, and only then renames it over the running binary in
//! one atomic step. A failed download or checksum changes nothing on disk. `--check` only
//! reports. Works for any install location the user can write to (sudo otherwise).

use crate::adapters::checksum;
use crate::adapters::github::{GitHub, Release};
use crate::ui;
use anyhow::Context;
use semver::Version;
use std::path::{Path, PathBuf};

const ASSET: &str = concat!("42ctl-", env!("FT_TARGET"));

/// Check for, or install, a newer release; `pin` installs that exact version instead.
pub async fn run(check: bool, pin: Option<&str>) -> anyhow::Result<()> {
    let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
    let github = GitHub::new()?;
    let release = match pin {
        Some(wanted) => Release::from_tag(wanted)?,
        None => github.latest().await?,
    };
    ui::field("installed", &current.to_string());
    ui::field("available", &release.version.to_string());
    if check {
        report(&current, &release);
        return Ok(());
    }
    if pin.is_none() && release.version <= current {
        ui::success("42ctl is up to date");
        return Ok(());
    }
    let exe = std::env::current_exe().context("cannot locate the running binary")?;
    let bytes = fetch_verified(&github, &release).await?;
    swap(&exe, &bytes).with_context(|| permission_hint(&exe))?;
    ui::success(&format!(
        "42ctl {current} → {} installed at {}",
        release.version,
        exe.display()
    ));
    Ok(())
}

/// Print whether `release` is newer than `current` and what to run.
fn report(current: &Version, release: &Release) {
    if release.version > *current {
        println!(
            "{} run {} to install it",
            ui::warn("update available:"),
            ui::accent("42ctl update")
        );
    } else {
        ui::success("42ctl is up to date");
    }
}

/// Download `ASSET` for `release` and return its bytes only if the SHA-256 matches.
async fn fetch_verified(github: &GitHub, release: &Release) -> anyhow::Result<Vec<u8>> {
    let sums = github.checksums(&release.tag).await?;
    let expected = checksum::expected_digest(&sums, ASSET)?;
    println!(
        "{}",
        ui::dim(&format!("downloading {ASSET} {} …", release.tag))
    );
    let bytes = github.asset(&release.tag, ASSET).await?;
    checksum::verify_digest(&bytes, &expected)?;
    println!("{}", ui::dim("checksum verified"));
    Ok(bytes)
}

/// Stage `bytes` beside `exe` (same filesystem), mark it executable, and rename it over
/// `exe` — one atomic step, so a crash mid-way leaves the old binary intact.
fn swap(exe: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let staging: PathBuf = exe.with_extension(format!("new.{}", std::process::id()));
    std::fs::write(&staging, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&staging, exe).inspect_err(|_| {
        let _ = std::fs::remove_file(&staging);
    })?;
    Ok(())
}

/// The error context for a failed swap: where it lives and how to get write access.
fn permission_hint(exe: &Path) -> String {
    let dir = exe.parent().unwrap_or(exe).display();
    format!("cannot replace the binary in {dir} — re-run with sudo, or reinstall to ~/.local/bin")
}
