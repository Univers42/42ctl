#!/usr/bin/env bash
# s41 — who may do what, asked through 42ctl alone.
#
# Every other organisation spec builds its world over REST (`qa_api POST /v1/orgs …`) and uses
# the CLI only for the data. That proves the authority's rules and leaves 42ctl's own half —
# the flags, the request each verb builds, what it prints, how a refusal reads — unexercised: a
# coverage count against the parser found the org, team, group, project, invite and grant verbs
# invoked by NO spec. Here nothing is set up by hand. Every account, invite, team, grant and
# secret is made by `42ctl`, spelled the way the reshaped tree spells it (the older specs keep
# the old spellings on purpose, so the two together cover both).
#
# The cast, and the rights each should end up with:
#   ada  — founds the organisation: owner
#   ben  — invited as admin: may administer, may NOT mint an owner
#   cy   — member, in team `writers`, granted WRITE on prod through the team
#   dot  — member, in team `readers`, granted READ on prod through the team; later removed
#   eve  — plain member: in the organisation, granted nothing
#   fox  — has an account, is in no organisation
#
# "A member can do X because their team was granted it" is the rule under test, so every
# refusal is paired with the positive that proves the same action works for someone entitled
# to it. A refusal on its own is satisfied by a system where nothing works.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"
source "$QA_LIB_DIR/fixtures.sh"

spec_begin "s41-cli-rights"
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
W="$QA_RESULTS/s41"
rm -rf "$W"
ORG="acme-$N"
PEOPLE="ada ben cy dot eve fox"
for who in $PEOPLE; do
	qa_actor_reset "$who"
	mkdir -p "$W/$who"
done
export N W ORG

# act <who> <42ctl args…> — run the CLI as someone, with their password in FT_PASSWORD.
# Templates are written without spaces because the argument string is word-split in the
# container, exactly as it is for every other spec.
act() {
	local who="$1"
	shift
	QA_ACCOUNT_PASSWORD="pw-$who-$N" qa_actor "$who" "$W/$who" "$*"
}
# The value of a `label  value` line that a verb prints.
field() { awk -v k="$1" '$1 == k { print $2; exit }'; }
mail() { printf '%s-%s@archicode.codes' "$1" "$N"; }
export -f act field mail

# expect_as <description> <who> <exact expected stdout> <42ctl args…>
expect_as() {
	local desc="$1" who="$2" want="$3"
	shift 3
	assert_green "$desc" -- bash -c 'who="$1"; want="$2"; shift 2
		got="$(act "$who" "$@" 2>/dev/null)" || { printf "exit non-zero\n"; exit 1; }
		[ "$got" = "$want" ] || { diff <(printf "%s\n" "$want") <(printf "%s\n" "$got"); exit 1; }' \
		_ "$who" "$want" "$@"
}
# refused_as <description> <who> <text the refusal must contain, or ""> <42ctl args…>
refused_as() {
	local desc="$1" who="$2" says="$3"
	shift 3
	assert_green "$desc" -- bash -c 'who="$1"; says="$2"; shift 2
		out="$(act "$who" "$@" 2>&1)" && { printf "it was allowed:\n%s\n" "$out"; exit 1; }
		[ -z "$says" ] || grep -qi -- "$says" <<<"$out" || { printf "refused, but not with %s:\n%s\n" "$says" "$out"; exit 1; }' \
		_ "$who" "$says" "$@"
}

# ── accounts, entirely through the CLI ───────────────────────────────────────
for who in $PEOPLE; do
	assert_green "$who signs up through the CLI" \
		-- bash -c 'act "$1" "auth signup --email $(mail "$1")" >/dev/null 2>&1' _ "$who"
	assert_green "$who logs in with a password and is told who they are" \
		-- bash -c 'act "$1" "auth login --password --email $(mail "$1")" 2>/dev/null | grep -qx "signed in as $(mail "$1")"' _ "$who"
done
declare -A ID
for who in $PEOPLE; do ID[$who]="$(act "$who" "auth me" 2>/dev/null | field account)"; done
assert_green "six people hold six distinct account ids" \
	-- bash -c 'printf "%s\n" "$@" | grep -c . | grep -qx 6 && [ "$(printf "%s\n" "$@" | sort -u | wc -l)" -eq 6 ]' _ "${ID[@]}"
expect_as "auth status says a password login holds a session, and what a vault server may also need" ada \
	"profile 'default': logged in — session only; a gated vault server also needs \`auth login --tenant <name>\`" "auth status"
assert_green "auth me names the account's own address" \
	-- bash -c 'act ada "auth me" 2>/dev/null | grep -qE "^email +$(mail ada)$"'
assert_green "a wrong password is refused, and the session already held is kept" \
	-- bash -c 'QA_ACCOUNT_PASSWORD=not-the-password qa_actor fox "$W/fox" "auth login --password --email $(mail fox)" >/dev/null 2>&1 && exit 1
		act fox "auth status" 2>/dev/null | grep -q "^profile .default.: logged in"'

# ── an organisation, and the invites that fill it ────────────────────────────
assert_green "ada founds the organisation and gets its id back" \
	-- bash -c 'out="$(act ada "org create --slug $ORG --name Acme" 2>/dev/null)" || exit 1
		grep -qx "created org .Acme. ($ORG)" <<<"$out" && [ -n "$(field id <<<"$out")" ]'
expect_as "the founder is the organisation's owner" ada "owner" "org member ls --org $ORG --format {{.Role}}"

declare -A INV INV_ID
invite() { act ada "org invite --org $ORG --email $(mail "$1") --role $2" 2>/dev/null; }
for pair in ben:admin cy:member dot:member eve:member; do
	who="${pair%%:*}"
	out="$(invite "$who" "${pair#*:}")"
	INV[$who]="$(field token <<<"$out")"
	INV_ID[$who]="$(field invite_id <<<"$out")"
done
assert_green "four invites were issued, each with a token and an id" \
	-- bash -c 'for v in "$@"; do [ -n "$v" ] || exit 1; done; [ $# -eq 8 ]' _ "${INV[@]}" "${INV_ID[@]}"
assert_green "the addressee can read their pending invite" \
	-- bash -c 'out="$(act cy "invite show --id $1" 2>/dev/null)" || exit 1
		grep -qE "^status +pending$" <<<"$out" && grep -qE "^role +member$" <<<"$out"' _ "${INV_ID[cy]}"
refused_as "somebody else cannot read that invite" fox "" "invite show --id ${INV_ID[cy]}"
refused_as "an invite token is useless to anyone but its addressee" eve "" "invite accept --token ${INV[ben]}"
for who in ben cy dot eve; do
	expect_as "$who accepts their own invite" "$who" "accepted invite" "invite accept --token ${INV[$who]}"
done
refused_as "the same invite cannot be redeemed twice" ben "" "invite accept --token ${INV[ben]}"
assert_green "the roster in full, sorted, holds every role once per person" \
	-- bash -c '[ "$(act ada "org member ls --org $ORG --format {{.Role}}" 2>/dev/null | sort | tr "\n" " ")" = "admin member member member owner " ]'
assert_green "-q with a role filter yields exactly the three member ids" \
	-- bash -c '[ "$(act ada "org member ls --org $ORG -q --filter Role=member" 2>/dev/null | sort)" = "$(printf "%s\n" "$@" | sort)" ]' \
	_ "${ID[cy]}" "${ID[dot]}" "${ID[eve]}"
assert_green "the roster as JSON carries UserID, Role and Joined for all five" \
	-- bash -c 'act ada "org member ls --org $ORG --format json" 2>/dev/null | python3 -c "
import json, sys
rows = json.load(sys.stdin)
assert len(rows) == 5 and all(set(r) == {\"UserID\", \"Role\", \"Joined\"} for r in rows), rows
"'
refused_as "a filter that keeps nobody is refused, not answered with an empty roster" ada "no row matches" \
	"org member ls --org $ORG --filter Role=auditor"

# An outsider must not learn whether an organisation exists: the refusal for a real one and
# for one that was never created must be the same words.
assert_green "an outsider is refused the roster with the same words as for an org that does not exist" \
	-- bash -c 'real="$(act fox "org member ls --org $ORG" 2>&1)" && exit 1
		ghost="$(act fox "org member ls --org ghost-$N" 2>&1)" && exit 1
		[ "${real//$ORG/X}" = "${ghost//ghost-$N/X}" ] || { printf "%s\n---\n%s\n" "$real" "$ghost"; exit 1; }'

# ── administration is for administrators ─────────────────────────────────────
refused_as "a plain member cannot invite anyone" eve "" "org invite --org $ORG --email $(mail zed) --role member"
refused_as "an admin cannot mint an owner" ben "" "org invite --org $ORG --email $(mail heir) --role owner"
assert_green "an admin may still invite another admin" \
	-- bash -c 'act ben "org invite --org $ORG --email $(mail peer) --role admin" 2>/dev/null | grep -q "^token "'
assert_green "the owner may name another owner" \
	-- bash -c 'act ada "org invite --org $ORG --email $(mail heir) --role owner" 2>/dev/null | grep -q "^token "'

refused_as "a plain member cannot create a project" eve "" "project create --org $ORG --slug api --name API"
PROJECT="$(act ben "project create --org $ORG --slug api --name API" 2>/dev/null | field id)"
export PROJECT
assert_green "an admin creates the project and gets its id" -- test -n "$PROJECT"
expect_as "the project is listed by slug and name" eve "api/API" "project ls --org $ORG --format {{.Slug}}/{{.Name}}"

refused_as "a plain member cannot create an environment" eve "" "env create --project $PROJECT --name prod"
assert_green "the owner creates prod" \
	-- bash -c 'act ada "env create --project $PROJECT --name prod" 2>/dev/null | grep -q "created environment .prod."'
expect_as "the environment is listed" cy "prod" "env ls --project $PROJECT --format {{.Name}}"

refused_as "a plain member cannot create a team" eve "" "team create --org $ORG --slug rogue --name Rogue"
for team in writers readers; do
	assert_green "the owner creates team $team" \
		-- bash -c 'act ada "team create --org $ORG --slug $1 --name $1" >/dev/null 2>&1' _ "$team"
done
assert_green "both teams are listed, sorted" \
	-- bash -c '[ "$(act eve "team ls --org $ORG --format {{.Slug}}" 2>/dev/null | sort | tr "\n" " ")" = "readers writers " ]'
expect_as "cy joins writers, addressed by email" ada "added $(mail cy) to team 'writers' as 'member'" \
	"team member add --org $ORG --team writers --user $(mail cy)"
expect_as "dot joins readers" ada "added $(mail dot) to team 'readers' as 'member'" \
	"team member add --org $ORG --team readers --user $(mail dot)"
refused_as "somebody outside the organisation cannot be put in a team" ada "" \
	"team member add --org $ORG --team writers --user $(mail fox)"
refused_as "a plain member cannot put themselves in a team" eve "" \
	"team member add --org $ORG --team writers --user $(mail eve)"

assert_green "writers are granted WRITE on prod" \
	-- bash -c 'act ada "team grant --org $ORG --team writers --project $PROJECT --role write --env prod" 2>/dev/null | grep -q "^grant_id "'
assert_green "readers are granted READ on prod" \
	-- bash -c 'act ada "team grant --org $ORG --team readers --project $PROJECT --role read --env prod" 2>/dev/null | grep -q "^grant_id "'
refused_as "a plain member cannot grant a team anything" eve "" \
	"team grant --org $ORG --team readers --project $PROJECT --role admin"
assert_green "the grant listing shows one write and one read" \
	-- bash -c '[ "$(act ada "project grant ls --org $ORG --project $PROJECT --format {{.Role}}" 2>/dev/null | sort | tr "\n" " ")" = "read write " ]'

GRANT="$(act ada "project grant add --org $ORG --project $PROJECT --user $(mail eve) --role read" 2>/dev/null | field grant_id)"
export GRANT
assert_green "a user grant is added and its id printed" -- test -n "$GRANT"
assert_green "three grants are listed now" \
	-- bash -c '[ "$(act ada "project grant ls --org $ORG --project $PROJECT -q" 2>/dev/null | grep -c .)" -eq 3 ]'
refused_as "a plain member cannot revoke a grant" eve "" "project grant rm --org $ORG --project $PROJECT --grant $GRANT"
assert_green "the owner revokes it, and is told a rotation is what ends access already held" \
	-- bash -c 'out="$(act ada "project grant rm --org $ORG --project $PROJECT --grant $GRANT" 2>/dev/null)" || exit 1
		[ "$(head -1 <<<"$out")" = "revoked grant $GRANT on project '"'"'$PROJECT'"'"'" ] && grep -qi "rotate" <<<"$out"'
assert_green "and two remain" \
	-- bash -c '[ "$(act ada "project grant ls --org $ORG --project $PROJECT -q" 2>/dev/null | grep -c .)" -eq 2 ]'

# ── groups ───────────────────────────────────────────────────────────────────
refused_as "a plain member cannot create a group" eve "" "group create --project $PROJECT"
GROUP="$(act ada "group create --project $PROJECT" 2>/dev/null | field id)"
export GROUP
assert_green "the owner creates a group" -- test -n "$GROUP"
expect_as "a member is added to the group by email, as the flag documents" ada "added $(mail eve) to group '$GROUP'" \
	"group member add --group $GROUP --user $(mail eve)"
refused_as "somebody outside the organisation cannot be added to a group" ada "" \
	"group member add --group $GROUP --user ${ID[fox]}"
GINV="$(act ada "group invite --group $GROUP --email $(mail dot)" 2>/dev/null | field token)"
export GINV
expect_as "a group invite is accepted by its addressee" dot "accepted invite" "invite accept --token $GINV"
assert_green "a member is removed from the group" \
	-- bash -c 'act ada "group member rm --group $GROUP --user $(mail eve)" 2>/dev/null | grep -q "removed $(mail eve) from group"'

# ── the environment's key, and who ends up holding it ────────────────────────
for who in ada ben cy dot eve; do
	assert_green "$who publishes their public keys to the organisation" \
		-- bash -c 'act "$1" "keys enroll --org $ORG" >/dev/null 2>&1' _ "$who"
done
refused_as "somebody outside the organisation cannot publish keys to it" fox "" "keys enroll --org $ORG"
assert_green "the owner creates prod's key" -- bash -c 'act ada "env init --org $ORG --project api --env prod" >/dev/null 2>&1'
refused_as "a plain member cannot hand the key out" eve "" "env keys sync --org $ORG --project api --env prod"
assert_green "the owner hands the key to everyone a grant covers" \
	-- bash -c 'act ada "env keys sync --org $ORG --project api --env prod" >/dev/null 2>&1'
assert_green "the writer and the reader hold the key, and the ungranted member does not" \
	-- bash -c 'act ada "env keys ls --org $ORG --project api --env prod --format json" 2>/dev/null | python3 -c "
import json, sys
rows = {r[\"Member\"]: r[\"State\"] for r in json.load(sys.stdin)}
assert rows.get(sys.argv[1]) == \"active\", rows
assert rows.get(sys.argv[2]) == \"active\", rows
assert sys.argv[3] not in rows, rows
" "$1" "$2" "$3"' _ "${ID[cy]}" "${ID[dot]}" "${ID[eve]}"

CANARY="DB_PASSWORD=s41-team-only-$N"
printf '%s\n' "$CANARY" >"$W/ada/secret.txt"
printf '%s\n' "DB_PASSWORD=s41-reader-overwrite-$N" >"$W/dot/secret.txt"
printf '%s\n' "DB_PASSWORD=s41-writer-update-$N" >"$W/cy/secret.txt"
export CANARY
assert_green "the owner stores a secret in prod" \
	-- bash -c 'act ada "env secret set --org $ORG --project api --env prod app/db < /project/secret.txt" >/dev/null 2>&1'
for who in cy dot; do
	assert_green "$who reads it, because their TEAM was granted prod" \
		-- bash -c '[ "$(act "$1" "env secret get --org $ORG --project api --env prod app/db" 2>/dev/null)" = "$CANARY" ]' _ "$who"
done
for who in eve fox; do
	assert_green "$who cannot read it — not refused politely, unable to decrypt" \
		-- bash -c '! act "$1" "env secret get --org $ORG --project api --env prod app/db" 2>/dev/null | grep -qF "$CANARY"' _ "$who"
done
refused_as "the reader cannot overwrite it" dot "" "env secret set --org $ORG --project api --env prod app/db < /project/secret.txt"
assert_green "and the secret is exactly what the owner stored" \
	-- bash -c '[ "$(act cy "env secret get --org $ORG --project api --env prod app/db" 2>/dev/null)" = "$CANARY" ]'
assert_green "the writer CAN overwrite it, through the team's write grant" \
	-- bash -c 'act cy "env secret set --org $ORG --project api --env prod app/db < /project/secret.txt" >/dev/null 2>&1'
assert_green "and the reader sees the writer's update" \
	-- bash -c 'act dot "env secret get --org $ORG --project api --env prod app/db" 2>/dev/null | grep -qx "DB_PASSWORD=s41-writer-update-$N"'
refused_as "a plain member cannot rotate the environment's key" eve "" "env keys rotate --org $ORG --project api --env prod"

# ── a whole tree, shared through the team ────────────────────────────────────
TREE="$W/ada/tree"
rm -rf "$TREE"
cp -r "$(fixture_secrets_tree)" "$TREE"
fixture_project_marker "$TREE" "s41-tree-$N" '"*"'
mkdir -p "$W/dot/restore" "$W/eve/restore"
assert_green "the tree to share holds real files" \
	-- bash -c '[ "$(find "$1" -type f ! -path "*/.42ctl/*" | wc -l)" -ge 2 ]' _ "$TREE"
assert_green "the owner pushes the tree to prod" \
	-- bash -c 'qa_actor ada "$1" "env push --org $ORG --project api --env prod" >/dev/null 2>&1' _ "$TREE"
assert_green "the reader's listing names every file of the tree without fetching one" \
	-- bash -c 'got="$(qa_actor dot "$2" "env files --org $ORG --project api --env prod --format {{.Path}}" 2>/dev/null | sort)" || exit 1
		want="$(cd "$1" && find . -type f ! -path "./.42ctl/*" | sed "s|^\./||" | sort)"
		[ -n "$want" ] && [ "$got" = "$want" ] || { diff <(printf "%s\n" "$want") <(printf "%s\n" "$got"); exit 1; }' \
	_ "$TREE" "$W/dot/restore"
assert_green "the reader restores the tree byte-for-byte" \
	-- bash -c 'qa_actor dot "$2" "env pull --org $ORG --project api --env prod --apply" >/dev/null 2>&1 || exit 1
		files="$(cd "$1" && find . -type f ! -path "./.42ctl/*" | sort)"
		[ -n "$files" ] || exit 1
		while read -r f; do cmp -s "$1/$f" "$2/$f" || { echo "differs or missing: $f"; exit 1; }; done <<<"$files"' \
	_ "$TREE" "$W/dot/restore"
assert_green "the ungranted member restores nothing" \
	-- bash -c 'qa_actor eve "$1" "env pull --org $ORG --project api --env prod --apply" >/dev/null 2>&1
		[ -z "$(find "$1" -type f ! -path "*/.42ctl/*" 2>/dev/null)" ]' _ "$W/eve/restore"
assert_green "the server's stored bytes can be read back for searching" -- qa_dump_server_db "$W/db.dump"
# The marker is the opaque prefix shared-tree entries are stored under, which IS in the clear —
# proof the dump holds the environment's rows before anything is concluded from an absence.
assert_zero_knowledge "no environment secret reached the server in the clear" \
	"s41-writer-update-$N" "__42ctl/f/" "$W/db.dump"
assert_zero_knowledge "no file of the shared tree reached the server in the clear" \
	"qa-db-root-password-0001" "__42ctl/f/" "$W/db.dump"

# ── somebody leaves, and takes nothing with them ─────────────────────────────
assert_green "removing the reader says a rotation is still needed" \
	-- bash -c 'out="$(act ada "org member rm --org $ORG --user $(mail dot)" 2>/dev/null)" || exit 1
		grep -q "removed $(mail dot) from org" <<<"$out" && grep -qi "rotat\|remain readable" <<<"$out"'
refused_as "the removed member can no longer read the organisation" dot "" "org member ls --org $ORG"
assert_green "the owner rotates prod's key" \
	-- bash -c 'act ada "env keys rotate --org $ORG --project api --env prod" >/dev/null 2>&1'
printf '%s\n' "DB_PASSWORD=s41-after-rotation-$N" >"$W/ada/secret.txt"
assert_green "the owner stores a secret after the rotation" \
	-- bash -c 'act ada "env secret set --org $ORG --project api --env prod app/db < /project/secret.txt" >/dev/null 2>&1'
assert_green "the writer, still entitled, reads the new secret" \
	-- bash -c 'act cy "env secret get --org $ORG --project api --env prod app/db" 2>/dev/null | grep -qx "DB_PASSWORD=s41-after-rotation-$N"'
assert_green "the removed member cannot read anything written after the rotation" \
	-- bash -c '! act dot "env secret get --org $ORG --project api --env prod app/db" 2>/dev/null | grep -qF "s41-after-rotation-$N"'
assert_green "a member may always leave on their own" \
	-- bash -c 'act eve "org member rm --org $ORG --user $(mail eve)" 2>/dev/null | grep -qx "removed $(mail eve) from org .$ORG."'
refused_as "and once gone cannot read the roster" eve "" "org member ls --org $ORG"

# ── sessions end when they are told to ───────────────────────────────────────
expect_as "logging out says so" cy "logged out of profile 'default'" "auth logout"
assert_green "and the profile reads as logged out" \
	-- bash -c 'act cy "auth status" 2>/dev/null | grep -qx "profile .default.: logged out"'
refused_as "and the verbs that need a session refuse" cy "" "org member ls --org $ORG"
BEN_OLD="$(cat "$(qa_actor_dir ben)/session.tok")"
export BEN_OLD
expect_as "changing a password says every session was revoked" ben \
	"password changed — every session was revoked, log in again" "auth passwd"
assert_green "the session that existed before the change no longer works at all" \
	-- bash -c '[ "$(qa_code GET /v1/auth/me "$BEN_OLD")" = 401 ]'
refused_as "deleting an account without --yes is refused and names what is lost" fox "yes" "account delete"
assert_green "with --yes the account is gone and its password no longer logs in" \
	-- bash -c 'act fox "account delete --yes" >/dev/null 2>&1 || exit 1
		! act fox "auth login --password --email $(mail fox)" >/dev/null 2>&1'

spec_end
