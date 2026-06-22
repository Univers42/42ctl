/* ************************************************************************** */
/*                                                                            */
/*                                                        :::      ::::::::   */
/*   keys.rs                                            :+:      :+:    :+:   */
/*                                                    +:+ +:+         +:+     */
/*   By: dlesieur <dlesieur@student.42.fr>          +#+  +:+       +#+        */
/*                                                +#+#+#+#+#+   +#+           */
/*   Created: 2026/06/19 00:00:00 by dlesieur          #+#    #+#             */
/*   Updated: 2026/06/21 04:27:12 by dlesieur         ###   ########.fr       */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl keys` — manage the local zero-knowledge identity (X25519 + Ed25519 keypair),
//! sealed in a passphrase-wrapped keystore. The private key never leaves the machine and
//! is never exported in plaintext. `escrow`/`recover` move the SEALED keystore between
//! devices through grobase (gated by an email OTP); the server only ever holds ciphertext.

use crate::adapters::{address, escrow, keystore, otp, passphrase, session};
use crate::cli::Keys;
use crate::cmd::scope_pubkey;
use crate::profile::Config;
use crate::ui;
use anyhow::Context;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use vault42_core::{fingerprint, seal_keystore, Identity, KdfParams, KeystoreBlob};

/// Dispatch a `keys` subcommand. Init/export-pub are offline; escrow/recover hit grobase.
pub async fn run(cmd: &Keys, profile: &str) -> anyhow::Result<()> {
    match cmd {
        Keys::Init { force } => init(*force),
        Keys::ExportPub => export_pub(),
        Keys::Enroll { org } => enroll(profile, org).await,
        Keys::Escrow { email } => escrow_keystore(profile, email).await,
        Keys::Recover { email } => recover(profile, email).await,
    }
}

/// Publish this identity's public keys to grobase's wrap-target registry so a scope admin's
/// `sync-keys` can wrap environment keys to this member. Unlocks the keystore only to sign
/// the proof-of-possession; the private key never leaves the machine.
async fn enroll(profile: &str, org: &str) -> anyhow::Result<()> {
    let (grobase, token) = session::connect(profile)?;
    let identity = passphrase::unlock()?;
    scope_pubkey::register_self(&grobase, &token, org, &identity).await?;
    ui::success(&format!(
        "pubkey enrolled in org {org} — an admin can now provision environment keys to you"
    ));
    Ok(())
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
    ui::success("identity created");
    ui::field(
        "principal",
        &hex::encode(fingerprint(&identity.author_public().to_bytes())),
    );
    ui::field("address", &address::encode(&identity));
    Ok(())
}

/// Print this identity's shareable public address (unlocks the keystore, prints no key).
fn export_pub() -> anyhow::Result<()> {
    let identity = passphrase::unlock()?;
    println!("{}", address::encode(&identity));
    Ok(())
}

/// Escrow the SEALED keystore to grobase for multi-device recovery: prove the email by
/// OTP, then upload the already-encrypted blob (the passphrase never leaves this machine).
async fn escrow_keystore(profile: &str, email: &str) -> anyhow::Result<()> {
    let endpoint = Config::load()?.endpoint(profile)?;
    let path = keystore::keystore_path()?;
    let raw = std::fs::read(&path)
        .with_context(|| format!("no keystore at {} — run `keys init` first", path.display()))?;
    let proof = otp::email_otp(endpoint.otp_base(), email).await?;
    escrow::put(endpoint.otp_base(), email, &proof, &STANDARD.encode(&raw)).await?;
    ui::success(&format!(
        "keystore escrowed for {email} (passphrase-wrapped; the server holds only ciphertext)"
    ));
    Ok(())
}

/// Recover the keystore on a new machine: OTP → fetch the escrow → write it locally →
/// unlock with the passphrase (which proves recovery worked and prints the principal).
async fn recover(profile: &str, email: &str) -> anyhow::Result<()> {
    let endpoint = Config::load()?.endpoint(profile)?;
    let path = keystore::keystore_path()?;
    if path.exists() {
        anyhow::bail!(
            "a keystore already exists at {} — move it aside first",
            path.display()
        );
    }
    let proof = otp::email_otp(endpoint.otp_base(), email).await?;
    let raw = STANDARD.decode(escrow::fetch(endpoint.otp_base(), email, &proof).await?)?;
    let blob: KeystoreBlob =
        serde_json::from_slice(&raw).context("escrow blob is not a valid keystore")?;
    keystore::save(&path, &blob)?;
    let identity = passphrase::unlock()?;
    ui::success("keystore recovered + unlocked on this machine");
    ui::field(
        "principal",
        &hex::encode(fingerprint(&identity.author_public().to_bytes())),
    );
    ui::field("address", &address::encode(&identity));
    Ok(())
}
