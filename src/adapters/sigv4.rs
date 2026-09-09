/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   sigv4.rs                                             :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! AWS SigV4 request signing for the S3-compatible chunk store.
//!
//! Request authentication, not content encryption: content encryption stays in
//! `vault42-core`, which owns it. Separated from the store's object operations because it
//! is a distinct concern with a distinct kind of test — the arithmetic here is checked
//! against an independently computed vector, while whether a real server accepts the
//! result is something only a real server can settle.
//!
//! The signing key is derived per request and never persisted.

use anyhow::{anyhow, Result};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

/// SHA-256 of the empty string, the payload hash every bodyless request must send.
pub const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// The headers this signer covers, in the order the canonical request requires.
const SIGNED_HEADERS: &str = "host;x-amz-content-sha256;x-amz-date";

/// What is being signed.
pub struct Request<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    pub host: &'a str,
    pub payload_hash: &'a str,
}

/// Who is signing.
pub struct Credential<'a> {
    pub key_id: &'a str,
    pub secret: &'a str,
    pub region: &'a str,
}

/// The Authorization header value and the `x-amz-date` stamp it is bound to.
///
/// Both come back together because a header signed for one instant and sent with another is
/// rejected, and computing them in two places is how they drift apart.
pub fn authorization(req: &Request<'_>, cred: &Credential<'_>) -> Result<(String, String)> {
    let (stamp, date) = timestamps()?;
    let canonical = canonical_request(req, &stamp);
    let scope = format!("{date}/{}/s3/aws4_request", cred.region);
    let to_sign = format!(
        "AWS4-HMAC-SHA256\n{stamp}\n{scope}\n{}",
        hex::encode(Sha256::digest(canonical.as_bytes()))
    );
    let signing = signing_key(cred.secret, &date, cred.region)?;
    let signature = hex::encode(hmac(&signing, to_sign.as_bytes())?);
    let header = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={SIGNED_HEADERS}, \
         Signature={signature}",
        cred.key_id
    );
    Ok((header, stamp))
}

/// The canonical query string: parameters sorted by name, both sides percent-encoded.
///
/// Sorting is part of the specification rather than a tidiness choice — a server that sorts
/// differently from the signer computes a different signature and refuses the request.
pub fn canonical_query(params: &[(&str, &str)]) -> String {
    let mut encoded: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{}={}", uri_encode(k), uri_encode(v)))
        .collect();
    encoded.sort();
    encoded.join("&")
}

/// An instant `seconds` in the past, shaped like an S3 `LastModified`.
///
/// Formatted rather than parsed on purpose: this format sorts lexicographically in the same
/// order it sorts chronologically, so a caller can compare against a stored timestamp
/// without a date parser and without a date dependency.
pub fn instant_ago(seconds: i64) -> Result<String> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let then = now - seconds;
    let (y, m, d) = civil_from_days(then.div_euclid(86_400));
    let rem = then.rem_euclid(86_400);
    Ok(format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    ))
}

/// The canonical request SigV4 hashes. Header order and the blank lines are part of the
/// specification, so this is deliberately literal rather than built from a map.
fn canonical_request(req: &Request<'_>, stamp: &str) -> String {
    format!(
        "{}\n{}\n{}\nhost:{}\nx-amz-content-sha256:{}\nx-amz-date:{stamp}\n\n\
         {SIGNED_HEADERS}\n{}",
        req.method, req.path, req.query, req.host, req.payload_hash, req.payload_hash
    )
}

/// Percent-encode one query component: unreserved characters pass, everything else escapes.
fn uri_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Derive the request signing key: date, then region, then service, then the terminator.
fn signing_key(secret: &str, date: &str, region: &str) -> Result<Vec<u8>> {
    let mut key = hmac(format!("AWS4{secret}").as_bytes(), date.as_bytes())?;
    key = hmac(&key, region.as_bytes())?;
    key = hmac(&key, b"s3")?;
    hmac(&key, b"aws4_request")
}

/// One HMAC-SHA256 step.
fn hmac(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| anyhow!("bad signing key"))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

/// The two timestamps SigV4 needs: the full instant and the date alone.
fn timestamps() -> Result<(String, String)> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    let date = format!("{y:04}{m:02}{d:02}");
    let stamp = format!(
        "{date}T{:02}{:02}{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    );
    Ok((stamp, date))
}

/// Days since the Unix epoch to a civil date, by Howard Hinnant's algorithm. Written out
/// rather than pulling a date crate for the one timestamp SigV4 requires.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_converts_to_its_civil_date() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn a_leap_day_converts_correctly() {
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
    }

    /// The expected value here was computed independently rather than recalled: the first
    /// constant written was wrong, and the temptation on a red is to adjust the code until
    /// it matches the expectation. Verifying the expectation first is what stops a correct
    /// implementation being "fixed" into a broken one.
    #[test]
    fn the_signing_key_matches_an_independently_computed_vector() {
        let key = signing_key(
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            "20130524",
            "us-east-1",
        )
        .expect("derive");
        assert_eq!(
            hex::encode(key),
            "f117494eff5d09da21cbf7f0339559ea04fc9582d31299cb992be70a6b27c97a"
        );
    }

    /// A different date, region or service must give a different key, or the derivation
    /// chain has collapsed and the first vector proves nothing.
    #[test]
    fn the_signing_key_is_bound_to_date_and_region() {
        let secret = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";
        let base = signing_key(secret, "20130524", "us-east-1").expect("base");
        assert_ne!(
            base,
            signing_key(secret, "20130525", "us-east-1").expect("date")
        );
        assert_ne!(
            base,
            signing_key(secret, "20130524", "eu-west-1").expect("region")
        );
    }

    /// Sorted by the ENCODED name, and every reserved byte escaped. A continuation token is
    /// base64 and routinely carries `+`, `/` and `=`, each of which changes the signature if
    /// it travels unescaped.
    #[test]
    fn a_query_string_is_sorted_and_escaped() {
        let query = canonical_query(&[
            ("prefix", "chunks/ab"),
            ("list-type", "2"),
            ("continuation-token", "a+b/c="),
        ]);
        assert_eq!(
            query,
            "continuation-token=a%2Bb%2Fc%3D&list-type=2&prefix=chunks%2Fab"
        );
    }

    #[test]
    fn an_empty_query_string_is_empty() {
        assert_eq!(canonical_query(&[]), "");
    }

    /// The canonical request carries the query on its own line, so a signed listing and a
    /// signed object read differ in the one place the specification says they must.
    #[test]
    fn the_query_line_is_part_of_what_gets_signed() {
        let base = Request {
            method: "GET",
            path: "/bucket",
            query: "",
            host: "s3.example",
            payload_hash: EMPTY_SHA256,
        };
        let listed = Request {
            query: "list-type=2",
            ..base
        };
        assert_ne!(
            canonical_request(&base, "20260101T000000Z"),
            canonical_request(&listed, "20260101T000000Z")
        );
    }

    /// An instant in the past must sort before one closer to now, since that comparison is
    /// what keeps collection from deleting a chunk an in-flight push has just uploaded.
    #[test]
    fn an_older_instant_sorts_before_a_newer_one() {
        let old = instant_ago(86_400).expect("a day ago");
        let recent = instant_ago(60).expect("a minute ago");
        assert!(old < recent, "{old} must sort before {recent}");
    }
}
