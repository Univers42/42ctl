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

//! `42ctl account` — the account itself, including the one verb that cannot be undone.
//!
//! Deletion asks the authority to erase the caller's own account. There is no account id in
//! the request, so there is no way to spell somebody else's; removing another person is an
//! org membership decision and lives under `org`, where the role check belongs.
//!
//! The confirmation is a flag rather than a typed prompt because this has to work in a
//! script, and a prompt a script cannot answer becomes a prompt somebody pipes `yes` into.

use crate::adapters::rbac::account;
use crate::adapters::session;
use crate::cli::Account;
use crate::ui;

/// Dispatch an `account` subcommand for `profile`.
pub async fn run(cmd: &Account, profile: &str) -> anyhow::Result<()> {
    match cmd {
        Account::Show => show(profile).await,
        Account::Delete { yes } => delete(profile, *yes).await,
    }
}

/// Print the calling account as the authority reports it.
async fn show(profile: &str) -> anyhow::Result<()> {
    let (base, token) = session::connect(profile)?;
    let me = account::me(&base, &token).await?;
    ui::field("account", &me.account_id);
    ui::field("email", &me.email);
    ui::field(
        "second factor",
        if me.mfa_required { "required" } else { "off" },
    );
    Ok(())
}

/// Erase the calling account, refusing until the caller has said so explicitly.
///
/// The refusal is checked BEFORE the session is resolved, so `account delete` with no
/// credentials still tells the operator what the verb does rather than failing with a login
/// error that says nothing about what they were about to destroy.
async fn delete(profile: &str, yes: bool) -> anyhow::Result<()> {
    confirmed(yes)?;
    let (base, token) = session::connect(profile)?;
    let me = account::me(&base, &token).await?;
    ui::field("deleting", &me.email);
    account::delete(&base, &token).await?;
    session::clear(profile)?;
    ui::success(&format!(
        "account {} is gone, along with every session it had",
        me.account_id
    ));
    println!(
        "{}",
        ui::warn("any tenant name this identity claimed is still claimed — nothing releases one")
    );
    Ok(())
}

/// Refuse an irreversible deletion that was not explicitly confirmed.
///
/// The message names both what is lost and the flag that proceeds, because a refusal an
/// operator cannot act on is a refusal they work around.
fn confirmed(yes: bool) -> anyhow::Result<()> {
    if yes {
        return Ok(());
    }
    anyhow::bail!(
        "this permanently deletes the account, its sessions and its org memberships — the \
         action is IRREVERSIBLE and nothing restores it. A tenant name claimed by `auth \
         login --tenant` is NOT removed and stays bound to the key that claimed it. Re-run \
         with --yes to confirm."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bare invocation must refuse, and the refusal must say both that the action cannot
    /// be undone and how to proceed. A bare non-zero exit would also be produced by a typo
    /// in the subcommand, which is why the message is asserted and not just the failure.
    #[test]
    fn a_bare_delete_refuses_and_says_why() {
        let err = confirmed(false).expect_err("a bare delete must refuse");
        let said = err.to_string().to_lowercase();
        assert!(said.contains("irreversible"), "{said}");
        assert!(said.contains("--yes"), "{said}");
    }

    /// A destructive verb has to say what it does NOT remove, or an accurate message gets
    /// read as a complete one. The tenant claim survives the account and nothing anywhere
    /// releases one, so "delete my account" leaves that name taken — by nobody who can use
    /// it, once the keystore is gone too.
    #[test]
    fn the_refusal_names_what_deletion_leaves_behind() {
        let said = confirmed(false)
            .expect_err("a bare delete must refuse")
            .to_string()
            .to_lowercase();
        assert!(said.contains("tenant"), "{said}");
        assert!(said.contains("not removed"), "{said}");
    }

    #[test]
    fn an_explicit_confirmation_proceeds() {
        confirmed(true).expect("--yes must proceed");
    }
}
