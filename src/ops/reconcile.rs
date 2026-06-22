/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   reconcile.rs                                         :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Applying a pull [`Action`] to the working tree: write the resolved bytes (or a binary
//! sidecar), report, and tell the caller whether the file ended in conflict. The decision
//! is pure ([`crate::core::merge::decide`]); this layer is only the I/O + reporting +
//! base bookkeeping.

use crate::core::materialize::{self, Opts};
use crate::core::merge::Action;
use crate::core::projpath::{self, RelPath};
use crate::core::syncstate::{self, SyncState};
use crate::ui;
use std::path::Path;

/// Write the resolution for `rel` (dry-run only prints intent), returning whether it is a
/// conflict. Reuses [`materialize::write_one`] for the guarded atomic write.
pub(crate) fn write_action(
    root: &Path,
    rel: &RelPath,
    action: &Action,
    mode: u32,
    opts: &Opts,
) -> anyhow::Result<bool> {
    let label = rel.as_str();
    if !opts.apply {
        ui::field(label, plan_label(action));
        return Ok(is_conflict(action));
    }
    match action {
        Action::Create(bytes) | Action::FastForward(bytes) => {
            materialize::write_one(root, rel, bytes, mode, opts.backup)?;
            ui::success(label);
            Ok(false)
        }
        Action::InSync => Ok(false),
        Action::KeepLocal => {
            println!(
                "{}",
                ui::warn(&format!("keep {label} (local ahead — push to sync)"))
            );
            Ok(false)
        }
        Action::Conflict(bytes) => {
            materialize::write_one(root, rel, bytes, mode, opts.backup)?;
            println!(
                "{}",
                ui::warn(&format!("CONFLICT {label} — resolve the <<<<<<< markers"))
            );
            Ok(true)
        }
        Action::Binary(bytes) => write_binary_sidecar(root, rel, bytes, mode),
    }
}

/// Update the recorded base after applying — only when the local file now equals remote
/// (create / fast-forward / already-in-sync). Conflicts and local-ahead keep the old base.
pub(crate) fn update_base(
    state: &mut SyncState,
    rel: &str,
    action: &Action,
    remote: &[u8],
    rev: u64,
) {
    if matches!(
        action,
        Action::Create(_) | Action::FastForward(_) | Action::InSync
    ) {
        state.set(rel, rev, syncstate::hash(remote));
    }
}

/// Write the diverged remote of a non-text file to a `<rel>.remote` sidecar, keeping the
/// local file untouched. Returns true (conflict).
fn write_binary_sidecar(
    root: &Path,
    rel: &RelPath,
    bytes: &[u8],
    mode: u32,
) -> anyhow::Result<bool> {
    let side = projpath::validate_stored(&format!("{}.remote", rel.as_str()))?;
    materialize::write_one(root, &side, bytes, mode, false)?;
    println!(
        "{}",
        ui::warn(&format!(
            "CONFLICT {} (binary) — remote written to {}",
            rel.as_str(),
            side.as_str()
        ))
    );
    Ok(true)
}

/// The dry-run intent label for `action`.
fn plan_label(action: &Action) -> &'static str {
    match action {
        Action::Create(_) => "would create",
        Action::InSync => "in sync",
        Action::FastForward(_) => "would fast-forward (local unchanged)",
        Action::KeepLocal => "would keep local (ahead)",
        Action::Conflict(_) => "would conflict (write markers)",
        Action::Binary(_) => "would conflict (binary sidecar)",
    }
}

/// Whether an action represents a conflict.
fn is_conflict(action: &Action) -> bool {
    matches!(action, Action::Conflict(_) | Action::Binary(_))
}
