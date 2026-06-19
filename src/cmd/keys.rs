/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   keys.rs                                              :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl keys` — manage the local zero-knowledge identity (X25519 encryption + Ed25519
//! signing keypair), sealed in a passphrase-wrapped keystore. The private key never leaves
//! the machine and is never exported in plaintext; `export-pub` prints only the public
//! address.

use crate::adapters::{address, keystore, passphrase};
use crate::cli::Keys;
use crate::ui;
use vault42_core::{seal_keystore, Identity, KdfParams};

/// Dispatch a `keys` subcommand.
pub fn run(cmd: &Keys, _profile: &str) -> anyhow::Result<()> {
    match cmd {
        Keys::Init { force } => init(*force),
        Keys::ExportPub => export_pub(),
    }
}

/// Generate a fresh identity, seal it under a new passphrase, and write the keystore.
fn init(force: bool) -> anyhow::Result<()> {
    let path = keystore::keystore_path()?;
    if path.exists() && !force {
        anyhow::bail!(
            "keystore already exists at {} (use --force)",
            path.display()
        );
    }
    let identity = Identity::generate();
    let passphrase = passphrase::prompt_new_passphrase()?;
    let blob = seal_keystore(&identity, passphrase.as_bytes(), KdfParams::default())?;
    keystore::save(&path, &blob)?;
    let principal = hex::encode(vault42_core::fingerprint(
        &identity.author_public().to_bytes(),
    ));
    ui::success("identity created");
    ui::field("principal", &principal);
    ui::field("address", &address::encode(&identity));
    Ok(())
}

/// Print this identity's shareable public address (unlocks the keystore, prints no key).
fn export_pub() -> anyhow::Result<()> {
    let identity = passphrase::unlock()?;
    println!("{}", address::encode(&identity));
    Ok(())
}
