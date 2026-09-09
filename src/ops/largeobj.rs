/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   largeobj.rs                                          :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Pushing and fetching an object too large for one sealed envelope.
//!
//! The chunks go to the object store and only the chunk list goes to the vault, so a
//! gigabyte never touches the fly volume and never crosses a gRPC message limit. The list
//! is sealed as an ordinary blob, which means the author signature already covers the
//! order, the count and every chunk's length without any new signed object type.
//!
//! Nothing here trusts the store to return what it was given. The list is validated against
//! itself before a single chunk is fetched, every chunk is opened under a read scope bound
//! to its own name, and the reassembled length is checked against what the list claimed —
//! so a dropped, reordered or substituted chunk is an error rather than short bytes.

use crate::adapters::blobstore::BlobStore;
use crate::adapters::compose::{self, ChunkSeal};
use crate::adapters::derive;
use crate::core::chunk::{self, ChunkSet};
use vault42_core::{open, Envelope, Identity, ReadScope};
use zeroize::Zeroizing;

/// Who is sealing an object.
///
/// Grouped rather than passed one at a time because a caller that mismatches the principal
/// and the identity produces chunks it cannot open again.
pub struct Owner<'a> {
    pub identity: &'a Identity,
    pub principal: &'a str,
}

/// Seal and upload every chunk that the store does not already hold, and return the list.
///
/// The existence check is what makes an interrupted upload resumable: a chunk already
/// present is one that does not need sending again, which is the difference between
/// retrying a gigabyte and resuming one. It is safe only because a chunk is named by its
/// content, so a chunk whose bytes changed is a chunk the store does not hold.
pub async fn put_object(
    owner: &Owner<'_>,
    store: &BlobStore,
    object_id: &str,
    plaintext: &[u8],
) -> anyhow::Result<ChunkSet> {
    let set = chunk::build(object_id, &naming(owner), plaintext);
    let mut offset = 0usize;
    for part in &set.chunks {
        let end = offset + part.plain_len as usize;
        if !store.head(&part.name).await? {
            let sealed = seal_chunk(owner, part, &plaintext[offset..end])?;
            store.put(&part.name, sealed).await?;
        }
        offset = end;
    }
    Ok(set)
}

/// Fetch every chunk and reassemble, refusing anything the list did not describe.
pub async fn get_object(
    identity: &Identity,
    principal: &str,
    store: &BlobStore,
    set: &ChunkSet,
) -> anyhow::Result<Zeroizing<Vec<u8>>> {
    set.validate()?;
    let mut out = Zeroizing::new(Vec::with_capacity(set.total_len as usize));
    for part in &set.chunks {
        let sealed = store.get(&part.name).await?;
        let plain = open_chunk(identity, principal, &part.name, &sealed)?;
        if plain.len() as u64 != part.plain_len {
            anyhow::bail!(
                "chunk {} is {} bytes but the list claims {}",
                part.index,
                plain.len(),
                part.plain_len
            );
        }
        out.extend_from_slice(&plain);
    }
    if out.len() as u64 != set.total_len {
        anyhow::bail!(
            "reassembled {} bytes but the list claims {}",
            out.len(),
            set.total_len
        );
    }
    Ok(out)
}

/// How this caller names its chunks: a blinded prefix plus a key only it holds.
///
/// Per identity rather than per environment, so an identity deduplicates against its own
/// history but two members of one environment still store separate copies.
pub fn naming(owner: &Owner<'_>) -> chunk::Naming {
    chunk::Naming::new(
        owner.principal,
        &owner.identity.encryption_secret().to_bytes(),
    )
}

/// Seal one chunk, binding it to its own name so it cannot be served in another's place.
fn seal_chunk(
    owner: &Owner<'_>,
    part: &chunk::ChunkRef,
    plaintext: &[u8],
) -> anyhow::Result<Vec<u8>> {
    compose::chunk_envelope(
        owner.identity,
        &ChunkSeal {
            owner: owner.principal,
            name: &part.name,
            plaintext,
        },
    )
}

/// Open one chunk under a read scope bound to the name it was fetched from.
///
/// The scope pins the derived secret id, so a store that returns chunk 7's bytes when
/// chunk 3 was asked for fails the read rather than producing plausible wrong data.
fn open_chunk(
    identity: &Identity,
    principal: &str,
    name: &str,
    sealed: &[u8],
) -> anyhow::Result<Zeroizing<Vec<u8>>> {
    let env = Envelope::from_bytes(sealed)?;
    let author = identity.signing_key().verifying_key();
    let expected = derive::secret_id(principal, name);
    let scope = ReadScope {
        secret_id: &expected,
        min_rev: 0,
    };
    Ok(open(&env, identity.encryption_secret(), &author, &scope)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compare multi-megabyte payloads by digest: a byte-slice assertion prints both
    /// operands, which turns one failure into tens of megabytes of unreadable output.
    fn digest(bytes: &[u8]) -> String {
        blake3::hash(bytes).to_hex().to_string()
    }

    /// A chunk lives in a store run by somebody else, so its metadata must say less than a
    /// vault blob's. The project id would let that operator group chunks into objects and
    /// count how many each project has; the owner would tell them whose they are. Both are
    /// exactly the inferences a blinded, content-addressed namespace exists to deny.
    #[test]
    fn a_sealed_chunk_names_neither_its_project_nor_its_owner() {
        let identity = Identity::generate();
        let owner = Owner {
            identity: &identity,
            principal: "s33-owner-fingerprint",
        };
        let part = chunk::ChunkRef {
            index: 0,
            name: "chunks/ns/deadbeef".into(),
            plain_len: 5,
        };
        let sealed = seal_chunk(&owner, &part, b"hello").expect("seal");
        let haystack = String::from_utf8_lossy(&sealed).to_string();
        assert!(
            haystack.contains("chunk"),
            "the content type is expected in the clear, so its absence would mean this searched the wrong bytes"
        );
        assert!(
            !haystack.contains("s33-secret-project"),
            "no project id may reach the object store"
        );
        assert!(
            !haystack.contains("s33-owner-fingerprint"),
            "no owner may reach the object store"
        );
    }

    /// The whole path against a real object store: chunk, seal, upload, fetch, reassemble.
    /// Ignored unless an endpoint is supplied. A payload larger than one chunk is the point
    /// — a single-chunk object would exercise none of the ordering or reassembly.
    #[tokio::test]
    #[ignore]
    async fn a_multi_chunk_object_round_trips_and_resumes() {
        let store = BlobStore::from_profile(&crate::profile::BlobLocation::default())
            .expect("FT_S3_* unset");
        store.ensure_bucket().await.expect("bucket");
        let identity = Identity::generate();
        let principal = "test-principal";
        let object = format!("obj-{}", std::process::id());
        let payload: Vec<u8> = (0..(chunk::CHUNK_BYTES * 2 + 1234))
            .map(|i| (i % 251) as u8)
            .collect();

        let owner = Owner {
            identity: &identity,
            principal,
        };
        let set = put_object(&owner, &store, &object, &payload)
            .await
            .expect("put");
        assert_eq!(set.chunks.len(), 3, "two full chunks and a remainder");

        let back = get_object(&identity, principal, &store, &set)
            .await
            .expect("get");
        assert_eq!(
            digest(&back),
            digest(&payload),
            "bytes must survive exactly"
        );

        // Resume: a second push must upload nothing, because every chunk is already there.
        let again = put_object(&owner, &store, &object, &payload)
            .await
            .expect("second put");
        assert_eq!(again, set, "a resumed push must produce the identical list");

        // A store that loses a chunk must be caught rather than yielding short bytes.
        store.delete(&set.chunks[1].name).await.expect("delete");
        assert!(
            get_object(&identity, principal, &store, &set)
                .await
                .is_err(),
            "a missing chunk must fail the read"
        );
    }

    /// A second push of DIFFERENT content must not serve the previous version's bytes.
    ///
    /// Resume skips any chunk the store already holds. A chunk name that does not depend on
    /// the content therefore turns the skip into silent corruption: the changed bytes never
    /// upload, the list still validates, and the read returns the old version looking
    /// entirely healthy. Same length on purpose, so nothing but the content differs.
    #[tokio::test]
    #[ignore]
    async fn changed_content_is_not_masked_by_a_resumed_upload() {
        let store = BlobStore::from_profile(&crate::profile::BlobLocation::default())
            .expect("FT_S3_* unset");
        store.ensure_bucket().await.expect("bucket");
        let identity = Identity::generate();
        let principal = "test-principal";
        let object = format!("mutate-{}", std::process::id());
        let len = chunk::CHUNK_BYTES + 4096;
        let first: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
        let second: Vec<u8> = (0..len).map(|i| (i % 241) as u8).collect();

        let owner = Owner {
            identity: &identity,
            principal,
        };
        put_object(&owner, &store, &object, &first)
            .await
            .expect("first put");
        let set = put_object(&owner, &store, &object, &second)
            .await
            .expect("second put");

        let back = get_object(&identity, principal, &store, &set)
            .await
            .expect("get");
        assert_eq!(
            digest(&back),
            digest(&second),
            "the read must return the version that was last pushed, not the one before it"
        );
    }
}
