# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this repo is

`42ctl` is the umbrella platform CLI for the 42 stack (grobase + vault42) — one binary that holds a
local **zero-knowledge** identity and does every plaintext seal/open on the user's machine. The
servers only ever hold opaque ciphertext. Because the binary decrypts plaintext, **its supply chain
is part of the vault threat model**: releases are signed + provenance-attested and `update` verifies
before it swaps.

**Precedence** (`DECISIONS.md` D0) overrides the global default: `security ≈ correctness >
performance > minimalism > readability > style`. Record trade-offs in `DECISIONS.md`.

The crate is **`c42`** (crates.io names can't lead with a digit); the binary is **`42ctl`**.

## Build, test, lint

**There is no host cargo** — everything runs in Docker. The image the scripts use is
`public.ecr.aws/docker/library/rust:1.96-slim-bookworm` (present on this machine). Reuse the named
cache volumes so rebuilds are not from scratch:

```sh
IMG=public.ecr.aws/docker/library/rust:1.96-slim-bookworm
C42="docker run --rm -v $PWD:/build -w /build \
  -v 42ctl-cargo-registry:/usr/local/cargo/registry \
  -v 42ctl-cargo-git:/usr/local/cargo/git $IMG"

$C42 cargo test
$C42 cargo build --release            # target/release/42ctl
$C42 sh -c 'rustup component add rustfmt clippy && cargo fmt --all -- --check'
$C42 sh -c 'rustup component add rustfmt clippy && cargo clippy --all-targets -- -D warnings'
docker build -t 42ctl:dev .           # distroless non-root image, no push
```

The slim image ships **neither rustfmt nor clippy**, hence the `rustup component add` prefix on
those two. The container needs **network**: `vault42-core` and `vault42-proto` are git dependencies
and nothing is vendored.

**A single test** — all tests are inline `#[cfg(test)] mod tests`; there is no `tests/` directory.
Filter by module path:

```sh
$C42 cargo test core::projpath        # the Zip-Slip / traversal suite
$C42 cargo test core::merge           # the 3-way pull reconciliation
$C42 cargo test adapters::scope       # scope-id + member-id derivation
```

`RUNBOOK.md` lists these commands in their bare (host-cargo) form; CI runs them that way plus
`cargo audit`, `cargo deny check`, and a gitleaks scan. CI must be green to merge.

Before tagging: `sh scripts/release-dryrun.sh vX.Y.Z` — read-only except for a containerized release
build, and it hard-fails if the `Cargo.toml` version does not equal the tag minus its `v`.

### The QA battery, and the older verify gates

`./qa/run.sh` is the real end-to-end coverage: 22 specs standing up vault42-server, the authority
and a MinIO chunk store in Docker. Its exit status counts REGRESSIONS ONLY, so it works as a merge
gate while `assert_spec` assertions stay red on purpose. `QA_SHUFFLE=1` randomises the order —
use it, because two specs have already passed only because of what ran before them. `qa/README.md`
has the rules; the one that matters most is that an absence assertion must prove its haystack.

`scripts/verify/v10-secret-sync.sh` … `v13-github-cli.sh` predate it and still work, but **both
their defaults are wrong on this machine**: they need `RUST_TOOLCHAIN_IMG` (the default image is
absent) and `VAULT42_DIR` (the sibling checkout is `../vault42`), and they **exit 1 rather than
skip**, so a missing prerequisite reads as a failure.

## Architecture

Hexagonal, each layer depending only inward. `DECISIONS.md` D6 names four; `ops/` is the fifth that
grew out of `cmd/` as the verbs got real:

| Layer | Role |
|---|---|
| `cli.rs` | clap types only — the whole command surface, no logic |
| `cmd/` | thin handlers: resolve profile → unlock identity → open a session → dispatch |
| `core/` | pure use-cases: project scan, encrypted manifest, path model, merge, materialize |
| `ops/` | `impl Session` verbs — the vault/sync/notes logic over an open session |
| `adapters/` | the I/O edge: keystore, passphrase, gRPC session, authority, grobase REST, envelope codecs |

`main.rs` → `cmd::dispatch`: `version`/`update`/`unseal`/`config` run synchronously; everything else
goes through a fresh multi-thread tokio runtime. `unsafe_code = "forbid"`.

### Two credentials, two planes — the thing that trips people up

They are unrelated and each verb needs the right one:

- **vault42 gRPC** (secrets, sync, notes, scope wraps). Every request carries `x-v42-ts` /
  `x-v42-pub` / `x-v42-sig`, an Ed25519 signature over `"{ts}\n{grpc-method}"`, plus
  `x-v42-contract` when the profile has one. The contract comes from the **authority**
  (`grobase-nano`) at `42ctl auth login` and is saved as `contract-<profile>.tok`.
- **grobase REST** (org / team / group / env / project / invite / GitHub / pubkey registry). Bearer
  a GoTrue session JWT minted by `42ctl auth login --github` (device flow), saved as
  `session-<profile>.tok`.

So the RBAC verb groups fail without `auth login --github`, and the vault verbs fail without the
plain `auth login`. `adapters/creds.rs` owns the first, `adapters/session.rs` the second.

### The three endpoints in a profile

`profile.rs` resolves one `Endpoint` per profile from `$FT_CONFIG` (default
`~/.config/42ctl/config.json`): `server` = vault42 (secrets), `authority` = grobase-nano (contract,
`/v1/register`), `grobase` = grobase-stack (the email-OTP + escrow routes). Pointing `grobase` at
the authority is a real, already-fixed bug class — the OTP routes 404 there.

### Zero-knowledge sync (`push` / `pull`)

`ops/sync.rs` seals each `*.env*` file under an **opaque** vault path; the real relative paths exist
only inside the encrypted manifest (`core/manifest.rs`), so the server never learns them.
`core/projpath.rs::validate_stored` is the load-bearing Zip-Slip guard — a **pure string check** that
refuses any stored path that could escape the project root; a violating path is an error, never
sanitized. `core/syncstate.rs` (`.42ctl/sync.json`) is the merge base, exactly as git uses the index,
which is what lets `core/merge.rs` distinguish fast-forward from a real divergence and emit
git-style conflict markers. `pull` is dry-run unless `--apply`.

### Large objects (`push` above the ceiling, `vault gc`)

A file larger than `chunk::CHUNK_BYTES` (4 MiB minus 64 KiB) is split, each chunk sealed with
`compose::chunk_envelope` and uploaded to an S3-compatible store (`adapters/blobstore.rs`, signed
by `adapters/sigv4.rs`); the vault receives only the chunk list. Configure with
`config endpoint --blobstore <url> --bucket <name>`; the credential comes from `FT_S3_KEY` /
`FT_S3_SECRET` and is **never** written to the config file. Without a configured store an
oversized file is **refused**, naming the flag — never half-transferred.

A chunk's name is a keyed BLAKE3 digest of its plaintext under a blinded per-identity prefix.
That is what makes the "already in the store, skip it" resume safe: a positional name turns that
skip into silent corruption, since the changed bytes never upload and the read returns the
previous version. `vault gc` walks **every** manifest version, refuses when it found none, and
never collects a chunk inside its grace period. See `DECISIONS.md` D11.

### Accounts (`auth signup` / `passwd` / `me`, `account delete`)

Password-backed accounts on the authority, distinct from the local Ed25519 identity. Passwords
are prompted through `adapters/passphrase.rs` and never echoed; `FT_PASSWORD` is the CI knob and
is deliberately NOT `FT_PASSPHRASE`, which is the keystore secret — one variable serving both
would silently make them the same in every automated run.

`account delete` is the only irreversible verb. It refuses without `--yes`, and the refusal
names both what is lost and the flag, since a refusal an operator cannot act on is one they work
around. The request carries no account id, so there is no way to spell somebody else's; removing
another person is an org membership decision under `org`, where the role check lives.

### Scope keys — the grobase ↔ vault42 bridge

Shared per-environment secrets. The admin runs `vault env-init` (generate the scope keyset at epoch
1, publish its public key to grobase, self-wrap the secret), each member runs `keys enroll --org`,
then `vault sync-keys` wraps the scope secret to every authorized member that has a registered
pubkey. `set-env` seals to the scope **public** key; `get-env` recovers the scope **secret** from
the caller's own wrap (the two-hop unwrap in `cmd/scope_recover.rs`). `rotate-scope` re-seals every
env secret at `epoch+1` and re-wraps only the remaining members, so a removed member loses access by
absence. The scope secret never leaves a `Zeroizing` buffer. The server gates all of it behind
`VAULT42_SCOPE_KEYS_ENABLED`.

## Trip-wires

- **`Cargo.toml` pins `vault42-core`/`vault42-proto` to a REV, not a tag** (currently `91c7c47` on
  `develop`). The sibling `../vault42` checkout moves independently, so a server built from it can
  be ahead of or behind what this crate compiles against. Re-pin deliberately, never incidentally.
  `DECISIONS.md` D1 still says "tag `v0.1.2`" and is stale.
- **`README.md` and `main.rs` both say "P0 — scaffold".** Both are stale: push/pull,
  notes, org/team/group/env/invite RBAC, GitHub device login, escrow/recover, and the whole scope-key
  suite are all implemented.
- **`42ctl-release`, a 9.7 MB binary, is committed.** `.gitignore` covers only `/42ctl-bin`. Don't
  refresh it as part of unrelated work.
- **`cargo install c42` from crates.io is not a channel** and never will be while the git deps stand
  (crates.io forbids them). The cargo channel is `cargo binstall c42` (D9).
- **`.github/workflows/release.yml` is generated by `dist`** and must stay generator-pure — `dist
  plan` aborts on divergence. Never hand-edit it; re-pin its actions with `pinact run` after every
  `dist generate`. The three workflows we own (`publish.yml`, `docker.yml`, `sign-release.yml`) are
  SHA-pinned and gated on the protected `publish` environment.
- **`installers/` is empty** — the shell/PowerShell/npm/Homebrew installers are cargo-dist artifacts
  produced at release time, not files in the tree.
- **`42ctl unseal` is a stub** pending the gRPC unseal surface.
- **CI's push trigger names `develop`, which does not exist here** (branches are `main` plus
  `feat/*`). Pull requests are what actually run CI.

## Conventions binding on every edit

Vendored rules live in `.claude/rules/`; there is no `.claude/AGENTS.md` in this repo.

- **Every source file starts with the 42-school header** — 71 of 74 do (`cmd/auth.rs`, `cmd/db.rs`,
  `cmd/vault.rs` are the exceptions). Generate it with `python3 scripts/ops/gen-42-header.py <file>`.
- **No prose comment inside a function body.** All commentary goes in one `///` doc comment above the
  declaration. The only tolerated in-body comments are the greppable tags `// ponytail:`, `// perf:`,
  `// SAFETY:`. Wanting a mid-body comment is the signal to split the function.
- **Max ~25 lines per function**, `max_width = 100`, no `unwrap()` outside tests. Files get split to
  stay in the norm rather than growing — that is why `team.rs`/`team_members.rs` and the eight
  `scope_*.rs` handlers exist. Follow the same pattern instead of enlarging a file.
- **No globals.** Config and clients are built at the edge and threaded down; `ui.rs` decides styling
  per call rather than caching it.
- **Never roll your own crypto.** All of it comes from `vault42-core`; this crate only orchestrates.
- **Plaintext, passphrases, and key material are radioactive** — `Zeroizing` buffers, never logged,
  never in an error or a trace. `FT_PASSPHRASE` exists for CI; interactive input is never echoed.
- Binary crate, so errors are `anyhow` with a cause chain printed by `ui::report_error`.
- Tests are inline next to the primitive; the crypto/protocol/path-safety paths get a failing test
  first.

### Env knobs

`cli.rs::HOWTO` lists the operator-facing ones. Two more matter here: `FT_GIT_SHA` is stamped by
`build.rs` and is what `42ctl version` reports, and `FT_PASSWORD` (account) is deliberately not
`FT_PASSPHRASE` (keystore) — one variable for both would make them the same secret in CI.

### Git

Conventional-commit subjects, a body explaining *why* when the change is not obvious, and **no
`Co-Authored-By` and no "Generated with" trailer** — the history has none. Pushes, signed tags, and
anything that reaches npm / Docker Hub / the Homebrew tap are irreversible and need an explicit
operator go-ahead with the target re-verified at that moment.

## Doc map

`DECISIONS.md` D0–D10 (architecture + the distribution choices) · `RUNBOOK.md` build, release, credential
rotation, and how to yank a compromised release · `SECURITY.md` the user-facing verification story ·
`docs/RELEASE-DOD.md` the per-channel definition of done · `42ctl --help` carries the full operator
how-to in `cli.rs::HOWTO`.

## Sibling repo

`../vault42` is the server side: the crypto core, the Protobuf spine, and the gRPC edge this CLI
talks to. It has its own `CLAUDE.md`. 42ctl consumes `vault42-core` and `vault42-proto` as **pinned
git dependencies, never copies** (D1), so any breaking change to those public APIs must be landed
there first and then re-pinned here.
