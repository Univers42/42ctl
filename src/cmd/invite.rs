/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   invite.rs                                           :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/21 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/21 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! `42ctl invite` — generalized invite operations (team/group/project): accept by its
//! one-time token, and show by id. Authenticates with the grobase session token from
//! `auth login --github`.

use crate::adapters::rbac::invite;
use crate::adapters::session;
use crate::cli::Invite;
use crate::ui;

/// Dispatch an `invite` subcommand for `profile`.
pub async fn run(cmd: &Invite, profile: &str) -> anyhow::Result<()> {
    let (grobase, token) = session::connect(profile)?;
    match cmd {
        Invite::Accept {
            token: invite_token,
        } => accept(&grobase, &token, invite_token).await,
        Invite::Show { id } => show(&grobase, &token, id).await,
    }
}

/// Accept an invite by its one-time token.
async fn accept(grobase: &str, token: &str, invite_token: &str) -> anyhow::Result<()> {
    invite::accept(grobase, token, invite_token).await?;
    ui::success("accepted invite");
    Ok(())
}

/// Show an invite's scope, email, role, status, and expiry.
async fn show(grobase: &str, token: &str, id: &str) -> anyhow::Result<()> {
    let inv = invite::show(grobase, token, id).await?;
    ui::field("id", &inv.id);
    ui::field("scope", &format!("{} {}", inv.scope_kind, inv.scope_id));
    ui::field("email", &inv.email);
    ui::field("role", &inv.role);
    ui::field("status", &inv.status);
    ui::field("expires", &inv.expires_at);
    Ok(())
}
