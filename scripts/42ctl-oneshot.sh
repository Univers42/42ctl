#!/bin/sh
# 42ctl as a one-shot container: run a verb against the project in the current directory and
# exit. Not a service — nothing is left running, and the image holds no state.
#
# Three things this wrapper exists to get right, each of which breaks the naive `docker run`:
#
#   1. UID. `pull-env` WRITES your project files. The image's own user is nonroot(65532), so
#      without --user the restored tree lands owned by a uid you are not, and the next plain
#      `make` cannot read its own secrets. We run as the caller.
#   2. Identity. The keystore and the profile's contract/session tokens live on the host and
#      must be mounted, never copied into the image. The passphrase arrives as an environment
#      variable and is never written anywhere.
#   3. Networking. A local vault42 on 127.0.0.1 is unreachable from a bridge namespace, so the
#      default is host networking; override with V42_NET for an isolated run.
#
# Usage:
#   cd <project> && sh 42ctl-oneshot.sh vault pull-env --org <o> --project <id> --env prod --apply
#
# Knobs: V42_IMAGE, V42_CONFIG_DIR, V42_NET, plus any FT_* the CLI reads.
set -eu

IMAGE=${V42_IMAGE:-42ctl:oneshot}
CONFIG_DIR=${V42_CONFIG_DIR:-$HOME/.config/42ctl}
NET=${V42_NET:-host}

# Fail before docker does, with a sentence that names the fix.
preflight() {
	if ! command -v docker >/dev/null 2>&1; then
		printf '42ctl-oneshot: docker is not on PATH\n' >&2
		exit 1
	fi
	if [ "$#" -eq 0 ]; then
		printf 'usage: 42ctl-oneshot.sh <42ctl verb> [args...]\n' >&2
		printf 'example: 42ctl-oneshot.sh vault pull-env --org acme --project <id> --env prod --apply\n' >&2
		exit 2
	fi
	if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
		printf '42ctl-oneshot: image %s not found — build it with:\n' "$IMAGE" >&2
		printf '  docker build -t %s <path-to-42ctl>\n' "$IMAGE" >&2
		exit 1
	fi
	mkdir -p "$CONFIG_DIR"
}

# Forward an FT_* variable only when it is actually set, so an unset passphrase still prompts
# rather than arriving as an empty string that unlocks nothing. `-e NAME` (no value) passes the
# value from this process's environment, so no secret ever appears in the argument list.
forward() {
	value=""
	eval "value=\${$1:-__unset__}"
	if [ "$value" != "__unset__" ]; then
		FORWARD="$FORWARD -e $1"
	fi
}

main() {
	preflight "$@"
	FORWARD=""
	for name in FT_PASSPHRASE FT_PASSWORD FT_LOGIN_EMAIL FT_PROFILE FT_REGISTER_TOKEN \
		FT_S3_KEY FT_S3_SECRET NO_COLOR; do
		forward "$name"
	done
	# shellcheck disable=SC2086
	exec docker run --rm -i \
		--network "$NET" \
		--user "$(id -u):$(id -g)" \
		-v "$PWD":/work -w /work \
		-v "$CONFIG_DIR":/config \
		-e FT_CONFIG=/config/config.json \
		-e FT_KEYSTORE=/config/keystore.v42 \
		$FORWARD \
		"$IMAGE" "$@"
}

main "$@"
