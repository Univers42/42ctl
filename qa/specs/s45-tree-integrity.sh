#!/usr/bin/env bash
# s45 — an environment's tree is always one somebody pushed, and a key rotation keeps all of it.
#
# s35 races two pushes four times and hopes the timing lands. It usually does not, so a tree
# that mixed both pushes went unseen for weeks: restore fetched each file's NEWEST version, and a
# push that loses — or dies — after writing some files leaves newer versions the manifest never
# names. Here the interleavings are made to happen rather than hoped for:
#
#   · a push that dies partway, at an unreachable object store, after writing the files before it;
#   · a push held open at the object store (the container is paused) while a teammate's push
#     lands, then released.
#
# And rotation, which re-seals an environment to a new key, had only ever been run over single
# secrets. A tree holds three other kinds of thing: chunks in the object store, named and sealed
# with the environment's key; each member's private files, sealed to that member alone, which an
# administrator can neither open nor re-author; and a manifest pinning revisions. Every kind is
# pushed, rotated, and restored byte-exact by the people it belongs to.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"
source "$QA_LIB_DIR/fixtures.sh"

spec_begin "s45-tree-integrity"
qa_require_docker_stack
qa_require_cmd python3
trap 'docker unpause "$QA_S3_SRV" >/dev/null 2>&1; qa_server_cleanup; [ "${QA_KEEP_SERVER:-0}" = "1" ] || qa_authority_down' EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
assert_green "the authority is listening" -- qa_authority_up
assert_green "an object store is available for chunks" -- qa_s3_up
[ "$_qa_regression" -eq 0 ] || {
	spec_end
	exit 1
}

N="$$$(date +%s)"
W="$QA_RESULTS/s45"
rm -rf "$W"
ORG="tree-$N"
for who in ada ben cal; do
	qa_actor_reset "$who"
	mkdir -p "$W/$who"
done
ADA="$W/ada/tree"
BEN="$W/ben/tree"
E="--org $ORG --project api --env prod"
export N W ORG ADA BEN E

# act <who> <42ctl args as one string>, from the actor's own directory
act() {
	local who="$1"
	shift
	QA_ACCOUNT_PASSWORD="pw-$who-$N" qa_actor "$who" "$W/$who" "$*"
}
# in_tree <who> <dir> <42ctl args as one string>, from a tree
in_tree() { QA_ACCOUNT_PASSWORD="pw-$1-$N" qa_actor "$1" "$2" "$3"; }
mail() { printf '%s-%s@archicode.codes' "$1" "$N"; }
field() { awk -v k="$1" '$1 == k { print $2; exit }'; }
fresh() { rm -rf "$1" && mkdir -p "$1"; }
# same <want dir> <got dir> <relative path>... — byte-exact, naming the first file that is not
same() {
	local want="$1" got="$2" rel
	shift 2
	for rel in "$@"; do
		cmp -s "$want/$rel" "$got/$rel" || {
			printf '%s differs: want %q, got %q\n' "$rel" "$(head -c 60 "$want/$rel" 2>/dev/null)" "$(head -c 60 "$got/$rel" 2>/dev/null)"
			return 1
		}
	done
}
# pull_into <who> <dir> [scope flags] — a whole restore into an empty directory, from prod unless told
pull_into() { fresh "$2" && in_tree "$1" "$2" "env pull ${3:-$E} --apply" >/dev/null 2>&1; }
# stored <who> <relative path> — the path the shared manifest stores a file at
stored() {
	act "$1" "env secret get __42ctl/tree $E" 2>/dev/null |
		python3 -c 'import json, sys; print(next(e["vault_path"] for e in json.load(sys.stdin)["entries"] if e["relative_path"] == sys.argv[1]))' "$2"
}
export -f act in_tree mail field fresh same pull_into stored

# ── the world ────────────────────────────────────────────────────────────────
for who in ada ben cal; do
	assert_green "$who signs up and logs in" \
		-- bash -c 'act "$1" "auth signup --email $(mail "$1")" >/dev/null 2>&1 &&
			act "$1" "auth login --password --email $(mail "$1")" >/dev/null 2>&1' _ "$who"
done
assert_green "ada founds an organisation" -- bash -c 'act ada "org create --slug $ORG --name Trees" >/dev/null 2>&1'
PROJECT="$(act ada "project create --org $ORG --slug api --name API" 2>/dev/null | field id)"
export PROJECT
assert_green "with a project and a prod environment" \
	-- bash -c '[ -n "$PROJECT" ] && act ada "env create --project $PROJECT --name prod" >/dev/null 2>&1'
for who in ben cal; do
	token="$(act ada "org invite --org $ORG --email $(mail "$who") --role member" 2>/dev/null | field token)"
	assert_green "$who is invited and joins" -- bash -c '[ -n "$2" ] && act "$1" "invite accept --token $2" >/dev/null 2>&1' _ "$who" "$token"
done
CAL_GRANT="$(act ada "project grant add --org $ORG --project $PROJECT --user $(mail cal) --role read --env prod" 2>/dev/null | field grant_id)"
export CAL_GRANT
assert_green "ben may write prod and cal may read it" \
	-- bash -c '[ -n "$CAL_GRANT" ] && act ada "project grant add --org $ORG --project $PROJECT --user $(mail ben) --role write --env prod" >/dev/null 2>&1'
assert_green "everyone publishes keys, prod gets its key, and it reaches everyone" \
	-- bash -c 'for who in ada ben cal; do act "$who" "keys enroll --org $ORG" >/dev/null 2>&1 || exit 1; done
		act ada "env init $E" >/dev/null 2>&1 && act ada "env keys sync $E" >/dev/null 2>&1'
assert_green "everyone can reach the object store" \
	-- bash -c 'for who in ada ben cal; do
			act "$who" "config endpoint --blobstore $(qa_s3_internal) --bucket $QA_S3_BUCKET" >/dev/null 2>&1 || exit 1
		done'

mkdir -p "$ADA/secrets" "$ADA/srcs" "$BEN/secrets" "$BEN/srcs"
printf 'ROUND=ada-1\n' >"$ADA/marker.env"
printf 'key-ada-1\n' >"$ADA/secrets/server.key"
printf 'SRC=ada-1\n' >"$ADA/srcs/.env"
printf 'ADA_LOCAL=only-ada-%s\n' "$N" >"$ADA/.env.local"
fixture_varied_file "$ADA/volume.bin" 8 "QA42-S45-ADA-1"
fixture_project_marker "$ADA" "s45-$N" '"*"'
SHARED="marker.env secrets/server.key srcs/.env volume.bin"
export SHARED

# ── a pull restores the revisions the manifest names ─────────────────────────
assert_green "ada pushes a tree of small files, a chunked volume and a private .env.local" \
	-- bash -c 'out="$(in_tree ada "$ADA" "env push $E" 2>&1)" || { printf "%s\n" "$out"; exit 1; }
		grep -q "pushed 4 shared and 1 private file" <<<"$out" || { printf "%s\n" "$out"; exit 1; }'
cp -a "$ADA" "$W/t-ada-1"
assert_green "ben restores it byte-exact, without ada's private file — the control for what follows" \
	-- bash -c 'pull_into ben "$W/pull/ben-1" && same "$W/t-ada-1" "$W/pull/ben-1" $SHARED && [ ! -e "$W/pull/ben-1/.env.local" ]'

# A push dies partway: volume.bin sorts last and is chunked, and ada's object store is pointed
# somewhere nothing listens, so every smaller file is written before the push fails. (A file it
# cannot read would do too, but a rootless container reads a mode-000 file regardless.)
printf 'ROUND=died\n' >"$ADA/marker.env"
printf 'key-died\n' >"$ADA/secrets/server.key"
fixture_varied_file "$ADA/volume.bin" 8 "QA42-S45-DIED"
act ada "config endpoint --blobstore http://127.0.0.1:9 --bucket $QA_S3_BUCKET" >/dev/null 2>&1
assert_green "a push whose object store is unreachable fails, and names the file it was pushing" \
	-- bash -c 'out="$(in_tree ada "$ADA" "env push $E" 2>&1)" && { printf "it succeeded:\n%s\n" "$out"; exit 1; }
		grep -q "volume.bin" <<<"$out" || { printf "%s\n" "$out"; exit 1; }'
act ada "config endpoint --blobstore $(qa_s3_internal) --bucket $QA_S3_BUCKET" >/dev/null 2>&1
assert_green "it had already written the files before that one — so the restore below has something to avoid" \
	-- bash -c 'at="$(stored ben marker.env)" && [ -n "$at" ] || exit 1
		[ "$(act ben "env secret get $at $E" 2>/dev/null)" = "ROUND=died" ]'
assert_green "a restore gives the last tree that finished, not the files of the push that died" \
	-- bash -c 'pull_into ben "$W/pull/ben-2" && same "$W/t-ada-1" "$W/pull/ben-2" $SHARED'
cp "$W/t-ada-1/secrets/server.key" "$ADA/secrets/server.key"

# ── two pushes that overlap ──────────────────────────────────────────────────
printf 'ROUND=ada-2\n' >"$ADA/marker.env"
printf 'SRC=ada-2\n' >"$ADA/srcs/.env"
fixture_varied_file "$ADA/volume.bin" 8 "QA42-S45-ADA-2"
cp -a "$ADA" "$W/t-ada-2"
printf 'ROUND=ben-1\n' >"$BEN/marker.env"
printf 'key-ben-1\n' >"$BEN/secrets/server.key"
printf 'SRC=ben-1\n' >"$BEN/srcs/.env"
fixture_project_marker "$BEN" "s45-$N" '"*"'
cp -a "$BEN" "$W/t-ben-1"
BEN_SHARED="marker.env secrets/server.key srcs/.env"
export BEN_SHARED

docker pause "$QA_S3_SRV" >/dev/null
(
	in_tree ada "$ADA" "env push $E" >"$W/overlap.out" 2>&1
	printf '%s' "$?" >"$W/overlap.code"
) &
HELD=$!
sleep 8
assert_green "ada's push is held open at the object store, so the overlap below is real" -- kill -0 "$HELD"
assert_green "meanwhile ben pushes a tree that needs no object store, and it lands" \
	-- bash -c 'out="$(in_tree ben "$BEN" "env push $E" 2>&1)" || { printf "%s\n" "$out"; exit 1; }'
docker unpause "$QA_S3_SRV" >/dev/null
wait "$HELD"
assert_green "ada's push, begun before ben's landed, is refused and says what to do" \
	-- bash -c '[ "$(cat "$W/overlap.code")" != 0 ] || { printf "it succeeded:\n"; cat "$W/overlap.out"; exit 1; }
		grep -q "while this push ran" "$W/overlap.out" || { cat "$W/overlap.out"; exit 1; }'
assert_green "prod holds ben's tree whole — none of the files ada wrote before she was refused" \
	-- bash -c 'pull_into cal "$W/pull/cal-1" && same "$W/t-ben-1" "$W/pull/cal-1" $BEN_SHARED &&
		{ [ ! -e "$W/pull/cal-1/volume.bin" ] || { printf "volume.bin came from the refused push\n"; exit 1; }; }'
assert_green "ada pushes again, and her tree lands whole" \
	-- bash -c 'in_tree ada "$ADA" "env push $E" >/dev/null 2>&1 &&
		pull_into cal "$W/pull/cal-2" && same "$W/t-ada-2" "$W/pull/cal-2" $SHARED'

# ── rotation keeps every kind of file ────────────────────────────────────────
printf 'BEN_LOCAL=only-ben-%s\n' "$N" >"$BEN/.env.local"
assert_green "ben keeps a private .env.local in prod too" \
	-- bash -c 'in_tree ben "$BEN" "env push $E" >/dev/null 2>&1'
assert_green "and ada publishes her tree over his" \
	-- bash -c 'in_tree ada "$ADA" "env push $E" >/dev/null 2>&1'
assert_green "before rotation ada restores the shared tree and her private file" \
	-- bash -c 'pull_into ada "$W/pull/ada-pre" && same "$ADA" "$W/pull/ada-pre" $SHARED .env.local'
assert_green "and ben the shared tree and his own" \
	-- bash -c 'pull_into ben "$W/pull/ben-pre" && same "$ADA" "$W/pull/ben-pre" $SHARED &&
		same "$BEN" "$W/pull/ben-pre" .env.local'

assert_green "cal's grant is revoked" \
	-- bash -c 'act ada "project grant rm --org $ORG --project $PROJECT --grant $CAL_GRANT" >/dev/null 2>&1'
assert_green "ada rotates prod's key" \
	-- bash -c 'out="$(act ada "env keys rotate $E" 2>&1)" || { printf "%s\n" "$out"; exit 1; }'

assert_green "after rotation ada restores the shared tree, the chunked volume and her private file" \
	-- bash -c 'pull_into ada "$W/pull/ada-post" && same "$ADA" "$W/pull/ada-post" $SHARED .env.local'
assert_green "ben restores the shared tree and his own private file" \
	-- bash -c 'pull_into ben "$W/pull/ben-post" && same "$ADA" "$W/pull/ben-post" $SHARED &&
		same "$BEN" "$W/pull/ben-post" .env.local'
assert_green "ada's inventory still lists her private file" \
	-- bash -c 'act ada "env files $E -q" 2>/dev/null | grep -qx ".env.local"'
assert_green "cal, revoked before the rotation, restores nothing" \
	-- bash -c 'fresh "$W/pull/cal-post"; in_tree cal "$W/pull/cal-post" "env pull $E --apply" >/dev/null 2>&1 && exit 1
		[ -z "$(find "$W/pull/cal-post" -type f)" ]'

printf 'ROUND=after-rotation\n' >"$W/t-after-marker"
assert_green "a push after rotation restores for the other writer" \
	-- bash -c 'mkdir -p "$W/ben/tree-2" && cp -a "$ADA/." "$W/ben/tree-2/" || exit 1
		rm -f "$W/ben/tree-2/.env.local"; cp "$W/t-after-marker" "$W/ben/tree-2/marker.env"
		in_tree ben "$W/ben/tree-2" "env push $E" >/dev/null 2>&1 || exit 1
		pull_into ada "$W/pull/ada-after" && same "$W/ben/tree-2" "$W/pull/ada-after" $SHARED'
assert_green "and ada still restores her private file, which she has not pushed since the rotation" \
	-- bash -c 'same "$ADA" "$W/pull/ada-after" .env.local'
assert_green "a private file ada deletes and pushes without stays deleted" \
	-- bash -c 'rm -f "$ADA/.env.local"; cp "$W/t-after-marker" "$ADA/marker.env"
		in_tree ada "$ADA" "env push $E" >/dev/null 2>&1 || exit 1
		pull_into ada "$W/pull/ada-deleted" || exit 1
		[ ! -e "$W/pull/ada-deleted/.env.local" ] || { printf "the deleted private file came back\n"; exit 1; }'

# ── pushes that overlap a rotation ───────────────────────────────────────────
# A push writes at the epoch it resolved when it began, and keeps writing there until it ends. A
# rotation that publishes the next epoch meanwhile would leave that push in an epoch nobody reads
# — with "pushed" printed. Both orders are forced on a second environment, each by holding one
# side open at the paused object store: a rotation re-chunking a volume, and a push uploading one.
S="--org $ORG --project api --env staging"
STAGE_A="$W/ada/stage"
STAGE_B="$W/ben/stage"
export S STAGE_A STAGE_B
mkdir -p "$STAGE_A" "$STAGE_B"
cp -a "$W/t-ada-2/." "$STAGE_A/"
rm -f "$STAGE_A/.env.local"
cp -a "$W/t-ben-1/." "$STAGE_B/"
printf 'ROUND=ben-during-rotation\n' >"$STAGE_B/marker.env"
assert_green "a staging environment ben may write, keyed and synced" \
	-- bash -c 'act ada "env create --project $PROJECT --name staging" >/dev/null 2>&1 &&
		act ada "project grant add --org $ORG --project $PROJECT --user $(mail ben) --role write --env staging" >/dev/null 2>&1 &&
		act ada "env init $S" >/dev/null 2>&1 && act ada "env keys sync $S" >/dev/null 2>&1'
assert_green "ada pushes a tree with a chunked volume to staging" \
	-- bash -c 'in_tree ada "$STAGE_A" "env push $S" >/dev/null 2>&1'

docker pause "$QA_S3_SRV" >/dev/null
(
	act ada "env keys rotate $S" >"$W/rotate-held.out" 2>&1
	printf '%s' "$?" >"$W/rotate-held.code"
) &
HELD=$!
sleep 8
assert_green "ada's rotation is held open at the object store, re-chunking the volume" -- kill -0 "$HELD"
assert_green "meanwhile ben pushes a small tree to staging at the old key, and it lands" \
	-- bash -c 'out="$(in_tree ben "$STAGE_B" "env push $S" 2>&1)" || { printf "%s\n" "$out"; exit 1; }'
docker unpause "$QA_S3_SRV" >/dev/null
wait "$HELD"
assert_green "the rotation completes, and says it carried what landed while it ran" \
	-- bash -c '[ "$(cat "$W/rotate-held.code")" = 0 ] && grep -q "carried_late" "$W/rotate-held.out" ||
		{ cat "$W/rotate-held.out"; exit 1; }'
cp -a "$STAGE_B" "$W/t-stage-ben"
assert_green "staging, at its new key, holds ben's tree whole" \
	-- bash -c 'pull_into ada "$W/pull/stage-1" "$S" && same "$W/t-stage-ben" "$W/pull/stage-1" $BEN_SHARED &&
		{ [ ! -e "$W/pull/stage-1/volume.bin" ] || { printf "volume.bin is from the tree ben replaced\n"; exit 1; }; }'

printf 'ROUND=ben-held\n' >"$STAGE_B/marker.env"
fixture_varied_file "$STAGE_B/volume.bin" 8 "QA42-S45-BEN-HELD"
docker pause "$QA_S3_SRV" >/dev/null
(
	in_tree ben "$STAGE_B" "env push $S" >"$W/push-held.out" 2>&1
	printf '%s' "$?" >"$W/push-held.code"
) &
HELD=$!
sleep 8
assert_green "ben's push of a new volume is held open at the object store" -- kill -0 "$HELD"
assert_green "meanwhile ada rotates staging, which holds no chunked file, and it completes" \
	-- bash -c 'out="$(act ada "env keys rotate $S" 2>&1)" || { printf "%s\n" "$out"; exit 1; }'
docker unpause "$QA_S3_SRV" >/dev/null
wait "$HELD"
assert_green "ben's push, which the rotation overtook, is refused and says to push again" \
	-- bash -c '[ "$(cat "$W/push-held.code")" != 0 ] || { printf "it succeeded:\n"; cat "$W/push-held.out"; exit 1; }
		grep -q "rotated while this push ran" "$W/push-held.out" || { cat "$W/push-held.out"; exit 1; }'
assert_green "staging still holds the tree from before it, whole" \
	-- bash -c 'pull_into ada "$W/pull/stage-2" "$S" && same "$W/t-stage-ben" "$W/pull/stage-2" $BEN_SHARED &&
		[ ! -e "$W/pull/stage-2/volume.bin" ]'
assert_green "ben pushes again, and his tree lands, volume and all" \
	-- bash -c 'in_tree ben "$STAGE_B" "env push $S" >/dev/null 2>&1 &&
		pull_into ada "$W/pull/stage-3" "$S" && same "$STAGE_B" "$W/pull/stage-3" $SHARED'

spec_end
