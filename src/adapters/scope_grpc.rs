/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   scope_grpc.rs                                       :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/22 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/22 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The vault42 scope-key gRPC surface on the signed `Session` — deposit a member's wrap
//! (`wrap_scope_key`), fetch the caller's own wrap (`get_scope_key`), and list a scope's
//! provisioned members (`list_scope_members`). Each carries the Ed25519-signed auth
//! metadata; the server gates them behind `VAULT42_SCOPE_KEYS_ENABLED`. The blob crossing
//! the wire is an opaque serialized `GrantedScopeKey` — the server never decrypts it.

use crate::adapters::api::Session;
use tonic::Request;
use vault42_proto::vault::v1::{GetScopeKeyRequest, ListScopeMembersRequest, WrapScopeKeyRequest};

/// A member's deposited wrap: the storage `member_id`, hex `scope_id`, `epoch`, the opaque
/// serialized `GrantedScopeKey`, and the granter's 32-byte Ed25519 key (keeps deposits ≤4 args).
pub struct ScopeDeposit<'a> {
    pub member_id: &'a str,
    pub scope_id: &'a str,
    pub epoch: u32,
    pub granted_blob: Vec<u8>,
    pub granter_pubkey: Vec<u8>,
}

impl Session {
    /// Deposit `deposit`'s wrap into the member's namespace (a foreign-owner write like
    /// Share); the server verifies the granter signature without decrypting.
    pub async fn wrap_scope_key(&mut self, deposit: ScopeDeposit<'_>) -> anyhow::Result<()> {
        let mut request = Request::new(WrapScopeKeyRequest {
            member_id: deposit.member_id.to_string(),
            scope_id: deposit.scope_id.to_string(),
            epoch: deposit.epoch,
            granted_blob: deposit.granted_blob,
            granter_pubkey: deposit.granter_pubkey,
        });
        self.authorize(&mut request, "/vault.v1.Vault/WrapScopeKey")?;
        self.client.wrap_scope_key(request).await?;
        Ok(())
    }

    /// Fetch the caller's OWN wrap for `(scope_id, epoch)` → `(granted_blob, granter_pubkey)`,
    /// or `None` when the caller has no wrap yet (server `NotFound`).
    pub async fn get_scope_key(
        &mut self,
        scope_id: &str,
        epoch: u32,
    ) -> anyhow::Result<Option<(Vec<u8>, Vec<u8>)>> {
        let mut request = Request::new(GetScopeKeyRequest {
            scope_id: scope_id.to_string(),
            epoch,
        });
        self.authorize(&mut request, "/vault.v1.Vault/GetScopeKey")?;
        match self.client.get_scope_key(request).await {
            Ok(resp) => {
                let r = resp.into_inner();
                Ok(Some((r.granted_blob, r.granter_pubkey)))
            }
            Err(status) if status.code() == tonic::Code::NotFound => Ok(None),
            Err(status) => Err(status.into()),
        }
    }

    /// List the provisioned member ids of `(scope_id, epoch)` the caller may see.
    pub async fn list_scope_members(
        &mut self,
        scope_id: &str,
        epoch: u32,
    ) -> anyhow::Result<Vec<String>> {
        let mut request = Request::new(ListScopeMembersRequest {
            scope_id: scope_id.to_string(),
            epoch,
        });
        self.authorize(&mut request, "/vault.v1.Vault/ListScopeMembers")?;
        let resp = self.client.list_scope_members(request).await?;
        Ok(resp
            .into_inner()
            .members
            .into_iter()
            .map(|m| m.member_id)
            .collect())
    }
}
