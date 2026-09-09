#!/usr/bin/env bash
# qa/run.sh — run the whole battery and print one scoreboard.
#
# Exit status counts REGRESSIONS ONLY. Expected reds are the point of this battery, not
# a failure of it, so the suite stays usable as a merge gate while the unbuilt half of
# the product is still red. A regression — something that worked and stopped working —
# is the only thing that fails the run.
#
#   ./qa/run.sh              every spec
#   ./qa/run.sh s10 s12      only the named specs
#   QA_KEEP_SERVER=1 ...     leave the server up afterwards for poking at

set -uo pipefail
QA_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$QA_DIR/lib/harness.sh"
source "$QA_DIR/lib/server.sh"

: >"$QA_JSON"
qa_reclaim_workspace

# Both real defects found so far were state-dependent: one only bit after provisioning
# had converged, the other only appeared when something else had run first. A suite that
# only ever passes in one fixed order is weaker than it looks, so the order can be
# shuffled and the whole thing repeated.
#
#   QA_SHUFFLE=1 ./qa/run.sh      run the specs in a random order
#   QA_REPEAT=3 ./qa/run.sh       run the whole battery three times
#   QA_SEED=42 QA_SHUFFLE=1 ...   reproduce a specific shuffle
SHUFFLE="${QA_SHUFFLE:-0}"
REPEAT="${QA_REPEAT:-1}"
SEED="${QA_SEED:-$RANDOM}"

if [ $# -gt 0 ]; then
	SPECS=()
	for want in "$@"; do
		for f in "$QA_DIR/specs/${want}"*.sh; do [ -f "$f" ] && SPECS+=("$f"); done
	done
else
	SPECS=("$QA_DIR"/specs/s*.sh)
fi

STARTED=$(date +%s)
FAILED_SPECS=()
for pass in $(seq 1 "$REPEAT"); do
	ORDER=("${SPECS[@]}")
	if [ "$SHUFFLE" = "1" ]; then
		# s00 stays first: it is the preflight that says whether anything else is
		# trustworthy, and a shuffled preflight tells you nothing useful.
		mapfile -t ORDER < <(printf '%s\n' "${SPECS[@]}" | grep -v 's00-' | shuf --random-source=<(yes "$SEED$pass"))
		ORDER=("$QA_DIR/specs/s00-preflight.sh" "${ORDER[@]}")
		printf '\n# pass %s/%s, shuffled with seed %s\n' "$pass" "$REPEAT" "$SEED$pass"
	elif [ "$REPEAT" != "1" ]; then
		printf '\n# pass %s/%s\n' "$pass" "$REPEAT"
	fi
	for spec in "${ORDER[@]}"; do
		[ -f "$spec" ] || continue
		QA_KEEP_SERVER=1 bash "$spec"
		[ $? -ne 0 ] && FAILED_SPECS+=("$(basename "$spec" .sh)")
	done
done
[ "${QA_KEEP_SERVER:-0}" = "1" ] || { qa_server_down; qa_authority_down; }

# ── scoreboard, read back from the machine-readable log ──────────────────────
# grep -c prints 0 AND exits 1 when nothing matches, so a `|| printf 0` fallback would
# emit "0\n0" and every later arithmetic test would break on it. Capture, don't chain.
count() { local n; n=$(grep -c "\"status\":\"$1\"" "$QA_JSON" 2>/dev/null); printf '%s' "${n:-0}"; }
PASS=$(count pass); REG=$(count regression); RED=$(count expected_red)
MET=$(count now_met); SKIP=$(count skipped)

printf '\n'
printf '═══════════════════════════════════════════════════════════\n'
printf ' vault42 QA battery — %s specs in %ss\n' "${#SPECS[@]}" "$((  $(date +%s) - STARTED ))"
# Record what was actually tested. vault42 is pinned, but the 42ctl tree is edited by
# whoever is working in it, so a result is only attributable if both are named.
printf '  42ctl   %s\n' "$(git -C "$C42_ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
printf '  vault42 %s (pinned)\n' "$(git -C "$QA_DIR/.cache/vault42" rev-parse --short HEAD 2>/dev/null || echo unknown)"
printf '═══════════════════════════════════════════════════════════\n'
printf '  %-28s %s\n' "passing (must stay green)" "$PASS"
printf '  %-28s %s\n' "REGRESSIONS" "$REG"
printf '  %-28s %s\n' "expected red (unbuilt)" "$RED"
printf '  %-28s %s\n' "spec now met (promote)" "$MET"
printf '  %-28s %s\n' "skipped" "$SKIP"
printf '  machine-readable: %s\n' "$QA_JSON"

if [ "$MET" -gt 0 ]; then
	printf '\n  Specs that just went green — change assert_spec to assert_green:\n'
	grep '"status":"now_met"' "$QA_JSON" | sed 's/.*"desc":"\([^"]*\)".*/    • \1/'
fi
if [ "$REG" -gt 0 ]; then
	printf '\n  FAILING — assertions that must be green:\n'
	grep '"status":"regression"' "$QA_JSON" | sed 's/.*"desc":"\([^"]*\)".*/    • \1/'
	printf '\n  failing specs: %s\n' "${FAILED_SPECS[*]}"
	exit 1
fi
printf '\n  No regressions.\n'
exit 0
