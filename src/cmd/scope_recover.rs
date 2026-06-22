/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope_recover.rs                                    :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Recover the env scope SECRET from the caller's OWN wrap — the two-hop unwrap shared by
//! every scope verb that must touch plaintext (`sync-keys`, `set-env`, `get-env`,
//! `rotate-scope`). Fetch the caller's wrap for `(scope_id, epoch)`, deserialize the grant,
//! pin the granter (the Ed25519 key the wire returned), and unwrap with the caller's X25519
//! key. The recovered secret stays in a `Zeroizing` buffer; only members ever get this far.

use crate::adapters::api::Session;
use vault42_core::{open_scope_key, AuthorPublicKey, GrantedScopeKey};
use zeroize::Zeroizing;

/// Recover the scope secret for `(scope_id, epoch)` from the caller's own deposited wrap.
/// Errors when the caller has no wrap (not yet provisioned / not the bootstrapping admin).
pub async fn recover_scope_secret(
    session: &mut Session,
    scope_id: [u8; 16],
    epoch: u32,
) -> anyhow::Result<Zeroizing<[u8; 32]>> {
    let (blob, granter) = session
        .get_scope_key(&hex::encode(scope_id), epoch)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no scope key for this env — run `vault env-init` first"))?;
    let grant = GrantedScopeKey::from_bytes(&blob)
        .map_err(|_| anyhow::anyhow!("stored scope-key grant is malformed"))?;
    let granter_pub = granter_key(&granter)?;
    let member_secret = session.identity.encryption_secret();
    open_scope_key(&grant, member_secret, &granter_pub)
        .map_err(|_| anyhow::anyhow!("could not open the scope key — are you a wrapped member?"))
}

/// Rebuild the granter's Ed25519 verifying key from the 32 raw bytes the wire returned,
/// rejecting a wrong length or a non-canonical encoding.
pub fn granter_key(bytes: &[u8]) -> anyhow::Result<AuthorPublicKey> {
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("granter pubkey must be 32 bytes"))?;
    AuthorPublicKey::from_bytes(&arr)
        .map_err(|_| anyhow::anyhow!("granter pubkey is not a valid key"))
}
