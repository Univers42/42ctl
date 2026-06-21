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
    Session::connect(&endpoint.server, identity, contract).await
}

/// `push` — scan + seal + upload the project's tree and its encrypted manifest.
pub async fn push(profile: &str, project: Option<&str>) -> anyhow::Result<()> {
    let mut session = open_session(profile).await?;
    session.cmd_push(project).await
}

/// `pull` — fetch the manifest + blobs and materialize the tree (dry-run unless apply).
pub async fn pull(
    profile: &str,
    project: Option<&str>,
    apply: bool,
    force: bool,
    backup: bool,
) -> anyhow::Result<()> {
    let mut session = open_session(profile).await?;
    session.cmd_pull(project, Opts { apply, force, backup }).await
}
