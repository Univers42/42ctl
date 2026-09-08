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
//! version + commit, and the target triple into `FT_TARGET` so `42ctl update` can name the
//! matching release asset (`42ctl-<target>`). An explicit `FT_GIT_SHA` in the environment
//! (CI/release) wins; otherwise git is asked, falling back to "unknown" without a `.git`.

/// Stamp `FT_GIT_SHA` (explicit env, else git, else "unknown") and `FT_TARGET`.
fn main() {
    let sha = std::env::var("FT_GIT_SHA")
        .ok()
        .filter(|s| !s.is_empty() && s != "unknown")
        .unwrap_or_else(git_short_sha);
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=FT_GIT_SHA={sha}");
    println!("cargo:rustc-env=FT_TARGET={target}");
    println!("cargo:rerun-if-env-changed=FT_GIT_SHA");
    println!("cargo:rerun-if-changed=.git/HEAD");
}

/// `git rev-parse --short HEAD`, or "unknown" when git or the history is unavailable.
fn git_short_sha() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}
