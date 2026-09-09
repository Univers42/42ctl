#!/usr/bin/env bash
# s35 — what a teammate can do to a colleague's machine.
#
# Sharing a tree with an environment created an attack surface that sharing a single value
# did not. `push-env` writes a MANIFEST that other people's machines then act on: it names
# the paths they write to and the modes they write with. Anyone who may write to the
# environment may write that manifest, and on a team that is not one person.
#
# So the threat is no longer only the server. It is a colleague — or anyone who has taken a
# colleague's session — steering where a pull lands. The pulling client is the only thing
# standing between a manifest and the filesystem, and it must treat that manifest as hostile
# input even though it arrived sealed and authenticated: a legitimate writer authenticates
# perfectly well.
#
# Every attack here is planted through the ORDINARY verb, `set-env` at the reserved path, so
# it is what a real writer can actually do rather than a shape invented for the test.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"
source "$QA_LIB_DIR/fixtures.sh"

spec_begin "s35-hostile-teammate"
qa_require_docker_stack
qa_require_cmd curl
trap 'qa_server_cleanup; [ "${QA_KEEP_SERVER:-0}" = "1" ] || qa_authority_down' EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
assert_green "the authority is listening" -- qa_authority_up
[ "$_qa_regression" -eq 0 ] || {
	spec_end
	exit 1
}

N="$$-$(date +%s)"
W="$QA_RESULTS/s35"
rm -rf "$W"
mkdir -p "$W" "$W/src/srcs" "$W/src/secrets" "$W/victim"
ORG="org35-$N"
PUUID="35353535-6767-4898-9abc-$(printf '%012d' $$)"

for who in alice mallory victim; do qa_actor_reset "$who"; done
A_ID="$(qa_actor_account alice "a35-$N@archicode.codes" "pw-a-$N")"; A_TOK="$(cat "$QA_RESULTS/actors/alice/session.tok")"
M_ID="$(qa_actor_account mallory "m35-$N@archicode.codes" "pw-m-$N")"
V_ID="$(qa_actor_account victim "v35-$N@archicode.codes" "pw-v-$N")"

qa_api POST /v1/orgs "$A_TOK" "{\"slug\":\"$ORG\",\"name\":\"O35\"}" >/dev/null
qa_api POST "/v1/orgs/$ORG/projects" "$A_TOK" "{\"id\":\"$PUUID\",\"slug\":\"p\",\"name\":\"P\"}" >/dev/null
qa_api POST "/v1/projects/$PUUID/environments" "$A_TOK" '{"name":"prod"}' >/dev/null
for pair in "mallory:m35" "victim:v35"; do
	who="${pair%%:*}"; pfx="${pair#*:}"
	t="$(qa_json "$(qa_api POST "/v1/orgs/$ORG/invites" "$A_TOK" "{\"email\":\"$pfx-$N@archicode.codes\",\"role\":\"member\"}" | cut -f2-)" token)"
	qa_api POST /v1/orgs/invites/accept "$(cat "$QA_RESULTS/actors/$who/session.tok")" "{\"token\":\"$t\"}" >/dev/null
done
for id in "$M_ID" "$V_ID"; do
	qa_api POST "/v1/orgs/$ORG/projects/$PUUID/grants" "$A_TOK" \
		"{\"grantee_kind\":\"user\",\"grantee_id\":\"$id\",\"project_role\":\"write\"}" >/dev/null
done
for who in alice mallory victim; do qa_actor "$who" "$W" "keys enroll --org $ORG" >/dev/null 2>&1; done
qa_actor alice "$W" "vault env-init --org $ORG --project $PUUID --env prod" >/dev/null 2>&1
qa_actor alice "$W" "vault sync-keys --org $ORG --project $PUUID --env prod" >/dev/null 2>&1

# An honest tree first, so every refusal below is measured against a pull that works.
printf 'DOMAIN_NAME=s35.42.fr\n' >"$W/src/srcs/.env"
printf 'honest-key-material-0001\n' >"$W/src/secrets/server.key"
chmod 600 "$W/src/secrets/server.key"
fixture_project_marker "$W/src" "s35-$N" '"*"'
assert_green "alice publishes an honest tree to the environment" \
	-- bash -c 'qa_actor alice "$1" "vault push-env --org $2 --project $3 --env prod" >/dev/null 2>&1' \
	_ "$W/src" "$ORG" "$PUUID"
assert_green "the victim restores it normally, which is the control for everything below" \
	-- bash -c 'qa_actor victim "$2" "vault pull-env --org $3 --project $4 --env prod --apply" >/dev/null 2>&1
		cmp -s "$1/srcs/.env" "$2/srcs/.env"' _ "$W/src" "$W/victim" "$ORG" "$PUUID"

# ── a manifest that points outside the project root ──────────────────────────
# Mallory may write to this environment, so she may write the manifest the victim acts on.
# The stored path is the one thing a puller must never trust, however well authenticated.
ESCAPE="$W/escape-target.env"
rm -f "$ESCAPE"
printf '{"version":2,"project_id":"s35","entries":[{"relative_path":"../escape-target.env","vault_path":"__42ctl/tree","mode":384,"kind":1,"chunked":false,"rev":1}]}' \
	>"$W/evil-traversal.json"
assert_green "mallory can write the manifest — she is a legitimate writer" \
	-- bash -c 'qa_actor mallory "$1" "vault set-env --org $2 --project $3 --env prod __42ctl/tree < /project/evil-traversal.json" >/dev/null 2>&1' \
	_ "$W" "$ORG" "$PUUID"
assert_green "a manifest pointing outside the project root writes nothing outside it" \
	-- bash -c 'qa_actor victim "$1" "vault pull-env --org $2 --project $3 --env prod --apply" >/dev/null 2>&1
		[ ! -e "$4" ] || { printf "the traversal landed at %s\n" "$4"; exit 1; }' \
	_ "$W/victim" "$ORG" "$PUUID" "$ESCAPE"
assert_green "and it refuses rather than reporting a restore" \
	-- bash -c '! qa_actor victim "$1" "vault pull-env --org $2 --project $3 --env prod --apply" >/dev/null 2>&1' \
	_ "$W/victim" "$ORG" "$PUUID"

# ── a manifest that widens the mode of a private key ─────────────────────────
# 511 is 0777. A key restored world-readable is a key leaked to every process on the box,
# and nothing about the bytes is wrong, so no integrity check would notice.
printf '{"version":2,"project_id":"s35","entries":[{"relative_path":"secrets/server.key","vault_path":"__42ctl/tree","mode":511,"kind":1,"chunked":false,"rev":1}]}' \
	>"$W/evil-mode.json"
assert_green "mallory publishes a manifest marking the private key world-readable" \
	-- bash -c 'qa_actor mallory "$1" "vault set-env --org $2 --project $3 --env prod __42ctl/tree < /project/evil-mode.json" >/dev/null 2>&1' \
	_ "$W" "$ORG" "$PUUID"
assert_green "a restored file is never given a mode wider than its owner" \
	-- bash -c 'rm -rf "$1"; mkdir -p "$1"
		qa_actor victim "$1" "vault pull-env --org $2 --project $3 --env prod --apply" >/dev/null 2>&1
		m=$(stat -c %a "$1/secrets/server.key" 2>/dev/null || printf none)
		[ "$m" != none ] || { printf "nothing was restored, so the mode proves nothing\n"; exit 1; }
		[ $((0$m & 0077)) -eq 0 ] || { printf "restored group- or world-accessible: %s\n" "$m"; exit 1; }' \
	_ "$W/victim-mode" "$ORG" "$PUUID"

# ── an absolute path, which is traversal without the dots ───────────────────
printf '{"version":2,"project_id":"s35","entries":[{"relative_path":"/tmp/s35-absolute-target","vault_path":"__42ctl/tree","mode":384,"kind":1,"chunked":false,"rev":1}]}' \
	>"$W/evil-absolute.json"
rm -f /tmp/s35-absolute-target
assert_green "an absolute path in the manifest writes nothing at that path" \
	-- bash -c 'qa_actor mallory "$1" "vault set-env --org $2 --project $3 --env prod __42ctl/tree < /project/evil-absolute.json" >/dev/null 2>&1
		rm -rf "$4"; mkdir -p "$4"
		qa_actor victim "$4" "vault pull-env --org $2 --project $3 --env prod --apply" >/dev/null 2>&1
		[ ! -e /tmp/s35-absolute-target ]' _ "$W" "$ORG" "$PUUID" "$W/victim-abs"

# ── the manifest cannot reach outside its own environment ───────────────────
# Every fetch uses the scope the CALLER asked for, never one the manifest names, so a
# manifest cannot be used to pull another environment's secrets into this tree. Asserted
# because it is a property of where the scope id comes from rather than of any check, and
# that is exactly the kind of property that survives until somebody makes it configurable.
assert_green "the stored path a manifest names is resolved inside the caller's own environment" \
	-- bash -c 'grep -q "let owner = hex::encode(crypto::scope_id" "$1/src/cmd/scope_tree.rs" &&
		! grep -qE "entry\.(scope|owner)" "$1/src/cmd/scope_tree.rs"' _ "$C42_ROOT"

spec_end
