#!/usr/bin/env bash
# s38 — private files inside a shared environment.
#
# `push-env` shares a tree with everyone the environment is granted to. Some of what sits in
# a project tree is one person's: a `.env.local`, a personal key. Pushing it shared would hand
# it to the team; leaving it out means the next machine has to be told about it by hand. So a
# file can travel WITH the tree while being sealed to its pusher alone: stored under the same
# environment, named only in a second manifest that is sealed the same way.
#
# What must hold, from each perspective:
#   the teammate     restores every shared file and never sees the private one — not its
#                    bytes, and not its PATH (their listing does not contain it);
#   the pusher       restores both, from an empty tree, byte-exact;
#   the server       holds neither the plaintext nor the real path (zero-knowledge);
#   a private copy   wins over a shared file at the same path, and says so;
#   a writer         cannot plant a "private" manifest for somebody else and have it acted on.
# Every absence assertion here is preceded by a positive control over the same haystack.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"
source "$QA_LIB_DIR/fixtures.sh"

spec_begin "s38-private-in-scope"
qa_require_docker_stack
qa_require_cmd curl
qa_require_cmd python3
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
W="$QA_RESULTS/s38"
rm -rf "$W"
mkdir -p "$W/src/srcs" "$W/src/secrets" "$W/bob" "$W/alice-restore"
ORG="org38-$N"
PUUID="38383838-4242-4787-89ab-$(qa_uuid_tail)"
SENTINEL="alice-only-sentinel-$N"

for who in alice bob; do qa_actor_reset "$who"; done
qa_actor_account alice "a38-$N@archicode.codes" "pw-a-$N" >/dev/null; A_TOK="$(cat "$(qa_actor_dir alice)/session.tok")"
B_ID="$(qa_actor_account bob "b38-$N@archicode.codes" "pw-b-$N")"
qa_api POST /v1/orgs "$A_TOK" "{\"slug\":\"$ORG\",\"name\":\"O38\"}" >/dev/null
qa_api POST "/v1/orgs/$ORG/projects" "$A_TOK" "{\"id\":\"$PUUID\",\"slug\":\"p\",\"name\":\"P\"}" >/dev/null
qa_api POST "/v1/projects/$PUUID/environments" "$A_TOK" '{"name":"prod"}' >/dev/null
t="$(qa_json "$(qa_api POST "/v1/orgs/$ORG/invites" "$A_TOK" "{\"email\":\"b38-$N@archicode.codes\",\"role\":\"member\"}" | cut -f2-)" token)"
qa_api POST /v1/orgs/invites/accept "$(cat "$(qa_actor_dir bob)/session.tok")" "{\"token\":\"$t\"}" >/dev/null
qa_api POST "/v1/orgs/$ORG/projects/$PUUID/grants" "$A_TOK" \
	"{\"grantee_kind\":\"user\",\"grantee_id\":\"$B_ID\",\"project_role\":\"write\"}" >/dev/null
for who in alice bob; do qa_actor "$who" "$W" "keys enroll --org $ORG" >/dev/null 2>&1; done
qa_actor alice "$W" "vault env-init --org $ORG --project $PUUID --env prod" >/dev/null 2>&1
qa_actor alice "$W" "vault sync-keys --org $ORG --project $PUUID --env prod" >/dev/null 2>&1
V="--org $ORG --project $PUUID --env prod"

# Two shared files, two private: one by the `*.local` default, one by an explicit pattern.
printf 'DOMAIN_NAME=s38.42.fr\n' >"$W/src/srcs/.env"
printf 'LOCAL_SENTINEL=%s\n' "$SENTINEL" >"$W/src/srcs/.env.local"
printf 'shared-key-material-0001\n' >"$W/src/secrets/server.key"
printf 'alice-personal-key-0001\n' >"$W/src/secrets/me.key"
chmod 600 "$W/src/secrets/server.key" "$W/src/secrets/me.key"
fixture_project_marker "$W/src" "s38-$N" '"*"'
assert_green "alice pushes: two files shared, two sealed to her alone, all labelled" \
	-- bash -c 'out=$(qa_actor alice "$1" "vault push-env $2 --private secrets/me.\* --label app=s38" 2>&1) || { printf "%s\n" "$out" | tail -3; exit 1; }
		grep -q "pushed 2 shared and 2 private" <<<"$out" || { printf "%s\n" "$out" | tail -3; exit 1; }' _ "$W/src" "$V"

# ── the teammate's perspective ───────────────────────────────────────────────
assert_green "bob restores the shared files byte-exact — the control for the absences below" \
	-- bash -c 'qa_actor bob "$2" "vault pull-env $3 --apply" >/dev/null 2>&1
		cmp -s "$1/srcs/.env" "$2/srcs/.env" && cmp -s "$1/secrets/server.key" "$2/secrets/server.key"' \
	_ "$W/src" "$W/bob" "$V"
assert_green "bob receives neither private file" \
	-- bash -c '[ ! -e "$1/srcs/.env.local" ] && [ ! -e "$1/secrets/me.key" ]' _ "$W/bob"
assert_green "bob's listing names a shared path (so the haystack is real)" \
	-- bash -c 'qa_actor bob "$1" "vault ls-env $2 --format {{.Path}}" 2>/dev/null | grep -qx "srcs/.env"' _ "$W/bob" "$V"
assert_green "and never names a private one — a teammate does not learn the path either" \
	-- bash -c 'out=$(qa_actor bob "$1" "vault ls-env $2 --format {{.Path}}" 2>/dev/null)
		! grep -q "env.local\|me.key" <<<"$out"' _ "$W/bob" "$V"

# ── the server's perspective ─────────────────────────────────────────────────
qa_dump_server_db "$W/server.dump" >/dev/null 2>&1
assert_zero_knowledge "the private file's plaintext never reached the server" \
	"$SENTINEL" "__42ctl/p/" "$W/server.dump"
assert_zero_knowledge "nor its real path" \
	".env.local" "__42ctl/p/" "$W/server.dump"

# ── the pusher's perspective ─────────────────────────────────────────────────
assert_green "alice restores shared AND private into an empty tree, byte-exact" \
	-- bash -c 'qa_actor alice "$2" "vault pull-env $3 --apply" >/dev/null 2>&1
		for f in srcs/.env srcs/.env.local secrets/server.key secrets/me.key; do
			cmp -s "$1/$f" "$2/$f" || { printf "differs or missing: %s\n" "$f"; exit 1; }
		done' _ "$W/src" "$W/alice-restore" "$V"
assert_green "her listing flags exactly the private files" \
	-- bash -c 'got=$(qa_actor alice "$1" "vault ls-env $2 --filter Private=true --format {{.Path}}" 2>/dev/null)
		[ "$got" = "$(printf "secrets/me.key\nsrcs/.env.local")" ] || { printf "got:\n%s\n" "$got"; exit 1; }' _ "$W/alice-restore" "$V"
assert_green "a label filter selects every file of that push, and a wrong one is an error" \
	-- bash -c 'n=$(qa_actor alice "$1" "vault ls-env $2 --filter label=app=s38 --format {{.Path}}" 2>/dev/null | wc -l)
		[ "$n" -eq 4 ] || { printf "expected 4 labelled rows, got %s\n" "$n"; exit 1; }
		! qa_actor alice "$1" "vault ls-env $2 --filter label=app=nope" >/dev/null 2>&1' _ "$W/alice-restore" "$V"
assert_green "the JSON form parses and carries Private, Size and Labels per row" \
	-- bash -c 'qa_actor alice "$1" "vault ls-env $2 --format json" 2>/dev/null | python3 -c "
import json, sys
rows = json.load(sys.stdin)
assert len(rows) == 4, rows
assert sum(r[\"Private\"] for r in rows) == 2, rows
assert all(r[\"Labels\"][\"app\"] == \"s38\" and r[\"Size\"] > 0 for r in rows), rows"' _ "$W/alice-restore" "$V"

# ── a private copy shadows a shared file at the same path ────────────────────
# Bob pushes a tree that puts a SHARED secrets/me.key where alice keeps a private one. Her
# restore must give her HER bytes, and must say the shared copy was hidden rather than
# silently choosing either.
mkdir -p "$W/bob-src/srcs" "$W/bob-src/secrets" "$W/alice-shadow"
printf 'DOMAIN_NAME=bob.42.fr\n' >"$W/bob-src/srcs/.env"
printf 'bob-put-a-shared-file-here\n' >"$W/bob-src/secrets/me.key"
fixture_project_marker "$W/bob-src" "s38-$N" '"*"'
assert_green "bob publishes a shared file at the path alice keeps private" \
	-- bash -c 'qa_actor bob "$1" "vault push-env $2" >/dev/null 2>&1' _ "$W/bob-src" "$V"
assert_green "alice's restore keeps her private copy, takes bob's other file, and reports the shadow" \
	-- bash -c 'out=$(qa_actor alice "$3" "vault pull-env $4 --apply" 2>&1) || { printf "%s\n" "$out" | tail -3; exit 1; }
		cmp -s "$1/secrets/me.key" "$3/secrets/me.key" || { printf "the shared copy overrode the private one\n"; exit 1; }
		cmp -s "$2/srcs/.env" "$3/srcs/.env" || { printf "bob'"'"'s shared srcs/.env was not restored\n"; exit 1; }
		grep -q "shadows" <<<"$out" || { printf "the shadow was not reported:\n%s\n" "$out"; exit 1; }' \
	_ "$W/src" "$W/bob-src" "$W/alice-shadow" "$V"

# ── an oversized private file is refused before anything is uploaded ─────────
mkdir -p "$W/big/srcs" "$W/bob-after"
printf 'DOMAIN_NAME=after-the-refusal\n' >"$W/big/srcs/.env"
fixture_sized_file "$W/big/huge.local" 8388608
fixture_project_marker "$W/big" "s38-$N" '"*"'
assert_green "a private file above the ceiling makes the push fail, by name" \
	-- bash -c 'out=$(qa_actor alice "$1" "vault push-env $2" 2>&1) && { printf "the push succeeded\n"; exit 1; }
		grep -q "huge.local" <<<"$out"' _ "$W/big" "$V"
assert_green "and nothing of that push reached the environment — bob still restores the previous tree" \
	-- bash -c 'qa_actor bob "$2" "vault pull-env $3 --apply" >/dev/null 2>&1
		cmp -s "$1/srcs/.env" "$2/srcs/.env"' _ "$W/bob-src" "$W/bob-after" "$V"

# ── a "private" manifest planted by somebody else is not acted on ────────────
# Bob may write the environment, so he may write at alice's private manifest path. What he
# writes is sealed to the ENVIRONMENT, not to alice, and authored by him, not by her. Either
# is enough to refuse; the restore must fail closed rather than restore what he planted.
A_P="$(qa_actor alice "$W" "auth whoami" 2>/dev/null | awk '$1 == "principal" { print $2 }')"
mkdir -p "$W/alice-planted"
printf '{"version":2,"project_id":"s38","entries":[{"relative_path":"planted.txt","vault_path":"__42ctl/tree","mode":384,"kind":1,"chunked":false,"rev":1}]}' \
	>"$W/evil-private.json"
assert_green "bob can write at alice's private manifest path — he is a legitimate writer" \
	-- bash -c '[ -n "$4" ] || { printf "no principal, so no path to plant at\n"; exit 1; }
		qa_actor bob "$1" "vault set-env $2 __42ctl/p/$4/tree < /project/evil-private.json" >/dev/null 2>&1' \
	_ "$W" "$V" _ "$A_P"
assert_green "alice's restore refuses the planted manifest and writes nothing from it" \
	-- bash -c '! qa_actor alice "$1" "vault pull-env $2 --apply" >/dev/null 2>&1 || { printf "the planted manifest was accepted\n"; exit 1; }
		[ ! -e "$1/planted.txt" ]' _ "$W/alice-planted" "$V"

spec_end
