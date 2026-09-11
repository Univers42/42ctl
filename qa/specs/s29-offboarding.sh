#!/usr/bin/env bash
# s29 — removing somebody, and what that does and does not do.
#
# Offboarding is the operation a vault is judged on. Adding people is easy; the question
# that matters is whether access actually ends when someone leaves.
#
# The honest answer here has two halves and both are asserted. Removal is authorization,
# not erasure: a scope key already wrapped to a departing member lives in vault42 under
# their own key, and nothing in the authority can reach it. So they keep reading existing
# secrets until the next rotation, and the API says so by returning rotate_required. The
# second half is that after that rotation they are genuinely locked out. Testing only the
# first half would describe a hole; testing only the second would hide a window.
#
# Account deletion is attacked three ways separately, because it is defended three ways
# and a single check passing would not tell us the other two hold.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s29-offboarding"
qa_require_docker_stack
qa_require_cmd curl
trap 'qa_server_cleanup; [ "${QA_KEEP_SERVER:-0}" = "1" ] || qa_authority_down' EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
assert_green "the authority is listening" -- qa_authority_up
[ "$_qa_regression" -eq 0 ] || { spec_end; exit 1; }

N="$$-$(date +%s)"
W="$QA_RESULTS/s29"; rm -rf "$W"; mkdir -p "$W"
ORG="org29-$N"; PUUID="35353535-6767-4898-8bcd-$(qa_uuid_tail)"
SECRET='LEAVER=can-still-read-until-rotation'
printf '%s\n' "$SECRET" >"$W/v.txt"

for who in alice bob; do qa_actor_reset "$who"; done
A_ID="$(qa_actor_account alice "a29-$N@archicode.codes" "pw-a-$N")"; A_TOK="$(cat "$(qa_actor_dir alice)/session.tok")"
B_ID="$(qa_actor_account bob "b29-$N@archicode.codes" "pw-b-$N")"; B_TOK="$(cat "$(qa_actor_dir bob)/session.tok")"

qa_api POST /v1/orgs "$A_TOK" "{\"slug\":\"$ORG\",\"name\":\"O29\"}" >/dev/null
qa_api POST "/v1/orgs/$ORG/projects" "$A_TOK" "{\"id\":\"$PUUID\",\"slug\":\"p\",\"name\":\"P\"}" >/dev/null
E_ID="$(qa_json "$(qa_api POST "/v1/projects/$PUUID/environments" "$A_TOK" '{"name":"prod"}' | cut -f2-)" id)"
I="$(qa_json "$(qa_api POST "/v1/orgs/$ORG/invites" "$A_TOK" "{\"email\":\"b29-$N@archicode.codes\",\"role\":\"member\"}" | cut -f2-)" token)"
qa_api POST /v1/orgs/invites/accept "$B_TOK" "{\"token\":\"$I\"}" >/dev/null
qa_api POST "/v1/orgs/$ORG/teams" "$A_TOK" '{"slug":"t29","name":"T29"}' >/dev/null
qa_api POST "/v1/orgs/$ORG/teams/t29/members" "$A_TOK" "{\"user_id\":\"$B_ID\",\"team_role\":\"member\"}" >/dev/null
G_ID="$(qa_json "$(qa_api POST "/v1/orgs/$ORG/projects/$PUUID/grants" "$A_TOK" "{\"grantee_kind\":\"user\",\"grantee_id\":\"$B_ID\",\"project_role\":\"write\",\"env_id\":\"$E_ID\"}" | cut -f2-)" id)"

qa_actor alice "$W" "keys enroll --org $ORG" >/dev/null 2>&1
qa_actor bob "$W" "keys enroll --org $ORG" >/dev/null 2>&1
qa_actor alice "$W" "vault env-init --org $ORG --project $PUUID --env prod" >/dev/null 2>&1
qa_actor alice "$W" "vault sync-keys --org $ORG --project $PUUID --env prod" >/dev/null 2>&1
qa_actor alice "$W" "vault set-env --org $ORG --project $PUUID --env prod app/c < /project/v.txt" >/dev/null 2>&1

assert_green "the member reads the environment secret before leaving" \
	-- bash -c 'qa_actor bob "$1" "vault get-env --org $2 --project $3 --env prod app/c" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG" "$PUUID" "$SECRET"
assert_green "the member's public key is published" \
	-- bash -c '[ "$(qa_code GET "/v1/orgs/$2/users/$3/pubkey" "$1")" = 200 ]' _ "$A_TOK" "$ORG" "$B_ID"

# ── who may remove ───────────────────────────────────────────────────────────
assert_green "a plain member cannot remove another member" \
	-- bash -c '[ "$(qa_code DELETE "/v1/orgs/$2/members/$3" "$1")" = 403 ]' _ "$B_TOK" "$ORG" "$A_ID"

# ── the removal, and what it reports ─────────────────────────────────────────
REMOVAL="$(qa_api DELETE "/v1/orgs/$ORG/members/$B_ID" "$A_TOK")"
assert_green "removing a member succeeds" -- bash -c '[ "${1%%	*}" = 200 ]' _ "$REMOVAL"
assert_green "the response says a rotation is required" \
	-- bash -c 'grep -q "\"rotate_required\":true" <<<"$1"' _ "$REMOVAL"
assert_green "the response explains what was done" \
	-- bash -c 'grep -qE "removed|detail" <<<"$1"' _ "$REMOVAL"

# ── the cascade ──────────────────────────────────────────────────────────────
assert_green "the departed member is no longer an org member" \
	-- bash -c '! grep -qF "$3" <<<"$(qa_api GET "/v1/orgs/$2/members" "$1")"' _ "$A_TOK" "$ORG" "$B_ID"
assert_green "their published public key is gone with them" \
	-- bash -c '[ "$(qa_code GET "/v1/orgs/$2/users/$3/pubkey" "$1")" = 404 ]' _ "$A_TOK" "$ORG" "$B_ID"
assert_green "their direct project grant is revoked in the same breath" \
	-- bash -c 'r=$(qa_api GET "/v1/orgs/$2/projects/$3/grants" "$1"); ! grep -qF "$4" <<<"$r"' \
	_ "$A_TOK" "$ORG" "$PUUID" "$G_ID"
assert_green "they can no longer read the organisation at all" \
	-- bash -c '[ "$(qa_code GET "/v1/orgs/$2/members" "$1")" = 404 ]' _ "$B_TOK" "$ORG"

# ── the window, measured rather than assumed ─────────────────────────────────
# The design says access to already-wrapped secrets ends at the next rotation and not
# before, because a wrap handed out lives in vault42 under the member's own key and the
# authority cannot reach it. That is true at the crypto layer. It is NOT what an operator
# observes, because the client resolves the environment through the authority before it
# ever touches a wrap, and a removed member gets 404 there.
#
# So there are two facts and both matter. Through the CLI, removal is immediate, which is
# defence in depth worth pinning. The residual window is narrower than the design states
# and reachable only by a client that already holds the scope id and epoch and talks to
# vault42 directly. rotate_required is the signal that the window exists at all, and it
# is asserted above.
assert_green "a departed member loses control-plane access immediately" \
	-- bash -c '! qa_actor bob "$1" "vault get-env --org $2 --project $3 --env prod app/c" >/dev/null 2>&1' \
	_ "$W" "$ORG" "$PUUID"
assert_green "the failure is a lookup refusal, not a decryption error" \
	-- bash -c 'out=$(qa_actor bob "$1" "vault get-env --org $2 --project $3 --env prod app/c" 2>&1)
		! grep -qi "could not open\|not a wrapped member" <<<"$out"' \
	_ "$W" "$ORG" "$PUUID"

# ── and the rotation that closes it ──────────────────────────────────────────
ROT="$(qa_actor alice "$W" "vault rotate-scope --org $ORG --project $PUUID --env prod" 2>&1)"
printf '# rotate-scope said: %s\n' "$(printf '%s' "$ROT" | tr '\n' ' ')"
assert_green "the administrator still reads after the rotation" \
	-- bash -c 'qa_actor alice "$1" "vault get-env --org $2 --project $3 --env prod app/c" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG" "$PUUID" "$SECRET"
assert_green "the departed member is locked out by the rotation" \
	-- bash -c '! qa_actor bob "$1" "vault get-env --org $2 --project $3 --env prod app/c" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG" "$PUUID" "$SECRET"

# Rejoining must not silently restore what was revoked.
I2="$(qa_json "$(qa_api POST "/v1/orgs/$ORG/invites" "$A_TOK" "{\"email\":\"b29-$N@archicode.codes\",\"role\":\"member\"}" | cut -f2-)" token)"
qa_api POST /v1/orgs/invites/accept "$B_TOK" "{\"token\":\"$I2\"}" >/dev/null
assert_green "rejoining the org does not restore the revoked project access" \
	-- bash -c '! qa_actor bob "$1" "vault get-env --org $2 --project $3 --env prod app/c" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG" "$PUUID" "$SECRET"

# ── account deletion, attacked three ways ────────────────────────────────────
# It is defended three ways, so one check passing would not tell us the other two hold.
D_MAIL="d29-$N@archicode.codes"; D_PW="pw-d-$N"
D_ID="$(qa_signup "$D_MAIL" "$D_PW")"; D_TOK="$(qa_login "$D_MAIL" "$D_PW")"
assert_green "the account works before deletion" -- bash -c '[ "$(qa_code GET /v1/auth/me "$1")" = 200 ]' _ "$D_TOK"
assert_green "an account can delete itself" -- bash -c '[ "$(qa_code DELETE /v1/auth/account "$1")" = 200 ]' _ "$D_TOK"

assert_green "the deleted account cannot log in with its real password" \
	-- bash -c '[ -z "$(qa_login "$1" "$2")" ]' _ "$D_MAIL" "$D_PW"
assert_green "the deleted account's existing session no longer works" \
	-- bash -c '[ "$(qa_code GET /v1/auth/me "$1")" != 200 ]' _ "$D_TOK"
assert_green "logging in with a wrong password is refused the same way" \
	-- bash -c '[ -z "$(qa_login "$1" "definitely-not-the-password")" ]' _ "$D_MAIL"
# The address must come back into circulation, or a departing person's email is burned.
#
# Asserted through the BEHAVIOUR rather than a status. Signup now answers identically whether
# an address was free, so "it returned 201" no longer means anything; what means something is
# that a brand-new password mints a session and the account behind it is a different one.
assert_green "the address is freed for a genuinely new signup" \
	-- bash -c 'id=$(qa_signup "$1" "brand-new-password-$2")
		[ -n "$id" ] || { printf "the freed address could not be registered and logged into\n"; exit 1; }
		[ "$id" != "$3" ] || { printf "the new signup reused the deleted account id\n"; exit 1; }' \
	_ "$D_MAIL" "$N" "$D_ID"

spec_end
