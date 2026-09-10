#!/usr/bin/env bash
# qa/lib/fixtures.sh — deterministic fake project repos.
#
# Every fixture is written from FIXED literal content, never a random generator, so
# two runs on two machines produce byte-identical trees. That is what makes a red
# reproducible enough to hand to someone else.
#
# The shapes exist to cover different environment/secret layouts a real project has:
# a flat single .env, a nested multi-env tree, a Docker-secrets tree like Inception's,
# a decoy tree full of near-miss filenames, and a hostile tree whose names and values
# are chosen to break naive scanners and naive serializers.

set -uo pipefail

: "${QA_GEN:=$QA_ROOT/fixtures/gen}"

# Remove and recreate a fixture root so every run starts from the same bytes.
fixture_reset() {
	rm -rf "$QA_GEN/$1"
	mkdir -p "$QA_GEN/$1"
	printf '%s' "$QA_GEN/$1"
}

# A flat project: one .env at the root, ordinary KEY=VALUE pairs.
fixture_flat() {
	local root
	root="$(fixture_reset flat)"
	cat >"$root/.env" <<'EOF'
APP_NAME=flat
DB_HOST=localhost
DB_PORT=5432
DB_PASSWORD=s3cr3t-flat-pw
API_TOKEN=tok_flat_0001
EOF
	printf '%s' "$root"
}

# A nested project: four env files at four depths. Round-tripping this is the real
# test of path preservation — a fetch must restore each file to its own directory.
fixture_nested() {
	local root
	root="$(fixture_reset nested)"
	mkdir -p "$root/srcs" "$root/config" "$root/apps/web" "$root/apps/api"
	cat >"$root/.env" <<'EOF'
ROOT_LEVEL=1
SHARED_SECRET=root-shared-value
EOF
	cat >"$root/srcs/.env" <<'EOF'
COMPOSE_PROJECT_NAME=nested
MYSQL_PASSWORD=nested-mysql-pw
EOF
	cat >"$root/config/db.env" <<'EOF'
PGUSER=nested_user
PGPASSWORD=nested-pg-pw
EOF
	cat >"$root/apps/web/.env.production" <<'EOF'
NEXT_PUBLIC_URL=https://example.invalid
SESSION_KEY=nested-web-session-key
EOF
	cat >"$root/apps/api/.env.local" <<'EOF'
JWT_SIGNING_KEY=nested-api-jwt-key
EOF
	printf '%s' "$root"
}

# A Docker-secrets tree shaped like Inception: config in srcs/.env, but the actual
# secrets are extensionless-ish files under secrets/. 42ctl's default scan patterns
# are *.env* and *.secrets, so nothing under secrets/ matches — which is precisely
# what s11 exists to prove.
fixture_secrets_tree() {
	local root
	root="$(fixture_reset secrets_tree)"
	mkdir -p "$root/srcs" "$root/secrets"
	cat >"$root/srcs/.env" <<'EOF'
DOMAIN_NAME=qa.42.fr
MYSQL_DATABASE=wordpress
MYSQL_USER=wpuser
FTP_USER=ftpuser
EOF
	printf 'qa-db-password-0001\n' >"$root/secrets/db_password.txt"
	printf 'qa-db-root-password-0001\n' >"$root/secrets/db_root_password.txt"
	printf 'qa-ftp-password-0001\n' >"$root/secrets/ftp_password.txt"
	printf 'admin:qa-admin-credential-0001\n' >"$root/secrets/credentials.txt"
	printf -- '-----BEGIN CERTIFICATE-----\nQUFB\n-----END CERTIFICATE-----\n' >"$root/secrets/server.crt"
	printf -- '-----BEGIN PRIVATE KEY-----\nQkJC\n-----END PRIVATE KEY-----\n' >"$root/secrets/server.key"
	printf '%s' "$root"
}

# Near-miss filenames. Exactly one of these holds a live secret; the rest are decoys
# that a scanner must NOT sweep into the vault (a committed template) or must not
# mistake for the real file (editor and backup droppings).
fixture_decoys() {
	local root
	root="$(fixture_reset decoys)"
	cat >"$root/.env" <<'EOF'
REAL_SECRET=decoys-live-value
EOF
	cat >"$root/.env.example" <<'EOF'
REAL_SECRET=replace-me
EOF
	cat >"$root/.env.sample" <<'EOF'
REAL_SECRET=replace-me
EOF
	printf 'REAL_SECRET=stale-backup-value\n' >"$root/.env.bak"
	printf 'REAL_SECRET=editor-swap-value\n' >"$root/.env.swp"
	mkdir -p "$root/node_modules/leftover" "$root/target/debug"
	printf 'VENDORED=should-not-be-swept\n' >"$root/node_modules/leftover/.env"
	printf 'BUILD=should-not-be-swept\n' >"$root/target/debug/.env"
	printf '%s' "$root"
}

# Hostile content. Every line here is a serializer or round-trip hazard: CRLF, an
# equals sign inside the value, quotes, a very long value, an empty value, leading
# and trailing whitespace, a UTF-8 BOM, non-ASCII, and no trailing newline at EOF.
fixture_hostile_values() {
	local root
	root="$(fixture_reset hostile_values)"
	{
		printf '\xEF\xBB\xBFBOM_FIRST=bom-leading-value\n'
		printf 'CRLF_LINE=crlf-value\r\n'
		printf 'EQUALS_IN_VALUE=a=b=c\n'
		printf 'QUOTED_VALUE="double quoted"\n'
		printf "SINGLE_QUOTED='single quoted'\n"
		printf 'EMPTY_VALUE=\n'
		printf 'SPACED_VALUE=   padded   \n'
		printf 'UNICODE_VALUE=clé-privée-🔐\n'
		printf 'HASH_IN_VALUE=not#a#comment\n'
		printf 'DOLLAR_VALUE=$NOT_EXPANDED\n'
		printf 'BACKTICK_VALUE=`not_executed`\n'
		printf 'NEWLINE_ESCAPED=line1\\nline2\n'
		printf 'LONG_VALUE=%s\n' "$(printf 'x%.0s' $(seq 1 4096))"
		printf 'NO_TRAILING_NEWLINE=last-value-no-eol'
	} >"$root/.env"
	printf '%s' "$root"
}

# Hostile FILENAMES, all legal on Linux. A scanner and a manifest must survive a
# space, a hash, a unicode name, a dotted name and a very long name without losing,
# colliding or truncating any of them.
fixture_hostile_names() {
	local root
	root="$(fixture_reset hostile_names)"
	mkdir -p "$root/dir with spaces" "$root/dir.with.dots" "$root/répertoire"
	printf 'A=1\n' >"$root/with space.env"
	printf 'B=2\n' >"$root/with#hash.env"
	printf 'C=3\n' >"$root/dir with spaces/.env"
	printf 'D=4\n' >"$root/dir.with.dots/nested.env"
	printf 'E=5\n' >"$root/répertoire/.env"
	printf 'F=6\n' >"$root/$(printf 'l%.0s' $(seq 1 180)).env"
	printf '%s' "$root"
}

# A project with no secret material at all. Push/pull must handle the empty set
# without inventing a manifest entry or erroring.
fixture_empty() {
	local root
	root="$(fixture_reset empty)"
	printf '# no env files here\n' >"$root/README.md"
	printf '%s' "$root"
}

# Build every fixture and echo one "name<TAB>path" line each.
fixture_build_all() {
	printf 'flat\t%s\n' "$(fixture_flat)"
	printf 'nested\t%s\n' "$(fixture_nested)"
	printf 'secrets_tree\t%s\n' "$(fixture_secrets_tree)"
	printf 'decoys\t%s\n' "$(fixture_decoys)"
	printf 'hostile_values\t%s\n' "$(fixture_hostile_values)"
	printf 'hostile_names\t%s\n' "$(fixture_hostile_names)"
	printf 'empty\t%s\n' "$(fixture_empty)"
}

# Create a project marker with explicit scan patterns.
#
# Note the trap this exists to work around: passing --project to push REPLACES the
# marker's patterns with the defaults, so a project configured to store arbitrary files
# silently stores nothing when the flag is used. Push without --project to honour them.
fixture_project_marker() {
	local root="$1" id="$2" patterns="${3:-\"*\"}"
	mkdir -p "$root/.42ctl"
	printf '{"project_id":"%s","patterns":[%s]}' "$id" "$patterns" >"$root/.42ctl/project.json"
}

# A file of an exact byte size, filled deterministically so a mismatch is meaningful.
fixture_sized_file() {
	local path="$1" bytes="$2"
	mkdir -p "$(dirname "$path")"
	head -c "$bytes" /dev/zero | tr '\0' 'A' >"$path"
}

# A large file whose megabytes differ, with a unique plaintext canary at the front.
#
# fixture_sized_file writes one byte repeated, which is deterministic but degenerate for
# chunking: with content-addressed chunks a file of identical bytes collapses to a single
# stored object, so a spec written on it would report success while exercising neither the
# ordering nor the reassembly it claims to test. Every mebibyte here gets its own filler
# byte, so each chunk is distinct, and the canary gives the zero-knowledge searches
# something specific to look for.
fixture_varied_file() {
	local path="$1" mib="$2" canary="$3" i ch
	mkdir -p "$(dirname "$path")"
	: >"$path"
	for i in $(seq 0 $((mib - 1))); do
		ch="$(printf '%b' "$(printf '\\%03o' $((65 + i % 26)))")"
		head -c 1048576 /dev/zero | tr '\0' "$ch" >>"$path"
	done
	printf '%s' "$canary" | dd of="$path" conv=notrunc status=none
}

# An orchestrator repo with submodules, the shape the user actually works in: one root
# project that pulls in several other projects, each carrying its own environment and
# secrets. Submodules get a `.git` FILE, as real submodules do, rather than a
# directory, so the scanner sees them exactly as it would in a real checkout.
fixture_orchestrator() {
	local root
	root="$(fixture_reset orchestrator)"
	printf 'ROOT_ENV=orchestrator-root\nSHARED_TOKEN=root-token-0001\n' >"$root/.env"

	_submodule() {
		local path="$1" name="$2"
		mkdir -p "$root/$path/srcs"
		printf 'gitdir: ../../.git/modules/%s\n' "$name" >"$root/$path/.git"
		printf 'SERVICE_NAME=%s\nDB_PASSWORD=%s-db-pw\n' "$name" "$name" >"$root/$path/.env"
		printf 'COMPOSE_NAME=%s\n' "$name" >"$root/$path/srcs/.env"
	}
	_submodule services/api api
	_submodule services/web web
	_submodule libs/shared shared

	# A submodule placed under vendor/. `vendor` is in the scanner's skip list because
	# it usually holds vendored dependencies, but it is also a very common place to put
	# submodules — and there the skip becomes silent data loss.
	_submodule vendor/thirdparty thirdparty

	# And a genuinely vendored library beside it: NOT a repository, carrying a config file
	# that must stay out of the vault. This is the case that makes "just stop skipping
	# vendor" the wrong fix, and the case the scan should MENTION rather than take.
	mkdir -p "$root/vendor/plainlib"
	printf 'VENDORED_DEFAULT=not-ours\n' >"$root/vendor/plainlib/config.env"

	# A submodule that is ALSO its own 42ctl project, which is the normal case once a
	# team manages each service separately.
	mkdir -p "$root/services/api/.42ctl"
	printf '{"project_id":"submodule-api","patterns":["*.env*","*.secrets"]}' \
		>"$root/services/api/.42ctl/project.json"

	unset -f _submodule
	printf '%s' "$root"
}
