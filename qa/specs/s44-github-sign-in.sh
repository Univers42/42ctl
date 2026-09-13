#!/usr/bin/env bash
# s44 — signing in with GitHub, through 42ctl, to the end.
#
# The authority's device flow is tested in vault42 against a hostile stub, but nothing had ever
# run `42ctl auth login --github` to a session: every battery authority lacked a GitHub app, so
# the only thing a spec could see was the refusal. Here the authority is pointed at
# qa/fixtures/github/stub.pl, which answers GitHub's three routes and lets the spec play the
# person — approve the code, deny it — by writing a file.
#
# What must hold, beyond "it logs in": the CLI keeps polling while the grant is pending instead of
# giving up; GitHub only vouches for an address, so the session is for the account that already
# holds that VERIFIED address and no other; an address with no account, or a denied grant, is
# refused with the authority's reason rather than a bare status; and the session it saves is a
# real one that the organisation verbs accept.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s44-github-sign-in"
qa_require_docker_stack
trap 'qa_github_down; qa_server_cleanup; [ "${QA_KEEP_SERVER:-0}" = "1" ] || qa_authority_down' EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
assert_green "the authority is listening" -- qa_authority_up
[ "$_qa_regression" -eq 0 ] || {
	spec_end
	exit 1
}

N="$$$(date +%s)"
W="$QA_RESULTS/s44"
rm -rf "$W"
mkdir -p "$W/stub" "$W/gia" "$W/hal"
STUB="$W/stub"
GIA="gia-$N@archicode.codes"
export N W STUB GIA
qa_actor_reset gia
qa_actor_reset hal
assert_green "the GitHub stand-in is listening on the battery network" -- qa_github_up "$STUB"

gia() { QA_ACCOUNT_PASSWORD="pw-gia-$N" qa_actor gia "$W/gia" "$*"; }
hal() { qa_actor hal "$W/hal" "$*"; }
# github_says <emails json> — what GitHub will report as the signer's addresses.
github_says() { printf '%s\n' "$1" >"$STUB/emails.json"; }
# person <approve|deny|nothing> — the person's answer to the code, reset each time.
person() {
	rm -f "$STUB/approve" "$STUB/deny"
	[ "$1" = nothing ] || : >"$STUB/$1"
}
export -f gia hal github_says person

assert_green "gia has an account, made with a password" \
	-- bash -c 'gia "auth signup --email $GIA" >/dev/null 2>&1'

# ── the flow that works ──────────────────────────────────────────────────────
# GitHub lists an unverified PRIMARY address first. Anybody can type an address into a profile,
# so the verified one must win even though it is not primary.
github_says "[{\"email\":\"someone-else-$N@archicode.codes\",\"primary\":true,\"verified\":false},{\"email\":\"$GIA\",\"primary\":false,\"verified\":true}]"
person nothing
: >"$STUB/requests.log"
assert_green "login shows the code to type, waits while it is pending, and signs in once approved" \
	-- bash -c '( sleep 4; : >"$STUB/approve" ) &
		out="$(gia "auth login --github" 2>&1)" || { printf "%s\n" "$out"; exit 1; }
		grep -q "enter code: QA42-0001" <<<"$out" && grep -qx "logged in via GitHub on profile .default." <<<"$out" ||
			{ printf "%s\n" "$out"; exit 1; }'
assert_green "it polled while the grant was pending, rather than failing at the first answer" \
	-- bash -c '[ "$(grep -c "^POST /login/oauth/access_token pending$" "$STUB/requests.log")" -ge 2 ] &&
		grep -q "^POST /login/oauth/access_token token$" "$STUB/requests.log" || { cat "$STUB/requests.log"; exit 1; }'
assert_green "the authority asked GitHub with its client id, and read the addresses with the token it got" \
	-- bash -c 'grep -q "^POST /login/device/code client_id=qa-github-client$" "$STUB/requests.log" &&
		grep -q "^GET /user/emails bearer-ok$" "$STUB/requests.log" && ! grep -q "bearer-wrong" "$STUB/requests.log"'
assert_green "the session belongs to the account holding the verified address" \
	-- bash -c 'gia "auth me" 2>/dev/null | grep -qE "^email +$GIA$"'
assert_green "auth status reads it as a session" \
	-- bash -c 'gia "auth status" 2>/dev/null | grep -q "^profile .default.: logged in — session only"'
assert_green "and the organisation verbs accept it" \
	-- bash -c 'gia "org create --slug gh-$N --name GitHubbed" 2>/dev/null | grep -q "created org"'

# ── the flows that must not work ─────────────────────────────────────────────
assert_green "a denied grant is refused with the authority's reason, and saves no session" \
	-- bash -c 'person deny
		out="$(hal "auth login --github" 2>&1)" && { printf "signed in: %s\n" "$out"; exit 1; }
		grep -q "GitHub sign-in was refused (HTTP 401)" <<<"$out" || { printf "%s\n" "$out"; exit 1; }
		hal "auth status" 2>/dev/null | grep -q "logged out"'
assert_green "an approved grant for an address with no account here is refused — GitHub cannot create one" \
	-- bash -c 'github_says "[{\"email\":\"hal-$N@archicode.codes\",\"primary\":true,\"verified\":true}]"; person approve
		out="$(hal "auth login --github" 2>&1)" && { printf "signed in: %s\n" "$out"; exit 1; }
		grep -q "refused (HTTP 401)" <<<"$out" && hal "auth status" 2>/dev/null | grep -q "logged out"'
assert_green "an account whose address GitHub has not verified is refused, and the reason says so" \
	-- bash -c 'github_says "[{\"email\":\"$GIA\",\"primary\":true,\"verified\":false}]"; person approve
		out="$(hal "auth login --github" 2>&1)" && { printf "signed in: %s\n" "$out"; exit 1; }
		grep -q "no verified address on this GitHub account" <<<"$out" || { printf "%s\n" "$out"; exit 1; }'
assert_green "none of those refusals left a session behind" \
	-- bash -c '[ ! -s "$(qa_actor_dir hal)/session.tok" ]'

spec_end
