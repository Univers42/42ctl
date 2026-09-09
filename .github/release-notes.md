## Install

```sh
curl -fsSL https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | sh
```

Already installed? `42ctl update` — it downloads the asset for your machine, verifies the
SHA-256 against `SHA256SUMS`, and swaps the binary atomically.

## Assets

| asset | runs on |
|---|---|
| `42ctl-x86_64-unknown-linux-musl` | any x86_64 Linux (static, no libc dependency) |
| `42ctl-aarch64-unknown-linux-musl` | any aarch64 / arm64 Linux (static) |
| `SHA256SUMS` | checksums the installer and `42ctl update` verify against |

## Verify by hand

```sh
sha256sum -c --ignore-missing SHA256SUMS
gh attestation verify 42ctl-x86_64-unknown-linux-musl --repo Univers42/42ctl
```
