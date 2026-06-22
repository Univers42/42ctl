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

//! Generalized invite verbs (team/group/project): accept an invite by its one-time token,
//! and show an invite by id. Both authenticate with the grobase session JWT.

use crate::adapters::rbac::{self, Invite};
use serde_json::json;

/// Accept an invite by its one-time `invite_token` (`POST /v1/invites/accept`).
pub async fn accept(grobase: &str, token: &str, invite_token: &str) -> anyhow::Result<()> {
    let body = json!({ "token": invite_token });
    rbac::post_unit(grobase, token, "/v1/invites/accept", &body).await
}

/// Show the invite `id` (`GET /v1/invites/{id}`).
pub async fn show(grobase: &str, token: &str, id: &str) -> anyhow::Result<Invite> {
    let path = format!("/v1/invites/{id}");
    rbac::get_json(grobase, token, &path).await
}
