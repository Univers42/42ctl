/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   db.rs                                                :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl db` — read RBAC-checked encrypted records from the platform and decrypt them
//! client-side. The server enforces access; vault-crypto does the decryption locally.
//! Stub until P3.

use crate::cli::Db;
use crate::profile::Config;

/// Stub: encrypted-record reads land in P3.
pub fn run(_cmd: &Db, profile: &str) -> anyhow::Result<()> {
    let endpoint = Config::load()?.endpoint(profile)?;
    println!(
        "db: wired in P3 — RBAC-checked records from {}, decrypted locally",
        endpoint.server
    );
    Ok(())
}
