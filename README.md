# 42ctl

The umbrella platform CLI for the **42 stack** (grobase + vault42 + future apps) — the `flyctl`
of the stack. One downloadable, cross-platform binary: `42ctl auth login` authenticates against the
platform; then you read encrypted records and secrets (decrypted **client-side**, zero-knowledge)
and manage your vault.

> **The guarantee that overrides everything:** `42ctl` decrypts plaintext locally, so its **supply
> chain is part of the vault threat model**. Every release is signed + provenance-attested, and
> every installer verifies before executing. A tampered binary, through any channel, is detectable
> and refused.

## Command surface

```
42ctl auth     login | logout | whoami | status        # platform auth → a contract for your key
42ctl keys     init | export-pub                        # local zero-knowledge identity (X25519+Ed25519)
42ctl vault    get | set | ls | rm | rotate | share | audit   # (alias: secrets) all plaintext crypto local
42ctl db       get | ls                                 # RBAC-checked encrypted records, decrypted locally
42ctl config   profile | endpoint | show               # multi-profile (orgs / environments)
42ctl version | update | unseal
```

```sh
42ctl version
42ctl config show                       # default profile points at the public duo
42ctl config profile staging            # create/switch profiles (orgs/environments)
```

## Architecture

Hexagonal: `cli` (clap) → `cmd` (thin handlers) → `core` (pure use-cases) → `adapters`
(gRPC client, creds/keyring, config, updater). Talks gRPC/HTTPS to vault42's public edge; never
directly to private grobase. Depends on `vault-crypto` (the audited crypto core) and the `contracts`
Protobuf spine as **versioned dependencies, not copies** (see `DECISIONS.md` D1).

## Status

**P0 — scaffold + CI.** The command tree, profiles/config, and `version` are in; the network/crypto
verbs (`auth`, `keys`, `vault`, `db`, `unseal`) are wired across P1–P3, and the signed multi-channel
distribution (curl|sh, GitHub Releases, npm, Docker, cargo, Homebrew) across P4–P8. See
`DECISIONS.md` (architecture + the §12 choices) and `RUNBOOK.md` (release / credential rotation).

## Distribution (planned, gated)

`cargo-dist`-driven: cross-platform binaries + checksums + shell/PowerShell installers + npm
(`@universe42/42ctl`, with provenance) + Homebrew tap + cargo-binstall + a verify-before-swap
self-updater; a separate `buildx` job for the multi-arch Docker image (Docker Hub, cosign-signed,
SBOM + provenance). All registry publishes are gated and run only on a signed semver tag through a
protected CI environment.

## License

AGPL-3.0-only (see `LICENSE`). SDKs/clients calling a 42 server are not bound by the copyleft.
