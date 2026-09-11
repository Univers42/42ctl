#!/usr/bin/env bash
# s37 — a whole company, and every pair of people who must not reach each other.
#
# The other scenarios prove a mechanism with the smallest cast that can show it. This one asks
# whether the mechanisms still hold when there are enough people, teams, projects and
# environments for them to interfere. Most access-control bugs are not in one rule; they are in
# two rules meeting, and two rules cannot meet with three users.
#
# The cast: two organisations that must never see each other, three teams inside the first with
# different rights, an environment per project, a contractor on one environment only, and a
# person who belongs to two teams whose rights differ.
#
# The assertions are mostly NEGATIVE, and each negative is paired with the positive that gives
# it meaning. "X cannot read Y" is satisfied by a vault where nobody can read anything, which
# is the failure this battery has already produced once — a permission test's negative half
# reads as extra safety exactly when it is worthless.
#
# Volume is part of the question rather than a separate spec: an environment carrying a real
# tree with a hundred files must reconcile as exactly as one carrying two, and a person denied
# an environment must be denied all of it rather than most of it.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"
source "$QA_LIB_DIR/fixtures.sh"

spec_begin "s37-dense-organisation"
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
W="$QA_RESULTS/s37"
rm -rf "$W"
mkdir -p "$W"
ACME="acme-$N"
RIVAL="rival-$N"
P_WEB="37373737-1111-4222-8333-$(qa_uuid_tail)"
P_API="37373737-4444-4555-8666-$(qa_uuid_tail)"
P_RIVAL="37373737-7777-4888-8999-$(qa_uuid_tail)"

# ── the cast ─────────────────────────────────────────────────────────────────
# ida founds acme. platform writes, support reads, contractors get one environment.
# quinn is in two teams whose rights differ, which is where a "strongest wins" rule is
# either right or quietly wrong. zoe founds a rival company and shares nothing.
PEOPLE="ida jon kim leo mia nils olga pia quinn rui zoe"
for who in $PEOPLE; do qa_actor_reset "$who"; done
declare -A ID TOK
for who in $PEOPLE; do
	ID[$who]="$(qa_actor_account "$who" "$who-$N@archicode.codes" "pw-$who-$N")"
	TOK[$who]="$(cat "$(qa_actor_dir "$who")/session.tok")"
done
assert_green "eleven people hold eleven distinct accounts" \
	-- bash -c 'printf "%s\n" "$@" | sort -u | wc -l | grep -qx 11' _ "${ID[@]}"

qa_api POST /v1/orgs "${TOK[ida]}" "{\"slug\":\"$ACME\",\"name\":\"Acme\"}" >/dev/null
qa_api POST /v1/orgs "${TOK[zoe]}" "{\"slug\":\"$RIVAL\",\"name\":\"Rival\"}" >/dev/null
assert_green "two organisations exist and each has its own owner" \
	-- bash -c 'a=$(qa_api GET "/v1/orgs/$3/members" "$1"); b=$(qa_api GET "/v1/orgs/$4/members" "$2")
		grep -q owner <<<"$a" && grep -q owner <<<"$b" && [ "$a" != "$b" ]' \
	_ "${TOK[ida]}" "${TOK[zoe]}" "$ACME" "$RIVAL"

for who in jon kim leo mia nils olga pia quinn rui; do
	t="$(qa_json "$(qa_api POST "/v1/orgs/$ACME/invites" "${TOK[ida]}" "{\"email\":\"$who-$N@archicode.codes\",\"role\":\"member\"}" | cut -f2-)" token)"
	qa_api POST /v1/orgs/invites/accept "${TOK[$who]}" "{\"token\":\"$t\"}" >/dev/null
done
assert_green "nine people join acme and the rival's owner does not" \
	-- bash -c 'r=$(qa_api GET "/v1/orgs/$2/members" "$1")
		[ "$(grep -o "user_id" <<<"$r" | wc -l)" -ge 10 ] || { printf "only %s members\n" "$(grep -o user_id <<<"$r" | wc -l)"; exit 1; }
		! grep -qF "$3" <<<"$r"' _ "${TOK[ida]}" "$ACME" "${ID[zoe]}"

# ── projects, environments, teams ────────────────────────────────────────────
qa_api POST "/v1/orgs/$ACME/projects" "${TOK[ida]}" "{\"id\":\"$P_WEB\",\"slug\":\"web\",\"name\":\"Web\"}" >/dev/null
qa_api POST "/v1/orgs/$ACME/projects" "${TOK[ida]}" "{\"id\":\"$P_API\",\"slug\":\"api\",\"name\":\"Api\"}" >/dev/null
qa_api POST "/v1/orgs/$RIVAL/projects" "${TOK[zoe]}" "{\"id\":\"$P_RIVAL\",\"slug\":\"secret\",\"name\":\"Secret\"}" >/dev/null
for pair in "$P_WEB:prod" "$P_WEB:staging" "$P_API:prod" "$P_RIVAL:prod"; do
	qa_api POST "/v1/projects/${pair%%:*}/environments" "${TOK[ida]}" "{\"name\":\"${pair#*:}\"}" >/dev/null 2>&1
	qa_api POST "/v1/projects/${pair%%:*}/environments" "${TOK[zoe]}" "{\"name\":\"${pair#*:}\"}" >/dev/null 2>&1
done
WEB_PROD="$(qa_json "$(qa_api GET "/v1/projects/$P_WEB/environments" "${TOK[ida]}" | cut -f2-)" id)"
assert_green "the projects carry their environments" -- bash -c '[ -n "$1" ]' _ "$WEB_PROD"

PLATFORM="$(qa_json "$(qa_api POST "/v1/orgs/$ACME/teams" "${TOK[ida]}" '{"slug":"platform","name":"Platform"}' | cut -f2-)" id)"
SUPPORT="$(qa_json "$(qa_api POST "/v1/orgs/$ACME/teams" "${TOK[ida]}" '{"slug":"support","name":"Support"}' | cut -f2-)" id)"
OUTSIDE="$(qa_json "$(qa_api POST "/v1/orgs/$ACME/teams" "${TOK[ida]}" '{"slug":"outside","name":"Outside"}' | cut -f2-)" id)"
assert_green "three teams exist and are distinct" \
	-- bash -c '[ -n "$1" ] && [ -n "$2" ] && [ -n "$3" ] && [ "$1" != "$2" ] && [ "$2" != "$3" ]' \
	_ "$PLATFORM" "$SUPPORT" "$OUTSIDE"

# The body is built with plain quoting. `${2@Q}` produces shell quoting, not JSON quoting, so
# it emitted 'id' instead of "id" — invalid JSON that every add silently failed on, leaving
# three empty teams and a spec that looked like a permission bug.
add_to_team() {
	local code
	code="$(qa_code POST "/v1/orgs/$ACME/teams/$1/members" "${TOK[ida]}" "{\"user_id\":\"$2\",\"team_role\":\"member\"}")"
	case "$code" in 200 | 201 | 204) ;; *) printf '%s -> %s\n' "$2" "$code" >>"$W/team-add-failures" ;; esac
}
: >"$W/team-add-failures"
for who in jon kim quinn; do add_to_team "$PLATFORM" "${ID[$who]}"; done
for who in leo mia quinn; do add_to_team "$SUPPORT" "${ID[$who]}"; done
for who in nils; do add_to_team "$OUTSIDE" "${ID[$who]}"; done

# Every add must have been accepted, or the teams are empty and every denial below passes
# because nobody was ever in anything. This is the assertion that would have caught the
# quoting bug immediately instead of letting it surface as a permission error seven
# assertions later.
assert_green "every team membership was accepted" \
	-- bash -c '[ ! -s "$1/team-add-failures" ] || { cat "$1/team-add-failures"; exit 1; }' _ "$W"
# olga, pia and rui belong to no team at all: the org-member-with-nothing case.

qa_api POST "/v1/orgs/$ACME/projects/$P_WEB/grants" "${TOK[ida]}" \
	"{\"grantee_kind\":\"team\",\"grantee_id\":\"$PLATFORM\",\"project_role\":\"write\"}" >/dev/null
qa_api POST "/v1/orgs/$ACME/projects/$P_WEB/grants" "${TOK[ida]}" \
	"{\"grantee_kind\":\"team\",\"grantee_id\":\"$SUPPORT\",\"project_role\":\"read\"}" >/dev/null
qa_api POST "/v1/orgs/$ACME/projects/$P_API/grants" "${TOK[ida]}" \
	"{\"grantee_kind\":\"team\",\"grantee_id\":\"$OUTSIDE\",\"project_role\":\"read\"}" >/dev/null

for who in ida jon kim leo mia nils olga pia quinn rui; do
	qa_actor "$who" "$W" "keys enroll --org $ACME" >/dev/null 2>&1
done
qa_actor zoe "$W" "keys enroll --org $RIVAL" >/dev/null 2>&1
qa_actor ida "$W" "vault env-init --org $ACME --project $P_WEB --env prod" >/dev/null 2>&1
qa_actor ida "$W" "vault sync-keys --org $ACME --project $P_WEB --env prod" >/dev/null 2>&1
qa_actor zoe "$W" "vault env-init --org $RIVAL --project $P_RIVAL --env prod" >/dev/null 2>&1
qa_actor zoe "$W" "vault sync-keys --org $RIVAL --project $P_RIVAL --env prod" >/dev/null 2>&1

# ── volume: a real tree, not a single value ──────────────────────────────────
# A hundred files across ten directories, plus a secrets directory with a private key. The
# question is whether an environment carrying a real project reconciles as exactly as one
# carrying a toy, and whether a denial covers all of it rather than most of it.
SRC="$W/web"
mkdir -p "$SRC/secrets"
for d in $(seq 1 10); do
	mkdir -p "$SRC/svc$d"
	for f in $(seq 1 10); do
		printf 'SERVICE=svc%s\nSLOT=%s\nSECRET_VALUE=acme-web-%s-%s-0001\n' "$d" "$f" "$d" "$f" >"$SRC/svc$d/.env.$f"
	done
done
printf 'ACME_TLS_PRIVATE_KEY=acme-web-tls-key-0001\n' >"$SRC/secrets/server.key"
chmod 600 "$SRC/secrets/server.key"
fixture_project_marker "$SRC" "web-$N" '"*"'
assert_green "the payload is a hundred files across ten directories plus a key" \
	-- bash -c '[ "$(find "$1" -type f -name ".env.*" | wc -l)" -eq 100 ] && [ -f "$1/secrets/server.key" ]' _ "$SRC"
# A file too large for one envelope goes into the SAME tree. The personal path chunks such a
# file to object storage; the shared path was built without it, so an archive or a database
# dump — the thing a team most wants to share — was the one thing it could not carry.
#
# Same tree rather than a second one, because an environment holds ONE tree: pushing a second
# project into it replaces the first, which is coherent and is also how the first version of
# this measured a permission failure that was really two pushes fighting over one manifest.
assert_green "an object store is available for the large payload" -- qa_s3_up
for who in jon kim mia nils olga rui zoe quinn; do
	qa_actor "$who" "$W" "config endpoint --blobstore $(qa_s3_internal) --bucket $QA_S3_BUCKET" >/dev/null 2>&1
done
fixture_varied_file "$SRC/volume.bin" 8 "QA42-S37-TEAM-VOLUME"
assert_green "the tree also holds a file over the transport ceiling" \
	-- bash -c '[ "$(stat -c %s "$1/volume.bin")" -gt 4194304 ]' _ "$SRC"
assert_green "a writer publishes the whole tree, large file included, in one push" \
	-- bash -c 'out=$(qa_actor jon "$1" "vault push-env --org $2 --project $3 --env prod" 2>&1) || { printf "%s\n" "$out" | tail -4; exit 1; }
		grep -q "chunk(s) in the object store" <<<"$out"' _ "$SRC" "$ACME" "$P_WEB"

# ── who may read it, and who may not ─────────────────────────────────────────
pull_as() {
	local who="$1" dest="$W/pull-$1"
	rm -rf "$dest"; mkdir -p "$dest"
	qa_actor "$who" "$dest" "vault pull-env --org $ACME --project $P_WEB --env prod --apply" >/dev/null 2>&1
	printf '%s' "$dest"
}
export -f pull_as 2>/dev/null || true

assert_green "a platform member gets every one of the hundred files, byte-exact" \
	-- bash -c 'd="$2/pull-kim"; rm -rf "$d"; mkdir -p "$d"
		qa_actor kim "$d" "vault pull-env --org $3 --project $4 --env prod --apply" >/dev/null 2>&1
		n=$(find "$d" -type f -name ".env.*" | wc -l)
		[ "$n" -eq 100 ] || { printf "restored %s of 100\n" "$n"; exit 1; }
		bad=0
		while IFS= read -r f; do cmp -s "$f" "$d/${f#"$1"/}" || bad=1; done < <(find "$1" -type f -name ".env.*")
		[ "$bad" -eq 0 ]' _ "$SRC" "$W" "$ACME" "$P_WEB"
assert_green "the same pull brings the large file back byte-exact" \
	-- bash -c 'cmp -s "$1/volume.bin" "$2/pull-kim/volume.bin"' _ "$SRC" "$W"
assert_green "the private key comes back owner-only" \
	-- bash -c 'm=$(stat -c %a "$2/pull-kim/secrets/server.key" 2>/dev/null) || exit 1
		[ $((0$m & 0077)) -eq 0 ]' _ "$SRC" "$W"
assert_green "a support member reads it too, because their team was granted read" \
	-- bash -c 'd="$1/pull-mia"; rm -rf "$d"; mkdir -p "$d"
		qa_actor mia "$d" "vault pull-env --org $2 --project $3 --env prod --apply" >/dev/null 2>&1
		[ "$(find "$d" -type f -name ".env.*" | wc -l)" -eq 100 ]' _ "$W" "$ACME" "$P_WEB"

# Every negative below is only meaningful because the two positives above passed.
for who in nils olga rui zoe; do
	assert_green "$who reaches none of the hundred files" \
		-- bash -c 'd="$1/deny-$2"; rm -rf "$d"; mkdir -p "$d"
			qa_actor "$2" "$d" "vault pull-env --org $3 --project $4 --env prod --apply" >/dev/null 2>&1
			n=$(find "$d" -type f | wc -l)
			[ "$n" -eq 0 ] || { printf "%s restored %s file(s)\n" "$2" "$n"; exit 1; }' \
		_ "$W" "$who" "$ACME" "$P_WEB"
done

# ── two members, the same bytes, one copy ───────────────────────────────────
# Chunk names are a keyed hash under a key derived from the ENVIRONMENT's secret, which every
# member holds. So two people pushing the same archive compute the same names and the second
# stores nothing — the deduplication a team actually wants, and it needs no convergent
# ciphertext: identical names plus the already-present check mean the first copy is the only
# copy, and any member can open it because it is sealed to the environment rather than to a
# person.
assert_green "a second writer pushing the same bytes stores no second copy" \
	-- bash -c 'before=$(qa_s3_count)
		cp -r "$1" "$2/quinn-copy"
		out=$(qa_actor quinn "$2/quinn-copy" "vault push-env --org $3 --project $4 --env prod" 2>&1) || { printf "%s\n" "$out" | tail -3; exit 1; }
		after=$(qa_s3_count)
		[ "$before" -gt 0 ] || { printf "nothing was stored to begin with\n"; exit 1; }
		[ "$after" -eq "$before" ] || { printf "the store grew from %s to %s\n" "$before" "$after"; exit 1; }' \
	_ "$SRC" "$W" "$ACME" "$P_WEB"
assert_green "and the first writer can still read what the second pushed" \
	-- bash -c 'd="$1/pull-after-dedup"; rm -rf "$d"; mkdir -p "$d"
		out=$(qa_actor jon "$d" "vault pull-env --org $2 --project $3 --env prod --apply" 2>&1) || { printf "%s\n" "$out" | tail -3; exit 1; }
		[ -f "$d/volume.bin" ] || { printf "no volume.bin restored; got: %s\n" "$(ls -A "$d" | tr "\n" " ")"; exit 1; }
		cmp -s "$4/volume.bin" "$d/volume.bin"' _ "$W" "$ACME" "$P_WEB" "$SRC"

# ── the person in two teams ──────────────────────────────────────────────────
# quinn is in platform (write) and support (read). A rule that took the last grant read, or
# the weakest, would make quinn a reader — and nothing else in the system would look wrong.
QUINN_WRITE='SERVICE=svc1
SLOT=1
SECRET_VALUE=written-by-quinn-0002'
printf '%s\n' "$QUINN_WRITE" >"$W/quinn.txt"
assert_green "somebody in two teams gets the stronger of their rights, not the weaker" \
	-- bash -c 'qa_actor quinn "$1" "vault set-env --org $2 --project $3 --env prod svc1/slot1 < /project/quinn.txt" >/dev/null 2>&1 || exit 1
		qa_actor ida "$1" "vault get-env --org $2 --project $3 --env prod svc1/slot1" 2>/dev/null | grep -q "written-by-quinn-0002"' \
	_ "$W" "$ACME" "$P_WEB"
assert_green "a reader in one team only still cannot write" \
	-- bash -c 'qa_actor ida "$1" "vault get-env --org $2 --project $3 --env prod svc1/slot1" 2>/dev/null | grep -q "written-by-quinn-0002" ||
			{ printf "no writer has written, so a reader being refused proves nothing\n"; exit 1; }
		qa_actor leo "$1" "vault set-env --org $2 --project $3 --env prod svc1/slot1 < /project/quinn.txt" >/dev/null 2>&1 && exit 1
		exit 0' _ "$W" "$ACME" "$P_WEB"

# ── the other company ────────────────────────────────────────────────────────
# Two organisations, two environments, and no relationship. The interesting direction is not
# that zoe cannot read acme's secrets — it is that acme's OWNER cannot read zoe's.
printf 'RIVAL_SECRET=rival-only-value-0003\n' >"$W/rival.txt"
assert_green "the rival stores a secret of its own" \
	-- bash -c 'qa_actor zoe "$1" "vault set-env --org $2 --project $3 --env prod app/x < /project/rival.txt" >/dev/null 2>&1' \
	_ "$W" "$RIVAL" "$P_RIVAL"
assert_green "the rival can read it back, which is what makes the refusals below mean something" \
	-- bash -c 'qa_actor zoe "$1" "vault get-env --org $2 --project $3 --env prod app/x" 2>/dev/null | grep -q rival-only-value-0003' \
	_ "$W" "$RIVAL" "$P_RIVAL"
for who in ida jon quinn; do
	assert_green "acme's $who cannot read the rival's environment" \
		-- bash -c '! qa_actor "$2" "$1" "vault get-env --org $3 --project $4 --env prod app/x" 2>/dev/null | grep -q rival-only-value-0003' \
		_ "$W" "$who" "$RIVAL" "$P_RIVAL"
done
assert_green "and the rival reaches none of acme's hundred files" \
	-- bash -c 'd="$1/deny-zoe-web"; rm -rf "$d"; mkdir -p "$d"
		qa_actor zoe "$d" "vault pull-env --org $2 --project $3 --env prod --apply" >/dev/null 2>&1
		[ "$(find "$d" -type f | wc -l)" -eq 0 ]' _ "$W" "$ACME" "$P_WEB"

# ── the CLI's own rendering of all this ─────────────────────────────────────
# Every assertion above reads the AUTHORITY. None of them read what 42ctl makes of the
# answer, and that is where `org members` was broken for weeks: the authority emits every
# timestamp as an integer, the client declared one a string, and the whole listing failed to
# decode. Membership was intact and the verb reported nothing — a client-side decode error
# wearing the costume of an empty organisation.
#
# A suite that only ever drives the API cannot see that, and mine only ever drove the API.
assert_green "the CLI lists the organisation's members, not just the API" \
	-- bash -c 'out=$(qa_actor ida "$1" "org members --org $2" 2>&1) || { printf "%s\n" "$out" | tail -3; exit 1; }
		grep -qF "$3" <<<"$out" || { printf "the listing does not name a known member:\n%s\n" "$out"; exit 1; }' \
	_ "$W" "$ACME" "${ID[jon]}"
assert_green "the CLI lists the organisation's teams" \
	-- bash -c 'out=$(qa_actor ida "$1" "team list --org $2" 2>&1) || { printf "%s\n" "$out" | tail -3; exit 1; }
		grep -q "platform" <<<"$out"' _ "$W" "$ACME"
assert_green "the CLI lists the project's environments" \
	-- bash -c 'out=$(qa_actor ida "$1" "env list --project $2" 2>&1) || { printf "%s\n" "$out" | tail -3; exit 1; }
		grep -q "prod" <<<"$out"' _ "$W" "$P_WEB"

# ── the server learns nothing from any of it ─────────────────────────────────
DB37="$W/server.db"
assert_green "the server database can be read back for searching" -- qa_dump_server_db "$DB37"
assert_zero_knowledge "no value from the hundred files reached the server" \
	"acme-web-5-5-0001" "__42ctl/f/" "$DB37"
assert_zero_knowledge "no service directory name reached the server" \
	"svc7/.env.3" "__42ctl/f/" "$DB37"
assert_zero_knowledge "the rival's secret is not in there either" \
	"rival-only-value-0003" "__42ctl/f/" "$DB37"

spec_end
