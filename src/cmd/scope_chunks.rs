/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope_chunks.rs                                      :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The chunked half of `push-env` / `pull-env`: a file above the transport ceiling is split
//! into chunks in the object store, each sealed to the ENVIRONMENT so every member can open
//! it, with only the chunk list in the vault. Names come from the environment's secret, so
//! two members holding the same bytes compute the same name and the second stores nothing.

use crate::adapters::api::Session;
use crate::adapters::blobstore::BlobStore;
use crate::adapters::compose::{self, ScopeChunkSeal};
use crate::adapters::{decrypt, derive};
use crate::cmd::scope::Ctx;
use crate::cmd::scope_recover::recover_scope_secret;
use crate::cmd::scope_tree::{put_one, scope_public, tree_path};
use crate::core::chunk::{self, ChunkSet, Naming};
use crate::core::manifest::Entry;
use crate::ops::largeobj;
use crate::ui;
use vault42_core::ReadScope;
use zeroize::Zeroizing;

/// Split a large file into chunks in the object store and keep only the list in the vault.
///
/// The chunks are sealed to the ENVIRONMENT rather than to the pusher, so every member who
/// holds a wrap can open them. Sealing them to one identity would put the team's archive
/// somewhere only its author can read, which is the failure the shared tree exists to avoid.
pub(super) async fn seal_large(
    session: &mut Session,
    ctx: &Ctx,
    at: (&str, &str, &Naming),
    plaintext: &[u8],
) -> anyhow::Result<Entry> {
    let (owner, rel, naming) = at;
    let vault_path = tree_path(owner, rel);
    let store = store_for(session, rel, plaintext.len())?;
    store.ensure_bucket().await?;
    let scope_pub = scope_public(ctx)?;
    let identity = &session.identity;
    let author = identity.author_public().to_bytes();
    let set = largeobj::put_chunks(store, (&vault_path, naming), plaintext, |part, bytes| {
        let envelope = compose::scope_chunk_envelope(
            identity,
            &ScopeChunkSeal {
                scope_owner: owner,
                name: &part.name,
                scope_pub,
                plaintext: bytes,
            },
        )?;
        Ok(compose::chunkframe::wrap(&author, &envelope))
    })
    .await?;
    let at = (owner, vault_path.as_str(), scope_pub);
    let rev = put_one(session, ctx, at, &set.to_bytes()?).await?;
    ui::field(
        rel,
        &format!("{} chunk(s) in the object store", set.chunks.len()),
    );
    Ok(Entry {
        chunked: true,
        ..Entry::file(rel, vault_path, rev, plaintext.len() as u64)
    })
}

/// Fetch and reassemble a chunked entry, opening each chunk with the environment's key.
pub(super) async fn open_large(
    session: &mut Session,
    ctx: &Ctx,
    key: (&str, &Zeroizing<[u8; 32]>),
    set: &ChunkSet,
) -> anyhow::Result<Zeroizing<Vec<u8>>> {
    let (owner, secret) = key;
    let naming = Naming::new(owner, secret.as_slice());
    let store = store_for(session, &ctx.env_name, set.total_len as usize)?;
    largeobj::get_chunks(store, set, |name, stored| {
        let (author, envelope) = compose::chunkframe::unwrap(stored)?;
        let expected = derive::secret_id(owner, name);
        let scope = ReadScope {
            secret_id: &expected,
            min_rev: 0,
        };
        let plain = decrypt::open_env_envelope(secret, envelope, &author, scope)?;
        verify_name(&naming, name, &plain)?;
        Ok(plain)
    })
    .await
}

/// Require a chunk's name to be the keyed hash of the bytes it turned out to hold.
///
/// Deduplication is what makes this necessary. A writer whose chunk name already exists skips
/// the upload and points at what is there, so a member who seals unrelated bytes under a name
/// they computed dishonestly poisons every later writer of that content: the honest writer
/// stores nothing, and their restore returns the poisoner's bytes. Nothing else notices —
/// the envelope is validly sealed, validly signed, and bound to the right secret id.
///
/// Checked on READ rather than on write, because the write side is where the dishonest party
/// stands. Recomputing costs one keyed hash per chunk against bytes already in memory.
fn verify_name(naming: &Naming, name: &str, plaintext: &[u8]) -> anyhow::Result<()> {
    if chunk::chunk_name(naming, plaintext) == name {
        return Ok(());
    }
    anyhow::bail!("chunk {name} does not hold the bytes its name says it does")
}

/// The configured object store, or a refusal naming what to configure.
fn store_for<'a>(session: &'a Session, what: &str, len: usize) -> anyhow::Result<&'a BlobStore> {
    session.store.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "{what} is {len} bytes, above the transport ceiling, and this profile names no \
             object store — set one with `42ctl config endpoint --blobstore <url> --bucket \
             <name>` and export FT_S3_KEY and FT_S3_SECRET"
        )
    })
}

/// How this environment names its chunks: a prefix and key derived from the scope secret.
///
/// Derived from the SCOPE rather than from a person, so two members holding the same bytes
/// compute the same name and the second one stores nothing. That is the deduplication a team
/// actually wants, and it needs no convergent ciphertext: identical names plus the
/// already-present check mean the first copy is the only copy, and every member can open it
/// because it is sealed to the environment.
pub(super) async fn environment_naming(
    session: &mut Session,
    ctx: &Ctx,
    scope_id: [u8; 16],
) -> anyhow::Result<Naming> {
    let epoch = ctx.scope_epoch.max(1);
    let secret =
        recover_scope_secret(session, scope_id, epoch, ctx.scope_pubkey.as_deref()).await?;
    Ok(Naming::new(&hex::encode(scope_id), secret.as_slice()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chunk whose plaintext does not hash to the name it was fetched under is refused.
    ///
    /// This is the poisoning case, and it is the one a substituted OBJECT cannot show: copying
    /// a whole stored chunk over another's name is caught earlier, by the secret id bound to
    /// the name it was sealed for. What this catches is a dishonest client that seals
    /// correctly FOR a name while putting unrelated bytes inside — which every later writer of
    /// those real bytes then points at, storing nothing and restoring the poisoner's content.
    #[test]
    fn a_chunk_holding_other_bytes_than_its_name_says_is_refused() {
        let naming = Naming::new("scope-id", &[3u8; 32]);
        let honest = b"the real chunk bytes";
        let name = chunk::chunk_name(&naming, honest);
        verify_name(&naming, &name, honest).expect("the honest chunk must open");
        assert!(
            verify_name(&naming, &name, b"unrelated bytes").is_err(),
            "a chunk must not hold bytes other than the ones its name names"
        );
    }

    /// And the check is keyed: a different environment's key must not validate this one's
    /// chunk, or the poisoning defence would be portable between environments.
    #[test]
    fn the_name_check_is_bound_to_the_environment_key() {
        let bytes = b"shared content";
        let mine = Naming::new("scope-id", &[3u8; 32]);
        let theirs = Naming::new("scope-id", &[9u8; 32]);
        let name = chunk::chunk_name(&mine, bytes);
        assert!(verify_name(&theirs, &name, bytes).is_err());
    }
}
