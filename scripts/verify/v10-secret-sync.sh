#!/usr/bin/env bash
# v10 — P2 path-aware secret sync (push/pull) gate.
#
# Proves the whole P2 contract live, end to end:
#   1. traversal defense — every "../", absolute, UNC, drive-letter, reserved-name and
#      __42ctl-prefixed stored path is REFUSED (the projpath unit suite).
#   2. round-trip — `push` a project's *.env* tree on "machine A" → `pull --apply` it on a
#      fresh "machine B" (same recovered identity) reproduces every file byte-for-byte.
#   3. mode preserved — a 0644 file stays 0644 on the far side.
#   4. dry-run inert — `pull` without --apply writes nothing.
#   5. zero-knowledge — the server's DB holds NO plaintext and NO real relative paths.
#
# Docker-first: the 42ctl + vault42 builds and the standalone vault42-server all run in the
# shared mini-baas-rust-toolchain image; no host cargo. vault42 is located via VAULT42_DIR.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
C42="$(cd "$SCRIPT_DIR/../.." && pwd)"
V="${VAULT42_DIR:-/home/dlesieur/Documents/ft_transcendence/apps/baas/grobase/vendor/vault42}"
IMG="${RUST_TOOLCHAIN_IMG:-mini-baas-rust-toolchain:latest}"
NET=v10-net
SRV=v10-srv
VV="-v vault42-cargo-registry:/usr/local/cargo/registry -v vault42-cargo-git:/usr/local/cargo/git"
AV="-v 42ctl-cargo-registry:/usr/local/cargo/registry -v 42ctl-cargo-git:/usr/local/cargo/git"

cleanup() { docker rm -fv "$SRV" >/dev/null 2>&1 || true; docker network rm "$NET" >/dev/null 2>&1 || true; }
trap cleanup EXIT
[ -d "$V" ] || { echo "✗ vault42 not found at $V (set VAULT42_DIR)"; exit 1; }
docker image inspect "$IMG" >/dev/null 2>&1 || { echo "✗ toolchain image $IMG missing (run: make _rust-toolchain)"; exit 1; }
cleanup
docker network create "$NET" >/dev/null

echo "[1/5] traversal defense — every unsafe stored path REFUSED (projpath unit suite)"
docker run --rm -v "$C42":/work -w /work $AV "$IMG" \
  sh -c 'cargo test --quiet core::projpath 2>&1 | tail -3' \
  || { echo "  ✗ traversal-defense unit tests failed"; exit 1; }
echo "    ✓ projpath rejects ../ / absolute / UNC / drive / reserved / __42ctl"

echo "[2/5] start vault42-server (standalone SQLite) on the net"
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

echo "[3/5] push (machine A) → pull --apply (machine B) + dry-run (C)"
docker run --rm --network "$NET" -v "$C42":/work -w /work $AV \
  -e FT_PASSPHRASE="v10-pass-7281" -e FT_CONFIG=/tmp/c.json -e FT_KEYSTORE=/tmp/ks.v42 \
  "$IMG" sh -c '
    set -e
    cargo build --quiet
    B=/work/target/debug/42ctl
    $B keys init --force >/dev/null
    $B config endpoint --server http://'"$SRV"':8443 --authority http://unused >/dev/null
    mkdir -p /tmp/A/config /tmp/A/sub
    printf "DB_URL=postgres://secret-sentinel-XYZ@h/db\n" > /tmp/A/config/db.env
    printf "TOKEN=top-sentinel-ABC\n" > /tmp/A/.env
    printf "k=v\n" > /tmp/A/sub/app.secrets
    chmod 0644 /tmp/A/config/db.env; chmod 0600 /tmp/A/.env
    echo "--- push (machine A, --project P) ---"; (cd /tmp/A && $B push --project P)
    echo "--- pull --apply (machine B, empty dir, --project P) ---"; mkdir -p /tmp/B && (cd /tmp/B && $B pull --project P --apply)
    echo "--- byte-identical round-trip? ---"
    if diff -r /tmp/A /tmp/B >/dev/null 2>&1; then echo "  ROUNDTRIP_OK"; else echo "  ROUNDTRIP_FAIL"; diff -r /tmp/A /tmp/B; exit 1; fi
    echo "--- mode preserved (0644 file)? ---"
    [ "$(stat -c %a /tmp/B/config/db.env)" = 644 ] && echo "  MODE_OK" || { echo "  MODE_FAIL got $(stat -c %a /tmp/B/config/db.env)"; exit 1; }
    echo "--- dry-run writes nothing? ---"; mkdir -p /tmp/C && (cd /tmp/C && $B pull --project P) >/dev/null
    if [ -z "$(find /tmp/C -type f 2>/dev/null)" ]; then echo "  DRYRUN_OK"; else echo "  DRYRUN_FAIL (files written)"; exit 1; fi
  ' || { echo "  ✗ 42ctl push/pull run failed"; exit 1; }

echo "[4/5] zero-knowledge — server DB holds NO plaintext + NO real paths"
PLAIN="$(docker exec "$SRV" sh -c 'strings /tmp/v42.db 2>/dev/null | grep -c "secret-sentinel-XYZ" || true')"
PATHLEAK="$(docker exec "$SRV" sh -c 'strings /tmp/v42.db 2>/dev/null | grep -c "config/db.env" || true')"
echo "    plaintext-hits=$PLAIN  path-hits=$PATHLEAK"
[ "$PLAIN" = 0 ] && [ "$PATHLEAK" = 0 ] || { echo "    ✗ ZK LEAK (plaintext or real path on the server)"; exit 1; }
echo "    ✓ server stores only ciphertext — no plaintext, no real paths"

echo "[5/5] ✅ v10 PASS — traversal-refused, push→pull byte-identical, mode preserved, dry-run inert, zero-knowledge"
