//! `42ctl db` — read owner-scoped encrypted records and decrypt them client-side. A
//! record is just a vault secret read through the same signed, contract-bound session:
//! the server owner-scopes the read (RBAC) and returns an opaque envelope; vault-crypto
//! decrypts it locally. `get` prints one record's plaintext; `ls` lists readable records.

use crate::adapters::api::Session;
use crate::adapters::{creds, passphrase};
use crate::cli::Db;
use crate::profile::Config;

/// Dispatch a `db` subcommand for `profile`.
pub async fn run(cmd: &Db, profile: &str) -> anyhow::Result<()> {
    let mut session = open_session(profile).await?;
    match cmd {
        Db::Get { path } => session.cmd_get(path, 0).await,
        Db::Ls { prefix } => session.cmd_ls(prefix).await,
    }
}

/// Open a signed session: resolve the endpoint, unlock the identity, load the contract.
async fn open_session(profile: &str) -> anyhow::Result<Session> {
    let endpoint = Config::load()?.endpoint(profile)?;
    let identity = passphrase::unlock()?;
    let contract = creds::load(profile);
    Session::connect(&endpoint.server, identity, contract).await
}
