/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   decrypt.rs                                           :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Local envelope opening, shared by `get` and `share`. It pins the author key the
//! server returned by checking its fingerprint against the signature-bound id in the
//! envelope (so a malicious server cannot substitute a different author), then opens
//! the envelope with the caller's X25519 secret under the expected read scope.

use vault42_core::{open, AuthorPublicKey, Envelope, Identity, ReadScope, RecipientSecretKey};
use vault42_proto::vault::v1::GetResponse;
use zeroize::Zeroizing;

/// Verify authorship and decrypt `resp` for `identity`, binding the read to
/// `expected_secret_id` and `min_rev` (anti-substitution / anti-rollback).
pub fn open_envelope(
    identity: &Identity,
    resp: &GetResponse,
    expected_secret_id: &str,
    min_rev: u64,
) -> anyhow::Result<Zeroizing<Vec<u8>>> {
    let env = Envelope::from_bytes(&resp.envelope)?;
    let author = author_key(&resp.author_pubkey, &env)?;
    let scope = ReadScope {
        secret_id: expected_secret_id,
        min_rev,
    };
    Ok(open(&env, identity.encryption_secret(), &author, &scope)?)
}

/// Open an env-secret envelope with the recovered SCOPE secret (not the caller's key) —
/// the scope is the recipient, so a wrapped member opens it via the scope secret. Pins the
/// author the same way `open_envelope` does, binding the read to `expected_secret_id`/`min_rev`.
pub fn open_env_envelope(
    scope_secret: &Zeroizing<[u8; 32]>,
    envelope: &[u8],
    author_pubkey: &[u8],
    scope: ReadScope,
) -> anyhow::Result<Zeroizing<Vec<u8>>> {
    let env = Envelope::from_bytes(envelope)?;
    let author = author_key(author_pubkey, &env)?;
    let recipient = RecipientSecretKey::from(**scope_secret);
    Ok(open(&env, &recipient, &author, &scope)?)
}

/// Reconstruct the author public key the server returned, rejecting it unless its
/// fingerprint matches the envelope's signature-bound author id.
pub fn author_key(author_pubkey: &[u8], env: &Envelope) -> anyhow::Result<AuthorPublicKey> {
    if author_pubkey.len() != 32 {
        anyhow::bail!("author key must be 32 bytes");
    }
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(author_pubkey);
    if vault42_core::fingerprint(&bytes) != env.author_pubkey_id {
        anyhow::bail!("author key does not match the envelope");
    }
    AuthorPublicKey::from_bytes(&bytes).map_err(|_| anyhow::anyhow!("invalid author key"))
}

/// Open an env-secret envelope sealed to the CALLER — a private file inside a shared
/// environment. Beyond pinning the author as `open_env_envelope` does, the author must be
/// the caller themself: any writer of the environment can store bytes sealed to a public key
/// the registry publishes, and a "private" file somebody else wrote is the one thing a
/// private file must never be.
pub fn open_private_envelope(
    identity: &Identity,
    envelope: &[u8],
    author_pubkey: &[u8],
    scope: ReadScope,
) -> anyhow::Result<Zeroizing<Vec<u8>>> {
    let env = Envelope::from_bytes(envelope)?;
    let author = author_key(author_pubkey, &env)?;
    require_own_author(identity, author_pubkey)?;
    Ok(open(&env, identity.encryption_secret(), &author, &scope)?)
}

/// Refuse an author key that is not the caller's own.
pub fn require_own_author(identity: &Identity, author_pubkey: &[u8]) -> anyhow::Result<()> {
    if identity.author_public().to_bytes().as_slice() == author_pubkey {
        return Ok(());
    }
    anyhow::bail!("this private file was written by somebody else — refusing to restore it")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A private file written by anybody but the caller is refused before decryption is
    /// even attempted. Sealing to a published key is something every writer can do, so the
    /// recipient set alone proves nothing about who wrote it.
    #[test]
    fn a_private_envelope_by_somebody_else_is_refused() {
        let me = Identity::generate();
        let them = Identity::generate();
        require_own_author(&me, &me.author_public().to_bytes()).expect("my own author key");
        let err = require_own_author(&me, &them.author_public().to_bytes())
            .expect_err("a colleague's author key must be refused");
        assert!(err.to_string().contains("somebody else"), "{err}");
    }
}
