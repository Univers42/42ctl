/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   projpath.rs                                          :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Path canonicalization + traversal defense for project push/pull. `validate_stored`
//! is the load-bearing Zip-Slip guard: a PURE string check (no filesystem access) that
//! refuses any path which could escape the project root when materialized. A violating
//! path is treated as hostile (an error), never sanitized.
//!
//! ponytail: NFC unicode normalization is not applied (a decomposed vs precomposed name
//! could collide on a case/normalization-insensitive FS) — add `unicode-normalization`
//! if cross-platform collision becomes a concern. The traversal guards below are
//! normalization-independent.

use std::path::{Component, Path, PathBuf};

const MAX_PATH_LEN: usize = 4096;
const MAX_COMPONENT_LEN: usize = 255;

/// The reserved prefix for vault42's own blobs/manifest — a project file may never use it.
pub const RESERVED_PREFIX: &str = "__42ctl";

/// A validated, canonical relative POSIX path (forward slashes, no traversal).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelPath(String);

impl RelPath {
    /// The canonical POSIX string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validate a stored (manifest) path string BEFORE it touches the filesystem. Refuses
/// absolute, `.`/`..`, backslash, drive-letter, UNC, NUL, reserved Windows names,
/// over-long, empty, and the reserved `__42ctl` prefix.
pub fn validate_stored(raw: &str) -> anyhow::Result<RelPath> {
    if raw.is_empty() || raw.len() > MAX_PATH_LEN {
        anyhow::bail!("path empty or too long");
    }
    // sec: reject NUL, backslash, absolute, and drive/UNC forms outright
    if raw.as_bytes().contains(&0)
        || raw.contains('\\')
        || raw.starts_with('/')
        || has_drive_or_unc(raw)
    {
        anyhow::bail!("illegal path form (nul/backslash/absolute/drive/unc)");
    }
    for comp in raw.split('/') {
        check_component(comp)?;
    }
    if raw.split('/').next() == Some(RESERVED_PREFIX) {
        anyhow::bail!("path uses the reserved {RESERVED_PREFIX} prefix");
    }
    Ok(RelPath(raw.to_string()))
}

/// Validate one path component (the per-segment Zip-Slip + Windows-portability guard).
fn check_component(comp: &str) -> anyhow::Result<()> {
    // sec: empty (//, leading/trailing slash), dot, or dot-dot are all traversal vectors
    if comp.is_empty() || comp == "." || comp == ".." {
        anyhow::bail!("empty or dot path component");
    }
    if comp.len() > MAX_COMPONENT_LEN {
        anyhow::bail!("path component too long");
    }
    if is_windows_reserved(comp) || comp.ends_with(' ') || comp.ends_with('.') {
        anyhow::bail!("reserved or trailing-space/dot component");
    }
    Ok(())
}

/// Detect a Windows drive prefix (`C:`) or UNC (`//`) form.
fn has_drive_or_unc(raw: &str) -> bool {
    let b = raw.as_bytes();
    (b.len() >= 2 && b[1] == b':') || raw.starts_with("//")
}

/// Reserved Windows device names (case-insensitive, on the stem before the first dot).
fn is_windows_reserved(comp: &str) -> bool {
    let stem = comp.split('.').next().unwrap_or(comp).to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit())
}

/// Join a validated rel path under `root` as a native path.
pub fn to_native(root: &Path, rel: &RelPath) -> PathBuf {
    let mut path = root.to_path_buf();
    for comp in rel.0.split('/') {
        path.push(comp);
    }
    path
}

/// Canonicalize a real file under `root` into a stored RelPath (push side): resolve
/// symlinks, strip the root prefix, reject anything outside.
// sec: strip_prefix on two canonicalized absolute paths defeats symlink-escape.
pub fn canonicalize_for_storage(file: &Path, root: &Path) -> anyhow::Result<RelPath> {
    let abs = std::fs::canonicalize(file)?;
    let root_canon = std::fs::canonicalize(root)?;
    let rel = abs
        .strip_prefix(&root_canon)
        .map_err(|_| anyhow::anyhow!("file escapes the project root"))?;
    let mut parts = Vec::new();
    for comp in rel.components() {
        match comp {
            Component::Normal(s) => parts.push(s.to_string_lossy().to_string()),
            _ => anyhow::bail!("non-normal path component"),
        }
    }
    validate_stored(&parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_safe_relative_paths() {
        for p in [
            "config/db.env",
            "a/b/c.env",
            ".env",
            "deep/nested/x.secrets",
        ] {
            assert!(validate_stored(p).is_ok(), "should accept {p}");
        }
    }

    #[test]
    fn refuses_every_traversal_form() {
        for p in [
            "/etc/passwd",
            "../escape",
            "a/../../b",
            "a/./b",
            "a\\b",
            "C:\\x",
            "//unc/x",
            "a//b",
            "CON",
            "com1.txt",
            "trailingdot.",
            "trailingspace ",
            "__42ctl/secret",
            "",
        ] {
            assert!(validate_stored(p).is_err(), "should refuse {p}");
        }
        assert!(validate_stored("a\0b").is_err(), "should refuse NUL");
    }
}
