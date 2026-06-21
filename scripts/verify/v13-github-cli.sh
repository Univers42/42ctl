#!/usr/bin/env bash
# v13 — P5-edge GitHub CLI smoke + relay HMAC conformance.
#
# Proves the 42ctl GitHub command surface end to end against a mock grobase (no GitHub App,
# no full stack needed — the real server side is m163):
#   1. `auth login --github` runs the device flow (poll pending → token) and saves a session.
#   2. org verbs need a session — `org github sync` BEFORE login is refused (client guard).
#   3. after login `org github connect|link|sync` succeed AND carry the Bearer token (the
#      mock 401s any /v1/orgs call without it, so success proves transmission).
#   4. relay HMAC: a relay-signed `v1.<ts>.<sig>` header verifies under grobase's documented
#      scheme, and a tampered body / wrong secret are rejected (server verify proven by m163).
#
# Docker-first; mock = python:3-alpine, CLI = mini-baas-rust-toolchain, node check = node:20-alpine.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
C42="$(cd "$SCRIPT_DIR/../.." && pwd)"
IMG="${RUST_TOOLCHAIN_IMG:-mini-baas-rust-toolchain:latest}"
NET=v13-net
MOCK=v13-mock
AV="-v 42ctl-cargo-registry:/usr/local/cargo/registry -v 42ctl-cargo-git:/usr/local/cargo/git"
TMPD="$(mktemp -d)"

cleanup() { docker rm -fv "$MOCK" >/dev/null 2>&1 || true; docker network rm "$NET" >/dev/null 2>&1 || true; rm -rf "$TMPD"; }
trap cleanup EXIT
docker image inspect "$IMG" >/dev/null 2>&1 || { echo "✗ toolchain image $IMG missing"; exit 1; }
docker rm -fv "$MOCK" >/dev/null 2>&1 || true; docker network rm "$NET" >/dev/null 2>&1 || true
docker network create "$NET" >/dev/null

cat > "$TMPD/mock.py" <<'PY'
import json
from http.server import BaseHTTPRequestHandler, HTTPServer

class H(BaseHTTPRequestHandler):
    polls = 0
    def _send(self, code, obj):
        b = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(b)))
        self.end_headers()
        self.wfile.write(b)
    def _bearer(self):
        return self.headers.get("Authorization", "").startswith("Bearer ")
    def do_POST(self):
        ln = int(self.headers.get("content-length", "0") or 0)
        if ln:
            self.rfile.read(ln)
        p = self.path.split("?")[0]
        if p == "/v1/github/device/start":
            return self._send(200, {"device_code": "dc-1", "user_code": "WXYZ-1234",
                                    "verification_uri": "https://github.com/login/device",
                                    "expires_in": 60, "interval": 1})
        if p == "/v1/github/device/poll":
            H.polls += 1
            if H.polls < 2:
                return self._send(200, {"status": "authorization_pending"})
            return self._send(200, {"access_token": "fake.session.jwt.ABC"})
        if p.startswith("/v1/orgs/"):
            if not self._bearer():
                return self._send(401, {"error": "unauthorized"})
            if p.endswith("/github/connect/start"):
                return self._send(201, {"nonce": "n-1",
                                        "install_url": "https://relay.example/api/connect-start?nonce=n-1"})
            if p.endswith("/github/link"):
                return self._send(200, {"linked": True})
            if p.endswith("/github/sync"):
                return self._send(200, {"teams": 2, "members": 5, "repos": 3, "roles_seeded": 3})
        return self._send(404, {"error": "not_found"})
    def log_message(self, *a):
        pass

print("mock listening", flush=True)
HTTPServer(("0.0.0.0", 9099), H).serve_forever()
PY

echo "[1/3] start mock grobase github API"
docker run -d --name "$MOCK" --network "$NET" -v "$TMPD":/m -w /m python:3-alpine python3 mock.py >/dev/null
for _ in $(seq 1 60); do docker logs "$MOCK" 2>&1 | grep -q "mock listening" && break; sleep 1; done
docker logs "$MOCK" 2>&1 | grep -q "mock listening" || { echo "  ✗ mock never started"; exit 1; }
echo "    ✓ mock listening"

echo "[2/3] 42ctl device login + org verbs (guard + Bearer)"
docker run --rm --network "$NET" -v "$C42":/work -w /work $AV \
  -e FT_PASSPHRASE="v13-pass" -e FT_CONFIG=/tmp/c.json -e FT_KEYSTORE=/tmp/ks.v42 \
  "$IMG" sh -c '
    set -e
    cargo build --quiet
    B=/work/target/debug/42ctl
    $B config endpoint --server http://unused --authority http://'"$MOCK"':9099 --grobase http://'"$MOCK"':9099 >/dev/null
    echo "--- org sync BEFORE login must be refused (no session) ---"
    if $B org github sync ORG1 >/dev/null 2>&1; then echo "  PRELOGIN_FAIL (allowed without session)"; exit 1; else echo "  PRELOGIN_GUARD_OK"; fi
    echo "--- auth login --github (device flow) ---"
    $B auth login --github
    echo "--- org github connect ---"; $B org github connect ORG1 | grep -q "install_url" && echo "  CONNECT_OK" || { echo "  CONNECT_FAIL"; exit 1; }
    echo "--- org github link ---"; $B org github link ORG1 my-gh-org | grep -q "linked" && echo "  LINK_OK" || { echo "  LINK_FAIL"; exit 1; }
    echo "--- org github sync (Bearer carried → mock returns counts) ---"; $B org github sync ORG1 | grep -q "repos" && echo "  SYNC_OK" || { echo "  SYNC_FAIL"; exit 1; }
    echo "--- logout clears the session → sync refused again ---"; $B auth logout >/dev/null
    if $B org github sync ORG1 >/dev/null 2>&1; then echo "  LOGOUT_FAIL (still authed)"; exit 1; else echo "  LOGOUT_CLEARS_SESSION_OK"; fi
  ' || { echo "  ✗ 42ctl github CLI smoke failed"; exit 1; }

echo "[3/3] relay HMAC conformance (documented v1 scheme; server verify = m163)"
docker run --rm node:20-alpine node -e '
  const crypto = require("node:crypto");
  const secret = "relay-secret-xyz";
  const body = JSON.stringify({ installation_id: 12345, state: "n-1" });
  const ts = Math.floor(Date.now() / 1000).toString();
  const bh = crypto.createHash("sha256").update(body).digest("hex");
  const sig = crypto.createHmac("sha256", secret).update(`v1\n${ts}\n${bh}`).digest("hex");
  const header = `v1.${ts}.${sig}`;
  function verify(sec, hdr, b, now) {
    const p = hdr.trim().split(".");
    if (p.length !== 3 || p[0] !== "v1") return false;
    const t = parseInt(p[1], 10);
    if (!Number.isFinite(t) || Math.abs(now - t) > 300) return false;
    const h = crypto.createHash("sha256").update(b).digest("hex");
    const exp = crypto.createHmac("sha256", sec).update(`v1\n${p[1]}\n${h}`).digest("hex");
    try { return crypto.timingSafeEqual(Buffer.from(exp), Buffer.from(p[2])); } catch { return false; }
  }
  const now = Math.floor(Date.now() / 1000);
  if (!verify(secret, header, body, now)) { console.log("  SIGN_VERIFY_FAIL"); process.exit(1); }
  if (verify(secret, header, body.replace("12345", "99999"), now)) { console.log("  TAMPER_ACCEPTED_FAIL"); process.exit(1); }
  if (verify("other", header, body, now)) { console.log("  WRONG_SECRET_ACCEPTED_FAIL"); process.exit(1); }
  console.log("  RELAY_HMAC_CONFORMANCE_OK");
' || { echo "  ✗ relay HMAC conformance failed"; exit 1; }

echo "✅ v13 PASS — device-login, org guard + Bearer transmission, logout clears session, relay HMAC conformant"
