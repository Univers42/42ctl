/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   compose.rs                                           :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Local envelope composition — the single place the CLI builds signed metadata, picks
//! the recipient set, and seals. Keeping it here means `set`/`rotate`/`share` stay small
//! and the metadata shape has one definition (DRY). All sealing is local; only the
//! resulting opaque bytes ever leave the machine.

use crate::adapters::derive;
use vault42_core::{seal, Identity, Kind, Metadata, RecipientPublicKey, Recipients, DEFAULT_MODE};

/// Build v2 metadata for `(owner, path, rev)` with a deterministic secret id. The
/// path-aware fields stay at their non-project defaults; project push/pull populates
/// them via `project_envelope`, and a leaf's `relative_path` always stays empty on the
/// wire (the real path lives only in the encrypted manifest — zero-knowledge).
fn metadata(owner: &str, path: &str, rev: u64) -> Metadata {
    Metadata {
        version: 2,
        secret_id: derive::secret_id(owner, path),
        tenant: "self".to_string(),
        owner: owner.to_string(),
        rev,
        content_type: "opaque".to_string(),
        recovery_optin: false,
        project_id: String::new(),
        relative_path: String::new(),
        kind: Kind::Generic,
        mode: DEFAULT_MODE,
    }
}

/// The owner/path/rev/plaintext bundle a seal needs (keeps `self_envelope` ≤4 params).
pub struct SelfSeal<'a> {
    pub owner: &'a str,
    pub path: &'a str,
    pub rev: u64,
    pub plaintext: &'a [u8],
}

/// Seal `plaintext` for the caller's own identity at `owner`/`path`/`rev`.
pub fn self_envelope(identity: &Identity, seal_spec: &SelfSeal) -> anyhow::Result<Vec<u8>> {
    let recipients = Recipients {
        users: &[identity.encryption_public()],
        recovery: None,
    };
    Ok(seal(
        seal_spec.plaintext,
        metadata(seal_spec.owner, seal_spec.path, seal_spec.rev),
        &recipients,
        identity.signing_key(),
    )?
    .to_bytes()?)
}

/// The owner/path/friend-key/plaintext bundle a shared seal needs (keeps ≤4 params).
pub struct SharedSeal<'a> {
    pub owner: &'a str,
    pub path: &'a str,
    pub friend: RecipientPublicKey,
    pub plaintext: &'a [u8],
}

/// The path-aware seal spec for a project blob (or its manifest). `relative_path` is
/// NEVER set on the wire — the real path lives only inside the encrypted manifest (ZK);
/// `vault_path` is the opaque server location.
pub struct ProjectSeal<'a> {
    pub owner: &'a str,
    pub vault_path: &'a str,
    pub project_id: &'a str,
    pub kind: Kind,
    pub mode: u32,
    pub rev: u64,
    pub plaintext: &'a [u8],
}

/// Seal `plaintext` as a path-aware project blob for the caller's own identity. The
/// secret id is derived from the opaque `vault_path` (so `open` binds the read scope);
/// the real path is omitted from the leaf metadata.
pub fn project_envelope(identity: &Identity, spec: &ProjectSeal) -> anyhow::Result<Vec<u8>> {
    let meta = Metadata {
        version: 2,
        secret_id: derive::secret_id(spec.owner, spec.vault_path),
        tenant: "self".to_string(),
        owner: spec.owner.to_string(),
        rev: spec.rev,
        content_type: "project".to_string(),
        recovery_optin: false,
        project_id: spec.project_id.to_string(),
        relative_path: String::new(), // sec: ZK — the real path lives only in the manifest
        kind: spec.kind,
        mode: spec.mode,
    };
    let recipients = Recipients {
        users: &[identity.encryption_public()],
        recovery: None,
    };
    Ok(seal(spec.plaintext, meta, &recipients, identity.signing_key())?.to_bytes()?)
}

/// The scope-sealed env-secret spec: like `ProjectSeal`, but the sole recipient is the
/// env's X25519 SCOPE public key (`scope_pub`) — never the caller — so any holder of the
/// scope SECRET (a wrapped member) can open it. `owner`/`project_id` are both the hex
/// scope id; `vault_path` is the opaque server path; the real path stays out of the leaf.
pub struct ScopeSeal<'a> {
    pub owner: &'a str,
    pub vault_path: &'a str,
    pub project_id: &'a str,
    pub scope_pub: RecipientPublicKey,
    pub rev: u64,
    pub plaintext: &'a [u8],
}

/// Seal `plaintext` to the env SCOPE public key (NOT the caller), authored by `identity`.
/// The secret id binds the opaque `vault_path`; the real path is omitted from the leaf (ZK).
pub fn scope_envelope(identity: &Identity, spec: &ScopeSeal) -> anyhow::Result<Vec<u8>> {
    let meta = Metadata {
        version: 2,
        secret_id: derive::secret_id(spec.owner, spec.vault_path),
        tenant: "self".to_string(),
        owner: spec.owner.to_string(),
        rev: spec.rev,
        content_type: "env-secret".to_string(),
        recovery_optin: false,
        project_id: spec.project_id.to_string(),
        relative_path: String::new(), // sec: ZK — the real path lives only in the manifest
        kind: Kind::Generic,
        mode: DEFAULT_MODE,
    };
    let recipients = Recipients {
        users: std::slice::from_ref(&spec.scope_pub),
        recovery: None,
    };
    Ok(seal(spec.plaintext, meta, &recipients, identity.signing_key())?.to_bytes()?)
}

/// Seal `plaintext` for a friend (their X25519 key) plus the author, under the friend's
/// owner space at `path`.
pub fn shared_envelope(identity: &Identity, seal_spec: &SharedSeal) -> anyhow::Result<Vec<u8>> {
    let recipients = Recipients {
        users: &[seal_spec.friend, identity.encryption_public()],
        recovery: None,
    };
    Ok(seal(
        seal_spec.plaintext,
        metadata(seal_spec.owner, seal_spec.path, 1),
        &recipients,
        identity.signing_key(),
    )?
    .to_bytes()?)
}
