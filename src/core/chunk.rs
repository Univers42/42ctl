/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   chunk.rs                                             :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Splitting a large object into chunks, and naming them.
//!
//! A file too large for one sealed envelope becomes many chunks in the object store plus a
//! chunk list in the vault. The list is ordinary envelope plaintext, exactly as the project
//! manifest is, so the author signature that already covers an envelope covers the order,
//! the count and every chunk's digest without any new signed object type.
//!
//! A chunk is named by its CONTENT, under a key only the client holds. Two properties fall
//! out of that, and the first one is a correctness property rather than an optimisation:
//!
//! * A changed chunk gets a new name, so the "already in the store, skip it" test that makes
//!   an upload resumable cannot skip bytes that actually changed. Naming a chunk by its
//!   position instead makes that skip silently wrong — the new bytes never upload and the
//!   read returns the previous version looking entirely healthy.
//! * Unchanged chunks keep their name, so a second version of a large archive costs only the
//!   chunks that differ. That is what makes keeping full history affordable.
//!
//! The key is what stops the store operator testing whether you hold a file they already
//! have: with a bare content hash they could hash a known file and look for it. The key is
//! derived per identity today, so deduplication reaches across an identity's own objects and
//! versions but not between two members of one environment.
//!
//! Chunks are grouped under a per-identity NAMESPACE, which is a one-way function of the
//! principal rather than the principal itself. Collection needs the grouping so it can walk
//! one identity's objects without reading everybody else's, and the blinding is what stops a
//! bucket listing naming the owner of each group.

// ponytail: the naming key is per identity — two members of the same environment store
// separate copies of an identical chunk. The upgrade is an environment-scoped key derived
// from the scope secret, which needs the convergent-chunk primitive in vault42-core;
// s33-large-objects holds the deduplication assertions that go green when it lands.

use serde::{Deserialize, Serialize};

/// The plaintext bytes per chunk. Comfortably under the 4 MiB transport ceiling so a chunk
/// can also travel through the vault if it ever needs to, and large enough that a gigabyte
/// is hundreds of requests rather than hundreds of thousands.
pub const CHUNK_BYTES: usize = 4 * 1024 * 1024 - 64 * 1024;

/// Domain separation for the naming key, so key material can never serve two purposes.
const NAME_KEY_CONTEXT: &str = "vault42 2026-06-19 chunk name key v1";

/// The key chunk names are hashed under. Never leaves the process and never reaches a store.
pub type NameKey = [u8; 32];

/// Domain separation for the namespace, kept apart from the naming key's own context.
const NAMESPACE_CONTEXT: &str = "vault42 2026-06-19 chunk namespace v1";

/// How one identity names its chunks: the prefix they share and the key they hash under.
pub struct Naming {
    namespace: String,
    key: NameKey,
}

impl Naming {
    /// Derive both from the caller's principal and its own key material.
    pub fn new(principal: &str, material: &[u8]) -> Self {
        let blinded = blake3::derive_key(NAMESPACE_CONTEXT, principal.as_bytes());
        Self {
            namespace: hex::encode(&blinded[..8]),
            key: blake3::derive_key(NAME_KEY_CONTEXT, material),
        }
    }

    /// The prefix every one of this identity's chunks lives under, collection included.
    pub fn prefix(&self) -> String {
        format!("chunks/{}/", self.namespace)
    }
}

/// One chunk's place in an object: where it belongs and how to find it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkRef {
    pub index: u32,
    pub name: String,
    pub plain_len: u64,
}

/// The ordered list that reconstitutes an object, sealed as ordinary envelope plaintext.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkSet {
    pub object_id: String,
    pub total_len: u64,
    pub chunk_bytes: u32,
    pub chunks: Vec<ChunkRef>,
}

impl ChunkSet {
    /// Serialise for sealing. JSON rather than bincode so an older client meeting a newer
    /// list ignores fields it does not know instead of failing to decode, which is the
    /// mistake the frozen envelope format cannot undo.
    pub fn to_bytes(&self) -> anyhow::Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    /// Parse a chunk list, rejecting one whose own bookkeeping disagrees with itself.
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let set: Self = serde_json::from_slice(bytes)?;
        set.validate()?;
        Ok(set)
    }

    /// A list must be complete, in order, and account for exactly the bytes it claims.
    ///
    /// This is what turns a dropped, reordered or truncated chunk into an error rather than
    /// silently short bytes. The store is not trusted to return what it was given, so the
    /// list is checked against itself before a single chunk is fetched.
    pub fn validate(&self) -> anyhow::Result<()> {
        let claimed: u64 = self.chunks.iter().map(|c| c.plain_len).sum();
        if claimed != self.total_len {
            anyhow::bail!(
                "chunk list accounts for {claimed} bytes but claims {}",
                self.total_len
            );
        }
        for (position, chunk) in self.chunks.iter().enumerate() {
            if chunk.index as usize != position {
                anyhow::bail!("chunk list is out of order at position {position}");
            }
            if chunk.plain_len == 0 {
                anyhow::bail!("chunk {position} is empty");
            }
        }
        Ok(())
    }
}

/// Split `plaintext` and name every chunk, without encrypting or uploading anything.
///
/// Splitting is separated from sealing on purpose, so the list can be compared against what
/// the store already holds before a single byte is encrypted. That comparison is what makes
/// an interrupted upload resumable and a second version cheap.
pub fn build(object_id: &str, naming: &Naming, plaintext: &[u8]) -> ChunkSet {
    let chunks = plaintext
        .chunks(CHUNK_BYTES)
        .enumerate()
        .map(|(index, part)| ChunkRef {
            index: index as u32,
            name: chunk_name(naming, part),
            plain_len: part.len() as u64,
        })
        .collect();
    ChunkSet {
        object_id: object_id.to_string(),
        total_len: plaintext.len() as u64,
        chunk_bytes: CHUNK_BYTES as u32,
        chunks,
    }
}

/// Where one chunk lives in the store: a keyed digest of the bytes it holds.
pub fn chunk_name(naming: &Naming, plaintext: &[u8]) -> String {
    format!(
        "{}{}",
        naming.prefix(),
        blake3::keyed_hash(&naming.key, plaintext).to_hex()
    )
}

/// Whether an object is large enough to be worth chunking at all.
pub fn needs_chunking(len: u64) -> bool {
    len > CHUNK_BYTES as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naming(seed: u8) -> Naming {
        Naming::new("principal-fingerprint", &[seed; 32])
    }

    fn payload(len: usize, modulus: usize) -> Vec<u8> {
        (0..len).map(|i| (i % modulus) as u8).collect()
    }

    #[test]
    fn a_small_object_is_not_chunked() {
        assert!(!needs_chunking(1024));
        assert!(!needs_chunking(CHUNK_BYTES as u64));
        assert!(needs_chunking(CHUNK_BYTES as u64 + 1));
    }

    #[test]
    fn a_list_accounts_for_every_byte() {
        let set = build("obj", &naming(1), &payload(CHUNK_BYTES * 2 + 99, 251));
        set.validate().expect("valid");
        assert_eq!(
            set.chunks.iter().map(|c| c.plain_len).sum::<u64>(),
            set.total_len
        );
    }

    #[test]
    fn the_last_chunk_is_the_remainder_not_a_full_one() {
        let set = build("obj", &naming(1), &payload(CHUNK_BYTES + 7, 251));
        assert_eq!(set.chunks.len(), 2);
        assert_eq!(set.chunks[1].plain_len, 7);
    }

    /// The correctness property, not the optimisation: changed bytes must get a new name,
    /// or the resume check skips them and the read serves the previous version.
    #[test]
    fn changed_content_gets_a_different_name() {
        let k = naming(1);
        assert_ne!(chunk_name(&k, b"alpha"), chunk_name(&k, b"beta"));
    }

    /// The optimisation: unchanged bytes keep their name, so a second version costs only
    /// what actually differs.
    #[test]
    fn unchanged_content_keeps_its_name() {
        let k = naming(1);
        let first = build("obj", &k, &payload(CHUNK_BYTES * 2, 251));
        let mut edited = payload(CHUNK_BYTES * 2, 251);
        edited[CHUNK_BYTES + 10] ^= 0xff;
        let second = build("obj", &k, &edited);
        assert_eq!(
            first.chunks[0].name, second.chunks[0].name,
            "an untouched chunk must be recognised as already stored"
        );
        assert_ne!(
            first.chunks[1].name, second.chunks[1].name,
            "the edited chunk must not reuse the stored name"
        );
    }

    /// Confirmation resistance: the store operator holding a known file must not be able to
    /// test whether it is one of ours. A bare content hash would let them.
    #[test]
    fn a_different_key_names_the_same_content_differently() {
        assert_ne!(
            chunk_name(&naming(1), b"payroll.csv"),
            chunk_name(&naming(2), b"payroll.csv")
        );
    }

    /// The namespace groups an identity's chunks without naming it, so collection can walk
    /// one prefix while a bucket listing still does not say whose chunks those are.
    #[test]
    fn the_namespace_blinds_the_principal() {
        let naming = Naming::new("deadbeefcafe", &[1u8; 32]);
        let prefix = naming.prefix();
        assert!(prefix.starts_with("chunks/"), "{prefix}");
        assert!(
            !prefix.contains("deadbeefcafe"),
            "a listing must not name the owner"
        );
        assert_eq!(
            prefix,
            Naming::new("deadbeefcafe", &[9u8; 32]).prefix(),
            "the prefix follows the principal, not the naming key"
        );
        assert_ne!(
            prefix,
            Naming::new("cafedeadbeef", &[1u8; 32]).prefix(),
            "two identities must not share a prefix, or collection would cross into another's chunks"
        );
    }

    #[test]
    fn a_chunk_lives_under_its_identity_prefix() {
        let naming = naming(1);
        assert!(chunk_name(&naming, b"bytes").starts_with(&naming.prefix()));
    }

    #[test]
    fn a_name_carries_no_plaintext() {
        let name = chunk_name(&naming(1), b"DATABASE_PASSWORD=hunter2");
        assert!(!name.contains("hunter2"), "a name must not echo the bytes");
        assert!(name.starts_with("chunks/"));
    }

    #[test]
    fn a_list_that_lost_a_chunk_is_refused() {
        let mut set = build("obj", &naming(1), &payload(CHUNK_BYTES * 3, 251));
        set.chunks.pop();
        assert!(set.validate().is_err(), "a dropped chunk must not validate");
    }

    #[test]
    fn a_reordered_list_is_refused() {
        let mut set = build("obj", &naming(1), &payload(CHUNK_BYTES * 3, 251));
        set.chunks.swap(0, 2);
        assert!(
            set.validate().is_err(),
            "a reordered chunk must not validate"
        );
    }

    #[test]
    fn a_truncated_total_is_refused() {
        let mut set = build("obj", &naming(1), &payload(CHUNK_BYTES * 2, 251));
        set.total_len -= 1;
        assert!(
            set.validate().is_err(),
            "a mismatched total must not validate"
        );
    }

    #[test]
    fn a_chunk_list_round_trips_through_its_own_codec() {
        let set = build("obj", &naming(1), &payload(CHUNK_BYTES + 5, 251));
        let back = ChunkSet::from_bytes(&set.to_bytes().expect("encode")).expect("decode");
        assert_eq!(set, back);
    }
}
