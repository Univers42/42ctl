/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   privatefile.rs                                       :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Writing a file only its owner can read.
//!
//! Three things this crate stores are bearer credentials: the passphrase-wrapped keystore,
//! the vault contract, and the control-plane session token. Any of them read by another
//! account on the same machine is that person acting as you — the keystore still needs the
//! passphrase, but the other two are enough on their own.
//!
//! The keystore was written and then chmodded; the other two were written with whatever the
//! umask allowed, which on an ordinary desktop is group- and world-readable. That is how one
//! of three got the treatment: the rule lived at one call site instead of in one place. It
//! lives here now, and every credential goes through it.
//!
//! The mode is set AT CREATION rather than after. Write-then-chmod leaves the bytes readable
//! for the width of two syscalls, which is short and is not zero, and a reader that loses
//! that race only has to try again.

use std::path::Path;

/// Write `bytes` to `path`, creating it readable and writable by its owner alone.
///
/// Parent directories are created first. An existing file is truncated but keeps whatever
/// mode it already had, so this also repairs a file written before the rule existed.
pub fn write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    create(path)?;
    std::fs::write(path, bytes)?;
    restrict(path)
}

/// Create (or truncate) `path` with an owner-only mode from the moment it exists.
#[cfg(unix)]
fn create(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    Ok(())
}

#[cfg(not(unix))]
fn create(path: &Path) -> anyhow::Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    Ok(())
}

/// Narrow an existing file to its owner, repairing one written before this rule existed.
#[cfg(unix)]
fn restrict(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path).expect("stat").mode() & 0o777
    }

    /// The whole point: whatever the umask says, a credential is owner-only.
    #[cfg(unix)]
    #[test]
    fn a_credential_is_written_owner_only() {
        let dir = std::env::temp_dir().join(format!("privatefile-{}", std::process::id()));
        let path = dir.join("token.tok");
        write(&path, b"bearer-value").expect("write");
        assert_eq!(mode_of(&path), 0o600, "a credential must be owner-only");
        assert_eq!(std::fs::read(&path).expect("read"), b"bearer-value");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file left group-readable by an older version is repaired on the next write, so an
    /// existing installation stops leaking without anybody having to notice.
    #[cfg(unix)]
    #[test]
    fn an_already_open_file_is_narrowed_on_the_next_write() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("privatefile-old-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("token.tok");
        std::fs::write(&path, b"old").expect("seed");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("widen");
        write(&path, b"new").expect("rewrite");
        assert_eq!(mode_of(&path), 0o600, "an old wide file must be narrowed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A missing parent directory is created rather than reported, because the first login
    /// on a fresh machine is exactly when neither exists.
    #[cfg(unix)]
    #[test]
    fn a_missing_directory_is_created() {
        let dir = std::env::temp_dir().join(format!("privatefile-deep-{}", std::process::id()));
        let path = dir.join("nested").join("token.tok");
        write(&path, b"v").expect("write into a missing directory");
        assert_eq!(mode_of(&path), 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
