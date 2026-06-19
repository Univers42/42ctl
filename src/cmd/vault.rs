//! `42ctl vault` (alias `secrets`) — zero-knowledge secret operations. Every plaintext
//! seal/open happens locally via vault-crypto; only opaque envelopes cross the wire. This
//! handler resolves the profile endpoint, unlocks the local identity, loads the profile's
//! contract, opens a signed gRPC session, and dispatches to the matching `ops` verb.

use crate::adapters::api::Session;
use crate::adapters::{creds, passphrase};
use crate::cli::Vault;
use crate::profile::Config;
use zeroize::Zeroizing;

/// Dispatch a `vault` subcommand for `profile`.
pub async fn run(cmd: &Vault, profile: &str) -> anyhow::Result<()> {
    let mut session = open_session(profile).await?;
    dispatch(&mut session, cmd).await
}

/// Open a signed session: resolve the endpoint, unlock the identity, load the contract.
async fn open_session(profile: &str) -> anyhow::Result<Session> {
    let endpoint = Config::load()?.endpoint(profile)?;
    let identity = passphrase::unlock()?;
    let contract = creds::load(profile);
    Session::connect(&endpoint.server, identity, contract).await
}

/// Route the unlocked session to the requested verb.
async fn dispatch(session: &mut Session, cmd: &Vault) -> anyhow::Result<()> {
    match cmd {
        Vault::Get { path, version } => session.cmd_get(path, *version).await,
        Vault::Set { path, file } => session.cmd_set(path, read_input(file.as_deref())?).await,
        Vault::Ls { prefix } => session.cmd_ls(prefix).await,
        Vault::Rm { path } => session.cmd_rm(path).await,
        Vault::Rotate { path } => session.cmd_rotate(path).await,
        Vault::Share { path, to } => session.cmd_share(path, to).await,
        Vault::Audit { since } => session.cmd_audit(*since).await,
        Vault::Import { source } => session.cmd_import(source, "").await,
        Vault::Export { prefix } => session.cmd_export(prefix).await,
    }
}

/// Read secret input from a file or stdin into a zeroizing buffer.
fn read_input(file: Option<&str>) -> anyhow::Result<Zeroizing<Vec<u8>>> {
    match file {
        Some(path) => Ok(Zeroizing::new(std::fs::read(path)?)),
        None => {
            use std::io::Read;
            let mut buf = Vec::new();
            std::io::stdin().read_to_end(&mut buf)?;
            Ok(Zeroizing::new(buf))
        }
    }
}
