#!/usr/bin/env bash
# s13 — mutation testing. Flip bytes in the things an attacker or a disk fault can
# reach, and assert the system fails CLOSED.
#
# The property is not "an error is printed". It is that a mutation must never produce
# plaintext that differs from what was sealed. Returning an error is acceptable;
# returning silently different bytes is the failure this spec exists to catch.
#
# Targets: the server's stored database (a hostile or corrupted server), the local
# keystore (a stolen or damaged identity file), and the local sync base (a tampered
# merge base that could drive a bad three-way resolution).

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/fixtures.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s13-tamper-mutation"
qa_require_docker_stack
# This spec deliberately corrupts the server's database. Never hand that store to the
# next spec — force a fresh container on the way out regardless of QA_KEEP_SERVER.
trap qa_server_down EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
[ "$_qa_regression" -eq 0 ] || { spec_end; exit 1; }

WORK="$QA_RESULTS/s13"; rm -rf "$WORK"; mkdir -p "$WORK"
FLAT="$(fixture_flat)"
ORIG="$FLAT/.env"

assert_green "the victim project pushes" -- qa_actor mallory "$FLAT" "push --project qa-tamper"

# ── 0. the manifest is from a client that knows more than this one ───────────
# Not tampering, but the same requirement: fail closed on data you cannot fully read. FIRST,
# because everything below deliberately breaks the server's database, and a push that fails
# with "storage error" would report this as a manifest problem it is not.
# serde drops an unknown field silently, so a manifest from a newer client parses cleanly
# and the reader carries on with a partial understanding of what each entry MEANS. For a
# chunked entry that means writing a few hundred bytes of chunk list to disk in place of the
# file and reporting it restored. Before the reader checked the version, this exact sequence
# exited 0 having restored nothing at all.
#
# The control comes first: an ordinary pull of the same project must succeed, or "the pull
# failed" would prove nothing about the version.
FUT="$WORK/future"; rm -rf "$FUT"; mkdir -p "$FUT/src" "$FUT/dst" "$FUT/dst2"
fixture_project_marker "$FUT/src" qa-future '"*"'
printf 'KEY=value-0001\n' >"$FUT/src/.env"
assert_green "the future-manifest fixture pushes" \
	-- bash -c 'out=$(qa_actor mallory "$1" "push" 2>&1) || { printf "%s\n" "$out"; exit 1; }' _ "$FUT/src"
assert_green "it pulls back normally BEFORE the manifest is replaced" \
	-- bash -c 'qa_actor mallory "$2" "pull --project qa-future --apply" >/dev/null 2>&1
		cmp -s "$1/.env" "$2/.env"' _ "$FUT/src" "$FUT/dst"
printf '{"version":99,"project_id":"qa-future","entries":[]}' >"$FUT/src/future.json"
assert_green "a manifest claiming a newer version can be planted" \
	-- bash -c 'qa_actor mallory "$1" "vault set __42ctl/m/qa-future --file /project/future.json" >/dev/null 2>&1' _ "$FUT/src"
assert_green "pulling a manifest from a newer client fails instead of restoring nothing" \
	-- bash -c 'out=$(qa_actor mallory "$1" "pull --project qa-future --apply" 2>&1) && exit 1
		grep -q 99 <<<"$out" || { printf "%s\n" "$out"; exit 1; }
		grep -qi "update" <<<"$out"' _ "$FUT/dst2"
assert_green "and it writes nothing while refusing" \
	-- bash -c '[ ! -f "$1/.env" ]' _ "$FUT/dst2"

# ── 1. the server's stored bytes are mutated ─────────────────────────────────
# Stop the server so SQLite is not holding the file, flip one byte deep in the
# payload region, and bring it back up on the same database.
docker stop "$QA_SRV" >/dev/null 2>&1
qa_dump_server_db "$WORK/before.db"
if [ -s "$WORK/before.db" ]; then
	python3 - "$WORK/before.db" "$WORK/after.db" <<'PY'
import sys
raw = bytearray(open(sys.argv[1], 'rb').read())
# Flip one bit three quarters of the way in: past the schema pages, inside stored rows.
i = int(len(raw) * 0.75)
raw[i] ^= 0x01
open(sys.argv[2], 'wb').write(raw)
PY
	docker cp "$WORK/after.db" "$QA_SRV:/tmp/qa42.db" >/dev/null 2>&1
	assert_green "the mutated database was written back" -- test -s "$WORK/after.db"
fi
docker start "$QA_SRV" >/dev/null 2>&1
sleep 3

mkdir -p "$WORK/tampered"
qa_actor mallory "$WORK/tampered" "pull --project qa-tamper --apply" >/dev/null 2>&1
# Either the pull refused, or it produced exactly the sealed bytes. Anything else is
# silent corruption reaching the developer's disk.
assert_green "a mutated server database never yields altered plaintext" \
	-- bash -c '[ ! -f "$2" ] || cmp -s "$1" "$2"' _ "$ORIG" "$WORK/tampered/.env"

# ── 2. the local keystore is mutated ─────────────────────────────────────────
KS="$(qa_actor_dir mallory)/keystore.v42"
if [ -f "$KS" ]; then
	cp "$KS" "$WORK/keystore.orig"
	python3 - "$KS" <<'PY'
import sys
p = sys.argv[1]
raw = bytearray(open(p, 'rb').read())
raw[len(raw) // 2] ^= 0x01
open(p, 'wb').write(raw)
PY
	assert_green "a mutated keystore is refused rather than silently unlocked" \
		-- bash -c '! qa_actor mallory "$1" "vault ls" >/dev/null 2>&1' _ "$WORK"
	cp "$WORK/keystore.orig" "$KS"
	assert_green "the restored keystore works again" \
		-- qa_actor mallory "$FLAT" "vault ls"
fi

# ── 3. the local sync base is mutated ────────────────────────────────────────
# .42ctl/sync.json is the merge base. A tampered base is how a three-way merge can be
# steered into resolving the wrong way, so a damaged one must never silently drive an
# --apply that rewrites the developer's file with someone else's content.
SYNC="$FLAT/.42ctl/sync.json"
if [ -f "$SYNC" ]; then
	cp "$ORIG" "$WORK/env.orig"
	printf '{"files":{"..\/..\/escape.env":{"rev":99,"hash":"deadbeef"}}}' >"$SYNC"
	qa_actor mallory "$FLAT" "pull --project qa-tamper --apply" >/dev/null 2>&1
	assert_green "a tampered sync base cannot write outside the project root" \
		-- test ! -e "$(dirname "$FLAT")/escape.env"
	assert_green "a tampered sync base does not corrupt the working file" \
		-- bash -c '[ ! -f "$2" ] || cmp -s "$1" "$2"' _ "$WORK/env.orig" "$ORIG"
fi


spec_end
