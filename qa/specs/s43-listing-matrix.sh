#!/usr/bin/env bash
# s43 — every listing the vault and the authority answer, in every shape its output flags offer.
#
# Other specs check `--format`, `-q` and `--filter` where a scenario needs them, which left most
# combinations on most listings untried. This builds one small world in which every listing has
# at least two rows that differ, then runs `listing_matrix` (qa/lib/listing.sh) on each: the
# unshaped output, json, a bare template, `{{json .}}`, a `table` template, -q, --filter exact,
# in another case, two at once, on a label and under -q, a filter that keeps nothing, a key that
# is not a column, and -q against --format — each checked against the same json rows.
#
# The listing set is read from `help commands`, not written down alone: a listing added later
# with no row in LISTINGS below turns this spec red. The cloud listings run the same matrix in
# s40, against the flyctl stand-in.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"
source "$QA_LIB_DIR/fixtures.sh"
source "$QA_LIB_DIR/listing.sh"

spec_begin "s43-listing-matrix"
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
W="$QA_RESULTS/s43"
rm -rf "$W"
ORG="matrix-$N"
for who in mia noa; do
	qa_actor_reset "$who"
	mkdir -p "$W/$who"
done
export N W ORG

# as <who> <42ctl args as one string>
as() {
	local who="$1"
	shift
	QA_ACCOUNT_PASSWORD="pw-$who-$N" qa_actor "$who" "$W/$who" "$*"
}
mia_line() { as mia "$1"; }
mail() { printf '%s-%s@archicode.codes' "$1" "$N"; }
field() { awk -v k="$1" '$1 == k { print $2; exit }'; }
export -f as mail field

# ── the world: two people, and two differing rows in every listing ───────────
for who in mia noa; do
	assert_green "$who signs up and logs in" \
		-- bash -c 'as "$1" "auth signup --email $(mail "$1")" >/dev/null 2>&1 &&
			as "$1" "auth login --password --email $(mail "$1")" >/dev/null 2>&1' _ "$who"
done

printf 'value-a-%s' "$N" >"$W/mia/a.txt"
printf 'value-b-%s' "$N" >"$W/mia/b.txt"
printf '# runbook %s\n' "$N" >"$W/mia/runbook.txt"
fixture_project_marker "$W/mia" "s43-notes-$N" '"*.env*"'
assert_green "mia keeps three personal secrets, one rotated to a second version" \
	-- bash -c 'as mia "vault set app/alpha --file /project/a.txt" >/dev/null 2>&1 &&
		as mia "vault set app/beta --file /project/b.txt" >/dev/null 2>&1 &&
		as mia "vault set ops/gamma --file /project/a.txt" >/dev/null 2>&1 &&
		as mia "vault rotate app/alpha" >/dev/null 2>&1'
assert_green "and two notes in mia's project" \
	-- bash -c 'as mia "note add runbook.md --file /project/runbook.txt" >/dev/null 2>&1 &&
		as mia "note add oncall.md --file /project/a.txt" >/dev/null 2>&1'

assert_green "mia founds an organisation" -- bash -c 'as mia "org create --slug $ORG --name Matrix" >/dev/null 2>&1'
INVITE="$(as mia "org invite --org $ORG --email $(mail noa) --role member" 2>/dev/null | field token)"
export INVITE
assert_green "noa is invited and joins" -- bash -c '[ -n "$INVITE" ] && as noa "invite accept --token $INVITE" >/dev/null 2>&1'
assert_green "two teams" \
	-- bash -c 'as mia "team create --org $ORG --slug writers --name Writers" >/dev/null 2>&1 &&
		as mia "team create --org $ORG --slug readers --name Readers" >/dev/null 2>&1'
PROJECT="$(as mia "project create --org $ORG --slug api --name API" 2>/dev/null | field id)"
export PROJECT
assert_green "two projects" \
	-- bash -c '[ -n "$PROJECT" ] && as mia "project create --org $ORG --slug web --name Web" >/dev/null 2>&1'
assert_green "two environments" \
	-- bash -c 'as mia "env create --project $PROJECT --name prod" >/dev/null 2>&1 &&
		as mia "env create --project $PROJECT --name staging" >/dev/null 2>&1'
assert_green "two grants of different kinds: mia's team writes prod, noa reads it" \
	-- bash -c 'as mia "team member add --org $ORG --team writers --user $(mail mia)" >/dev/null 2>&1 &&
		as mia "team grant --org $ORG --team writers --project $PROJECT --role write --env prod" >/dev/null 2>&1 &&
		as mia "project grant add --org $ORG --project $PROJECT --user $(mail noa) --role read --env prod" >/dev/null 2>&1'
assert_green "both publish their keys, and prod's key reaches both" \
	-- bash -c 'as mia "keys enroll --org $ORG" >/dev/null 2>&1 && as noa "keys enroll --org $ORG" >/dev/null 2>&1 &&
		as mia "env init --org $ORG --project api --env prod" >/dev/null 2>&1 &&
		as mia "env keys sync --org $ORG --project api --env prod" >/dev/null 2>&1'
TREE="$W/mia/tree"
cp -r "$(fixture_secrets_tree)" "$TREE"
fixture_project_marker "$TREE" "s43-tree-$N" '"*"'
assert_green "a labelled tree of several files is pushed to prod" \
	-- bash -c 'qa_actor mia "$1" "env push --org $ORG --project api --env prod --label tier=web --label owner=mia" >/dev/null 2>&1' \
	_ "$TREE"

# ── the matrix ───────────────────────────────────────────────────────────────
declare -A LISTINGS=(
	["vault ls"]="vault ls"
	["db ls"]="db ls"
	["note ls"]="note ls"
	["org member ls"]="org member ls --org $ORG"
	["team ls"]="team ls --org $ORG"
	["project ls"]="project ls --org $ORG"
	["env ls"]="env ls --project $PROJECT"
	["project grant ls"]="project grant ls --org $ORG --project $PROJECT"
	["env keys ls"]="env keys ls --org $ORG --project api --env prod"
	["env files"]="env files --org $ORG --project api --env prod"
)
as mia "help commands" >"$W/commands.txt" 2>/dev/null
assert_green "the matrix covers exactly the non-cloud listings help commands knows" \
	-- bash -c 'want="$1"; got="$(printf "%s\n" "${@:2}" | sort)"
		[ -n "$want" ] && [ "$want" = "$got" ] || { diff <(printf "%s\n" "$want") <(printf "%s\n" "$got"); exit 1; }' \
	_ "$(listing_commands "$W/commands.txt" | grep -v "^cloud ")" "${!LISTINGS[@]}"

export LISTING_MIN_ROWS=2
for name in $(printf '%s\n' "${!LISTINGS[@]}" | sort | tr ' ' '+'); do
	name="${name//+/ }"
	case "$name" in
	"vault ls" | "db ls") export LISTING_DEFAULT=tsv ;;
	*) unset LISTING_DEFAULT ;;
	esac
	listing_matrix "$name" mia_line "${LISTINGS[$name]}"
done
unset LISTING_MIN_ROWS LISTING_DEFAULT

# ── the composition the listings exist for, on the data that matters most ────
# `vault rm $(vault ls -q)` is the idiom `vault rm --help` teaches. The vault also holds 42ctl's
# own records — the notes above, a push's manifest and chunk lists, under `__42ctl/` — and a
# listing that hands those to rm deletes a person's notes and pushed trees along with the
# secrets they meant to clear.
assert_green "the owner's notes read back before anything is removed" \
	-- bash -c '[ "$(as mia "note get runbook.md" 2>/dev/null)" = "# runbook $N" ]'
assert_green "vault ls lists the secrets a person stored, not 42ctl's own records" \
	-- bash -c '[ "$(as mia "vault ls -q" 2>/dev/null | sort | tr "\n" " ")" = "app/alpha app/beta ops/gamma " ]'
assert_green "and --all shows the records too, so nothing is hidden from somebody who asks" \
	-- bash -c 'as mia "vault ls --all -q" 2>/dev/null | grep -q "^__42ctl/" &&
		[ "$(as mia "vault ls --all -q" 2>/dev/null | grep -vc "^__42ctl/")" -eq 3 ]'
assert_green "vault export writes the secrets as KEY=value and none of 42ctl's records" \
	-- bash -c 'out="$(as mia "vault export" 2>/dev/null)" || exit 1
		[ "$(cut -d= -f1 <<<"$out" | sort | tr "\n" " ")" = "alpha beta gamma " ] || { printf "%s\n" "$out"; exit 1; }'
assert_green "removing one of 42ctl's own records by name is refused, saying what it holds" \
	-- bash -c 'record="$(as mia "vault ls --all -q" 2>/dev/null | grep "^__42ctl/" | head -1)"; [ -n "$record" ] || exit 1
		out="$(as mia "vault rm $record" 2>&1)" && { echo "removed: $out"; exit 1; }
		grep -q "42ctl.s own" <<<"$out"'
assert_green "vault rm \$(vault ls -q) clears every secret and leaves the notes readable" \
	-- bash -c 'ids="$(as mia "vault ls -q" 2>/dev/null | tr "\n" " ")"; [ -n "$ids" ] || exit 1
		as mia "vault rm $ids" >/dev/null 2>&1 || exit 1
		! as mia "vault get app/beta" >/dev/null 2>&1 || { echo "a secret survived"; exit 1; }
		[ "$(as mia "note get runbook.md" 2>/dev/null)" = "# runbook $N" ] || { echo "the notes went with the secrets"; exit 1; }'

spec_end
