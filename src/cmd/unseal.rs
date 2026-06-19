/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   unseal.rs                                            :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl unseal` — operator-only. Drives the vault42 server's unseal RPC. Stub until the
//! unseal surface is wired through the gRPC client.

use crate::profile::Config;

/// Stub: the operator unseal flow is wired with the gRPC client.
pub fn run(profile: &str) -> anyhow::Result<()> {
    let endpoint = Config::load()?.endpoint(profile)?;
    println!(
        "unseal: operator-only — wired with the vault42 unseal RPC ({})",
        endpoint.server
    );
    Ok(())
}
