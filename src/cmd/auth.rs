//! `42ctl auth` — authenticate against the platform. `login` unlocks the local identity,
//! registers its PUBLIC author key with the profile's contract authority, and saves the
//! returned contract per profile. `whoami` prints the local principal + address (and
//! whether a contract is bound), `status` reports whether this profile is logged in, and
//! `logout` clears the saved contract. The private key never leaves the machine.

use crate::adapters::rbac::account;
use crate::adapters::{address, authority, creds, github_device, otp, passphrase, session};
use crate::cli::Auth;
use crate::profile::Config;
use crate::ui;
use anyhow::Context;

/// Dispatch an `auth` subcommand for `profile`.
pub async fn run(cmd: &Auth, profile: &str) -> anyhow::Result<()> {
    match cmd {
        Auth::Login {
            tenant,
            token,
            email,
            github,
        } => {
            if *github {
                github_login(profile).await
            } else {
                let tenant = tenant
                    .as_deref()
                    .context("`--tenant` is required (or use `--github`)")?;
                login(profile, tenant, token.as_deref(), email.as_deref()).await
            }
        }
        Auth::Signup { email } => signup(profile, email).await,
        Auth::Passwd => passwd(profile).await,
        Auth::Me => me(profile).await,
        Auth::Whoami => whoami(profile),
        Auth::Status => status(profile),
        Auth::Logout => logout(profile),
    }
}

/// Create an account on the authority. The password is prompted twice and never echoed.
///
/// Signup does not log in: the account exists afterwards and the caller still has to log in
/// for a session, so a failed signup never leaves a half-authenticated profile behind.
async fn signup(profile: &str, email: &str) -> anyhow::Result<()> {
    let base = Config::load()?.endpoint(profile)?.otp_base().to_string();
    let password = passphrase::prompt_new_secret("password")?;
    let created = account::signup(&base, email, &password).await?;
    ui::field("account", &created.account_id);
    ui::success(&format!("registered {email} — log in to obtain a session"));
    Ok(())
}

/// Change this account's password, which revokes every session including this one.
async fn passwd(profile: &str) -> anyhow::Result<()> {
    let (base, token) = session::connect(profile)?;
    let current = passphrase::prompt_secret("current password")?;
    let fresh = passphrase::prompt_new_secret("new password")?;
    account::passwd(&base, &token, &current, &fresh).await?;
    session::clear(profile)?;
    ui::success("password changed — every session was revoked, log in again");
    Ok(())
}

/// Show the account the saved session belongs to.
async fn me(profile: &str) -> anyhow::Result<()> {
    let (base, token) = session::connect(profile)?;
    let who = account::me(&base, &token).await?;
    ui::field("account", &who.account_id);
    ui::field("email", &who.email);
    ui::field(
        "second factor",
        if who.mfa_required { "required" } else { "off" },
    );
    Ok(())
}

/// Log in to grobase via the GitHub device flow and save the minted session token. No
/// local identity is needed — this is a grobase login (org/RBAC), not a vault42 contract.
async fn github_login(profile: &str) -> anyhow::Result<()> {
    let endpoint = Config::load()?.endpoint(profile)?;
    let token = github_device::device_login(endpoint.otp_base()).await?;
    session::save(profile, &token)?;
    ui::success(&format!(
        "logged in to grobase via GitHub on profile '{profile}'"
    ));
    Ok(())
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

/// Clear the saved contract AND grobase session token for `profile`.
fn logout(profile: &str) -> anyhow::Result<()> {
    creds::clear(profile)?;
    session::clear(profile)?;
    ui::success(&format!("logged out of profile '{profile}'"));
    Ok(())
}
