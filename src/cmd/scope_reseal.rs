/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope_reseal.rs                                      :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The re-seal half of `env keys rotate`: what an environment holds, moved from the old key's
//! epoch to the new key's.
//!
//! A tree holds three kinds of stored item besides single secrets, and treating every one as a
//! single secret is what made a rotation fail on any environment a tree was pushed to:
//!
//! - a shared file is opened with the old key and sealed to the new one — at the REVISION the
//!   tree's manifest names, not the newest, since a push that died or was overtaken leaves
//!   newer revisions nobody published;
//! - a chunked file's chunks are named and sealed with the environment's key, so moving only
//!   its chunk list leaves chunks the new key can neither find nor open. The file is opened
//!   whole and chunked again under the new key;
//! - a member's private file is sealed to that member, and the server takes a write only from
//!   the envelope's author, so the administrator can neither open nor re-author it. It stays
//!   in the epoch it was written in, where its owner's pull still finds it.
//!
//! The manifest goes last, rewritten to name where each file now is and at which revision, so
//! an interrupted rotation leaves the new epoch without a tree rather than with one naming
//! files that are not there. Nothing is live until `scope_rotate` publishes the new epoch.
//!
//! A push can land at the old epoch while this runs, and would be in no epoch anybody reads once
//! the new one is published. So after publishing, [`carry_late`] lists the old epoch again and
//! moves everything once more if it changed; a push landing after THAT listing lands after the
//! publish too, and the push itself sees the epoch move and says so (`scope_tree`).

use crate::adapters::api::Session;
use crate::cmd::scope::Ctx;
use crate::cmd::scope_chunks;
use crate::cmd::scope_private::PRIVATE_PREFIX;
use crate::cmd::scope_secret_reseal::RotateState;
use crate::cmd::scope_store::{self, Dest, Heads, Key, Slot};
use crate::cmd::scope_tree::TREE_MANIFEST;
use crate::core::chunk::{ChunkSet, Naming};
use crate::core::manifest::{Entry, Manifest};
use std::collections::BTreeMap;

/// A rotation's writes: where they go, and the new epoch's heads before the first of them.
struct Reseal<'a> {
    state: &'a RotateState<'a>,
    dest: Dest<'a>,
    heads: Heads,
}

/// Where each moved item now is, and at which revision, keyed by where it was.
type Moved = BTreeMap<String, (String, u64)>;

/// Move every shared item of the old epoch to the new one, the tree's manifest last. Returns
/// how many items were re-sealed — members' private files are left where they are — and the
/// old epoch's heads as they were listed before the first move.
pub async fn reseal_all(
    session: &mut Session,
    ctx: &Ctx,
    state: &RotateState<'_>,
) -> anyhow::Result<(usize, Heads)> {
    let owner = hex::encode(state.scope_id);
    let run = Reseal {
        state,
        dest: Dest {
            owner: &owner,
            epoch: state.new_epoch,
            to: state.keyset.public,
        },
        heads: Heads::read(session, &owner, state.new_epoch).await?,
    };
    let before = Heads::read(session, &owner, state.old_epoch).await?;
    let tree = old_tree(session, &run).await?;
    let mut moved = Moved::new();
    for (path, _) in before.iter().filter(|(path, _)| carried(path)) {
        let now = move_item(session, ctx, &run, (path, tree.as_ref())).await?;
        moved.insert(path.to_string(), now);
    }
    let count = moved.len() + usize::from(tree.is_some());
    if let Some(manifest) = tree {
        move_tree(session, &run, manifest, &moved).await?;
    }
    Ok((count, before))
}

/// Once the new epoch is published, move everything again if the old epoch's shared items
/// changed since `before` was listed. Returns how many items that second pass moved, 0 when
/// nothing arrived. Private files are left out of the comparison: they stay where they land.
pub async fn carry_late(
    session: &mut Session,
    ctx: &Ctx,
    state: &RotateState<'_>,
    before: &Heads,
) -> anyhow::Result<usize> {
    let owner = hex::encode(state.scope_id);
    let after = Heads::read(session, &owner, state.old_epoch).await?;
    if shared(&after).eq(shared(before)) {
        return Ok(0);
    }
    Ok(reseal_all(session, ctx, state).await?.0)
}

/// The heads of every item that is not a member's private file.
fn shared(heads: &Heads) -> impl Iterator<Item = (&str, u64)> {
    heads
        .iter()
        .filter(|(path, _)| !path.starts_with(PRIVATE_PREFIX))
}

/// Whether an item is re-sealed item by item: everything but the tree's manifest, which is
/// rewritten last, and members' private files, which the administrator cannot re-author.
fn carried(path: &str) -> bool {
    path != TREE_MANIFEST && !path.starts_with(PRIVATE_PREFIX)
}

/// The tree's manifest at the old epoch, when the environment holds a tree.
async fn old_tree(session: &mut Session, run: &Reseal<'_>) -> anyhow::Result<Option<Manifest>> {
    let slot = Slot {
        owner: run.dest.owner,
        epoch: run.state.old_epoch,
        path: TREE_MANIFEST,
        rev: 0,
    };
    match scope_store::fetch_one(session, slot, &Key::Scope(run.state.old_secret)).await? {
        Some(raw) => Ok(Some(Manifest::parse(&raw)?)),
        None => Ok(None),
    }
}

/// Move one item — at the revision the manifest names, if it names it — and return where it
/// now is and at which revision.
async fn move_item(
    session: &mut Session,
    ctx: &Ctx,
    run: &Reseal<'_>,
    at: (&str, Option<&Manifest>),
) -> anyhow::Result<(String, u64)> {
    let (path, tree) = at;
    let named = tree.and_then(|m| m.entries.iter().find(|e| e.vault_path == path));
    let slot = Slot {
        owner: run.dest.owner,
        epoch: run.state.old_epoch,
        path,
        rev: named.map_or(0, |entry| entry.rev),
    };
    let plain = scope_store::fetch_one(session, slot, &Key::Scope(run.state.old_secret))
        .await?
        .ok_or_else(|| anyhow::anyhow!("env secret '{path}' vanished mid-rotation"))?;
    if let Some(entry) = named.filter(|entry| entry.chunked) {
        return rechunk(session, ctx, run, (entry, &plain)).await;
    }
    let rev = scope_store::put_one(session, run.dest, (path, run.heads.of(path)), &plain).await?;
    Ok((path.to_string(), rev))
}

/// Open a chunked file whole with the old key and chunk it again under the new one.
async fn rechunk(
    session: &mut Session,
    ctx: &Ctx,
    run: &Reseal<'_>,
    at: (&Entry, &[u8]),
) -> anyhow::Result<(String, u64)> {
    let (entry, list) = at;
    let set = ChunkSet::from_bytes(list)?;
    let old = (run.dest.owner, run.state.old_secret);
    let plain = scope_chunks::open_large(session, ctx, old, &set).await?;
    let naming = Naming::new(run.dest.owner, run.state.new_secret.as_slice());
    let named = (entry.relative_path.as_str(), &naming, &run.heads);
    let moved = scope_chunks::seal_large(session, run.dest, named, &plain).await?;
    Ok((moved.vault_path, moved.rev))
}

/// Write the tree's manifest to the new epoch, naming where each file now is and at which
/// revision.
async fn move_tree(
    session: &mut Session,
    run: &Reseal<'_>,
    mut manifest: Manifest,
    moved: &Moved,
) -> anyhow::Result<()> {
    for entry in &mut manifest.entries {
        if let Some((path, rev)) = moved.get(&entry.vault_path) {
            entry.vault_path.clone_from(path);
            entry.rev = *rev;
        }
    }
    let at = (TREE_MANIFEST, run.heads.of(TREE_MANIFEST));
    scope_store::put_one(session, run.dest, at, &manifest.to_bytes()?).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The manifest is rewritten rather than copied, and a member's private file cannot be
    /// re-authored by whoever rotates, so neither is moved item by item. Everything else is.
    #[test]
    fn only_shared_items_are_moved_one_by_one() {
        assert!(!carried(TREE_MANIFEST));
        assert!(!carried("__42ctl/p/ab12/tree"));
        assert!(!carried("__42ctl/p/ab12/f/0123"));
        assert!(carried("__42ctl/f/0123"));
        assert!(carried("app/db"));
    }
}
