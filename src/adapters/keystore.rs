/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   keystore.rs                                          :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The local identity keystore — the passphrase-wrapped X25519+Ed25519 keypair, stored
//! as JSON of the already-encrypted `KeystoreBlob` (only ciphertext + public KDF params)
//! at `$FT_KEYSTORE` or `~/.config/42ctl/keystore.v42`, written `0600`. The private keys
//! never leave the process. (OS-keyring-first storage is a P2/P7 enhancement; the
//! Argon2id file keystore is the portable fallback the prompt sanctions.)

use anyhow::Context;
use std::path::{Path, PathBuf};
use vault42_core::KeystoreBlob;

/// The keystore path: `$FT_KEYSTORE` or the per-user default.
pub fn keystore_path() -> anyhow::Result<PathBuf> {
    if let Ok(custom) = std::env::var("FT_KEYSTORE") {
        return Ok(PathBuf::from(custom));
    }
    let base = dirs::config_dir().context("no config directory")?;
    Ok(base.join("42ctl").join("keystore.v42"))
}

/// Write the wrapped keystore blob, creating parents, owner-only.
pub fn save(path: &Path, blob: &KeystoreBlob) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec(blob)?)?;
    restrict(path)
}

/// Read the wrapped keystore blob (still encrypted).
pub fn load(path: &Path) -> anyhow::Result<KeystoreBlob> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

/// Restrict the keystore file to owner-only access on Unix.
fn restrict(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
