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

/// Create `dir` and any missing ancestors, owner-only.
///
/// A directory created while restoring a tree of credentials exists for the credentials in it,
/// so it takes the same posture they do: the caller narrows a file to 0600, and this narrows
/// the directory holding it to 0700. Restoring a tree whose `secrets/` no longer exists would
/// otherwise recreate it at whatever the umask happens to say — commonly 0775, which lets every
/// local process LIST the secret filenames. The bytes stay sealed, but an inventory of what a
/// project keeps secret is a disclosure of its own, and it appears only on the restore path,
/// which is exactly where nobody is looking.
///
/// EXISTING directories are left alone: the mode applies only to what this creates, so a
/// project that deliberately widened a directory keeps its choice.
fn create_owner_only(dir: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

/// Write `bytes` to `target` byte-exact via a sibling temp file + atomic rename.
fn write_atomic(target: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = target.parent() {
        create_owner_only(parent)?;
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode_of(path: &Path) -> u32 {
        std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    }

    /// A directory this creates during a restore must not be listable by anyone else.
    ///
    /// The whole point of the restore path is that the tree is GONE — `secrets/` included — so
    /// every ancestor is created here, at whatever the umask says unless this narrows it. A
    /// 0775 `secrets/` leaks the inventory of what a project keeps secret to any local process,
    /// while every file inside is correctly 0600 and nothing looks wrong.
    #[test]
    fn a_created_parent_is_owner_only_at_every_depth() {
        let root = std::env::temp_dir().join(format!("v42-mat-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let deep = root.join("a/b/c/secrets");
        create_owner_only(&deep).expect("create");
        for dir in [root.join("a"), root.join("a/b"), root.join("a/b/c"), deep] {
            assert_eq!(mode_of(&dir), 0o700, "{} must be owner-only", dir.display());
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An existing directory keeps the mode its owner chose. Narrowing one we did not create
    /// would silently retighten a tree the project deliberately opened up.
    #[test]
    fn an_existing_directory_keeps_its_own_mode() {
        let root = std::env::temp_dir().join(format!("v42-mat-keep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create");
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        create_owner_only(&root).expect("recreate");
        assert_eq!(
            mode_of(&root),
            0o755,
            "an existing directory must be left alone"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
