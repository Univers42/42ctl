/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   auth.rs                                              :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl auth` — authenticate against the platform. Login obtains a signed contract for
//! this identity from the authority and stores the credential in the OS keyring (P2);
//! `whoami`/`status`/`logout` read/clear it. Stub until P2.

use crate::cli::Auth;
use crate::profile::Config;

/// Stub: the auth/contract flow lands in P2.
pub fn run(_cmd: &Auth, profile: &str) -> anyhow::Result<()> {
    let endpoint = Config::load()?.endpoint(profile)?;
    println!(
        "auth: wired in P2 — login obtains a contract from {}",
        endpoint.authority
    );
    Ok(())
}
