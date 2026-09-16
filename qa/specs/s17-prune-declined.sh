#!/usr/bin/env bash
# s17 — `--prune` must not delete what the scan merely failed to see.
#
# The scan declines whole directories: a skip-listed name (vendor, node_modules …) is
# descended only when the child is its own git repository. So the set of scanned paths is a
# LOWER BOUND on the tree, never a census of it — and `--prune` used to drop every manifest
# entry that set did not contain.
#
# Measured before the guard: pushing from a tree whose vendor/ children were plain
# directories rather than checkouts scanned 33 of 39 files, declined the rest, and with
# --prune deleted six entries from the vault while printing success. The declined-directory
# warning cannot save you either — its probe gives up past 64 entries, so the largest
# declined directory is the silent one.
#
# This spec reproduces that exact shape: store a file under vendor/ while it IS a
# repository, take the marker away so the directory becomes declined, then prune. The entry
# must survive, because the file is still on the disk.
#
# Notes are checked in the same run: they live in this manifest with a path the scan can
# never produce, so a scanned-set prune deleted every note in the project.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/fixtures.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s17-prune-declined"
qa_require_docker_stack
trap qa_server_cleanup EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
[ "$_qa_regression" -eq 0 ] || { spec_end; exit 1; }

WORK="$QA_RESULTS/s17"; rm -rf "$WORK"; mkdir -p "$WORK"
TREE="$WORK/tree"; mkdir -p "$TREE/vendor/lib"

printf 'ROOT=1\n' >"$TREE/.env"
printf 'VENDORED=1\n' >"$TREE/vendor/lib/.env"
# `.git` is a FILE in a submodule and a directory in a clone; the scan accepts both, so a
# file is enough to make this child a repository boundary the scan will descend into.
printf 'gitdir: ../../.git/modules/lib\n' >"$TREE/vendor/lib/.git"

assert_green "a vendored repository's env file is stored" \
	-- qa_actor alice "$TREE" "push --project qa-prune"

# The child stops being a repository. Nothing about the FILE changed — only whether the
# scan is willing to look at it.
rm -f "$TREE/vendor/lib/.git"

assert_green "pruning from a tree that now declines vendor/ still succeeds" \
	-- qa_actor alice "$TREE" "push --project qa-prune --prune"
assert_green "the push says it kept an entry whose file was not scanned" \
	-- bash -c 'qa_actor alice "$1" "push --project qa-prune --prune" 2>&1 | grep -q "not scanned"' _ "$TREE"

mkdir -p "$WORK/restore"
assert_green "the project pulls" \
	-- qa_actor alice "$WORK/restore" "pull --project qa-prune --apply"
assert_green "the declined file SURVIVED the prune" \
	-- test -f "$WORK/restore/vendor/lib/.env"
assert_green "the scanned file is still there too" \
	-- test -f "$WORK/restore/.env"

# A file that is genuinely gone is still pruned — the guard must not disable prune.
rm -f "$TREE/.env"
assert_green "a deleted file is still pruned" \
	-- qa_actor alice "$TREE" "push --project qa-prune --prune"
mkdir -p "$WORK/restore2"
assert_green "the project pulls again" \
	-- qa_actor alice "$WORK/restore2" "pull --project qa-prune --apply"
assert_green "the deleted file is gone from the vault" \
	-- bash -c '! test -f "$1/.env"' _ "$WORK/restore2"
assert_green "and the declined file is STILL there" \
	-- test -f "$WORK/restore2/vendor/lib/.env"

spec_end
