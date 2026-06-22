/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   materialize.rs                                       :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Materialize decrypted blobs into the project tree — byte-exact + traversal-safe.
//! Every path is validated, no symlinked ancestor may be traversed (the materialize-time
//! Zip-Slip kill), writes are temp-then-rename, and the mode is restored. Dry-run by
//! default; `--apply` writes; existing files are skipped unless `--force`; `--backup`
//! keeps a `.bak`. ponytail: per-file atomic (validate-all-first), not a whole-tree journal.

use crate::core::projpath::{self, RelPath};
use std::path::Path;

/// The pull policy: dry-run unless `apply`; `force` takes remote even on divergence (no
/// conflict markers); `backup` keeps a `.bak` before overwriting an existing file.
pub struct Opts {
    pub apply: bool,
    pub force: bool,
    pub backup: bool,
}

/// Write one resolved file byte-exact: traversal guard, optional `.bak` of an existing
/// target, atomic temp-then-rename, mode restore. The `pull` reconciler decides per-file
/// what to write, then calls this.
pub(crate) fn write_one(
    root: &Path,
    rel: &RelPath,
    bytes: &[u8],
    mode: u32,
    backup: bool,
) -> anyhow::Result<()> {
    guard(root, rel)?;
    let target = projpath::to_native(root, rel);
    if backup && target.exists() {
        let _ = std::fs::rename(&target, target.with_extension("bak"));
    }
    write_atomic(&target, bytes)?;
    apply_mode(&target, mode);
    Ok(())
}

/// Refuse to materialize through a symlinked ancestor (existing ancestors only — the
/// rest are created fresh by `write_atomic`).
// sec: symlink_metadata does NOT follow, so a pre-planted `sub -> /etc` is rejected.
fn guard(root: &Path, rel: &RelPath) -> anyhow::Result<()> {
    let target = projpath::to_native(root, rel);
    let mut ancestor = target.parent();
    while let Some(dir) = ancestor {
        if let Ok(meta) = std::fs::symlink_metadata(dir) {
            if meta.file_type().is_symlink() {
                anyhow::bail!(
                    "refusing to write through a symlinked path: {}",
                    rel.as_str()
                );
            }
        }
        if dir == root {
            break;
        }
        ancestor = dir.parent();
    }
    Ok(())
}

/// Write `bytes` to `target` byte-exact via a sibling temp file + atomic rename.
fn write_atomic(target: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = target.with_extension("42ctl-tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, target)?;
    Ok(())
}

/// Restore the Unix file mode (best-effort; no-op on non-Unix).
fn apply_mode(target: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(target, std::fs::Permissions::from_mode(mode));
    }
    #[cfg(not(unix))]
    {
        let _ = (target, mode);
    }
}
