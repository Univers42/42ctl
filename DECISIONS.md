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
  on `vault42-core` (tag `v0.1.1`) — never a copy. Same for the Protobuf spine via `vault42-proto`.
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
