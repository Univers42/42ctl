/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   version.rs                                           :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                        +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl version` — report the crate version, the commit it was built from, and the
//! target triple. The first line is the stable, script-parseable `42ctl X.Y.Z (<commit>)`
//! (`FT_GIT_SHA` from `build.rs`); the target names the release asset `update` fetches.

use crate::ui;

/// Print `42ctl <version> (<commit>)`, then the build target.
pub fn run() -> anyhow::Result<()> {
    println!(
        "42ctl {} ({})",
        env!("CARGO_PKG_VERSION"),
        env!("FT_GIT_SHA")
    );
    ui::field("target", env!("FT_TARGET"));
    Ok(())
}
