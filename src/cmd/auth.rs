//! `42ctl auth` — authenticate against the platform. `login` unlocks the local identity,
//! registers its PUBLIC author key with the profile's contract authority, and saves the
//! returned contract per profile. `whoami` prints the local principal + address (and
//! whether a contract is bound), `status` reports whether this profile is logged in, and
//! `logout` clears the saved contract. The private key never leaves the machine.

use crate::adapters::{address, authority, creds, otp, passphrase};
use crate::cli::Auth;
use crate::profile::Config;
use crate::ui;

/// Dispatch an `auth` subcommand for `profile`.
pub async fn run(cmd: &Auth, profile: &str) -> anyhow::Result<()> {
    match cmd {
        Auth::Login {
            tenant,
            token,
            email,
        } => login(profile, tenant, token.as_deref(), email.as_deref()).await,
        Auth::Whoami => whoami(profile),
        Auth::Status => status(profile),
        Auth::Logout => logout(profile),
    }
}

/// Register this identity with the profile's authority and save the issued contract.
/// When `email` is set, an email OTP (6-digit code) must pass FIRST — a Bitwarden-style
/// second factor: the authority mails the code and the terminal waits for it.
async fn login(
    profile: &str,
    tenant: &str,
    token: Option<&str>,
    email: Option<&str>,
) -> anyhow::Result<()> {
    let endpoint = Config::load()?.endpoint(profile)?;
    let identity = passphrase::unlock()?;
    let proof = match email {
        Some(addr) => {
            let p = otp::email_otp(endpoint.otp_base(), addr).await?;
            ui::success("email verification passed");
            Some(p)
        }
        None => None,
    };
    let author_pubkey = hex::encode(identity.author_public().to_bytes());
    let contract = authority::register(
        &endpoint.authority,
        &authority::RegisterSpec {
            author_pubkey_hex: &author_pubkey,
            tenant,
            token,
            email,
            otp_proof: proof.as_deref(),
        },
    )
    .await?;
    creds::save(profile, &contract)?;
    ui::success(&format!("logged in to '{tenant}' on profile '{profile}'"));
    println!(
        "{}",
        ui::dim(&format!(
            "contract → {}",
            creds::contract_path(profile).display()
        ))
    );
    Ok(())
}

/// Print this identity's principal, address, and whether a contract is bound.
fn whoami(profile: &str) -> anyhow::Result<()> {
    let identity = passphrase::unlock()?;
    let principal = hex::encode(vault42_core::fingerprint(
        &identity.author_public().to_bytes(),
    ));
    ui::field("principal", &principal);
    ui::field("address", &address::encode(&identity));
    match creds::load(profile) {
        Some(_) => ui::field("contract", &format!("bound (profile '{profile}')")),
        None => ui::field("contract", &ui::warn("none — run `42ctl auth login`")),
    }
    Ok(())
}

/// Report whether `profile` has a saved contract.
fn status(profile: &str) -> anyhow::Result<()> {
    match creds::load(profile) {
        Some(_) => ui::success(&format!("profile '{profile}': logged in")),
        None => println!("{}", ui::warn(&format!("profile '{profile}': logged out"))),
    }
    Ok(())
}

/// Clear the saved contract for `profile`.
fn logout(profile: &str) -> anyhow::Result<()> {
    creds::clear(profile)?;
    ui::success(&format!("logged out of profile '{profile}'"));
    Ok(())
}
