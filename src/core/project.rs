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
//! patterns (`*.env*`, `*.secrets`), skipping `.42ctl/` and symlinks.

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
    walk(&project.root, &project.patterns, &mut out)?;
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
    name == MARKER_DIR || SKIP_DIRS.contains(&name)
}

/// Whether a *matched* file should still be skipped: deliberately-stale (`*.stale`)
/// or backup (`*.bak*`) copies that shadow a real env file and are not real secrets.
fn skip_file(name: &str) -> bool {
    name.ends_with(".stale") || name.contains(".bak")
}

/// Recursively collect matching files, skipping the marker + build/dep dirs + symlinks.
fn walk(dir: &Path, patterns: &[String], out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        let name = entry.file_name().to_string_lossy().to_string();
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            if !skip_dir(&name) {
                walk(&entry.path(), patterns, out)?;
            }
        } else if matches(&name, patterns) && !skip_file(&name) {
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
