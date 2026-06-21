#!/usr/bin/env bash
# v11 — P3 project notes (kind=Note) gate.
#
# Proves notes ride the P2 manifest without colliding with the env tree, end to end:
#   1. round-trip — `note add` then `note get` returns the text byte-for-byte.
#   2. listing   — `note ls` shows the note.
#   3. coexistence — an env `push` in the same project leaves the note intact, and
#      `pull --apply` materializes ONLY the env file (never the note).
#   4. removal   — after `note rm` the note is no longer readable.
#   5. zero-knowledge — the server DB holds NO note text and NO real note path.
#
# Docker-first; vault42-server runs standalone (SQLite). vault42 located via VAULT42_DIR.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
C42="$(cd "$SCRIPT_DIR/../.." && pwd)"
V="${VAULT42_DIR:-/home/dlesieur/Documents/ft_transcendence/apps/baas/grobase/vendor/vault42}"
IMG="${RUST_TOOLCHAIN_IMG:-mini-baas-rust-toolchain:latest}"
NET=v11-net
SRV=v11-srv
VV="-v vault42-cargo-registry:/usr/local/cargo/registry -v vault42-cargo-git:/usr/local/cargo/git"
AV="-v 42ctl-cargo-registry:/usr/local/cargo/registry -v 42ctl-cargo-git:/usr/local/cargo/git"

cleanup() { docker rm -fv "$SRV" >/dev/null 2>&1 || true; docker network rm "$NET" >/dev/null 2>&1 || true; }
trap cleanup EXIT
[ -d "$V" ] || { echo "✗ vault42 not found at $V (set VAULT42_DIR)"; exit 1; }
docker image inspect "$IMG" >/dev/null 2>&1 || { echo "✗ toolchain image $IMG missing"; exit 1; }
cleanup
docker network create "$NET" >/dev/null

echo "[1/3] start vault42-server (standalone SQLite) on the net"
docker run -d --name "$SRV" --network "$NET" -v "$V":/work -w /work $VV \
  -e VAULT42_HOST=0.0.0.0 -e VAULT42_PORT=8443 -e VAULT42_DB=/tmp/v42.db -e RUST_LOG=info \
  "$IMG" sh -c 'cargo run --quiet --bin vault42-server' >/dev/null
for _ in $(seq 1 240); do
  docker logs "$SRV" 2>&1 | grep -q "vault42-server listening" && break
  docker inspect "$SRV" >/dev/null 2>&1 || { docker logs "$SRV" 2>&1 | tail -20; exit 1; }
  sleep 1
done
docker logs "$SRV" 2>&1 | grep -q "vault42-server listening" || { echo "  ✗ server never listened"; exit 1; }
echo "    ✓ vault42-server listening"

echo "[2/3] note add/get/ls + env coexistence + rm"
docker run --rm --network "$NET" -v "$C42":/work -w /work $AV \
  -e FT_PASSPHRASE="v11-pass-7281" -e FT_CONFIG=/tmp/c.json -e FT_KEYSTORE=/tmp/ks.v42 \
  "$IMG" sh -c '
    set -e
    cargo build --quiet
    B=/work/target/debug/42ctl
    $B keys init --force >/dev/null
    $B config endpoint --server http://'"$SRV"':8443 --authority http://unused >/dev/null
    mkdir -p /tmp/NP
    printf "onboarding: ssh keys live in vault — ask note-sentinel-NOTE42\n" > /tmp/note-in.txt
    echo "--- note add ---"; (cd /tmp/NP && $B note add docs/onboarding.md --project NP --file /tmp/note-in.txt) >/dev/null
    echo "--- note get byte-exact? ---"; (cd /tmp/NP && $B note get docs/onboarding.md --project NP) > /tmp/note-out.txt
    diff /tmp/note-in.txt /tmp/note-out.txt >/dev/null && echo "  NOTE_ROUNDTRIP_OK" || { echo "  NOTE_ROUNDTRIP_FAIL"; exit 1; }
    echo "--- note ls lists it? ---"
    (cd /tmp/NP && $B note ls --project NP) | grep -q "docs/onboarding.md" && echo "  NOTE_LS_OK" || { echo "  NOTE_LS_FAIL"; exit 1; }
    echo "--- env push in the same project; note must survive; pull must NOT write the note ---"
    printf "TOKEN=env-sentinel-XYZ\n" > /tmp/NP/.env
    (cd /tmp/NP && $B push --project NP) >/dev/null
    mkdir -p /tmp/NB && (cd /tmp/NB && $B pull --project NP --apply) >/dev/null
    [ -f /tmp/NB/.env ] && echo "  PULL_ENV_OK" || { echo "  PULL_ENV_FAIL (.env missing)"; exit 1; }
    [ ! -e /tmp/NB/docs/onboarding.md ] && echo "  PULL_SKIPS_NOTE_OK" || { echo "  PULL_WROTE_NOTE_FAIL"; exit 1; }
    (cd /tmp/NP && $B note get docs/onboarding.md --project NP) > /tmp/note-out2.txt
    diff /tmp/note-in.txt /tmp/note-out2.txt >/dev/null && echo "  NOTE_SURVIVES_PUSH_OK" || { echo "  NOTE_LOST_FAIL"; exit 1; }
    echo "--- note rm then get fails ---"; (cd /tmp/NP && $B note rm docs/onboarding.md --project NP) >/dev/null
    if (cd /tmp/NP && $B note get docs/onboarding.md --project NP) >/dev/null 2>&1; then echo "  RM_FAIL (still readable)"; exit 1; else echo "  RM_OK"; fi
  ' || { echo "  ✗ 42ctl note run failed"; exit 1; }

echo "[3/3] zero-knowledge — server DB holds NO note text + NO real note path"
PLAIN="$(docker exec "$SRV" sh -c 'strings /tmp/v42.db 2>/dev/null | grep -c "note-sentinel-NOTE42" || true')"
PATHLEAK="$(docker exec "$SRV" sh -c 'strings /tmp/v42.db 2>/dev/null | grep -c "docs/onboarding.md" || true')"
echo "    note-text-hits=$PLAIN  note-path-hits=$PATHLEAK"
[ "$PLAIN" = 0 ] && [ "$PATHLEAK" = 0 ] || { echo "    ✗ ZK LEAK (note text or real path on the server)"; exit 1; }
echo "    ✓ server stores only ciphertext — no note text, no real paths"

echo "✅ v11 PASS — note round-trip byte-exact, ls, env-coexistence (pull skips notes), rm, zero-knowledge"
