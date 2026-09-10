/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   project.rs                                           :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Project discovery + file scan. A project is rooted where a `.42ctl/` marker lives;
//! the stable `project_id` (shared across machines to pull) is kept in
//! `.42ctl/project.json`. `scan` walks the tree for files matching the configured
//! patterns (`*.env*`, `*.secrets`), skipping `.42ctl/` and symlinks, and takes EVERY
//! regular file under a directory named `secrets/` regardless of the patterns.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const MARKER_DIR: &str = ".42ctl";
const PROJECT_NS: Uuid = Uuid::from_bytes([
    0x34, 0x32, 0x63, 0x74, 0x6c, 0x70, 0x72, 0x6f, 0x6a, 0x65, 0x63, 0x74, 0x6e, 0x73, 0x76, 0x31,
]);

#[derive(Serialize, Deserialize)]
struct Marker {
    project_id: String,
    patterns: Vec<String>,
}

/// A resolved project: its root, stable id, and scan patterns.
pub struct Project {
    pub root: PathBuf,
    pub project_id: String,
    pub patterns: Vec<String>,
}

/// Open/initialise the project at `start`. An `explicit_id` (pulling on a new machine)
/// wins; otherwise read `.42ctl/project.json`, or create one with a fresh id derived
/// from the canonical root path. Returns the project + whether it was newly created.
pub fn open(start: &Path, explicit_id: Option<&str>) -> anyhow::Result<(Project, bool)> {
    let root = start.to_path_buf();
    let marker = root.join(MARKER_DIR).join("project.json");
    if let Some(id) = explicit_id {
        return Ok((mk(root, id, default_patterns()), false));
    }
    if marker.exists() {
        let m: Marker = serde_json::from_slice(&std::fs::read(&marker)?)?;
        return Ok((mk(root, &m.project_id, m.patterns), false));
    }
    let canon = std::fs::canonicalize(&root)?;
    let project_id = Uuid::new_v5(&PROJECT_NS, canon.to_string_lossy().as_bytes()).to_string();
    let m = Marker {
        project_id: project_id.clone(),
        patterns: default_patterns(),
    };
    std::fs::create_dir_all(root.join(MARKER_DIR))?;
    std::fs::write(&marker, serde_json::to_vec_pretty(&m)?)?;
    Ok((mk(root, &project_id, default_patterns()), true))
}

/// Whether the project root is itself a secret directory, so `scan` starts with `all` set.
fn is_secret_dir_root(root: &Path) -> bool {
    root.file_name()
        .map(|n| is_secret_dir(&n.to_string_lossy()))
        .unwrap_or(false)
}

/// Construct a Project value.
fn mk(root: PathBuf, project_id: &str, patterns: Vec<String>) -> Project {
    Project {
        root,
        project_id: project_id.to_string(),
        patterns,
    }
}

/// The default scan patterns: every `*.env*` and `*.secrets` file.
pub fn default_patterns() -> Vec<String> {
    vec!["*.env*".to_string(), "*.secrets".to_string()]
}

/// Scan the project tree for matching files (skips `.42ctl/` + symlinks), path-sorted.
pub fn scan(project: &Project) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk(
        &project.root,
        &project.patterns,
        is_secret_dir_root(&project.root),
        &mut out,
    )?;
    out.sort();
    Ok(out)
}

/// Directory names never descended during a scan: the marker dir plus VCS / build /
/// dependency trees that may hold stray `*.env*` files irrelevant to the project (and
/// would bloat the encrypted tree). Keeps the sync to the project's own files.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "dist",
    "build",
    "vendor",
    ".cache",
    "coverage",
    "__pycache__",
    ".next",
    ".venv",
    ".claude",
    ".vault",
    "baas.bak",
];

/// Whether a directory `name` should be skipped (not descended) during a scan.
fn skip_dir(name: &str) -> bool {
    SKIP_DIRS.contains(&name)
}

/// Whether `dir` is the root of its own git repository — a submodule, or a nested checkout.
///
/// `.git` is a DIRECTORY in an ordinary clone and a FILE in a submodule (holding a gitdir
/// pointer), so both shapes are accepted. Anything else is not a repository boundary.
fn is_repository_root(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Scan the repository roots sitting directly inside a skipped directory, and nothing else.
///
/// A submodule parked under `vendor/` had its environment and its whole `secrets/` directory
/// dropped, and `push` reported success — the same silent shape as the `secrets/` omission, and
/// the exact layout an operator with several projects and submodules ends up with.
///
/// Dropping `vendor` from the skip list would fix this instance and break what the list is for,
/// sweeping genuinely vendored trees into the vault. So the skip is kept and pierced for one
/// case: a directory that is its own repository is a project boundary, and projects are the
/// things that have secrets. Vendored source that is not a repository stays skipped.
///
/// ONE LEVEL DEEP on purpose. `vendor/<submodule>` is the layout that loses credentials, while
/// searching a whole `node_modules` for a `.git` would cost far more than the case is worth —
/// and a scan that walks a dependency tree exhaustively is a scan people turn off.
fn walk_repositories_inside(
    dir: &Path,
    patterns: &[String],
    out: &mut Vec<PathBuf>,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.file_type().is_symlink() || !meta.is_dir() {
            continue;
        }
        let path = entry.path();
        if is_repository_root(&path) {
            walk(&path, patterns, false, out)?;
        }
    }
    Ok(())
}

/// Directories whose every regular file is a secret, matched by directory name at any depth.
///
/// The patterns are file-NAME globs, so they cannot see these files at all. Docker Compose
/// mounts secrets by path out of a `secrets/` directory, and those files are named for what
/// they hold — `db_password.txt`, `server.crt`, `server.key` — never for the fact that they
/// are secret. Nothing about a name-only scan can be widened to cover that, because there is
/// no name to widen to.
///
/// Measured against the real Inception project on the deployed vault: `push` sealed two files,
/// skipped six including a TLS private key, printed `pushed 2 file(s)` and exited 0. The pull
/// on the other machine then restored a project that could not start, and no step of that
/// reported an error.
const SECRET_DIRS: &[&str] = &["secrets", ".secrets"];

/// Whether every regular file under a directory called `name` is a secret.
fn is_secret_dir(name: &str) -> bool {
    SECRET_DIRS.contains(&name)
}

/// Whether a *matched* file should still be skipped: deliberately-stale (`*.stale`)
/// or backup (`*.bak*`) copies that shadow a real env file and are not real secrets.
fn skip_file(name: &str) -> bool {
    name.ends_with(".stale") || name.contains(".bak")
}

/// Recursively collect matching files, skipping the marker + build/dep dirs + symlinks.
///
/// `all` is set once the walk is inside a secret directory and stays set below it, so a
/// `secrets/tls/server.key` is taken as surely as a `secrets/db_password.txt`. The skip list
/// still applies underneath, so a `secrets/node_modules` is not swept into the vault.
fn walk(dir: &Path, patterns: &[String], all: bool, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        let name = entry.file_name().to_string_lossy().to_string();
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            let path = entry.path();
            if name == MARKER_DIR {
                continue;
            }
            if skip_dir(&name) {
                walk_repositories_inside(&path, patterns, out)?;
            } else {
                walk(&path, patterns, all || is_secret_dir(&name), out)?;
            }
        } else if (all || matches(&name, patterns)) && !skip_file(&name) {
            out.push(entry.path());
        }
    }
    Ok(())
}

/// Whether `name` matches any configured pattern.
fn matches(name: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| glob_match(name, p))
}

/// A minimal glob: a single optional leading and/or trailing `*` (covers `*.env*`,
/// `*.secrets`, `prefix*`, exact).
fn glob_match(name: &str, pattern: &str) -> bool {
    let core = pattern.trim_matches('*');
    match (pattern.starts_with('*'), pattern.ends_with('*')) {
        (true, true) => name.contains(core),
        (true, false) => name.ends_with(core),
        (false, true) => name.starts_with(core),
        (false, false) => name == core,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a throwaway tree and return its root.
    fn tree(tag: &str, files: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("42ctl-scan-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for rel in files {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().expect("has a parent")).expect("mkdir");
            std::fs::write(&path, b"x").expect("write");
        }
        root
    }

    fn scan_names(root: &Path) -> Vec<String> {
        let project = mk(root.to_path_buf(), "test", default_patterns());
        let mut names: Vec<String> = scan(&project)
            .expect("scan")
            .iter()
            .map(|p| {
                p.strip_prefix(root)
                    .expect("under root")
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        names.sort();
        names
    }

    /// A submodule parked under a skipped directory keeps its environment and its secrets.
    ///
    /// `vendor` is on the skip list, correctly, for vendored source. A submodule put there is a
    /// project boundary rather than vendored code, and dropping its files while reporting a
    /// successful push is the same silent shape as the `secrets/` omission.
    #[test]
    fn a_submodule_under_a_skipped_directory_is_still_scanned() {
        let root = tree(
            "submodule",
            &[
                "srcs/.env",
                "vendor/thirdparty/.git",
                "vendor/thirdparty/srcs/.env",
                "vendor/thirdparty/secrets/db_password.txt",
                "vendor/plainlib/config.env",
                "node_modules/pkg/.env",
            ],
        );
        let found = scan_names(&root);
        assert!(
            found.iter().any(|f| f == "srcs/.env"),
            "positive control: the root project's own file must be found, or this proves nothing"
        );
        for want in [
            "vendor/thirdparty/srcs/.env",
            "vendor/thirdparty/secrets/db_password.txt",
        ] {
            assert!(
                found.iter().any(|f| f == want),
                "{want} belongs to a submodule and must be scanned; got {found:?}"
            );
        }
        assert!(
            !found.iter().any(|f| f == "vendor/plainlib/config.env"),
            "vendored source that is NOT a repository must stay skipped, or this widening \
             sweeps the trees the skip list exists for; got {found:?}"
        );
        assert!(
            !found.iter().any(|f| f.starts_with("node_modules/")),
            "a dependency tree with no repository inside it must stay skipped; got {found:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The whole `secrets/` tree is taken, and this is the case that was silently lost.
    ///
    /// These are Inception's six real secret files. Not one of them matches `*.env*` or
    /// `*.secrets`, because they are named for what they hold. Before this, `push` sealed
    /// `srcs/.env`, skipped all six, printed `pushed 2 file(s)` and exited 0.
    #[test]
    fn every_file_under_a_secrets_directory_is_scanned() {
        let root = tree(
            "secrets",
            &[
                "srcs/.env",
                "secrets/credentials.txt",
                "secrets/db_password.txt",
                "secrets/db_root_password.txt",
                "secrets/ftp_password.txt",
                "secrets/server.crt",
                "secrets/server.key",
            ],
        );
        let found = scan_names(&root);
        for want in [
            "secrets/credentials.txt",
            "secrets/db_password.txt",
            "secrets/db_root_password.txt",
            "secrets/ftp_password.txt",
            "secrets/server.crt",
            "secrets/server.key",
        ] {
            assert!(found.iter().any(|f| f == want), "{want} was not scanned");
        }
        assert!(
            found.iter().any(|f| f == "srcs/.env"),
            "positive control: the pattern-matched file must still be found, \
             or this test proves nothing about the directory rule"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Nested directories under `secrets/` stay covered, and the skip list still applies
    /// underneath so a dependency tree that happens to sit there is not swept into the vault.
    #[test]
    fn a_secrets_directory_covers_its_subtree_but_not_the_skip_list() {
        let root = tree(
            "nested",
            &[
                "secrets/tls/server.key",
                "secrets/node_modules/pkg/leftover.pem",
            ],
        );
        let found = scan_names(&root);
        assert!(
            found.iter().any(|f| f == "secrets/tls/server.key"),
            "a nested secret must still be scanned, got {found:?}"
        );
        assert!(
            !found.iter().any(|f| f.contains("node_modules")),
            "a dependency tree under secrets/ must not be swept in, got {found:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A file that merely mentions the word is not a secret directory, and an ordinary
    /// directory is still scanned by pattern alone.
    #[test]
    fn only_a_directory_named_secrets_widens_the_scan() {
        assert!(is_secret_dir("secrets"));
        assert!(is_secret_dir(".secrets"));
        for other in ["secret", "secretsx", "my-secrets", "Secrets", "SECRETS"] {
            assert!(!is_secret_dir(other), "{other} must not widen the scan");
        }
        let root = tree("plain", &["conf/db_password.txt", "conf/app.env"]);
        let found = scan_names(&root);
        assert_eq!(
            found,
            vec!["conf/app.env".to_string()],
            "outside a secrets/ directory the name patterns still decide"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
