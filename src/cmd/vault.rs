/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   vault.rs                                             :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl vault` (alias `secrets`) — zero-knowledge secret operations. Every plaintext
//! seal/open happens locally via vault-crypto; only opaque envelopes cross the wire.
//! Stub until P3.

use crate::cli::Vault;
use crate::profile::Config;

/// Stub: zero-knowledge vault ops land in P3.
pub fn run(_cmd: &Vault, profile: &str) -> anyhow::Result<()> {
    let endpoint = Config::load()?.endpoint(profile)?;
    println!(
        "vault: wired in P3 — zero-knowledge ops against {}",
        endpoint.server
    );
    Ok(())
}
