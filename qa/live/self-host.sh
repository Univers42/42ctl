#!/usr/bin/env bash
# *************************************************************************** #
#                                                                             #
#   self-host.sh — a vault42 deployment built by its manual, then the         #
#   Inception walkthrough driven through it.                                  #
#                                                                             #
#   vault42's operator's manual is not paraphrased here: its shell blocks are #
#   extracted from the release being deployed and executed as written.        #
#                                                                             #
#     chapter 2 §2.2–2.5   build the images, network, volumes, invitation     #
#                          token, the authority, the server pinned to its key #
#     chapter 2 §2.6       the first account and secret, from 42ctl; every    #
#                          output line the manual shows must be printed       #
#     42ctl chapter 13     qa/live/inception.sh, against that deployment      #
#     chapter 6 §6.2       the Docker backups of both services, after which   #
#                          the deployment still serves what it held           #
#                                                                             #
#   The manual's example release is replaced by the one under test, and its   #
#   git URL by a local clone holding it, so a branch can be tested too.       #
#                                                                             #
#   Needs Docker, git, curl, openssl and what inception.sh needs. It refuses  #
#   to start if a container, network or volume with the manual's names        #
#   exists — it would reuse it and then remove it — and removes only what it  #
#   created (KEEP=1 leaves the deployment running).                           #
#                                                                             #
#     C42_BIN=target/release/42ctl bash qa/live/self-host.sh [WORKDIR]        #
#                                                                             #
#   VAULT42_REF     tag or branch to deploy (default: the newest release)     #
#   VAULT42_SOURCE  where to clone vault42 from (default: GitHub)             #
#                                                                             #
# *************************************************************************** #
set -euo pipefail
unset XDG_CONFIG_HOME

WORK="${1:-$(mktemp -d)}"
: "${C42_BIN:?C42_BIN must name the 42ctl build to test}"
SOURCE="${VAULT42_SOURCE:-https://github.com/Univers42/vault42.git}"
HERE="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$WORK/deploy" "$WORK/backup" "$WORK/bin" "$WORK/admin" "$WORK/manual"
WORK="$(cd "$WORK" && pwd)"
C42_BIN="$(cd "$(dirname "$C42_BIN")" && pwd)/$(basename "$C42_BIN")"
ln -sf "$C42_BIN" "$WORK/bin/42ctl"
OBJECTS=(container:vault42-authority container:vault42-server network:vault42
	volume:vault42-authority-data volume:vault42-server-data)

step() { printf '\n== %s\n' "$*"; }
fail() {
	printf 'FAIL %s\n' "$*" >&2
	exit 1
}

# refuse_existing — stop before touching a deployment this script did not create.
refuse_existing() {
	local entry
	for entry in "${OBJECTS[@]}"; do
		if docker "${entry%%:*}" inspect "${entry#*:}" >/dev/null 2>&1; then
			printf 'refusing: a %s named %s already exists\n' "${entry%%:*}" "${entry#*:}" >&2
			exit 2
		fi
	done
}

# tear_down — print the services' last logs on failure, then remove what the manual created.
tear_down() {
	local status=$?
	if [ "$status" -ne 0 ]; then
		for name in vault42-authority vault42-server; do
			printf '\n-- %s logs\n' "$name"
			docker logs --tail 30 "$name" 2>&1 || true
		done
	fi
	[ "${KEEP:-}" = 1 ] && return
	docker rm -f vault42-authority vault42-server >/dev/null 2>&1 || true
	docker network rm vault42 >/dev/null 2>&1 || true
	docker volume rm vault42-authority-data vault42-server-data >/dev/null 2>&1 || true
}

# fenced <from> <to> <file> [pattern] — the shell blocks between two headings, only those
# matching the pattern when one is given.
fenced() {
	awk -v from="$1" -v to="$2" -v want="${4:-}" '
		index($0, from) == 1 { on = 1; next }
		on && index($0, to) == 1 { exit }
		on && /^```/ { if (fence && block ~ want) printf "%s", block; fence = !fence; block = ""; next }
		on && fence { block = block $0 "\n" }' "$3"
}

# as_admin <command…> — run as the manual's administrator, in a home of their own.
as_admin() {
	(cd "$WORK/admin" && HOME="$WORK/admin" PATH="$WORK/bin:$PATH" NO_COLOR=1 \
		FT_PASSPHRASE=walkthrough-passphrase FT_PASSWORD=Walkthrough-password-long \
		REGISTER_TOKEN="$(cat "$WORK/register-token")" "$@")
}

# checkout — clone vault42, tag the commit under test, and take the two chapters from it.
checkout() {
	local commit chapter
	git clone -q "$SOURCE" "$WORK/vault42"
	REF="${VAULT42_REF:-$(git -C "$WORK/vault42" tag --list 'v*' --sort=-v:refname | head -n 1)}"
	commit="$(git -C "$WORK/vault42" rev-parse --verify -q "origin/$REF^{commit}" ||
		git -C "$WORK/vault42" rev-parse --verify "$REF^{commit}")"
	git -C "$WORK/vault42" tag -f walkthrough "$commit" >/dev/null
	COMMIT="$commit"
	for chapter in 02-deploy-docker 06-operations; do
		git -C "$WORK/vault42" show "walkthrough:docs/manual/$chapter.md" >"$WORK/manual/$chapter.md" ||
			fail "vault42 $REF has no docs/manual/$chapter.md to follow"
	done
	printf 'vault42 %s at %s\n' "$REF" "$COMMIT"
}

# deploy — §2.2 to §2.5, with the manual's release and URL pointed at the checkout.
deploy() {
	local manual="$WORK/manual/02-deploy-docker.md" pinned version
	pinned="$(grep -o 'git checkout v[0-9][0-9.]*' "$manual" | head -n 1 | cut -d' ' -f3)"
	[ -n "$pinned" ] || fail "chapter 2 names no release to check out"
	{
		printf 'set -euo pipefail\ncd %q\n' "$WORK/deploy"
		fenced '## 2.2 ' '## 2.6 ' "$manual" | sed -e "s|https://github.com/Univers42/vault42.git|$WORK/vault42|" \
			-e "s|${pinned//./\\.}|walkthrough|g"
		printf 'printf "%%s" "$REGISTER_TOKEN" >%q\n' "$WORK/register-token"
	} >"$WORK/deploy.sh"
	bash "$WORK/deploy.sh"
	version="$(curl -fsS http://127.0.0.1:8444/version)"
	grep -qF "\"commit\":\"$COMMIT\"" <<<"$version" ||
		fail "the authority reports $version, not the commit it was built from"
	[ "$(docker inspect -f '{{.State.Running}}' vault42-server)" = true ] || fail "the server is not running"
}

# first_use — §2.6: every `$ ` line runs, and every output line the manual shows is printed.
first_use() {
	local manual="$WORK/manual/02-deploy-docker.md" out line
	fenced '## 2.6 ' '## 2.7 ' "$manual" | sed -n 's/^\$ //p' |
		sed 's/\bREGISTER_TOKEN\b/"$REGISTER_TOKEN"/g' >"$WORK/first-use.sh"
	out="$(as_admin bash -euo pipefail "$WORK/first-use.sh")" || fail "§2.6 did not run to the end"
	printf '%s\n' "$out"
	while IFS= read -r line; do
		[ -z "$line" ] || grep -qxF -- "$line" <<<"$out" || fail "§2.6 shows '$line', which was not printed"
	done < <(fenced '## 2.6 ' '## 2.7 ' "$manual" | grep -v '^\$ ')
}

# backups — §6.2's Docker blocks, then proof both snapshots exist and the server still serves.
backups() {
	local day listing
	day="$(date +%F)"
	{
		printf 'set -euo pipefail\ncd %q\n' "$WORK/backup"
		fenced '## 6.2 ' '### Restoring' "$WORK/manual/06-operations.md" docker
	} >"$WORK/backup.sh"
	bash "$WORK/backup.sh"
	docker run --rm -v vault42-authority-data:/data public.ecr.aws/docker/library/debian:bookworm-slim \
		test -s "/data/backup-$day.db" || fail "the authority's snapshot is missing"
	listing="$(tar -tzf "$WORK/backup/vault42-server-$day.tar.gz")"
	grep -q 'vault42\.db$' <<<"$listing" || fail "the server's archive holds no database: $listing"
	for _ in $(seq 30); do
		[ "$(as_admin 42ctl vault get smoke/test 2>/dev/null)" = "it works" ] && return
		sleep 1
	done
	fail "after the server backup, the deployment no longer serves smoke/test"
}

refuse_existing
trap tear_down EXIT
step "vault42's source, and the manual that ships with it"
checkout
step "operator's manual §2.2–2.5: a deployment from nothing"
deploy
step "operator's manual §2.6: the first account and secret"
first_use
step "42ctl manual chapter 13: the Inception walkthrough, against this deployment"
C42_BIN="$C42_BIN" C42_AUTHORITY=http://127.0.0.1:8444 C42_SERVER=http://127.0.0.1:8443 \
	VAULT42_REGISTER_TOKEN="$(cat "$WORK/register-token")" bash "$HERE/inception.sh" "$WORK/inception" ||
	fail "the walkthrough failed against a deployment built by the manual"
step "operator's manual §6.2: backing up both services"
backups
printf '\nok — the deployment the manual built passed the walkthrough and survived its backups\n'
