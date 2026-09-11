#!/usr/bin/env bash
# s27 — what a grant actually authorizes.
#
# A grant either names an environment or leaves it null, and a null one is meant to apply
# to every environment in the project. That distinction decides who the administrator
# wraps the scope secret to, so getting it wrong is not a listing bug: too wide hands the
# key to somebody who should not hold it, too narrow silently locks out someone who
# should. Both failures are quiet, which is why they are worth testing directly.
#
# Two environments, two colleagues. One is granted a single environment, the other the
# whole project. The test is that each ends up able to read exactly what they should.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s27-grant-scoping"
qa_require_docker_stack
qa_require_cmd curl
trap 'qa_server_cleanup; [ "${QA_KEEP_SERVER:-0}" = "1" ] || qa_authority_down' EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
assert_green "the authority is listening" -- qa_authority_up
[ "$_qa_regression" -eq 0 ] || { spec_end; exit 1; }

N="$$-$(date +%s)"
W="$QA_RESULTS/s27"; rm -rf "$W"; mkdir -p "$W"
ORG="org27-$N"
PUUID="12121212-3434-4565-8787-$(qa_uuid_tail)"
PROD_SECRET='WHICH=prod-only-value-0001'
STAGE_SECRET='WHICH=staging-only-value-0002'
printf '%s\n' "$PROD_SECRET" >"$W/prod.txt"
printf '%s\n' "$STAGE_SECRET" >"$W/stage.txt"

for who in alice bob carol; do qa_actor_reset "$who"; done
A_ID="$(qa_actor_account alice "a27-$N@archicode.codes" "pw-a-$N")"; A_TOK="$(cat "$(qa_actor_dir alice)/session.tok")"
B_ID="$(qa_actor_account bob "b27-$N@archicode.codes" "pw-b-$N")"; B_TOK="$(cat "$(qa_actor_dir bob)/session.tok")"
C_ID="$(qa_actor_account carol "c27-$N@archicode.codes" "pw-c-$N")"; C_TOK="$(cat "$(qa_actor_dir carol)/session.tok")"

qa_api POST /v1/orgs "$A_TOK" "{\"slug\":\"$ORG\",\"name\":\"O27\"}" >/dev/null
qa_api POST "/v1/orgs/$ORG/projects" "$A_TOK" "{\"id\":\"$PUUID\",\"slug\":\"p\",\"name\":\"P\"}" >/dev/null
PROD_ID="$(qa_json "$(qa_api POST "/v1/projects/$PUUID/environments" "$A_TOK" '{"name":"prod"}' | cut -f2-)" id)"
STAGE_ID="$(qa_json "$(qa_api POST "/v1/projects/$PUUID/environments" "$A_TOK" '{"name":"staging"}' | cut -f2-)" id)"
assert_green "a project can hold two environments" \
	-- bash -c '[ -n "$1" ] && [ -n "$2" ] && [ "$1" != "$2" ]' _ "$PROD_ID" "$STAGE_ID"

for pair in "bob:b27" "carol:c27"; do
	who="${pair%%:*}"; pfx="${pair#*:}"
	t="$(qa_json "$(qa_api POST "/v1/orgs/$ORG/invites" "$A_TOK" "{\"email\":\"$pfx-$N@archicode.codes\",\"role\":\"member\"}" | cut -f2-)" token)"
	tok="$(cat "$(qa_actor_dir "$who")/session.tok")"
	qa_api POST /v1/orgs/invites/accept "$tok" "{\"token\":\"$t\"}" >/dev/null
	qa_actor "$who" "$W" "keys enroll --org $ORG" >/dev/null 2>&1
done
qa_actor alice "$W" "keys enroll --org $ORG" >/dev/null 2>&1
qa_actor alice "$W" "vault env-init --org $ORG --project $PUUID --env prod" >/dev/null 2>&1
qa_actor alice "$W" "vault env-init --org $ORG --project $PUUID --env staging" >/dev/null 2>&1

# Bob: one environment only. Carol: the whole project, expressed by omitting env_id.
qa_api POST "/v1/orgs/$ORG/projects/$PUUID/grants" "$A_TOK" \
	"{\"grantee_kind\":\"user\",\"grantee_id\":\"$B_ID\",\"project_role\":\"write\",\"env_id\":\"$PROD_ID\"}" >/dev/null
qa_api POST "/v1/orgs/$ORG/projects/$PUUID/grants" "$A_TOK" \
	"{\"grantee_kind\":\"user\",\"grantee_id\":\"$C_ID\",\"project_role\":\"write\"}" >/dev/null

for e in prod staging; do
	qa_actor alice "$W" "vault sync-keys --org $ORG --project $PUUID --env $e" >/dev/null 2>&1
done
qa_actor alice "$W" "vault set-env --org $ORG --project $PUUID --env prod app/c < /project/prod.txt" >/dev/null 2>&1
qa_actor alice "$W" "vault set-env --org $ORG --project $PUUID --env staging app/c < /project/stage.txt" >/dev/null 2>&1

# ── the environment-scoped grant ─────────────────────────────────────────────
assert_green "an environment-scoped grant lets the member read that environment" \
	-- bash -c 'qa_actor bob "$1" "vault get-env --org $2 --project $3 --env prod app/c" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG" "$PUUID" "$PROD_SECRET"
assert_green "an environment-scoped grant does NOT leak into the other environment" \
	-- bash -c '! qa_actor bob "$1" "vault get-env --org $2 --project $3 --env staging app/c" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG" "$PUUID" "$STAGE_SECRET"
assert_green "the scoped member is not listed as pending in the environment they lack" \
	-- bash -c '! qa_actor alice "$1" "vault scope-status --org $2 --project $3 --env staging" 2>/dev/null | grep -q "$4"' \
	_ "$W" "$ORG" "$PUUID" "$(printf '%s' "$B_ID" | cut -c1-8)"

# ── the project-wide grant ───────────────────────────────────────────────────
assert_green "a project-wide grant reaches the first environment" \
	-- bash -c 'qa_actor carol "$1" "vault get-env --org $2 --project $3 --env prod app/c" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG" "$PUUID" "$PROD_SECRET"
assert_green "a project-wide grant reaches the second environment too" \
	-- bash -c 'qa_actor carol "$1" "vault get-env --org $2 --project $3 --env staging app/c" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG" "$PUUID" "$STAGE_SECRET"

# ── and the boundary still holds ─────────────────────────────────────────────
assert_green "an org member with no grant at all reads neither environment" \
	-- bash -c 'qa_actor_reset dave >/dev/null
		id=$(qa_actor_account dave "d27-$1@archicode.codes" "pw-d-$1")
		t=$(qa_json "$(qa_api POST "/v1/orgs/$2/invites" "$3" "{\"email\":\"d27-$1@archicode.codes\",\"role\":\"member\"}" | cut -f2-)" token)
		qa_api POST /v1/orgs/invites/accept "$(cat "$(qa_actor_dir dave)/session.tok")" "{\"token\":\"$t\"}" >/dev/null
		qa_actor dave "$4" "keys enroll --org $2" >/dev/null 2>&1
		! qa_actor dave "$4" "vault get-env --org $2 --project $5 --env prod app/c" >/dev/null 2>&1' \
	_ "$N" "$ORG" "$A_TOK" "$W" "$PUUID"

spec_end
