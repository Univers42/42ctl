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
            password,
        } => {
            if *github {
                github_login(profile).await
            } else if *password {
                let email = email
                    .as_deref()
                    .context("`--password` needs `--email` to say which account")?;
                password_login(profile, email, tenant.as_deref(), token.as_deref()).await
            } else {
                let tenant = tenant
                    .as_deref()
                    .context("`--tenant` is required (or use `--github`)")?;
                login(profile, tenant, token.as_deref()).await
            }
        }
        Auth::Signup { email, token } => signup(profile, email, token.as_deref()).await,
        Auth::Passwd => passwd(profile).await,
        Auth::Mfa { on, .. } => mfa(profile, *on).await,
        Auth::Me => me(profile).await,
        Auth::Whoami => whoami(profile),
        Auth::Status => status(profile),
        Auth::Logout => logout(profile),
    }
}

/// Sign in with an email and password, save the session, and register a contract if asked.
///
/// This is the second door to a session, and for most deployments it is the only one. The
/// other is the GitHub device flow, which needs a GitHub app configured on the authority —
/// without one, every organisation, team, project and grant verb is unreachable, because they
/// all authenticate with a session rather than a contract. A feature nobody can reach is not
/// a feature, however well it is tested.
async fn password_login(
    profile: &str,
    email: &str,
    tenant: Option<&str>,
    token: Option<&str>,
) -> anyhow::Result<()> {
    let base = Config::load()?.endpoint(profile)?.otp_base().to_string();
    let secret = passphrase::prompt_secret("password")?;
    let minted = account::login(&base, email, &secret).await?;
    session::save(profile, &minted.token)?;
    ui::field("account", &minted.account_id);
    ui::success(&format!("signed in as {email}"));
    match tenant {
        Some(tenant) => login(profile, tenant, token).await,
        None => Ok(()),
    }
}

/// Create an account on the authority. The password is prompted twice and never echoed.
///
/// Signup does not log in: the account exists afterwards and the caller still has to log in
/// for a session, so a failed signup never leaves a half-authenticated profile behind.
///
/// The success message is deliberately non-committal about whether the address was already
/// registered, and the account id is printed only if the authority chose to return one.
/// Saying "registered" for a fresh address and something else for a taken one is an
/// enumeration oracle that needs no password: an attacker learns who has an account by
/// trying to register them.
async fn signup(profile: &str, email: &str, token: Option<&str>) -> anyhow::Result<()> {
    let base = Config::load()?.endpoint(profile)?.otp_base().to_string();
    let password = passphrase::prompt_new_secret("password")?;
    let created = account::signup(&base, email, &password, token).await?;
    if let Some(id) = &created.account_id {
        ui::field("account", id);
    }
    ui::success(&format!(
        "if {email} was not already registered, it is now — log in to obtain a session"
    ));
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

/// Turn the account's email second factor on or off, proving mailbox possession first.
///
/// The address comes from the authority rather than a flag: the proof is verified against the
/// CALLER's own address, so letting one be typed here would only produce a refusal the operator
/// then has to diagnose.
async fn mfa(profile: &str, on: bool) -> anyhow::Result<()> {
    let endpoint = Config::load()?.endpoint(profile)?;
    let (base, token) = session::connect(profile)?;
    let who = account::me(&base, &token).await?;
    let proof = otp::email_otp(endpoint.otp_base(), &who.email).await?;
    account::set_mfa(&base, &token, on, &proof).await?;
    ui::success(&format!(
        "second factor {} for {}",
        if on { "REQUIRED" } else { "off" },
        who.email
    ));
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

/// Log in through the GitHub device flow and save the session the authority mints. No local
/// identity is needed — this is an account session (org/RBAC), not a vault42 contract, and
/// GitHub only vouches for an address: the account must already exist.
async fn github_login(profile: &str) -> anyhow::Result<()> {
    let endpoint = Config::load()?.endpoint(profile)?;
    let token = github_device::device_login(endpoint.otp_base()).await?;
    session::save(profile, &token)?;
    ui::success(&format!("logged in via GitHub on profile '{profile}'"));
    Ok(())
}

/// Register this identity with the profile's authority and save the issued contract.
///
/// The SESSION is what authenticates this: the authority issues a contract to an account, so
/// the saved session token is sent as a bearer and a caller without one is refused before
/// anything is signed.
///
/// There is deliberately no email-OTP step here any more. It used to run one and hand the
/// resulting proof to `/v1/register`, whose request type has no field for it — serde dropped
/// it, so the check bound nothing and anybody calling the route directly skipped it. A second
/// factor that only the honest operator performs is theatre; the real one lives in
/// `mint_session`, which is the only place a sign-in mints a session, and this step now
/// inherits it by carrying that session.
async fn login(profile: &str, tenant: &str, token: Option<&str>) -> anyhow::Result<()> {
    let endpoint = Config::load()?.endpoint(profile)?;
    let identity = passphrase::unlock()?;
    let author_pubkey = hex::encode(identity.author_public().to_bytes());
    let session = session::load(profile).context(
        "no session for this profile — run `42ctl auth login --password --email <mail>` first, \
         because the authority issues a contract to an account",
    )?;
    let contract = authority::register(
        &endpoint.authority,
        &authority::RegisterSpec {
            author_pubkey_hex: &author_pubkey,
            tenant,
            session: Some(&session),
            token,
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

/// Report which of the two credentials `profile` holds.
///
/// There are two, for two planes: a SESSION for the organisation verbs and a CONTRACT for the
/// vault verbs. This used to look at the contract alone, so a password login — which saves a
/// session and no contract — reported "logged out" while every org, team and project verb
/// worked. The leading words stay `profile 'X': logged in` / `logged out`, so a script that
/// matched them still does; the rest says which half is missing and how to get it.
fn status(profile: &str) -> anyhow::Result<()> {
    let (logged_in, line) = status_line(
        profile,
        session::load(profile).is_some(),
        creds::load(profile).is_some(),
    );
    if logged_in {
        ui::success(&line);
    } else {
        println!("{}", ui::warn(&line));
    }
    Ok(())
}

/// Whether `profile` counts as logged in, and the line that says with what.
fn status_line(profile: &str, session: bool, contract: bool) -> (bool, String) {
    let head = format!("profile '{profile}'");
    match (session, contract) {
        (true, true) => (true, format!("{head}: logged in — session and contract")),
        (true, false) => (
            true,
            format!(
                "{head}: logged in — session only; a gated vault server also needs \
                 `auth login --tenant <name>`"
            ),
        ),
        (false, true) => (
            true,
            format!(
                "{head}: logged in — contract only; the organisation verbs need \
                 `auth login --password --email <mail>`"
            ),
        ),
        (false, false) => (false, format!("{head}: logged out")),
    }
}

/// Clear the saved contract AND grobase session token for `profile`.
fn logout(profile: &str) -> anyhow::Result<()> {
    creds::clear(profile)?;
    session::clear(profile)?;
    ui::success(&format!("logged out of profile '{profile}'"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::status_line;

    /// A password login saves a session and no contract, and is still logged in.
    #[test]
    fn a_session_alone_is_logged_in_and_names_the_missing_contract() {
        let (logged_in, line) = status_line("default", true, false);
        assert!(logged_in);
        assert!(line.starts_with("profile 'default': logged in"), "{line}");
        assert!(line.contains("--tenant"), "{line}");
    }

    /// Every combination keeps the leading words a script matches on.
    #[test]
    fn each_combination_reports_what_it_holds() {
        assert_eq!(
            status_line("p", true, true),
            (
                true,
                "profile 'p': logged in — session and contract".to_string()
            )
        );
        assert!(status_line("p", false, true).1.contains("contract only"));
        assert_eq!(
            status_line("p", false, false),
            (false, "profile 'p': logged out".to_string())
        );
    }
}
