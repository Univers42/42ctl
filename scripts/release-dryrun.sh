#!/bin/sh
# *************************************************************************** #
#                                                                            #
#   release-dryrun.sh                                                        #
#                                                                            #
#   42ctl release preflight — validate that a release WOULD succeed without  #
#   publishing, tagging, pushing, or deploying anything. Read-only except    #
#   for a release build inside the Docker toolchain (no network publish).    #
#                                                                            #
#   Usage: sh scripts/release-dryrun.sh vX.Y.Z                               #
#   Exits non-zero if any hard check fails.                                  #
#                                                                            #
# *************************************************************************** #
set -eu

REPO_ROOT=$(unset CDPATH; cd -- "$(dirname -- "$0")/.." && pwd)
TOOLCHAIN_IMAGE="public.ecr.aws/docker/library/rust:1.96-slim-bookworm"
HARD_FAILS=0

ready() {
	printf 'READY:   %s\n' "$1"
}

missing() {
	printf 'MISSING: %s\n' "$1"
	HARD_FAILS=$((HARD_FAILS + 1))
}

warn() {
	printf 'WARN:    %s\n' "$1" >&2
}

# Argument must be a well-formed vX.Y.Z semver tag (no pre-release suffixes here).
check_semver_tag() {
	tag="$1"
	if printf '%s' "$tag" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$'; then
		ready "tag '$tag' is a well-formed semver tag"
	else
		missing "tag '$tag' is not a vMAJOR.MINOR.PATCH semver tag"
	fi
}

# The working tree must be clean so the build matches what will be tagged.
check_clean_tree() {
	if [ -z "$(git -C "$REPO_ROOT" status --porcelain)" ]; then
		ready "git working tree is clean"
	else
		missing "git working tree has uncommitted changes"
	fi
}

# HEAD should already carry the tag we are dry-running (warn, not hard fail:
# the operator runs this BEFORE pushing the tag too).
check_on_tag() {
	tag="$1"
	if git -C "$REPO_ROOT" describe --exact-match --tags HEAD 2>/dev/null | grep -qx "$tag"; then
		ready "HEAD is at tag '$tag'"
	else
		warn "HEAD is not at tag '$tag' (expected before the signed tag is pushed)"
	fi
}

# cargo-dist config presence is a soft check — P4 wires it; warn if absent.
check_dist_config() {
	if grep -q 'metadata.dist' "$REPO_ROOT/Cargo.toml"; then
		ready "cargo-dist config present in Cargo.toml"
	else
		warn "no [workspace.metadata.dist] in Cargo.toml (wired in P4)"
	fi
}

# Report whether $1 (a secret name) is NAMED in any of the remaining workflow
# path arguments; warn (not hard-fail) since publish is wired in P4-P6.
report_secret() {
	name="$1"
	shift
	if grep -lq "$name" "$@" 2>/dev/null; then
		hits=$(grep -l "$name" "$@" | tr '\n' ' ')
		ready "secret $name is referenced in $hits"
	else
		warn "secret $name is not referenced in any workflow yet"
	fi
}

# Collect the workflow files that exist, then probe each required secret,
# passing the files as positional args so each stays quoted.
check_publish_secrets() {
	wf_dir="$REPO_ROOT/.github/workflows"
	set --
	[ -f "$wf_dir/release.yml" ] && set -- "$@" "$wf_dir/release.yml"
	[ -f "$wf_dir/docker.yml" ] && set -- "$@" "$wf_dir/docker.yml"
	if [ "$#" -eq 0 ]; then
		warn "no release.yml/docker.yml workflow found (publish wired in P4-P6)"
		return
	fi
	for secret in NPM_TOKEN DOCKER_LOGIN DOCKER_PAT; do
		report_secret "$secret" "$@"
	done
}

# Build the release binary in the Docker toolchain and confirm its version
# string matches the tag (minus the leading 'v'); also print the SHA-256.
check_build_and_version() {
	tag="$1"
	want=$(printf '%s' "$tag" | sed 's/^v//')
	if ! command -v docker >/dev/null 2>&1; then
		missing "docker is required for the toolchain build"
		return
	fi
	out=$(run_toolchain_build) || { missing "release build failed in toolchain"; return; }
	got=$(printf '%s' "$out" | sed -n 's/^42ctl \([0-9.]*\) .*/\1/p')
	if [ "$got" = "$want" ]; then
		ready "42ctl version '$got' matches tag '$tag'"
	else
		missing "42ctl version '$got' does not match tag '$tag' (want '$want')"
	fi
	bin_sha=$(printf '%s' "$out" | sed -n 's/^SHA256 //p')
	[ -n "$bin_sha" ] && printf 'READY:   binary SHA-256 %s\n' "$bin_sha"
}

# One-shot containerized release build; emits '42ctl <ver> (<sha>)' then
# 'SHA256 <hash>'. No network publish, no host toolchain.
run_toolchain_build() {
	docker run --rm -v "$REPO_ROOT":/build -w /build "$TOOLCHAIN_IMAGE" \
		sh -euc 'cargo build --release >&2 && ./target/release/42ctl version && \
			printf "SHA256 %s\n" "$(sha256sum target/release/42ctl | cut -d" " -f1)"'
}

main() {
	if [ "$#" -ne 1 ]; then
		printf 'usage: %s vX.Y.Z\n' "$0" >&2
		exit 2
	fi
	tag="$1"
	printf '42ctl release dry-run for %s (no publish, no push)\n' "$tag"
	check_semver_tag "$tag"
	check_clean_tree
	check_on_tag "$tag"
	check_dist_config
	check_publish_secrets
	check_build_and_version "$tag"
	if [ "$HARD_FAILS" -ne 0 ]; then
		printf '\n%d hard check(s) failed — NOT ready to release.\n' "$HARD_FAILS" >&2
		exit 1
	fi
	printf '\nAll hard checks passed — release would proceed.\n'
}

main "$@"
