#!/usr/bin/env bash
# s15 — an orchestrator repo with submodules.
#
# The real working shape: one root project that pulls in several other projects as
# submodules, each with its own environment and secrets. The requirement is that a
# single fetch from the root puts every file back where it was, inside the submodule
# it came from, without the operator moving anything by hand.
#
# Two hazards this spec exists to pin. A submodule placed under vendor/ is silently
# skipped, because vendor is in the scanner's dependency skip list. And a submodule
# that is also its own 42ctl project is captured twice, once by the root and once by
# itself, with no rule saying which copy wins on restore.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/fixtures.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s15-submodule-orchestrator"
qa_require_docker_stack
trap qa_server_cleanup EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
[ "$_qa_regression" -eq 0 ] || { spec_end; exit 1; }

WORK="$QA_RESULTS/s15"; rm -rf "$WORK"; mkdir -p "$WORK"
ORCH="$(fixture_orchestrator)"

assert_green "the orchestrator project pushes from its root" \
	-- qa_actor alice "$ORCH" "push --project qa-orchestrator"
mkdir -p "$WORK/restore"
assert_green "the orchestrator project pulls into a clean tree" \
	-- qa_actor alice "$WORK/restore" "pull --project qa-orchestrator --apply"

# ── every submodule's environment lands back inside that submodule ───────────
for rel in \
	.env \
	services/api/.env \
	services/api/srcs/.env \
	services/web/.env \
	services/web/srcs/.env \
	libs/shared/.env \
	libs/shared/srcs/.env; do
	assert_bytes_equal "restores $rel inside its own submodule" \
		"$ORCH/$rel" "$WORK/restore/$rel"
done

# Paths must not be flattened into the root, which is the failure that would make a
# restore look successful while leaving every service unable to start.
assert_green "no submodule file is flattened into the root" \
	-- bash -c '[ ! -f "$1/.env.api" ] && [ ! -f "$1/api.env" ]' _ "$WORK/restore"
assert_green "the submodule directory structure is recreated" \
	-- bash -c '[ -d "$1/services/api/srcs" ] && [ -d "$1/libs/shared/srcs" ]' _ "$WORK/restore"

# ── a submodule under vendor/ is silently dropped ────────────────────────────
# vendor is skipped as a dependency tree, which is right for vendored code and wrong
# for a submodule parked there. Push reports success either way.
assert_spec "a submodule under vendor/ is not silently skipped" \
	-- test -f "$WORK/restore/vendor/thirdparty/.env"

# ── the same file captured by two overlapping projects ───────────────────────
# services/api is its own project as well as part of the root. Pushing both means the
# same plaintext is sealed under two project ids, and nothing states which restore
# wins if they diverge.
assert_green "a submodule can also push as its own project" \
	-- bash -c 'cd "$1/services/api" && qa_actor apiteam "$1/services/api" "push"' _ "$ORCH"
mkdir -p "$WORK/api-only"
assert_green "that submodule pulls standalone" \
	-- qa_actor apiteam "$WORK/api-only" "pull --project submodule-api --apply"
assert_bytes_equal "the standalone pull reproduces the submodule's env" \
	"$ORCH/services/api/.env" "$WORK/api-only/.env"

# The overlap is real; what is missing is a stated rule. Divergent copies of one file
# under two projects should be detected rather than resolved by whichever ran last.
printf 'SERVICE_NAME=api\nDB_PASSWORD=CHANGED-BY-THE-API-TEAM\n' >"$ORCH/services/api/.env"
qa_actor apiteam "$ORCH/services/api" "push" >/dev/null 2>&1
assert_spec "a file owned by two overlapping projects reports the divergence" \
	-- bash -c 'qa_actor alice "$1" "pull --project qa-orchestrator" 2>&1 | grep -qi "conflict\|diverge\|overlap"' _ "$WORK/restore"

# ── zero knowledge holds across the whole orchestrator ───────────────────────
DB="$WORK/server.db"
qa_dump_server_db "$DB"
if [ -f "$DB" ]; then
	assert_zero_knowledge "no submodule password reached the server" "api-db-pw" "qa-orchestrator" "$DB"
	assert_zero_knowledge "no submodule path reached the server" "services/api" "qa-orchestrator" "$DB"
fi

spec_end
