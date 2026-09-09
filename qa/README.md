# vault42 QA battery

A reproducible test and pentest harness that drives **vault42** through **42ctl**, the
way a real operator would. It exists to be red: most of the product it specifies is not
built yet, and the battery is the executable specification for building it.

```sh
./qa/run.sh              # everything
./qa/run.sh s10 s12      # named specs
./qa/run.sh s00          # preflight only — start here on a new machine
QA_SHUFFLE=1 ./qa/run.sh # random spec order — both real defects so far were order-dependent
QA_REPEAT=3 ./qa/run.sh  # run the whole battery three times
```

## Reading the output

Every assertion declares what kind of failure it is. That distinction is the whole
design, and it is what makes a permanently-red suite still worth running.

| Marker | Meaning | What to do |
|---|---|---|
| `ok` | passing, and must stay so | nothing |
| `not ok … # REGRESSION` | a green assertion is failing | **fix it** — the run exits non-zero |
| `not ok … # EXPECTED-RED` | the feature is not built yet | build it; this is the spec |
| `ok … # SPEC-NOW-MET` | an unbuilt feature just landed | promote `assert_spec` to `assert_green` |
| `# SKIP` | a prerequisite is missing | fix the prerequisite; nothing ran |

In CI, invoke `./qa/run.sh` directly. Piping it into `tail` or `head` replaces its exit
status with the pipe's, which silently turns a failing run into a passing one.

**The run fails only on regressions.** Expected reds never fail it, so the battery is
usable as a merge gate today while the authority is still being written.

`qa/results/latest.jsonl` carries the same information one JSON object per assertion,
for consumption by tooling rather than eyes.

## Prerequisites

Neither repo has a host `cargo`, so everything runs in Docker.

- `docker`, and the image `public.ecr.aws/docker/library/rust:1.96-slim-bookworm`
- a `vault42` checkout beside this one, or `VAULT42_DIR` pointing at one
- `curl` on the host, for the authority route probes
- `git submodule update --init` for the Inception fixture

`s00` checks all of these and skips loudly rather than emitting a misleading red.

## Which vault42 gets tested

The battery builds from a **pinned commit**, not from the sibling working tree, because
that tree is edited continuously by the session writing the authority. Building from a
moving tree makes every result depend on what someone else happened to have saved, and
a red that cannot be reproduced tomorrow is not worth reporting.

```sh
QA_VAULT42_REV=<sha> ./qa/run.sh    # pin to a specific commit (default in lib/server.sh)
QA_VAULT42_REV= ./qa/run.sh         # build from the live sibling checkout instead
```

The pin is a detached read-only clone under `qa/.cache/vault42`. The sibling checkout is
never modified. Because of the pin, a green result is always attributable to one commit.

## Two servers, two ports

The battery starts both halves of the system, because they are separate binaries:

| Binary | Port | Serves |
|---|---|---|
| `vault42-server` | 8443 | the gRPC data plane that `push`, `pull` and the vault verbs use |
| `vault42-authority` | 8444 | the REST control plane: accounts, sessions, contracts, the org model |

Host ports are chosen from the free range at start, then **adopted from the running
container** whenever anything needs them. That indirection is not decoration: assertions
run through command substitution, so a helper that exports a port does so in a subshell
and the value is lost. Anything needing the base URL calls `qa_base` at the moment of
use rather than reading a variable.

## Fixtures

Deterministic by construction: every fixture is written from fixed literal content, so
two runs produce byte-identical trees and a red is reproducible on another machine.
`s00` proves this by generating everything twice and comparing hashes.

| Fixture | Shape |
|---|---|
| `flat` | one `.env` at the root |
| `nested` | five env files at four depths — the path-preservation case |
| `secrets_tree` | `srcs/.env` plus Docker secrets under `secrets/` |
| `orchestrator` | a root repo with four submodules, one parked under `vendor/` |
| `decoys` | `.env.example`, `.bak`, `.swp`, vendored and build-tree copies |
| `hostile_values` | BOM, CRLF, `=` in values, quotes, 4 KiB value, no trailing newline |
| `hostile_names` | spaces, hashes, accents, a 180-character filename |
| `empty` | no secret material at all |
| `inception` | the real `Univers42/Inception` repo, pinned as a submodule |

Generated fixtures live in `qa/fixtures/gen/` and are git-ignored. The Inception
submodule is only ever written at paths its own `.gitignore` excludes, and `s30`
asserts it is left git-clean.

## The specs

| Spec | Covers |
|---|---|
| `s00-preflight` | prerequisites, fixture reproducibility, credential-leak guard |
| `s10-roundtrip-paths` | byte-exact push/pull, path and mode preservation, zero knowledge |
| `s11-raw-file-coverage` | which files the scanner takes, and which it silently drops |
| `s12-hostile-input-fuzz` | hostile values and filenames survive byte-for-byte |
| `s13-tamper-mutation` | mutated database, keystore and sync base all fail closed |
| `s14-large-payloads` | the payload ceiling, oversize refusal, chunked sizes, archives |
| `s15-submodule-orchestrator` | a root repo with submodules, restored into each submodule |
| `s16-multi-user-transfer` | handing a credential to a teammate, and who cannot read it |
| `s20-accounts-and-passwords` | accounts, passwords, irreversible account deletion, end to end |
| `s21-org-team-invites` | the standalone org model, route by route |
| `s22-pop-injectivity` | signature-message injectivity |
| `s23-org-lifecycle-scenario` | sign up, found an org, invite, join, and every refusal |
| `s24-scope-bridge-authz` | projects, environments, pubkeys, grants, and who may do what |
| `s25-scope-lifecycle` | an environment secret shared between two people, and rotation |
| `s26-scope-key-attacks` | scope namespacing, multi-tenancy isolation, missing defences |
| `s27-grant-scoping` | environment-scoped versus project-wide grants |
| `s28-variable-precedence` | variables at three scopes, which wins, and who may write |
| `s29-offboarding` | removal, its cascade, the rotation that closes it, account deletion |
| `s31-operability` | backup and restore, fail-closed startup, deployment, doc honesty |
| `s32-second-factor-escrow` | one-time codes, the keystore escrow round trip, the device flow |
| `s24-scope-bridge-authz` | projects, environments, pubkeys, grants, and who may do what |
| `s25-scope-lifecycle` | an environment secret shared between two people, and rotation |
| `s26-scope-key-attacks` | scope namespacing, multi-tenancy isolation, missing defences |
| `s30-inception-live` | a real compose project, filled, pushed, wiped and restored |
| `s33-large-objects` | chunked transfer, resume, tamper, collection, restoring a version |
| `s34-team-project-access` | the whole team story: accounts, org, team, grants, the tree |

## Writing a spec

```sh
source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s99-my-spec"
qa_require_docker_stack
trap qa_server_cleanup EXIT

assert_green "something that works today"  -- some_command
assert_spec  "something not built yet"     -- some_command
spec_end
```

Two rules learned the hard way while building this:

1. **Make a red fail for the right reason.** A helper that is not exported into a
   `bash -c` body fails with "command not found", which is indistinguishable from a
   real red. Read the failure text, do not just count the reds.
2. **Never let an assertion pass vacuously.** `account delete` exits non-zero because
   the subcommand does not exist, not because it demanded confirmation. Assert on the
   reason, not only on the exit status.

## Known defects

- ~~`vault rotate-scope` stranded the environment.~~ Fixed. It re-wraps the full
  authorized set and always keeps the rotator, and the server now refuses a rotation
  carrying no wrap for the caller. `s25` and `s29` hold the cover.
- **Removal is authorization, not erasure.** A scope key already wrapped to a departing
  member lives in vault42 under their own key and the authority cannot reach it, so the
  removal response returns `rotate_required`. In practice the client resolves the
  environment through the authority first, so a removed member loses access immediately
  through the CLI; the residual window is reachable only by a client that already holds
  the scope id and epoch.

## An absence assertion must prove its haystack

Every zero-knowledge assertion in this battery was **vacuous for its whole life**. They
searched a database dump that never contained the data, because the dump copied only the
`.db` while the rows live in the write-ahead log beside it. They passed, and would have
passed identically had the server written plaintext to disk.

`assert_zero_knowledge` in `lib/harness.sh` is the fix, and the control lives *inside* the
assertion rather than beside it, because a control you have to remember calling is one the
next caller omits. It fails three ways: the dump is empty, the dump lacks a marker the
server legitimately stores, or the secret is present. Use it for every absence check.

The same lesson bit twice more in one day. A fresh-start check grepped container logs for
`listening|bound` and matched build output, because the crate is full of words like
`unbounded`; it reported a start that never happened. Assert on the property — ask the
service whether it serves — not on a hopeful string in a log.

## The backup drill

`s31` restores a backup into a fresh server and reads a known secret back through the real
client. It exists before any backup script does, on purpose: an untested backup is a guess,
and writing the verification first means whatever gets built is measured against a restore
that is known to work.

It also pins the trap that script will fall into. With write-ahead logging on, the database
file is one empty page and every byte of data sits in the `-wal` beside it. Copying just the
`.db` — the obvious thing to back up — captures an empty vault, and the restore *succeeds*,
so the loss surfaces only when the data is needed. The drill asserts both halves: a full copy
reproduces the secret, and a database-file-only copy does not.

## Known ceilings

Measured, not assumed, and re-measured on every run:

- **A single file must stay under about 4 MiB on the DIRECT path.** Neither side raises
  tonic's default `max_decoding_message_size`. Above that the file is split and the chunks
  go to an object store, so 4, 16 and 64 MiB now round-trip in `s14` and `s33` carries the
  resume, tamper and collection cases. With no object store configured the file is still
  refused, and `s14` asserts the refusal names both the size and the ceiling, because a
  refusal an operator cannot act on is one they will work around.
- **Deduplication reaches across one identity's own objects and versions, not between two
  members of one environment.** That needs identical plaintext to seal to identical
  ciphertext; `s33` states the properties the primitive must have rather than only that it
  is missing.
- **A submodule under `vendor/` is skipped**, because `vendor` is in the dependency
  skip list. Push still reports success.
- **`--project` overrides configured scan patterns** with the defaults, so a project
  set up to store arbitrary files stores nothing when the flag is used. Two assertions
  passed for their whole life this way, reporting success over a transfer that never
  happened; specs that push now require evidence of a transfer, not merely a zero exit.
- **A spec must start every server it talks to.** `s22` asserted a live authority route
  without starting the authority and passed only because some earlier spec had left one
  running; under a shuffled order it ran first and reported a defect that did not exist.

## Credential handling

The operator's `.env` lives in the parent directory, outside both repositories. `s00`
asserts it stays untracked and that its values appear nowhere in either tree. Values
are compared, never printed.
