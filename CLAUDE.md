# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`42ctl` — the single-binary platform CLI for the 42 stack (grobase + vault42). Crate is `c42`
(crates.io forbids a leading digit), binary is `42ctl`. It decrypts plaintext **locally**
(zero-knowledge): the server only ever holds opaque ciphertext, so the CLI's own supply chain is
part of the vault threat model. `DECISIONS.md` is the ADR log, `RUNBOOK.md` the release/rotation
procedures, `docs/RELEASE-DOD.md` the acceptance gate.

## Build / lint / test

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings   # CI treats warnings as errors
cargo test
cargo test core::merge                      # one module
cargo test config_json_round_trips          # one test by name
cargo build --release                       # target/release/42ctl
docker build -t 42ctl:dev .                 # same image CI builds (no push)
```

CI (`.github/workflows/ci.yml`) additionally runs `cargo audit`, `cargo deny check`, and the
gitleaks binary. All four jobs must be green to merge.

Local rustc may be older than the crate's `rust-version`; the Docker image the Dockerfile uses
(`public.ecr.aws/docker/library/rust:1.96-slim-bookworm`) with `cargo` cache volumes is the
reliable toolchain. The release build CI performs is a static musl one:

```sh
rustup target add x86_64-unknown-linux-musl   # + apt musl-tools
RUSTFLAGS="-C target-feature=+crt-static" cargo build --release --locked --target x86_64-unknown-linux-musl
```

### Releasing

```sh
sh scripts/release.sh patch --dry-run   # preflight only (token loads, tree clean, on main, tag free)
sh scripts/release.sh patch             # or minor | major | vX.Y.Z — bump, commit, tag, push
```

`GH_PAT` comes from the environment or the git-ignored `.env` (`.env.example`). The pushed tag runs
`.github/workflows/release.yml`, which refuses a tag that doesn't match `Cargo.toml`'s version.
`scripts/release-dryrun.sh vX.Y.Z` additionally does a containerised release build.

### End-to-end verify gates

`scripts/verify/v<NN>-*.sh` are live integration gates, not unit tests — they are **Docker-first**
and need a local vault42 checkout and the shared toolchain image:

```sh
VAULT42_DIR=/path/to/vault42 RUST_TOOLCHAIN_IMG=mini-baas-rust-toolchain:latest \
  bash scripts/verify/v12-merge.sh
```

Each gate spins a standalone `vault42-server` on a throwaway Docker network, runs the real binary
against it, and cleans up on EXIT. A new user-visible behaviour ships with a new or extended gate.

## Architecture — hexagonal, one direction only

```
cli/ (clap types: mod.rs, vault.rs, rbac.rs)  →  cmd/ (thin handlers, one file per verb group)
                     →  core/ (pure, I/O-light use-cases — where the tests live)
                     →  ops/  (impl Session methods: orchestration over the wire)
                     →  adapters/ (gRPC/HTTP, keystore, creds, config, envelope codecs, GitHub Releases)
```

- `cmd/mod.rs::dispatch` splits offline verbs (`version`, `help`, `unseal`, `config`) from network
  verbs (including `update`), which get a fresh multi-thread tokio runtime via `block_on_net`.
  Anything that touches the network belongs in the async arm.
- `42ctl help [topic]` is the user-facing guide: content lives as consts in `cmd/help_topics.rs`
  (tiny markup: `## ` section, `$ ` command, `! ` warning) and `cmd/help.rs` renders it through
  `ui`. A new user-facing workflow gets a topic there, and the `--help` doc comment in `cli/`.
- `42ctl update` is native (`adapters/github.rs` + `adapters/checksum.rs`): resolve the tag by
  following GitHub's `releases/latest` redirect (no API), download `42ctl-<target>` +
  `SHA256SUMS`, verify, then rename over `current_exe()`. The target comes from `FT_TARGET`,
  stamped by `build.rs`; the asset names must stay in step with `release.yml` and `install.sh`.
- `ops/` is not a layer of free functions — it is `impl Session` blocks split by concern
  (`secret`, `manage`, `share`, `audit`, `io`, `notes`, `sync`, `reconcile`). Every request is
  signed and contract-bound by `Session::authorize`; only opaque bytes cross the wire.
- Adapters depend on `vault42-core` (the crypto) — never the reverse. Domain code must not grow an
  adapter type in its signature.
- Crypto lives in `vault42-core` / `vault42-proto`, pinned **git** dependencies on the public
  vault42 repo (`rev` in `Cargo.toml`). Never copy crypto or protobuf code into this repo — bump the
  pin instead (`DECISIONS.md` D1). Those git deps are also why `cargo install c42` from crates.io is
  not a channel: distribution is `install.sh` / `42ctl update` off the GitHub Release (D11).
- The crate is `unsafe_code = "forbid"` at the manifest level.

### Zero-knowledge model (the invariant to preserve)

- `core/manifest.rs` is the only place real relative paths exist; it is sealed like any other secret,
  so blob entries the server can see carry opaque vault paths only.
- `core/projpath.rs::validate_stored` is the Zip-Slip guard — a **pure string check** run before any
  path touches the filesystem. A violating path is an error, never sanitized.
- A project is rooted at a `.42ctl/` marker dir holding a stable `project_id` (`core/project.rs`);
  that id is what lets a second machine pull the same tree.
- `core/merge.rs` holds the 3-way pull reconciliation (fast-forward / keep-local / conflict markers)
  as pure decision logic — keep it I/O-free so `cargo test core::merge` stays the fast gate.

### State on disk

Everything is per-profile and lives beside the config file, so `FT_CONFIG` isolates a whole
environment: `config.json` (profiles → `Endpoint { server, authority, grobase }`),
`contract-<profile>.tok`, `session-<profile>.tok`. Env overrides: `FT_CONFIG`, `FT_CONTRACT`,
`FT_SESSION`, `FT_KEYSTORE`, `FT_PASSPHRASE`, `FT_PROFILE`. Two distinct credentials exist and are
not interchangeable — the vault42 **contract** (`adapters/creds.rs`, sent as `x-v42-contract`) and
the grobase **session JWT** from the GitHub device flow (`adapters/session.rs`, sent as Bearer).

## Repo conventions

- Every source file (`.rs`, `.sh`, `.py`) opens with the 11-line 42 banner. Generate it, don't hand-
  type it: `python3 scripts/ops/gen-42-header.py <file>...` (idempotent).
- `--help` text is the doc comment on each clap variant/arg in `src/cli/`; keep every arg
  documented with a `value_name`.
- Commits: `type(scope): what — why`, e.g. `feat(sync): …`, `chore(deps): bump … (RUSTSEC-…)`.
- The rules in `.claude/rules/` are binding (refactor-rust, no-globals, comments-above-declarations,
  minimalism ladder). In particular: no package-level mutable state — config and clients are built at
  the edge and threaded down; no prose comments inside a function body.

## Release ownership (do not blur these)

- `release.yml` (D11) is ours: `vX.Y.Z` tag → static musl binaries for x86_64 + aarch64 on
  native runners → `SHA256SUMS` + SLSA provenance → GitHub Release with raw assets
  `42ctl-<target>`. `install.sh` (repo root) and `42ctl update` consume exactly those names.
- `sign-release.yml` (cosign keyless) and `docker.yml` (multi-arch image) trigger on the published
  release, gated behind the protected `publish` environment. Every action is pinned by commit SHA.
- A release is cut only by `scripts/release.sh`; nothing is published by hand. There is no npm,
  Homebrew or crates.io channel (git deps forbid crates.io).
