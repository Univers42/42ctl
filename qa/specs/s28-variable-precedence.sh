#!/usr/bin/env bash
# s28 — variables at three scopes, and which one wins.
#
# An organisation sets a default, a project overrides it, an environment overrides that.
# Precedence bugs are quiet by nature: staging silently inherits production's value, or a
# deployment keeps an old default because the override never took. Nobody sees an error,
# they see the wrong behaviour somewhere else, much later.
#
# So every assertion checks the resolved view rather than the write, and each layer is
# removed again to prove the fallback is real rather than a coincidence of ordering.
#
# The other half is authorization. Writes need organisation administration or a live
# project grant, which is where grants stop being bookkeeping and start deciding
# something. That claim is tested from both sides.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s28-variable-precedence"
qa_require_docker_stack
qa_require_cmd curl
trap 'qa_server_cleanup; [ "${QA_KEEP_SERVER:-0}" = "1" ] || qa_authority_down' EXIT
assert_green "the authority is listening" -- qa_authority_up
[ "$_qa_regression" -eq 0 ] || { spec_end; exit 1; }

N="$$-$(date +%s)"
ORG="org28-$N"
PUUID="24242424-5656-4787-89ab-$(qa_uuid_tail)"
A_ID="$(qa_signup "a28-$N@archicode.codes" "pw-a-$N")"; A_TOK="$(qa_login "a28-$N@archicode.codes" "pw-a-$N")"
M_ID="$(qa_signup "m28-$N@archicode.codes" "pw-m-$N")"; M_TOK="$(qa_login "m28-$N@archicode.codes" "pw-m-$N")"
O_ID="$(qa_signup "o28-$N@archicode.codes" "pw-o-$N")"; O_TOK="$(qa_login "o28-$N@archicode.codes" "pw-o-$N")"

qa_api POST /v1/orgs "$A_TOK" "{\"slug\":\"$ORG\",\"name\":\"O28\"}" >/dev/null
qa_api POST "/v1/orgs/$ORG/projects" "$A_TOK" "{\"id\":\"$PUUID\",\"slug\":\"p\",\"name\":\"P\"}" >/dev/null
E_ID="$(qa_json "$(qa_api POST "/v1/projects/$PUUID/environments" "$A_TOK" '{"name":"prod"}' | cut -f2-)" id)"
I="$(qa_json "$(qa_api POST "/v1/orgs/$ORG/invites" "$A_TOK" "{\"email\":\"m28-$N@archicode.codes\",\"role\":\"member\"}" | cut -f2-)" token)"
qa_api POST /v1/orgs/invites/accept "$M_TOK" "{\"token\":\"$I\"}" >/dev/null

RESOLVE="/v1/projects/$PUUID/environments/prod/resolve"
# Echo the resolved value of KEY, or nothing when it is absent.
resolved() {
	qa_api GET "$RESOLVE" "$A_TOK" | cut -f2- |
		tr '}' '\n' | grep -F "\"key\":\"$1\"" | sed -n 's/.*"value":"\([^"]*\)".*/\1/p' | head -1
}
export -f resolved; export RESOLVE A_TOK

# ── precedence, layer by layer ───────────────────────────────────────────────
qa_api PUT "/v1/orgs/$ORG/variables/TIER" "$A_TOK" '{"value":"org-default"}' >/dev/null
assert_green "an organisation default resolves when nothing overrides it" \
	-- bash -c '[ "$(resolved TIER)" = "org-default" ]'
qa_api PUT "/v1/projects/$PUUID/variables/TIER" "$A_TOK" '{"value":"project-override"}' >/dev/null
assert_green "a project value overrides the organisation default" \
	-- bash -c '[ "$(resolved TIER)" = "project-override" ]'
qa_api PUT "/v1/projects/$PUUID/environments/prod/variables/TIER" "$A_TOK" '{"value":"env-override"}' >/dev/null
assert_green "an environment value overrides the project value" \
	-- bash -c '[ "$(resolved TIER)" = "env-override" ]'

# ── the fallback, which proves precedence is ordering and not luck ───────────
qa_api DELETE "/v1/projects/$PUUID/environments/prod/variables/TIER" "$A_TOK" >/dev/null
assert_green "removing the environment value falls back to the project" \
	-- bash -c '[ "$(resolved TIER)" = "project-override" ]'
qa_api DELETE "/v1/projects/$PUUID/variables/TIER" "$A_TOK" >/dev/null
assert_green "removing the project value falls back to the organisation" \
	-- bash -c '[ "$(resolved TIER)" = "org-default" ]'
qa_api DELETE "/v1/orgs/$ORG/variables/TIER" "$A_TOK" >/dev/null
assert_green "removing the last value leaves the key unresolved" \
	-- bash -c '[ -z "$(resolved TIER)" ]'

qa_api PUT "/v1/orgs/$ORG/variables/ONLY_ORG" "$A_TOK" '{"value":"o"}' >/dev/null
qa_api PUT "/v1/projects/$PUUID/variables/ONLY_PROJECT" "$A_TOK" '{"value":"p"}' >/dev/null
qa_api PUT "/v1/projects/$PUUID/environments/prod/variables/ONLY_ENV" "$A_TOK" '{"value":"e"}' >/dev/null
assert_green "keys from all three scopes appear together in the resolved view" \
	-- bash -c '[ "$(resolved ONLY_ORG)" = o ] && [ "$(resolved ONLY_PROJECT)" = p ] && [ "$(resolved ONLY_ENV)" = e ]'
assert_green "the resolved view says which scope each value came from" \
	-- bash -c 'r=$(qa_api GET "$RESOLVE" "$A_TOK"); grep -q "scope_kind" <<<"$r"'

# ── authorization ────────────────────────────────────────────────────────────
assert_green "a plain member may read the resolved view" \
	-- bash -c '[ "$(qa_code GET "$RESOLVE" "$1")" = 200 ]' _ "$M_TOK"
assert_green "a plain member may NOT write an organisation variable" \
	-- bash -c '[ "$(qa_code PUT "/v1/orgs/$2/variables/TIER" "$1" "{\"value\":\"x\"}")" = 403 ]' _ "$M_TOK" "$ORG"
assert_green "a plain member with no grant may NOT write a project variable" \
	-- bash -c 'c=$(qa_code PUT "/v1/projects/$2/variables/TIER" "$1" "{\"value\":\"x\"}"); [ "$c" = 403 ] || [ "$c" = 404 ]' _ "$M_TOK" "$PUUID"

qa_api POST "/v1/orgs/$ORG/projects/$PUUID/grants" "$A_TOK" \
	"{\"grantee_kind\":\"user\",\"grantee_id\":\"$M_ID\",\"project_role\":\"write\",\"env_id\":\"$E_ID\"}" >/dev/null
assert_green "a project grant of write lets that member write the environment variable" \
	-- bash -c '[ "$(qa_code PUT "/v1/projects/$2/environments/prod/variables/GRANTED" "$1" "{\"value\":\"y\"}")" != 403 ]' \
	_ "$M_TOK" "$PUUID"
assert_green "an environment-scoped grant does not authorize writing the org default" \
	-- bash -c '[ "$(qa_code PUT "/v1/orgs/$2/variables/TIER" "$1" "{\"value\":\"x\"}")" = 403 ]' _ "$M_TOK" "$ORG"
assert_green "an outsider cannot read the resolved view at all" \
	-- bash -c '[ "$(qa_code GET "$RESOLVE" "$1")" = 404 ]' _ "$O_TOK"

# ── the is_secret flag ───────────────────────────────────────────────────────
# The flag is enforced rather than advisory: a value marked secret must actually be a
# sealed envelope, proved by parsing it without opening it. That closes the trap where a
# name promising protection provided none.
qa_api PUT "/v1/projects/$PUUID/environments/prod/variables/PLAIN" "$A_TOK" '{"value":"plainly-readable-value"}' >/dev/null
assert_green "an ordinary value is stored and returned exactly as sent" \
	-- bash -c '[ "$(resolved PLAIN)" = "plainly-readable-value" ]'
assert_green "the resolved view carries the is_secret flag" \
	-- bash -c 'r=$(qa_api GET "$RESOLVE" "$A_TOK"); grep -q "is_secret" <<<"$r"'
assert_green "a value marked secret that is not sealed is refused at the boundary" \
	-- bash -c '[ "$(qa_code PUT "/v1/projects/$2/environments/prod/variables/NAKED" "$1" "{\"value\":\"hunter2\",\"is_secret\":true}")" = 400 ]' \
	_ "$A_TOK" "$PUUID"
# The control. Without it, a route that refused everything would look identically green.
# The same bytes with the flag off must be accepted, so the refusal is about the claim
# being made rather than about the value or the route.
assert_green "the identical value without the flag is accepted" \
	-- bash -c '[ "$(qa_code PUT "/v1/projects/$2/environments/prod/variables/NAKED" "$1" "{\"value\":\"hunter2\"}")" != 400 ]' \
	_ "$A_TOK" "$PUUID"
assert_green "the value that landed is the one that was allowed" \
	-- bash -c '[ "$(resolved NAKED)" = "hunter2" ]'

spec_end
