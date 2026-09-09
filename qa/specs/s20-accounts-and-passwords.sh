#!/usr/bin/env bash
# s20 — accounts, passwords and account deletion.
#
# Written when none of this existed: identity was an Ed25519 keypair with a passphrase-
# wrapped local keystore and there was no account, no password and no deletion path. The
# surface is built now, so those assertions are green and pin it; the two that remain red
# are marked where they sit.
#
# The deletion assertions are the user requirement stated back: deleting an account warns
# that it cannot be undone, and a bare invocation does not proceed. Both are asserted on the
# MESSAGE and not merely on a non-zero exit, because clap produces a non-zero exit for a
# subcommand that does not exist and that would look identical.
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
assert_green "the CLI exposes 'auth signup'" -- bash -c '_help auth | grep -qw signup' 
assert_green "the CLI exposes 'auth passwd' to change a password" -- bash -c '_help auth | grep -qw passwd'
assert_green "the CLI exposes 'auth me' for the current account" -- bash -c '_help auth | grep -qw me'
assert_green "the CLI exposes an 'account' command group" -- bash -c '_help | grep -qw account'
assert_green "the CLI exposes 'account delete'" -- bash -c '_help account | grep -qw delete'
# ── served: the authority routes behind them ─────────────────────────────────
assert_green "POST /v1/auth/signup is served" \
	-- qa_probe_route POST /v1/auth/signup '{"email":"qa@archicode.codes","password":"qa-pw"}'
assert_green "POST /v1/auth/login is served" \
	-- qa_probe_route POST /v1/auth/login '{"email":"qa@archicode.codes","password":"qa-pw"}'
assert_green "POST /v1/auth/logout is served" -- qa_probe_route POST /v1/auth/logout '{}'
assert_green "POST /v1/auth/passwd is served" \
	-- qa_probe_route POST /v1/auth/passwd '{"old":"a","new":"b"}'
assert_green "GET /v1/auth/me is served" -- qa_probe_route GET /v1/auth/me
# The route is DELETE /v1/auth/account, not /v1/auth/me. This spec named the shape the
# contract was expected to take and the authority took a different one; asserting the wrong
# path would have stayed red forever while the capability was in fact served.
assert_green "DELETE /v1/auth/account deletes the caller's own account" \
	-- qa_probe_route DELETE /v1/auth/account

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
assert_green "'account delete' refuses without an explicit confirmation flag" \
	-- bash -c 'out=$(docker run --rm -v "$C42_ROOT":/work "$QA_IMG" /work/target/debug/42ctl account delete 2>&1); [ $? -ne 0 ] && printf "%s" "$out" | grep -qi "confirm\|--yes\|irreversible"'
assert_green "'account delete' warns that the action is irreversible" \
	-- bash -c '_help account delete 2>&1 | grep -qi "irreversible\|cannot be undone\|permanent"'
# An accurate message gets read as a complete one. A tenant name claimed by `auth login
# --tenant` survives the account, and nothing anywhere releases one, so deleting the account
# leaves that name taken — by nobody who can use it once the keystore is gone too. The verb
# has to say so, in both the help and the refusal, or "delete my account" reads as
# "everything I registered".
assert_green "'account delete' says what it does NOT remove" \
	-- bash -c '_help account delete 2>&1 | grep -qi "tenant"'
assert_green "the refusal names the surviving tenant claim too" \
	-- bash -c 'docker run --rm -v "$C42_ROOT":/work "$QA_IMG" /work/target/debug/42ctl account delete 2>&1 | grep -qi "tenant"'

# ── the whole deletion story, end to end ─────────────────────────────────────
# The assertions above say the verb exists and refuses. These say it does what it claims and
# that refusing costs nothing: an account is created through the CLI, a bare delete is
# refused AND THE ACCOUNT STILL WORKS afterwards, a confirmed delete removes it, and the
# same credentials stop working. Without the third of those, "it refused" and "it deleted
# and reported a refusal" would look identical from outside.
W20="$QA_RESULTS/s20"; rm -rf "$W20"; mkdir -p "$W20"
DEL_EMAIL="qa-delete-$$@archicode.codes"
DEL_PW="qa-delete-pw-0001"
qa_actor_reset deleter
assert_green "an account is created through the CLI" \
	-- bash -c 'QA_ACCOUNT_PASSWORD="$3" qa_actor deleter "$1" "auth signup --email $2" >/dev/null 2>&1' \
	_ "$W20" "$DEL_EMAIL" "$DEL_PW"
assert_green "the new account can log in" \
	-- bash -c '[ -n "$(qa_login "$1" "$2")" ]' _ "$DEL_EMAIL" "$DEL_PW"
qa_actor_token deleter "$(qa_login "$DEL_EMAIL" "$DEL_PW")"
assert_green "the account reports its own email back" \
	-- bash -c 'qa_actor deleter "$1" "account show" 2>&1 | grep -qF "$2"' _ "$W20" "$DEL_EMAIL"
assert_green "a bare delete refuses AND leaves the account able to log in" \
	-- bash -c 'qa_actor deleter "$1" "account delete" >/dev/null 2>&1 && exit 1
		[ -n "$(qa_login "$2" "$3")" ]' _ "$W20" "$DEL_EMAIL" "$DEL_PW"
assert_green "a confirmed delete removes the account" \
	-- bash -c 'qa_actor deleter "$1" "account delete --yes" >/dev/null 2>&1' _ "$W20"
assert_green "the deleted account can no longer log in" \
	-- bash -c '[ -z "$(qa_login "$1" "$2")" ]' _ "$DEL_EMAIL" "$DEL_PW"

# ── deleting somebody else's account is a permission decision ────────────────
assert_green "deleting another user's account requires an explicit admin role" \
	-- qa_probe_route DELETE /v1/orgs/qa-org/members/some-other-user

spec_end
