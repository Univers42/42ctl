/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope.rs                                            :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The scope-key crypto bridge — the LOCAL half of the orchestration. It derives the
//! deterministic `scope_id` both the admin and every member agree on, derives a member's
//! storage `member_id` from their Ed25519 public key (the same hex fingerprint vault42
//! uses for `Principal.id`), and decodes the base64 public keys grobase returns into the
//! dalek key types `grant_scope_key`/`open_scope_key` consume. Pure crypto, no I/O.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use vault42_core::{fingerprint, RecipientPublicKey};

/// Derive the deterministic 16-byte scope id for `(project_uuid, env_name)` as
/// `blake3(project_uuid_bytes ‖ env_name_bytes)[..16]`, where `project_uuid_bytes` is the
/// project UUID's 16 raw bytes and `env_name_bytes` is the env name's UTF-8 bytes. Both the
/// admin (`env-init`/`sync-keys`) and every member compute the same id; vault42 round-trips
/// its hex (`hex::encode`) opaquely, so this convention is the single source of truth.
pub fn scope_id(project_uuid: &str, env_name: &str) -> anyhow::Result<[u8; 16]> {
    let uuid = uuid::Uuid::parse_str(project_uuid)
        .map_err(|_| anyhow::anyhow!("project must be a UUID (got '{project_uuid}')"))?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(uuid.as_bytes());
    hasher.update(env_name.as_bytes());
    let mut id = [0u8; 16];
    id.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    Ok(id)
}

/// Derive a member's vault42 storage id from their Ed25519 public key: the same
/// `hex(fingerprint(ed25519_pub))` vault42-server computes for a caller's `Principal.id`,
/// so a wrap deposited under this id is exactly the one the member's `GetScopeKey` reads.
pub fn member_id(ed25519_pub_b64: &str) -> anyhow::Result<String> {
    let bytes = decode32(ed25519_pub_b64, "ed25519_pub")?;
    Ok(hex::encode(fingerprint(&bytes)))
}

/// Decode a base64 X25519 public key into the wrap-target type.
pub fn x25519_pub(b64: &str) -> anyhow::Result<RecipientPublicKey> {
    Ok(RecipientPublicKey::from(decode32(b64, "x25519_pub")?))
}

/// Decode a base64 string that must be exactly 32 bytes, naming `field` in any error.
fn decode32(b64: &str, field: &str) -> anyhow::Result<[u8; 32]> {
    let bytes = STANDARD
        .decode(b64)
        .map_err(|_| anyhow::anyhow!("{field} is not valid base64"))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("{field} must decode to 32 bytes"))?;
    Ok(arr)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT: &str = "11111111-1111-1111-1111-111111111111";

    #[test]
    fn scope_id_is_deterministic_and_16_bytes() {
        let a = scope_id(PROJECT, "prod").expect("derive a");
        let b = scope_id(PROJECT, "prod").expect("derive b");
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn scope_id_changes_with_env_and_project() {
        let prod = scope_id(PROJECT, "prod").expect("prod");
        let staging = scope_id(PROJECT, "staging").expect("staging");
        let other = scope_id("22222222-2222-2222-2222-222222222222", "prod").expect("other");
        assert_ne!(prod, staging);
        assert_ne!(prod, other);
    }

    #[test]
    fn scope_id_matches_blake3_of_uuid_bytes_then_name() {
        let uuid = uuid::Uuid::parse_str(PROJECT).expect("uuid");
        let mut hasher = blake3::Hasher::new();
        hasher.update(uuid.as_bytes());
        hasher.update(b"prod");
        let mut want = [0u8; 16];
        want.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
        assert_eq!(scope_id(PROJECT, "prod").expect("derive"), want);
    }

    #[test]
    fn scope_id_rejects_a_non_uuid_project() {
        assert!(scope_id("not-a-uuid", "prod").is_err());
    }

    #[test]
    fn member_id_is_hex_fingerprint_of_the_ed25519_key() {
        let key = [7u8; 32];
        let b64 = STANDARD.encode(key);
        assert_eq!(
            member_id(&b64).expect("derive"),
            hex::encode(fingerprint(&key))
        );
    }
}
