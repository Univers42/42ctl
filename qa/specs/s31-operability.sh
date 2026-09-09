#!/usr/bin/env bash
# s31 — can you run this for years without losing it?
#
# Every other spec asks whether the product works. This one asks what happens when the
# operator gets something wrong, the disk goes away, or nobody is watching. Those are the
# failures that end a self-hosted vault, and none of them are functional bugs.
#
# The backup drill is the centrepiece and it is green: it proves a restore actually
# reproduces a secret. It exists before any backup script does, deliberately — an untested
# backup is a guess, and writing the verification first means whatever gets built is
# measured against a restore that is known to work.
#
# Most of the rest is red. Each red is an item from the reliability plan, phrased as the
# behaviour an operator is entitled to expect.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s31-operability"
qa_require_docker_stack
trap 'qa_server_cleanup; docker rm -fv qa42-restore qa42-try >/dev/null 2>&1' EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
[ "$_qa_regression" -eq 0 ] || { spec_end; exit 1; }

W="$QA_RESULTS/s31"; rm -rf "$W"; mkdir -p "$W/full" "$W/dbonly"
CANARY='BACKUP_CANARY=restore-me-0001'
printf '%s\n' "$CANARY" >"$W/v.txt"
qa_actor_reset baker

# ── the backup drill ─────────────────────────────────────────────────────────
assert_green "a secret is stored that a restore must reproduce" \
	-- qa_actor baker "$W" "vault set canary/one --file /project/v.txt"

# The trap any backup script will fall into: with write-ahead logging on, the database
# FILE is one empty page and every byte of data is in the -wal beside it. Copying just
# the .db — the obvious thing to back up — captures an empty vault, and the restore
# succeeds, so the loss is discovered only when the data is needed.
for f in qa42.db qa42.db-wal qa42.db-shm; do docker cp "$QA_SRV:/tmp/$f" "$W/full/$f" >/dev/null 2>&1; done
docker cp "$QA_SRV:/tmp/qa42.db" "$W/dbonly/qa42.db" >/dev/null 2>&1
printf '# database file %s bytes, write-ahead log %s bytes\n' \
	"$(stat -c %s "$W/full/qa42.db" 2>/dev/null)" "$(stat -c %s "$W/full/qa42.db-wal" 2>/dev/null)"

# Bring a server up on a restored copy and read the secret back through the real client.
restore_and_read() {
	local dir="$1"
	docker rm -fv qa42-restore >/dev/null 2>&1
	docker run -d --name qa42-restore --network "$QA_NET" -v "$dir":/restore \
		-v "$VAULT42_DIR":/work -w /work $QA_V42_VOLS \
		-e VAULT42_HOST=0.0.0.0 -e VAULT42_PORT=8443 -e VAULT42_DB=/restore/qa42.db \
		-e VAULT42_STORE=sqlite -e VAULT42_SCOPE_KEYS_ENABLED=1 -e RUST_LOG=info \
		"$QA_IMG" sh -c 'cargo run --quiet --bin vault42-server' >/dev/null 2>&1
	local i
	for i in $(seq 1 120); do docker logs qa42-restore 2>&1 | grep -q listening && break; sleep 1; done
	docker run --rm --network "$QA_NET" -v "$C42_ROOT":/work \
		-v "$QA_RESULTS/actors/baker":/state -w /tmp --user "$(id -u):$(id -g)" \
		-e HOME=/state -e FT_PASSPHRASE=qa-pass-baker -e FT_CONFIG=/state/config.json \
		-e FT_KEYSTORE=/state/keystore.v42 -e FT_CONTRACT=/state/contract.tok "$QA_IMG" \
		sh -c '/work/target/debug/42ctl config endpoint --server http://qa42-restore:8443 --authority http://unused >/dev/null 2>&1
			/work/target/debug/42ctl vault get canary/one' 2>/dev/null
	docker rm -fv qa42-restore >/dev/null 2>&1
}
export -f restore_and_read; export QA_NET VAULT42_DIR QA_V42_VOLS QA_IMG C42_ROOT QA_RESULTS

assert_green "a full backup restores into a fresh server and reproduces the secret" \
	-- bash -c 'restore_and_read "$1" | grep -qF "$2"' _ "$W/full" "$CANARY"

# The same drill against the naive backup. It must NOT quietly succeed with an empty
# vault; whatever backup script is written has to pass the full drill, not this one.
assert_green "a database-file-only copy does not reproduce the secret" \
	-- bash -c '! restore_and_read "$1" | grep -qF "$2"' _ "$W/dbonly" "$CANARY"

V42="${VAULT42_DIR:-$C42_ROOT/qa/.cache/vault42}"
assert_green "a backup procedure exists in the repository" \
	-- bash -c 'ls "$1"/scripts/ops/backup* >/dev/null 2>&1 || grep -rqi "VACUUM INTO\|litestream" "$1/scripts" 2>/dev/null' _ "$V42"
assert_green "the runbook documents how to restore" \
	-- bash -c 'grep -qiE "^#+.*(backup|restore)" "$1/RUNBOOK.md"' _ "$V42"

# ── fail-closed startup ──────────────────────────────────────────────────────
# A misconfigured auth gate must stop the server, not open it. Today a missing OR
# malformed contract key silently disables the contract requirement and any self-generated
# keypair becomes a valid principal, with a green boot log.
VALID_KEY="$(printf 'a%.0s' $(seq 1 64))"
assert_green "the server starts with a well-formed contract public key" \
	-- bash -c '[ "$(qa_try_start "VAULT42_CONTRACT_PUBKEY=$1")" = up ]' _ "$VALID_KEY"
assert_green "the server refuses to start with a malformed contract public key" \
	-- bash -c '[ "$(qa_try_start "VAULT42_CONTRACT_PUBKEY=not-hex-at-all")" = refused ]'
assert_green "the server refuses to start with a truncated contract public key" \
	-- bash -c '[ "$(qa_try_start "VAULT42_CONTRACT_PUBKEY=abcdef")" = refused ]'
assert_spec "running without a contract gate requires saying so explicitly" \
	-- bash -c '[ "$(qa_try_start "VAULT42_UNUSED=1")" = refused ]'

# A new deployment starts with neither a database nor a signing key. That state must
# succeed, or the service can never be deployed at all. It is the case both harnesses
# missed: gates that seed a key first never reach it, and it only appears once refusing to
# mint a key is the right behaviour in every OTHER case.
assert_green "the authority starts from a genuinely fresh state" \
	-- bash -c '[ "$(qa_try_authority_fresh)" = up ]'

# The contract authority mints a NEW root of trust when its key file is missing and boots
# normally, which locks every existing user out of intact data while looking healthy.
assert_green "the authority refuses to start rather than minting a new signing key" \
	-- bash -c 'sed -n "/fn load_or_create/,/^}/p" "$1/crates/vault42-contract/src/signing.rs" | grep -qiE "bail|refuse|Err\(" ' _ "$V42"

# ── knowing it is alive ──────────────────────────────────────────────────────
# Split, because the two apps cannot be checked the same way. The authority serves HTTP
# and /healthz, so an http check is the right instrument. vault42-server speaks only gRPC
# and has no HTTP endpoint at all, so asserting an http check against it would demand
# something that cannot work; it needs a tcp check or a gRPC health service instead.
assert_green "the control plane's deployment configures a health check" \
	-- bash -c 'grep -q "checks" "$1/fly.authority.toml"' _ "$V42"
# Pinned to the REQUIREMENT, not to a spelling. The first version grepped for `tcp_checks`,
# fly's older syntax; the check that landed uses a top-level [checks] block, so a real check
# read as no check at all. A check block alone proves nothing either — one polling a port
# nothing serves passes forever and reports healthy — so it must poll the port the service
# declares.
#
# The port match is anchored. Unanchored, `port = 8443` was satisfied by the very
# `internal_port = 8443` line the number came from, so moving the check to a dead port left
# the assertion green. Broken on purpose both ways before being trusted.
assert_green "the gRPC server's deployment configures a liveness check on the port it serves" \
	-- bash -c 'grep -qE "^\[checks\]|tcp_checks|grpc_checks|type *= *\"tcp\"" "$1/fly.toml" ||
			{ printf "no check block at all\n"; exit 1; }
		port=$(grep -oE "internal_port *= *[0-9]+" "$1/fly.toml" | grep -oE "[0-9]+" | head -1)
		[ -n "$port" ] || { printf "no internal_port to check the check against\n"; exit 1; }
		grep -qE "^[[:space:]]*port *= *$port" "$1/fly.toml" ||
			{ printf "a check exists but does not poll port %s\n" "$port"; exit 1; }' _ "$V42"
assert_spec "a storage failure carries its cause instead of being discarded" \
	-- bash -c '! grep -q "map_err(|_| StoreError::Sql)" "$1/crates/vault42-server/src/store.rs"' _ "$V42"

# ── configuration that matches what the product does ─────────────────────────
assert_green "the deployed server enables the scope-key surface" \
	-- bash -c 'grep -q "VAULT42_SCOPE_KEYS_ENABLED" "$1/fly.toml"' _ "$V42"
assert_green "the control plane has a deployment artifact" \
	-- bash -c 'ls "$1"/deploy/Dockerfile.authority >/dev/null 2>&1 || grep -rql "vault42-authority" "$1"/fly*.toml 2>/dev/null' _ "$V42"
# Comments stripped, and matched on the ARM rather than the word. `grep "Unimplemented"`
# over the whole file was satisfiable by a comment or by prose mentioning it, which is the
# same weakness as asserting a property by the text near it. The control is separate, so a
# red cannot mean "the hint table moved" while reading as "the hint is missing".
#
# The behavioural version would run a scope verb against a server started with
# VAULT42_SCOPE_KEYS_ENABLED unset and require a hint in the output. Not written: it needs a
# second server on the opposite flag, and s31's server has the surface on for everything
# else in this spec.
assert_green "the CLI's hint table is where this spec looks for it" \
	-- bash -c 'grep -q "tonic::Code::NotFound" "$1/src/ui.rs"' _ "$C42_ROOT"
assert_spec "an unimplemented server feature gets an explanatory hint in the CLI" \
	-- bash -c 'sed "s|//.*||" "$1/src/ui.rs" | grep -qE "Code::Unimplemented *=>"' _ "$C42_ROOT"

# ── build reproducibility ────────────────────────────────────────────────────
# The client pins the crypto core by commit. If that commit is only reachable from a
# branch, a force-push makes the client unbuildable, retroactively, including past releases.
assert_spec "the client pins the crypto core to a tag rather than a branch commit" \
	-- bash -c 'rev=$(sed -n "s/.*vault42-core.*rev = \"\([^\"]*\)\".*/\1/p" "$1/Cargo.toml" | head -1)
		[ -n "$rev" ] && [ -n "$(git -C "$2" tag --contains "$rev" 2>/dev/null)" ]' _ "$C42_ROOT" "$V42"

# ── documentation that does not overstate protection ─────────────────────────
# A threat model claiming a mitigation that does not exist is worse than an honest gap,
# because it stops anyone from looking. Each of these names a protection with no code.
assert_green "the threat model does not claim key rotation that does not exist" \
	-- bash -c '! grep -q "revocable via key rotation" "$1/THREAT-MODEL.md"' _ "$V42"
assert_spec "the runbook does not document a seal state that does not exist" \
	-- bash -c '! grep -q "VAULT42_UNSEAL_SEED" "$1/RUNBOOK.md"' _ "$V42"
# The CLI half of this reads src/cli/vault.rs, where the Audit variant moved when cli.rs was
# split. It used to read src/cli.rs: with that file gone, sed errors, the fallback finds
# nothing, and the assertion fails — in the SAFE direction, but for the wrong reason. A
# control asserts the variant is where this looks, so "the file moved" cannot masquerade as
# "the flag is missing".
assert_green "the audit command is where this spec looks for it" \
	-- bash -c 'sed -n "/^    Audit {/,/^    }/p" "$1/src/cli/vault.rs" | grep -q since' _ "$C42_ROOT"
assert_green "the runbook does not document an audit --verify that does not exist" \
	-- bash -c '! grep -q "audit --verify" "$1/RUNBOOK.md" ||
		sed -n "/^    Audit {/,/^    }/p" "$2/src/cli/vault.rs" | grep -q verify' _ "$V42" "$C42_ROOT"

spec_end
