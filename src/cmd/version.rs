/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   version.rs                                           :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl version` — report the crate version and the commit it was built from. The
//! commit is stamped at build time by `build.rs` (`FT_GIT_SHA`); `update`/release flows
//! rely on this matching the published artifact.

/// Print `42ctl <version> (<commit>)`.
pub fn run() -> anyhow::Result<()> {
    println!(
        "42ctl {} ({})",
        env!("CARGO_PKG_VERSION"),
        env!("FT_GIT_SHA")
    );
    Ok(())
}
