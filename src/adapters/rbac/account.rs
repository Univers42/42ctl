/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   account.rs                                           :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Account routes on the authority: signup, password change, self-inspection, and the one
//! irreversible verb in the product.
//!
//! Every password here lives in a `Zeroizing` buffer and is sent to `/v1/auth/*` and
//! nowhere else. Deletion is a plain DELETE with the caller's own session as the only
//! argument: there is no account id to pass, so there is no way to spell somebody else's.

use crate::adapters::rbac;
use serde::{Deserialize, Serialize};
use serde_json::json;
use zeroize::Zeroizing;

/// What a signup returns.
///
/// `account_id` is optional because an authority that does not reveal whether an address is
/// already registered cannot return one: answering with an id for a fresh address and
/// something else for a taken one IS the disclosure. The id is available from
/// `GET /v1/auth/me` after logging in, where returning it leaks nothing.
#[derive(Deserialize)]
pub struct Created {
    #[serde(default)]
    pub account_id: Option<String>,
}

/// The caller's own account, as the authority reports it.
#[derive(Deserialize)]
pub struct Account {
    pub account_id: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub mfa_required: bool,
}

/// The password-change body, kept as a type so neither field can be passed in the other's
/// place — two same-typed arguments at one call site is how a new password becomes the old.
#[derive(Serialize)]
struct PasswdReq {
    current_password: String,
    new_password: String,
}

/// Register an account (`POST /v1/auth/signup`). 409 when the email is already taken.
pub async fn signup(
    base: &str,
    email: &str,
    password: &Zeroizing<String>,
    token: Option<&str>,
) -> anyhow::Result<Created> {
    let mut body = json!({ "email": email, "password": password.as_str() });
    if let Some(invite) = token {
        body["token"] = json!(invite);
    }
    rbac::post_public(base, "/v1/auth/signup", &body).await
}

/// A minted session: the bearer and who it belongs to.
#[derive(Deserialize)]
pub struct Session {
    pub token: String,
    pub account_id: String,
}

/// Exchange an email and password for a session (`POST /v1/auth/login`).
pub async fn login(
    base: &str,
    email: &str,
    password: &Zeroizing<String>,
) -> anyhow::Result<Session> {
    let body = json!({ "email": email, "password": password.as_str() });
    rbac::post_public(base, "/v1/auth/login", &body).await
}

/// Change the caller's password (`POST /v1/auth/passwd`), revoking every session.
pub async fn passwd(
    base: &str,
    token: &str,
    current: &Zeroizing<String>,
    fresh: &Zeroizing<String>,
) -> anyhow::Result<()> {
    let body = PasswdReq {
        current_password: current.to_string(),
        new_password: fresh.to_string(),
    };
    rbac::post_unit(base, token, "/v1/auth/passwd", &body).await
}

/// Report the caller's own account (`GET /v1/auth/me`).
pub async fn me(base: &str, token: &str) -> anyhow::Result<Account> {
    rbac::get_json(base, token, "/v1/auth/me").await
}

/// Erase the caller's own account (`DELETE /v1/auth/account`). There is no undo.
pub async fn delete(base: &str, token: &str) -> anyhow::Result<()> {
    rbac::delete_unit(base, token, "/v1/auth/account").await
}

/// Turn this account's email second factor on or off (`POST /v1/auth/mfa`).
///
/// `proof` is a live one-time-code proof bound to the account's own address. Requiring it in
/// BOTH directions is the point: enabling without proving you hold the mailbox would let a
/// stolen session lock the owner out of their own account, and disabling without it would make
/// the second factor removable by exactly the attacker it exists to stop.
pub async fn set_mfa(base: &str, token: &str, required: bool, proof: &str) -> anyhow::Result<()> {
    let body = json!({ "required": required, "proof": proof });
    rbac::post_unit(base, token, "/v1/auth/mfa", &body).await
}
