#!/usr/bin/env bash
# qa/lib/listing.sh — every listing, in every shape its output flags offer.
#
# `listing_matrix NAME RUNNER ARGS` runs one listing a dozen ways and asserts each shape says the
# same thing about the same rows: the default output, json, a bare template, `{{json .}}`, a
# `table` template, -q, --filter (exact, any case, two at once, a label, with -q), a filter that
# keeps nothing, a key that is not a column, and -q against --format. The rows come from
# `--format json`; the columns from the refusal of an unknown key, which must name them.
#
# RUNNER is a function called as `RUNNER "<42ctl args as one string>"`, so a template with
# spaces travels single-quoted. Set LISTING_MIN_ROWS so a filter is proved to exclude a row, and
# LISTING_DEFAULT=tsv for a listing whose documented unshaped output to a pipe is tab-separated.
#
# `listing_commands` prints every command `help commands` says takes --format, so a spec can
# assert it covered all of them and a listing added later cannot slip past the matrix.

# shellcheck disable=SC2154 # QA_LIB_DIR and QA_RESULTS come from harness.sh

_listing_run() {
	local runner="$1" dir="$2" variant="$3" args="$4"
	"$runner" "$args" >"$dir/$variant.out" 2>"$dir/$variant.err" </dev/null
	printf '%s\n' "$?" >"$dir/$variant.code"
}

listing_matrix() {
	local name="$1" runner="$2" args="$3" dir variant flags check
	dir="$QA_RESULTS/listings/${name// /-}"
	rm -rf "$dir"
	mkdir -p "$dir"
	_listing_run "$runner" "$dir" base-json "$args --format json"
	_listing_run "$runner" "$dir" columns "$args --filter NoSuchColumnQA=x"
	_listing_run "$runner" "$dir" default "$args"
	python3 "$QA_LIB_DIR/listing.py" plan "$dir" >"$dir/variants" 2>"$dir/plan.err"
	while IFS=$'\t' read -r variant flags; do
		_listing_run "$runner" "$dir" "$variant" "$args $flags"
	done <"$dir/variants"
	for check in $(python3 "$QA_LIB_DIR/listing.py" checks); do
		assert_green "$name: $(python3 "$QA_LIB_DIR/listing.py" describe "$check")" \
			-- python3 "$QA_LIB_DIR/listing.py" verify "$dir" "$check"
	done
}

# Every command `help commands` (in FILE) lists with a --format flag, one per line, sorted.
listing_commands() {
	python3 -B -c 'import sys; sys.path.insert(0, sys.argv[2]); import coverage
print("\n".join(sorted(p for p, f in coverage.commands(sys.argv[1]).items() if "--format" in f)))' "$1" "$QA_ROOT"
}
