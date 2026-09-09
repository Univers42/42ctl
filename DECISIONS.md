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

- **Registry: Docker Hub** (`docker.io/dlesieur/*`, pushed with the `DOCK_PAT` repository secret). Multi-arch
  (amd64+arm64), minimal runtime, non-root, cosign-signed, SBOM + provenance attached.
- **Signing: cosign / sigstore keyless** (GitHub OIDC — no long-lived key to manage); public
  verification instructions published.
- **Credentials: CI secrets only** (the repository secret `DOCK_PAT`); never printed, never
  committed, never baked into images. Keyless OIDC (cosign, provenance) everywhere else.

## D6 — Architecture

Hexagonal. `cli` (clap types) → `cmd` (thin handlers) → `core` (pure use-cases, the bulk of the
tests) → `adapters` (api client, creds/keyring, config/profiles, updater). No globals; config and
clients are constructed at the edge and threaded down. The crate forbids `unsafe`.

## D7 — Self-security

OS keyring first for keys and tokens, with an Argon2id passphrase keystore as the fallback; short-lived, revocable,
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

## D11 — Owned release engine: static Linux binaries, `install.sh`, native `update`

Supersedes D4, D8, D9 for the binary channel. `cargo-dist` never shipped a release here (the
`v0.1.0` run was cancelled; no GitHub Release ever existed), it emitted a workflow we were not
allowed to edit, and it targeted channels (npm, Homebrew, PowerShell, Windows, macOS) the
project does not need. Replaced by three small, owned pieces:

- **`release.yml` (ours, SHA-pinned):** a `vX.Y.Z` tag builds **static musl binaries** for
  `x86_64` and `aarch64` on native GitHub runners (`ubuntu-24.04`, `ubuntu-24.04-arm` — no
  cross toolchain), refuses a tag whose version differs from `Cargo.toml`, attests SLSA build
  provenance, writes `SHA256SUMS`, and publishes the GitHub Release. Static musl means one
  binary per arch runs on every distro (Debian/Ubuntu/Fedora/Arch/Alpine/NixOS) with no libc
  dependency. Assets are raw binaries (`42ctl-<target>`), not tarballs — one code path for the
  installer and the updater, no archive crate in the CLI.
- **`install.sh` (repo root, POSIX sh):** `curl -fsSL …/install.sh | sh`. Detects the arch,
  resolves the latest tag by following GitHub's `releases/latest` redirect (no API, no rate
  limit, no token), downloads the asset + `SHA256SUMS`, verifies, installs to `~/.local/bin`
  (or `/usr/local/bin` as root), fixes `PATH`. A failed check leaves nothing on disk.
- **`42ctl update` (native, `adapters/github` + `adapters/checksum`):** same discovery, same
  verification, then an atomic rename over `current_exe()`. No install receipt is needed — it
  works for any install location the user can write to. `--check` reports, `--version X.Y.Z`
  pins. `axoupdater` is gone.
- **`scripts/release.sh`:** the only way a tag is cut — bumps `Cargo.toml` + `Cargo.lock`,
  commits `release: vX.Y.Z`, tags, pushes over HTTPS with `GH_PAT` read from the environment by a
  one-shot credential helper (never on a command line).

Consequences: `publish.yml` (npm) is deleted — it consumed a dist-generated package that no
longer exists. The **`cargo` channel is dropped** (D9's `cargo binstall` relied on dist's
archive naming). Windows/macOS are out of scope (Linux only; Docker elsewhere).

**Amendment — chaining and the image.** A release created with the job's `GITHUB_TOKEN` emits
no `release: published` event to other workflows, so `sign-release.yml` and `docker.yml`
chain on `release.yml` with `workflow_run` and run unattended (the operator asked for a fully
green, self-approving board; `sign-release` keeps `environment: publish` so reviewers can be
re-added there). The
Docker image no longer compiles from source under QEMU: `deploy/Dockerfile.dist` is
`FROM scratch` + the **released** static musl binary per arch, fetched and SHA-256-verified
against the release's `SHA256SUMS` inside the build — the image ships the exact attested
bytes and builds in seconds. The repo-root `Dockerfile` stays the from-source reproducible
build proven in `ci.yml`.

## D12 — Large objects: the vault keeps the keys, an object store keeps the bytes

A file above the transport ceiling is split into chunks that go to an S3-compatible store, and
the vault receives only the chunk list, sealed as an ordinary blob. The author signature already
covers that list, so the count, the order and every chunk's length are signed with no new object
type and **no change to the frozen envelope format**. The store never sees a key. Configured per
profile (`config endpoint --blobstore --bucket --region`); the **credential is read from
`FT_S3_KEY` / `FT_S3_SECRET` and is never written to the config file**, which is plain JSON.

**A chunk is named by its content, not its position.** `chunks/<blinded-namespace>/<keyed BLAKE3
of the plaintext>`. This is a correctness rule before it is an optimisation: an upload skips any
chunk the store already holds, and with a positional name that skip silently drops changed bytes
— the list still validates and the read returns the previous version. Naming by content also
makes a second version cost only the chunks that differ, which is what makes keeping full history
affordable. The hash is **keyed** so the store operator cannot test whether we hold a file they
already have; the namespace is a one-way function of the principal so collection can walk one
identity's objects without a listing naming its owner.

Dedup reaches across one identity's own objects and versions. Sharing chunks **between members of
one environment** needs identical plaintext to seal to identical ciphertext, which is
`vault42_core::seal_chunk` — per-environment by our user's choice, because tenant-scoped
convergent encryption leaks which parts of a file changed between versions.

**Collection is mandatory, not optional.** Chunks are never overwritten, so every edit leaves its
predecessors behind. `vault gc` walks **every version of every manifest**, refuses outright when
it found no manifests at all (an unread reference set and an empty one are indistinguishable from
the deletion side), never touches a chunk younger than a grace period (an interrupted push has
already uploaded chunks no manifest names yet, and collecting them is what makes a resumable
upload unresumable), and is a dry run unless `--apply`.

**The manifest reader fails closed on a version it does not know.** `Manifest.version` existed
from the first commit and was read by nothing — a field with no reader is a gate that cannot
fail. serde drops an unknown field silently, so a manifest from a newer client parses cleanly and
the reader carries on with a partial understanding of what each entry MEANS: a chunked entry read
by a client that does not know the flag writes the chunk list to disk in place of the file and
reports it restored. `parse` now refuses a version above `manifest::VERSION`. This does nothing
for clients already released, whose damage is unfixable; it decides whether the class recurs.

**History has to be reachable to count.** `Entry.rev` records which blob revision a manifest
version was written against, and `pull --at <version>` fetches each file at that revision.
Reading an old manifest while fetching the latest blobs reproduces a tree that never existed —
the file names of one moment with the contents of another. Reconciliation is unchanged, so
restoring over local edits is `--at N --apply --force` rather than a silent overwrite.

`MAX_BLOB` is the chunk size. It previously read 64 MiB while the server decodes with tonic's
4 MiB default and never raises it, so every payload between the two passed the client's own guard
and then died at the transport — a guard that converted a clear refusal into a protocol error.

