/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   unseal.rs                                            :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl unseal` — refuses, because there is nothing to unseal yet.

/// Refuse, naming why.
///
/// vault42's `Unseal` RPC authenticates the caller and then always reports the vault fully
/// unsealed: the server has no seal state to open. This verb used to print a line naming that RPC
/// and exit 0, which reads exactly like an unseal that happened — the worst thing an operator
/// verb can say after a restart, since it invites them to stop checking. Until the server grows a
/// real seal, the honest answer is a refusal a script cannot mistake for success.
pub fn run() -> anyhow::Result<()> {
    anyhow::bail!(
        "unseal is not implemented — vault42 has no seal state yet, so a restarted server needs no \
         unsealing and this command does nothing"
    )
}

#[cfg(test)]
mod tests {
    /// An operator verb that does nothing must not exit 0.
    #[test]
    fn unseal_refuses_rather_than_reporting_success() {
        let error = super::run().expect_err("a stub must not succeed");
        assert!(error.to_string().contains("not implemented"), "{error}");
    }
}
