/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   blobstore.rs                                         :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The S3-compatible chunk store — where large objects actually live.
//!
//! The vault carries the keys and the chunk list; the bytes go here, because a fly volume
//! costs roughly ten times what object storage does per gigabyte and a single gRPC call
//! cannot carry a volume at any message limit. The server never sees these bytes at all,
//! so the zero-knowledge boundary moves outward rather than being weakened: this store
//! holds ciphertext whose key it has never been offered.
//!
//! Signing lives in `sigv4`; this module is the object operations alone.

use crate::adapters::sigv4::{self, Credential, EMPTY_SHA256};
use crate::profile::BlobLocation;
use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

/// A bucket on an S3-compatible endpoint, with the credential to sign for it.
pub struct BlobStore {
    endpoint: String,
    region: String,
    bucket: String,
    key_id: String,
    secret: Zeroizing<String>,
    http: reqwest::Client,
}

/// One object as the store reports it in a listing.
pub struct StoredObject {
    pub name: String,
    pub last_modified: String,
}

/// One request against the store: the verb, the object, and anything it carries.
struct Call<'a> {
    method: &'a str,
    name: &'a str,
    query: &'a str,
    payload_hash: &'a str,
    body: Option<Vec<u8>>,
}

impl<'a> Call<'a> {
    /// A bodyless request against one object.
    fn plain(method: &'a str, name: &'a str) -> Self {
        Self {
            method,
            name,
            query: "",
            payload_hash: EMPTY_SHA256,
            body: None,
        }
    }
}

/// Why a profile yielded no object store. The two cases are fixed differently, and
/// reporting one as the other sends the operator to the wrong file.
///
/// A missing credential used to collapse into the same `None` as a missing location, so a
/// profile that demonstrably named an endpoint, a bucket and a region was reported as
/// naming "no object store" — and the advice was to re-run the one command that could not
/// help. Carries the bucket NAME only, which `config show` already prints; never a
/// credential, and deliberately no `Debug` derive that could put one in an error chain.
#[derive(Clone, PartialEq, Eq)]
pub enum StoreGap {
    /// No endpoint or bucket, in the profile or the environment.
    NoLocation,
    /// A location is configured, but the credential is not in the environment.
    NoCredential { bucket: String },
}

/// Written by hand rather than derived. This type travels in an error chain, and `anyhow`
/// prints that chain — a derive would print whatever field someone adds next. Only the
/// bucket name is ever safe to show here, so adding a field forces a decision in this impl
/// instead of leaking by default.
impl std::fmt::Debug for StoreGap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreGap::NoLocation => write!(f, "NoLocation"),
            StoreGap::NoCredential { bucket } => write!(f, "NoCredential({bucket})"),
        }
    }
}

impl StoreGap {
    /// The cause and its fix, naming environment VARIABLES and never their values.
    pub fn explain(&self) -> String {
        match self {
            StoreGap::NoLocation => "this profile names no object store — set one with \
                 `42ctl config endpoint --blobstore <url> --bucket <name>` and export \
                 FT_S3_KEY and FT_S3_SECRET"
                .to_string(),
            StoreGap::NoCredential { bucket } => format!(
                "this profile names object-store bucket `{bucket}`, but FT_S3_KEY and \
                 FT_S3_SECRET are not set in this environment — export them and retry"
            ),
        }
    }
}

impl BlobStore {
    /// Build a store client. `endpoint` is the service root, without the bucket.
    pub fn new(endpoint: &str, region: &str, bucket: &str, key_id: &str, secret: &str) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            region: region.to_string(),
            bucket: bucket.to_string(),
            key_id: key_id.to_string(),
            secret: Zeroizing::new(secret.to_string()),
            http: reqwest::Client::new(),
        }
    }

    /// Build a store from the profile's location and the credential in the environment.
    ///
    /// `None` means large objects are not configured, which push reports as a refusal rather
    /// than inventing a destination. The location may also be given by `FT_S3_ENDPOINT`,
    /// `FT_S3_BUCKET` and `FT_S3_REGION` for CI; the credential comes only from
    /// `FT_S3_KEY` / `FT_S3_SECRET`, because the config file has nowhere to put one.
    pub fn from_profile(location: &BlobLocation) -> Result<Self, StoreGap> {
        let (Some(endpoint), Some(bucket)) = (
            configured(&location.endpoint, "FT_S3_ENDPOINT"),
            configured(&location.bucket, "FT_S3_BUCKET"),
        ) else {
            return Err(StoreGap::NoLocation);
        };
        let region = configured(&location.region, "FT_S3_REGION")
            .unwrap_or_else(|| location.signing_region().to_string());
        let (Some(key), Some(secret)) = (from_env("FT_S3_KEY"), from_env("FT_S3_SECRET")) else {
            return Err(StoreGap::NoCredential { bucket });
        };
        Ok(Self::new(&endpoint, &region, &bucket, &key, &secret))
    }

    /// Store `body` under `name`, overwriting any existing object.
    pub async fn put(&self, name: &str, body: Vec<u8>) -> Result<()> {
        let hash = hex::encode(Sha256::digest(&body));
        let call = Call {
            method: "PUT",
            name,
            query: "",
            payload_hash: &hash,
            body: Some(body),
        };
        let resp = self.send(call).await?;
        status_ok(resp, "put").await
    }

    /// Fetch the object at `name`.
    pub async fn get(&self, name: &str) -> Result<Vec<u8>> {
        let resp = self.send(Call::plain("GET", name)).await?;
        let resp = ok_or_status(resp, "get").await?;
        Ok(resp.bytes().await?.to_vec())
    }

    /// Whether `name` exists. This is what makes an interrupted upload resumable: a chunk
    /// already present is one that does not need sending again.
    pub async fn head(&self, name: &str) -> Result<bool> {
        let resp = self.send(Call::plain("HEAD", name)).await?;
        Ok(resp.status().is_success())
    }

    /// Remove `name`. Used only by collection, never by a push.
    pub async fn delete(&self, name: &str) -> Result<()> {
        let resp = self.send(Call::plain("DELETE", name)).await?;
        status_ok(resp, "delete").await
    }

    /// Every object under `prefix`, following continuation pages to the end.
    ///
    /// Paging is not optional here. A store holding several versions of a large object
    /// passes a thousand keys quickly, and a listing that stopped at the first page would
    /// make collection treat every unseen chunk as absent — which is the direction that
    /// deletes live data rather than merely leaving garbage behind.
    pub async fn list(&self, prefix: &str) -> Result<Vec<StoredObject>> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let page = self.list_page(prefix, token.as_deref()).await?;
            out.extend(parse_listing(&page));
            let next = next_token(&page);
            if next.is_none() || next == token {
                return Ok(out);
            }
            token = next;
        }
    }

    /// Create the bucket if it is absent, so a first push does not require manual setup.
    pub async fn ensure_bucket(&self) -> Result<()> {
        let call = Call {
            method: "PUT",
            name: "",
            query: "",
            payload_hash: EMPTY_SHA256,
            body: Some(Vec::new()),
        };
        let resp = self.send(call).await?;
        match resp.status().as_u16() {
            200 | 204 | 409 => Ok(()),
            _ => status_ok(resp, "create bucket").await,
        }
    }

    /// One page of a listing, as the store's own XML.
    async fn list_page(&self, prefix: &str, token: Option<&str>) -> Result<String> {
        let mut params = vec![("list-type", "2"), ("prefix", prefix)];
        if let Some(token) = token {
            params.push(("continuation-token", token));
        }
        let query = sigv4::canonical_query(&params);
        let call = Call {
            method: "GET",
            name: "",
            query: &query,
            payload_hash: EMPTY_SHA256,
            body: None,
        };
        let resp = ok_or_status(self.send(call).await?, "list").await?;
        Ok(resp.text().await?)
    }

    /// Sign and send one request.
    async fn send(&self, call: Call<'_>) -> Result<reqwest::Response> {
        let path = object_path(&self.bucket, call.name);
        let host = self.host()?;
        let signed = sigv4::Request {
            method: call.method,
            path: &path,
            query: call.query,
            host: &host,
            payload_hash: call.payload_hash,
        };
        let credential = Credential {
            key_id: &self.key_id,
            secret: &self.secret,
            region: &self.region,
        };
        let (auth, stamp) = sigv4::authorization(&signed, &credential)?;
        self.dispatch(&call, &path, &host, (&auth, &stamp)).await
    }

    /// Issue the signed request, attaching exactly the headers the signature covers.
    async fn dispatch(
        &self,
        call: &Call<'_>,
        path: &str,
        host: &str,
        signature: (&str, &str),
    ) -> Result<reqwest::Response> {
        let separator = if call.query.is_empty() { "" } else { "?" };
        let url = format!("{}{path}{separator}{}", self.endpoint, call.query);
        let mut req = self
            .http
            .request(call.method.parse()?, &url)
            .header("host", host)
            .header("x-amz-date", signature.1)
            .header("x-amz-content-sha256", call.payload_hash)
            .header("authorization", signature.0);
        if let Some(bytes) = call.body.clone() {
            req = req.body(bytes);
        }
        req.send().await.context("chunk store request failed")
    }

    /// The Host header, which SigV4 signs and therefore must match the URL exactly.
    fn host(&self) -> Result<String> {
        let rest = self
            .endpoint
            .split_once("://")
            .ok_or_else(|| anyhow!("chunk store endpoint has no scheme: {}", self.endpoint))?
            .1;
        Ok(rest.to_string())
    }
}

/// The request path, bucket first, with the object key appended when there is one.
fn object_path(bucket: &str, name: &str) -> String {
    if name.is_empty() {
        format!("/{bucket}")
    } else {
        format!("/{bucket}/{name}")
    }
}

/// The objects a listing page reports.
fn parse_listing(xml: &str) -> Vec<StoredObject> {
    xml.split("<Contents>")
        .skip(1)
        .filter_map(|block| {
            Some(StoredObject {
                name: tag(block, "Key")?,
                last_modified: tag(block, "LastModified").unwrap_or_default(),
            })
        })
        .collect()
}

/// The token for the next page, or `None` when the listing is complete.
fn next_token(xml: &str) -> Option<String> {
    match tag(xml, "IsTruncated").as_deref() {
        Some("true") => tag(xml, "NextContinuationToken"),
        _ => None,
    }
}

/// The text of the first `<name>` element in `xml`.
///
/// Three lines rather than an XML dependency: the only documents parsed here are S3
/// listings, and the only keys in them are hex digests and slashes, so there is nothing to
/// unescape. A key containing `&` or `<` would need a real parser, and this store never
/// writes one.
fn tag(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].to_string())
}

/// Turn a non-success response into an error naming the operation and the body.
async fn status_ok(resp: reqwest::Response, what: &str) -> Result<()> {
    ok_or_status(resp, what).await.map(|_| ())
}

/// Return the response when it succeeded, or an error carrying the store's own message.
async fn ok_or_status(resp: reqwest::Response, what: &str) -> Result<reqwest::Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    Err(anyhow!(
        "chunk store {what} failed: {status} {}",
        body.trim()
    ))
}

/// A configured value, or the environment variable that stands in for it.
fn configured(value: &str, key: &str) -> Option<String> {
    if value.is_empty() {
        from_env(key)
    } else {
        Some(value.to_string())
    }
}

/// An environment variable, treating empty as unset.
fn from_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bucket_operation_has_no_object_segment() {
        assert_eq!(object_path("b", ""), "/b");
        assert_eq!(object_path("b", "chunks/aa"), "/b/chunks/aa");
    }

    /// A real MinIO listing, kept verbatim, so the extractor is tested against what a store
    /// actually sends rather than against what this file assumed it would send.
    #[test]
    fn a_listing_yields_every_key_with_its_age() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult><Name>qa42chunks</Name><IsTruncated>false</IsTruncated>
<Contents><Key>chunks/aa11</Key><LastModified>2026-09-09T19:13:44.000Z</LastModified>
<Size>4128899</Size></Contents>
<Contents><Key>chunks/bb22</Key><LastModified>2026-09-09T19:14:02.000Z</LastModified>
<Size>131203</Size></Contents></ListBucketResult>"#;
        let objects = parse_listing(xml);
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0].name, "chunks/aa11");
        assert_eq!(objects[1].last_modified, "2026-09-09T19:14:02.000Z");
        assert!(next_token(xml).is_none(), "a complete listing has no token");
    }

    /// A truncated page must hand back its token. Missing this would make collection see
    /// only the first page and treat every later chunk as unreferenced garbage.
    #[test]
    fn a_truncated_listing_yields_its_continuation_token() {
        let xml = "<ListBucketResult><IsTruncated>true</IsTruncated>\
                   <NextContinuationToken>abc123==</NextContinuationToken>\
                   <Contents><Key>chunks/aa</Key></Contents></ListBucketResult>";
        assert_eq!(next_token(xml).as_deref(), Some("abc123=="));
    }

    /// An empty bucket is an empty listing, not an error and not a phantom entry.
    #[test]
    fn an_empty_listing_yields_nothing() {
        let xml = "<ListBucketResult><Name>b</Name><IsTruncated>false</IsTruncated>\
                   </ListBucketResult>";
        assert!(parse_listing(xml).is_empty());
        assert!(next_token(xml).is_none());
    }

    /// A live round trip against a real S3 implementation, skipped unless an endpoint is
    /// supplied. The signing arithmetic is necessary and nowhere near sufficient: it says
    /// nothing about whether the path, the Host header and the signed-header list agree
    /// with what a server will actually accept. Only a server can refuse that.
    #[tokio::test]
    #[ignore]
    async fn a_live_round_trip_stores_reads_lists_and_removes() {
        let store = BlobStore::from_profile(&BlobLocation::default())
            .expect("FT_S3_ENDPOINT / FT_S3_BUCKET / FT_S3_KEY / FT_S3_SECRET must be set");
        store.ensure_bucket().await.expect("bucket");
        let prefix = format!("live-{}/", std::process::id());
        let name = format!("{prefix}chunk");
        assert!(
            !store.head(&name).await.expect("head before"),
            "object existed already"
        );
        store
            .put(&name, b"chunk-bytes-0001".to_vec())
            .await
            .expect("put");
        assert!(
            store.head(&name).await.expect("head after"),
            "put did not store"
        );
        assert_eq!(store.get(&name).await.expect("get"), b"chunk-bytes-0001");
        let listed = store.list(&prefix).await.expect("list");
        assert_eq!(
            listed.len(),
            1,
            "the listing must find exactly what was put"
        );
        assert_eq!(listed[0].name, name);
        assert!(
            !listed[0].last_modified.is_empty(),
            "collection needs an age to refuse deleting a chunk a push is still writing"
        );
        store.delete(&name).await.expect("delete");
        assert!(
            store.list(&prefix).await.expect("list after").is_empty(),
            "delete did not remove"
        );
    }

    /// A configured profile whose credential is absent used to be reported as naming "no
    /// object store", sending the operator to re-run `config endpoint` — the one action that
    /// cannot help. The two gaps must read differently.
    #[test]
    fn a_missing_credential_is_not_reported_as_a_missing_store() {
        let text = StoreGap::NoCredential {
            bucket: "vault42-seeds".to_string(),
        }
        .explain();
        assert!(text.contains("vault42-seeds"), "{text}");
        assert!(
            text.contains("FT_S3_KEY") && text.contains("FT_S3_SECRET"),
            "{text}"
        );
        assert!(
            !text.contains("names no object store"),
            "a missing credential still reads as a missing store: {text}"
        );
    }

    /// The other half: a profile that truly names nothing keeps the original wording, which
    /// the QA battery asserts on.
    #[test]
    fn a_missing_location_still_names_the_flag_to_set() {
        let text = StoreGap::NoLocation.explain();
        assert!(text.contains("names no object store"), "{text}");
        assert!(text.contains("--blobstore"), "{text}");
    }

    /// The refusal names environment VARIABLES, never their values — the gap carries the
    /// bucket and nothing else, and derives no `Debug` that could put a credential in a chain.
    #[test]
    fn a_refusal_never_carries_a_credential_value() {
        for gap in [
            StoreGap::NoLocation,
            StoreGap::NoCredential {
                bucket: "b".to_string(),
            },
        ] {
            let text = gap.explain();
            assert!(!text.contains("AKIA"), "{text}");
            assert!(!text.to_lowercase().contains("secret="), "{text}");
        }
    }
}
