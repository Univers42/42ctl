# 42ctl — Security & Release Definition of Done

The release is **not done** until every box below is checked. This is the §11 acceptance gate:
`42ctl` decrypts plaintext locally, so its supply chain is part of the vault threat model — these
items are security controls, not packaging niceties. Verify against a real tag (`vX.Y.Z`); a box
is only checked when it has been *demonstrated*, not assumed.

Companion docs: `RUNBOOK.md` (release / rotate / yank procedures), `SECURITY.md` (user-facing
verification per channel), `scripts/release-dryrun.sh` (pre-tag preflight).

## Zero-knowledge & secret handling

The binary that ships must never weaken the local-decryption guarantee or leak key material.

- [ ] **Zero-knowledge preserved** — the server never sees plaintext or the private key; all
  decryption stays client-side in the shipped binary.
- [ ] **Keys/tokens zeroized and never logged** — every key, plaintext, and token buffer is
  `zeroize`d on drop; no secret appears in logs, errors, traces, crash dumps, or shell history.
- [ ] **Auth tokens are short-lived, keyring-stored, and revocable** — OS keyring first
  (Argon2id-passphrase keystore fallback); per-profile, cleared on `logout`; revocable server-side.

## Signing, provenance & SBOM

Every published byte is attributable to this pipeline and reproducible from the SBOM.

- [ ] **Every artifact and image is signed** — all GitHub Release artifacts (cosign keyless
  `verify-blob`) and the Docker image (cosign keyless `verify`) carry valid signatures from the
  release/docker workflow OIDC identity.
- [ ] **SLSA provenance on binaries and images** — build provenance is attested and verifies with
  `gh attestation verify` / `slsa-verifier` for the binaries and the image.
- [ ] **npm provenance** — `@universe42/42ctl` is published with `--provenance`; the npmjs.com
  package page shows the provenance panel and `npm audit signatures` passes.
- [ ] **SBOM per release** — a CycloneDX/SPDX SBOM is generated and attached for the release
  artifacts and the image.

## Installer & update integrity

A tampered artifact must be refused before it ever runs or swaps in.

- [ ] **Every installer verifies signature/checksum and refuses tampered artifacts** — the
  `curl | sh`, PowerShell, npm, Homebrew, and cargo-binstall paths verify before placing the
  binary. **Demonstrated** with a *deliberately corrupted* artifact: the installer refuses it and
  leaves nothing on disk.
- [ ] **Clean-machine install per channel yields matching version + commit** — on a fresh
  machine/container, each v1 channel (curl|sh, GitHub Releases, npm, Docker, cargo/cargo-binstall,
  Homebrew) installs a binary whose `42ctl version` reports the released `X.Y.Z` **and** the release
  commit.
- [ ] **Update verifies-before-swap** — `42ctl update` verifies signature + provenance + checksum
  and only then atomically swaps the binary; a failed verification changes nothing.

## Container hardening

The image is a minimal, non-root, multi-arch artifact.

- [ ] **Docker image is minimal, non-root, multi-arch** — `FROM scratch` (static musl binary + CA
  bundle only), runs as a non-root UID, built for `linux/amd64` + `linux/arm64`.

## CI/CD hardening

The pipeline that produces trust must itself be trustworthy.

- [ ] **Actions pinned by commit SHA** — every `uses:` in `ci.yml`, `release.yml`, and `docker.yml`
  is pinned to a full commit SHA, not a floating tag.
- [ ] **Least-privilege tokens** — workflow `permissions:` are minimal (`contents: read` by
  default; `id-token: write` only where cosign/provenance need it); no broad `write-all`.
- [ ] **Protected publish environment** — all registry publishes run only on a signed semver tag
  inside the protected `publish` GitHub Actions environment (required reviewers; environment-scoped
  secrets `NPM_TOKEN`/`DOCKER_LOGIN`/`DOCKER_PAT`/`CARGO_REGISTRY_TOKEN`, never printed or baked in).
  Prefer OIDC trusted publishing (npm) and keyless cosign over long-lived tokens.

## Documentation

The operator has the procedures before they need them.

- [ ] **RUNBOOK covers release, rotate, and yank** — `RUNBOOK.md` documents cutting a release,
  rotating each publish credential, and yanking/revoking a compromised release, with concrete
  commands.
