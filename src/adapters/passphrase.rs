/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   passphrase.rs                                        :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Passphrase prompting + keystore unlock. The passphrase is read without echo (or from
//! `$FT_PASSPHRASE` for automation) and held in a `Zeroizing` buffer; `unlock` derives the
//! key locally and reconstructs the identity — the passphrase and unwrapped keys never
//! leave the process and never touch the network.

use crate::adapters::keystore;
use vault42_core::{open_keystore, Identity};
use zeroize::Zeroizing;

/// Prompt once for an existing passphrase (`$FT_PASSPHRASE` supplies it non-interactively).
pub fn prompt_passphrase() -> anyhow::Result<Zeroizing<String>> {
    if let Ok(passphrase) = std::env::var("FT_PASSPHRASE") {
        return Ok(Zeroizing::new(passphrase));
    }
    Ok(Zeroizing::new(rpassword::prompt_password("passphrase: ")?))
}

/// Prompt once for a secret that is not the keystore passphrase, labelled by `what`.
///
/// A separate environment variable from `FT_PASSPHRASE` on purpose: an account password and
/// a keystore passphrase are different secrets, and one variable serving both would silently
/// make them the same in every automated run.
pub fn prompt_secret(what: &str) -> anyhow::Result<Zeroizing<String>> {
    if let Ok(secret) = std::env::var("FT_PASSWORD") {
        return Ok(Zeroizing::new(secret));
    }
    Ok(Zeroizing::new(rpassword::prompt_password(format!(
        "{what}: "
    ))?))
}

/// Prompt twice for a new secret labelled by `what` and require the two to match.
pub fn prompt_new_secret(what: &str) -> anyhow::Result<Zeroizing<String>> {
    if let Ok(secret) = std::env::var("FT_PASSWORD") {
        return Ok(Zeroizing::new(secret));
    }
    let first = rpassword::prompt_password(format!("{what}: "))?;
    let second = rpassword::prompt_password(format!("confirm {what}: "))?;
    if first != second {
        anyhow::bail!("the two entries do not match");
    }
    Ok(Zeroizing::new(first))
}

/// Prompt twice for a new passphrase and require a match (`$FT_PASSPHRASE` bypasses).
pub fn prompt_new_passphrase() -> anyhow::Result<Zeroizing<String>> {
    if let Ok(passphrase) = std::env::var("FT_PASSPHRASE") {
        return Ok(Zeroizing::new(passphrase));
    }
    let first = rpassword::prompt_password("new passphrase: ")?;
    let second = rpassword::prompt_password("confirm passphrase: ")?;
    if first != second {
        anyhow::bail!("passphrases do not match");
    }
    Ok(Zeroizing::new(first))
}

/// Load the keystore and unlock it into an in-memory identity.
pub fn unlock() -> anyhow::Result<Identity> {
    let path = keystore::keystore_path()?;
    if !path.exists() {
        anyhow::bail!(
            "no keystore at {} — run `42ctl keys init` first",
            path.display()
        );
    }
    let blob = keystore::load(&path)?;
    let passphrase = prompt_passphrase()?;
    open_keystore(&blob, passphrase.as_bytes())
        .map_err(|_| anyhow::anyhow!("could not unlock keystore"))
}
