# 42ctl — Architecture Decision Records

`42ctl` is the umbrella platform CLI for the 42 stack (grobase + vault42 + future apps).
Decisions are deliberate and recorded; supply-chain integrity is a security control, not
packaging convenience.

## D0 — Precedence

`security ≈ correctness > performance > minimalism > readability > style`. `42ctl` decrypts
plaintext locally, so **its supply chain is part of the vault threat model** — every release
is signed + provenance-attested, every installer verifies before executing.

## D1 — Reconciliation with the vault42 build (done first)

- **`42ctl` supersedes the planned `vault42/cli`.** There is no separate vault CLI; the vault
  verbs are the `42ctl vault`/`secrets` group, and `unseal` is `42ctl unseal`. The already-shipped
  `vault42-cli` (in vault42 v0.1.1, deployed) is kept but superseded — a thin reference client,
  not deleted (deletion-gate discipline). Recorded in vault42 `DECISIONS.md` D12.
- **The crypto is the future standalone `vault-crypto` crate.** Until it is published to crates.io
  (a gated, irreversible step), `42ctl` depends on the audited crypto **via a pinned git dependency**
  on `vault42-core` (tag `v0.1.2`) — never a copy. Same for the Protobuf spine via `vault42-proto`.
- **`42ctl` depends on `contracts/`** (the Protobuf spine) for its gRPC client, through
  `vault42-proto`.

> **Open dependency item (resolved at P3):** building those git deps in Docker/CI needs the
> vault42 repo reachable without interactive auth. Options: (a) make vault42 public (it is AGPL,
> designed to be) so https git deps resolve; (b) publish `vault-crypto`/`contracts` crates;
> (c) a CI deploy key. Default lean: **(a)**, decided when P3 wires the crypto. P0 is dependency-free.

## D2 — Dedicated repo (§12a)

`42ctl` is its **own org repo** (`Univers42/42ctl`), sibling to grobase/vault42. It is the front
door to the whole platform, has a distinct public-artifact trust posture, and carries heavy
distribution machinery (5+ channels, signing, provenance) that should not clutter a backend repo.
Dependency flow stays clean: `42ctl → {contracts, vault-crypto}`.

## D3 — Names (§12b)

The command is always **`42ctl`**. crates.io names can't lead with a digit, so the crate is
**`c42`** with `[[bin]] name = "42ctl"`. npm publishes the scoped package **`@universe42/42ctl`**
(the org's existing npm scope is `universe42`; note the GitHub org is `Univers42`). Docker image
under the Docker Hub login.

## D4 — v1 channels (§12c)

Ship in v1: **`curl | sh`, GitHub Releases, npm (`@universe42`), Docker (Docker Hub), cargo /
cargo-binstall, Homebrew tap.** Deferred: winget/scoop/AUR/Nix, MSI. `cargo-dist` is the release
engine for the source/binary channels + shell/PowerShell installers + npm + Homebrew + the
self-updater; Docker is a separate `buildx` job.

## D5 — Docker registry (§12d) + signing (§12e) + credentials (§12f)

- **Registry: Docker Hub** (the provisioned `DOCKER_LOGIN`/`DOCKER_PAT`). Multi-arch
  (amd64+arm64), minimal runtime, non-root, cosign-signed, SBOM + provenance attached.
- **Signing: cosign / sigstore keyless** (GitHub OIDC — no long-lived key to manage); public
  verification instructions published.
- **Credentials: CI secrets only**, environment-scoped on a protected publish environment with
  required reviewers; never printed, never committed, never baked into images. Prefer **npm OIDC
  trusted publishing + `--provenance`** over the long-lived `NPM_TOKEN` where the registry allows.

## D6 — Architecture

Hexagonal. `cli` (clap types) → `cmd` (thin handlers) → `core` (pure use-cases, the bulk of the
tests) → `adapters` (api client, creds/keyring, config/profiles, updater). No globals; config and
clients are constructed at the edge and threaded down. The crate forbids `unsafe`.

## D7 — Self-security

OS keyring first for keys/tokens, Argon2id-passphrase keystore fallback; short-lived, revocable,
per-profile auth credentials cleared on `logout`; `zeroize` on every key/plaintext/token buffer;
no secret in logs/errors/traces/crash dumps/shell history; `update` verifies signature + provenance
+ checksum and only then atomically swaps the binary (a failed verification changes nothing).

## D8 — Release engine: who owns what (P4/P5/P7)

`cargo-dist` (`dist` 0.28.0) is the release engine, configured in `[workspace.metadata.dist]` and
**regenerable** with `dist generate`. To keep that file authentic, we do **not** hand-edit
`release.yml`; instead the ownership is split so each concern lives where it can be owned cleanly:

- **`release.yml` (dist-owned):** matrix build (6 targets, incl. linux musl), SHA-256 checksums,
  **SLSA build provenance** (`actions/attest-build-provenance`, sigstore-keyless), the
  `curl|sh` + PowerShell + npm + Homebrew **installer artifacts**, the **self-update receipt**
  (`install-updater = true`), GitHub Release upload, and the **Homebrew tap** push. `dist` enforces
  this file matches its generator (`dist plan` aborts otherwise), so it must stay generator-pure.
- **`publish.yml` (ours, gated):** the **npm publish** — moved out of dist's `publish-jobs` so it
  can run with `npm publish --provenance` (registry provenance) **and** the protected `publish`
  environment, because an npm publish is irreversible (no unpublish after 72h).
- **`sign-release.yml` (ours, gated):** explicit **cosign keyless `sign-blob`** over every release
  artifact (so `cosign verify-blob` in `SECURITY.md` is real) + a CycloneDX **source SBOM**.
- **`docker.yml` (ours, gated):** the multi-arch image → Docker Hub, **cosign-signed** + SBOM +
  provenance.

## D9 — `cargo install c42` from crates.io is NOT a channel (yet)

`c42` depends on `vault42-core`/`vault42-proto` via **git** dependencies, which **crates.io forbids**
in a published crate. So the **"cargo" channel is `cargo binstall c42`** (it pulls the signed
GitHub-Release binary cargo-dist publishes — no compile, no crates.io). A true `cargo install c42`
from source becomes possible only once `vault-crypto`/`vault42-proto` are themselves published to
crates.io (a separate gated step in vault42). Documented so nobody assumes a broken channel works.

## D10 — Supply-chain CI hardening (P7)

Every action in the workflows **we own** (`ci.yml`, `docker.yml`, `publish.yml`, `sign-release.yml`)
is **pinned by commit SHA** (resolved from the tag, comment carries the tag), checkouts use
`persist-credentials: false`, jobs run least-privilege `permissions`, and per-ref `concurrency`
cancels stale runs. The **dist-owned `release.yml`** pins its own dist version (`v0.28.0`) but uses
tag-pinned actions; re-pin it with `pinact run` (or `ratchet`) **after every `dist generate`**, since
regeneration reverts SHA pins. All registry/​image publishes are gated behind the protected
`publish` GitHub Environment (required reviewers + environment-scoped secrets); cosign is keyless, so
there is no signing key to leak or rotate.
