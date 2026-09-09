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

//! Member pubkey registration + proof-of-possession.
//!
//! `pubkey_sig` is an Ed25519 signature over `vault42_core::pop_message`, a canonical
//! LENGTH-FRAMED message binding the user id, the organization id, and the X25519 public key
//! being registered.
//!
//! The framing is load-bearing. The message was once a bare concatenation of the three values,
//! which is not injective: user "alice" in org "acme" and user "alicea" in org "cme" produce
//! identical bytes, so one member's proof verified as another's. Organization slugs are
//! user-chosen, so an attacker could pick a slug that made their pairing collide with a
//! victim's. Every field now carries its own length, and a domain tag stops a proof of
//! possession verifying as an envelope-author signature.
//!
//! The message is built by `vault42-core`, not here. This signer and the authority's verifier
//! call one definition, so they cannot drift apart — which a copy on each side eventually
//! would, silently, each half internally consistent while no longer agreeing.
//!
//! It binds the organization's canonical **id**, resolved via `GET /v1/orgs/{org}`, never the
//! alias the user typed. Two aliases for one organization would otherwise yield two different
//! valid proofs for the same key, and the authority verifies against the id.
//!
//! The caller's user id comes from `GET /v1/auth/me`, not from decoding a `sub` claim out of
//! the session token. The identity is asserted by the server that issued the session rather
//! than parsed by the client out of a credential it cannot verify.

use crate::adapters::rbac::{self, pubkey, MemberPubkey};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::json;
use vault42_core::{pop_message, sign_request, verify_request, Identity};

/// Register the caller's OWN public keys (idempotent). Signs the canonical
/// proof-of-possession with the identity's Ed25519 key so `sync-keys` can verify it.
pub async fn register_self(
    grobase: &str,
    token: &str,
    org: &str,
    identity: &Identity,
) -> anyhow::Result<()> {
    let x25519 = STANDARD.encode(identity.encryption_public().to_bytes());
    let ed25519 = STANDARD.encode(identity.author_public().to_bytes());
    let user_id = rbac::me(grobase, token).await?.account_id;
    let org_id = rbac::org::show(grobase, token, org).await?.id;
    let sig = sign_request(
        identity.signing_key(),
        &pop_message(&user_id, &org_id, &x25519),
    );
    let body = json!({
        "x25519_pub": x25519,
        "ed25519_pub": ed25519,
        "v42_address": crate::adapters::address::encode(identity),
        "pubkey_sig": STANDARD.encode(sig),
    });
    let _: MemberPubkey = pubkey::put(grobase, token, org, &body).await?;
    Ok(())
}

/// Verify a fetched member's proof of possession against the organization's canonical id.
///
/// `org_id` must be the id, not a slug — the same value the signer used. Returns `false` on any
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
    fn a_proof_cannot_be_replayed_across_a_colliding_user_and_org_pair() {
        let identity = Identity::generate();
        let pk = signed_pubkey(&identity, "alice", "acme");
        assert!(
            verify_member(&pk, "acme"),
            "the genuine pairing must verify"
        );
        let mut shifted = pk.clone();
        shifted.user_id = "alicea".to_string();
        assert!(
            !verify_member(&shifted, "cme"),
            "\"alice\"+\"acme\" and \"alicea\"+\"cme\" concatenate identically; \
             length framing must keep the proof bound to its own pairing"
        );
    }

    #[test]
    fn the_pop_message_is_injective_over_field_boundaries() {
        let a = pop_message("alice", "acme", "KEY");
        let b = pop_message("alicea", "cme", "KEY");
        assert_ne!(a, b, "a bare concatenation would make these equal");
        assert_ne!(pop_message("a", "bc", "K"), pop_message("ab", "c", "K"));
        assert_ne!(pop_message("", "abc", "K"), pop_message("abc", "", "K"));
    }

    #[test]
    fn the_pop_message_is_domain_separated() {
        let msg = pop_message("user-1", "org-1", "KEY");
        let prefix = b"14:vault42/pop/v1\n";
        assert!(
            msg.starts_with(prefix),
            "the domain tag must lead the message"
        );
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
