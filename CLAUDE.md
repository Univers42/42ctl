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

**Do not use a host cargo** (where one exists it is too old: `rust-version = 1.91`) — everything runs
in Docker. The image the scripts use is `public.ecr.aws/docker/library/rust:1.96-slim-bookworm`; a
fresh machine pulls it on first use. Reuse the named cache volumes so rebuilds are not from scratch:

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

**A patch release cuts itself.** `auto-release.yml` fires on a green `ci` run on `main`, bumps the
patch version, commits `release: vX.Y.Z`, tags and pushes — so `releases/latest`, which is all
`install.sh` and `42ctl update` ever read, follows `main`. It pushes with the repository secret
`GH_PAT` and not `GITHUB_TOKEN`, because GitHub suppresses workflow triggers for anything a job's
own token pushes and the tag would then build nothing; it skips its own `release: v` commit so it
does not recurse. A `minor`, a `major` or a pinned version is still hand-cut with
`sh scripts/release.sh minor --dry-run` then `sh scripts/release.sh minor` (`GH_PAT` from the
environment, `./.env`, or the workspace `../.env`). Either way the pushed tag runs `release.yml`,
which refuses a tag that does not match `Cargo.toml`; `sign-release.yml` and `docker.yml` chain on
a green `release.yml` via `workflow_run` — which is why the tag must arrive as a real push event.
`install-check.yml` chains on it too: it runs `scripts/verify/install-e2e.sh` (fresh install into
`~/.local/bin`, pinned older install, `update` in place) on Debian+curl x86_64/aarch64 and on
Alpine with only BusyBox wget, whose `wget` lacked the flag the installer used until this ran.

### The QA battery, and the older verify gates

`./qa/run.sh` is the real end-to-end coverage: 33 specs standing up vault42-server, the authority
and a MinIO chunk store in Docker. Its exit status counts REGRESSIONS ONLY, so it works as a merge
gate while `assert_spec` assertions stay red on purpose. `QA_SHUFFLE=1` randomises the order —
use it, because specs have passed or failed because of what ran before them. `qa/README.md`
has the rules; the one that matters most is that an absence assertion must prove its haystack.

What to know before running it:

- **It runs on rootless Docker** (this workstation): `QA_DOCKER_USER` drops `--user` there, since
  rootless maps the container's root to you and refuses any other uid.
- **One battery per Docker daemon.** Every run starts by tearing down the shared qa42-* containers,
  so `run.sh` holds a flock and a second run exits 3. Running a spec file with `bash` directly
  bypasses the lock — don't, while a battery is going.
- **Never edit a spec, `run.sh` or the source while a run uses them.** bash reads a script as it
  executes, so an edited spec runs half old, half new (assertions duplicate, fragments run as
  commands); and every spec starts with `cargo build`, so a source edit lands mid-battery.
- **A spec that pauses a container must bound every wait behind the pause.** s45 holds pushes
  and rotations open with `docker pause` on the object store; a step that unexpectedly needs the
  store waits as long as the spec waits on it, so the battery hangs instead of going red.
- **It measures which commands and flags RAN.** 42ctl (`src/trace.rs`) writes each parsed command
  path and the NAMES of the flags given (never a value) to `FT_TRACE_COMMANDS`; a full run prints
  "commands exercised" and "flags given" against `42ctl help commands`. `QA_REQUIRE_COVERAGE=1`
  fails the run if a command never ran, `QA_REQUIRE_FLAGS=1` if a flag was never given. A new
  verb therefore needs a spec that runs it. Ran is not checked, though, so a verb that can only
  be refused counts as covered — which is how `org github` (routes the authority never had) and
  `unseal` (no seal state) sat green until they were removed. Drive the success path.
- **Every listing is checked in every output shape** by `qa/lib/listing.sh` (s40 for the cloud
  listings, s43 for the rest), and each of those specs compares its list with `help commands`. A
  new verb taking `--format` needs a row there and a fixture giving it two differing rows.
- **Name specs as separate arguments.** A name that selects nothing is an error (exit 2); it used to
  run the preflight alone and print "No regressions".
- **The server rev matters**: s41 asserts authorization fixes and group grants that go red on older
  vault42 revs, and `QA_VAULT42_REV` defaults to one that has them. `QA_VAULT42_REV=` (empty)
  builds the sibling working tree; each spec prints which source it built.
- s40 (cloud) needs no server: `qa/fixtures/fly/flyctl` stands in for flyctl and records every
  command, which is how "no destructive command ever ran" is asserted. s41 is who-may-do-what
  through the CLI alone; s42 is one user's first day from an empty machine; s44 drives
  `auth login --github` to a session against `qa/fixtures/github/stub.pl`, which every battery
  authority is pointed at.

`scripts/verify/v10-secret-sync.sh` … `v13-github-cli.sh` predate it and still work, but **both
their defaults are wrong on this machine**: they need `RUST_TOOLCHAIN_IMG` (the default image is
absent) and `VAULT42_DIR` (the sibling checkout is `../vault42`), and they **exit 1 rather than
skip**, so a missing prerequisite reads as a failure.

## Architecture

Hexagonal, each layer depending only inward. `DECISIONS.md` D6 names four; `ops/` is the fifth that
grew out of `cmd/` as the verbs got real:

| Layer | Role |
|---|---|
| `cli/` | clap types only — the whole command surface, no logic (`mod.rs` + `env.rs`, `rbac.rs`, `store.rs`, `vault.rs`, `cloud.rs`), plus `legacy.rs`, the argv rewrite for retired spellings |
| `cmd/` | thin handlers: resolve profile → unlock identity → open a session → dispatch |
| `core/` | pure use-cases: project scan, encrypted manifest, path model, merge, materialize |
| `ops/` | `impl Session` verbs — the vault/sync/notes logic over an open session |
| `adapters/` | the I/O edge: keystore, passphrase, gRPC session, authority, grobase REST, envelope codecs |

`main.rs` → `cmd::dispatch`: `version`/`update`/`config` run synchronously; everything else
goes through a fresh multi-thread tokio runtime. `unsafe_code = "forbid"`.

### Two credentials, two planes — the thing that trips people up

They are unrelated and each verb needs the right one:

- **vault42 gRPC** (secrets, sync, notes, scope wraps). Every request carries `x-v42-ts` /
  `x-v42-pub` / `x-v42-sig`, an Ed25519 signature over `"{ts}\n{grpc-method}"`, plus
  `x-v42-contract` when the profile has one. The contract comes from the **authority**
  (`vault42-authority`) at `42ctl auth login --tenant` and is saved as `contract-<profile>.tok`.
- **authority REST** (org / team / group / env / project / invite / GitHub / pubkey registry).
  Bearer the opaque session token minted by `auth login --password --email` or `--github`, saved
  as `session-<profile>.tok`. The routes and the `grobase` names in the code predate the authority,
  which now serves them.

So the RBAC verb groups fail without a SESSION, and the vault verbs fail without a contract.
A session comes from `auth login --password --email <mail>` or from `auth login --github`.
The device flow needs a GitHub app configured on the authority, so on a deployment without one
`--password` is the ONLY door — and until it existed the entire group model was unreachable
there however well it tested locally. `adapters/creds.rs` owns the first, `adapters/session.rs` the second.

### The three endpoints in a profile

`profile.rs` resolves one `Endpoint` per profile from `$FT_CONFIG` (default
`~/.config/42ctl/config.json`): `server` = vault42-server (secrets), `authority` = vault42-authority
(accounts, contract, RBAC, codes, escrow), `grobase` = an optional override for the email-code and
escrow routes. `Endpoint::otp_base()` falls back to the authority when `grobase` is unset or names a
host in `RETIRED_CONTROL_PLANE` (`grobase-stack.fly.dev`, `grobase-nano.fly.dev`), so an old config
keeps working instead of 404ing.

### Zero-knowledge sync (`push` / `pull`)

`ops/sync.rs` seals each `*.env*` file under an **opaque** vault path; the real relative paths exist
only inside the encrypted manifest (`core/manifest.rs`), so the server never learns them.
`core/manifest.rs::VERSION` is the reader's fail-closed gate: a manifest from a newer client is
REFUSED, because serde drops an unknown field silently and a reader that ignores `chunked`
writes the chunk list to disk as the file and reports success. Bump it whenever a field changes
what an entry MEANS, not merely what it carries.

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
never collects a chunk inside its grace period. See `DECISIONS.md` D12.

`pull --at <version>` restores the tree as of a manifest version, fetching each file at the
revision that manifest recorded (`Entry.rev`). Without that, reading an old manifest fetches
today's bytes and reproduces a tree that never existed — old file names, new contents. Preserved
chunks that nothing can fetch are not history, so this is the half that makes collection's
all-versions rule mean something.

### Accounts (`auth signup` / `passwd` / `me`, `account delete`)

Password-backed accounts on the authority, distinct from the local Ed25519 identity. Passwords
are prompted through `adapters/passphrase.rs` and never echoed; `FT_PASSWORD` is the CI knob and
is deliberately NOT `FT_PASSPHRASE`, which is the keystore secret — one variable serving both
would silently make them the same in every automated run.

`account delete` is the only irreversible verb. It refuses without `--yes`, and the refusal names
what is lost and the flag — including that every tenant name the account claimed is RELEASED for
anyone to claim (vault42 `DECISIONS.md` D13; names used to outlive the account). A test in
`cmd/account.rs` pins that wording, so an accurate message is not read as a narrower one. The request carries no account id,
so there is no way to spell somebody else's; removing another person is an org membership decision
under `org`.

### Sharing a whole tree with a team (`env push` / `env pull`)

`push`/`pull` seal to the caller's OWN identity, so a teammate cannot read a personally-pushed
tree at all — that is measured in `s34`, not assumed. `env push` seals every scanned file
to the ENVIRONMENT's scope key instead, with a manifest at a reserved env path holding the real
relative paths and modes; `env pull` recovers the scope secret and restores the tree. Access
follows the grant, and a grant to a TEAM reaches every member of it.

The refusal is cryptographic, not advisory: an unauthorised member holds ciphertext and no
wrap, so they fail to decrypt rather than being told no. `s34` proves the refusal tracks the
grant by granting the refused member and watching the same command return the same tree.

A file over the transport ceiling is chunked to the object store here too, sealed to the
ENVIRONMENT rather than to the pusher so every member can open it. The naming key comes from
the environment's secret, so two members holding the same bytes compute the same name and the
second stores nothing — deduplication between people, with no convergent ciphertext needed.
Each stored chunk carries its author key in a small framing, because a deduplicated set's
chunks can have different authors and the object store hands back bytes alone.

**A restore is always one push's tree.** `env pull` reads each file at the revision its manifest
entry records (`Entry.rev`), and `env push` reads every head once before its first write and
conditions every put on it (`cmd/scope_store.rs`), so an overtaken push is refused rather than
interleaved. Both halves are needed: a push that dies or loses partway leaves newer revisions no
manifest names, and reading the newest restores those. s45 forces both interleavings (a push held
at a paused object store, a push whose store is unreachable); s35's four timed rounds could not.

**The manifest is hostile input on this path**, which it is not on the personal one: anyone who
may write the environment may write the manifest a colleague's machine then acts on. The
traversal guard already covered where files land; the MODE did not, and a manifest asking for
0777 on a private key restored it world-readable with correct bytes, so nothing downstream
would have noticed. `env pull` clamps every restored mode to the owner alone. `s35` is the
spec for that whole surface.

### Scope keys — the grobase ↔ vault42 bridge

Shared per-environment secrets. The admin runs `env init` (generate the scope keyset at epoch
1, publish its public key to grobase, self-wrap the secret), each member runs `keys enroll --org`,
then `env keys sync` wraps the scope secret to every authorized member that has a registered
pubkey. `env secret set` seals to the scope **public** key; `env secret get` recovers the scope **secret** from
the caller's own wrap (the two-hop unwrap in `cmd/scope_recover.rs`). `env keys rotate` re-seals every
env secret at `epoch+1` and re-wraps only the remaining members, so a removed member loses access by
absence. It moves a tree as a tree (`cmd/scope_reseal.rs`): shared files at the manifest's
revisions, chunked files re-chunked under the new key, the manifest rewritten last. Members'
private files cannot be moved — the server accepts a write only from its author — so they stay in
their epoch and `env pull` takes the caller's private manifest from the newest epoch holding one
(DECISIONS D13). The scope secret never leaves a `Zeroizing` buffer. The server gates all of it behind
`VAULT42_SCOPE_KEYS_ENABLED`.

## Trip-wires

- **The command tree was reshaped into nouns and verbs, and the old spellings still work.**
  `vault get-env` is `env secret get`, `org remove-member` is `org member rm`, `project grants` is
  `project grant ls` — eighteen paths in all, listed in `cli/legacy.rs`, which rewrites argv BEFORE
  clap parses it (an alias cannot change a command's depth). Every row is a pure change of path:
  same flags and handler, and the only output that differs is text that used to NAME an old verb
  ("run `env init` first"). Anything that also changes behaviour does not belong there.
  The deprecation note prints only when stderr is a terminal, so a script's output is byte-identical.
  **`qa/specs` deliberately still uses the old spellings** — a green battery is what proves the
  rewrite holds — so do not migrate them until the legacy layer is being removed, and remove the
  two together. Help text, docs and error messages use the new spellings only; the
  `every_example_command_parses` drift test enforces that for the topics.

- **`Cargo.toml` pins `vault42-core`/`vault42-proto` to a REV, not a tag** (`grep rev Cargo.toml`;
  it trails `develop`). The sibling `../vault42` checkout moves independently, so a server built from
  it can be ahead of or behind what this crate compiles against — and `qa/` builds yet another rev
  (`QA_VAULT42_REV` in `qa/lib/server.sh`). Re-pin deliberately, never incidentally.
  `DECISIONS.md` D1 still says "tag `v0.1.2`" and is stale.
- **No built binary is committed** and none should be. `42ctl-release` was, and after the
  endpoints moved it still had `vault42.fly.dev` compiled in — a second source of truth for
  the defaults, contradicting the source beside it, and the copy a person is most likely to
  run without building. Deleted; `.gitignore` covers it. Distribution is `install.sh` and
  `42ctl update` off the GitHub Release (D11), never a file in the tree.
- **There is no crates.io, npm or Homebrew channel** and there will not be while the git deps stand.
  Distribution is `install.sh` and `42ctl update`, both reading the raw GitHub Release assets named
  `42ctl-<target>` (D11). A release is cut by `auto-release.yml` or `scripts/release.sh`; nothing is
  published by hand.
- **There is no `unseal` and no `org github`.** Both were removed because neither could act
  against vault42: it has no seal state, and the authority never served the
  `/v1/orgs/{org}/github/*` routes grobase had. `auth login --github` is different — the authority
  implements it, and it works wherever `GITHUB_CLIENT_ID` is set (production has none). Rebuilding
  GitHub org sync means authority routes and a registered GitHub App first.
- **The vault holds 42ctl's own records** under `__42ctl/` (notes, push manifests, chunk lists).
  `vault ls` hides them unless `--all`, `vault rm` refuses them and `vault export` skips them —
  `vault rm $(vault ls -q)` used to delete a person's notes. `db ls` is the record-level view and
  shows everything.
- **CI's push trigger names `develop`, which does not exist here** (branches are `main` plus
  `feat/*`). Pull requests are what actually run CI.

## Conventions binding on every edit

Vendored rules live in `.claude/rules/`; there is no `.claude/AGENTS.md` in this repo.

- **Every source file starts with the 42-school header** — all but `cmd/auth.rs`, `cmd/db.rs` and
  `cmd/vault.rs` do. Generate it with `python3 scripts/ops/gen-42-header.py <file>`.
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

### The built-in help

`42ctl help` is three pieces. `cmd/help_topics.rs` holds the topic text in a tiny line markup
(`## ` section, `$ ` command with `  # comment`, `! ` warning). `cmd/help_commands.rs` generates
`help commands` by walking the clap tree, so a new verb or flag appears there with no edit.
`cmd/help.rs` renders both and carries the drift tests: every `$ 42ctl …` example must parse
(`every_example_command_parses` — write flags out, never `…`), the overview's command groups must
name every top-level command, and the `--help` footer and the `help` argument doc must name every
topic. `quickstart` is kept as an alias of `kickoff`. `docs/vault.md` is the long-form manual.

### Env knobs

`42ctl help config` lists the operator-facing ones (`CONFIG` in `cmd/help_topics.rs`). Two more matter here: `FT_GIT_SHA` is stamped by
`build.rs` and is what `42ctl version` reports, and `FT_PASSWORD` (account) is deliberately not
`FT_PASSPHRASE` (keystore) — one variable for both would make them the same secret in CI.

### Git

Conventional-commit subjects, a body explaining *why* when the change is not obvious, and **no
`Co-Authored-By` and no "Generated with" trailer** — the history has none. Pushes, signed tags, and
anything that reaches npm / Docker Hub / the Homebrew tap are irreversible and need an explicit
operator go-ahead with the target re-verified at that moment.

## Doc map

`DECISIONS.md` D0–D12 (architecture + the distribution choices) · `RUNBOOK.md` build, release, credential
rotation, and how to yank a compromised release · `SECURITY.md` the user-facing verification story ·
`docs/RELEASE-DOD.md` the per-channel definition of done · `docs/vault.md` the user manual for the
whole CLI surface · `42ctl help kickoff` / `help commands` the in-binary walkthrough and reference.

## Sibling repo

`../vault42` is the server side: the crypto core, the Protobuf spine, and the gRPC edge this CLI
talks to. It has its own `CLAUDE.md`. 42ctl consumes `vault42-core` and `vault42-proto` as **pinned
git dependencies, never copies** (D1), so any breaking change to those public APIs must be landed
there first and then re-pinned here.
