#!/usr/bin/env bash
# s11 — which files the vault actually takes.
#
# The stated requirement is that the vault stores raw files of any kind, keeping the
# extension and the path, so a fetch restores the whole tree in place. 42ctl's scanner
# matches only *.env* and *.secrets, so a Docker-secrets tree — the shape Inception and
# most compose projects use — is silently left behind. Silently is the dangerous part:
# push reports success having uploaded none of the actual secrets.
#
# The secrets/ assertions are now assert_green: the scan became directory-shaped, so every
# regular file under a directory named secrets is taken whatever its name. They are kept
# rather than deleted because the measurement that produced them was two files pushed out of
# eight, with a TLS private key among the six that were silently skipped.
#
# What stays assert_spec is the WARNING. A push that carries nothing it should have carried
# and says nothing is the failure that hurt here, and nothing yet reports a file the scan
# declined. Widening the scan fixed this instance; it did not fix the class.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/fixtures.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s11-raw-file-coverage"
qa_require_docker_stack
trap qa_server_cleanup EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
[ "$_qa_regression" -eq 0 ] || { spec_end; exit 1; }

WORK="$QA_RESULTS/s11"; rm -rf "$WORK"; mkdir -p "$WORK"

# ── a Docker-secrets tree ────────────────────────────────────────────────────
ST="$(fixture_secrets_tree)"
assert_green "a project whose secrets live under secrets/ pushes" \
	-- qa_actor alice "$ST" "push --project qa-secrets"
mkdir -p "$WORK/restore"
assert_green "that project pulls" \
	-- qa_actor alice "$WORK/restore" "pull --project qa-secrets --apply"

assert_bytes_equal "srcs/.env round-trips" "$ST/srcs/.env" "$WORK/restore/srcs/.env"

for f in db_password.txt db_root_password.txt ftp_password.txt credentials.txt server.crt server.key; do
	assert_green "secrets/$f is stored and restored" \
		-- test -f "$WORK/restore/secrets/$f"
done
assert_green "the restored private key is byte-identical" \
	-- cmp -s "$ST/secrets/server.key" "$WORK/restore/secrets/server.key"

# The general form of the same requirement, and the proof that a fix is complete:
# everything handed to the vault comes back, byte for byte, at the same path.
assert_tree_reproduced "every file given to the vault is returned byte-for-byte" \
	"$ST" "$WORK/restore"

# The failure mode that makes this dangerous: push exits 0 while carrying nothing.
assert_spec "push warns when a secrets/ directory is present but unscanned" \
	-- bash -c 'qa_actor alice "$1" "push --project qa-secrets" 2>&1 | grep -qi "secrets/\|unscanned\|skipped"' _ "$ST"

# ── decoys: near-miss filenames ──────────────────────────────────────────────
DEC="$(fixture_decoys)"
assert_green "the decoy project pushes" -- qa_actor alice "$DEC" "push --project qa-decoys"
mkdir -p "$WORK/decoy-restore"
assert_green "the decoy project pulls" \
	-- qa_actor alice "$WORK/decoy-restore" "pull --project qa-decoys --apply"

assert_bytes_equal "the live .env round-trips" "$DEC/.env" "$WORK/decoy-restore/.env"
assert_green "a .env.bak backup is not swept into the vault" \
	-- test ! -f "$WORK/decoy-restore/.env.bak"
# Not a regression: skip_file excludes only .stale and .bak, so .swp was never
# claimed to be filtered. It is still worth closing — a vim swap file holds the
# plaintext of the env it shadows, so sweeping it stores a stale plaintext copy and
# a later pull drops that copy back onto a developer's disk.
assert_spec "an editor .env.swp is not swept into the vault" \
	-- test ! -f "$WORK/decoy-restore/.env.swp"
assert_green "a vendored node_modules env is not swept into the vault" \
	-- test ! -f "$WORK/decoy-restore/node_modules/leftover/.env"
assert_green "a build-tree target env is not swept into the vault" \
	-- test ! -f "$WORK/decoy-restore/target/debug/.env"

# A committed template carries no secret and should stay out of the vault; today the
# *.env* glob takes it, so a pull can overwrite a tracked file.
assert_spec "a committed .env.example template is left out of the vault" \
	-- test ! -f "$WORK/decoy-restore/.env.example"

spec_end
