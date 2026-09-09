#!/bin/sh
# *************************************************************************** #
#                                                                            #
#   release.sh                                                               #
#                                                                            #
#   Cut a 42ctl release: bump the crate version, commit, tag, push. The tag  #
#   push triggers .github/workflows/release.yml, which builds the static     #
#   Linux binaries, writes SHA256SUMS, attests provenance and publishes the  #
#   GitHub Release that install.sh and `42ctl update` consume.               #
#                                                                            #
#   Usage:  sh scripts/release.sh patch|minor|major        # bump from Cargo.toml
#           sh scripts/release.sh vX.Y.Z                   # explicit version  #
#           sh scripts/release.sh vX.Y.Z --dry-run         # show, change nothing
#                                                                            #
#   Auth:   GH_PAT (env, or ./.env) — a token with repo + workflow scope.    #
#           The push goes over HTTPS with the token read from the            #
#           environment by a one-shot credential helper: it is never on a    #
#           command line, in shell history, or in the git config.            #
#                                                                            #
# *************************************************************************** #
set -eu

REPO="Univers42/42ctl"
REPO_ROOT=$(unset CDPATH; cd -- "$(dirname -- "$0")/.." && pwd)
DRY_RUN=""

say() { printf '%s\n' "$*"; }
ok() { printf '  \033[32m✓\033[0m %s\n' "$*"; }
die() { printf '  \033[31m✗\033[0m %s\n' "$*" >&2; exit 1; }

# GH_PAT from the environment, else from the git-ignored ./.env.
load_token() {
	if [ -z "${GH_PAT:-}" ] && [ -f "${REPO_ROOT}/.env" ]; then
		GH_PAT=$(sed -n 's/^GH_PAT=//p' "${REPO_ROOT}/.env" | head -1 | tr -d '"'"'")
	fi
	[ -n "${GH_PAT:-}" ] || die "GH_PAT is not set (export it, or put GH_PAT=… in ./.env)"
	export GH_PAT
}

current_version() {
	sed -n 's/^version = "\(.*\)"/\1/p' "${REPO_ROOT}/Cargo.toml" | head -1
}

# Turn patch|minor|major|vX.Y.Z|X.Y.Z into the target version X.Y.Z.
resolve_version() {
	cur=$(current_version)
	major=${cur%%.*}
	rest=${cur#*.}
	minor=${rest%%.*}
	patch=${rest#*.}
	case "$1" in
	patch) NEW="${major}.${minor}.$((patch + 1))" ;;
	minor) NEW="${major}.$((minor + 1)).0" ;;
	major) NEW="$((major + 1)).0.0" ;;
	v[0-9]*.[0-9]*.[0-9]*) NEW="${1#v}" ;;
	[0-9]*.[0-9]*.[0-9]*) NEW="$1" ;;
	*) die "want patch|minor|major or vX.Y.Z (got '$1')" ;;
	esac
	printf '%s' "$NEW" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || die "'$NEW' is not X.Y.Z"
	TAG="v${NEW}"
}

# Refuse to release from a dirty tree, off main, behind origin, or onto an existing tag.
preflight() {
	cd "$REPO_ROOT"
	[ -z "$(git status --porcelain)" ] || die "working tree is not clean — commit or stash first"
	branch=$(git rev-parse --abbrev-ref HEAD)
	[ "$branch" = "main" ] || die "release from main (you are on '$branch')"
	git fetch -q origin main --tags
	[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || die "main is not in sync with origin/main — pull or push first"
	! git rev-parse -q --verify "refs/tags/${TAG}" >/dev/null || die "tag ${TAG} already exists"
	ok "clean main at $(git rev-parse --short HEAD), ${TAG} is free"
}

# Rewrite the first `version = "…"` in Cargo.toml and the c42 block in Cargo.lock.
bump_files() {
	awk -v v="$NEW" '!done && /^version = "/ { sub(/"[^"]*"/, "\"" v "\""); done = 1 } { print }' \
		Cargo.toml >Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml
	awk -v v="$NEW" '
		/^\[\[package\]\]/ { in_c42 = 0 }
		/^name = "c42"$/ { in_c42 = 1 }
		in_c42 && /^version = "/ { sub(/"[^"]*"/, "\"" v "\"") }
		{ print }' Cargo.lock >Cargo.lock.tmp && mv Cargo.lock.tmp Cargo.lock
	[ "$(current_version)" = "$NEW" ] || die "Cargo.toml bump failed"
	ok "Cargo.toml + Cargo.lock → ${NEW}"
}

# One commit, one annotated tag (signed when a signing key is configured).
commit_and_tag() {
	git add Cargo.toml Cargo.lock
	git commit -q -m "release: ${TAG}"
	if git config --get user.signingkey >/dev/null 2>&1; then
		git tag -s "$TAG" -m "42ctl ${TAG}"
	else
		git tag -a "$TAG" -m "42ctl ${TAG}"
	fi
	ok "committed and tagged ${TAG}"
}

# Push main + the tag over HTTPS; the helper reads GH_PAT from the environment.
push() {
	# shellcheck disable=SC2016  # $GH_PAT must expand inside git's helper, not here
	helper='!f() { printf "username=x-access-token\npassword=%s\n" "$GH_PAT"; }; f'
	git -c credential.helper= -c "credential.helper=${helper}" \
		push -q "https://github.com/${REPO}.git" main "$TAG"
	ok "pushed main + ${TAG}"
	say ""
	say "  watch:    https://github.com/${REPO}/actions/workflows/release.yml"
	say "  release:  https://github.com/${REPO}/releases/tag/${TAG}"
	say ""
}

main() {
	[ "$#" -ge 1 ] || die "usage: $0 patch|minor|major|vX.Y.Z [--dry-run]"
	[ "${2:-}" = "--dry-run" ] && DRY_RUN="yes"
	load_token
	resolve_version "$1"
	say ""
	say "  42ctl release  $(current_version) → ${NEW}"
	preflight
	if [ -n "$DRY_RUN" ]; then
		say "  dry-run: would bump, commit 'release: ${TAG}', tag, and push"
		exit 0
	fi
	bump_files
	commit_and_tag
	push
}

main "$@"
