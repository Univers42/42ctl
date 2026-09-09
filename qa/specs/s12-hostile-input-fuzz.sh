#!/usr/bin/env bash
# s12 — hostile input. Everything here is legal on a real filesystem and turns up in
# real projects: a BOM, CRLF line endings, an equals sign inside a value, quotes, a
# 4 KiB value, an empty value, non-ASCII, no trailing newline, filenames with spaces,
# hashes, accents, and a 180-character name.
#
# The property under test is byte-exactness. A vault that silently normalises a value
# is worse than one that refuses it: the developer gets a .env back that looks right
# and breaks at runtime. So every assertion compares bytes, never text.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/fixtures.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s12-hostile-input-fuzz"
qa_require_docker_stack
trap qa_server_cleanup EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
[ "$_qa_regression" -eq 0 ] || { spec_end; exit 1; }

WORK="$QA_RESULTS/s12"; rm -rf "$WORK"; mkdir -p "$WORK"

# ── hostile VALUES ───────────────────────────────────────────────────────────
HV="$(fixture_hostile_values)"
assert_green "a .env of hostile values pushes" -- qa_actor alice "$HV" "push --project qa-hostile-values"
mkdir -p "$WORK/values"
assert_green "a .env of hostile values pulls" \
	-- qa_actor alice "$WORK/values" "pull --project qa-hostile-values --apply"
assert_bytes_equal "every hostile value survives byte-for-byte" \
	"$HV/.env" "$WORK/values/.env"

# Spot-check the individual hazards so a failure names the culprit instead of just
# saying the file differs.
if [ -f "$WORK/values/.env" ]; then
	assert_green "the UTF-8 BOM is preserved" \
		-- bash -c 'head -c3 "$1" | cmp -s - <(printf "\xEF\xBB\xBF")' _ "$WORK/values/.env"
	assert_green "the CRLF line ending is preserved" \
		-- grep -qU $'\r' "$WORK/values/.env"
	assert_green "the file still ends without a trailing newline" \
		-- bash -c '[ -n "$(tail -c1 "$1")" ]' _ "$WORK/values/.env"
	assert_green "the 4 KiB value is not truncated" \
		-- bash -c 'grep -q "x\{4096\}" "$1"' _ "$WORK/values/.env"
	assert_green "non-ASCII in a value is preserved" \
		-- grep -qa 'clé-privée' "$WORK/values/.env"
fi

# ── hostile FILENAMES ────────────────────────────────────────────────────────
HN="$(fixture_hostile_names)"
assert_green "a tree of hostile filenames pushes" -- qa_actor alice "$HN" "push --project qa-hostile-names"
mkdir -p "$WORK/names"
assert_green "a tree of hostile filenames pulls" \
	-- qa_actor alice "$WORK/names" "pull --project qa-hostile-names --apply"

assert_green "a filename containing a space round-trips" \
	-- cmp -s "$HN/with space.env" "$WORK/names/with space.env"
assert_green "a filename containing a hash round-trips" \
	-- cmp -s "$HN/with#hash.env" "$WORK/names/with#hash.env"
assert_green "a directory containing a space round-trips" \
	-- cmp -s "$HN/dir with spaces/.env" "$WORK/names/dir with spaces/.env"
assert_green "a non-ASCII directory name round-trips" \
	-- cmp -s "$HN/répertoire/.env" "$WORK/names/répertoire/.env"
assert_green "a 180-character filename round-trips" \
	-- bash -c 'cmp -s "$1" "$2"' _ \
	"$HN/$(printf 'l%.0s' $(seq 1 180)).env" \
	"$WORK/names/$(printf 'l%.0s' $(seq 1 180)).env"
assert_green "no hostile filename was lost in the round trip" \
	-- bash -c '[ "$(find "$1" -name "*.env" -o -name ".env" | wc -l)" = "$(find "$2" -name "*.env" -o -name ".env" | wc -l)" ]' _ "$HN" "$WORK/names"

spec_end
