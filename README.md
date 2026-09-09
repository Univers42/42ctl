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
```

That downloads the static binary for your machine from the latest GitHub Release, verifies its
SHA-256 against the release's `SHA256SUMS`, installs it to `~/.local/bin` (or `/usr/local/bin`
as root) and puts it on your `PATH`. Options: `--version vX.Y.Z`, `--bin-dir DIR`,
`--no-modify-path`, `--uninstall` (pass them after `sh -s --`).

Already installed?

```sh
42ctl update --check     # is there a newer release?
42ctl update             # download → verify SHA-256 → atomic swap
```

Other channels: the Docker image (`deploy/Dockerfile.dist`, `FROM scratch`, non-root) and the raw
assets on the [Releases](https://github.com/Univers42/42ctl/releases) page — see `SECURITY.md` to
verify one by hand.

## First five minutes

```sh
42ctl help                 # the guided walkthrough; `42ctl help <topic>` for one subject
42ctl keys init            # your local identity (X25519 + Ed25519), sealed by a passphrase
42ctl auth login --tenant <tenant> --email you@example.com     # email OTP → login contract
42ctl push --project <name>                                    # seal + upload your *.env tree
42ctl pull --project <name> --apply                            # bring it back, byte-exact
```

Topics: `quickstart` · `sync` · `keys` · `teams` · `scopes` · `notes` · `config` · `security` ·
`update`. Every verb also answers `--help`.

## Command surface

```
42ctl auth     login | logout | whoami | status                  # platform auth → a contract for your key
42ctl keys     init | export-pub | enroll | escrow | recover      # local zero-knowledge identity
42ctl vault    get | set | ls | rm | rotate | share | audit | import | export   (alias: secrets)
               env-init | sync-keys | scope-status | set-env | get-env | rotate-scope   # shared env keys
42ctl push | pull                                                # the project's *.env tree, path-aware
42ctl note     add | get | ls | rm                               # encrypted project notes
42ctl db       get | ls                                          # RBAC-checked encrypted records
42ctl org | team | group | env | project | invite                # RBAC over grobase
42ctl config   profile | endpoint | show                         # multi-profile (orgs / environments)
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
