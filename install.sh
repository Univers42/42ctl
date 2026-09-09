#!/bin/sh
# *************************************************************************** #
#                                                                            #
#   install.sh                                                               #
#                                                                            #
#   42ctl installer for Linux — every distro, x86_64 + aarch64, no root.     #
#   Downloads the static binary for this machine from the GitHub Release,    #
#   verifies its SHA-256 against the release's SHA256SUMS, and only then      #
#   places it. A failed verification leaves nothing on disk.                 #
#                                                                            #
#   curl -fsSL https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | sh
#   curl -fsSL …/install.sh | sh -s -- --version v0.2.0 --bin-dir /opt/bin   #
#                                                                            #
#   Options    --version vX.Y.Z   pin a release (default: latest)            #
#              --bin-dir DIR      install directory                          #
#                                 (default: ~/.local/bin; /usr/local/bin as root)
#              --no-modify-path   do not touch shell rc files                #
#              --uninstall        remove the binary from --bin-dir           #
#   Env        FT_VERSION, FT_BIN_DIR, FT_NO_MODIFY_PATH mirror the flags.   #
#                                                                            #
# *************************************************************************** #
set -eu

REPO="Univers42/42ctl"
BASE="https://github.com/${REPO}"
VERSION="${FT_VERSION:-}"
BIN_DIR="${FT_BIN_DIR:-}"
MODIFY_PATH="${FT_NO_MODIFY_PATH:+no}"
UNINSTALL=""
TMP=""

say() { printf '%s\n' "$*"; }
ok() { printf '  \033[32m✓\033[0m %s\n' "$*"; }
die() { printf '  \033[31m✗\033[0m %s\n' "$*" >&2; exit 1; }

cleanup() {
	if [ -n "$TMP" ]; then
		rm -rf "$TMP"
	fi
}

# Parse flags; every flag has an env-var twin so `curl | sh` users can configure too.
parse_args() {
	while [ "$#" -gt 0 ]; do
		case "$1" in
		--version) VERSION="$2"; shift ;;
		--version=*) VERSION="${1#*=}" ;;
		--bin-dir) BIN_DIR="$2"; shift ;;
		--bin-dir=*) BIN_DIR="${1#*=}" ;;
		--no-modify-path) MODIFY_PATH="no" ;;
		--uninstall) UNINSTALL="yes" ;;
		-h | --help) sed -n '2,20p' "$0" 2>/dev/null || say "see the header of install.sh"; exit 0 ;;
		*) die "unknown option: $1" ;;
		esac
		shift
	done
}

# Only Linux is supported; map uname's arch to the release target triple.
detect_target() {
	os=$(uname -s)
	[ "$os" = "Linux" ] || die "42ctl installer supports Linux only (got $os) — use the Docker image elsewhere"
	case "$(uname -m)" in
	x86_64 | amd64) arch="x86_64" ;;
	aarch64 | arm64) arch="aarch64" ;;
	*) die "unsupported architecture: $(uname -m) (x86_64 and aarch64 are published)" ;;
	esac
	TARGET="${arch}-unknown-linux-musl"
}

# Prefer curl, fall back to wget; both as `fetch URL OUT`.
pick_fetcher() {
	if command -v curl >/dev/null 2>&1; then
		fetch() { curl -fsSL --proto '=https' --tlsv1.2 -o "$2" "$1"; }
		head_url() { curl -fsSLI --proto '=https' --tlsv1.2 -o /dev/null -w '%{url_effective}' "$1"; }
	elif command -v wget >/dev/null 2>&1; then
		fetch() { wget -q -O "$2" "$1"; }
		head_url() { wget -q --max-redirect=5 -O /dev/null -S "$1" 2>&1 | sed -n 's/^ *Location: *//p' | tail -1; }
	else
		die "need curl or wget to download"
	fi
}

# Resolve the tag to install: the pinned one, or where releases/latest redirects.
resolve_version() {
	if [ -n "$VERSION" ]; then
		case "$VERSION" in v*) ;; *) VERSION="v$VERSION" ;; esac
		return
	fi
	location=$(head_url "${BASE}/releases/latest") || die "cannot reach github.com"
	VERSION="${location##*/}"
	case "$VERSION" in
	v[0-9]*) ;;
	*) die "no published release found at ${BASE}/releases" ;;
	esac
}

# Default install dir: /usr/local/bin for root, ~/.local/bin otherwise.
resolve_bin_dir() {
	[ -n "$BIN_DIR" ] && return
	if [ "$(id -u)" = "0" ]; then
		BIN_DIR="/usr/local/bin"
	else
		BIN_DIR="${HOME}/.local/bin"
	fi
}

# Download the asset + SHA256SUMS into $TMP and verify; die (and clean up) on mismatch.
download_and_verify() {
	asset="42ctl-${TARGET}"
	TMP=$(mktemp -d) || die "mktemp failed"
	say "  downloading ${asset} ${VERSION} …"
	fetch "${BASE}/releases/download/${VERSION}/${asset}" "${TMP}/${asset}" ||
		die "download failed — does ${VERSION} publish ${asset}? see ${BASE}/releases"
	fetch "${BASE}/releases/download/${VERSION}/SHA256SUMS" "${TMP}/SHA256SUMS" ||
		die "download failed: SHA256SUMS"
	expected=$(awk -v n="$asset" '$2 == n || $2 == "*" n { print $1 }' "${TMP}/SHA256SUMS")
	[ -n "$expected" ] || die "SHA256SUMS has no entry for ${asset}"
	actual=$(sha256_of "${TMP}/${asset}")
	[ "$actual" = "$expected" ] || die "checksum mismatch for ${asset} — refusing to install"
	ok "SHA-256 verified"
}

# `sha256sum` (coreutils/busybox) or `shasum -a 256` (perl), whichever exists.
sha256_of() {
	if command -v sha256sum >/dev/null 2>&1; then
		sha256sum "$1" | cut -d' ' -f1
	elif command -v shasum >/dev/null 2>&1; then
		shasum -a 256 "$1" | cut -d' ' -f1
	else
		die "need sha256sum or shasum to verify the download"
	fi
}

# Place the verified binary; use sudo only when the directory is not writable.
install_binary() {
	mkdir -p "$BIN_DIR" 2>/dev/null || true
	if [ -w "$BIN_DIR" ]; then
		install -m 755 "${TMP}/42ctl-${TARGET}" "${BIN_DIR}/42ctl"
	elif command -v sudo >/dev/null 2>&1; then
		say "  ${BIN_DIR} is not writable — using sudo"
		sudo install -m 755 "${TMP}/42ctl-${TARGET}" "${BIN_DIR}/42ctl"
	else
		die "${BIN_DIR} is not writable and sudo is unavailable — pass --bin-dir"
	fi
	ok "installed ${BIN_DIR}/42ctl"
}

# Append the bin dir to PATH in the user's shell rc, unless it is already on PATH.
ensure_path() {
	case ":${PATH}:" in *":${BIN_DIR}:"*) return ;; esac
	[ "$MODIFY_PATH" = "no" ] && { say "  add ${BIN_DIR} to your PATH"; return; }
	line="export PATH=\"${BIN_DIR}:\$PATH\""
	for rc in "${HOME}/.bashrc" "${HOME}/.zshrc" "${HOME}/.profile"; do
		[ -f "$rc" ] || continue
		grep -qF "$line" "$rc" 2>/dev/null && continue
		printf '\n# added by the 42ctl installer\n%s\n' "$line" >>"$rc"
	done
	if [ -d "${HOME}/.config/fish" ]; then
		mkdir -p "${HOME}/.config/fish/conf.d"
		printf 'fish_add_path %s\n' "$BIN_DIR" >"${HOME}/.config/fish/conf.d/42ctl.fish"
	fi
	say "  PATH updated in your shell rc — open a new shell, or run:  ${line}"
}

uninstall() {
	resolve_bin_dir
	[ -f "${BIN_DIR}/42ctl" ] || die "nothing installed at ${BIN_DIR}/42ctl"
	rm -f "${BIN_DIR}/42ctl" 2>/dev/null || sudo rm -f "${BIN_DIR}/42ctl"
	ok "removed ${BIN_DIR}/42ctl"
	say "  your config and keystore in ~/.config/42ctl were left in place"
}

main() {
	parse_args "$@"
	trap cleanup EXIT
	[ -n "$UNINSTALL" ] && { uninstall; exit 0; }
	say ""
	say "  42ctl installer"
	detect_target
	pick_fetcher
	resolve_version
	resolve_bin_dir
	download_and_verify
	install_binary
	ensure_path
	say ""
	"${BIN_DIR}/42ctl" version
	say ""
	say "  next:  42ctl help quickstart"
	say ""
}

main "$@"
