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
//! pin the granter (the Ed25519 key the wire returned), unwrap with the caller's X25519 key,
//! and check the result against the key the environment advertises. The recovered secret
//! stays in a `Zeroizing` buffer; only members ever get this far.
//!
//! That last check is what makes the granter pin meaningful. The granter is whatever key the
//! wire returned, and a wrap deposit is not restricted to the depositor's own namespace, so
//! any authenticated account can overwrite a member's wrap with a self-consistent grant of a
//! key it chose. Without the check the victim's client accepts it and fails later with an
//! opaque decryption error. Comparing the recovered secret's public half to the environment's
//! published scope key turns that into a named refusal at the point of substitution.

use crate::adapters::api::Session;
use crate::adapters::scope;
use vault42_core::{
    open_scope_key, AuthorPublicKey, GrantedScopeKey, RecipientPublicKey, RecipientSecretKey,
};
use zeroize::Zeroizing;

/// Recover the scope secret for `(scope_id, epoch)` from the caller's own deposited wrap, and
/// refuse it unless it matches the scope key `advertised` for the environment (base64 X25519,
/// `None` for an env with no published key yet). Errors when the caller has no wrap (not yet
/// provisioned / not the bootstrapping admin).
pub async fn recover_scope_secret(
    session: &mut Session,
    scope_id: [u8; 16],
    epoch: u32,
    advertised: Option<&str>,
) -> anyhow::Result<Zeroizing<[u8; 32]>> {
    let (blob, granter) = session
        .get_scope_key(&hex::encode(scope_id), epoch)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no scope key for this env — run `vault env-init` first"))?;
    let grant = GrantedScopeKey::from_bytes(&blob)
        .map_err(|_| anyhow::anyhow!("stored scope-key grant is malformed"))?;
    let granter_pub = granter_key(&granter)?;
    let member_secret = session.identity.encryption_secret();
    let secret = open_scope_key(&grant, member_secret, &granter_pub)
        .map_err(|_| anyhow::anyhow!("could not open the scope key — are you a wrapped member?"))?;
    check_advertised(&secret, advertised)?;
    Ok(secret)
}

/// Refuse a recovered secret whose public half is not the scope key the environment publishes.
///
/// The scope secret IS the X25519 scalar, so its public half is derivable and comparable
/// without any new crypto. A mismatch means the wrap this client just opened grants some other
/// key than the one every secret in the environment was sealed to — the signature of a
/// substituted wrap — so it is named here rather than surfacing as a decryption failure much
/// later. Both values are public, so a plain comparison leaks nothing.
fn check_advertised(secret: &Zeroizing<[u8; 32]>, advertised: Option<&str>) -> anyhow::Result<()> {
    let Some(advertised) = advertised.filter(|key| !key.is_empty()) else {
        return Ok(());
    };
    let expected = scope::x25519_pub(advertised)?;
    let derived = RecipientPublicKey::from(&RecipientSecretKey::from(**secret));
    if derived.to_bytes() != expected.to_bytes() {
        anyhow::bail!(
            "the scope key you recovered is not the one this environment publishes — \
             your wrap was replaced; ask an administrator to run `vault sync-keys`"
        );
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    use vault42_core::generate_keyset;

    /// A secret recovered from the environment's real keyset is accepted, and the base64 the
    /// control plane publishes is the derived public half.
    #[test]
    fn the_environments_own_scope_secret_is_accepted() {
        let (keyset, secret) = generate_keyset([1u8; 16], 1);
        let advertised = STANDARD.encode(keyset.public.to_bytes());
        check_advertised(&secret, Some(&advertised)).expect("the real secret must be accepted");
    }

    /// A secret for some other keyset is refused by name rather than failing later as an
    /// opaque decryption error. This is the substituted-wrap case.
    #[test]
    fn a_secret_for_another_key_is_refused() {
        let (keyset, _mine) = generate_keyset([1u8; 16], 1);
        let (_other, substituted) = generate_keyset([1u8; 16], 1);
        let advertised = STANDARD.encode(keyset.public.to_bytes());
        let refused = check_advertised(&substituted, Some(&advertised))
            .expect_err("a foreign scope secret must be refused");
        assert!(refused
            .to_string()
            .contains("not the one this environment publishes"));
    }

    /// An environment with no published key yet cannot be checked against anything, so the
    /// check abstains instead of inventing a comparison. An empty string is the same case.
    #[test]
    fn an_unpublished_scope_key_skips_the_check() {
        let (_keyset, secret) = generate_keyset([2u8; 16], 1);
        check_advertised(&secret, None).expect("nothing to compare against");
        check_advertised(&secret, Some("")).expect("an empty advertisement is not a key");
    }

    /// A malformed advertisement is an error, never silently treated as absent — the fallback
    /// would be exactly the silence this check exists to remove.
    #[test]
    fn a_malformed_advertised_key_is_an_error() {
        let (_keyset, secret) = generate_keyset([3u8; 16], 1);
        assert!(check_advertised(&secret, Some("not base64!!")).is_err());
        assert!(check_advertised(&secret, Some(&STANDARD.encode([0u8; 16]))).is_err());
    }
}
