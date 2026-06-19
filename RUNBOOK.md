# 42ctl — Operations Runbook

How to build, cut a release, rotate publish credentials, and revoke a bad release. Fleshed out
through P4–P8; this is the starting frame.

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

## Cut a release (gated — wired in P4–P8)

Releases are **tag-driven** and run through a protected CI environment with required reviewers.

1. Bump the version, update the changelog, commit on `main`.
2. Push a signed semver tag: `git tag -s vX.Y.Z && git push origin vX.Y.Z`.
3. `release.yml` (P4+) builds the cross-platform matrix via `cargo-dist`, produces SHA-256
   checksums, **signs** all artifacts (cosign keyless), attaches **SLSA provenance** + an **SBOM**,
   and publishes to GitHub Releases + npm (`--provenance`) + crates.io + the Homebrew tap; a
   separate `buildx` job publishes the multi-arch, cosign-signed Docker image to Docker Hub.
4. Verify each channel on a clean machine (`§11` Definition of Done): the installed `42ctl version`
   matches the tag, and signature/checksum verification passes; a deliberately corrupted artifact
   is refused.

## Credentials

`NPM_TOKEN`, `DOCKER_LOGIN`, `DOCKER_PAT` live **only** as environment-scoped GitHub Actions secrets
on the protected publish environment — never printed, committed, or baked into an image. Prefer npm
**OIDC trusted publishing** over the long-lived `NPM_TOKEN` where possible.

### Rotate a publish credential
- **npm:** revoke the old token in npmjs.com → mint a new automation token (or switch to OIDC) →
  update the `NPM_TOKEN` repo/environment secret.
- **Docker Hub:** revoke the old PAT in Docker Hub → create a new PAT (write scope) → update the
  `DOCKER_PAT` secret (and `DOCKER_LOGIN` if the account changed).

## Revoke / yank a compromised release
1. `cargo yank --version X.Y.Z` (crates.io); `npm deprecate @universe42/42ctl@X.Y.Z "compromised"`.
2. Delete/retag the bad Docker tag; publish a fixed `X.Y.Z+1` immediately.
3. Mark the GitHub Release as a security advisory; rotate any credential that may have leaked.
4. Because every artifact is signed + provenance-attested, downstreams that verify will already
   refuse a tampered build; the yank closes the install path.
