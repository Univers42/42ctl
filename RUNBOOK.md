# 42ctl — Operations Runbook

How to build, cut a release, rotate publish credentials, and revoke a bad release. `42ctl`
decrypts plaintext locally, so its supply chain is part of the vault threat model — treat every
publish path as a security control, not a packaging convenience.

Roles referenced below:

- **`auto-release.yml`** (ours) — cuts the patch release on its own once `ci` is green on `main`,
  so `releases/latest` follows `main` instead of following somebody remembering. See "Cut a
  release".
- **`scripts/release.sh`** — the same bump, commit, tag and push by hand, for a `minor`, a `major`,
  or a specific version. See "Cut a release".
- **`release.yml`** (ours, SHA-pinned) — on a `vX.Y.Z` tag: static musl binaries for x86_64 +
  aarch64, `SHA256SUMS`, SLSA provenance, GitHub Release. What `install.sh` and `42ctl update`
  consume (D11).
- **`sign-release.yml`** (ours) — cosign keyless `sign-blob` of every release artifact
  + a CycloneDX source SBOM.
- **`docker.yml`** (ours) — multi-arch image → `docker.io/dlesieur/42ctl` from the verified
  release assets, cosign-signed + SBOM + provenance.
- **`ci.yml`** — per-PR gate (must be green to merge).

There is no crates.io, npm or Homebrew channel — Linux binaries via GitHub Releases, plus the Docker
image. Both follow a green `release.yml` automatically (`workflow_run`). Every action in every
workflow is **pinned by commit SHA** (re-pin with `pinact run`).

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

Releases are **tag-driven**: one `vX.Y.Z` tag in, one GitHub Release out (D11). The tag is cut by
`auto-release.yml` or by `scripts/release.sh` — never by hand — so the crate version, the commit
and the tag can't drift.

### The patch release cuts itself

Merge to `main`, and when `ci` goes green `auto-release.yml` bumps the patch version, commits
`release: vX.Y.Z`, tags, and pushes. Nothing to run.

It exists because the two installers read `releases/latest`, so what a fresh machine gets is the
last TAG, not `main` — and `main` once ran 47 commits past `v0.1.3` while every new install picked
up a binary whose help still named retired hosts. A lagging release is a shipped defect here, not
a paperwork gap.

Three properties are worth knowing before touching it:

- It triggers on `workflow_run` of `ci`, never on `push`, so a red commit is never released. It
  also re-checks that `main` still points at the revision CI passed, and stands down if `main`
  moved — that revision's own `ci` run releases it.
- It pushes with the repository secret **`GH_PAT`**, not `GITHUB_TOKEN`. GitHub suppresses workflow
  triggers for anything a job's own token pushes, so a tag pushed with `GITHUB_TOKEN` would create
  a tag and build nothing — and `sign-release.yml` and `docker.yml` both chain off `release.yml`,
  so the entire channel hangs off that push being a real `push: tags` event. The workflow fails
  loudly when the secret is absent rather than tagging into silence.
- Its own bump commit is a push to `main`, so it comes back around; the `release: v` subject is
  what it skips to stop recursing. `scripts/release.sh` writes the same subject, so a hand-cut
  release does not trigger a second automatic one either.

### By hand, for a minor, a major, or a specific version

```sh
sh scripts/release.sh patch --dry-run   # preview: what would be bumped, committed, tagged, pushed
sh scripts/release.sh minor             # 0.1.7 → 0.2.0   (or: patch | major | vX.Y.Z)
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

When `release.yml` succeeds (`workflow_run`), `sign-release.yml` (cosign keyless `.sig`/`.pem`
+ CycloneDX SBOM) and `docker.yml` (multi-arch image → `docker.io/dlesieur/42ctl`, `FROM scratch`
+ the verified release binary per arch) run. `sign-release` still goes through the `publish`
environment — add required reviewers there to gate it; `docker.yml` runs unattended.

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

`DOCK_PAT` (a Docker Hub access token for the `dlesieur` account, which is also the image
namespace `docker.io/dlesieur/42ctl`) is a **repository** Actions secret — never printed,
committed, or baked into an image. `release.yml` needs nothing beyond the job's own
`GITHUB_TOKEN`; cosign is keyless (OIDC) and needs no key at all.

`GH_PAT` lives in **both** places and they are not the same copy: the operator's git-ignored
`./.env` for `scripts/release.sh`, and a repository Actions secret for `auto-release.yml`, which
cannot use `GITHUB_TOKEN` because a tag pushed with it triggers no build. Scope the CI copy to
`contents: write` on this repository alone and nothing else — a broader token in a public
repository's Actions is reachable by anyone who can land a commit on `main`, which is the only
thing `auto-release.yml` waits for.

| Secret | Used by | Scope | Preferred replacement |
|---|---|---|---|
| `GH_PAT` | `scripts/release.sh` (push `main` + tag) | operator's `.env` | fine-grained PAT, `contents:write` on this repo, short expiry |
| `GH_PAT` | `auto-release.yml` (push `main` + tag) | repository secret | a SEPARATE fine-grained PAT, `contents:write` on this repo only, short expiry |
| `DOCK_PAT` | `docker.yml` (Docker Hub push, user `dlesieur`) | repository secret | short-lived access token, rotate on schedule |

### Rotate a publish credential

The rotation shape is always **revoke the old → mint the new → update the secret**, then
verify a dry-run/release picks it up. Never delete the old before the new is in place if a release is
mid-flight; otherwise revoke-first.

Set a secret (CLI):

```sh
gh secret set DOCK_PAT --repo Univers42/42ctl      # paste the new Docker Hub token
gh secret set DOCK_PAT --repo Univers42/vault42    # the server image uses the same account
gh secret set GH_PAT   --repo Univers42/42ctl      # the tag-pushing token auto-release.yml uses
```

- **`GH_PAT`** — revoke the old token in `github.com → Settings → Developer settings → Tokens`,
  mint a fine-grained one with `contents: write` on `Univers42/42ctl` only, put it in `./.env`.
- **Docker Hub (`DOCK_PAT`)** — revoke the old token at `hub.docker.com → Account Settings →
  Security → Access Tokens` → create a new one with **Read/Write** scope → `gh secret set
  DOCK_PAT` on both repos. The account (`dlesieur`) doubles as the image namespace and is
  spelled out in `docker.yml`.
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
     "https://hub.docker.com/v2/repositories/dlesieur/42ctl/tags/vX.Y.Z/"
   ```
3. **GitHub Release / advisory** — mark the GitHub Release as a security release and open a GitHub
   **Security Advisory** (GHSA) describing the affected versions and the fix:

   ```sh
   gh release edit vX.Y.Z --repo Univers42/42ctl --notes "SECURITY: yanked — see GHSA-xxxx"
   ```
4. **Rotate every credential that may have leaked** — "Rotate a publish credential" above for
   `GH_PAT` / `DOCK_PAT`. cosign is keyless, so nothing to rotate there.
5. **Ship the fix** — cut `X.Y.(Z+1)` immediately through the normal gated release; the SBOM +
   provenance on the new build are the proof of what changed.

Because every artifact and image is signed + SLSA-attested, downstreams that verify (the installers,
`cosign verify`, `gh attestation verify`) will already refuse the tampered build before it runs.
