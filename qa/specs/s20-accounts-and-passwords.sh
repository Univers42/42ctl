#!/usr/bin/env bash
# s20 — accounts, passwords and account deletion.
#
# None of this exists yet: identity today is an Ed25519 keypair with a passphrase-
# wrapped local keystore, and there is no account, no password, no server-side user
# record and no deletion path. Every assertion here is assert_spec, and together they
# are the executable specification for that subsystem.
#
# Two tiers per capability, because they land at different times:
#   surface  — the CLI verb exists and is discoverable in --help
#   served   — the authority answers the REST route behind it
#
# Route shapes follow the contract the vault42 authority is being built to.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s20-accounts-and-passwords"
qa_require_docker_stack
qa_require_cmd curl
trap 'qa_server_cleanup; [ "${QA_KEEP_SERVER:-0}" = "1" ] || qa_authority_down' EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
assert_green "the standalone authority is listening" -- qa_authority_up

_help() { docker run --rm -v "$C42_ROOT":/work "$QA_IMG" /work/target/debug/42ctl "$@" --help 2>&1; }
export -f _help
export QA_IMG C42_ROOT

# ── surface: the verbs a password-backed account needs ───────────────────────
assert_spec "the CLI exposes 'auth signup'" -- bash -c '_help auth | grep -qw signup' 
assert_spec "the CLI exposes 'auth passwd' to change a password" -- bash -c '_help auth | grep -qw passwd'
assert_spec "the CLI exposes 'auth me' for the current account" -- bash -c '_help auth | grep -qw me'
assert_spec "the CLI exposes an 'account' command group" -- bash -c '_help | grep -qw account'
assert_spec "the CLI exposes 'account delete'" -- bash -c '_help account | grep -qw delete'
# ── served: the authority routes behind them ─────────────────────────────────
assert_green "POST /v1/auth/signup is served" \
	-- qa_probe_route POST /v1/auth/signup '{"email":"qa@archicode.codes","password":"qa-pw"}'
assert_green "POST /v1/auth/login is served" \
	-- qa_probe_route POST /v1/auth/login '{"email":"qa@archicode.codes","password":"qa-pw"}'
assert_green "POST /v1/auth/logout is served" -- qa_probe_route POST /v1/auth/logout '{}'
assert_green "POST /v1/auth/passwd is served" \
	-- qa_probe_route POST /v1/auth/passwd '{"old":"a","new":"b"}'
assert_green "GET /v1/auth/me is served" -- qa_probe_route GET /v1/auth/me
assert_spec "DELETE /v1/auth/me deletes the caller's own account" \
	-- qa_probe_route DELETE /v1/auth/me

# ── the password must never be recoverable from what the server stores ───────
# A password-backed account is only as good as its storage. The spec is that the
# account row holds a verifier (argon2id), never the password and never something
# reversible, and that the wire never carries the password to any route but auth.
# Expressed against the route rather than the table, so it becomes a real check the
# moment /v1/auth/me answers instead of a placeholder that can never do anything.
assert_green "GET /v1/auth/me never echoes a password or a verifier" \
	-- bash -c 'qa_probe_route GET /v1/auth/me || exit 1
		body=$(curl -sS -m 10 "$(qa_base)/v1/auth/me" 2>/dev/null)
		! printf "%s" "$body" | grep -qi "password\|argon2\|\$2[aby]\$"'

# ── an absent account must be indistinguishable from a wrong password ────────
# If the two differ in status, body or timing, /v1/auth/login becomes an account
# enumeration oracle: an attacker learns which addresses are registered without ever
# guessing a password.
assert_green "an unknown email and a wrong password are indistinguishable at login" \
	-- bash -c '
		base="$(qa_base)"
		mk() { curl -sS -m 10 -w "\n%{http_code}" -X POST -H "content-type: application/json" \
			-d "$1" "$base/v1/auth/login"; }
		absent=$(mk "{\"email\":\"nobody-here@archicode.codes\",\"password\":\"whatever-pw\"}")
		wrong=$(mk "{\"email\":\"qa-known@archicode.codes\",\"password\":\"definitely-wrong\"}")
		[ "$absent" = "$wrong" ]'

# ── a small-order public key must be refused at registration ─────────────────
# VerifyingKey::from_bytes admits any value that decompresses to a curve point, but
# verify_strict — which every request path uses — rejects small-order keys. Such a key
# therefore registers successfully, consumes the tenant name, and then fails every
# signature check afterwards. That is a cheap denial of service on tenant names.
assert_green "POST /v1/register refuses an all-zero author public key" \
	-- bash -c '
		code=$(curl -sS -m 10 -o /dev/null -w "%{http_code}" -X POST \
			-H "content-type: application/json" \
			-d "{\"tenant\":\"qa-smallorder\",\"author_pubkey\":\"$(printf "0%.0s" $(seq 1 64))\"}" \
			"$(qa_base)/v1/register")
		[ "$code" = 400 ]'

# The control. Without it, a request-shape change would make the assertion above pass
# for a parse error instead of the key check, and it would look just as green.
assert_green "the same request with a valid Ed25519 key is accepted" \
	-- bash -c '
		code=$(curl -sS -m 10 -o /dev/null -w "%{http_code}" -X POST \
			-H "content-type: application/json" \
			-d "{\"tenant\":\"qa-control-$$\",\"author_pubkey\":\"d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a\"}" \
			"$(qa_base)/v1/register")
		[ "$code" = 200 ] || [ "$code" = 409 ]'

# ── deletion is irreversible and must say so ─────────────────────────────────
# The user requirement is explicit: deleting an account warns that the action cannot
# be undone, and does not proceed on a bare invocation.
# Must fail AND say why. A bare non-zero exit would also be produced by clap when the
# subcommand does not exist, which would make this pass for the wrong reason.
assert_spec "'account delete' refuses without an explicit confirmation flag" \
	-- bash -c 'out=$(docker run --rm -v "$C42_ROOT":/work "$QA_IMG" /work/target/debug/42ctl account delete 2>&1); [ $? -ne 0 ] && printf "%s" "$out" | grep -qi "confirm\|--yes\|irreversible"'
assert_spec "'account delete' warns that the action is irreversible" \
	-- bash -c '_help account delete 2>&1 | grep -qi "irreversible\|cannot be undone\|permanent"'

# ── deleting somebody else's account is a permission decision ────────────────
assert_green "deleting another user's account requires an explicit admin role" \
	-- qa_probe_route DELETE /v1/orgs/qa-org/members/some-other-user

spec_end
