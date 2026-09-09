#!/usr/bin/env bash
# s34 — the whole question, asked as one scenario.
#
# Can a team keep a project's credentials in the vault, and get them back at the paths they
# came from, with an account, a password, and a role that actually decides what they may do?
#
# Everything is invented here: the people, the organisation, the project, the team. Alice
# founds an org and a project, creates a team, invites Bob and Dave into it, and leaves Carol
# in the organisation but outside the team. The Inception tree — a real compose project with
# its .env and its secrets/ directory — is the payload, because the question is about files
# with paths and modes, not about one string.
#
# The scenario is in four parts and they fail differently, which is the point of asking it
# as one story:
#
#   IDENTITY AND MEMBERSHIP works. Accounts with passwords, orgs, projects, environments,
#   teams, invites and grants that resolve THROUGH a team are all built and green below.
#
#   READ ACCESS works, and is cryptographic rather than advisory: an environment's secrets
#   are sealed to the environment's key, and only a member the authority has authorised gets
#   a wrap of that key. Carol is refused by not being able to decrypt, not by being told no.
#
#   THE FILE TREE does not. `push`/`pull` preserve paths and modes but seal to the pusher's
#   own identity, so a teammate cannot read them at all — measured, not assumed. `set-env` /
#   `get-env` are shared but carry one value under one name, with no manifest, no paths and
#   no modes. Nothing joins the two, so the tree half is red.
#
#   WRITE IS GATED BY A ROLE, and getting there took three findings. Writing was originally
#   authorised by nothing at all, so any account that could reach the port could overwrite any
#   environment. Closing that left read and write indistinguishable, because reading requires a
#   wrap and so a read-only member holds one exactly as a writer does — which needed a
#   granter-signed role inside the wrap. And when THAT landed, the read-only assertion went
#   green while every wrap was still being minted Reader, because the control plane did not
#   report a grant's role: a permission test's negative half is satisfied by a system that
#   refuses everybody, and it reads as extra safety rather than as a fault. The positive
#   control below is the only reason that was caught.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"
source "$QA_LIB_DIR/fixtures.sh"

spec_begin "s34-team-project-access"
qa_require_docker_stack
qa_require_cmd curl
trap 'qa_server_cleanup; [ "${QA_KEEP_SERVER:-0}" = "1" ] || qa_authority_down' EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
assert_green "the authority is listening" -- qa_authority_up
[ "$_qa_regression" -eq 0 ] || {
	spec_end
	exit 1
}

N="$$-$(date +%s)"
W="$QA_RESULTS/s34"
rm -rf "$W"
mkdir -p "$W"
ORG="acme-$N"
PUUID="34343434-5656-4787-89ab-$(printf '%012d' $$)"
TEAM_SLUG="platform"

# ── the people ───────────────────────────────────────────────────────────────
# Four invented accounts, each with its own email and password and its own keystore, so
# these are four genuinely separate people rather than four names for one identity.
for who in alice bob carol dave; do qa_actor_reset "$who"; done
A_MAIL="alice-$N@archicode.codes"; A_PW="alice-pw-$N"
B_MAIL="bob-$N@archicode.codes";   B_PW="bob-pw-$N"
C_MAIL="carol-$N@archicode.codes"; C_PW="carol-pw-$N"
D_MAIL="dave-$N@archicode.codes";  D_PW="dave-pw-$N"

A_ID="$(qa_actor_account alice "$A_MAIL" "$A_PW")"; A_TOK="$(cat "$QA_RESULTS/actors/alice/session.tok")"
B_ID="$(qa_actor_account bob "$B_MAIL" "$B_PW")";   B_TOK="$(cat "$QA_RESULTS/actors/bob/session.tok")"
C_ID="$(qa_actor_account carol "$C_MAIL" "$C_PW")"; C_TOK="$(cat "$QA_RESULTS/actors/carol/session.tok")"
D_ID="$(qa_actor_account dave "$D_MAIL" "$D_PW")";  D_TOK="$(cat "$QA_RESULTS/actors/dave/session.tok")"

assert_green "four people sign up with an email and a password and each gets an account id" \
	-- bash -c 'for id in "$@"; do
			[[ $id =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$ ]] || exit 1
		done' _ "$A_ID" "$B_ID" "$C_ID" "$D_ID"
assert_green "each of them logs in and receives their own distinct session" \
	-- bash -c '[ "${#1}" -ge 20 ] && [ "$1" != "$2" ] && [ "$2" != "$3" ] && [ "$3" != "$4" ]' \
	_ "$A_TOK" "$B_TOK" "$C_TOK" "$D_TOK"
assert_green "a wrong password does not log anybody in" \
	-- bash -c '[ -z "$(qa_login "$1" "definitely-not-the-password")" ]' _ "$A_MAIL"

# ── the organisation, the project, the environment ───────────────────────────
assert_green "alice founds an organisation" \
	-- bash -c '[ "$(qa_code POST /v1/orgs "$1" "{\"slug\":\"$2\",\"name\":\"Acme\"}")" = 201 ]' _ "$A_TOK" "$ORG"
assert_green "alice creates a project inside that organisation" \
	-- bash -c '[ "$(qa_code POST "/v1/orgs/$2/projects" "$1" "{\"id\":\"$3\",\"slug\":\"inception\",\"name\":\"Inception\"}")" = 201 ]' \
	_ "$A_TOK" "$ORG" "$PUUID"
ENV_ID="$(qa_json "$(qa_api POST "/v1/projects/$PUUID/environments" "$A_TOK" '{"name":"prod"}' | cut -f2-)" id)"
assert_green "the project holds an environment" -- bash -c '[ -n "$1" ]' _ "$ENV_ID"

# ── everybody joins the organisation, then the team is formed ────────────────
for pair in "bob:$B_MAIL" "carol:$C_MAIL" "dave:$D_MAIL"; do
	who="${pair%%:*}"; mail="${pair#*:}"
	t="$(qa_json "$(qa_api POST "/v1/orgs/$ORG/invites" "$A_TOK" "{\"email\":\"$mail\",\"role\":\"member\"}" | cut -f2-)" token)"
	qa_api POST /v1/orgs/invites/accept "$(cat "$QA_RESULTS/actors/$who/session.tok")" "{\"token\":\"$t\"}" >/dev/null
done
assert_green "the three invited people are all organisation members" \
	-- bash -c 'r=$(qa_api GET "/v1/orgs/$2/members" "$1")
		for id in "$3" "$4" "$5"; do grep -qF "\"user_id\":\"$id\"" <<<"$r" || exit 1; done' \
	_ "$A_TOK" "$ORG" "$B_ID" "$C_ID" "$D_ID"

TEAM_ID="$(qa_json "$(qa_api POST "/v1/orgs/$ORG/teams" "$A_TOK" "{\"slug\":\"$TEAM_SLUG\",\"name\":\"Platform\"}" | cut -f2-)" id)"
assert_green "alice creates a team in the organisation" -- bash -c '[ -n "$1" ]' _ "$TEAM_ID"
assert_green "bob and dave are added to the team" \
	-- bash -c 'for id in "$3" "$4"; do
			c=$(qa_code POST "/v1/orgs/$2/teams/$5/members" "$1" "{\"user_id\":\"$id\",\"team_role\":\"member\"}")
			case "$c" in 200|201|204) ;; *) printf "adding %s returned %s\n" "$id" "$c"; exit 1 ;; esac
		done' _ "$A_TOK" "$ORG" "$B_ID" "$D_ID" "$TEAM_ID"
assert_green "carol is in the organisation but NOT in the team" \
	-- bash -c '! grep -qF "\"user_id\":\"$3\"" <<<"$(qa_api GET "/v1/orgs/$2/teams" "$1")"' _ "$A_TOK" "$ORG" "$C_ID"

# ── access is granted to the TEAM, not to the people ─────────────────────────
# The point of a team is that membership carries the grant. Nobody names bob or dave here.
assert_green "the team is granted READ access to the project" \
	-- bash -c '[ "$(qa_code POST "/v1/orgs/$2/projects/$3/grants" "$1" \
		"{\"grantee_kind\":\"team\",\"grantee_id\":\"$4\",\"project_role\":\"read\"}")" = 201 ]' \
	_ "$A_TOK" "$ORG" "$PUUID" "$TEAM_ID"
# Bob additionally, and directly, as a writer. Two people in the same team with different
# rights is the only way to tell a role apart from mere membership, and it exercises the rule
# that a member under two grants gets the stronger of them.
assert_green "bob is additionally granted WRITE directly" \
	-- bash -c '[ "$(qa_code POST "/v1/orgs/$2/projects/$3/grants" "$1" \
		"{\"grantee_kind\":\"user\",\"grantee_id\":\"$4\",\"project_role\":\"write\"}")" = 201 ]' \
	_ "$A_TOK" "$ORG" "$PUUID" "$B_ID"

for who in alice bob carol dave; do
	qa_actor "$who" "$W" "keys enroll --org $ORG" >/dev/null 2>&1
done
qa_actor alice "$W" "vault env-init --org $ORG --project $PUUID --env prod" >/dev/null 2>&1
qa_actor alice "$W" "vault sync-keys --org $ORG --project $PUUID --env prod" >/dev/null 2>&1

CANARY='MYSQL_ROOT_PASSWORD=team-only-value-0001'
printf '%s\n' "$CANARY" >"$W/secret.txt"
assert_green "alice stores an environment secret" \
	-- bash -c 'qa_actor alice "$1" "vault set-env --org $2 --project $3 --env prod app/db < /project/secret.txt" >/dev/null 2>&1' \
	_ "$W" "$ORG" "$PUUID"
assert_green "bob reads it because his TEAM was granted access" \
	-- bash -c 'qa_actor bob "$1" "vault get-env --org $2 --project $3 --env prod app/db" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG" "$PUUID" "$CANARY"
assert_green "dave reads it for the same reason" \
	-- bash -c 'qa_actor dave "$1" "vault get-env --org $2 --project $3 --env prod app/db" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG" "$PUUID" "$CANARY"
# Carol is refused by not being able to decrypt, not by being told no. That is the stronger
# kind of refusal: it does not depend on the server choosing to enforce anything.
assert_green "carol is in the organisation and still cannot read the environment" \
	-- bash -c '! qa_actor carol "$1" "vault get-env --org $2 --project $3 --env prod app/db" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG" "$PUUID" "$CANARY"

# ── the file tree, which is the actual question ──────────────────────────────
# A real compose project: srcs/.env plus a secrets/ directory holding a TLS private key.
INC="$W/inception"
mkdir -p "$INC"
cp -r qa/fixtures/inception/. "$INC/" 2>/dev/null || fixture_secrets_tree >/dev/null
[ -d "$INC/secrets" ] || { rm -rf "$INC"; cp -r "$(fixture_secrets_tree)" "$INC"; }
chmod 600 "$INC/secrets/"* 2>/dev/null || true
fixture_project_marker "$INC" "inception-$N" '"*"'
assert_green "the payload is a real tree with a secrets directory" \
	-- bash -c '[ -f "$1/srcs/.env" ] && [ -d "$1/secrets" ] && [ "$(ls "$1/secrets" | wc -l)" -ge 2 ]' _ "$INC"

# push/pull today seal to the pusher's OWN identity. Measured rather than assumed: alice
# pushes with the personal verb, and bob — a granted member of the team — cannot see it.
assert_green "alice can push the tree for herself" \
	-- bash -c 'qa_actor alice "$1" "push" >/dev/null 2>&1' _ "$INC"
mkdir -p "$W/bob-personal"
assert_green "a granted teammate cannot pull a personally-pushed tree — it is sealed to her alone" \
	-- bash -c '! qa_actor bob "$2" "pull --project inception-$3 --apply" >/dev/null 2>&1
		[ -z "$(find "$2" -type f 2>/dev/null)" ]' _ "$INC" "$W/bob-personal" "$N"

# The verb that would answer the question. Named to sit beside set-env / get-env, because it
# is the same sharing model applied to a tree rather than to one value.
assert_green "the CLI exposes an environment-scoped tree push" \
	-- bash -c 'qa_actor alice "$1" "vault --help" 2>&1 | grep -q "push-env"' _ "$W"
assert_green "the CLI exposes an environment-scoped tree pull" \
	-- bash -c 'qa_actor alice "$1" "vault --help" 2>&1 | grep -q "pull-env"' _ "$W"
assert_green "alice pushes the whole tree to the environment" \
	-- bash -c 'qa_actor alice "$1" "vault push-env --org $2 --project $3 --env prod" >/dev/null 2>&1' \
	_ "$INC" "$ORG" "$PUUID"
mkdir -p "$W/bob-tree"
assert_green "bob pulls every file back at the path it came from" \
	-- bash -c 'qa_actor bob "$2" "vault pull-env --org $3 --project $4 --env prod --apply" >/dev/null 2>&1
		cmp -s "$1/srcs/.env" "$2/srcs/.env" && cmp -s "$1/secrets/server.key" "$2/secrets/server.key"' \
	_ "$INC" "$W/bob-tree" "$ORG" "$PUUID"
assert_green "the restored private key keeps its 0600 mode" \
	-- bash -c '[ "$(stat -c %a "$1/secrets/server.key" 2>/dev/null)" = 600 ]' _ "$W/bob-tree"
mkdir -p "$W/carol-tree"
# This one has to prove the mechanism WORKED for somebody in the same breath. Without that,
# "carol got nothing" is satisfied by a command that does not exist, and it passed exactly
# that way the first time it ran.
assert_green "carol, outside the team, gets nothing where a team member got everything" \
	-- bash -c '[ -n "$(find "$4" -type f 2>/dev/null)" ] || { printf "no member got the tree, so a refusal proves nothing\n"; exit 1; }
		qa_actor carol "$1" "vault pull-env --org $2 --project $3 --env prod --apply" >/dev/null 2>&1
		[ -z "$(find "$1" -type f 2>/dev/null)" ]' _ "$W/carol-tree" "$ORG" "$PUUID" "$W/bob-tree"

# The refusal has to track the GRANT, not something incidental. Carol is granted directly and
# re-synced, and the same command that gave her nothing now gives her the tree. Without this,
# "carol got nothing" could be any failure at all wearing the costume of an access decision.
mkdir -p "$W/carol-granted"
assert_green "granting carol directly turns the same refusal into the same tree" \
	-- bash -c 'qa_api POST "/v1/orgs/$2/projects/$3/grants" "$4" \
			"{\"grantee_kind\":\"user\",\"grantee_id\":\"$5\",\"project_role\":\"read\"}" >/dev/null
		qa_actor alice "$6" "vault sync-keys --org $2 --project $3 --env prod" >/dev/null 2>&1
		qa_actor carol "$1" "vault pull-env --org $2 --project $3 --env prod --apply" >/dev/null 2>&1
		cmp -s "$7/srcs/.env" "$1/srcs/.env"' \
	_ "$W/carol-granted" "$ORG" "$PUUID" "$A_TOK" "$C_ID" "$W" "$INC"

# The core promise, on the new path: the server holds the tree and learns neither the bytes
# nor the real paths. The marker is the scope id, which the server legitimately stores as the
# owner of every env secret, so a dump missing it is a dump that searched the wrong thing.
DB34="$W/server.db"
assert_green "the server database can be read back for searching" -- qa_dump_server_db "$DB34"
assert_zero_knowledge "no file content from the tree reached the server" \
	"MYSQL_ROOT_PASSWORD" "__42ctl/f/" "$DB34"
assert_zero_knowledge "no real path from the tree reached the server" \
	"secrets/server.key" "__42ctl/f/" "$DB34"
assert_zero_knowledge "not even the directory name reached it" \
	"srcs/.env" "__42ctl/f/" "$DB34"

# ── what a role is allowed to DO ─────────────────────────────────────────────
# Reading is gated by the seal and that half is real. Writing is gated by nothing: the server
# checks only that an envelope is authored by whoever sent it, so anyone who can reach the
# port can overwrite an environment's secrets. Both assertions below are written as the
# behaviour an operator is entitled to, and both are red because the attack succeeds today.
POISON='MYSQL_ROOT_PASSWORD=overwritten-by-somebody-who-should-not'
printf '%s\n' "$POISON" >"$W/poison.txt"

# Each attack gets its OWN path. The first version shared one, so when the read-only write
# succeeded it destroyed the value the outsider assertion then checked — and the outsider
# assertion went red while the outsider had been correctly refused. An attack that succeeds
# must not be able to make the next assertion lie about a different attack.
OUTSIDER_CANARY='MYSQL_ROOT_PASSWORD=outsider-target-0003'
printf '%s\n' "$OUTSIDER_CANARY" >"$W/outsider.txt"
assert_green "alice stores a second secret for the outsider test to aim at" \
	-- bash -c 'qa_actor alice "$1" "vault set-env --org $2 --project $3 --env prod app/other < /project/outsider.txt" >/dev/null 2>&1' \
	_ "$W" "$ORG" "$PUUID"

assert_green "somebody outside the organisation cannot overwrite an environment secret" \
	-- bash -c 'qa_actor_reset outsider >/dev/null
		qa_actor_account outsider "out-$4@archicode.codes" "out-pw-$4" >/dev/null
		qa_actor outsider "$1" "vault set-env --org $2 --project $3 --env prod app/other < /project/poison.txt" >/dev/null 2>&1 && { printf "the outsider write SUCCEEDED\n"; exit 1; }
		qa_actor alice "$1" "vault get-env --org $2 --project $3 --env prod app/other" 2>/dev/null | grep -qF "$5"' \
	_ "$W" "$ORG" "$PUUID" "$N" "$OUTSIDER_CANARY"

# Still red, and it should stay red rather than be softened. Reading an environment requires
# a wrap, so a read-only member holds one exactly as a writer does, and membership cannot
# tell them apart. Separating them needs a granter-signed ROLE inside the wrap, which is a
# change across the crypto core, this client and the server — not a missing check.
# THE CONTROL, and it was missing. Without it "a reader cannot write" is satisfied by a system
# in which NOBODY can write — which is what was happening: the control plane does not return a
# grant's role in its listing, so every wrap was minted Reader and the read-only assertion
# passed for a reason with nothing to do with roles.
BOBS_WRITE='MYSQL_ROOT_PASSWORD=written-by-a-writer-0004'
printf '%s\n' "$BOBS_WRITE" >"$W/bobs.txt"
assert_green "a member granted write CAN overwrite an environment secret" \
	-- bash -c 'qa_actor bob "$1" "vault set-env --org $2 --project $3 --env prod app/db < /project/bobs.txt" >/dev/null 2>&1 || exit 1
		qa_actor alice "$1" "vault get-env --org $2 --project $3 --env prod app/db" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG" "$PUUID" "$BOBS_WRITE"
# This one has to prove a writer CAN write in the same breath, or it is satisfied by a system
# where nobody can — and that is not a hypothetical, it is the state this spec found. A green
# here while the assertion above is red would be the most misleading result in the battery:
# "roles are enforced" reported by a vault that refuses everybody.
assert_green "a member with only read access cannot overwrite an environment secret" \
	-- bash -c 'qa_actor alice "$1" "vault get-env --org $2 --project $3 --env prod app/db" 2>/dev/null | grep -qF "$5" ||
			{ printf "no writer has written, so a reader being refused proves nothing\n"; exit 1; }
		qa_actor dave "$1" "vault set-env --org $2 --project $3 --env prod app/db < /project/poison.txt" >/dev/null 2>&1 && exit 1
		qa_actor alice "$1" "vault get-env --org $2 --project $3 --env prod app/db" 2>/dev/null | grep -qF "$5"' \
	_ "$W" "$ORG" "$PUUID" "$CANARY" "$BOBS_WRITE"

# ── leaving the team ends the access ─────────────────────────────────────────
# DAVE is the one removed, not bob. Bob now holds a direct grant as well as the team's, so
# taking him off the team would leave him authorised and the assertion would be measuring the
# direct grant rather than the removal. Dave holds nothing but the team's grant, so he is the
# only one whose access the team decides.
#
# No route lists a team's members, so membership is asserted through its EFFECT: dave reads
# before, and does not read after removal plus rotation. That is the property anyway — a
# member list agreeing while the keys disagree would be the worse outcome.
assert_green "dave reads the environment while he is on the team" \
	-- bash -c 'qa_actor dave "$1" "vault get-env --org $2 --project $3 --env prod app/db" 2>/dev/null | grep -q MYSQL_ROOT_PASSWORD' \
	_ "$W" "$ORG" "$PUUID"
assert_green "dave is removed from the team" \
	-- bash -c 'c=$(qa_code DELETE "/v1/orgs/$2/teams/$4/members/$3" "$1")
		case "$c" in 200|204) ;; *) printf "removal returned %s\n" "$c"; exit 1 ;; esac' \
	_ "$A_TOK" "$ORG" "$D_ID" "$TEAM_ID"

# Removal alone cannot un-tell somebody a secret they already hold; rotation closes it.
assert_green "after rotation the removed member cannot read the new secret" \
	-- bash -c 'rot=$(qa_actor alice "$1" "vault rotate-scope --org $2 --project $3 --env prod" 2>&1) || { printf "rotation itself failed:\n%s\n" "$rot"; exit 1; }
		printf "MYSQL_ROOT_PASSWORD=rotated-value-0002\n" >"$1/rotated.txt"
		w=$(qa_actor alice "$1" "vault set-env --org $2 --project $3 --env prod app/db < /project/rotated.txt" 2>&1) || { printf "the write after rotation failed:\n%s\n" "$w"; exit 1; }
		out=$(qa_actor dave "$1" "vault get-env --org $2 --project $3 --env prod app/db" 2>&1)
		grep -q "rotated-value-0002" <<<"$out" && { printf "the removed member STILL READS it\n"; exit 1; }
		exit 0' \
	_ "$W" "$ORG" "$PUUID"
assert_green "bob, who holds a grant of his own, still reads it" \
	-- bash -c 'qa_actor alice "$1" "vault sync-keys --org $2 --project $3 --env prod" >/dev/null 2>&1
		qa_actor bob "$1" "vault get-env --org $2 --project $3 --env prod app/db" 2>/dev/null | grep -q "rotated-value-0002"' \
	_ "$W" "$ORG" "$PUUID"

spec_end
