/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   io.rs                                                :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `import` and `export` of `.env` files. `import` reads a dotenv file and seals each
//! `KEY=VALUE` as the secret `<prefix>/KEY`; `export` lists the caller's secrets under a
//! prefix, decrypts each locally, and prints `KEY=value`. Both reuse the same
//! zero-knowledge set/fetch path — the server never sees a plaintext value.

use crate::adapters::api::Session;
use tonic::Request;
use vault42_proto::vault::v1::LsRequest;
use zeroize::Zeroizing;

/// Split a dotenv line into `KEY=VALUE`, ignoring blanks, comments, and malformed lines.
fn parse_env_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let (key, value) = trimmed.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some((key.to_string(), value.trim().to_string()))
}

/// Join `prefix` and `key` into a secret path (`prefix/key`, or just `key` when empty).
fn secret_path(prefix: &str, key: &str) -> String {
    let trimmed = prefix.trim_matches('/');
    if trimmed.is_empty() {
        key.to_string()
    } else {
        format!("{trimmed}/{key}")
    }
}

impl Session {
    /// Read the dotenv file at `source` and seal each `KEY=VALUE` as `<prefix>/KEY`.
    pub async fn cmd_import(&mut self, source: &str, prefix: &str) -> anyhow::Result<()> {
        let contents = std::fs::read_to_string(source)?;
        for line in contents.lines() {
            if let Some((key, value)) = parse_env_line(line) {
                let path = secret_path(prefix, &key);
                self.cmd_set(&path, Zeroizing::new(value.into_bytes()))
                    .await?;
            }
        }
        Ok(())
    }

    /// List the caller's secrets under `prefix`, decrypt each, and print `KEY=value`.
    pub async fn cmd_export(&mut self, prefix: &str) -> anyhow::Result<()> {
        for path in self.list_paths(prefix).await? {
            let key = path.rsplit('/').next().unwrap_or(&path).to_string();
            let plaintext = self.fetch_plaintext(&path).await?;
            let value = String::from_utf8_lossy(&plaintext);
            println!("{key}={value}");
        }
        Ok(())
    }

    /// The caller's secret paths under `prefix` (owner-scoped by the server).
    async fn list_paths(&mut self, prefix: &str) -> anyhow::Result<Vec<String>> {
        let mut request = Request::new(LsRequest {
            prefix: prefix.to_string(),
        });
        self.authorize(&mut request, "/vault.v1.Vault/Ls")?;
        let secrets = self.client.ls(request).await?.into_inner().secrets;
        Ok(secrets.into_iter().map(|secret| secret.path).collect())
    }
}
