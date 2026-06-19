//! `42ctl auth` — authenticate against the platform. `login` unlocks the local identity,
//! registers its PUBLIC author key with the profile's contract authority, and saves the
//! returned contract per profile. `whoami` prints the local principal + address (and
//! whether a contract is bound), `status` reports whether this profile is logged in, and
//! `logout` clears the saved contract. The private key never leaves the machine.

use crate::adapters::{address, authority, creds, passphrase};
use crate::cli::Auth;
use crate::profile::Config;

/// Dispatch an `auth` subcommand for `profile`.
pub async fn run(cmd: &Auth, profile: &str) -> anyhow::Result<()> {
    match cmd {
        Auth::Login { tenant, token } => login(profile, tenant, token.as_deref()).await,
        Auth::Whoami => whoami(profile),
        Auth::Status => status(profile),
        Auth::Logout => logout(profile),
    }
}

/// Register this identity with the profile's authority and save the issued contract.
async fn login(profile: &str, tenant: &str, token: Option<&str>) -> anyhow::Result<()> {
    let endpoint = Config::load()?.endpoint(profile)?;
    let identity = passphrase::unlock()?;
    let author_pubkey = hex::encode(identity.author_public().to_bytes());
    let contract = authority::register(&endpoint.authority, &author_pubkey, tenant, token).await?;
    creds::save(profile, &contract)?;
    println!("logged in to tenant '{tenant}' on profile '{profile}'");
    println!(
        "contract saved to {}",
        creds::contract_path(profile).display()
    );
    Ok(())
}

/// Print this identity's principal, address, and whether a contract is bound.
fn whoami(profile: &str) -> anyhow::Result<()> {
    let identity = passphrase::unlock()?;
    let principal = hex::encode(vault42_core::fingerprint(
        &identity.author_public().to_bytes(),
    ));
    println!("principal: {principal}");
    println!("address:   {}", address::encode(&identity));
    match creds::load(profile) {
        Some(_) => println!("contract:  bound (profile '{profile}')"),
        None => println!("contract:  none (run `42ctl auth login`)"),
    }
    Ok(())
}

/// Report whether `profile` has a saved contract.
fn status(profile: &str) -> anyhow::Result<()> {
    match creds::load(profile) {
        Some(_) => println!("profile '{profile}': logged in (contract present)"),
        None => println!("profile '{profile}': logged out (no contract)"),
    }
    Ok(())
}

/// Clear the saved contract for `profile`.
fn logout(profile: &str) -> anyhow::Result<()> {
    creds::clear(profile)?;
    println!("logged out of profile '{profile}'");
    Ok(())
}
