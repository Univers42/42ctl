#!/usr/bin/env bash
# s40 — every cloud verb, what it prints, and the two things it must never do.
#
# `42ctl cloud` drives flyctl, and a battery cannot hold a Fly account or risk a production
# machine. FT_FLYCTL points 42ctl at `qa/fixtures/fly/flyctl` instead: a stand-in that answers
# with the JSON flyctl prints and RECORDS every command it is handed. That record is what makes
# the negative assertions here worth something — "nothing destructive ran" is checked against a
# log proved to contain the lifecycle commands that did run.
#
# The two rules the operator set, and how each is held:
#   - NOTHING in 42ctl deletes, detaches, releases or scales away a cloud resource. There is no
#     such verb, and `adapters/flyctl.rs` refuses those commands at the one place every
#     invocation is built, so a verb added later cannot either.
#   - Changing a machine is for ADMINISTRATORS. Fly enforces it — a read-only token is refused a
#     lease on the machine, which was measured against production — and 42ctl turns Fly's
#     refusal into one that names the token and says what kind would work. STUB_READONLY=1
#     reproduces Fly's exact words.
#
# Every listing is checked in each form `--format` offers, so this is also the format battery
# for the cloud half of the CLI: the default table, json, a bare template, a `table` template,
# `{{json .}}`, `-q`, `--filter` (including a key in the wrong case), and a filter key that is
# not a column, which must be refused by name.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s40-cloud-controller"
command -v docker >/dev/null 2>&1 || spec_skip "docker is not available"
qa_require_cmd python3
qa_build_client >/dev/null 2>&1
assert_green "the client binary is built" -- test -x "$C42_ROOT/target/debug/42ctl"

W="$QA_RESULTS/s40"
rm -rf "$W"
mkdir -p "$W/fly" "$W/state"
cp "$QA_ROOT/fixtures/fly/"*.json "$W/fly/"
printf '[{"id":"vs_fresh","size":1048576,"status":"created","created_at":"%s","retention_days":5}]\n' \
	"$(date -u -d '-2 hours' +%Y-%m-%dT%H:%M:%SZ)" >"$W/fly/snapshots-vol_server00000001.json"
printf '[{"id":"vs_stale","size":1048576,"status":"created","created_at":"%s","retention_days":5}]\n' \
	"$(date -u -d '-10 days' +%Y-%m-%dT%H:%M:%SZ)" >"$W/fly/snapshots-vol_authority000001.json"
printf '%s\n' '{"current":"prod","profiles":{"prod":{"server":"https://vault42-server.fly.dev","authority":"https://vault42-authority.fly.dev"}}}' \
	>"$W/state/config.json"
: >"$W/state/flyctl.log"
S40_TOKEN="s40-stub-token-$$-never-on-argv"
export W S40_TOKEN

# Run 42ctl against the stub. stdout and stderr are kept apart, because the lifecycle verbs
# echo the command they run on stderr and the listings must stay clean on stdout.
cloud() {
	# shellcheck disable=SC2086 # QA_DOCKER_USER is empty or a two-word flag
	docker run --rm -i $QA_DOCKER_USER \
		-v "$C42_ROOT":/work:ro -v "$W/fly":/fly:ro -v "$W/state":/state -w /state \
		-e HOME=/state -e NO_COLOR=1 -e FT_CONFIG=/state/config.json \
		-e FT_FLYCTL=/work/qa/fixtures/fly/flyctl -e STUB_DIR=/fly -e STUB_LOG=/state/flyctl.log \
		-e FLY_API_TOKEN="$S40_TOKEN" -e STUB_READONLY="${STUB_READONLY:-0}" \
		--entrypoint /work/target/debug/42ctl "$QA_IMG" "$@"
}
export -f cloud

# expect <description> <exact expected stdout> <42ctl args…>
# The command must succeed and print exactly the expected text — no more, no less.
expect() {
	local desc="$1" want="$2"
	shift 2
	assert_green "$desc" -- bash -c 'want="$1"; shift
		got="$(cloud "$@" 2>/dev/null)" || { echo "exit non-zero"; exit 1; }
		[ "$got" = "$want" ] || { diff <(printf "%s\n" "$want") <(printf "%s\n" "$got"); exit 1; }' \
		_ "$want" "$@"
}

# ── which apps, and the default tables ───────────────────────────────────────
expect "apps are derived from the profile's endpoints, with no configuration" \
	"vault42-server server https://vault42-server.fly.dev
vault42-authority authority https://vault42-authority.fly.dev" \
	cloud apps --format '{{.App}} {{.Role}} {{.Endpoint}}'

assert_green "the default machine table has a header, a rule, and one row per machine" \
	-- bash -c 'out="$(cloud cloud machine ls 2>/dev/null)" || exit 1
		head -1 <<<"$out" | grep -qE "^ID +App +Name +State +Region +Checks +Size +Volume *$" || exit 1
		sed -n 2p <<<"$out" | grep -qE "^─+$" || exit 1
		grep -qE "^3d8e4a1f0b2c77 +vault42-server +stub-server-one +started +cdg +1/1 +shared-1x:256MB +vol_server00000001 *$" <<<"$out" || exit 1
		grep -qE "^91c5d2e6a7f0b3 +vault42-authority +stub-authority-one +stopped +cdg +1/1 +shared-1x:256MB +vol_authority000001 *$" <<<"$out" || exit 1
		[ "$(wc -l <<<"$out")" -eq 4 ]'

# ── every --format form, on one listing ──────────────────────────────────────
expect "a bare template prints one line per row and no header" \
	"3d8e4a1f0b2c77 started
91c5d2e6a7f0b3 stopped" \
	cloud machine ls --format '{{.ID}} {{.State}}'

expect "-q prints the first column alone, ready for \$( … )" \
	"3d8e4a1f0b2c77
91c5d2e6a7f0b3" \
	cloud machine ls -q

expect "--filter keeps the rows that match" \
	"91c5d2e6a7f0b3" \
	cloud machine ls -q --filter State=stopped

expect "a filter key is matched regardless of case" \
	"3d8e4a1f0b2c77" \
	cloud machine ls -q --filter app=vault42-server

expect "two filters must both hold" \
	"" \
	cloud machine ls -q --filter State=stopped --filter App=vault42-server

expect "a template naming a field that does not exist renders it as nothing" \
	"3d8e4a1f0b2c77 []
91c5d2e6a7f0b3 []" \
	cloud machine ls --format '{{.ID}} [{{.NoSuchField}}]'

assert_green "a table template prints the columns it names, tab escapes honoured" \
	-- bash -c 'out="$(cloud cloud machine ls --format "table {{.ID}}\t{{.Volume}}" 2>/dev/null)" || exit 1
		head -1 <<<"$out" | grep -qE "^ID +Volume *$" &&
		grep -qE "^91c5d2e6a7f0b3 +vol_authority000001 *$" <<<"$out" &&
		[ "$(wc -l <<<"$out")" -eq 4 ]'

assert_green "--format json is one array holding every kept row, parseable" \
	-- bash -c 'cloud cloud machine ls --format json 2>/dev/null | python3 -c "
import json, sys
rows = json.load(sys.stdin)
assert isinstance(rows, list) and len(rows) == 2, rows
assert {r[\"ID\"] for r in rows} == {\"3d8e4a1f0b2c77\", \"91c5d2e6a7f0b3\"}, rows
assert rows[0][\"Volume\"] == \"vol_server00000001\", rows
"'

assert_green "{{json .}} prints one JSON object per row" \
	-- bash -c 'cloud cloud machine ls --format "{{json .}}" 2>/dev/null | python3 -c "
import json, sys
lines = [json.loads(l) for l in sys.stdin.read().splitlines()]
assert len(lines) == 2 and lines[1][\"State\"] == \"stopped\", lines
"'

assert_green "--format json with a filter carries only the kept rows" \
	-- bash -c 'cloud cloud machine ls --format json --filter State=started 2>/dev/null |
		python3 -c "import json,sys; r=json.load(sys.stdin); assert [x[\"ID\"] for x in r]==[\"3d8e4a1f0b2c77\"], r"'

assert_green "a filter on a key that is not a column is refused, naming the columns" \
	-- bash -c 'out="$(cloud cloud machine ls --filter Colour=red 2>&1)"; [ $? -ne 0 ] || exit 1
		grep -q "unknown filter key .Colour." <<<"$out" && grep -q "ID, App, Name, State" <<<"$out"'

assert_green "-q and --format together are refused rather than one silently winning" \
	-- bash -c '! cloud cloud machine ls -q --format json >/dev/null 2>&1'

# ── the other listings, each checked against its fixture ─────────────────────
expect "status summarises each app's machine" \
	"vault42-server 3d8e4a1f0b2c77 started 1/1 develop
vault42-authority 91c5d2e6a7f0b3 stopped 1/1 develop" \
	cloud status --format '{{.App}} {{.ID}} {{.State}} {{.Checks}} {{.Image}}'

expect "ports lists every published port with its handlers" \
	"443/tcp 8443 tls false
80/tcp 8444 http true
443/tcp 8444 tls,http false" \
	cloud machine ports --format '{{.Port}} {{.Internal}} {{.Handlers}} {{.ForceHTTPS}}'

expect "events reads one machine's history" \
	"stop stopped flyd" \
	cloud machine events 91c5d2e6a7f0b3 --format '{{.Event}} {{.Status}} {{.Source}}'

assert_green "inspect hands back fly's raw JSON for the machines asked for" \
	-- bash -c 'cloud cloud machine inspect 91c5d2e6a7f0b3 2>/dev/null | python3 -c "
import json, sys
rows = json.load(sys.stdin)
assert rows[0][\"id\"] == \"91c5d2e6a7f0b3\" and rows[0][\"config\"][\"guest\"][\"memory_mb\"] == 256, rows
"'

assert_green "inspect of an unknown id fails naming it, and still prints the ones it found" \
	-- bash -c 'out="$(cloud cloud machine inspect 3d8e4a1f0b2c77 00000000000000 2>&1)"; [ $? -ne 0 ] || exit 1
		grep -q "\"id\": \"3d8e4a1f0b2c77\"" <<<"$out" && grep -q "00000000000000" <<<"$out" && grep -q "1 of 2 failed" <<<"$out"'

expect "volumes lead with encryption, attachment and retention" \
	"vol_server00000001 vault42-server true 3d8e4a1f0b2c77 5
vol_authority000001 vault42-authority true 91c5d2e6a7f0b3 5" \
	cloud volume ls --format '{{.ID}} {{.App}} {{.Encrypted}} {{.Attached}} {{.Snapshots}}'

expect "snapshots of one volume" \
	"vs_stale created" \
	cloud volume snapshots vol_authority000001 --app vault42-authority --format '{{.ID}} {{.Status}}'

expect "app secrets are listed by name and digest, never by value" \
	"VAULT42_CONTRACT_PUBKEY vault42-server Deployed
VAULT42_REGISTER_TOKEN vault42-authority Deployed
VAULT42_OTP_PROOF_SECRET vault42-authority Staged" \
	cloud secret ls --format '{{.Name}} {{.App}} {{.Status}}'

assert_green "the secret listing has no column a value could hide in" \
	-- bash -c 'cloud cloud secret ls --format json 2>/dev/null | python3 -c "
import json, sys
rows = json.load(sys.stdin)
assert rows and all(set(r) == {\"Name\", \"App\", \"Digest\", \"Status\"} for r in rows), rows
"'

expect "ips per app" \
	"2a09:8280:1::stub:1 vault42-server v6
66.241.124.10 vault42-authority shared_v4" \
	cloud net ips --format '{{.Address}} {{.App}} {{.Type}}'

expect "certificates per app" \
	"authority.vault42.example vault42-authority Ready" \
	cloud net certs --format '{{.Hostname}} {{.App}} {{.Status}}'

assert_green "logs pass flyctl's stream through" \
	-- bash -c 'cloud cloud machine logs --app vault42-server --no-tail 2>/dev/null | grep -q "healthz ok"'

# ── health: verdicts, not just data ──────────────────────────────────────────
assert_green "health fails the deployment when a snapshot is ten days old, and says which" \
	-- bash -c 'out="$(cloud cloud health --no-wake --format "{{.Check}}|{{.Status}}|{{.Detail}}" 2>/dev/null)"; [ $? -ne 0 ] || exit 1
		grep -qx "vault42-authority snapshot age|fail|newest snapshot is 240h old" <<<"$out" &&
		grep -qx "vault42-server snapshot age|ok|newest snapshot is 2h old" <<<"$out" &&
		grep -qx "vault42-server scope keys|ok|scope keys enabled" <<<"$out" &&
		grep -qx "vault42-server volume|ok|vault42_data attached, encrypted" <<<"$out"'
assert_green "--no-wake skips the probes that would start a stopped machine, and says so" \
	-- bash -c 'cloud cloud health --no-wake --format "{{.Check}}|{{.Status}}" 2>/dev/null |
		grep -qx "authority /healthz|skip"'

# ── lifecycle: administrators only ───────────────────────────────────────────
assert_green "an administrator's token stops a machine, and the command run is echoed first" \
	-- bash -c 'err="$(cloud cloud machine stop 91c5d2e6a7f0b3 --app vault42-authority 2>&1 >/dev/null)" || exit 1
		grep -q "^+ /work/qa/fixtures/fly/flyctl machine stop 91c5d2e6a7f0b3 --app vault42-authority$" <<<"$err"'
for verb in start restart suspend; do
	assert_green "an administrator's token can $verb a machine" \
		-- bash -c 'cloud cloud machine "$1" 91c5d2e6a7f0b3 --app vault42-authority >/dev/null 2>&1' _ "$verb"
done
assert_green "wait blocks on a state through flyctl" \
	-- bash -c 'cloud cloud machine wait 91c5d2e6a7f0b3 --state stopped --app vault42-authority 2>/dev/null |
		grep -q "reached the requested state"'
assert_green "several machines in one stop are each attempted, and the unknown one is named" \
	-- bash -c 'out="$(cloud cloud machine stop 91c5d2e6a7f0b3 ffffffffffffff --app vault42-authority 2>&1)"; [ $? -ne 0 ] || exit 1
		grep -q "ffffffffffffff" <<<"$out" && grep -q "1 of 2 failed" <<<"$out"'

assert_green "--dry-run prints the command and never runs it" \
	-- bash -c 'before="$(grep -c "^argv machine restart" "$W/state/flyctl.log")"
		err="$(cloud cloud machine restart 3d8e4a1f0b2c77 --app vault42-server --dry-run 2>&1 >/dev/null)" || exit 1
		grep -q "machine restart 3d8e4a1f0b2c77" <<<"$err" || exit 1
		[ "$(grep -c "^argv machine restart" "$W/state/flyctl.log")" -eq "$before" ]'

assert_green "a MEMBER's read-only token is refused a stop, told why, and nothing changes" \
	-- bash -c 'out="$(STUB_READONLY=1 cloud cloud machine stop 3d8e4a1f0b2c77 --app vault42-server 2>&1)"; [ $? -ne 0 ] || exit 1
		grep -q "fly refused this FLY_API_TOKEN for .fly machine stop 3d8e4a1f0b2c77 --app vault42-server." <<<"$out" &&
		grep -q "needs an administrator.s token" <<<"$out"'
assert_green "a member's read-only token is refused a snapshot the same way" \
	-- bash -c 'out="$(STUB_READONLY=1 cloud cloud volume snapshot vol_server00000001 --app vault42-server 2>&1)"; [ $? -ne 0 ] || exit 1
		grep -q "needs an administrator.s token" <<<"$out"'
assert_green "the same read-only token still reads everything a member may see" \
	-- bash -c 'STUB_READONLY=1 cloud cloud machine ls -q 2>/dev/null | grep -qx 3d8e4a1f0b2c77 &&
		STUB_READONLY=1 cloud cloud volume ls -q 2>/dev/null | grep -qx vol_server00000001'
assert_green "an administrator's token snapshots a volume" \
	-- bash -c 'cloud cloud volume snapshot vol_server00000001 --app vault42-server 2>/dev/null | grep -q "Scheduled to snapshot"'

# ── what never happened ──────────────────────────────────────────────────────
# Each absence is paired with the presence that proves the log is the real haystack.
assert_green "the flyctl log recorded the lifecycle commands that did run" \
	-- bash -c 'grep -q "^argv machine stop 91c5d2e6a7f0b3 --app vault42-authority$" "$W/state/flyctl.log" &&
		grep -q "^argv volumes snapshots create vol_server00000001" "$W/state/flyctl.log"'
# The command words of every recorded invocation — the leading arguments before the first flag —
# checked one by one, so an app merely NAMED like a verb cannot trip it and a real one cannot hide.
destructive_runs() {
	awk '/^argv /{ for (i = 2; i <= 4 && i <= NF; i++) { if ($i ~ /^-/) break
		if ($i ~ /^(destroy|delete|remove|rm|release|unset|scale|detach|revoke)$/) { print; break } } }' "$1"
}
export -f destructive_runs
assert_green "the checker for destructive commands does catch one when it is there" \
	-- bash -c 'probe="$(mktemp)"; printf "argv machine list --app a --json\nargv machine destroy 81 --app a\n" >"$probe"
		[ "$(destructive_runs "$probe")" = "argv machine destroy 81 --app a" ]; s=$?; rm -f "$probe"; exit $s'
assert_green "no destructive flyctl command was ever run, by any verb" \
	-- bash -c '[ -z "$(destructive_runs "$W/state/flyctl.log")" ]'
assert_green "the stub never had to refuse a command no verb should run" \
	-- bash -c '! grep -q "^refused " "$W/state/flyctl.log"'
assert_green "the token reached flyctl through the environment on every call" \
	-- bash -c 'grep -q "^token-in-env$" "$W/state/flyctl.log" && ! grep -q "^token-missing$" "$W/state/flyctl.log"'
assert_green "and its value never appeared on flyctl's argument list" \
	-- bash -c '! grep -qF "$S40_TOKEN" "$W/state/flyctl.log"'

assert_green "the CLI has no verb that deletes a machine, a volume or an app" \
	-- bash -c 'pages="$(for group in "cloud machine" "cloud volume" "cloud secret" "cloud net" "cloud"; do
			cloud $group --help 2>/dev/null
		done)"
		grep -qE "^  stop " <<<"$pages" && grep -qE "^  snapshot " <<<"$pages" || { echo "help pages not read"; exit 1; }
		! grep -qE "^  (rm|destroy|delete|remove|prune|release|unset|scale|detach) " <<<"$pages"'

spec_end
