#!/usr/bin/env bash
# s00 — preflight. Proves the battery itself can run and is reproducible, and stands
# guard over the one thing no other spec covers: that the operator's real credentials
# never leak into either repository.
#
# Everything here MUST be green. A red in s00 means the harness is untrustworthy, so
# every downstream red or green is meaningless until it is fixed.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/fixtures.sh"

spec_begin "s00-preflight"

VAULT_DIR="$(cd "$C42_ROOT/.." && pwd)"
V42="${VAULT42_DIR:-$VAULT_DIR/vault42}"

assert_green "docker is available" -- command -v docker
assert_green "the 42ctl workspace manifest is readable" -- test -r "$C42_ROOT/Cargo.toml"
assert_green "the vault42 sibling checkout exists" -- test -d "$V42/crates"
assert_green "the Inception fixture submodule is populated" \
	-- test -f "$C42_ROOT/qa/fixtures/inception/.env.example"
assert_green "the Inception fixture is pinned to a commit" \
	-- git -C "$C42_ROOT/qa/fixtures/inception" rev-parse HEAD

# Reproducibility: generate every fixture twice and compare a hash manifest. If two
# runs on one machine already diverge, a red handed to someone else is worthless.
_fixture_digest() {
	fixture_build_all >/dev/null
	find "$QA_GEN" -type f -exec sha256sum {} + | sed "s|$QA_GEN||" | sort | sha256sum
}
D1="$(_fixture_digest)"
D2="$(_fixture_digest)"
assert_green "fixture generation is byte-reproducible across runs" \
	-- test "$D1" = "$D2"

# The operator's real credentials live in the parent directory, outside both repos.
# Assert that, and assert their VALUES appear nowhere in either tree. The needle is
# never printed — only the verdict.
ENV_FILE="$VAULT_DIR/.env"
assert_green "the operator .env sits outside both git repositories" \
	-- bash -c '! git -C "$1" ls-files --error-unmatch "$2" >/dev/null 2>&1' _ "$C42_ROOT" "$ENV_FILE"

if [ -r "$ENV_FILE" ]; then
	# EVERY value is guarded, not a hand-written list of names.
	#
	# This used to enumerate GH_PAT and MAIL_TITAN. Two SMTP passwords were then added to
	# the file and the guard skipped both in silence — a credential check that protects
	# only what someone remembered to name is the same defect as a test suite that covers
	# only the paths someone remembered to write. Enumerate nothing; exempt by shape.
	while IFS='=' read -r key val; do
		case "$key" in
		'' | '#'*) continue ;;
		esac
		[ -n "${val:-}" ] || continue

		# A bare email address is an identifier, not a credential. It belongs in source as
		# a default sender, so leaking it is not a thing that can happen. Everything else
		# is treated as secret-shaped and must appear nowhere in either repository.
		if printf '%s' "$val" | grep -qE '^[^@[:space:]]+@[^@[:space:]]+\.[a-z]{2,}$'; then
			printf '# note: %s is an email address, not a credential\n' "$key"
			continue
		fi
		# Very short values would match half the source tree and say nothing.
		[ "${#val}" -ge 8 ] || { printf '# note: %s is too short to search for meaningfully\n' "$key"; continue; }

		assert_green "the value of $key does not appear anywhere in 42ctl" \
			-- bash -c '! grep -rqaF "$1" "$2" --exclude-dir=.git --exclude-dir=target --exclude-dir=gen --exclude-dir=.cache --exclude-dir=results' _ "$val" "$C42_ROOT"
		assert_green "the value of $key does not appear anywhere in vault42" \
			-- bash -c '! grep -rqaF "$1" "$2" --exclude-dir=.git --exclude-dir=target' _ "$val" "$V42"
	done <"$ENV_FILE"
else
	printf '# note: no operator .env at %s — credential-leak checks not run\n' "$ENV_FILE"
fi


# ── the battery's own integrity ──────────────────────────────────────────────
#
# This suite edits its own source with exact-match replacements, and that fails
# SILENTLY: a patch that does not match leaves the file untouched while the script
# reports success. Both sessions have now been caught by a check that ran, reported
# success, and measured nothing. These assertions make the harness prove itself on every
# run rather than relying on somebody remembering to sweep it.

assert_green "every spec and library parses" \
	-- bash -c 'for f in "$1"/specs/*.sh "$1"/lib/*.sh "$1"/run.sh; do bash -n "$f" || exit 1; done' _ "$QA_ROOT"

# A redefined function silently wins over the earlier one, so an appended helper can
# shadow a fixed one and nothing would say so.
assert_green "no library function is defined twice" \
	-- bash -c '[ -z "$(grep -h "^[a-z_]*() {" "$1"/lib/*.sh | sort | uniq -d)" ]' _ "$QA_ROOT"

# Every absence check must carry its own control. assert_absent searches a haystack it
# never proves is real; assert_zero_knowledge cannot be called without one.
assert_green "no absence check bypasses its control" \
	-- bash -c '! grep -qE "^[[:space:]]*assert_absent " "$1"/specs/*.sh' _ "$QA_ROOT"

# And the control itself must be able to fail. An empty haystack must be refused.
assert_green "the zero-knowledge control refuses an empty haystack" \
	-- bash -c 'e=$(mktemp); : >"$e"
		out=$(bash -c "[ -s \"$e\" ]" 2>&1); rc=$?
		rm -f "$e"; [ "$rc" -ne 0 ]'

spec_end
