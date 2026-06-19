/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   manage.rs                                            :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `ls`, `rm`, and `rotate`. Listing and removal are owner-scoped by the server to the
//! calling identity; `rotate` re-seals a secret under a fresh DEK and pushes it at the
//! next version. All of it is signed + contract-bound and, for rotate, sealed locally.

use crate::adapters::api::Session;
use crate::adapters::compose::{self, SelfSeal};
use tonic::Request;
use vault42_proto::vault::v1::{LsRequest, PushRequest, RmRequest};

impl Session {
    /// List the caller's secrets under `prefix`.
    pub async fn cmd_ls(&mut self, prefix: &str) -> anyhow::Result<()> {
        let mut request = Request::new(LsRequest {
            prefix: prefix.to_string(),
        });
        self.authorize(&mut request, "/vault.v1.Vault/Ls")?;
        for secret in self.client.ls(request).await?.into_inner().secrets {
            println!(
                "{}\tv{}\t{}",
                secret.path, secret.version, secret.updated_at
            );
        }
        Ok(())
    }

    /// Remove every version of `path`.
    pub async fn cmd_rm(&mut self, path: &str) -> anyhow::Result<()> {
        let mut request = Request::new(RmRequest {
            path: path.to_string(),
            version: 0,
        });
        self.authorize(&mut request, "/vault.v1.Vault/Rm")?;
        let tombstoned = self.client.rm(request).await?.into_inner().tombstoned;
        println!("{}", if tombstoned { "removed" } else { "not found" });
        Ok(())
    }

    /// Re-seal the secret at `path` under a fresh DEK and push it at the next version.
    pub async fn cmd_rotate(&mut self, path: &str) -> anyhow::Result<()> {
        let current = self.current_version(path).await?;
        if current == 0 {
            anyhow::bail!("no secret at {path} to rotate");
        }
        let plaintext = self.fetch_plaintext(path).await?;
        let envelope = compose::self_envelope(
            &self.identity,
            &SelfSeal {
                owner: &self.principal,
                path,
                rev: current + 1,
                plaintext: plaintext.as_slice(),
            },
        )?;
        let mut request = Request::new(PushRequest {
            path: path.to_string(),
            envelope,
            expected_prev_rev: current,
        });
        self.authorize(&mut request, "/vault.v1.Vault/Rotate")?;
        let version = self.client.rotate(request).await?.into_inner().version;
        println!("rotated {path} to version {version}");
        Ok(())
    }
}
