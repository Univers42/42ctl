#!/usr/bin/env bash
# s25 — the shared-environment secret, start to finish, across two people.
#
# This is the machinery the whole product rests on. An administrator generates a scope
# keyset for an environment and publishes only its PUBLIC half. Members enroll their own
# public keys. The administrator recovers the scope secret from her own wrap and re-wraps
# it to each authorized member. From then on anyone in the environment can read a secret
# nobody else can, and the server holds only opaque blobs throughout.
#
# The story is driven entirely through the CLI, exactly as an operator would, against a
# live authority and a live vault42-server. The control-plane bearer is injected into the
# per-profile session file because the normal way to get one is a GitHub browser flow;
# the file format is the raw token, so every request is still authenticated normally.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s25-scope-lifecycle"
qa_require_docker_stack
qa_require_cmd curl
trap 'qa_server_cleanup; [ "${QA_KEEP_SERVER:-0}" = "1" ] || qa_authority_down' EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
assert_green "the authority is listening" -- qa_authority_up
[ "$_qa_regression" -eq 0 ] || { spec_end; exit 1; }

N="$$-$(date +%s)"
W="$QA_RESULTS/s25"; rm -rf "$W"; mkdir -p "$W"
ORG="org-$N"
PUUID="aaaaaaaa-bbbb-4ccc-8ddd-$(qa_uuid_tail)"
SECRET='TOPSECRET=env-shared-value-0001'
printf '%s\n' "$SECRET" >"$W/value.txt"

for who in alice bob mallory; do qa_actor_reset "$who"; done
A_ID="$(qa_actor_account alice "alice-$N@archicode.codes" "pw-alice-$N")"
A_TOK="$(cat "$(qa_actor_dir alice)/session.tok")"

# ── the administrator founds the scope ───────────────────────────────────────
qa_api POST /v1/orgs "$A_TOK" "{\"slug\":\"$ORG\",\"name\":\"Scope Org\"}" >/dev/null
assert_green "a project can be created under a client-supplied UUID" \
	-- bash -c '[ "$(qa_code POST "/v1/orgs/$2/projects" "$1" "{\"id\":\"$3\",\"slug\":\"proj\",\"name\":\"P\"}")" = 201 ]' \
	_ "$A_TOK" "$ORG" "$PUUID"
ENV_ID="$(qa_json "$(qa_api POST "/v1/projects/$PUUID/environments" "$A_TOK" '{"name":"prod"}' | cut -f2-)" id)"
assert_green "an environment is created under that project" -- bash -c '[ -n "$1" ]' _ "$ENV_ID"

assert_green "the administrator enrolls her own public key" \
	-- qa_actor alice "$W" "keys enroll --org $ORG"
assert_green "the administrator bootstraps the environment scope" \
	-- qa_actor alice "$W" "vault env-init --org $ORG --project $PUUID --env prod"
assert_green "the environment now advertises a scope public key" \
	-- bash -c 'r=$(qa_api GET "/v1/projects/$2/environments" "$1"); ! grep -q "\"scope_pubkey\":null" <<<"$r"' \
	_ "$A_TOK" "$PUUID"

# ── a colleague joins and is provisioned ─────────────────────────────────────
B_ID="$(qa_actor_account bob "bob-$N@archicode.codes" "pw-bob-$N")"
B_TOK="$(cat "$(qa_actor_dir bob)/session.tok")"
I_TOK="$(qa_json "$(qa_api POST "/v1/orgs/$ORG/invites" "$A_TOK" "{\"email\":\"bob-$N@archicode.codes\",\"role\":\"member\"}" | cut -f2-)" token)"
qa_api POST /v1/orgs/invites/accept "$B_TOK" "{\"token\":\"$I_TOK\"}" >/dev/null

# Before he enrolls a public key there is nothing to wrap to. Reporting him as pending
# rather than provisioning him is the correct behaviour: wrapping to an unverified key
# would hand the scope secret to whoever registered it.
G_ID="$(qa_json "$(qa_api POST "/v1/orgs/$ORG/projects/$PUUID/grants" "$A_TOK" "{\"grantee_kind\":\"user\",\"grantee_id\":\"$B_ID\",\"project_role\":\"write\",\"env_id\":\"$ENV_ID\"}" | cut -f2-)" id)"
assert_green "the grant is created and returns an id" -- bash -c '[ -n "$1" ]' _ "$G_ID"
assert_green "a granted member with no public key is reported as pending enrollment" \
	-- bash -c '

		qa_actor alice "$6" "vault scope-status --org $2 --project $3 --env prod" 2>&1 | grep -q "pending-enrollment"' \
	_ "$A_TOK" "$ORG" "$PUUID" "$B_ID" "$ENV_ID" "$W"

assert_green "the colleague enrolls his public key" -- qa_actor bob "$W" "keys enroll --org $ORG"
assert_green "reconciling provisions exactly one member" \
	-- bash -c 'qa_actor alice "$1" "vault sync-keys --org $2 --project $3 --env prod" 2>&1 | grep -q "provisioned 1"' \
	_ "$W" "$ORG" "$PUUID"
assert_green "reconciling again is idempotent and provisions nobody new" \
	-- bash -c 'qa_actor alice "$1" "vault sync-keys --org $2 --project $3 --env prod" 2>&1 | grep -q "provisioned 0"' \
	_ "$W" "$ORG" "$PUUID"
assert_green "scope-status now reports the colleague as active" \
	-- bash -c 'qa_actor alice "$1" "vault scope-status --org $2 --project $3 --env prod" 2>&1 | grep -q "active"' \
	_ "$W" "$ORG" "$PUUID"

# ── the payoff: a secret one writes and the other reads ──────────────────────
assert_green "the administrator seals a secret to the environment" \
	-- qa_actor alice "$W" "vault set-env --org $ORG --project $PUUID --env prod app/config < /project/value.txt"
assert_green "the colleague reads it back with the exact bytes" \
	-- bash -c 'qa_actor bob "$1" "vault get-env --org $2 --project $3 --env prod app/config" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG" "$PUUID" "$SECRET"

# ── and the half that makes it worth anything ────────────────────────────────
M_ID="$(qa_actor_account mallory "mallory-$N@archicode.codes" "pw-mal-$N")"
M_TOK="$(cat "$(qa_actor_dir mallory)/session.tok")"
assert_green "someone outside the organisation cannot read the environment secret" \
	-- bash -c '! qa_actor mallory "$1" "vault get-env --org $2 --project $3 --env prod app/config" >/dev/null 2>&1' \
	_ "$W" "$ORG" "$PUUID"
assert_green "an outsider cannot even see the environment exists" \
	-- bash -c '[ "$(qa_code GET "/v1/projects/$2/environments" "$1")" = 404 ]' _ "$M_TOK" "$PUUID"

DB="$W/server.db"; qa_dump_server_db "$DB"
A_FP="$(qa_actor alice "$W" "auth whoami" 2>/dev/null | sed -n 's/^principal *//p' | tr -d ' \r')"
if [ -f "$DB" ]; then
	assert_zero_knowledge "the environment plaintext never reached the server" "env-shared-value-0001" "$A_FP" "$DB"
fi

# ── rotation ────────────────────────────────────────────────────────────────
#
# ROTATION CURRENTLY STRANDS THE ENVIRONMENT. It re-seals every secret to a new epoch
# and then wraps the new key to nobody, including the administrator who ran it, so the
# scope secret is destroyed with the Zeroizing buffer when the command exits. The
# authority refuses an epoch regression, so there is no supported way back.
#
# The cause is in scope_secret_reseal.rs: the authorized set for re-wrapping is taken
# from the grant "missing" list, which is authorized-minus-already-wrapped. After a
# successful reconcile that list is empty. A ponytail marker there says a follow-up
# sync-keys re-adds anyone dropped, and that is the part that does not hold: sync-keys
# must first recover the scope secret from the caller's own wrap, and after rotation
# there is no such wrap at the new epoch, so the repair path fails too.
#
# These are assert_green because the commands ship and report success. Each one is the
# bug stated as the behaviour an operator is entitled to expect.

# Rotate ONCE and capture the output, then assert against what was captured. Invoking
# the command inside each assertion would rotate several times and make the result
# depend on assertion order, which is how this spec first gave two different answers on
# two runs of the same code.
ROT_OUT="$(qa_actor alice "$W" "vault rotate-scope --org $ORG --project $PUUID --env prod" 2>&1)"
printf '# rotate-scope said: %s\n' "$(printf '%s' "$ROT_OUT" | tr '\n' ' ')"

assert_green "rotation reports re-wrapping at least one member" \
	-- bash -c 'grep -qE "rewrapped [1-9]" <<<"$1"' _ "$ROT_OUT"
assert_green "the administrator can still read her own environment after rotation" \
	-- bash -c 'qa_actor alice "$1" "vault get-env --org $2 --project $3 --env prod app/config" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG" "$PUUID" "$SECRET"
assert_green "a member who remains authorized still reads after rotation" \
	-- bash -c 'qa_actor bob "$1" "vault get-env --org $2 --project $3 --env prod app/config" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG" "$PUUID" "$SECRET"
assert_green "a stranded environment can be repaired by reconciling again" \
	-- bash -c 'qa_actor alice "$1" "vault sync-keys --org $2 --project $3 --env prod" >/dev/null 2>&1 &&
		qa_actor bob "$1" "vault get-env --org $2 --project $3 --env prod app/config" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG" "$PUUID" "$SECRET"

# ── offboarding ──────────────────────────────────────────────────────────────
#
# Rotation exists so that a member who is no longer authorized loses access. That is
# unreachable for a different reason: the authority has 29 routes and not one DELETE.
# Nobody can be removed from an org, a team or a grant, so "the remaining authorized
# members" is always everyone. For a secrets vault, offboarding is the operation that
# matters most after storing a secret.
# Removal is authorization, not erasure. A wrap already handed out lives in vault42 under
# the member's own key and nothing in the authority can reach it, so access to existing
# secrets ends at the next rotation and not before. The API says so in its response, and
# pinning that keeps it from drifting into an unstated assumption.
assert_green "a member's grant can be revoked" \
	-- bash -c '[ "$(qa_code DELETE "/v1/orgs/$2/projects/$3/grants/$4" "$1")" = 200 ]' _ "$A_TOK" "$ORG" "$PUUID" "$G_ID"
assert_green "a member can be removed from the organisation" \
	-- bash -c '[ "$(qa_code DELETE "/v1/orgs/$2/members/$3" "$1")" = 200 ]' _ "$A_TOK" "$ORG" "$B_ID"

spec_end
