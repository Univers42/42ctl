/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   sync.rs                                              :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl push` / `pull` — path-aware project sync. Opens a signed gRPC session (like
//! the vault verbs) and delegates to the `ops::sync` use-cases. All sealing/decryption
//! is local; only opaque envelopes + an encrypted manifest cross the wire.

use crate::adapters::api::Session;
use crate::adapters::{creds, passphrase};
use crate::core::materialize::Opts;
use crate::profile::Config;

/// Open a signed session: resolve the endpoint, unlock the identity, load the contract.
pub(in crate::cmd) async fn open_session(profile: &str) -> anyhow::Result<Session> {
    let endpoint = Config::load()?.endpoint(profile)?;
    let identity = passphrase::unlock()?;
    let contract = creds::load(profile);
    Session::connect(&endpoint, identity, contract).await
}

/// `push` — scan + seal + upload the project's tree and its encrypted manifest.
/// `prune` mirrors: manifest entries whose file is no longer scanned are dropped.
pub async fn push(profile: &str, project: Option<&str>, prune: bool) -> anyhow::Result<()> {
    let mut session = open_session(profile).await?;
    session.cmd_push(project, prune).await
}

/// `pull` — fetch the manifest + blobs and materialize the tree (dry-run unless apply).
///
/// `at` selects a manifest version to restore instead of the latest. The write options
/// travel as one struct because they always travel together, and because five loose
/// booleans at a call site is how the wrong one gets passed.
pub async fn pull(
    profile: &str,
    project: Option<&str>,
    at: Option<u64>,
    opts: Opts,
) -> anyhow::Result<()> {
    let mut session = open_session(profile).await?;
    session.cmd_pull(project, at, opts).await
}
