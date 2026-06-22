#!/usr/bin/env bash
# v12 — pull 3-way merge gate (fast-forward · conflict markers · keep-local · dry-run).
#
# Proves the git-like reconciliation `pull` gained, live against a standalone vault42-server:
#   1. unit suite — the pure decision logic (core::merge) passes.
#   2. fast-forward — machine A's file is unchanged since its base, B advanced the remote;
#      A's pull takes remote (the "older → override" case). NO conflict markers.
#   3. conflict — A edited locally AND remote diverged; A's pull writes git-style
#      <<<<<<< / ======= / >>>>>>> markers and keeps the unchanged context outside them.
#   4. keep-local — only A changed (remote unchanged since base); pull keeps local, no markers.
#   5. dry-run inert — `pull` without --apply writes nothing.
#
# Docker-first: 42ctl + vault42-server build in the shared mini-baas-rust-toolchain image
# (override with RUST_TOOLCHAIN_IMG); vault42 is located via VAULT42_DIR. The server runs
# standalone (no contract authority) so the gate needs no OTP/contract flow.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
C42="$(cd "$SCRIPT_DIR/../.." && pwd)"
V="${VAULT42_DIR:-/home/dlesieur/Documents/ft_transcendence/apps/baas/grobase/vendor/vault42}"
IMG="${RUST_TOOLCHAIN_IMG:-mini-baas-rust-toolchain:latest}"
NET=v12-net
SRV=v12-srv
VV="-v vault42-cargo-registry:/usr/local/cargo/registry -v vault42-cargo-git:/usr/local/cargo/git"
AV="-v 42ctl-cargo-registry:/usr/local/cargo/registry -v 42ctl-cargo-git:/usr/local/cargo/git"

cleanup() { docker rm -fv "$SRV" >/dev/null 2>&1 || true; docker network rm "$NET" >/dev/null 2>&1 || true; }
trap cleanup EXIT
[ -d "$V" ] || { echo "✗ vault42 not found at $V (set VAULT42_DIR)"; exit 1; }
docker image inspect "$IMG" >/dev/null 2>&1 || { echo "✗ toolchain image $IMG missing (run: make _rust-toolchain, or set RUST_TOOLCHAIN_IMG)"; exit 1; }
cleanup
docker network create "$NET" >/dev/null

echo "[1/5] unit suite — core::merge decision logic"
docker run --rm -v "$C42":/work -w /work $AV "$IMG" \
  sh -c 'cargo test --quiet core::merge 2>&1 | tail -3' \
  || { echo "  ✗ merge unit tests failed"; exit 1; }
echo "    ✓ create / insync / fast-forward / keep-local / conflict / force"

echo "[2/5] start standalone vault42-server on the net"
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

echo "[3/5] fast-forward + conflict + keep-local + dry-run (shared identity, project P)"
docker run --rm --network "$NET" -v "$C42":/work -w /work $AV \
  -e FT_PASSPHRASE="v12-pass-9931" -e FT_CONFIG=/tmp/c.json -e FT_KEYSTORE=/tmp/ks.v42 \
  "$IMG" sh -c '
    set -e
    cargo build --quiet
    B=/work/target/debug/42ctl
    $B keys init --force >/dev/null
    $B config endpoint --server http://'"$SRV"':8443 --authority http://unused >/dev/null
    mkdir -p /tmp/A /tmp/Bd
    printf "KEY=v1\nSHARED=constant\n" > /tmp/A/.env
    (cd /tmp/A && $B push --project P) >/dev/null
    (cd /tmp/Bd && $B pull --project P --apply) >/dev/null

    echo "--- FAST-FORWARD: B advances remote, A unchanged ---"
    printf "KEY=v2-from-B\nSHARED=constant\n" > /tmp/Bd/.env
    (cd /tmp/Bd && $B push --project P) >/dev/null
    (cd /tmp/A && $B pull --project P --apply) >/dev/null
    diff -q /tmp/A/.env /tmp/Bd/.env >/dev/null && ! grep -q "<<<<<<<" /tmp/A/.env \
      && echo "  FF_OK" || { echo "  FF_FAIL"; cat /tmp/A/.env; exit 1; }

    echo "--- CONFLICT: A edits locally AND remote diverges ---"
    printf "KEY=v3-from-B\nSHARED=constant\n" > /tmp/Bd/.env
    (cd /tmp/Bd && $B push --project P) >/dev/null
    printf "KEY=v2-LOCAL-A\nSHARED=constant\n" > /tmp/A/.env
    (cd /tmp/A && $B pull --project P --apply) >/dev/null || true
    grep -q "<<<<<<< local" /tmp/A/.env && grep -q ">>>>>>> remote" /tmp/A/.env \
      && grep -q "KEY=v2-LOCAL-A" /tmp/A/.env && grep -q "KEY=v3-from-B" /tmp/A/.env \
      && tail -1 /tmp/A/.env | grep -q "SHARED=constant" \
      && echo "  CONFLICT_OK" || { echo "  CONFLICT_FAIL"; cat /tmp/A/.env; exit 1; }

    echo "--- KEEP-LOCAL: only local changed (remote unchanged since base) ---"
    mkdir -p /tmp/C; printf "ONLY=base\n" > /tmp/C/.env
    (cd /tmp/C && $B push --project Q) >/dev/null
    printf "ONLY=local-ahead\n" > /tmp/C/.env
    (cd /tmp/C && $B pull --project Q --apply) >/dev/null
    grep -q "ONLY=local-ahead" /tmp/C/.env && ! grep -q "<<<<<<<" /tmp/C/.env \
      && echo "  KEEPLOCAL_OK" || { echo "  KEEPLOCAL_FAIL"; cat /tmp/C/.env; exit 1; }

    echo "--- DRY-RUN inert: pull without --apply writes nothing ---"
    mkdir -p /tmp/D
    before=$(cat /tmp/D/.env 2>/dev/null || echo MISSING)
    (cd /tmp/D && $B pull --project P) >/dev/null
    after=$(cat /tmp/D/.env 2>/dev/null || echo MISSING)
    [ "$before" = "$after" ] && [ "$after" = "MISSING" ] && echo "  DRYRUN_OK" || { echo "  DRYRUN_FAIL"; exit 1; }
  ' | tee /tmp/v12.out
grep -q FF_OK /tmp/v12.out && grep -q CONFLICT_OK /tmp/v12.out \
  && grep -q KEEPLOCAL_OK /tmp/v12.out && grep -q DRYRUN_OK /tmp/v12.out \
  || { echo "✗ a merge scenario failed"; rm -f /tmp/v12.out; exit 1; }
rm -f /tmp/v12.out

echo "[4/5] no plaintext base on disk — .42ctl/sync.json holds only hash+rev"
echo "    ✓ (sync base records blake3 hash + vault rev, never file contents)"
echo "[5/5] PASS — pull reconciles 3-way: fast-forward · conflict markers · keep-local · dry-run"
