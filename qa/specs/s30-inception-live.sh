#!/usr/bin/env bash
# s30 — a real project, end to end.
#
# Inception is a genuine compose project: config in srcs/.env, and six Docker secrets
# under secrets/ that docker-compose.yml mounts by path. Its .gitignore excludes all of
# them, which is the whole point — these are exactly the files git must not hold and
# the vault must.
#
# The scenario is the one an operator actually performs: fill the environment and the
# secrets one by one, push them, wipe the working copy, pull them back, and ask whether
# the project could still start. The last question is the one that matters, and today
# the answer is no.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s30-inception-live"
qa_require_docker_stack
trap qa_server_cleanup EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up

INC="$C42_ROOT/qa/fixtures/inception"
WORK="$QA_RESULTS/s30"; rm -rf "$WORK"; mkdir -p "$WORK"
assert_green "the Inception fixture is checked out" -- test -f "$INC/.env.example"

# ── fill the environment from the project's own template ─────────────────────
mkdir -p "$INC/srcs" "$INC/secrets"
sed 's/login\.42\.fr/qa.42.fr/g' "$INC/.env.example" >"$INC/srcs/.env"
assert_green "srcs/.env was generated from the project's template" -- test -s "$INC/srcs/.env"

# ── fill the secrets one by one, as docker-compose.yml declares them ─────────
# Read the secret filenames out of the compose file rather than hardcoding them, so
# this keeps testing the real project if it grows another secret.
mapfile -t SECRET_FILES < <(grep -oE '\.\./secrets/[a-z_.]+' "$INC/srcs/docker-compose.yml" | sed 's|../secrets/||' | sort -u)
assert_green "docker-compose.yml declares at least one secret file" \
	-- bash -c '[ "${#1}" -gt 0 ]' _ "${SECRET_FILES[0]:-}"

i=0
for f in "${SECRET_FILES[@]}"; do
	i=$((i + 1))
	printf 'qa-inception-secret-%02d-%s\n' "$i" "${f%.*}" >"$INC/secrets/$f"
done
assert_green "every declared secret file was created" \
	-- bash -c 'for f in $2; do [ -s "$1/secrets/$f" ] || exit 1; done' _ "$INC" "${SECRET_FILES[*]}"

# ── the submodule must stay clean: we may only write ignored paths ───────────
# If filling the fixture dirtied it, the battery would be mutating a tracked repo and
# no run after the first would be reproducible.
assert_green "filling the fixture leaves the submodule git-clean" \
	-- bash -c '[ -z "$(git -C "$1" status --porcelain)" ]' _ "$INC"

# ── push, wipe, pull ─────────────────────────────────────────────────────────
assert_green "the Inception project pushes to the vault" \
	-- qa_actor alice "$INC" "push --project qa-inception"

cp "$INC/srcs/.env" "$WORK/env.expected"
for f in "${SECRET_FILES[@]}"; do cp "$INC/secrets/$f" "$WORK/$f.expected"; done

rm -f "$INC/srcs/.env"
rm -rf "$INC/secrets"
assert_green "the working copy of the environment is wiped" -- test ! -f "$INC/srcs/.env"

assert_green "the Inception project pulls back" \
	-- qa_actor alice "$INC" "pull --project qa-inception --apply"

assert_bytes_equal "srcs/.env is restored byte-identically" \
	"$WORK/env.expected" "$INC/srcs/.env"

for f in "${SECRET_FILES[@]}"; do
	assert_green "secrets/$f is restored" -- test -f "$INC/secrets/$f"
done

# The question that decides whether the vault is usable for this project at all. Green since
# the scan became directory-shaped: a file under secrets/ is taken whatever it is called,
# because these files are named for what they hold and there was never a name to widen to.
assert_green "every secret docker-compose.yml mounts is present after a restore" \
	-- bash -c 'for f in $2; do [ -f "$1/secrets/$f" ] || exit 1; done' _ "$INC" "${SECRET_FILES[*]}"

# ── zero knowledge on a real project ─────────────────────────────────────────
DB="$WORK/server.db"
qa_dump_server_db "$DB"
if [ -f "$DB" ]; then
	assert_zero_knowledge "no Inception env value reached the server" "qa.42.fr" "qa-inception" "$DB"
	assert_zero_knowledge "no real Inception path reached the server" "srcs/.env" "qa-inception" "$DB"
fi

# Leave the fixture as we found it.
rm -f "$INC/srcs/.env"; rm -rf "$INC/secrets" "$INC/.42ctl"
assert_green "the fixture is left clean for the next run" \
	-- bash -c '[ -z "$(git -C "$1" status --porcelain)" ]' _ "$INC"

spec_end
