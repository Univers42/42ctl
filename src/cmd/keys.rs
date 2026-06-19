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
//! signing keypair), sealed in a passphrase-wrapped keystore. The private key never
//! leaves the machine and is never exported in plaintext. Stub until P3 (vault-crypto).

use crate::cli::Keys;

/// Stub: local identity management lands in P3.
pub fn run(_cmd: &Keys, _profile: &str) -> anyhow::Result<()> {
    println!("keys: wired in P3 — local identity via vault-crypto (X25519 + Ed25519)");
    Ok(())
}
