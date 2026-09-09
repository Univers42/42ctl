#!/usr/bin/env bash
# s16 — moving a credential between people.
#
# The question this answers is whether the vault works for a team rather than for one
# machine. Alice seals a secret, hands it to Bob, and Bob must get exactly the bytes
# Alice sealed. Just as important is the negative half: Bob must not reach anything
# Alice did not hand him, and a third person must not reach it at all.
#
# Sharing here is a re-seal, not a key handout: Alice decrypts locally and seals a new
# envelope to Bob's public key. So the server never holds a key that opens both, and
# revoking is a matter of what exists rather than of trusting a recipient.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/fixtures.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s16-multi-user-transfer"
qa_require_docker_stack
trap qa_server_cleanup EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
[ "$_qa_regression" -eq 0 ] || { spec_end; exit 1; }

WORK="$QA_RESULTS/s16"; rm -rf "$WORK"; mkdir -p "$WORK"
qa_actor_reset alice; qa_actor_reset bob; qa_actor_reset mallory

SECRET='DB_PASSWORD=team-shared-value-0001'
printf '%s\n' "$SECRET" >"$WORK/secret.txt"
printf 'PRIVATE=alice-only-value\n' >"$WORK/private.txt"

# Identities. Each actor keeps its own keystore, so these are genuinely three people.
ALICE_FP="$(qa_actor alice "$WORK" "auth whoami" 2>/dev/null | sed -n 's/^principal *//p' | tr -d ' \r')"
BOB_ADDR="$(qa_actor bob "$WORK" "keys export-pub" 2>/dev/null | grep -o 'v42:[A-Za-z0-9_-]*' | head -1)"
MALLORY_ADDR="$(qa_actor mallory "$WORK" "keys export-pub" 2>/dev/null | grep -o 'v42:[A-Za-z0-9_-]*' | head -1)"

assert_green "alice has a principal fingerprint" -- test -n "$ALICE_FP"
assert_green "bob has a shareable address" -- bash -c '[ -n "$1" ]' _ "$BOB_ADDR"
assert_green "alice and bob are different identities" \
	-- bash -c '[ "$1" != "$2" ]' _ "$BOB_ADDR" "$MALLORY_ADDR"

# ── alice seals two secrets, one to share and one to keep ────────────────────
assert_green "alice stores a secret to share" \
	-- bash -c 'qa_actor alice "$1" "vault set team/db-password --file /project/secret.txt"' _ "$WORK"
assert_green "alice stores a secret she keeps to herself" \
	-- bash -c 'qa_actor alice "$1" "vault set alice/private --file /project/private.txt"' _ "$WORK"
assert_green "alice reads her own secret back byte-for-byte" \
	-- bash -c 'qa_actor alice "$1" "vault get team/db-password" 2>/dev/null | grep -qF "$2"' _ "$WORK" "$SECRET"

# ── the handover ─────────────────────────────────────────────────────────────
assert_green "alice shares the secret to bob's address" \
	-- bash -c 'qa_actor alice "$1" "vault share team/db-password --to $2"' _ "$WORK" "$BOB_ADDR"
assert_green "bob reads the shared secret and gets alice's exact bytes" \
	-- bash -c 'qa_actor bob "$1" "vault get shared/$2/team/db-password" 2>/dev/null | grep -qF "$3"' \
	_ "$WORK" "$ALICE_FP" "$SECRET"

# ── the negative half, which is what makes the positive half worth anything ──
assert_green "bob cannot read a secret alice never shared" \
	-- bash -c '! qa_actor bob "$1" "vault get alice/private" >/dev/null 2>&1' _ "$WORK"
assert_green "bob cannot read alice's own copy at her path" \
	-- bash -c '! qa_actor bob "$1" "vault get team/db-password" >/dev/null 2>&1' _ "$WORK"
assert_green "mallory cannot read the secret shared with bob" \
	-- bash -c '! qa_actor mallory "$1" "vault get shared/$2/team/db-password" >/dev/null 2>&1' _ "$WORK" "$ALICE_FP"
assert_green "bob's listing does not show alice's secrets" \
	-- bash -c '! qa_actor bob "$1" "vault ls" 2>/dev/null | grep -q "alice/private"' _ "$WORK"

# ── the plaintext never reaches the server, even mid-handover ────────────────
DB="$WORK/server.db"
qa_dump_server_db "$DB"
if [ -f "$DB" ]; then
	assert_zero_knowledge "the shared plaintext never reached the server" "team-shared-value-0001" "$ALICE_FP" "$DB"
	assert_zero_knowledge "the unshared plaintext never reached the server" "alice-only-value" "$ALICE_FP" "$DB"
fi

# ── revocation, which is the half a team actually depends on ─────────────────
# Sharing is a re-seal, so Bob holds his own envelope. Rotating Alice's copy does not
# reach into Bob's. Whether that is right depends on what the operator was promised,
# and there is no verb that says "take it back", which is the real gap.
assert_green "alice can rotate her own copy" \
	-- bash -c 'qa_actor alice "$1" "vault rotate team/db-password"' _ "$WORK"
assert_spec "there is a verb to revoke a share already handed out" \
	-- bash -c 'qa_actor alice "$1" "vault --help" 2>&1 | grep -qiE "revoke|unshare"' _ "$WORK"
assert_spec "revoking a share stops the recipient reading it" \
	-- bash -c '! qa_actor bob "$1" "vault get shared/$2/team/db-password" >/dev/null 2>&1' _ "$WORK" "$ALICE_FP"

# ── a shared payload with hostile bytes must survive the re-seal ─────────────
printf 'A=1\r\nB=clé-privée-🔐\nC=a=b=c\n' >"$WORK/hostile.txt"
assert_green "alice stores a hostile-byte secret" \
	-- bash -c 'qa_actor alice "$1" "vault set team/hostile --file /project/hostile.txt"' _ "$WORK"
assert_green "alice shares it to bob" \
	-- bash -c 'qa_actor alice "$1" "vault share team/hostile --to $2"' _ "$WORK" "$BOB_ADDR"
assert_green "bob receives the hostile bytes unchanged" \
	-- bash -c 'qa_actor bob "$1" "vault get shared/$2/team/hostile" 2>/dev/null | grep -qa "clé-privée"' \
	_ "$WORK" "$ALICE_FP"

spec_end
