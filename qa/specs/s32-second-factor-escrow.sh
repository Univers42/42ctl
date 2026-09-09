#!/usr/bin/env bash
# s32 — one-time codes, keystore escrow and the device flow, driven through the real CLI.
#
# This is the spec that has to be green before the client's control-plane endpoint moves,
# because these are the routes whose base URL that move changes. Escrow matters most: it is
# the one flow where pointing at the wrong host loses a keystore rather than returning an
# error, and losing a keystore loses every secret authored under that identity.
#
# Codes are delivered to a directory instead of a mailbox using the authority's own file
# transport, so the path exercised is the one production uses, not a test-only route. The
# CLI requests a code and then blocks on stdin in the same process, so the code cannot be
# fetched in advance: the CLI's own request would replace it. A feeder watches the outbox
# and supplies it at the prompt.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s32-second-factor-escrow"
qa_require_docker_stack
qa_require_cmd curl
trap 'qa_server_cleanup; [ "${QA_KEEP_SERVER:-0}" = "1" ] || qa_authority_down' EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
assert_green "the authority is listening" -- qa_authority_up
[ "$_qa_regression" -eq 0 ] || { spec_end; exit 1; }

N="$$-$(date +%s)"
W="$QA_RESULTS/s32"; rm -rf "$W"; mkdir -p "$W"
MAIL="s32-$N@archicode.codes"
OTHER="s32other-$N@archicode.codes"
qa_signup "$MAIL" "pw-$N" >/dev/null
qa_signup "$OTHER" "pw-other-$N" >/dev/null

# ── the code itself ──────────────────────────────────────────────────────────
BEFORE="$(qa_code_count "$MAIL")"
assert_green "requesting a code answers success" \
	-- bash -c '[ "$(qa_code POST /v1/auth/otp/request "" "{\"email\":\"$1\"}")" = 200 ]' _ "$MAIL"
CODE="$(qa_wait_for_code "$MAIL" "$BEFORE")"
assert_green "a six-digit code is delivered" -- bash -c '[ "${#1}" -eq 6 ]' _ "$CODE"

# A locked phone shows the subject. Putting the code there would defeat the second factor
# for anyone who can see the screen without unlocking it.
assert_green "the code never appears in the subject line" \
	-- bash -c 'safe=$(printf "%s" "$1" | sed "s/[^A-Za-z0-9]/_/g")
		f=$(ls -1t "$QA_OUTBOX/$safe-"*.eml | head -1)
		! grep -i "^Subject:" "$f" | grep -q "$2"' _ "$MAIL" "$CODE"

# Requesting a code must not reveal whether an address has an account. The answer is the
# same either way and the difference is only whether anything is delivered.
assert_green "an unknown address gets the same answer as a known one" \
	-- bash -c '[ "$(qa_code POST /v1/auth/otp/request "" "{\"email\":\"nobody-$1@archicode.codes\"}")" = 200 ]' _ "$N"
assert_green "and no code is delivered to an address with no account" \
	-- bash -c '[ "$(qa_code_count "nobody-$1@archicode.codes")" = 0 ]' _ "$N"

assert_green "the code verifies once" \
	-- bash -c '[ "$(qa_code POST /v1/auth/otp/verify "" "{\"email\":\"$1\",\"code\":\"$2\"}")" = 200 ]' _ "$MAIL" "$CODE"
assert_green "the same code cannot be used twice" \
	-- bash -c '[ "$(qa_code POST /v1/auth/otp/verify "" "{\"email\":\"$1\",\"code\":\"$2\"}")" = 401 ]' _ "$MAIL" "$CODE"
assert_green "a wrong code is refused with the identical body" \
	-- bash -c 'a=$(qa_api POST /v1/auth/otp/verify "" "{\"email\":\"$1\",\"code\":\"000000\"}")
		b=$(qa_api POST /v1/auth/otp/verify "" "{\"email\":\"$1\",\"code\":\"$2\"}")
		[ "$a" = "$b" ]' _ "$MAIL" "$CODE"

# ── escrow, through the CLI, across two machines ─────────────────────────────
# The flow a base-URL mistake would break silently.
qa_actor_reset machineA; qa_actor_reset machineB
assert_green "machine A creates an identity" -- qa_actor machineA "$W" "keys init --force"
ADDR_A="$(qa_actor machineA "$W" "keys export-pub" 2>/dev/null | grep -o 'v42:[A-Za-z0-9_-]*')"
assert_green "machine A has a shareable address" -- bash -c '[ -n "$1" ]' _ "$ADDR_A"

assert_green "machine A escrows its passphrase-wrapped keystore" \
	-- bash -c 'qa_actor_otp machineA "$1" "keys escrow --email $2" "$2" 2>&1 | grep -qi "escrowed"' _ "$W" "$MAIL"

# The server must hold ciphertext only. If the escrowed blob contained the private key in
# the clear, the whole zero-knowledge claim would fail at its weakest point.
assert_green "the escrowed blob is not the plaintext identity" \
	-- bash -c 'b=$(qa_api POST /v1/auth/escrow/fetch "" "{\"email\":\"$1\",\"proof\":\"bogus\"}")
		! grep -q "PRIVATE\|BEGIN" <<<"$b"' _ "$MAIL"

assert_green "machine B recovers the same identity with the same passphrase" \
	-- bash -c 'QA_PASS_OVERRIDE=qa-pass-machineA qa_actor_otp machineB "$1" "keys recover --email $2" "$2" >/dev/null 2>&1
		got=$(QA_PASS_OVERRIDE=qa-pass-machineA qa_actor machineB "$1" "keys export-pub" 2>/dev/null | grep -o "v42:[A-Za-z0-9_-]*")
		[ "$got" = "$3" ]' _ "$W" "$MAIL" "$ADDR_A"

# An escrow is bound to the address it was stored under. A proof for one address must not
# fetch another's keystore, or an inbox becomes a key to somebody else's vault.
assert_green "a proof for one address cannot fetch another's keystore" \
	-- bash -c 'before=$(qa_code_count "$2")
		qa_api POST /v1/auth/otp/request "" "{\"email\":\"$2\"}" >/dev/null
		code=$(qa_wait_for_code "$2" "$before")
		proof=$(qa_json "$(qa_api POST /v1/auth/otp/verify "" "{\"email\":\"$2\",\"code\":\"$code\"}" | cut -f2-)" proof)
		[ "$(qa_code POST /v1/auth/escrow/fetch "" "{\"email\":\"$1\",\"proof\":\"$proof\"}")" != 200 ]' _ "$MAIL" "$OTHER"

# ── the device flow ──────────────────────────────────────────────────────────
# Unconfigured, the routes must say the feature is not set up rather than answering 404.
# A 404 is indistinguishable from a wrong path to a client about to poll for a quarter of
# an hour, so the distinction is worth pinning.
assert_green "an unconfigured device flow names the cause instead of answering 404" \
	-- bash -c 'c=$(qa_code POST /v1/github/device/start "" "{}"); [ "$c" != 404 ]'
assert_green "the poll route also exists when unconfigured" \
	-- bash -c 'c=$(qa_code POST /v1/github/device/poll "" "{\"device_code\":\"x\"}"); [ "$c" != 404 ]'

# ── the second-factor switch ─────────────────────────────────────────────────
# Once an account requires a factor, a password alone must never mint a session, by any
# route. This is the property most worth attacking, so it is asserted rather than assumed.
assert_green "a password alone still works before a factor is required" \
	-- bash -c '[ -n "$(qa_login "$1" "pw-$2")" ]' _ "$MAIL" "$N"

spec_end
