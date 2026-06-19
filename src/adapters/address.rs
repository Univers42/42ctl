/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   address.rs                                           :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The shareable identity address — `v42:<base64url(ed25519_pub ‖ x25519_pub)>`. It
//! carries only public keys: the Ed25519 key fixes a recipient's principal, the X25519
//! key is the wrap target for sharing. No private material is ever in an address.

use base64::Engine as _;
use vault42_core::Identity;

/// Encode an identity's public keys into a shareable address. (Decoding lands with
/// `vault share` in P3.)
pub fn encode(identity: &Identity) -> String {
    let mut buf = Vec::with_capacity(64);
    buf.extend_from_slice(&identity.author_public().to_bytes());
    buf.extend_from_slice(&identity.encryption_public().to_bytes());
    format!(
        "v42:{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
    )
}
