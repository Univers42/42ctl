#!/usr/bin/env bash
# s42 — one person's first day, from an empty machine to a second laptop, through 42ctl alone.
#
# The question this spec answers is the one an operator actually asks before handing the tool
# to somebody: starting from nothing — no identity, no account, no project — does the whole
# pipeline hold? Configure it, make an identity, make an account, keep personal secrets and
# read them back in every shape the listings offer, push a project and its notes, lose the
# laptop, recover on another machine and get everything back byte for byte, then found an
# organisation and share an environment. Each step feeds the next, so a verb that quietly
# stopped working breaks the story instead of hiding in a spec that tests it alone.
#
# It is also where the verbs no other spec runs get run: config profile/show, keys init's
# refusal to overwrite, vault import/export/audit/rotate/rm, db, notes, help, version,
# update --check, auth mfa, and the absence of the two verbs that could never act against vault42 —
# `unseal` (no seal state) and `org github` (routes the authority never had).
#
# Spellings are the current ones throughout; `vault get-env --help` is checked once to prove
# the old spelling still lands on the same page.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"
source "$QA_LIB_DIR/fixtures.sh"

spec_begin "s42-fresh-user-journey"
qa_require_docker_stack
qa_require_cmd python3
trap 'qa_server_cleanup; [ "${QA_KEEP_SERVER:-0}" = "1" ] || qa_authority_down' EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
assert_green "the authority is listening" -- qa_authority_up
[ "$_qa_regression" -eq 0 ] || {
	spec_end
	exit 1
}

N="$$$(date +%s)"
W="$QA_RESULTS/s42"
rm -rf "$W"
mkdir -p "$W/laptop" "$W/second" "$W/shared-restore"
MAIL="lea-$N@archicode.codes"
PW="pw-lea-$N"
ORG="lea-co-$N"
PROJECT_ID="s42-journey-$N"
qa_actor_reset lea
qa_actor_reset lea2
export N W MAIL PW ORG PROJECT_ID

# lea <42ctl args…> — run the CLI on her laptop.
lea() { QA_ACCOUNT_PASSWORD="$PW" qa_actor lea "$W/laptop" "$*"; }
# lea2 <42ctl args…> — the same person on a second machine that starts empty.
lea2() { QA_PASS_OVERRIDE=qa-pass-lea QA_ACCOUNT_PASSWORD="$PW" qa_actor lea2 "$W/second" "$*"; }
field() { awk -v k="$1" '$1 == k { print $2; exit }'; }
export -f lea lea2 field

# ── the binary and its manual ────────────────────────────────────────────────
# A battery build has no git inside its container, so the commit honestly reads `unknown` there;
# a release build stamps the SHA (`release.yml` sets FT_GIT_SHA).
assert_green "version names a semver release and the commit it was built from, or says unknown" \
	-- bash -c 'lea version 2>/dev/null | head -1 | grep -qE "^42ctl [0-9]+\.[0-9]+\.[0-9]+ \(([0-9a-f]{7,40}|unknown)\)$"'
assert_green "the root help is grouped the way docker's is" \
	-- bash -c 'out="$(lea --help 2>/dev/null)" || exit 1
		for s in "Common Commands:" "Management Commands:" "Operator Commands:" "Global Options:"; do
			grep -qx "$s" <<<"$out" || { echo "missing: $s"; exit 1; }
		done'
assert_green "help commands lists every command, the reshaped ones included" \
	-- bash -c 'out="$(lea help commands 2>/dev/null)" || exit 1
		[ "$(grep -c "^    42ctl " <<<"$out")" -ge 90 ] && grep -q "^    42ctl env secret get " <<<"$out" &&
		grep -q "^    42ctl project grant rm " <<<"$out"'
assert_green "the kickoff topic walks the current spellings" \
	-- bash -c 'out="$(lea help kickoff 2>/dev/null)" || exit 1
		grep -q "42ctl env init" <<<"$out" && ! grep -q "vault env-init" <<<"$out"'
assert_green "an unknown help topic is refused, naming the ones that exist" \
	-- bash -c 'out="$(lea help nonsense-topic 2>&1)" && exit 1; grep -q "topics: kickoff" <<<"$out"'
assert_green "the old spelling of a verb still opens its page" \
	-- bash -c '[ "$(lea vault get-env --help 2>/dev/null | head -1)" = "$(lea env secret get --help 2>/dev/null | head -1)" ]'

# ── an identity, then an account ─────────────────────────────────────────────
ADDR="$(lea keys export-pub 2>/dev/null)"
export ADDR
assert_green "a fresh identity has a shareable address" -- bash -c '[[ "$ADDR" =~ ^v42:[A-Za-z0-9_-]+$ ]]'
assert_green "keys init refuses to overwrite the identity that exists" \
	-- bash -c 'out="$(lea keys init 2>&1)" && exit 1; grep -q "already exists" <<<"$out" && grep -q -- "--force" <<<"$out"'
assert_green "and the identity is unchanged by the refusal" -- bash -c '[ "$(lea keys export-pub 2>/dev/null)" = "$ADDR" ]'
assert_green "she signs up" -- bash -c 'lea "auth signup --email $MAIL" >/dev/null 2>&1'
assert_green "she logs in" -- bash -c 'lea "auth login --password --email $MAIL" 2>/dev/null | grep -qx "signed in as $MAIL"'
assert_green "account show is her account" -- bash -c 'lea account show 2>/dev/null | grep -qE "^email +$MAIL$"'
assert_green "whoami reports the same address the identity exports" \
	-- bash -c 'lea auth whoami 2>/dev/null | grep -qE "^address +$ADDR$"'

# ── configuration ────────────────────────────────────────────────────────────
assert_green "config show names both endpoints the CLI talks to" \
	-- bash -c 'out="$(lea config show 2>/dev/null)" || exit 1
		grep -qE "^profile +default$" <<<"$out" && grep -q "http://qa42-srv" <<<"$out" && grep -q "http://qa42-auth" <<<"$out"'
assert_green "a second profile is created and listed beside the first" \
	-- bash -c 'lea config profile staging >/dev/null 2>&1 || exit 1
		out="$(lea config profile 2>/dev/null)"; grep -q "default" <<<"$out" && grep -q "staging" <<<"$out"'
assert_green "and switching back leaves default active" \
	-- bash -c 'lea config profile default >/dev/null 2>&1 && lea config show 2>/dev/null | grep -qE "^profile +default$"'

# ── personal secrets, and every way to read them back ────────────────────────
STRIPE="sk_live_s42_$N"
printf '%s' "$STRIPE" >"$W/laptop/stripe.txt"
printf '%s' "postgres://lea:s42@db/$N" >"$W/laptop/db.txt"
printf 'API_KEY=s42-api-%s\nREGION=eu-west\n' "$N" >"$W/laptop/import.env"
export STRIPE
expect_line() { [ "$1" = "$2" ] || { diff <(printf "%s\n" "$2") <(printf "%s\n" "$1"); return 1; }; }
export -f expect_line
assert_green "a secret is stored from a file" \
	-- bash -c 'lea vault set app/STRIPE_KEY --file /project/stripe.txt 2>/dev/null | grep -qx "pushed app/STRIPE_KEY (v1)"'
assert_green "a secret is stored from stdin" \
	-- bash -c 'lea "vault set app/DB_URL < /project/db.txt" 2>/dev/null | grep -qx "pushed app/DB_URL (v1)"'
assert_green "it reads back byte for byte" -- bash -c '[ "$(lea vault get app/STRIPE_KEY 2>/dev/null)" = "$STRIPE" ]'
assert_green "the listing, as a template, is the two paths" \
	-- bash -c 'expect_line "$(lea vault ls --format {{.Path}} 2>/dev/null | sort | tr "\n" " ")" "app/DB_URL app/STRIPE_KEY "'
assert_green "a prefix narrows the listing and -q prints only paths" \
	-- bash -c '[ "$(lea vault ls app/ -q 2>/dev/null | grep -c "^app/")" -eq 2 ]'
assert_green "piped with no shaping, the listing stays tab-separated for scripts" \
	-- bash -c 'lea vault ls 2>/dev/null | grep -qP "^app/STRIPE_KEY\t1\t"'
assert_green "the JSON listing carries Path, Version and Updated" \
	-- bash -c 'lea vault ls --format json 2>/dev/null | python3 -c "
import json, sys
rows = json.load(sys.stdin)
assert len(rows) == 2 and all(set(r) == {\"Path\", \"Version\", \"Updated\"} for r in rows), rows
"'
assert_green "a filter selects one path" -- bash -c '[ "$(lea vault ls -q --filter Path=app/DB_URL 2>/dev/null)" = app/DB_URL ]'
assert_green "a filter naming a path that is not there is refused, not answered with nothing" \
	-- bash -c 'out="$(lea vault ls --filter Path=app/NOPE 2>&1)" && exit 1; grep -q "no row matches Path=app/NOPE" <<<"$out"'
assert_green "rotating re-seals under a fresh key at the next version" \
	-- bash -c 'lea vault rotate app/STRIPE_KEY 2>/dev/null | grep -qx "rotated app/STRIPE_KEY to v2"'
assert_green "the rotated secret still reads the same, and the listing says v2" \
	-- bash -c '[ "$(lea vault get app/STRIPE_KEY 2>/dev/null)" = "$STRIPE" ] &&
		[ "$(lea vault ls --format {{.Version}} --filter Path=app/STRIPE_KEY 2>/dev/null)" = 2 ]'
assert_green "the version before the rotation is still readable by number" \
	-- bash -c '[ "$(lea vault get app/STRIPE_KEY --version 1 2>/dev/null)" = "$STRIPE" ]'
assert_green "db reads the same records" \
	-- bash -c '[ "$(lea db get app/DB_URL 2>/dev/null)" = "postgres://lea:s42@db/$N" ] &&
		[ "$(lea db ls --format {{.Path}} 2>/dev/null | sort | tr "\n" " ")" = "app/DB_URL app/STRIPE_KEY " ]'
assert_green "a .env file is imported one secret per line" \
	-- bash -c 'lea vault import /project/import.env >/dev/null 2>&1'
assert_green "and export gives every line back" \
	-- bash -c 'out="$(lea vault export 2>/dev/null)" || exit 1
		grep -qx "API_KEY=s42-api-$N" <<<"$out" && grep -qx "REGION=eu-west" <<<"$out"'
assert_green "the audit chain recorded what she did, one numbered entry each" \
	-- bash -c 'out="$(lea vault audit 2>/dev/null)" || exit 1
		[ "$(grep -cE "^seq [0-9]+ ts [0-9]+ " <<<"$out")" -ge 4 ]'
assert_green "removing a secret says so and it leaves the listing" \
	-- bash -c 'lea vault rm app/DB_URL 2>/dev/null | grep -qx "removed app/DB_URL" &&
		! lea vault ls -q 2>/dev/null | grep -qx app/DB_URL'
assert_green "removing it again is not an error, and says it was not there" \
	-- bash -c 'lea vault rm app/DB_URL 2>/dev/null | grep -qx "app/DB_URL: not found"'
assert_green "the server's stored bytes can be read back" -- qa_dump_server_db "$W/db.after-secrets"
assert_zero_knowledge "her personal secret never reached the server in the clear" \
	"$STRIPE" "app/STRIPE_KEY" "$W/db.after-secrets"

# ── a project, its tree and its notes ────────────────────────────────────────
cp -r "$(fixture_secrets_tree)/." "$W/laptop/"
fixture_project_marker "$W/laptop" "$PROJECT_ID" '"srcs/.env"'
assert_green "a scan pattern naming a path is refused, and the refusal says how to fix it" \
	-- bash -c 'out="$(lea push 2>&1)" && exit 1
		grep -q "names a path, but patterns match file names" <<<"$out"'
fixture_project_marker "$W/laptop" "$PROJECT_ID" '"*.env*"'
printf '# onboarding for %s\n' "$N" >"$W/laptop/onboarding.txt"
assert_green "the project's tree is pushed" \
	-- bash -c 'lea push 2>/dev/null | grep -qE "pushed [1-9][0-9]* file\(s\)"'
assert_green "a note is added from a file" -- bash -c 'lea note add onboarding.md --file /project/onboarding.txt >/dev/null 2>&1'
assert_green "the note is listed" -- bash -c '[ "$(lea note ls --format {{.Note}} 2>/dev/null)" = onboarding.md ]'
assert_green "the note reads back" -- bash -c '[ "$(lea note get onboarding.md 2>/dev/null)" = "# onboarding for $N" ]'
assert_green "pull without --apply writes nothing and says it was a dry run" \
	-- bash -c 'before="$(find "$1" -type f -newer "$1/onboarding.txt" ! -path "*/.42ctl/*" | wc -l)"
		lea pull 2>/dev/null | grep -q "dry-run" || exit 1
		[ "$(find "$1" -type f -newer "$1/onboarding.txt" ! -path "*/.42ctl/*" | wc -l)" -eq "$before" ]' _ "$W/laptop"

# ── the laptop is lost: a second machine gets everything back ────────────────
assert_green "the laptop escrows its sealed keystore" \
	-- bash -c 'qa_actor_otp lea "$W/laptop" "keys escrow --email $MAIL" "$MAIL" 2>&1 | grep -qi escrowed'
assert_green "the second machine recovers the same identity" \
	-- bash -c 'QA_PASS_OVERRIDE=qa-pass-lea qa_actor_otp lea2 "$W/second" "keys recover --email $MAIL" "$MAIL" >/dev/null 2>&1
		[ "$(lea2 keys export-pub 2>/dev/null)" = "$ADDR" ]'
assert_green "and logs in to the same account" \
	-- bash -c 'lea2 "auth login --password --email $MAIL" 2>/dev/null | grep -qx "signed in as $MAIL"'
fixture_project_marker "$W/second" "$PROJECT_ID" '"*.env*"'
assert_green "the second machine restores the project tree byte for byte" \
	-- bash -c 'lea2 pull --apply >/dev/null 2>&1 || exit 1
		for f in srcs/.env secrets/db_password.txt secrets/db_root_password.txt; do
			cmp -s "$W/laptop/$f" "$W/second/$f" || { echo "differs or missing: $f"; exit 1; }
		done'
assert_green "its notes come with it" -- bash -c '[ "$(lea2 note get onboarding.md 2>/dev/null)" = "# onboarding for $N" ]'
assert_green "a note removed on the second machine is gone for both" \
	-- bash -c 'lea2 note rm onboarding.md 2>/dev/null | grep -qx "note onboarding.md removed" || exit 1
		! lea note get onboarding.md >/dev/null 2>&1'
assert_green "and so do her personal secrets" -- bash -c '[ "$(lea2 vault get app/STRIPE_KEY 2>/dev/null)" = "$STRIPE" ]'

# ── her own organisation, one environment, shared with herself ───────────────
assert_green "she founds an organisation" -- bash -c 'lea org create --slug "$ORG" --name Lea >/dev/null 2>&1'
PROJ="$(lea project create --org "$ORG" --slug web --name Web 2>/dev/null | field id)"
export PROJ
assert_green "creates a project" -- test -n "$PROJ"
assert_green "and an environment" -- bash -c 'lea env create --project "$PROJ" --name prod >/dev/null 2>&1'
assert_green "publishes her keys to the organisation" -- bash -c 'lea keys enroll --org "$ORG" >/dev/null 2>&1'
assert_green "grants herself admin on the project, as the kickoff says" \
	-- bash -c 'lea project grant add --org "$ORG" --project "$PROJ" --user "$MAIL" --role admin 2>/dev/null | grep -q "^grant_id "'
assert_green "creates prod's key and hands it out" \
	-- bash -c 'lea env init --org "$ORG" --project web --env prod >/dev/null 2>&1 &&
		lea env keys sync --org "$ORG" --project web --env prod >/dev/null 2>&1'
assert_green "her key state for prod is active" \
	-- bash -c 'lea env keys ls --org "$ORG" --project web --env prod --format {{.State}} 2>/dev/null | grep -qx active'
printf 'SESSION_SECRET=s42-env-%s\n' "$N" >"$W/laptop/env-secret.txt"
assert_green "an environment secret goes in and comes out" \
	-- bash -c 'lea "env secret set --org $ORG --project web --env prod app/session < /project/env-secret.txt" >/dev/null 2>&1 || exit 1
		[ "$(lea env secret get --org "$ORG" --project web --env prod app/session 2>/dev/null)" = "SESSION_SECRET=s42-env-$N" ]'
assert_green "the tree is pushed to the environment" \
	-- bash -c 'lea env push --org "$ORG" --project web --env prod >/dev/null 2>&1'
assert_green "the environment's files are listed with their paths" \
	-- bash -c 'lea env files --org "$ORG" --project web --env prod --format {{.Path}} 2>/dev/null | grep -qx srcs/.env'
assert_green "and the second machine pulls the environment's tree too" \
	-- bash -c 'QA_PASS_OVERRIDE=qa-pass-lea qa_actor lea2 "$W/shared-restore" "env pull --org $ORG --project web --env prod --apply" >/dev/null 2>&1 || exit 1
		cmp -s "$W/laptop/srcs/.env" "$W/shared-restore/srcs/.env"'

# ── the second factor ────────────────────────────────────────────────────────
assert_green "she turns the email second factor on, proving the mailbox" \
	-- bash -c 'qa_actor_otp lea "$W/laptop" "auth mfa --on" "$MAIL" 2>/dev/null | grep -qx "second factor REQUIRED for $MAIL"'
assert_green "the account now says a second factor is required" \
	-- bash -c 'lea auth me 2>/dev/null | grep -qE "^second +factor +required$"'
assert_green "and she turns it off again the same way" \
	-- bash -c 'qa_actor_otp lea "$W/laptop" "auth mfa --off" "$MAIL" 2>/dev/null | grep -qx "second factor off for $MAIL"'

# ── no verb that cannot act ───────────────────────────────────────────────────
# `unseal` and `org github` could only ever be refused against vault42 — no seal state, and no
# `/v1/orgs/{org}/github/*` routes on the authority — and a command that exists only to refuse
# still counts as covered. They are gone; neither the parser nor the help may offer them.
assert_green "unseal and org github are not commands, and help commands does not list them" \
	-- bash -c '! lea unseal >/dev/null 2>&1 && ! lea "org github sync $ORG" >/dev/null 2>&1 &&
		! lea "help commands" 2>/dev/null | grep -qE "42ctl (unseal|org github)"'
assert_green "update --check reports what is installed" \
	-- bash -c 'lea update --check 2>/dev/null | grep -qE "^installed +[0-9]+\.[0-9]+\.[0-9]+"'

# ── the end of the day ───────────────────────────────────────────────────────
assert_green "logging out ends the session on this profile" \
	-- bash -c 'lea auth logout >/dev/null 2>&1 && lea auth status 2>/dev/null | grep -qx "profile .default.: logged out"'

spec_end
