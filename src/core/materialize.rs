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
use crate::ui;
use std::path::Path;
use zeroize::Zeroizing;

/// One file to materialize: where, the decrypted bytes, and the mode to restore.
pub struct Plan {
    pub rel: RelPath,
    pub bytes: Zeroizing<Vec<u8>>,
    pub mode: u32,
}

/// The overwrite policy for `pull`.
pub struct Opts {
    pub apply: bool,
    pub force: bool,
    pub backup: bool,
}

/// Validate every target, then (when applying) write each byte-exact. A failure during
/// validation writes NOTHING; per-file writes are atomic temp-then-rename.
pub fn materialize(root: &Path, plans: Vec<Plan>, opts: &Opts) -> anyhow::Result<()> {
    for plan in &plans {
        guard(root, &plan.rel)?;
    }
    for plan in &plans {
        let target = projpath::to_native(root, &plan.rel);
        let exists = target.exists();
        if !opts.apply {
            ui::field(
                plan.rel.as_str(),
                if exists {
                    "would overwrite"
                } else {
                    "would create"
                },
            );
            continue;
        }
        if exists && !opts.force {
            println!(
                "{}",
                ui::warn(&format!(
                    "skip {} (exists — use --force)",
                    plan.rel.as_str()
                ))
            );
            continue;
        }
        if exists && opts.backup {
            let _ = std::fs::rename(&target, target.with_extension("bak"));
        }
        write_atomic(&target, &plan.bytes)?;
        apply_mode(&target, plan.mode);
        ui::success(plan.rel.as_str());
    }
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
