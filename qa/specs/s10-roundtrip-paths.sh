#!/usr/bin/env bash
# s10 — the core promise: a project's environment tree survives a round trip through
# the vault byte-for-byte, lands back at its ORIGINAL paths, and leaves neither
# plaintext nor a real path on the server.
#
# These are assert_green: push/pull is shipped, so a red here is a regression.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/fixtures.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s10-roundtrip-paths"
qa_require_docker_stack
trap qa_server_cleanup EXIT

assert_green "the 42ctl client builds" -- qa_build_client
assert_green "vault42-server starts and listens" -- qa_server_up
[ "$_qa_regression" -eq 0 ] || { spec_end; exit 1; }

WORK="$QA_RESULTS/s10"
rm -rf "$WORK"; mkdir -p "$WORK"
qa_actor_reset alice

# ── flat: one .env at the root ───────────────────────────────────────────────
FLAT="$(fixture_flat)"
assert_green "flat project pushes" -- qa_actor alice "$FLAT" "push --project qa-flat"
mkdir -p "$WORK/flat-restore"
assert_green "flat project pulls into an empty tree" \
	-- qa_actor alice "$WORK/flat-restore" "pull --project qa-flat --apply"
assert_bytes_equal "flat .env is byte-identical after the round trip" \
	"$FLAT/.env" "$WORK/flat-restore/.env"

# ── nested: five env files at four depths ────────────────────────────────────
NESTED="$(fixture_nested)"
chmod 0600 "$NESTED/.env"
chmod 0644 "$NESTED/config/db.env"
assert_green "nested project pushes" -- qa_actor alice "$NESTED" "push --project qa-nested"
mkdir -p "$WORK/nested-restore"
assert_green "nested project pulls into an empty tree" \
	-- qa_actor alice "$WORK/nested-restore" "pull --project qa-nested --apply"

for rel in .env srcs/.env config/db.env apps/web/.env.production apps/api/.env.local; do
	assert_bytes_equal "nested restores $rel to its original path" \
		"$NESTED/$rel" "$WORK/nested-restore/$rel"
done

assert_green "nested preserves the 0600 mode on .env" \
	-- bash -c '[ "$(stat -c %a "$1")" = 600 ]' _ "$WORK/nested-restore/.env"
assert_green "nested preserves the 0644 mode on config/db.env" \
	-- bash -c '[ "$(stat -c %a "$1")" = 644 ]' _ "$WORK/nested-restore/config/db.env"

# ── dry-run must write nothing ───────────────────────────────────────────────
mkdir -p "$WORK/dryrun"
qa_actor alice "$WORK/dryrun" "pull --project qa-nested" >/dev/null 2>&1
assert_green "pull without --apply writes no file" \
	-- bash -c '[ -z "$(find "$1" -type f -not -path "*/.42ctl/*" 2>/dev/null)" ]' _ "$WORK/dryrun"

# ── zero knowledge: the server holds neither value nor path ──────────────────
DB="$WORK/server.db"
assert_green "the server database can be read back for inspection" -- qa_dump_server_db "$DB"
if [ -f "$DB" ]; then
	assert_zero_knowledge "no plaintext secret value reached the server" "nested-mysql-pw" "qa-nested" "$DB"
	assert_zero_knowledge "no plaintext root value reached the server" "root-shared-value" "qa-nested" "$DB"
	assert_zero_knowledge "no real relative path reached the server" "apps/web/.env.production" "qa-nested" "$DB"
	assert_zero_knowledge "no real directory name reached the server" "config/db.env" "qa-nested" "$DB"
fi

# ── the empty project: no secret material at all ─────────────────────────────
EMPTY="$(fixture_empty)"
assert_green "a project with no env files pushes without error" \
	-- qa_actor alice "$EMPTY" "push --project qa-empty"

spec_end
