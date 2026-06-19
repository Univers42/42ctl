/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   mod.rs                                               :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! Vault verb operations — `impl Session` methods grouped by concern: `secret`
//! (set/get/rotate/fetch), `manage` (ls/rm), `share`, `audit`, and `io` (import/export of
//! `.env` files). Every plaintext seal/open happens locally via the envelope adapters;
//! only opaque bytes cross the wire, and each request is signed + contract-bound by
//! `Session::authorize`. The command layer wires these; the methods own the logic.

mod audit;
mod io;
mod manage;
mod secret;
mod share;
