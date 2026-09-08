/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   checksum.rs                                          :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! SHA-256 verification of a downloaded release asset against the release's `SHA256SUMS`
//! (the `sha256sum` text format: `<hex>  <name>` per line, `*<name>` for binary mode).
//! Pure — no I/O — so the refuse-on-mismatch path is unit-tested.

use anyhow::{bail, Context};
use sha2::{Digest, Sha256};

/// The lowercase hex SHA-256 recorded for `name` in a `sha256sum`-format manifest.
pub fn expected_digest(sums: &str, name: &str) -> anyhow::Result<String> {
    sums.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?, fields.next()?))
        })
        .find(|(_, entry)| entry.trim_start_matches('*') == name)
        .map(|(digest, _)| digest.to_ascii_lowercase())
        .with_context(|| format!("SHA256SUMS has no entry for '{name}'"))
}

/// Fail unless `bytes` hash to `expected` — a mismatch means the download is refused.
pub fn verify_digest(bytes: &[u8], expected: &str) -> anyhow::Result<()> {
    let actual = hex::encode(Sha256::digest(bytes));
    if actual != expected {
        bail!("checksum mismatch — expected {expected}, got {actual}; refusing the download");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    #[test]
    fn digest_is_found_by_asset_name_in_both_formats() {
        let sums = format!("{HELLO_SHA256}  42ctl-x86_64-unknown-linux-musl\nabc *other\n");
        let digest = expected_digest(&sums, "42ctl-x86_64-unknown-linux-musl").unwrap();
        assert_eq!(digest, HELLO_SHA256);
        assert_eq!(expected_digest(&sums, "other").unwrap(), "abc");
    }

    #[test]
    fn missing_asset_is_an_error() {
        assert!(expected_digest("deadbeef  a\n", "b").is_err());
    }

    #[test]
    fn verify_accepts_matching_bytes_and_refuses_tampered_ones() {
        assert!(verify_digest(b"hello", HELLO_SHA256).is_ok());
        assert!(verify_digest(b"hellO", HELLO_SHA256).is_err());
    }
}
