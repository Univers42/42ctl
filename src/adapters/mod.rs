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

//! Adapters — the thin I/O edge of the hexagon: the local keystore, passphrase prompting,
//! the shareable address codec, the signed gRPC session (api), the contract authority
//! client (authority), the per-profile contract store (creds), and the zero-knowledge
//! envelope codecs (compose/decrypt/derive). The core/command layers depend on these;
//! these depend on `vault-crypto` (vault42-core), never the reverse.

pub mod address;
pub mod api;
pub mod authority;
pub mod compose;
pub mod creds;
pub mod decrypt;
pub mod derive;
pub mod escrow;
pub mod github_device;
pub mod github_org;
pub mod keystore;
pub mod otp;
pub mod passphrase;
pub mod rbac;
pub mod session;
