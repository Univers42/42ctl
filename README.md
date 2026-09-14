# 42ctl

The umbrella platform CLI for the **42 stack** (grobase + vault42) — the `flyctl` of the stack.
One static binary: `42ctl auth login` authenticates against the platform; then you read and
write encrypted secrets and records (sealed and opened **on your machine**, zero-knowledge),
sync your project's `*.env` tree, and manage teams, environments and shared keys.

> **The guarantee that overrides everything:** `42ctl` decrypts plaintext locally, so its **supply
> chain is part of the vault threat model**. Every release is checksummed + provenance-attested, and
> the installer and the self-updater verify before they place a byte. A tampered binary is refused.

## Install (Linux — any distro, x86_64 / aarch64, no root needed)

```sh
curl -fsSL https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | sh
# no curl? (Alpine, minimal images)
wget -qO- https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | sh
```

That downloads the static binary for your machine from the latest GitHub Release, verifies its
SHA-256 against the release's `SHA256SUMS`, installs it to `~/.local/bin` (or `/usr/local/bin`
as root) and puts it on your `PATH`. Options: `--version vX.Y.Z`, `--bin-dir DIR`,
`--no-modify-path`, `--uninstall` (pass them after `sh -s --`).

Already installed?

```sh
42ctl update --check     # is there a newer release?
42ctl update             # download → verify SHA-256 → atomic swap
42ctl update --version 0.1.7   # or pin one, up or down
```

Every merge to `main` that passes CI becomes a patch release on its own (`auto-release.yml`), so
`update` always reaches what was merged. After each release `install-check.yml` installs it from
scratch and updates an older install to it, on Debian with curl (x86_64 and aarch64) and on
Alpine with only BusyBox wget — `scripts/verify/install-e2e.sh` runs the same check by hand.

Other channels: the Docker image (`deploy/Dockerfile.dist`, `FROM scratch`, non-root) and the raw
assets on the [Releases](https://github.com/Univers42/42ctl/releases) page — see `SECURITY.md` to
verify one by hand.

## First five minutes

```sh
42ctl keys init                                        # your local identity, sealed by a passphrase
42ctl auth signup --email you@example.com              # your account (password prompted)
42ctl auth login --password --email you@example.com --tenant <tenant>   # session + contract
42ctl push --project <name>                            # seal + upload your project's env tree
42ctl pull --project <name> --apply                    # bring it back, byte-exact
```

The login needs the session before it can take a contract: `--tenant` on its own is refused
until you have signed in with `--password` (or `--github`).

## Help that ships in the binary

```sh
42ctl help                 # the overview and the topic index
42ctl help kickoff         # the whole product end to end: you, a project, a team, an offboarding
42ctl help commands        # every command with its arguments, generated from the parser
42ctl help <topic>         # sync · large · keys · account · teams · scopes · notes · config · security · update
42ctl <command> --help     # the long form of any one command
```

Every `42ctl …` example in the built-in help is parsed by a test, so a renamed flag cannot
survive in the text. `docs/vault.md` is the long-form manual, checked against a live deployment.

## Command surface

```
42ctl auth     login | signup | passwd | mfa | me | logout | whoami | status
42ctl account  show | delete
42ctl keys     init | export-pub | enroll | escrow | recover
42ctl vault    get | set | ls | rm | gc | rotate | share | audit | import | export     (alias: secrets)
42ctl push | pull                                     # your project's env tree, sealed to you
42ctl note     add | get | ls | rm
42ctl db       get | ls
42ctl org      create | member ls|rm | invite | github connect|link|sync
42ctl team     create | ls | member add|rm | invite | grant
42ctl group    create | member add|rm | invite
42ctl env      create | ls | init | push | pull | files           # a project's environments,
               secret set|get | keys ls|sync|rotate                # and what a team shares in one
42ctl project  create | ls | grant ls|add|rm
42ctl invite   accept | show
42ctl config   profile | endpoint | show
42ctl cloud    apps | status | health | machine … | volume … | secret ls | net …
42ctl version | update | help | unseal
```

## Architecture

Hexagonal: `cli` (clap) → `cmd` (thin handlers) → `core` (pure use-cases) → `ops` (orchestration
over a signed session) → `adapters` (gRPC client, keystore, config, GitHub Releases). Talks
gRPC/HTTPS to vault42's public edge; never directly to private grobase. Depends on `vault42-core`
(the audited crypto) and the `vault42-proto` Protobuf spine as **pinned git dependencies, not
copies** (`DECISIONS.md` D1).

## Releasing

```sh
sh scripts/release.sh patch        # or minor | major | vX.Y.Z  (--dry-run to preview)
```

Bumps `Cargo.toml`/`Cargo.lock`, commits, tags `vX.Y.Z`, and pushes with `GH_PAT` (from the
environment or a git-ignored `.env`, see `.env.example`). The tag runs
`.github/workflows/release.yml`: static musl builds for x86_64 + aarch64 on native runners,
`SHA256SUMS`, SLSA provenance, GitHub Release. See `RUNBOOK.md` and `DECISIONS.md` D11.

## License

AGPL-3.0-only (see `LICENSE`). SDKs/clients calling a 42 server are not bound by the copyleft.
