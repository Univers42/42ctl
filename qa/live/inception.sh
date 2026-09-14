#!/usr/bin/env bash
# *************************************************************************** #
#                                                                             #
#   inception.sh — a real project's secrets, through a real deployment.       #
#                                                                             #
#   Inception (github.com/Univers42/Inception) is a PUBLIC repository whose   #
#   `make setup` generates real secrets: srcs/.env, five random passwords, a  #
#   local certificate authority and a TLS key pair. None of them is ever      #
#   committed. This drives what a team does with them, end to end, and checks #
#   every step — the refusals included:                                       #
#                                                                             #
#     install · identities and accounts · personal vault · personal sync ·    #
#     notes · organisation, team, group, grants · environment keys ·          #
#     environment secrets · the shared tree to a fresh clone · private files ·#
#     partial and backed-up restores · readers and outsiders · offboarding    #
#     and rotation · the restored credentials still fit together.             #
#                                                                             #
#   Every command in docs/manual/13-walkthrough.md is one that ran here.      #
#                                                                             #
#   It runs on a Linux host with bash, git, make, openssl, curl and hellish   #
#   (Inception's Makefile refuses any other shell). No Docker is needed. It   #
#   is NOT part of the battery: against production it creates four accounts  #
#   and an organisation named inception-<epoch>. Run it by hand:              #
#                                                                             #
#     VAULT42_REGISTER_TOKEN=… bash qa/live/inception.sh [WORKDIR]            #
#                                                                             #
#   C42_AUTHORITY / C42_SERVER  another deployment (default: the built-in     #
#                               production endpoints)                         #
#   C42_BIN                     a 42ctl to test (default: install the latest  #
#                               release with install.sh into WORKDIR/bin)     #
#   INCEPTION_REPO / _REF       what to clone (default: GitHub, main)         #
#                                                                             #
#   Addresses are inception-<who>-<epoch>@example.com: nothing sends mail.    #
#   Exits non-zero if any check failed.                                       #
#                                                                             #
# *************************************************************************** #
set -u

WORK="${1:-$(mktemp -d)}"
N="$(date +%s)"
ORG="inception-$N"
INCEPTION_REPO="${INCEPTION_REPO:-https://github.com/Univers42/Inception.git}"
INCEPTION_REF="${INCEPTION_REF:-main}"
PASS=0
FAIL=0
FAILED=()
mkdir -p "$WORK"
WORK="$(cd "$WORK" && pwd)"

check() {
	local desc="$1" out
	shift
	if out="$("$@" 2>&1)"; then
		PASS=$((PASS + 1))
		printf 'ok   %s\n' "$desc"
	else
		FAIL=$((FAIL + 1))
		FAILED+=("$desc")
		printf 'FAIL %s\n' "$desc"
		printf '%s\n' "$out" | tail -6 | sed 's/^/     | /'
	fi
}
section() { printf '\n== %s\n' "$*"; }

# as <who> <42ctl args…> — each person has their own home, identity, session and contract.
as() {
	local who="$1" home="$WORK/people/$1"
	shift
	mkdir -p "$home"
	HOME="$home" FT_CONFIG="$home/config.json" FT_KEYSTORE="$home/keystore.v42" \
		FT_SESSION="$home/session.tok" FT_CONTRACT="$home/contract.tok" \
		FT_PASSPHRASE="pass-$who-$N" FT_PASSWORD="Pw-$who-$N-long-enough" NO_COLOR=1 \
		"$C42" "$@"
}
# within <dir> <who> <42ctl args…> — the same, from inside a checkout.
within() {
	local dir="$1"
	shift
	(cd "$dir" && as "$@")
}
mail() { printf 'inception-%s-%s@example.com' "$1" "$N"; }
field() { awk -v k="$1" '$1 == k { print $2; exit }'; }
clone() { git clone -q --depth 1 --branch "$INCEPTION_REF" "$INCEPTION_REPO" "$1"; }
# restored <baseline> <dir> — every file the baseline lists is byte-exact under <dir>.
restored() { (cd "$2" && sha256sum --quiet -c "$1"); }
export -f as within mail field clone restored
export WORK N ORG C42 INCEPTION_REPO INCEPTION_REF

command -v hellish >/dev/null || { echo "hellish must be on PATH: Inception's Makefile runs on nothing else"; exit 2; }
[ -n "${VAULT42_REGISTER_TOKEN:-}" ] || printf 'note: VAULT42_REGISTER_TOKEN is unset; signup works only where account creation is open\n'
SIGNUP_TOKEN=()
[ -n "${VAULT42_REGISTER_TOKEN:-}" ] && SIGNUP_TOKEN=(--token "$VAULT42_REGISTER_TOKEN")

# ── 0 ────────────────────────────────────────────────────────────────────────
section "a machine installs 42ctl"
if [ -z "${C42_BIN:-}" ]; then
	check "install.sh installs the latest release, checksum verified" \
		bash -c 'curl -fsSL https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh |
			FT_BIN_DIR="$WORK/bin" FT_NO_MODIFY_PATH=1 sh'
	C42="$WORK/bin/42ctl"
else
	C42="$C42_BIN"
fi
export C42
"$C42" version | head -1
check "the installed binary is the latest release" bash -c '"$C42" update --check | grep -q "up to date"'
AUTHORITY="${C42_AUTHORITY:-$(as ada config show | awk '/authority/ { print $2; exit }')}"
export AUTHORITY
check "the authority answers its health route" bash -c 'curl -fsS "$AUTHORITY/healthz" | grep -qx ok'
curl -fsS "$AUTHORITY/version" 2>/dev/null && printf '\n'

# ── 1 ────────────────────────────────────────────────────────────────────────
section "four people: ada leads, bea develops, cid audits, dan is an outsider"
for who in ada bea cid dan; do
	if [ -n "${C42_AUTHORITY:-}" ]; then
		check "$who points at the deployment" \
			as "$who" config endpoint --authority "$C42_AUTHORITY" --server "$C42_SERVER"
	fi
	check "$who creates an identity" as "$who" keys init
	check "$who signs up" as "$who" auth signup --email "$(mail "$who")" "${SIGNUP_TOKEN[@]}"
	check "$who logs in and takes a contract for a tenant of their own" \
		as "$who" auth login --password --email "$(mail "$who")" --tenant "inc-$who-$N"
	check "$who holds a session and a contract" \
		bash -c 'as "$1" auth status | grep -q "session and contract"' _ "$who"
done
ADA_ID="$(as ada auth whoami | field principal)"
BEA_ADDR="$(as bea keys export-pub | tail -1)"
export ADA_ID BEA_ADDR

# ── 2 ────────────────────────────────────────────────────────────────────────
section "ada's personal vault"
printf 'ghp_inception_%s' "$N" >"$WORK/token.txt"
openssl rand 256 >"$WORK/deploy.key"
printf 'API_KEY=api-%s\nAPI_URL=https://api.example.com\n' "$N" >"$WORK/tools.env"
check "a value from stdin" bash -c 'as ada vault set tools/GITHUB_TOKEN <"$WORK/token.txt"'
check "and a binary file, byte-exact" \
	bash -c 'as ada vault set tools/deploy.key --file "$WORK/deploy.key" &&
		as ada vault get tools/deploy.key | cmp -s - "$WORK/deploy.key"'
check "rotation re-seals and changes nothing readable" \
	bash -c 'as ada vault rotate tools/GITHUB_TOKEN && [ "$(as ada vault get tools/GITHUB_TOKEN)" = "$(cat "$WORK/token.txt")" ]'
check "the listing names both, and the rotated one at version 2" \
	bash -c 'out="$(as ada vault ls tools/ --format "{{.Path}}={{.Version}}")" &&
		grep -qx "tools/GITHUB_TOKEN=2" <<<"$out" && grep -qx "tools/deploy.key=1" <<<"$out" || { echo "$out"; exit 1; }'
check "an older version stays readable" bash -c '[ "$(as ada vault get tools/GITHUB_TOKEN --version 1)" = "$(cat "$WORK/token.txt")" ]'
check "a .env file imports under a prefix, one secret per key, and exports back" \
	bash -c 'as ada vault import "$WORK/tools.env" --prefix imported &&
		[ "$(as ada vault export --prefix imported | sort | tr "\n" " ")" = "API_KEY=api-$N API_URL=https://api.example.com " ]'
check "a secret shared with bea is read by bea, under ada's name" \
	bash -c 'as ada vault share tools/GITHUB_TOKEN --to "$BEA_ADDR" &&
		[ "$(as bea vault get "shared/$ADA_ID/tools/GITHUB_TOKEN")" = "$(cat "$WORK/token.txt")" ]'
check "bea cannot read ada's vault itself" bash -c '! as bea vault get tools/GITHUB_TOKEN'
check "removing secrets removes them" \
	bash -c 'as ada vault rm imported/API_KEY imported/API_URL && ! as ada vault get imported/API_KEY'

# ── 3 ────────────────────────────────────────────────────────────────────────
section "ada clones the public repository and generates its secrets"
ADA_TREE="$WORK/ada/inception"
export ADA_TREE
check "the public clone holds no secret" \
	bash -c 'clone "$ADA_TREE" && [ ! -e "$ADA_TREE/secrets" ] && [ ! -e "$ADA_TREE/srcs/.env" ]'
check "make setup generates the environment, passwords, CA and TLS key pair" \
	bash -c 'cd "$ADA_TREE" && make setup DATA_DIR="$WORK/data" >"$WORK/setup.log" 2>&1 || { tail -20 "$WORK/setup.log"; exit 1; }
		for f in srcs/.env secrets/db_password.txt secrets/db_root_password.txt secrets/ftp_password.txt \
			secrets/api_db_password.txt secrets/credentials.txt secrets/ca.crt secrets/ca.key \
			secrets/server.crt secrets/server.key; do [ -s "$f" ] || { echo "missing $f"; exit 1; }; done'
check "git tracks none of them, so the repository stays publishable" \
	bash -c 'cd "$ADA_TREE" && [ -z "$(git status --porcelain --untracked-files=all | grep -E "secrets/|srcs/\.env")" ]'
printf 'WP_TITLE=Ada on her laptop\n' >"$ADA_TREE/srcs/.env.local"
(cd "$ADA_TREE" && sha256sum srcs/.env secrets/*) >"$WORK/shared.sha"
(cd "$ADA_TREE" && sha256sum srcs/.env srcs/.env.local secrets/*) >"$WORK/everything.sha"
printf '     baseline: %s shared file(s), plus ada'"'"'s private srcs/.env.local\n' "$(wc -l <"$WORK/shared.sha")"

# ── 4 ────────────────────────────────────────────────────────────────────────
section "ada's personal sync: the same tree, sealed to ada alone"
check "push seals every scanned file" \
	bash -c 'within "$ADA_TREE" ada push | tee "$WORK/push.out" | grep -q "pushed"'
check "a second checkout's pull is a dry run that writes nothing" \
	bash -c 'clone "$WORK/ada/laptop" && within "$WORK/ada/laptop" ada pull >"$WORK/pull-dry.out" &&
		[ ! -e "$WORK/ada/laptop/secrets" ] && [ ! -e "$WORK/ada/laptop/srcs/.env" ]'
check "pull --apply restores every file byte-exact, the private override included" \
	bash -c 'within "$WORK/ada/laptop" ada pull --apply >/dev/null && restored "$WORK/everything.sha" "$WORK/ada/laptop"'

# ── 5 ────────────────────────────────────────────────────────────────────────
section "a note that travels with the project"
printf '# Inception runbook\nmake up; make test\n' >"$WORK/runbook.md"
check "note add, ls and get" \
	bash -c 'within "$ADA_TREE" ada note add runbook.md --file "$WORK/runbook.md" &&
		within "$ADA_TREE" ada note ls -q | grep -qx runbook.md &&
		within "$ADA_TREE" ada note get runbook.md | cmp -s - "$WORK/runbook.md"'

# ── 6 ────────────────────────────────────────────────────────────────────────
section "ada founds the organisation, and gives access by team and by group"
check "org create" as ada org create --slug "$ORG" --name "Inception team"
PROJECT="$(as ada project create --org "$ORG" --slug inception --name Inception | field id)"
export PROJECT
check "project create" test -n "$PROJECT"
check "two environments" \
	bash -c 'as ada env create --project "$PROJECT" --name prod && as ada env create --project "$PROJECT" --name dev &&
		[ "$(as ada env ls --project "$PROJECT" -q | wc -l)" -eq 2 ]'
for who in bea cid; do
	token="$(as ada org invite --org "$ORG" --email "$(mail "$who")" --role member | field token)"
	check "$who is invited and accepts" as "$who" invite accept --token "$token"
done
check "the organisation has three members" bash -c '[ "$(as ada org member ls --org "$ORG" -q | wc -l)" -eq 3 ]'
check "a developers team holding bea, which may write prod" \
	bash -c 'as ada team create --org "$ORG" --slug devs --name Developers &&
		as ada team member add --org "$ORG" --team devs --user "$(mail bea)" &&
		as ada team grant --org "$ORG" --team devs --project inception --role write --env prod'
GROUP="$(as ada group create --project "$PROJECT" | field id)"
export GROUP
check "an auditors group holding cid, which may read prod" \
	bash -c '[ -n "$GROUP" ] && as ada group member add --group "$GROUP" --user "$(mail cid)" &&
		as ada project grant add --org "$ORG" --project inception --group "$GROUP" --role read --env prod'
check "the grant listing says whom each grant is for" \
	bash -c 'out="$(as ada project grant ls --org "$ORG" --project inception --format "{{.Kind}}:{{.Role}}")" &&
		grep -qx "team:write" <<<"$out" && grep -qx "group:read" <<<"$out" || { echo "$out"; exit 1; }'

# ── 7 ────────────────────────────────────────────────────────────────────────
E=(--org "$ORG" --project inception --env prod)
section "prod gets its key, and the key reaches everyone granted"
for who in ada bea cid; do check "$who publishes their public keys to the organisation" as "$who" keys enroll --org "$ORG"; done
check "ada initialises prod's key" as ada env init "${E[@]}"
check "and wraps it to every granted member" as ada env keys sync "${E[@]}"
check "all three are active" \
	bash -c '[ "$(as ada env keys ls --org "$ORG" --project inception --env prod --filter State=active -q | wc -l)" -eq 3 ]'

# ── 8 ────────────────────────────────────────────────────────────────────────
section "a single shared credential"
printf 'registry-token-%s' "$N" >"$WORK/registry.txt"
check "ada stores CI_REGISTRY_TOKEN in prod" \
	bash -c 'as ada env secret set CI_REGISTRY_TOKEN --org "$ORG" --project inception --env prod <"$WORK/registry.txt"'
for who in bea cid; do
	check "$who reads it" \
		bash -c '[ "$(as "$1" env secret get CI_REGISTRY_TOKEN --org "$ORG" --project inception --env prod)" = "$(cat "$WORK/registry.txt")" ]' _ "$who"
done
check "cid, a reader, cannot overwrite it" \
	bash -c '! printf x | as cid env secret set CI_REGISTRY_TOKEN --org "$ORG" --project inception --env prod'
check "dan, an outsider, cannot read it" \
	bash -c '! as dan env secret get CI_REGISTRY_TOKEN --org "$ORG" --project inception --env prod'

# ── 9 ────────────────────────────────────────────────────────────────────────
section "the whole tree, shared with prod"
check "ada pushes it, with labels; her .env.local stays hers" \
	bash -c 'within "$ADA_TREE" ada env push --org "$ORG" --project inception --env prod --label app=inception --label stage=prod |
		grep -q "shared and 1 private file"'
check "the inventory marks the private file, and every file carries the push's labels" \
	bash -c '[ "$(as ada env files --org "$ORG" --project inception --env prod --filter Private=true -q)" = "srcs/.env.local" ] &&
		all="$(as ada env files --org "$ORG" --project inception --env prod -q | wc -l)" &&
		[ "$all" -gt "$(wc -l <"$WORK/shared.sha")" ] &&
		[ "$(as ada env files --org "$ORG" --project inception --env prod --filter label=stage=prod -q | wc -l)" -eq "$all" ]'
check "bea's fresh clone: the dry run names every file and writes none" \
	bash -c 'clone "$WORK/bea/inception" && within "$WORK/bea/inception" bea env pull --org "$ORG" --project inception --env prod >"$WORK/bea-dry.out" &&
		grep -q "secrets/server.key" "$WORK/bea-dry.out" && [ ! -e "$WORK/bea/inception/secrets" ]'
check "env pull --apply restores the shared tree byte-exact" \
	bash -c 'within "$WORK/bea/inception" bea env pull --org "$ORG" --project inception --env prod --apply >/dev/null &&
		restored "$WORK/shared.sha" "$WORK/bea/inception"'
check "and never ada's private file, not even its name" \
	bash -c '[ ! -e "$WORK/bea/inception/srcs/.env.local" ] &&
		! as bea env files --org "$ORG" --project inception --env prod -q | grep -q "env.local"'
check "restored keys are owner-only, and the recreated secrets/ is 0700" \
	bash -c '[ "$(stat -c %a "$WORK/bea/inception/secrets/server.key")" = 600 ] && [ "$(stat -c %a "$WORK/bea/inception/secrets")" = 700 ]'
check "the restored certificate matches the restored key" \
	bash -c 'cd "$WORK/bea/inception/secrets" &&
		[ "$(openssl x509 -in server.crt -noout -pubkey)" = "$(openssl pkey -in server.key -pubout)" ]'
check "and chains to the restored certificate authority" \
	bash -c 'cd "$WORK/bea/inception/secrets" && openssl verify -CAfile ca.crt server.crt'
check "the restored srcs/.env is the one compose reads" \
	bash -c 'grep -q "^DOMAIN_NAME=" "$WORK/bea/inception/srcs/.env" && grep -q "^MYSQL_USER=" "$WORK/bea/inception/srcs/.env"'
check "--only restores just the passwords, and says what it left behind" \
	bash -c 'clone "$WORK/bea/passwords" && out="$(within "$WORK/bea/passwords" bea env pull --org "$ORG" --project inception --env prod --only "secrets/*.txt" --apply)" &&
		[ "$(ls "$WORK/bea/passwords/secrets" | wc -l)" -eq 5 ] && [ ! -e "$WORK/bea/passwords/srcs/.env" ] && grep -q "not selected" <<<"$out" || { echo "$out"; exit 1; }'
check "a pattern that matches nothing is refused" \
	bash -c '! within "$WORK/bea/passwords" bea env pull --org "$ORG" --project inception --env prod --only "secret/*" --apply'
check "--backup keeps every file it replaces, even two that share a name" \
	bash -c 'cd "$WORK/bea/inception" && printf edited >secrets/server.crt && printf edited >secrets/server.key &&
		within "$WORK/bea/inception" bea env pull --org "$ORG" --project inception --env prod --apply --backup >/dev/null &&
		restored "$WORK/shared.sha" "$WORK/bea/inception" &&
		[ "$(cat secrets/server.crt.bak 2>/dev/null)" = edited ] && [ "$(cat secrets/server.key.bak 2>/dev/null)" = edited ] ||
		{ ls -la secrets; exit 1; }'
check "cid, a reader, restores the same tree" \
	bash -c 'clone "$WORK/cid/inception" && within "$WORK/cid/inception" cid env pull --org "$ORG" --project inception --env prod --apply >/dev/null &&
		restored "$WORK/shared.sha" "$WORK/cid/inception"'
check "but cannot publish one" \
	bash -c '! within "$WORK/cid/inception" cid env push --org "$ORG" --project inception --env prod'
check "dan, an outsider, restores nothing" \
	bash -c 'clone "$WORK/dan/inception"; ! within "$WORK/dan/inception" dan env pull --org "$ORG" --project inception --env prod --apply &&
		[ ! -e "$WORK/dan/inception/secrets" ]'

# ── 10 ───────────────────────────────────────────────────────────────────────
section "two private overrides in one environment"
printf 'WP_TITLE=Bea on her desktop\n' >"$WORK/bea/inception/srcs/.env.local"
rm -f "$WORK/bea/inception/secrets/"*.bak
check "bea, a writer, pushes the tree with her own .env.local" \
	bash -c 'within "$WORK/bea/inception" bea env push --org "$ORG" --project inception --env prod | grep -q "1 private file"'
check "ada's fresh clone gets the shared tree and her own override, not bea's" \
	bash -c 'clone "$WORK/ada/fresh" && within "$WORK/ada/fresh" ada env pull --org "$ORG" --project inception --env prod --apply >/dev/null &&
		restored "$WORK/everything.sha" "$WORK/ada/fresh"'
check "bea's fresh clone gets hers" \
	bash -c 'clone "$WORK/bea/fresh" && within "$WORK/bea/fresh" bea env pull --org "$ORG" --project inception --env prod --apply >/dev/null &&
		[ "$(cat "$WORK/bea/fresh/srcs/.env.local")" = "WP_TITLE=Bea on her desktop" ]'

# ── 11 ───────────────────────────────────────────────────────────────────────
section "cid leaves the auditors, and prod is rotated"
check "ada removes cid from the group" as ada group member rm --group "$GROUP" --user "$(mail cid)"
check "ada rotates prod's key" as ada env keys rotate "${E[@]}"
check "cid restores nothing any more" \
	bash -c 'rm -rf "$WORK/cid/after"; clone "$WORK/cid/after"; ! within "$WORK/cid/after" cid env pull --org "$ORG" --project inception --env prod --apply &&
		[ ! -e "$WORK/cid/after/secrets" ]'
check "nor reads the shared credential" \
	bash -c '! as cid env secret get CI_REGISTRY_TOKEN --org "$ORG" --project inception --env prod'
check "bea still restores the whole shared tree" \
	bash -c 'clone "$WORK/bea/after" && within "$WORK/bea/after" bea env pull --org "$ORG" --project inception --env prod --apply >/dev/null &&
		restored "$WORK/shared.sha" "$WORK/bea/after"'
check "and ada everything, her private override included" \
	bash -c 'clone "$WORK/ada/after" && within "$WORK/ada/after" ada env pull --org "$ORG" --project inception --env prod --apply >/dev/null &&
		restored "$WORK/everything.sha" "$WORK/ada/after"'
check "the shared credential survives the rotation" \
	bash -c '[ "$(as bea env secret get CI_REGISTRY_TOKEN --org "$ORG" --project inception --env prod)" = "$(cat "$WORK/registry.txt")" ]'

printf '\n%s passed, %s failed — work directory %s\n' "$PASS" "$FAIL" "$WORK"
for f in "${FAILED[@]}"; do printf '  FAIL %s\n' "$f"; done
[ "$FAIL" -eq 0 ]
