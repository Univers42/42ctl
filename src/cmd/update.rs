/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   update.rs                                            :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl update` — self-update. Downloads the matching release artifact, verifies its
//! signature + provenance + checksum, and only then atomically swaps the running binary.
//! A failed verification aborts with no change. Stub until P7.

/// Stub: verify-before-swap self-update lands in P7.
pub fn run() -> anyhow::Result<()> {
    println!("update: wired in P7 — verifies signature + provenance + checksum before swapping");
    Ok(())
}
