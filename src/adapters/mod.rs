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
//! and the shareable address codec. Network + zero-knowledge envelope adapters (api,
//! compose, decrypt) land with P2/P3. The core/command layers depend on these; these
//! depend on `vault-crypto` (vault42-core), never the reverse.

pub mod address;
pub mod keystore;
pub mod passphrase;
