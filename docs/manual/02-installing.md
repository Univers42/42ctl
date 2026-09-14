# 2. Installing and updating

## 2.1 Requirements

- **Linux** on **x86_64** or **aarch64** (any distribution: the binary is statically linked against
  musl and needs no system library).
- **curl** or **wget** to download it, and **sha256sum** (or `shasum`) to verify it. BusyBox versions
  of all three are enough.
- No root access. The default installation is per user.

On macOS or Windows, run 42ctl from its Docker image (§2.6) or inside WSL.

## 2.2 Installing

```sh
curl -fsSL https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | sh
```

On a machine without curl:

```sh
wget -qO- https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | sh
```

The installer, in order:

1. works out the release asset for this machine (`42ctl-x86_64-unknown-linux-musl` or
   `42ctl-aarch64-unknown-linux-musl`);
2. resolves the latest release, unless told otherwise;
3. downloads the binary and the release's `SHA256SUMS`, and **refuses to install** unless the
   checksum matches — a failed verification leaves nothing on disk;
4. installs it to **`~/.local/bin/42ctl`**, or `/usr/local/bin/42ctl` when run as root;
5. adds that directory to `PATH` in each of `~/.bashrc`, `~/.zshrc` and `~/.profile` that exists
   (and `~/.config/fish/conf.d/42ctl.fish` for fish), unless it is on `PATH` already;
6. prints `42ctl version`.

Open a new shell, or run the `export PATH=…` line it prints, and check:

```sh
$ 42ctl version
42ctl 0.1.10 (…)
target    x86_64-unknown-linux-musl
```

### Installer options

Pass options after `sh -s --`. Every option has an environment variable twin, for when the script
is piped:

| Option | Variable | Effect |
|---|---|---|
| `--version vX.Y.Z` | `FT_VERSION` | install that release instead of the latest |
| `--bin-dir DIR` | `FT_BIN_DIR` | install into `DIR` |
| `--no-modify-path` | `FT_NO_MODIFY_PATH=1` | leave shell start-up files alone |
| `--uninstall` | — | remove the binary from the install directory |

```sh
curl -fsSL https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | sh -s -- --version v0.1.9
curl -fsSL https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | FT_BIN_DIR=/opt/bin sh
```

The installer is exercised after every release on a clean Debian (curl, x86_64 and aarch64) and a
clean Alpine (BusyBox wget only), by the `install-check` workflow.

## 2.3 Updating

```sh
$ 42ctl update --check            # is a newer release published? changes nothing
installed 0.1.9
available 0.1.10
update available: run 42ctl update to install it
$ 42ctl update                    # download, verify SHA-256, replace this binary in place
$ 42ctl update --version 0.1.9    # install exactly that release, newer or older
```

`update` replaces the file it is running from, atomically: it writes the new binary beside the old
one, verifies it, and renames it over. It therefore needs write permission on that directory — a
per-user installation always has it; a root installation needs `sudo 42ctl update`.

**How often there is something to update to.** Every change merged into 42ctl's `main` branch that
passes its checks becomes a patch release within minutes, and `releases/latest` — the only thing
the installer and `update` read — follows it. There is no separate "stable" channel.

## 2.4 Verifying a release by hand

The installer and `update` already refuse a binary whose SHA-256 does not match `SHA256SUMS`. A
checksum proves the bytes are the ones published; to prove *who published them*, verify the
signature. Every release asset carries a keyless [cosign](https://docs.sigstore.dev/cosign/installation)
signature made by 42ctl's release workflow:

```sh
V=v0.1.10; A=42ctl-x86_64-unknown-linux-musl
for f in "$A" "$A.sig" "$A.pem" SHA256SUMS; do
  curl -fsSLO "https://github.com/Univers42/42ctl/releases/download/$V/$f"
done
sha256sum -c --ignore-missing SHA256SUMS
cosign verify-blob \
  --certificate-identity-regexp '^https://github.com/Univers42/42ctl/\.github/workflows/sign-release\.yml@refs/.*$' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --signature "$A.sig" --certificate "$A.pem" "$A"
```

Both must print success: `…: OK`, then `Verified OK`. `SECURITY.md` in the repository also covers
SLSA provenance (`gh attestation verify`) and the Docker image.

## 2.5 Uninstalling

```sh
curl -fsSL https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | sh -s -- --uninstall
```

This removes the binary only. **Your identity, sessions and configuration in `~/.config/42ctl` are
left in place**, because deleting a keystore destroys every secret sealed to it. Remove that
directory yourself only when you are certain nothing you need is sealed to that identity.

## 2.6 The Docker image

```sh
docker run --rm docker.io/dlesieur/42ctl:latest version
```

The image is `FROM scratch`, runs as a non-root user and is signed like the release assets. To act on
a project from a container, mount the project and your configuration:

```sh
docker run --rm -it \
  -v "$PWD":/work -w /work \
  -v "$HOME/.config/42ctl":/config \
  -e FT_CONFIG=/config/config.json -e FT_KEYSTORE=/config/keystore.v42 \
  --user "$(id -u):$(id -g)" \
  docker.io/dlesieur/42ctl:latest env pull --org acme --project api --env prod
```

`--user` matters: a restore **writes files**, and without it they would belong to the image's user.
`scripts/42ctl-oneshot.sh` in the repository wraps exactly this.
