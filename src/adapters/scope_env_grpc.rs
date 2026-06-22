/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope_env_grpc.rs                                   :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The shared-env-secret + scope-rotation gRPC surface on the signed `Session` — store one
//! scope-sealed env secret (`put_env_secret`), fetch it back (`get_env_secret`), enumerate a
//! scope/epoch's paths (`list_env_secrets`), and forward-securely rotate a scope
//! (`rotate_scope`). Each carries the Ed25519-signed auth metadata; the server gates them
//! behind `VAULT42_SCOPE_KEYS_ENABLED` and never decrypts the envelopes crossing the wire.

use crate::adapters::api::Session;
use tonic::{Code, Request};
use vault42_proto::vault::v1::{
    EnvSecretEntry, GetEnvSecretRequest, ListEnvSecretsRequest, PutEnvSecretRequest,
    RotateScopeRequest, WrapScopeKeyRequest,
};

/// One shared env secret to store: hex `scope_id`, `epoch`, opaque `path`, the sealed
/// `envelope`, and `expected_prev_rev` (0 = create) for optimistic concurrency (≤4 args).
pub struct EnvSecretPut<'a> {
    pub scope_id: &'a str,
    pub epoch: u32,
    pub path: &'a str,
    pub envelope: Vec<u8>,
    pub expected_prev_rev: u64,
}

impl Session {
    /// Store `put`'s scope-sealed env secret, returning the new version. The server appends
    /// it under `(scope_id, epoch, path)` after verifying the author signature only.
    pub async fn put_env_secret(&mut self, put: EnvSecretPut<'_>) -> anyhow::Result<u64> {
        let mut request = Request::new(PutEnvSecretRequest {
            scope_id: put.scope_id.to_string(),
            epoch: put.epoch,
            path: put.path.to_string(),
            envelope: put.envelope,
            expected_prev_rev: put.expected_prev_rev,
        });
        self.authorize(&mut request, "/vault.v1.Vault/PutEnvSecret")?;
        Ok(self
            .client
            .put_env_secret(request)
            .await?
            .into_inner()
            .version)
    }

    /// Fetch one env-secret version (`0` ⇒ latest) → `(envelope, author_pubkey)`, or `None`
    /// when the path has no version yet (server `NotFound`).
    pub async fn get_env_secret(
        &mut self,
        scope_id: &str,
        epoch: u32,
        path: &str,
    ) -> anyhow::Result<Option<(Vec<u8>, Vec<u8>)>> {
        let mut request = Request::new(GetEnvSecretRequest {
            scope_id: scope_id.to_string(),
            epoch,
            path: path.to_string(),
            version: 0,
        });
        self.authorize(&mut request, "/vault.v1.Vault/GetEnvSecret")?;
        match self.client.get_env_secret(request).await {
            Ok(resp) => {
                let r = resp.into_inner();
                Ok(Some((r.envelope, r.author_pubkey)))
            }
            Err(status) if status.code() == Code::NotFound => Ok(None),
            Err(status) => Err(status.into()),
        }
    }

    /// List every env-secret `(path, version)` of `(scope_id, epoch)` at its latest version.
    pub async fn list_env_secrets(
        &mut self,
        scope_id: &str,
        epoch: u32,
    ) -> anyhow::Result<Vec<EnvSecretEntry>> {
        let mut request = Request::new(ListEnvSecretsRequest {
            scope_id: scope_id.to_string(),
            epoch,
        });
        self.authorize(&mut request, "/vault.v1.Vault/ListEnvSecrets")?;
        Ok(self
            .client
            .list_env_secrets(request)
            .await?
            .into_inner()
            .entries)
    }

    /// Forward-securely rotate `scope_id` to `new_epoch`: deposit one re-wrap per remaining
    /// member (built client-side), returning how many the server stored under `new_epoch`.
    pub async fn rotate_scope(
        &mut self,
        scope_id: &str,
        new_epoch: u32,
        rewraps: Vec<WrapScopeKeyRequest>,
    ) -> anyhow::Result<u32> {
        let mut request = Request::new(RotateScopeRequest {
            scope_id: scope_id.to_string(),
            new_epoch,
            rewraps,
        });
        self.authorize(&mut request, "/vault.v1.Vault/RotateScope")?;
        Ok(self
            .client
            .rotate_scope(request)
            .await?
            .into_inner()
            .rewrapped)
    }
}
