#!/bin/bash
# *************************************************************************** #
#                                                                             #
#   journey.sh — the real-case pipeline against the LIVE deployment.          #
#                                                                             #
#   A clean Debian machine installs the released 42ctl with install.sh, then  #
#   three new people run everything a team does: sign up behind the register  #
#   token, keep personal secrets, found an organisation, share an environment #
#   through a team, push a tree to a teammate, and remove somebody and rotate.#
#   Every step is checked, including the refusals.                            #
#                                                                             #
#   It is NOT part of the battery, on purpose: it creates real accounts and   #
#   an organisation named pg-org-<epoch> on production each time it runs, and #
#   wakes both scale-to-zero machines. Run it by hand after a release:        #
#                                                                             #
#     docker run --rm -e VAULT42_REGISTER_TOKEN -v "$PWD/qa/live/journey.sh:/journey.sh:ro" \
#       public.ecr.aws/docker/library/debian:bookworm-slim bash /journey.sh   #
#                                                                             #
#   VAULT42_REGISTER_TOKEN is passed by name. Exits non-zero if any check     #
#   failed. Addresses are playground-<who>-<epoch>@example.com: nothing it   #
#   does sends mail.                                                          #
#                                                                             #
# *************************************************************************** #
set -u
[ -n "${VAULT42_REGISTER_TOKEN:-}" ] || { echo "VAULT42_REGISTER_TOKEN must be set (by name)"; exit 2; }
N="$(date +%s)"
PASS=0
FAIL=0
FAILED=()

check() {
	local desc="$1"
	shift
	local out
	if out="$("$@" 2>&1)"; then
		PASS=$((PASS + 1))
		printf 'ok   %s\n' "$desc"
	else
		FAIL=$((FAIL + 1))
		FAILED+=("$desc")
		printf 'FAIL %s\n' "$desc"
		printf '%s\n' "$out" | tail -4 | sed 's/^/     | /'
	fi
}

# as <who> <42ctl args…> — each person has their own identity, session and contract.
as() {
	local who="$1"
	shift
	mkdir -p "/people/$who"
	FT_CONFIG="/people/$who/config.json" FT_KEYSTORE="/people/$who/keystore.v42" \
		FT_SESSION="/people/$who/session.tok" FT_CONTRACT="/people/$who/contract.tok" \
		FT_PASSPHRASE="pass-$who-$N" FT_PASSWORD="Pw-$who-$N-long-enough" NO_COLOR=1 \
		42ctl "$@"
}
mail() { printf 'playground-%s-%s@example.com' "$1" "$N"; }
export -f as mail
export N

apt-get update -qq >/dev/null 2>&1 && apt-get install -y -qq curl ca-certificates >/dev/null 2>&1

printf '\n== a clean machine installs the released 42ctl\n'
check "install.sh installs a signed release" \
	sh -c 'curl -fsSL https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | sh'
check "the installed version is the latest release" \
	sh -c '42ctl update --check | grep -q "up to date"'
42ctl version | head -1
check "a fresh install already points at production" \
	bash -c 'as ada config show | grep -q "https://vault42-authority.fly.dev"'
check "production answers its health route" \
	sh -c 'curl -fsS https://vault42-authority.fly.dev/healthz'

printf '\n== three people get identities and accounts\n'
for who in ada bea cid; do
	check "$who creates an identity" as "$who" keys init
	check "$who signs up with the register token" as "$who" auth signup --email "$(mail "$who")" --token "$VAULT42_REGISTER_TOKEN"
	check "$who logs in and takes a contract for their tenant" \
		as "$who" auth login --password --email "$(mail "$who")" --tenant "pg-$who-$N"
	check "$who holds both credentials" \
		bash -c 'as "$1" auth status | grep -q "session and contract"' _ "$who"
done
check "sign-up without the register token is refused" \
	bash -c '! as dan auth signup --email "$(mail dan)" >/dev/null 2>&1'

printf '\n== personal secrets, sealed on the machine\n'
check "ada stores a secret" bash -c 'printf "sk_live_%s" "$N" | as ada vault set app/STRIPE'
check "ada reads it back exactly" bash -c '[ "$(as ada vault get app/STRIPE)" = "sk_live_$N" ]'
check "the listing shapes as json" bash -c 'as ada vault ls --format json | grep -q "\"Path\": \"app/STRIPE\""'
check "bea cannot read ada's secret" bash -c '! as bea vault get app/STRIPE 2>/dev/null | grep -q "sk_live_$N"'

printf '\n== ada builds an organisation and shares an environment\n'
ORG="pg-org-$N"
export ORG
check "ada founds an organisation" as ada org create --slug "$ORG" --name Playground
PROJECT="$(as ada project create --org "$ORG" --slug web --name Web 2>/dev/null | awk '$1=="id"{print $2}')"
export PROJECT
check "ada creates a project" test -n "$PROJECT"
check "and a prod environment" as ada env create --project "$PROJECT" --name prod
for who in bea cid; do
	TOKEN="$(as ada org invite --org "$ORG" --email "$(mail "$who")" --role member 2>/dev/null | awk '$1=="token"{print $2}')"
	check "$who is invited and accepts" as "$who" invite accept --token "$TOKEN"
done
for who in ada bea cid; do check "$who publishes their keys to the organisation" as "$who" keys enroll --org "$ORG"; done
check "ada grants herself admin" as ada project grant add --org "$ORG" --project "$PROJECT" --user "$(mail ada)" --role admin
check "ada makes a team of readers and puts bea in it" \
	bash -c 'as ada team create --org "$ORG" --slug readers --name Readers &&
		as ada team member add --org "$ORG" --team readers --user "$(mail bea)" &&
		as ada team grant --org "$ORG" --team readers --project "$PROJECT" --role read --env prod'
check "ada creates prod's key and hands it out" \
	bash -c 'as ada env init --org "$ORG" --project web --env prod && as ada env keys sync --org "$ORG" --project web --env prod'
BEA_ID="$(as bea auth me 2>/dev/null | awk '$1=="account"{print $2}')"
export BEA_ID
check "the key listing shows bea, by account id, active" \
	bash -c '[ -n "$BEA_ID" ] && as ada env keys ls --org "$ORG" --project web --env prod --format "{{.Member}}:{{.State}}" | grep -qx "$BEA_ID:active"'
check "ada stores an environment secret" \
	bash -c 'printf "DB_PASSWORD=prod-%s" "$N" | as ada env secret set --org "$ORG" --project web --env prod app/db'
check "bea reads it through her team" \
	bash -c '[ "$(as bea env secret get --org "$ORG" --project web --env prod app/db)" = "DB_PASSWORD=prod-$N" ]'
check "bea, a reader, cannot overwrite it" \
	bash -c '! printf "tampered" | as bea env secret set --org "$ORG" --project web --env prod app/db >/dev/null 2>&1'
check "cid, a member with no grant, cannot read it" \
	bash -c '! as cid env secret get --org "$ORG" --project web --env prod app/db 2>/dev/null | grep -q "prod-$N"'
check "cid cannot invite anyone" \
	bash -c '! as cid org invite --org "$ORG" --email "$(mail eve)" --role member >/dev/null 2>&1'
check "cid cannot create a project" \
	bash -c '! as cid project create --org "$ORG" --slug x --name X >/dev/null 2>&1'

printf '\n== a tree travels to a teammate\n'
mkdir -p /tree/.42ctl /tree/srcs /tree/secrets /restore
printf 'DOMAIN=%s.example\n' "$N" >/tree/srcs/.env
printf 'root-%s\n' "$N" >/tree/secrets/db_root_password.txt
printf '{"project_id":"pg-tree-%s","patterns":["*.env*"]}' "$N" >/tree/.42ctl/project.json
check "ada pushes the tree to prod" bash -c 'cd /tree && as ada env push --org "$ORG" --project web --env prod'
check "bea lists its files" bash -c 'cd /restore && as bea env files --org "$ORG" --project web --env prod --format "{{.Path}}" | grep -qx srcs/.env'
check "bea restores it byte for byte" \
	bash -c 'cd /restore && as bea env pull --org "$ORG" --project web --env prod --apply >/dev/null &&
		cmp -s /tree/srcs/.env /restore/srcs/.env && cmp -s /tree/secrets/db_root_password.txt /restore/secrets/db_root_password.txt'

printf '\n== somebody leaves\n'
check "ada removes bea and is told to rotate" \
	bash -c 'as ada org member rm --org "$ORG" --user "$(mail bea)" | grep -qi rotate'
check "ada rotates prod's key" as ada env keys rotate --org "$ORG" --project web --env prod
check "ada stores a new secret" \
	bash -c 'printf "DB_PASSWORD=after-%s" "$N" | as ada env secret set --org "$ORG" --project web --env prod app/db'
check "bea cannot read what was written after she left" \
	bash -c '! as bea env secret get --org "$ORG" --project web --env prod app/db 2>/dev/null | grep -q "after-$N"'
check "ada still can" \
	bash -c '[ "$(as ada env secret get --org "$ORG" --project web --env prod app/db)" = "DB_PASSWORD=after-$N" ]'

printf '\n== %d passed, %d failed  (org %s)\n' "$PASS" "$FAIL" "$ORG"
[ "$FAIL" -eq 0 ] || { printf 'failed: %s\n' "${FAILED[*]}"; exit 1; }
