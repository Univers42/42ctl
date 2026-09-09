#!/usr/bin/env bash
# qa/lib/harness.sh — the shared assertion + reporting core for the vault42 QA battery.
#
# The battery exists to be READ BY A MACHINE as much as by a human, so every assertion
# declares which kind of failure it is:
#
#   assert_green  this MUST pass today. Red here fails the run. Usually that means a
#                 regression, but it also covers a shipped command that is simply
#                 broken: either way the behaviour is one an operator is entitled to.
#   assert_spec   this is a spec for an UNBUILT feature. Red here is EXPECTED and fine.
#                 Green here is news: the feature landed, and the spec should be
#                 promoted to assert_green.
#
# That split is the whole design. It lets the battery stay permanently red on the
# unbuilt half of the product without ever going numb: the exit code counts ONLY
# regressions, so it is still usable as a merge gate while specs accumulate.
#
# Deliberately NOT `set -e`: a spec must keep running after a red so one failure does
# not hide the twenty behind it.

set -uo pipefail

QA_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
QA_ROOT="$(cd "$QA_LIB_DIR/.." && pwd)"
C42_ROOT="$(cd "$QA_ROOT/.." && pwd)"
export QA_ROOT C42_ROOT

: "${QA_RESULTS:=$QA_ROOT/results}"
: "${QA_JSON:=$QA_RESULTS/latest.jsonl}"
mkdir -p "$QA_RESULTS"

SPEC_NAME=""
_qa_n=0
_qa_pass=0
_qa_regression=0
_qa_red_expected=0
_qa_spec_now_met=0

# Start a spec file. Resets counters and prints the TAP-ish banner.
spec_begin() {
	SPEC_NAME="$1"
	_qa_n=0
	_qa_pass=0
	_qa_regression=0
	_qa_red_expected=0
	_qa_spec_now_met=0
	printf '\n# ── %s ─────────────────────────────────────────────\n' "$SPEC_NAME"
}

# Emit one JSON Lines record so a machine can consume the battery without parsing TAP.
_qa_json() {
	printf '{"spec":"%s","n":%d,"kind":"%s","status":"%s","desc":"%s","ts":"%s"}\n' \
		"$SPEC_NAME" "$_qa_n" "$1" "$2" "$(printf '%s' "$3" | sed 's/"/\\"/g')" \
		"$(date -u +%Y-%m-%dT%H:%M:%SZ)" >>"$QA_JSON"
}

# Run a command, capturing stdout+stderr into QA_LAST_OUTPUT and its status into
# QA_LAST_STATUS. Never aborts the spec.
qa_run() {
	QA_LAST_OUTPUT="$("$@" 2>&1)"
	QA_LAST_STATUS=$?
	return 0
}

# assert_green <desc> -- <cmd...>
# The command MUST exit 0. A non-zero exit is a regression and fails the battery.
assert_green() {
	local desc="$1"
	shift
	[ "${1:-}" = "--" ] && shift
	_qa_n=$((_qa_n + 1))
	qa_run "$@"
	if [ "$QA_LAST_STATUS" -eq 0 ]; then
		_qa_pass=$((_qa_pass + 1))
		printf 'ok %d - %s\n' "$_qa_n" "$desc"
		_qa_json green pass "$desc"
	else
		_qa_regression=$((_qa_regression + 1))
		printf 'not ok %d - %s  # REGRESSION (exit %d)\n' "$_qa_n" "$desc" "$QA_LAST_STATUS"
		printf '%s\n' "$QA_LAST_OUTPUT" | sed 's/^/  | /' | head -15
		_qa_json green regression "$desc"
	fi
}

# assert_spec <desc> -- <cmd...>
# The command is EXPECTED to fail because the feature is not built yet. Red is
# recorded and does not fail the battery; green is reported as SPEC-NOW-MET so the
# implementer knows to promote it.
assert_spec() {
	local desc="$1"
	shift
	[ "${1:-}" = "--" ] && shift
	_qa_n=$((_qa_n + 1))
	qa_run "$@"
	if [ "$QA_LAST_STATUS" -eq 0 ]; then
		_qa_spec_now_met=$((_qa_spec_now_met + 1))
		printf 'ok %d - %s  # SPEC-NOW-MET (promote to assert_green)\n' "$_qa_n" "$desc"
		_qa_json spec now_met "$desc"
	else
		_qa_red_expected=$((_qa_red_expected + 1))
		printf 'not ok %d - %s  # EXPECTED-RED (unbuilt)\n' "$_qa_n" "$desc"
		printf '%s\n' "$QA_LAST_OUTPUT" | sed 's/^/  | /' | head -6
		_qa_json spec expected_red "$desc"
	fi
}

# Assert two files are byte-identical. The core promise of push/pull.
assert_bytes_equal() {
	local desc="$1" a="$2" b="$3" kind="${4:-green}"
	"assert_${kind}" "$desc" -- cmp -s "$a" "$b"
}

# Assert a string is absent from a file. Used for the zero-knowledge checks: a
# plaintext value or a real path must never appear in the server's stored bytes.
assert_absent() {
	local desc="$1" needle="$2" haystack="$3" kind="${4:-green}"
	"assert_${kind}" "$desc" -- bash -c '! grep -qaF "$1" "$2"' _ "$needle" "$haystack"
}

# Close a spec: print its tally and exit non-zero ONLY on regressions.
spec_end() {
	printf '# %s: %d passed, %d regressions, %d expected-red, %d spec-now-met\n' \
		"$SPEC_NAME" "$_qa_pass" "$_qa_regression" "$_qa_red_expected" "$_qa_spec_now_met"
	[ "$_qa_regression" -eq 0 ]
}

# Mark the whole spec as skipped for a stated reason (a missing prerequisite, not a
# product failure). A skip is never a regression, but it is loudly printed so a green
# battery can never be mistaken for a battery that actually ran.
spec_skip() {
	printf '# SKIP %s — %s\n' "$SPEC_NAME" "$1"
	_qa_json skip skipped "$1"
	exit 0
}

# Require a command on PATH or skip the spec.
qa_require_cmd() {
	command -v "$1" >/dev/null 2>&1 || spec_skip "missing command: $1"
}

# Assert that every file handed to the vault came back byte-identical at the same
# relative path. This is the no-silent-data-loss check: push reporting success while
# quietly carrying nothing is worse than push failing, because the loss is only
# discovered when a restore is actually needed.
assert_tree_reproduced() {
	local desc="$1" src="$2" dst="$3" kind="${4:-green}"
	"assert_${kind}" "$desc" -- bash -c '
		src=$1; dst=$2; missing=0
		while IFS= read -r -d "" f; do
			rel=${f#"$src"/}
			case $rel in .42ctl/*) continue ;; esac
			cmp -s "$f" "$dst/$rel" || { printf "  missing or differing: %s\n" "$rel"; missing=1; }
		done < <(find "$src" -type f -print0)
		[ "$missing" -eq 0 ]' _ "$src" "$dst"
}

# Assert a value never reached the server, AND prove the search was real.
#
# The control lives inside the assertion rather than beside it, because a control that has
# to be remembered is one the next caller omits. Three ways to fail, and the first two are
# the whole point: an absence check over an empty or unverified haystack proves nothing
# while looking identical to one that proves everything. That is not hypothetical — every
# zero-knowledge assertion in this battery was vacuous for its whole life because the dump
# it searched never contained the data.
#
# marker is something the server legitimately DOES store, such as an opaque path or the
# owner fingerprint. If the marker is missing the dump is not what it claims to be.
#
# Usage: assert_zero_knowledge <desc> <needle> <marker> <dump>
assert_zero_knowledge() {
	local desc="$1" needle="$2" marker="$3" dump="$4"
	assert_green "$desc" -- bash -c '
		[ -s "$3" ] || { printf "the dump is empty, so nothing was searched\n"; exit 1; }
		grep -qaF "$2" "$3" || { printf "the dump lacks the marker, so the search proves nothing\n"; exit 1; }
		! grep -qaF "$1" "$3"' _ "$needle" "$marker" "$dump"
}
