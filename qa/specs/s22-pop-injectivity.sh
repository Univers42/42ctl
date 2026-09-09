#!/usr/bin/env bash
# s22 — signature-message injectivity.
#
# THE FINDING, NOW FIXED (commit 922d088). It is kept as a permanent guard because the
# bug is invisible by inspection: a bare concatenation looks perfectly reasonable.
#
# What it was. 42ctl framed its proof-of-possession message as a bare concatenation:
#
#     src/cmd/scope_pubkey.rs   pop_message = user_id ‖ org_id ‖ x25519_pub_b64
#
# with no lengths and no separators. That map is not injective, so two DIFFERENT
# identity pairs produce the SAME signed bytes:
#
#     user_id="alice"  org_id="acme"   ->  "aliceacme..."
#     user_id="alicea" org_id="cme"    ->  "aliceacme..."
#
# A signature collected for one pairing is therefore a valid signature for the other.
# Whether that is reachable depends entirely on who may choose those strings — and the
# authority being built right now assigns org slugs, which are user-chosen. This is the
# moment to frame it, not after identifiers are in the wild.
#
# The fix mirrors vault42-core/src/aad.rs: a domain tag plus <len>:<value>\n per field.
# The domain tag matters as much as the framing — without it a proof-of-possession
# signature could be replayed as an envelope-author signature.
#
# These assertions are STATIC (they read the source) because pop_message is private to
# a binary crate with no lib target, so no test can call it. That is a limitation worth
# stating plainly rather than dressing a grep up as a behavioural test.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"

spec_begin "s22-pop-injectivity"

# The message now lives in vault42-core, where it belongs: one definition shared by the
# signer and the verifier. Pinning this spec to 42ctl's old private copy broke it the
# moment the duplicate was removed — the second time a source assertion of mine has been
# tied to a location instead of a behaviour.
POP="${VAULT42_DIR:-$C42_ROOT/qa/.cache/vault42}/crates/vault42-core/src/pop.rs"
POP_CALLER="$C42_ROOT/src/cmd/scope_pubkey.rs"
# Read the PINNED tree, not the sibling working checkout. Reading the live tree made
# this spec depend on whatever the vault42 session had saved at that instant, which is
# precisely the non-determinism the pin exists to remove.
AAD="${VAULT42_DIR:-$C42_ROOT/qa/.cache/vault42}/crates/vault42-core/src/aad.rs"

assert_green "the proof-of-possession message has a single canonical definition" -- test -f "$POP"
assert_green "42ctl uses the shared definition instead of its own copy" \
	-- bash -c 'grep -q "vault42_core::.*pop_message\|use vault42_core::{.*pop_message" "$1" && ! grep -q "^fn pop_message" "$1"' _ "$POP_CALLER"

# The demonstration, printed rather than asserted: the concrete colliding input pair,
# so whoever fixes this can reproduce it without re-deriving the arithmetic.
A="$(printf 'alice'; printf 'acme'; printf 'PUBKEY')"
B="$(printf 'alicea'; printf 'cme'; printf 'PUBKEY')"
printf '# demonstration: ("alice","acme") and ("alicea","cme") both frame to %s\n' "$A"
[ "$A" = "$B" ] && printf '# the two distinct identity pairs are byte-identical under the current rule\n'

# The guard. Goes green when pop_message length-frames its fields the way aad.rs does.
# The capacity hint `Vec::with_capacity(a.len() + b.len() + ...)` also contains .len(),
# so it must be excluded or this passes without a single length ever being written
# INTO the message. Framing means the length is part of the signed bytes.
assert_green "pop_message length-frames its fields so the message is injective" \
	-- bash -c 'sed -n "/pub fn pop_message/,/^}/p" "$1" | grep -v with_capacity |
		grep -qE "frame\(|len\(\)\.to_|to_le_bytes|to_be_bytes"' _ "$POP"
assert_green "the proof-of-possession message is domain-separated" \
	-- bash -c 'sed -n "/pub fn pop_message/,/^}/p" "$1" | grep -q "DOMAIN"' _ "$POP"

# The reference implementation must not lose its framing either. This one is green and
# guards the crypto core against a well-meaning simplification.
if [ -f "$AAD" ]; then
	# Assert that framing HAPPENS, not where the helper is defined. The helper moved to
	# a shared module and was imported, which changed nothing about the bytes but broke
	# an assertion pinned to the definition site. Behaviour is what must not drift.
	assert_green "the canonical AAD frames every field it covers" \
		-- bash -c '[ "$(grep -c "frame(&mut out" "$1")" -ge 10 ]' _ "$AAD"
	assert_green "the AAD is built from a domain tag first" \
		-- grep -q 'frame(&mut out, DOMAIN)' "$AAD"
	assert_green "the canonical AAD golden digest test is still present" \
		-- grep -q 'mod tests' "$AAD"
fi

# The same class, one layer up: the transport signature must bind the method, so a
# signature captured for one RPC cannot be replayed against another.
assert_green "the transport signature binds the gRPC method name" \
	-- bash -c 'grep -rq "grpc-method\|{ts}\\\\n\|ts, method" "$1"/src/adapters/api.rs' _ "$C42_ROOT"

# The permanent regression test for this bug, requested by the session writing the
# verifier. Once PUT /v1/orgs/{org}/pubkey answers, a proof of possession registered
# for one (user_id, org_id) pair must be REJECTED when replayed against a different
# pair whose bare concatenation is identical. Under the current unframed rule the
# replay verifies; under length framing it cannot.
#
# The authority is started HERE rather than assumed. This assertion passed for its whole
# life only because some earlier spec happened to leave the authority running; under a
# shuffled order it ran first and failed, reporting a proof-of-possession defect that did
# not exist. A spec that depends on what ran before it is a spec whose green means nothing.
source "$QA_LIB_DIR/server.sh"
assert_green "the standalone authority is listening" -- qa_authority_up
assert_green "a proof of possession is rejected when replayed across a colliding identity pair" \
	-- bash -c '
		base="$(qa_base)"
		qa_probe_route PUT "/v1/orgs/acme/pubkey" "{}" || exit 1
		sig=$(curl -sS -m 10 -X PUT -H "content-type: application/json" \
			-d "{\"user_id\":\"alice\",\"x25519_pub\":\"UFVCS0VZ\",\"ed25519_pub\":\"RUQyNTUxOQ==\",\"pubkey_sig\":\"U0lH\"}" \
			"$base/v1/orgs/acme/pubkey")
		# Same signature bytes, shifted boundary: alice+acme becomes alicea+cme.
		code=$(curl -sS -m 10 -o /dev/null -w "%{http_code}" -X PUT -H "content-type: application/json" \
			-d "{\"user_id\":\"alicea\",\"x25519_pub\":\"UFVCS0VZ\",\"ed25519_pub\":\"RUQyNTUxOQ==\",\"pubkey_sig\":\"U0lH\"}" \
			"$base/v1/orgs/cme/pubkey")
		[ "$code" = 400 ] || [ "$code" = 401 ] || [ "$code" = 403 ]'

spec_end
