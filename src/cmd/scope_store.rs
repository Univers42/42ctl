/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope_store.rs                                       :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Reading and writing one stored item of an environment — the primitives the shared tree, its
//! private half and a key rotation have in common.
//!
//! Every write is conditional on a revision read BEFORE the work began, not just before the
//! write. A head fetched per file guards only the instant between that fetch and the put, so
//! two overlapping pushes both pass it and interleave: each file ends up from whichever push
//! wrote it last, and the manifest from whichever committed last. A push instead reads every
//! head once, up front ([`Heads`]), and each put — the manifest's last — must still find the
//! environment as that snapshot saw it. A push that somebody else's overtook is refused.
//!
//! Every read names the revision it wants. A manifest records the revision each file was
//! written at, and a push that dies or is refused partway leaves NEWER revisions behind that no
//! manifest names. Reading the newest restores those: a tree nobody ever pushed.

use crate::adapters::api::Session;
use crate::adapters::compose::{self, ScopeSeal};
use crate::adapters::scope_env_grpc::EnvSecretPut;
use crate::adapters::{decrypt, derive};
use std::collections::BTreeMap;
use vault42_core::{ReadScope, RecipientPublicKey};
use zeroize::Zeroizing;

/// Which key opens an environment secret: the environment's recovered secret for a shared
/// file, or the caller's own identity for one sealed to them alone.
pub(super) enum Key<'a> {
    Scope(&'a Zeroizing<[u8; 32]>),
    Me,
}

/// Where writes go: the environment's owner (its scope id, hex), the epoch, and the key the
/// plaintext is sealed to — the environment's, or the pusher's own for a private file.
#[derive(Clone, Copy)]
pub(super) struct Dest<'a> {
    pub owner: &'a str,
    pub epoch: u32,
    pub to: RecipientPublicKey,
}

/// One stored item to read: its owner, the epoch it lives in, its opaque path, and the
/// revision wanted (`0` ⇒ the newest).
#[derive(Clone, Copy)]
pub(super) struct Slot<'a> {
    pub owner: &'a str,
    pub epoch: u32,
    pub path: &'a str,
    pub rev: u64,
}

/// Every path's head revision within one epoch, read in one listing before anything is written.
pub(super) struct Heads(BTreeMap<String, u64>);

impl Heads {
    /// Read the head of every path `(owner, epoch)` holds.
    pub async fn read(session: &mut Session, owner: &str, epoch: u32) -> anyhow::Result<Self> {
        let listed = session.list_env_secrets(owner, epoch).await?;
        Ok(Self(
            listed.into_iter().map(|e| (e.path, e.version)).collect(),
        ))
    }

    /// The head of `path` when the listing was read, or 0 when it held nothing there.
    pub fn of(&self, path: &str) -> u64 {
        self.0.get(path).copied().unwrap_or(0)
    }

    /// Every path the listing held, with its head, in path order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, u64)> {
        self.0.iter().map(|(path, head)| (path.as_str(), *head))
    }
}

/// Seal `plaintext` to `dest.to` and store it at `path` as the revision after `after`. The
/// server refuses it unless `after` is still the head. Returns the revision written.
pub(super) async fn put_one(
    session: &mut Session,
    dest: Dest<'_>,
    at: (&str, u64),
    plaintext: &[u8],
) -> anyhow::Result<u64> {
    let (path, after) = at;
    let envelope = compose::scope_envelope(
        &session.identity,
        &ScopeSeal {
            owner: dest.owner,
            vault_path: path,
            project_id: dest.owner,
            scope_pub: dest.to,
            rev: after + 1,
            plaintext,
        },
    )?;
    session
        .put_env_secret(EnvSecretPut {
            scope_id: dest.owner,
            epoch: dest.epoch,
            path,
            envelope,
            expected_prev_rev: after,
        })
        .await
}

/// Fetch and decrypt one revision, or `None` when the environment holds no such revision.
///
/// The envelope must carry at least the revision asked for, so a server answering with an
/// older one under that number is refused rather than restored.
pub(super) async fn fetch_one(
    session: &mut Session,
    slot: Slot<'_>,
    key: &Key<'_>,
) -> anyhow::Result<Option<Zeroizing<Vec<u8>>>> {
    let stored = session
        .get_env_secret(slot.owner, slot.epoch, slot.path, slot.rev)
        .await?;
    let Some((envelope, author)) = stored else {
        return Ok(None);
    };
    let expected = derive::secret_id(slot.owner, slot.path);
    let scope = ReadScope {
        secret_id: &expected,
        min_rev: slot.rev,
    };
    let plain = match key {
        Key::Scope(secret) => decrypt::open_env_envelope(secret, &envelope, &author, scope)?,
        Key::Me => decrypt::open_private_envelope(&session.identity, &envelope, &author, scope)?,
    };
    Ok(Some(plain))
}

/// Say what a refused write during a push means: another push landed while this one ran.
///
/// The server's words are "version conflict", which reads as a fault to report. It is the
/// guard working, and the person needs to know that nothing of theirs was published and what
/// to do about it. Any other error passes through unchanged.
pub(super) fn overtaken(error: anyhow::Error, env: &str) -> anyhow::Error {
    if !is_conflict(&error) {
        return error;
    }
    anyhow::anyhow!(
        "environment '{env}' was pushed by somebody else while this push ran, so nothing of \
         this push was published — `42ctl env pull` shows what they published, and pushing \
         again replaces it with this tree"
    )
}

/// Whether the server refused a write because the revision it expected was no longer the head.
fn is_conflict(error: &anyhow::Error) -> bool {
    error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<tonic::Status>())
        .any(|status| {
            status.code() == tonic::Code::FailedPrecondition
                && status.message().contains("conflict")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A refused write, however deep in the cause chain, becomes the explanation — and says
    /// which environment and what to run.
    #[test]
    fn an_overtaken_push_says_what_happened_and_what_to_do() {
        let refused = anyhow::Error::new(tonic::Status::failed_precondition("version conflict"))
            .context("could not push srcs/.env");
        let said = overtaken(refused, "prod").to_string();
        assert!(said.contains("environment 'prod'"), "{said}");
        assert!(said.contains("while this push ran"), "{said}");
        assert!(said.contains("env pull"), "{said}");
    }

    /// Every other failure keeps its own words: a store that cannot answer safely is not a
    /// race, and neither is a refusal to write.
    #[test]
    fn any_other_failure_passes_through() {
        for status in [
            tonic::Status::failed_precondition("this storage backend cannot answer that safely"),
            tonic::Status::permission_denied("env secret not authored by caller"),
            tonic::Status::unavailable("connection refused"),
        ] {
            let message = status.message().to_string();
            let kept = overtaken(anyhow::Error::new(status), "prod").to_string();
            assert!(kept.contains(&message), "{kept}");
            assert!(!kept.contains("while this push ran"), "{kept}");
        }
    }

    /// A path the listing never saw is a create, conditional on nothing being there.
    #[test]
    fn a_path_absent_from_the_listing_has_head_zero() {
        let heads = Heads(BTreeMap::from([("__42ctl/tree".to_string(), 7)]));
        assert_eq!(heads.of("__42ctl/tree"), 7);
        assert_eq!(heads.of("__42ctl/f/new"), 0);
    }
}
