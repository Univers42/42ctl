#!/usr/bin/env bash
# qa/lib/server.sh — stand up a real vault42-server in Docker and drive 42ctl at it.
#
# Docker-first because neither repo has a host cargo. The defaults here are the ones
# that actually hold on a fresh machine: the toolchain image is the public Rust image
# both repos already build with, and VAULT42_DIR is the sibling checkout. The existing
# 42ctl gates default to an image and a path that do not exist, which is why they
# hard-fail instead of running.

set -uo pipefail

: "${QA_IMG:=public.ecr.aws/docker/library/rust:1.96-slim-bookworm}"
: "${VAULT42_DIR:=$C42_ROOT/../vault42}"
# Build from a PINNED commit, not from the sibling working tree. The vault42 session
# edits that tree continuously, so building from it makes every result depend on what
# someone else happened to have saved — a red that cannot be reproduced tomorrow. The
# pin is a detached read-only clone, so their checkout is never touched.
: "${QA_VAULT42_REV:=7bd2bd8}"
: "${QA_NET:=qa42-net}"
: "${QA_SRV:=qa42-srv}"
: "${QA_PORT:=8443}"

# Pick a free host port rather than hardcoding one. Docker can leak a docker-proxy
# process that keeps holding a published port after its container is gone, and a fixed
# port turns that daemon-level leak into a permanent, confusing red.
# Whether something is already listening on a port.
#
# `ss` when it is available, and a connect attempt through bash's own /dev/tcp when it is
# not. Without the fallback a missing `ss` makes every probe report "free", so every suite
# picks the same first port and the collision surfaces as docker's "port is already
# allocated" — which reads as an infrastructure fault rather than as a port conflict, and
# costs whoever hits it the time to work out it was neither.
qa_port_taken() {
	if command -v ss >/dev/null 2>&1; then
		ss -ltn 2>/dev/null | grep -q ":$1 "
		return
	fi
	(exec 3<>/dev/tcp/127.0.0.1/"$1") >/dev/null 2>&1
}

qa_pick_host_port() {
	local p
	for p in $(seq "${1:-18443}" $(( ${1:-18443} + 80 ))); do
		qa_port_taken "$p" && continue
		printf '%s' "$p"
		return 0
	done
	printf '18443'
}
: "${QA_HOST_PORT:=$(qa_pick_host_port)}"
: "${QA_AUTH_SRV:=qa42-auth}"
: "${QA_AUTH_PORT:=8444}"
: "${QA_AUTH_HOST_PORT:=$(qa_pick_host_port $((QA_HOST_PORT + 1)))}"
# The authority is a SEPARATE binary on its own port, not a route table bolted onto
# vault42-server. Probing the gRPC port for REST would keep every route red for the
# wrong reason, which is the failure mode this battery exists to avoid.
: "${QA_AUTHORITY_BASE:=http://127.0.0.1:$QA_AUTH_HOST_PORT}"
# One-time codes are delivered to a directory instead of a mailbox, so a battery can read
# them without sending real mail. This is the authority's own file transport, not a
# test-only route, so the path exercised is the one production uses.
: "${QA_OUTBOX:=$QA_RESULTS/outbox}"

QA_V42_VOLS="-v vault42-cargo-registry:/usr/local/cargo/registry -v vault42-cargo-git:/usr/local/cargo/git"
QA_C42_VOLS="-v 42ctl-cargo-registry:/usr/local/cargo/registry -v 42ctl-cargo-git:/usr/local/cargo/git"

# Resolve the pinned vault42 revision into a private clone and point VAULT42_DIR at it.
# Set QA_VAULT42_REV= (empty) to build from the live sibling checkout instead.
qa_pin_vault42() {
	[ -n "${QA_VAULT42_REV:-}" ] || return 0
	local cache="$QA_ROOT/.cache/vault42" src="$C42_ROOT/../vault42"
	[ -d "$src/.git" ] || return 1
	if [ ! -d "$cache/.git" ]; then
		mkdir -p "$(dirname "$cache")"
		git clone --quiet --no-checkout "$src" "$cache" >/dev/null 2>&1 || return 1
	fi
	git -C "$cache" fetch --quiet "$src" >/dev/null 2>&1 || true
	git -C "$cache" checkout --quiet --detach "$QA_VAULT42_REV" >/dev/null 2>&1 || return 1
	VAULT42_DIR="$cache"
	export VAULT42_DIR
}

# Verify the prerequisites this battery cannot run without, skipping the spec with a
# precise reason rather than emitting a misleading red.
qa_require_docker_stack() {
	qa_require_cmd docker
	qa_pin_vault42 || spec_skip "cannot resolve pinned vault42 rev ${QA_VAULT42_REV:-}"
	docker image inspect "$QA_IMG" >/dev/null 2>&1 ||
		spec_skip "toolchain image absent: $QA_IMG (docker pull $QA_IMG)"
	[ -d "$VAULT42_DIR" ] ||
		spec_skip "vault42 checkout not found at $VAULT42_DIR (set VAULT42_DIR)"
}

# Whether a server started by this battery is already up and listening. Lets the
# runner start one server for the whole battery while each spec stays runnable alone.
qa_server_is_up() {
	docker inspect -f '{{.State.Running}}' "$QA_SRV" 2>/dev/null | grep -q true || return 1
	docker logs "$QA_SRV" 2>&1 | grep -q 'listening'
}

# Tear down unless the runner asked to keep the server for the next spec.
qa_server_cleanup() {
	[ "${QA_KEEP_SERVER:-0}" = "1" ] && return 0
	qa_server_down
}

# Tear down anything this battery left behind. Safe to call when nothing is running.
qa_server_down() {
	docker rm -fv "$QA_SRV" >/dev/null 2>&1 || true
	# `docker rm -f` returns before the daemon has released the published port, so a
	# start that follows immediately fails with "port is already allocated". Wait for
	# the container to actually be gone.
	local i
	for i in $(seq 1 20); do
		docker inspect "$QA_SRV" >/dev/null 2>&1 || break
		sleep 0.5
	done
	docker network rm "$QA_NET" >/dev/null 2>&1 || true
}

# Build the 42ctl debug binary inside the toolchain image, reusing the cargo caches.
# Echoes nothing on success; the caller asserts on the exit status.
qa_build_client() {
	docker run --rm -v "$C42_ROOT":/work -w /work $QA_C42_VOLS "$QA_IMG" \
		cargo build --quiet
}

# Start vault42-server on its own network with an embedded SQLite store, then wait for
# it to announce that it is listening. Scope-key RPCs are enabled so the shared-env
# specs can reach them.
qa_server_up() {
	qa_server_is_up && return 0
	qa_server_down
	# An existing network is fine, and now the normal case: the authority container
	# stays attached to it, so the teardown before this cannot remove it. Treating
	# "already exists" as fatal made every spec after s13 report the server as down.
	docker network create "$QA_NET" >/dev/null 2>&1 || true
	local attempt
	for attempt in 1 2 3; do
		_qa_server_start && break
		[ "$attempt" = 3 ] && return 1
		sleep 2
	done
	local i
	for i in $(seq 1 600); do
		docker logs "$QA_SRV" 2>&1 | grep -q 'listening' && return 0
		docker inspect "$QA_SRV" >/dev/null 2>&1 || return 1
		sleep 1
	done
	return 1
}

# One attempt at starting the server container.
#
# A host port that cannot be bound is retried on a FRESH port rather than reported. The
# port is chosen once when this library is sourced, or adopted from a container that was
# already running, and either can go stale: a removed container can leave its docker-proxy
# holding the port, and every later start then fails with "port is already allocated" — a
# message about networking, on every spec, for the rest of the run. That is how a whole
# battery turns red without a single assertion being wrong.
_qa_server_start() {
	docker rm -fv "$QA_SRV" >/dev/null 2>&1 || true
	local out
	out="$(_qa_server_run 2>&1)" && return 0
	case "$out" in
	*"already allocated"* | *"address already in use"*)
		QA_HOST_PORT="$(qa_pick_host_port $((QA_HOST_PORT + 1)))"
		export QA_HOST_PORT
		printf '# host port was taken, retrying on %s\n' "$QA_HOST_PORT" >&2
		docker rm -fv "$QA_SRV" >/dev/null 2>&1 || true
		_qa_server_run >/dev/null 2>&1
		;;
	*)
		printf '%s\n' "$out" >&2
		return 1
		;;
	esac
}

# The docker invocation itself, so a failed start can be retried on another port.
_qa_server_run() {
	docker run -d --name "$QA_SRV" --network "$QA_NET" \
		-v "$VAULT42_DIR":/work -w /work $QA_V42_VOLS \
		-e VAULT42_HOST=0.0.0.0 -e VAULT42_PORT="$QA_PORT" \
		-e VAULT42_DB=/tmp/qa42.db -e VAULT42_STORE=sqlite \
		-e VAULT42_SCOPE_KEYS_ENABLED=1 -e RUST_LOG=info \
		-p "${QA_HOST_PORT:-18443}:$QA_PORT" \
		"$QA_IMG" sh -c 'cargo run --quiet --bin vault42-server' >/dev/null
}

# Run a 42ctl command as a named actor.
#
# Each actor keeps its config, keystore and tokens in a HOST directory mounted at
# /state, so the identity survives across calls. That is what lets one spec model
# several people — alice, bob, mallory — and what lets "machine A pushes, machine B
# pulls" work at all: a fresh `keys init` per call would mint a new identity and the
# pull could never decrypt.
#
# The 42ctl arguments are passed as ONE string and expanded by the inner shell, so a
# caller that needs a filename with a space must quote it inside that string.
#
# Usage: qa_actor <name> <workdir> "<42ctl args>"
qa_actor() {
	local who="$1" workdir="$2" args="$3"
	local state="$QA_RESULTS/actors/$who"
	mkdir -p "$state"
	# Run as the invoking user, not root. Docker's default root would write the pulled
	# tree back root-owned, and a restored 0600 file would then be unreadable to the
	# host — surfacing as a phantom "bytes differ" that is really permission denied.
	docker run --rm --network "$QA_NET" --user "$(id -u):$(id -g)" \
		-v "$C42_ROOT":/work -v "$workdir":/project -v "$state":/state -w /project \
		-e HOME=/state \
		-e FT_PASSPHRASE="${QA_PASS_OVERRIDE:-qa-pass-$who}" \
		-e FT_CONFIG=/state/config.json \
		-e FT_KEYSTORE=/state/keystore.v42 \
		-e FT_CONTRACT=/state/contract.tok \
		-e FT_SESSION=/state/session.tok \
		-e FT_S3_KEY="$QA_S3_KEY" \
		-e FT_S3_SECRET="$QA_S3_SECRET" \
		-e FT_PASSWORD="${QA_ACCOUNT_PASSWORD:-}" \
		"$QA_IMG" sh -c "
			set -e
			B=/work/target/debug/42ctl
			\$B config endpoint --server http://$QA_SRV:$QA_PORT --authority http://$QA_AUTH_SRV:$QA_AUTH_PORT >/dev/null
			[ -f /state/keystore.v42 ] || \$B keys init >/dev/null 2>&1
			exec \$B $args
		"
}

# Hand an actor a control-plane session token.
#
# The scope verbs read a bearer from the per-profile session file, which the GitHub
# device flow normally writes. There is no browser in a battery, so the token is
# obtained from the authority's own password login and written straight to that file.
# The file format is the raw token, so this is setup, not a bypass: the CLI still
# authenticates every request with it exactly as it would in real use.
qa_actor_token() {
	local who="$1" token="$2"
	mkdir -p "$QA_RESULTS/actors/$who"
	printf '%s' "$token" >"$QA_RESULTS/actors/$who/session.tok"
}

# Sign an actor up at the authority and give it a session. Echoes the account id.
qa_actor_account() {
	local who="$1" email="$2" pw="$3" id
	id="$(qa_signup "$email" "$pw")"
	qa_actor_token "$who" "$(qa_login "$email" "$pw")"
	printf '%s' "$id"
}

# Give an actor a fresh identity, discarding any previous one.
qa_actor_reset() {
	rm -rf "$QA_RESULTS/actors/$1"
	mkdir -p "$QA_RESULTS/actors/$1"
}

# Start the standalone authority: accounts, sessions and contract issuance over its own
# embedded SQLite. Database and signing key go to /tmp so no volume is needed.
qa_authority_up() {
	mkdir -p "$QA_OUTBOX"
	qa_adopt_running_ports
	qa_authority_is_up && return 0
	docker rm -fv "$QA_AUTH_SRV" >/dev/null 2>&1 || true
	local i
	for i in $(seq 1 20); do docker inspect "$QA_AUTH_SRV" >/dev/null 2>&1 || break; sleep 0.5; done
	docker network create "$QA_NET" >/dev/null 2>&1 || true
	docker run -d --name "$QA_AUTH_SRV" --network "$QA_NET" \
		-v "$VAULT42_DIR":/work -w /work $QA_V42_VOLS \
		-p "$QA_AUTH_HOST_PORT:$QA_AUTH_PORT" \
		-e VAULT42_AUTHORITY_HOST=0.0.0.0 -e VAULT42_AUTHORITY_PORT="$QA_AUTH_PORT" \
		-e VAULT42_AUTHORITY_DB=/tmp/qa-authority.db \
		-e VAULT42_AUTHORITY_KEY=/tmp/qa-contract.key \
		-v "$QA_OUTBOX":/outbox \
		-e MAIL_TRANSPORT=file -e MAIL_OUTBOX=/outbox \
		-e MAIL_FROM=qa-sender@archicode.codes \
		-e VAULT42_OTP_PROOF_SECRET=qa-otp-proof-secret-do-not-use-in-production \
		-e RUST_LOG=info \
		"$QA_IMG" sh -c 'cargo run --quiet --bin vault42-authority' >/dev/null || return 1
	for i in $(seq 1 600); do
		qa_authority_is_up && return 0
		docker inspect "$QA_AUTH_SRV" >/dev/null 2>&1 || return 1
		sleep 1
	done
	return 1
}

# Adopt the port an already-running container actually published. The free-port scan
# runs per shell, so a second shell would otherwise compute a different port and
# conclude nothing is listening — then start a duplicate container.
qa_adopt_running_ports() {
	local p
	p=$(docker port "$QA_AUTH_SRV" "$QA_AUTH_PORT" 2>/dev/null | head -1 | sed 's/.*://')
	if [ -n "$p" ]; then
		QA_AUTH_HOST_PORT="$p"
		QA_AUTHORITY_BASE="http://127.0.0.1:$p"
		export QA_AUTH_HOST_PORT QA_AUTHORITY_BASE
	fi
	p=$(docker port "$QA_SRV" "$QA_PORT" 2>/dev/null | head -1 | sed 's/.*://')
	[ -n "$p" ] && { QA_HOST_PORT="$p"; export QA_HOST_PORT; }
	return 0
}

# Whether the authority answers its health route.
qa_authority_is_up() {
	qa_adopt_running_ports
	docker inspect -f '{{.State.Running}}' "$QA_AUTH_SRV" 2>/dev/null | grep -q true || return 1
	curl -sS -m 3 -o /dev/null "$QA_AUTHORITY_BASE/healthz" 2>/dev/null
}

qa_authority_down() {
	docker rm -fv "$QA_AUTH_SRV" >/dev/null 2>&1 || true
}

# Copy everything the server persisted, so a spec can prove against the REAL stored bytes
# that no plaintext and no real path reached it.
#
# The write-ahead log is not optional here and leaving it out made every zero-knowledge
# assertion in this battery vacuous for its whole life. With WAL on, the database file
# holds the schema and the rows live in the -wal beside it, so grepping the .db alone
# searched a haystack that never contained the data. Those assertions passed, and would
# have passed identically had the server stored plaintext. Both files, concatenated.
qa_dump_server_db() {
	local out="$1" tmp
	tmp="$(mktemp -d)"
	docker cp "$QA_SRV:/tmp/qa42.db" "$tmp/db" >/dev/null 2>&1
	docker cp "$QA_SRV:/tmp/qa42.db-wal" "$tmp/wal" >/dev/null 2>&1
	cat "$tmp/db" "$tmp/wal" >"$out" 2>/dev/null
	rm -rf "$tmp"
	[ -s "$out" ]
}

# Reclaim anything an earlier run left root-owned. Runs are non-root now, but a tree
# written by an older root-mode run would otherwise block fixture regeneration with a
# permission error that looks exactly like a product failure.
qa_reclaim_workspace() {
	find "$QA_ROOT/fixtures/gen" "$QA_RESULTS" -maxdepth 0 -writable >/dev/null 2>&1 && return 0
	docker run --rm -v "$QA_ROOT":/qa "$QA_IMG" \
		chown -R "$(id -u):$(id -g)" /qa/fixtures/gen /qa/results >/dev/null 2>&1 || true
}

# The authority's base URL, resolved from the RUNNING container every time it is asked.
#
# It cannot be a plain variable: assertions run through command substitution, so any
# export a helper performs happens in a subshell and is lost. Anything that needs the
# base must therefore ask for it at the moment of use.
qa_base() {
	local p
	p=$(docker port "${QA_AUTH_SRV:-qa42-auth}" "${QA_AUTH_PORT:-8444}" 2>/dev/null | head -1 | sed 's/.*://')
	if [ -n "$p" ]; then printf 'http://127.0.0.1:%s' "$p"; else printf '%s' "${QA_AUTHORITY_BASE:-http://127.0.0.1:18443}"; fi
}

# ── authenticated REST helpers ───────────────────────────────────────────────
# These make a scenario read like the story it is testing rather than like curl.
# Each prints "<http-code><TAB><body>" so an assertion can check either half.

# qa_api <METHOD> <PATH> [TOKEN] [JSON-BODY]
qa_api() {
	local method="$1" path="$2" token="${3:-}" body="${4:-}"
	local out code
	out=$(curl -sS -m 15 -w $'\n%{http_code}' -X "$method" \
		-H 'content-type: application/json' \
		${token:+-H "authorization: Bearer $token"} \
		${body:+-d "$body"} "$(qa_base)$path" 2>/dev/null)
	code="${out##*$'\n'}"
	printf '%s\t%s' "$code" "${out%$'\n'*}"
}

# Echo the JSON string value of a top-level field. Deliberately dependency-free.
qa_json() {
	sed -n "s/.*\"$2\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p" <<<"$1" | head -1
}

# qa_signup <email> <password> -> account_id on stdout
qa_signup() {
	local r; r=$(qa_api POST /v1/auth/signup "" "{\"email\":\"$1\",\"password\":\"$2\"}")
	qa_json "${r#*$'\t'}" account_id
}

# qa_login <email> <password> -> bearer token on stdout
qa_login() {
	local r; r=$(qa_api POST /v1/auth/login "" "{\"email\":\"$1\",\"password\":\"$2\"}")
	qa_json "${r#*$'\t'}" token
}

# The HTTP status of a call, for assertions that care only about the verdict.
qa_code() { local r; r=$(qa_api "$@"); printf '%s' "${r%%$'\t'*}"; }

# Probe one REST route on the authority endpoint from the host.
#
# The authority does not exist yet, so every probe is red today. Each one going green
# is a route landed, which makes this the implementer's burndown list. A route counts
# as served when it answers HTTP with a JSON content type and a status that is neither
# 404 (route absent) nor a connection failure (nothing listening at all).
qa_probe_route() {
	local method="$1" path="$2" body="${3:-}" base
	base="$(qa_base)"
	local out
	out="$(curl -sS -m 10 -o /dev/null -w '%{http_code} %{content_type}' \
		-X "$method" -H 'content-type: application/json' \
		${body:+-d "$body"} "$base$path" 2>/dev/null)" || return 1
	case "$out" in
	404*) return 1 ;;
	000*) return 1 ;;
	*json*) return 0 ;;
	*) return 1 ;;
	esac
}

# Make the probe usable from the `bash -c` bodies that assertions run in. Without this
# a compound assertion fails with "command not found" — a red for the wrong reason,
# which is indistinguishable from a real red in a battery meant to be trusted.
# Export EVERY helper an assertion body might call, plus the variables they read.
#
# This has bitten three times now. An assertion runs its command inside `bash -c`, so a
# helper that is only a shell function is "command not found" there. That surfaces two
# ways, and the second is the dangerous one: a plain call becomes a false RED, while a
# negated call — `! qa_actor ...` — becomes a false GREEN, because the missing command
# also exits non-zero. Exporting them centrally is the fix; adding them one at a time
# as each bites is not.
export -f qa_probe_route qa_base qa_adopt_running_ports qa_port_taken qa_pick_host_port
export -f qa_actor qa_actor_reset qa_server_is_up qa_authority_is_up qa_dump_server_db
export -f qa_api qa_json qa_signup qa_login qa_code qa_actor_token qa_actor_account
export QA_HOST_PORT QA_AUTH_HOST_PORT QA_AUTHORITY_BASE QA_IMG C42_ROOT
export QA_NET QA_SRV QA_PORT QA_AUTH_SRV QA_AUTH_PORT QA_RESULTS QA_C42_VOLS QA_V42_VOLS

# Start a throwaway vault42-server with EXTRA env and report whether it stayed up.
# Echoes "up" or "refused". Used to test fail-closed startup: a server that boots with a
# broken security configuration is the failure this exists to catch.
#
# Usage: qa_try_start "<NAME=VALUE> <NAME=VALUE> ..."
qa_try_start() {
	local extra="$1" name=qa42-try envs=""
	docker rm -fv "$name" >/dev/null 2>&1
	local kv
	for kv in $extra; do envs="$envs -e $kv"; done
	docker run -d --name "$name" -v "$VAULT42_DIR":/work -w /work $QA_V42_VOLS \
		-e VAULT42_HOST=0.0.0.0 -e VAULT42_PORT=8443 -e VAULT42_DB=/tmp/try.db \
		-e VAULT42_STORE=sqlite -e RUST_LOG=info $envs \
		"$QA_IMG" sh -c 'cargo run --quiet --bin vault42-server' >/dev/null 2>&1
	local i
	for i in $(seq 1 120); do
		docker logs "$name" 2>&1 | grep -q listening && { docker rm -fv "$name" >/dev/null 2>&1; printf 'up'; return 0; }
		docker inspect -f '{{.State.Running}}' "$name" 2>/dev/null | grep -q true || break
		sleep 1
	done
	docker rm -fv "$name" >/dev/null 2>&1
	printf 'refused'
}
export -f qa_try_start

# Wait for a NEW one-time code for an address and echo it.
#
# Delivery is spawned rather than awaited, so reading "the newest message" straight after a
# request returns the PREVIOUS code — which is already dead, because a new request replaces
# the old one rather than adding to it. Pass the message count taken BEFORE the request.
qa_wait_for_code() {
	local address="$1" before="$2" safe pat i n
	safe=$(printf '%s' "$address" | sed 's/[^A-Za-z0-9]/_/g')
	pat="$QA_OUTBOX/$safe-*.eml"
	for i in $(seq 1 100); do
		n=$(ls -1 $pat 2>/dev/null | wc -l)
		if [ "$n" -gt "$before" ]; then
			sed -n '/^$/,$p' "$(ls -1t $pat 2>/dev/null | head -1)" |
				grep -oE '[0-9]{6}' | head -1
			return 0
		fi
		sleep 0.3
	done
	return 1
}

# How many messages the outbox already holds for an address.
qa_code_count() {
	local safe; safe=$(printf '%s' "$1" | sed 's/[^A-Za-z0-9]/_/g')
	ls -1 "$QA_OUTBOX/$safe-"*.eml 2>/dev/null | wc -l
}
export -f qa_wait_for_code qa_code_count
export QA_OUTBOX QA_PASS_OVERRIDE

# Run a 42ctl command that prompts for a one-time code, feeding the code from the outbox.
#
# The CLI requests the code and then blocks on stdin in the same process, so the code
# cannot be fetched beforehand: the CLI's own request would replace whatever was fetched.
# Instead stdin is a feeder that watches the outbox and emits the code once it appears.
#
# Usage: qa_actor_otp <who> <workdir> "<42ctl args>" <email>
qa_actor_otp() {
	local who="$1" workdir="$2" args="$3" email="$4"
	local state="$QA_RESULTS/actors/$who" before
	mkdir -p "$state"
	before=$(qa_code_count "$email")
	docker run --rm -i --network "$QA_NET" --user "$(id -u):$(id -g)" \
		-v "$C42_ROOT":/work -v "$workdir":/project -v "$state":/state -w /project \
		-e HOME=/state -e FT_PASSPHRASE="${QA_PASS_OVERRIDE:-qa-pass-$who}" \
		-e FT_CONFIG=/state/config.json -e FT_KEYSTORE=/state/keystore.v42 \
		-e FT_CONTRACT=/state/contract.tok -e FT_SESSION=/state/session.tok \
		"$QA_IMG" sh -c "
			B=/work/target/debug/42ctl
			\$B config endpoint --server http://$QA_SRV:$QA_PORT --authority http://$QA_AUTH_SRV:$QA_AUTH_PORT >/dev/null
			exec \$B $args
		" < <(qa_wait_for_code "$email" "$before")
}
export -f qa_actor_otp

# Try to start the authority with a GENUINELY fresh state — no database, no signing key —
# and report whether it came up. Echoes "up" or "refused".
#
# This is the state every new deployment begins in, and it is the one neither harness
# covered: gates that seed a key first never reach it.
#
# It probes the health route rather than grepping the log. The first version of this
# matched the container log for "listening|bound" and reported a start that never happened,
# because the crate is full of words like "unbounded" and "boundary" that appear in build
# output — and the authority does not log the word "listening" at all. Asking the service
# whether it serves is the property; reading its log for a hopeful string is not.
qa_try_authority_fresh() {
	local name=qa42-fresh tag port
	tag="$$-$(date +%s)"
	port="$(qa_pick_host_port 18600)"
	docker rm -fv "$name" >/dev/null 2>&1
	docker run -d --name "$name" -v "$VAULT42_DIR":/work -w /work $QA_V42_VOLS \
		-p "$port:8444" \
		-e VAULT42_AUTHORITY_HOST=0.0.0.0 -e VAULT42_AUTHORITY_PORT=8444 \
		-e VAULT42_AUTHORITY_DB="/tmp/fresh-$tag.db" \
		-e VAULT42_AUTHORITY_KEY="/tmp/fresh-$tag.key" \
		-e MAIL_TRANSPORT=file -e MAIL_OUTBOX=/tmp/ob \
		-e MAIL_FROM=fresh@archicode.codes -e VAULT42_OTP_PROOF_SECRET=fresh-secret \
		-e NO_COLOR=1 -e RUST_LOG=info \
		"$QA_IMG" sh -c 'cargo run --quiet --bin vault42-authority' >/dev/null 2>&1
	local i
	for i in $(seq 1 240); do
		if curl -sS -m 2 -o /dev/null "http://127.0.0.1:$port/healthz" 2>/dev/null; then
			docker rm -fv "$name" >/dev/null 2>&1; printf 'up'; return 0
		fi
		docker inspect -f '{{.State.Running}}' "$name" 2>/dev/null | grep -q true || break
		sleep 1
	done
	docker rm -fv "$name" >/dev/null 2>&1
	printf 'refused'
}
export -f qa_try_authority_fresh

# ── object storage for the chunked-object work ───────────────────────────────
: "${QA_S3_SRV:=qa42-s3}"
: "${QA_S3_HOST_PORT:=$(qa_pick_host_port 18700)}"
: "${QA_S3_KEY:=qa42minioadmin}"
: "${QA_S3_SECRET:=qa42minioadmin-secret}"
: "${QA_S3_BUCKET:=qa42chunks}"

# Start a real S3-compatible store. Chunks are meant to live outside the vault, so the
# battery needs somewhere outside the vault to put them — and it has to be a real S3
# implementation, because the thing most likely to be wrong is the request signing.
qa_s3_up() {
	docker inspect -f '{{.State.Running}}' "$QA_S3_SRV" 2>/dev/null | grep -q true && return 0
	docker rm -fv "$QA_S3_SRV" >/dev/null 2>&1
	docker network create "$QA_NET" >/dev/null 2>&1 || true
	docker run -d --name "$QA_S3_SRV" --network "$QA_NET" \
		-p "$QA_S3_HOST_PORT:9000" \
		-e MINIO_ROOT_USER="$QA_S3_KEY" -e MINIO_ROOT_PASSWORD="$QA_S3_SECRET" \
		minio/minio:latest server /data >/dev/null 2>&1 || return 1
	local i
	for i in $(seq 1 60); do
		curl -sS -m 2 -o /dev/null "http://127.0.0.1:$QA_S3_HOST_PORT/minio/health/live" 2>/dev/null && return 0
		sleep 1
	done
	return 1
}

qa_s3_down() { docker rm -fv "$QA_S3_SRV" >/dev/null 2>&1; }

# The endpoint as the CLI container sees it, and as the host sees it.
qa_s3_internal() { printf 'http://%s:9000' "$QA_S3_SRV"; }
qa_s3_external() { printf 'http://127.0.0.1:%s' "$QA_S3_HOST_PORT"; }

# How many objects the bucket holds, counted from the host with the mc client in a
# throwaway container. Used to assert dedup and garbage collection by object count.
qa_s3_count() {
	qa_s3_names | grep -c . | tr -d ' \r\n'
}

# Run one mc command against the battery's bucket, with the alias already set.
#
# Extra docker arguments go in QA_MC_DOCKER_ARGS, which is how the mirror below gets a
# volume mounted. HOME is redirected because mc writes its config there and the container
# runs as the invoking user, who does not own /root.
qa_mc() {
	# shellcheck disable=SC2086 # QA_MC_DOCKER_ARGS is a deliberate argument list
	docker run --rm --network "$QA_NET" --user "$(id -u):$(id -g)" -e HOME=/tmp \
		${QA_MC_DOCKER_ARGS:-} --entrypoint sh minio/mc:latest -c \
		"mc alias set qa http://$QA_S3_SRV:9000 $QA_S3_KEY $QA_S3_SECRET >/dev/null 2>&1 && $*" \
		2>/dev/null
}

# Every object name in the bucket, one per line, sorted.
qa_s3_names() {
	qa_mc "mc ls --recursive qa/$QA_S3_BUCKET" | awk '{print $NF}' | sort
}

# Concatenate every stored object into one file, for the zero-knowledge searches.
#
# An absence check needs a haystack that provably contains something, so this returns
# non-zero when the bucket is empty rather than handing back an empty file that would make
# every "the plaintext is not there" assertion pass while proving nothing.
qa_s3_dump() {
	local out="$1" tmp
	tmp="$(mktemp -d)"
	QA_MC_DOCKER_ARGS="-v $tmp:/out" qa_mc "mc mirror --quiet --overwrite qa/$QA_S3_BUCKET /out" >/dev/null
	find "$tmp" -type f -exec cat {} + >"$out" 2>/dev/null
	rm -rf "$tmp"
	[ -s "$out" ]
}

# Overwrite one stored object with another's bytes — the substitution a hostile or broken
# store performs, and the one a chunk read must refuse rather than reassemble.
qa_s3_substitute() {
	qa_mc "mc cp --quiet qa/$QA_S3_BUCKET/$1 qa/$QA_S3_BUCKET/$2" >/dev/null
}

qa_s3_rm() { qa_mc "mc rm qa/$QA_S3_BUCKET/$1" >/dev/null; }

export -f qa_s3_up qa_s3_down qa_s3_internal qa_s3_external qa_s3_count
export -f qa_mc qa_s3_names qa_s3_dump qa_s3_substitute qa_s3_rm
export QA_S3_SRV QA_S3_HOST_PORT QA_S3_KEY QA_S3_SECRET QA_S3_BUCKET
