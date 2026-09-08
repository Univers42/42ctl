# 42ctl — Operations Runbook

How to build, cut a release, rotate publish credentials, and revoke a bad release. `42ctl`
decrypts plaintext locally, so its supply chain is part of the vault threat model — treat every
publish path as a security control, not a packaging convenience.

Roles referenced below:

- **`scripts/release.sh`** — the only way a release is cut: bumps the version, commits, tags,
  pushes with the operator's `GH_PAT`. See "Cut a release".
- **`release.yml`** (ours, SHA-pinned) — on a `vX.Y.Z` tag: static musl binaries for x86_64 +
  aarch64, `SHA256SUMS`, SLSA provenance, GitHub Release. What `install.sh` and `42ctl update`
  consume (D11).
- **`sign-release.yml`** (ours, gated) — cosign keyless `sign-blob` of every release artifact
  + a CycloneDX source SBOM.
- **`docker.yml`** (ours, gated) — separate `buildx` job → multi-arch image on Docker Hub,
  cosign-signed + SBOM + provenance.
- **`ci.yml`** — per-PR gate (must be green to merge).

There is no crates.io, npm or Homebrew channel — Linux binaries via GitHub Releases, plus the Docker
image. The image / signing jobs run inside the protected `publish` GitHub Actions environment
(required reviewers; environment-scoped secrets). Every action in every workflow is **pinned by
commit SHA** (re-pin with `pinact run`).

## Build & test (locally)

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release           # target/release/42ctl
docker build -t 42ctl:dev .     # minimal non-root image (no push)
```

CI (`.github/workflows/ci.yml`) runs the same plus `cargo audit` + `cargo deny` + `gitleaks` and a
no-push Docker build. Must be green to merge.

Before tagging, either preflight validates without publishing anything:

```sh
sh scripts/release.sh patch --dry-run      # token loads, tree clean, on main, tag free
sh scripts/release-dryrun.sh v0.2.0        # + a containerised release build; version must equal the tag
```

## Cut a release

Releases are **tag-driven**: one `vX.Y.Z` tag in, one GitHub Release out (D11). The tag is cut
by `scripts/release.sh` — never by hand — so the crate version, the commit and the tag can't drift.

```sh
sh scripts/release.sh patch --dry-run   # preview: what would be bumped, committed, tagged, pushed
sh scripts/release.sh patch             # 0.1.0 → 0.1.1   (or: minor | major | vX.Y.Z)
```

What it does, in order — each step refuses to continue if the previous one is not true:

1. **Preflight** — clean working tree, on `main`, in sync with `origin/main`, tag does not exist.
2. **Bump** — the first `version = "…"` in `Cargo.toml` and the `c42` block in `Cargo.lock`.
3. **Commit + tag** — `release: vX.Y.Z`, then an annotated tag (`-s` when `user.signingkey` is set).
4. **Push** — `main` + the tag over HTTPS. `GH_PAT` (env, or the git-ignored `./.env`, see
   `.env.example`) is handed to git by a one-shot credential helper that reads it from the
   environment, so the token is never on a command line, in shell history, or in git config.

It prints the Actions URL to watch and the Release URL that will exist a few minutes later.

### `release.yml` — what the tag triggers

1. **Guard** — the tag version must equal `Cargo.toml`'s; otherwise the job fails before building.
2. **Build** — `cargo build --release --locked` for `x86_64-unknown-linux-musl` on `ubuntu-24.04`
   and `aarch64-unknown-linux-musl` on `ubuntu-24.04-arm` (native runners, no cross toolchain),
   `RUSTFLAGS=-C target-feature=+crt-static` → fully static, runs on any distro. `FT_GIT_SHA` is
   the release commit, so `42ctl version` reports it.
3. **Assets** — `42ctl-<target>` (raw binaries) + `SHA256SUMS`, SLSA build provenance attested
   (`actions/attest-build-provenance`).
4. **Publish** — `gh release create vX.Y.Z` with generated notes prefixed by
   `.github/release-notes.md` (the install one-liner + asset table).

On `release: published`, `sign-release.yml` (cosign keyless `.sig`/`.pem` + CycloneDX SBOM) and
`docker.yml` (multi-arch image → Docker Hub) run in the protected `publish` environment.

### How users receive it

- **New install:** `curl -fsSL https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | sh`
  — resolves the tag by following `releases/latest`, downloads the asset + `SHA256SUMS`, verifies,
  installs to `~/.local/bin` (`/usr/local/bin` as root), fixes `PATH`.
- **Existing install:** `42ctl update` (`--check` to look, `--version X.Y.Z` to pin). Same
  discovery, same verification, then an atomic rename over the running binary.

### Verify on a clean machine (Definition of Done)

`docs/RELEASE-DOD.md` is the full checklist. At minimum, in a fresh container per arch:

```sh
docker run --rm -it debian:bookworm-slim sh -c \
  'apt-get update -qq && apt-get install -y -qq curl ca-certificates >/dev/null && \
   curl -fsSL https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | sh && \
   ~/.local/bin/42ctl version && ~/.local/bin/42ctl update --check'
```

- `42ctl version` prints the tag's `X.Y.Z` **and** the release commit.
- `sha256sum -c --ignore-missing SHA256SUMS` passes; `gh attestation verify <asset> --repo Univers42/42ctl` passes.
- A **deliberately corrupted** asset is **refused** by both `install.sh` and `42ctl update`
  (checksum mismatch → nothing written).
- The `update` path: install the previous tag with `--version`, then `42ctl update` lands on the new one.

## Credentials

`DOCKER_LOGIN` and `DOCKER_PAT` live **only** as environment-scoped GitHub Actions secrets on the
protected `publish` environment — never printed, committed, or baked into an image. `release.yml`
needs nothing beyond the job's own `GITHUB_TOKEN`; cosign is keyless (OIDC) and needs no key at
all. `GH_PAT` is an **operator-local** token (`./.env`, git-ignored) used only by
`scripts/release.sh` to push the tag — it is not a CI secret.

| Secret | Used by | Scope | Preferred replacement |
|---|---|---|---|
| `GH_PAT` | `scripts/release.sh` (push `main` + tag) | operator's `.env` | fine-grained PAT, `contents:write` on this repo, short expiry |
| `DOCKER_LOGIN` | `docker.yml` (login + image namespace) | publish env | — (username) |
| `DOCKER_PAT` | `docker.yml` (registry auth) | publish env | short-lived PAT, rotate on schedule |

### Rotate a publish credential

The rotation shape is always **revoke the old → mint the new → update the environment secret**, then
verify a dry-run/release picks it up. Never delete the old before the new is in place if a release is
mid-flight; otherwise revoke-first.

Set a secret (CLI):

```sh
gh secret set DOCKER_PAT  --env publish --repo Univers42/42ctl
gh secret set DOCKER_LOGIN --env publish --repo Univers42/42ctl
```

- **`GH_PAT`** — revoke the old token in `github.com → Settings → Developer settings → Tokens`,
  mint a fine-grained one with `contents: write` on `Univers42/42ctl` only, put it in `./.env`.
- **Docker Hub (`DOCKER_PAT`)** — revoke the old PAT at `hub.docker.com → Account Settings →
  Security → Access Tokens` → create a new PAT with **Read/Write** scope → `gh secret set
  DOCKER_PAT --env publish`. If the publishing account/namespace changed, also update
  `DOCKER_LOGIN` (it doubles as the image namespace in `docker.yml`).
- **cosign** — keyless (OIDC); there is **no key to rotate**. Trust is the workflow identity in the
  Fulcio certificate, which rotates automatically per run.

After any rotation, re-run `sh scripts/release.sh patch --dry-run` (it loads the token and
runs the preflight without changing anything) and confirm the next gated job authenticates.

## Revoke / yank a compromised release

A signed + provenance-attested release means a tampered build is already detectable and refused by
any verifying installer — yanking closes the *install* path and the *trust* signal. Move fast and in
this order:

1. **GitHub Release** — the source of the binaries (what `install.sh` and `42ctl update` pull;
   deleting the assets makes both refuse with "no asset", and `releases/latest` moves to the
   previous good release). Mark it as a security release (step 4) and, if needed, delete its assets so the
   bad binary can't be fetched:

   ```sh
   gh release delete-asset vX.Y.Z '*' --repo Univers42/42ctl --yes   # or delete the whole release
   ```
2. **Docker Hub** — delete (or re-point) the bad tag so it cannot be pulled:

   ```sh
   # delete the tag via the Docker Hub UI, or:
   curl -s -X DELETE -H "Authorization: JWT ${HUB_JWT}" \
     "https://hub.docker.com/v2/repositories/${DOCKER_LOGIN}/42ctl/tags/vX.Y.Z/"
   ```
3. **GitHub Release / advisory** — mark the GitHub Release as a security release and open a GitHub
   **Security Advisory** (GHSA) describing the affected versions and the fix:

   ```sh
   gh release edit vX.Y.Z --repo Univers42/42ctl --notes "SECURITY: yanked — see GHSA-xxxx"
   ```
4. **Rotate every credential that may have leaked** — "Rotate a publish credential" above for
   `GH_PAT` / `DOCKER_PAT`. cosign is keyless, so nothing to rotate there.
5. **Ship the fix** — cut `X.Y.(Z+1)` immediately through the normal gated release; the SBOM +
   provenance on the new build are the proof of what changed.

Because every artifact and image is signed + SLSA-attested, downstreams that verify (the installers,
`cosign verify`, `gh attestation verify`) will already refuse the tampered build before it runs.
