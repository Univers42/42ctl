#!/usr/bin/env bash
# s24 — the P3 control plane: projects, environments, pubkeys and grants, and the
# authorization rules that decide whether the key bridge is safe rather than merely
# present.
#
# The interesting assertions here are not "the route answers". They are the rules that,
# if wrong, hand an environment to the wrong person: who may publish the key everyone
# seals to, whether a key epoch can be rolled backwards, and whether the authorized
# member list can be made to include somebody who was never granted.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s24-scope-bridge-authz"
qa_require_docker_stack
qa_require_cmd curl
trap 'qa_server_cleanup; [ "${QA_KEEP_SERVER:-0}" = "1" ] || qa_authority_down' EXIT
assert_green "the authority is listening" -- qa_authority_up
assert_green "vault42-server is listening" -- qa_server_up
qa_build_client >/dev/null 2>&1
[ "$_qa_regression" -eq 0 ] || { spec_end; exit 1; }

N="$$-$(date +%s)"
WORK="$QA_RESULTS/s24"; rm -rf "$WORK"; mkdir -p "$WORK"
qa_actor_reset alice
ORG="org24-$N"
PUUID="cccccccc-dddd-4eee-8fff-$(printf '%012d' $$)"
A_ID="$(qa_signup "a24-$N@archicode.codes" "pw-a24-$N")"; A_TOK="$(qa_login "a24-$N@archicode.codes" "pw-a24-$N")"
M_ID="$(qa_signup "m24-$N@archicode.codes" "pw-m24-$N")"; M_TOK="$(qa_login "m24-$N@archicode.codes" "pw-m24-$N")"
qa_api POST /v1/orgs "$A_TOK" "{\"slug\":\"$ORG\",\"name\":\"O24\"}" >/dev/null
I="$(qa_json "$(qa_api POST "/v1/orgs/$ORG/invites" "$A_TOK" "{\"email\":\"m24-$N@archicode.codes\",\"role\":\"member\"}" | cut -f2-)" token)"
qa_api POST /v1/orgs/invites/accept "$M_TOK" "{\"token\":\"$I\"}" >/dev/null

# ── projects, and the identifier that ties the two halves together ───────────
assert_green "POST /v1/orgs/{org}/projects honours a client-supplied UUID" \
	-- bash -c 'r=$(qa_api POST "/v1/orgs/$2/projects" "$1" "{\"id\":\"$3\",\"slug\":\"p\",\"name\":\"P\"}"); grep -qF "$3" <<<"$r"' \
	_ "$A_TOK" "$ORG" "$PUUID"
# The scope id derivation parses the project as UUID bytes, so a non-UUID project could
# never derive a scope. Refusing it at creation is what keeps the two halves reconcilable.
assert_green "a non-UUID project id is refused" \
	-- bash -c '[ "$(qa_code POST "/v1/orgs/$2/projects" "$1" "{\"id\":\"not-a-uuid\",\"slug\":\"q\",\"name\":\"Q\"}")" = 400 ]' \
	_ "$A_TOK" "$ORG"
assert_green "GET /v1/orgs/{org} resolves a slug to its canonical id" \
	-- bash -c 'r=$(qa_api GET "/v1/orgs/$2" "$1"); grep -q "\"id\"" <<<"$r"' _ "$A_TOK" "$ORG"

E_ID="$(qa_json "$(qa_api POST "/v1/projects/$PUUID/environments" "$A_TOK" '{"name":"prod"}' | cut -f2-)" id)"
E2_ID="$(qa_json "$(qa_api POST "/v1/projects/$PUUID/environments" "$A_TOK" '{"name":"staging"}' | cut -f2-)" id)"
assert_green "environments can be created and listed" -- bash -c '[ -n "$1" ] && [ -n "$2" ]' _ "$E_ID" "$E2_ID"

# ── who may publish the key everyone seals to ────────────────────────────────
# This is the sharpest authorization question in the whole bridge. Anyone who can
# publish a scope public key can make every writer seal to a key they hold.
assert_green "a project admin may publish a scope public key" \
	-- bash -c '[ "$(qa_code PUT "/v1/projects/$2/environments/prod/scopekey" "$1" "{\"scope_pubkey\":\"S1\",\"scope_epoch\":1}")" = 200 ]' \
	_ "$A_TOK" "$PUUID"
assert_green "a plain org member may NOT publish a scope public key" \
	-- bash -c 'c=$(qa_code PUT "/v1/projects/$2/environments/prod/scopekey" "$1" "{\"scope_pubkey\":\"EVIL\",\"scope_epoch\":2}"); [ "$c" = 403 ] || [ "$c" = 404 ]' \
	_ "$M_TOK" "$PUUID"

# ── a key epoch must never go backwards ──────────────────────────────────────
# Accepting an older epoch would let an attacker re-point an environment at a key
# whose secret they already hold, and every subsequent write would be readable by them.
assert_green "publishing a higher epoch is accepted" \
	-- bash -c '[ "$(qa_code PUT "/v1/projects/$2/environments/prod/scopekey" "$1" "{\"scope_pubkey\":\"S2\",\"scope_epoch\":2}")" = 200 ]' \
	_ "$A_TOK" "$PUUID"
# 409 rather than 400: this is a conflict with the environment's current state, not a
# malformed request, and the distinction is worth pinning so it cannot drift.
assert_green "re-publishing the same epoch is refused as a conflict" \
	-- bash -c '[ "$(qa_code PUT "/v1/projects/$2/environments/prod/scopekey" "$1" "{\"scope_pubkey\":\"S2b\",\"scope_epoch\":2}")" = 409 ]' \
	_ "$A_TOK" "$PUUID"
assert_green "rolling the epoch backwards is refused as a conflict" \
	-- bash -c '[ "$(qa_code PUT "/v1/projects/$2/environments/prod/scopekey" "$1" "{\"scope_pubkey\":\"OLD\",\"scope_epoch\":1}")" = 409 ]' \
	_ "$A_TOK" "$PUUID"

# ── member public keys ───────────────────────────────────────────────────────
# A registration carries a proof that the registrant holds the private half. Forging it
# would let somebody register their own key under another account and then be handed the
# scope secret by the next reconcile, so the boundary check matters more than the route.
assert_green "a registration with an invalid proof of possession is refused" \
	-- bash -c '[ "$(qa_code PUT "/v1/orgs/$2/pubkey" "$1" "{\"x25519_pub\":\"eA==\",\"ed25519_pub\":\"ZQ==\",\"v42_address\":\"v42:z\",\"pubkey_sig\":\"cw==\"}")" = 400 ]' \
	_ "$A_TOK" "$ORG"

# Enrolling through the CLI produces a real proof. Doing it by the org SLUG is the
# regression test for a bug where the client signed whatever the user typed while the
# authority verified the canonical id: those bytes never match, so every proof would be
# refused and every member would be silently skipped as unverifiable.
qa_actor_token alice "$A_TOK"
assert_green "a member can enroll using the org slug" \
	-- qa_actor alice "$WORK" "keys enroll --org $ORG"
assert_green "a member can enroll using the canonical org id" \
	-- bash -c 'oid=$(qa_json "$(qa_api GET "/v1/orgs/$2" "$1" | cut -f2-)" id); qa_actor alice "$3" "keys enroll --org $oid"' \
	_ "$A_TOK" "$ORG" "$WORK"
assert_green "the enrolled public keys can be read back" \
	-- bash -c '[ "$(qa_code GET "/v1/orgs/$2/users/$3/pubkey" "$1")" = 200 ]' _ "$A_TOK" "$ORG" "$A_ID"

# ── the authorized set, which decides who receives the scope secret ──────────
G_ID="$(qa_json "$(qa_api POST "/v1/orgs/$ORG/projects/$PUUID/grants" "$A_TOK" "{\"grantee_kind\":\"user\",\"grantee_id\":\"$M_ID\",\"project_role\":\"write\",\"env_id\":\"$E_ID\"}" | cut -f2-)" id)"
assert_green "a grant can be created and returns an id" -- bash -c '[ -n "$1" ]' _ "$G_ID"
assert_green "grants can be listed for a project" \
	-- bash -c '[ "$(qa_code GET "/v1/orgs/$2/projects/$3/grants" "$1")" = 200 ]' _ "$A_TOK" "$ORG" "$PUUID"

# An account that was never granted must never appear in the set an administrator
# wraps the scope secret to. A name that leaks into this list receives the key.
O_ID="$(qa_signup "out24-$N@archicode.codes" "pw-o24-$N")"
# A wrap only means something for one environment at one epoch. Answering without
# those coordinates would describe a scope the caller is not provisioning, which is how
# a member could be reported as provisioned somewhere they cannot actually read.
assert_green "asking who is missing without an environment and epoch is refused" \
	-- bash -c '[ "$(qa_code GET "/v1/orgs/$2/projects/$3/grants/$4/fulfilled" "$1")" = 400 ]' \
	_ "$A_TOK" "$ORG" "$PUUID" "$G_ID"

FULFILLED="/v1/orgs/$ORG/projects/$PUUID/grants/$G_ID/fulfilled?env_id=$E_ID&epoch=1"
assert_green "asking with both coordinates succeeds" \
	-- bash -c '[ "$(qa_code GET "$2" "$1")" = 200 ]' _ "$A_TOK" "$FULFILLED"
# Rotation re-wraps the authorized set, not the worklist, so the two must be separately
# readable. Returning only "missing" is what made rotation drop everyone.
assert_green "the answer carries the authorized members as well as the missing ones" \
	-- bash -c 'r=$(qa_api GET "$2" "$1"); grep -q "members" <<<"$r" && grep -q "missing" <<<"$r"' _ "$A_TOK" "$FULFILLED"
assert_green "an account with no grant never appears in the missing set" \
	-- bash -c 'r=$(qa_api GET "$2" "$1"); [ "${r%%	*}" = 200 ] && ! grep -qF "$3" <<<"$r"' \
	_ "$A_TOK" "$FULFILLED" "$O_ID"
assert_green "the granted member does appear in the missing set before provisioning" \
	-- bash -c 'r=$(qa_api GET "$2" "$1"); [ "${r%%	*}" = 200 ] && grep -qF "$3" <<<"$r"' \
	_ "$A_TOK" "$FULFILLED" "$M_ID"

# Recording a wrap for an account the grant does not authorize would let an
# administrator mark an outsider as provisioned, and rotation reads this same set.
assert_green "recording a wrap for a non-authorized account is refused" \
	-- bash -c '[ "$(qa_code POST "/v1/orgs/$2/projects/$3/grants/$4/wraps" "$1" "{\"user_id\":\"$5\",\"env_id\":\"$6\",\"epoch\":1}")" != 200 ]' \
	_ "$A_TOK" "$ORG" "$PUUID" "$G_ID" "$O_ID" "$E_ID"
assert_green "recording a wrap for an environment the grant does not cover is refused" \
	-- bash -c '[ "$(qa_code POST "/v1/orgs/$2/projects/$3/grants/$4/wraps" "$1" "{\"user_id\":\"$5\",\"env_id\":\"$6\",\"epoch\":1}")" != 200 ]' \
	_ "$A_TOK" "$ORG" "$PUUID" "$G_ID" "$M_ID" "$E2_ID"

spec_end
