/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope_pubkey.rs                                     :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Member pubkey registration + proof-of-possession. `pubkey_sig` is an Ed25519 signature
//! over the canonical `user_id ‖ org_id ‖ x25519_pub_b64` bytes (the same framing on both
//! the register and the verify side, since 42ctl owns both). The caller's grobase user id
//! is the `sub` claim of the saved session JWT. Registration is idempotent (grobase upserts).

use crate::adapters::rbac::pubkey;
use crate::adapters::rbac::MemberPubkey;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use serde_json::json;
use vault42_core::{sign_request, verify_request, Identity};

/// The canonical proof-of-possession message: `user_id ‖ org_id ‖ x25519_pub_b64`.
fn pop_message(user_id: &str, org_id: &str, x25519_pub_b64: &str) -> Vec<u8> {
    let mut msg = Vec::with_capacity(user_id.len() + org_id.len() + x25519_pub_b64.len());
    msg.extend_from_slice(user_id.as_bytes());
    msg.extend_from_slice(org_id.as_bytes());
    msg.extend_from_slice(x25519_pub_b64.as_bytes());
    msg
}

/// Extract the `sub` claim (the grobase user id) from a JWT without verifying it — the
/// server already authenticated it; we only need the subject to frame the self-signature.
pub fn jwt_sub(token: &str) -> anyhow::Result<String> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("session token is not a JWT"))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| anyhow::anyhow!("session token payload is not base64url"))?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes)?;
    claims["sub"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("session token has no 'sub' claim"))
}

/// Register the caller's OWN public keys with grobase (idempotent). Signs the canonical
/// proof-of-possession with the identity's Ed25519 key so `sync-keys` can verify it.
pub async fn register_self(
    grobase: &str,
    token: &str,
    org: &str,
    identity: &Identity,
) -> anyhow::Result<()> {
    let x25519 = STANDARD.encode(identity.encryption_public().to_bytes());
    let ed25519 = STANDARD.encode(identity.author_public().to_bytes());
    let user_id = jwt_sub(token)?;
    let sig = sign_request(identity.signing_key(), &pop_message(&user_id, org, &x25519));
    let body = json!({
        "x25519_pub": x25519,
        "ed25519_pub": ed25519,
        "v42_address": crate::adapters::address::encode(identity),
        "pubkey_sig": STANDARD.encode(sig),
    });
    let _: MemberPubkey = pubkey::put(grobase, token, org, &body).await?;
    Ok(())
}

/// Verify a fetched member's proof-of-possession: the `pubkey_sig` must be a valid Ed25519
/// signature by `ed25519_pub` over `user_id ‖ org_id ‖ x25519_pub`. Returns `false` on any
/// malformed field so a bad pubkey is skipped, never wrapped to.
pub fn verify_member(pk: &MemberPubkey, org_id: &str) -> bool {
    let (Ok(ed), Ok(sig)) = (
        STANDARD.decode(&pk.ed25519_pub),
        STANDARD.decode(&pk.pubkey_sig),
    ) else {
        return false;
    };
    let (Ok(ed), Ok(sig)): (Result<[u8; 32], _>, Result<[u8; 64], _>) =
        (ed.try_into(), sig.try_into())
    else {
        return false;
    };
    verify_request(&ed, &pop_message(&pk.user_id, org_id, &pk.x25519_pub), &sig)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_pubkey(identity: &Identity, user: &str, org: &str) -> MemberPubkey {
        let x25519 = STANDARD.encode(identity.encryption_public().to_bytes());
        let ed25519 = STANDARD.encode(identity.author_public().to_bytes());
        let sig = sign_request(identity.signing_key(), &pop_message(user, org, &x25519));
        MemberPubkey {
            user_id: user.to_string(),
            x25519_pub: x25519,
            ed25519_pub: ed25519,
            pubkey_sig: STANDARD.encode(sig),
        }
    }

    #[test]
    fn verify_member_accepts_a_genuine_self_signature() {
        let identity = Identity::generate();
        let pk = signed_pubkey(&identity, "user-1", "org-1");
        assert!(verify_member(&pk, "org-1"));
    }

    #[test]
    fn verify_member_rejects_a_wrong_org_or_user_in_the_message() {
        let identity = Identity::generate();
        let pk = signed_pubkey(&identity, "user-1", "org-1");
        assert!(!verify_member(&pk, "org-2"));
        let mut moved = signed_pubkey(&identity, "user-1", "org-1");
        moved.user_id = "user-2".to_string();
        assert!(!verify_member(&moved, "org-1"));
    }

    #[test]
    fn verify_member_rejects_a_tampered_signature_or_malformed_field() {
        let identity = Identity::generate();
        let mut pk = signed_pubkey(&identity, "user-1", "org-1");
        let mut raw = STANDARD.decode(&pk.pubkey_sig).expect("sig");
        raw[0] ^= 0x01;
        pk.pubkey_sig = STANDARD.encode(&raw);
        assert!(!verify_member(&pk, "org-1"));
        pk.pubkey_sig = "not-base64!!".to_string();
        assert!(!verify_member(&pk, "org-1"));
    }
}
