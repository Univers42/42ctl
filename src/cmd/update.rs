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

//! `42ctl update` — self-update. Reads the cargo-dist install receipt, verifies the
//! matching release artifact's SHA-256 against the signed dist-manifest, and only then
//! atomically swaps the running binary. A failed verification aborts with no change.
//! Installs without a receipt (cargo install / Docker) update via their package manager.

use axoupdater::AxoUpdater;

/// Self-update the running binary, verifying the artifact checksum before the swap.
pub fn run() -> anyhow::Result<()> {
    let mut updater = AxoUpdater::new_for("42ctl");
    if updater.load_receipt().is_err() {
        print_no_receipt();
        return Ok(());
    }
    match updater.run_sync()? {
        Some(_) => println!("42ctl updated — run `42ctl version` to confirm the new build"),
        None => println!("42ctl is already up to date"),
    }
    Ok(())
}

/// Explain why self-update is unavailable and how to update instead.
fn print_no_receipt() {
    println!("self-update needs an installer receipt (curl|sh / npm / Homebrew install).");
    println!("cargo install:  cargo install c42 --force");
    println!("Docker:         docker pull <namespace>/42ctl:latest");
}
