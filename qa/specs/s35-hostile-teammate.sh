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
PUUID="35353535-6767-4898-9abc-$(qa_uuid_tail)"

for who in alice mallory victim; do qa_actor_reset "$who"; done
A_ID="$(qa_actor_account alice "a35-$N@archicode.codes" "pw-a-$N")"; A_TOK="$(cat "$(qa_actor_dir alice)/session.tok")"
M_ID="$(qa_actor_account mallory "m35-$N@archicode.codes" "pw-m-$N")"
V_ID="$(qa_actor_account victim "v35-$N@archicode.codes" "pw-v-$N")"

qa_api POST /v1/orgs "$A_TOK" "{\"slug\":\"$ORG\",\"name\":\"O35\"}" >/dev/null
qa_api POST "/v1/orgs/$ORG/projects" "$A_TOK" "{\"id\":\"$PUUID\",\"slug\":\"p\",\"name\":\"P\"}" >/dev/null
qa_api POST "/v1/projects/$PUUID/environments" "$A_TOK" '{"name":"prod"}' >/dev/null
for pair in "mallory:m35" "victim:v35"; do
	who="${pair%%:*}"; pfx="${pair#*:}"
	t="$(qa_json "$(qa_api POST "/v1/orgs/$ORG/invites" "$A_TOK" "{\"email\":\"$pfx-$N@archicode.codes\",\"role\":\"member\"}" | cut -f2-)" token)"
	qa_api POST /v1/orgs/invites/accept "$(cat "$(qa_actor_dir "$who")/session.tok")" "{\"token\":\"$t\"}" >/dev/null
done
for id in "$M_ID" "$V_ID"; do
	qa_api POST "/v1/orgs/$ORG/projects/$PUUID/grants" "$A_TOK" \
		"{\"grantee_kind\":\"user\",\"grantee_id\":\"$id\",\"project_role\":\"write\"}" >/dev/null
done
for who in alice mallory victim; do qa_actor "$who" "$W" "keys enroll --org $ORG" >/dev/null 2>&1; done
qa_actor alice "$W" "vault env-init --org $ORG --project $PUUID --env prod" >/dev/null 2>&1
qa_actor alice "$W" "vault sync-keys --org $ORG --project $PUUID --env prod" >/dev/null 2>&1
qa_s3_up >/dev/null 2>&1
for who in alice mallory victim; do
	qa_actor "$who" "$W" "config endpoint --blobstore $(qa_s3_internal) --bucket $QA_S3_BUCKET" >/dev/null 2>&1
done

# An honest tree first, so every refusal below is measured against a pull that works.
printf 'DOMAIN_NAME=s35.42.fr\n' >"$W/src/srcs/.env"
printf 'honest-key-material-0001\n' >"$W/src/secrets/server.key"
fixture_varied_file "$W/src/volume.bin" 8 "QA42-S35-CHUNKED"
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

# ── poisoning the deduplication table ───────────────────────────────────────
# Two members sharing an environment share its chunk store, and a chunk is named by the keyed
# hash of its bytes. A writer who stores unrelated bytes under a name they computed
# dishonestly poisons every later writer of that content: the honest writer finds the name
# already present, stores nothing, and their restore returns the poisoner's bytes. The
# envelope is validly sealed, validly signed and bound to the right secret id, so nothing else
# in the system notices.
#
# The store credential is the same for everyone here, which is exactly the situation on a team.
#
# Substituting a whole stored object is refused by the secret id, which is bound to the name it
# was sealed for — a mechanism that fires before the content check. The content check defends a
# case this cannot reach from a shell: a chunk sealed correctly FOR its name whose plaintext
# hashes to something else, which needs a dishonest client rather than a copied object. That
# half is unit-tested beside the code; this half proves the store cannot be shuffled.
# From an empty store, so every chunk in it belongs to THIS environment. The bucket is shared
# across the battery, and picking "the first two objects" out of a shared bucket picks two
# chunks of somebody else's tree — which the pull below never touches, so the attack lands
# nowhere and the assertion passes having tested nothing.
qa_mc "mc rm --recursive --force qa/$QA_S3_BUCKET" >/dev/null 2>&1
assert_green "the honest tree is republished so there is a chunked entry to attack" \
	-- bash -c 'out=$(qa_actor alice "$1" "vault push-env --org $2 --project $3 --env prod" 2>&1) || { printf "%s\n" "$out" | tail -3; exit 1; }
		grep -q "chunk(s) in the object store" <<<"$out"' _ "$W/src" "$ORG" "$PUUID"
assert_green "and it restores byte-exact before anything is poisoned" \
	-- bash -c 'rm -rf "$1"; mkdir -p "$1"
		qa_actor victim "$1" "vault pull-env --org $3 --project $4 --env prod --apply" >/dev/null 2>&1
		cmp -s "$2/volume.bin" "$1/volume.bin"' _ "$W/victim-pre-poison" "$W/src" "$ORG" "$PUUID"
assert_green "a chunk whose name does not match its bytes is refused on read" \
	-- bash -c 'names=$(qa_s3_names | grep "^chunks/" || true)
		victim=$(sed -n 1p <<<"$names")
		other=$(sed -n 2p <<<"$names")
		[ -n "$victim" ] && [ -n "$other" ] || { printf "fewer than two chunks, so nothing was poisoned\n"; exit 1; }
		qa_s3_substitute "$other" "$victim"
		rm -rf "$1"; mkdir -p "$1"
		out=$(qa_actor victim "$1" "vault pull-env --org $2 --project $3 --env prod --apply" 2>&1) && { printf "the poisoned chunk was accepted\n"; exit 1; }
		[ -z "$(find "$1" -name volume.bin 2>/dev/null)" ] || { printf "a poisoned file was written anyway\n"; exit 1; }' _ "$W/victim-poison" "$ORG" "$PUUID"

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
# Pinned to the PROPERTY, not to a line. The first version quoted one statement verbatim and
# went red the moment that statement was split in two, reporting an isolation failure that did
# not exist. What matters is that the scope comes from the caller's context and never from the
# manifest: no field of an entry may reach the scope id.
assert_green "the stored path a manifest names is resolved inside the caller's own environment" \
	-- bash -c 'body=$(sed "s|//.*||" "$1/src/cmd/scope_tree.rs")
		grep -q "crypto::scope_id(&ctx.project, &ctx.env_name)" <<<"$body" ||
			{ printf "the scope is no longer derived from the caller context\n"; exit 1; }
		! grep -qE "entry\.(scope|owner|env)" <<<"$body"' _ "$C42_ROOT"

# ── two writers at once ──────────────────────────────────────────────────────
# Not hostile, but the same requirement: the tree a colleague restores must be one somebody
# actually pushed. A shared environment has more than one writer by construction, so two
# pushes overlapping is ordinary rather than exotic, and the failure to avoid is a manifest
# naming files whose contents came from the other push — a tree that never existed on anyone's
# machine, restored without an error.
#
# Each file is stored with optimistic concurrency, so the losing writer should be REFUSED
# rather than merged. Both trees differ in every file, so a mixture is detectable.
mkdir -p "$W/racer-a" "$W/racer-b" "$W/race-restore"
for who in a b; do
	mkdir -p "$W/racer-$who/srcs"
	printf 'RACER=%s\nVALUE=race-value-%s-0001\n' "$who" "$who" >"$W/racer-$who/srcs/.env"
	printf 'RACER=%s\n' "$who" >"$W/racer-$who/marker.env"
	fixture_project_marker "$W/racer-$who" "race-$N" '"*"'
done
# Four rounds, because a race that happens to serialise once proves nothing about a race. Each
# round writes different content on both sides, so any mixture is visible in the restored tree.
#
# It holds for a structural reason rather than by luck, and the reason is worth recording so
# nobody removes the thing that makes it true. Files are pushed in sorted order, each with
# optimistic concurrency, and the MANIFEST is written last. So the first contended file acts as
# a lock: the writer that loses it aborts before reaching any later file and before writing any
# manifest, and the winner goes on to publish a tree entirely its own. Reordering the manifest
# to the front, or continuing past a conflicting file, would both break that — which is what
# this round-trips four times to catch.
assert_green "two writers pushing the same environment at once never produce a mixed tree" \
	-- bash -c 'for round in 1 2 3 4; do
			for who in a b; do
				printf "RACER=%s\nVALUE=race-%s-%s\n" "$who" "$who" "$round" >"$1/racer-$who/srcs/.env"
				printf "RACER=%s\nROUND=%s\n" "$who" "$round" >"$1/racer-$who/marker.env"
			done
			qa_actor alice "$1/racer-a" "vault push-env --org $2 --project $3 --env prod" >/dev/null 2>&1 &
			qa_actor mallory "$1/racer-b" "vault push-env --org $2 --project $3 --env prod" >/dev/null 2>&1 &
			wait
			d="$1/race-restore"; rm -rf "$d"; mkdir -p "$d"
			qa_actor victim "$d" "vault pull-env --org $2 --project $3 --env prod --apply" >/dev/null 2>&1 ||
				{ printf "round %s: nothing could be restored after the race\n" "$round"; exit 1; }
			one=$(sed -n "s/^RACER=//p" "$d/srcs/.env" 2>/dev/null)
			two=$(sed -n "s/^RACER=//p" "$d/marker.env" 2>/dev/null)
			[ -n "$one" ] && [ -n "$two" ] ||
				{ printf "round %s: the restored tree is incomplete\n" "$round"; exit 1; }
			[ "$one" = "$two" ] ||
				{ printf "round %s: MIXED — srcs/.env from %s, marker.env from %s\n" "$round" "$one" "$two"; exit 1; }
		done' \
	_ "$W" "$ORG" "$PUUID"

spec_end
