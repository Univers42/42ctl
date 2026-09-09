#!/usr/bin/env bash
# s23 — the organisation story, start to finish, against the live authority.
#
# Route probes prove an endpoint answers. They do not prove the rules hold. This walks
# the sequence a real team performs — sign up, found an org, invite someone, have them
# join, then try to do things they should not be allowed to do — and checks the refusals
# as carefully as the successes.
#
# Emails carry a per-run nonce because accounts persist for the life of the authority
# container. Everything else is fixed.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s23-org-lifecycle-scenario"
qa_require_docker_stack
qa_require_cmd curl
trap 'qa_server_cleanup; [ "${QA_KEEP_SERVER:-0}" = "1" ] || qa_authority_down' EXIT
assert_green "the authority is listening" -- qa_authority_up
[ "$_qa_regression" -eq 0 ] || { spec_end; exit 1; }

N="$$-$(date +%s)"
ORG="qaorg-$N"
A_MAIL="alice-$N@archicode.codes"; A_PW="alice-password-$N"
B_MAIL="bob-$N@archicode.codes";   B_PW="bob-password-$N"
C_MAIL="carol-$N@archicode.codes"
D_MAIL="dave-$N@archicode.codes";  D_PW="dave-password-$N"

# ── founding ─────────────────────────────────────────────────────────────────
A_ID="$(qa_signup "$A_MAIL" "$A_PW")"
A_TOK="$(qa_login "$A_MAIL" "$A_PW")"
assert_green "alice's account id is a UUID" \
	-- bash -c '[[ $1 =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$ ]]' _ "$A_ID"
assert_green "alice receives a bearer token on login" -- bash -c '[ ${#1} -ge 20 ]' _ "$A_TOK"

assert_green "alice founds an org" \
	-- bash -c '[ "$(qa_code POST /v1/orgs "$1" "{\"slug\":\"$2\",\"name\":\"QA Org\"}")" = 201 ]' _ "$A_TOK" "$ORG"
assert_green "the founder is the org owner" \
	-- bash -c 'r=$(qa_api GET "/v1/orgs/$2/members" "$1"); grep -q "\"role\":\"owner\"" <<<"$r"' _ "$A_TOK" "$ORG"

# The identity invariant: what /v1/auth/me calls the account must be exactly what the
# members list calls the user. 42ctl frames a proof of possession with this string, so
# any divergence breaks every downstream signature check.
assert_green "the account_id from /v1/auth/me equals the user_id in /members" \
	-- bash -c '
		me=$(qa_api GET /v1/auth/me "$1"); mid=$(qa_json "${me#*	}" account_id)
		mem=$(qa_api GET "/v1/orgs/$2/members" "$1")
		grep -qF "\"user_id\":\"$mid\"" <<<"$mem"' _ "$A_TOK" "$ORG"

# ── inviting somebody in ─────────────────────────────────────────────────────
INV="$(qa_api POST "/v1/orgs/$ORG/invites" "$A_TOK" "{\"email\":\"$B_MAIL\",\"role\":\"member\"}")"
INV_TOKEN="$(qa_json "${INV#*	}" token)"
assert_green "alice invites bob and receives a one-time token" -- bash -c '[ -n "$1" ]' _ "$INV_TOKEN"

B_ID="$(qa_signup "$B_MAIL" "$B_PW")"
B_TOK="$(qa_login "$B_MAIL" "$B_PW")"
assert_green "bob accepts his invite" \
	-- bash -c '[ "$(qa_code POST /v1/orgs/invites/accept "$1" "{\"token\":\"$2\"}")" = 200 ]' _ "$B_TOK" "$INV_TOKEN"
assert_green "bob now appears as a member" \
	-- bash -c 'grep -qF "\"user_id\":\"$3\"" <<<"$(qa_api GET "/v1/orgs/$2/members" "$1")"' _ "$A_TOK" "$ORG" "$B_ID"

# ── an invite is single-use and bound to its address ─────────────────────────
assert_green "the same invite token cannot be redeemed twice" \
	-- bash -c '[ "$(qa_code POST /v1/orgs/invites/accept "$1" "{\"token\":\"$2\"}")" = 409 ]' _ "$B_TOK" "$INV_TOKEN"

C_INV="$(qa_api POST "/v1/orgs/$ORG/invites" "$A_TOK" "{\"email\":\"$C_MAIL\",\"role\":\"member\"}")"
C_TOKEN="$(qa_json "${C_INV#*	}" token)"
qa_signup "$D_MAIL" "$D_PW" >/dev/null
D_TOK="$(qa_login "$D_MAIL" "$D_PW")"
assert_green "an invite addressed to carol cannot be redeemed by dave" \
	-- bash -c '[ "$(qa_code POST /v1/orgs/invites/accept "$1" "{\"token\":\"$2\"}")" = 403 ]' _ "$D_TOK" "$C_TOKEN"

# ── a plain member cannot administer ─────────────────────────────────────────
assert_green "a member cannot issue invites" \
	-- bash -c '[ "$(qa_code POST "/v1/orgs/$2/invites" "$1" "{\"email\":\"x-$3@archicode.codes\",\"role\":\"member\"}")" = 403 ]' \
	_ "$B_TOK" "$ORG" "$N"

# ── an outsider learns nothing, not even whether the org exists ──────────────
# Returning 403 to a non-member would confirm the slug is real, turning the endpoint
# into a membership oracle. Absent and forbidden must look identical.
assert_green "an outsider reading a real org gets 404" \
	-- bash -c '[ "$(qa_code GET "/v1/orgs/$2/members" "$1")" = 404 ]' _ "$D_TOK" "$ORG"
assert_green "an outsider reading an absent org gets the same 404" \
	-- bash -c '[ "$(qa_code GET "/v1/orgs/no-such-org-$2/members" "$1")" = 404 ]' _ "$D_TOK" "$N"

# ── roles are a closed set, checked at the boundary ──────────────────────────
assert_green "a junk org role is refused" \
	-- bash -c '[ "$(qa_code POST "/v1/orgs/$2/invites" "$1" "{\"email\":\"j-$3@archicode.codes\",\"role\":\"wizard\"}")" = 400 ]' \
	_ "$A_TOK" "$ORG" "$N"

# ── teams, and the rule that team membership cannot bypass org membership ────
assert_green "alice creates a team" \
	-- bash -c '[ "$(qa_code POST "/v1/orgs/$2/teams" "$1" "{\"slug\":\"qateam\",\"name\":\"QA Team\"}")" = 201 ]' _ "$A_TOK" "$ORG"
assert_green "an org member can be added to a team" \
	-- bash -c '[ "$(qa_code POST "/v1/orgs/$2/teams/qateam/members" "$1" "{\"user_id\":\"$3\",\"team_role\":\"member\"}")" = 204 ]' \
	_ "$A_TOK" "$ORG" "$B_ID"
assert_green "someone outside the org cannot be added to a team" \
	-- bash -c '[ "$(qa_code POST "/v1/orgs/$2/teams/qateam/members" "$1" "{\"user_id\":\"$3\",\"team_role\":\"member\"}")" = 400 ]' \
	_ "$A_TOK" "$ORG" "$(qa_json "$(qa_api GET /v1/auth/me "$D_TOK")" account_id)"
assert_green "an org role is refused where a team role belongs" \
	-- bash -c '[ "$(qa_code POST "/v1/orgs/$2/teams/qateam/members" "$1" "{\"user_id\":\"$3\",\"team_role\":\"owner\"}")" = 400 ]' \
	_ "$A_TOK" "$ORG" "$B_ID"
# The rule lives at ACCEPT time, not at invite time, and that is the right place:
# an invite is addressed to an email that need not have an account yet, so refusing to
# issue it would make it impossible to invite anyone new. What must not happen is a
# team membership existing without the org membership under it.
S_MAIL="stranger-$N@archicode.codes"; S_PW="stranger-password-$N"
T_INV="$(qa_api POST "/v1/orgs/$ORG/teams/qateam/invites" "$A_TOK" "{\"email\":\"$S_MAIL\",\"role\":\"member\"}")"
T_TOKEN="$(qa_json "${T_INV#*	}" token)"
qa_signup "$S_MAIL" "$S_PW" >/dev/null
S_TOK="$(qa_login "$S_MAIL" "$S_PW")"

assert_green "a team invite may be issued to someone with no org membership yet" \
	-- bash -c '[ -n "$1" ]' _ "$T_TOKEN"
assert_green "accepting a team invite without org membership is refused" \
	-- bash -c '[ "$(qa_code POST /v1/invites/accept "$1" "{\"token\":\"$2\"}")" = 400 ]' _ "$S_TOK" "$T_TOKEN"
assert_green "the refusal explains that the org invite comes first" \
	-- bash -c 'r=$(qa_api POST /v1/invites/accept "$1" "{\"token\":\"$2\"}"); grep -qi "organization" <<<"$r"' _ "$S_TOK" "$T_TOKEN"

spec_end
