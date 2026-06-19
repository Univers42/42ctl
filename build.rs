/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   build.rs                                             :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Build script: stamp the short git commit into `FT_GIT_SHA` so `42ctl version` reports
//! version + commit. Falls back to "unknown" when `.git` is absent (e.g. a release tarball
//! or a Docker build context without history); CI/release sets `FT_GIT_SHA` explicitly.

/// Capture `git rev-parse --short HEAD` (best-effort) into a compile-time env var.
fn main() {
    let sha = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=FT_GIT_SHA={sha}");
    println!("cargo:rerun-if-changed=.git/HEAD");
}
