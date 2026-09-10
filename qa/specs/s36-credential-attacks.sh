#!/usr/bin/env bash
# s36 — taking somebody's credentials without their permission.
#
# The other specs ask whether the right people can reach the right secrets. This one asks how
# an attacker gets to be one of the right people in the first place, and it is organised by
# where the attacker stands rather than by which component is at fault.
#
# ON THE MACHINE. A bearer credential in a world-readable file is every other account on that
# host acting as you. This crate stores three — the wrapped keystore, the vault contract, the
# control-plane session — and until this spec ran, one of the three was owner-only and two
# were whatever the umask allowed.
#
# AT THE FRONT DOOR. Unlimited password guessing, and signup answering differently for an
# address that exists, are the two oldest holes in any account system. Neither needs a flaw in
# the cryptography: they need only patience and a list of email addresses. Both were open when
# this spec was written and both are closed now — including the two halves that decide whether
# a limit is worth having: a correct password is refused WHILE throttled, since an attacker's
# last guess is a correct one, and a throttled unknown address answers exactly as a throttled
# real one, since a limit applied after the account lookup rebuilds the oracle it was meant to
# close.
#
# ON THE NETWORK. Every check above assumes the attacker had to steal something. A policy that
# refuses a valid credential arriving from an address nobody approved is the layer that holds
# when that assumption fails, and it is the one this product does not have.
#
# The classes here are the ones that actually happen: OWASP API Security's Broken
# Authentication, and the credential-storage and brute-force findings that dominate real
# incident reports. Nothing here is a novel attack, which is the point.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s36-credential-attacks"
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
W="$QA_RESULTS/s36"
rm -rf "$W"
mkdir -p "$W"
MAIL="s36-$N@archicode.codes"
PW="s36-password-$N"

# ── on the machine: what a credential file lets a neighbour do ───────────────
qa_actor_reset victim
V_ID="$(qa_actor_account victim "$MAIL" "$PW")"
qa_actor victim "$W" "auth whoami" >/dev/null 2>&1
STATE="$QA_RESULTS/actors/victim"
assert_green "the victim has a session and a keystore on disk" \
	-- bash -c '[ -s "$1/session.tok" ] && [ -s "$1/keystore.v42" ]' _ "$STATE"

# 0077 means no group and no other bits. Asserted per file rather than once, because these
# are written by three different call sites and the rule was in only one of them.
#
# Only files the CLIENT writes are asserted here. The session token in an actor's state is
# injected by the harness rather than minted by 42ctl, so asserting its mode would measure
# the harness — which is exactly the mistake the first version of this made.
for f in keystore.v42 session.tok; do
	assert_green "$f is readable only by its owner" \
		-- bash -c 'm=$(stat -c %a "$1/$2" 2>/dev/null) || exit 1
			[ $((0$m & 0077)) -eq 0 ] || { printf "%s is mode %s\n" "$2" "$m"; exit 1; }' \
		_ "$STATE" "$f"
done

# The repair case, driven through a real client write: a keystore left wide by an older
# version is narrowed the next time 42ctl writes it. A throwaway identity, because the way to
# make the client rewrite a keystore is to create one.
assert_green "a credential file left wide by an older version is narrowed on the next write" \
	-- bash -c 'qa_actor_reset repairee >/dev/null
		qa_actor repairee "$1" "keys init" >/dev/null 2>&1
		k="$QA_RESULTS/actors/repairee/keystore.v42"
		[ -s "$k" ] || { printf "no keystore was written, so nothing was narrowed\n"; exit 1; }
		chmod 644 "$k"
		[ "$(stat -c %a "$k")" = 644 ] || { printf "the widening did not take\n"; exit 1; }
		qa_actor repairee "$1" "keys init --force" >/dev/null 2>&1
		m=$(stat -c %a "$k")
		[ $((0$m & 0077)) -eq 0 ] || { printf "still mode %s\n" "$m"; exit 1; }' \
	_ "$W"

# ── at the front door: what an attacker learns and how fast ──────────────────
assert_green "an unknown email and a wrong password are refused identically at login" \
	-- bash -c 'a=$(qa_code POST /v1/auth/login "" "{\"email\":\"nobody-$1@archicode.codes\",\"password\":\"x\"}")
		b=$(qa_code POST /v1/auth/login "" "{\"email\":\"$2\",\"password\":\"definitely-wrong\"}")
		[ "$a" = "$b" ] || { printf "absent=%s wrong=%s\n" "$a" "$b"; exit 1; }' _ "$N" "$MAIL"

# Signup answers differently for an address that already exists, which is an enumeration
# oracle that needs no password at all: an attacker learns who has an account by trying to
# register them. The login path is careful about this and the signup path is not.
assert_green "signup does not reveal whether an address is already registered" \
	-- bash -c 'taken=$(qa_code POST /v1/auth/signup "" "{\"email\":\"$2\",\"password\":\"whatever-pw-1234\"}")
		fresh=$(qa_code POST /v1/auth/signup "" "{\"email\":\"fresh-$1@archicode.codes\",\"password\":\"whatever-pw-1234\"}")
		[ "$taken" = "$fresh" ]' _ "$N" "$MAIL"

# Its OWN address, on purpose. A throttle keyed on the address is exactly what this assertion
# is asking for, so running it against the account the rest of the spec logs into would leave
# that account refused for an hour and turn every later assertion red for a reason that has
# nothing to do with what it tests. An attack gets its own target here too.
BRUTE="brute-$N@archicode.codes"
qa_signup "$BRUTE" "$PW" >/dev/null
assert_green "the account the guessing runs against exists and works first" \
	-- bash -c '[ -n "$(qa_login "$1" "$2")" ]' _ "$BRUTE" "$PW"
assert_green "a run of failed logins is eventually refused rather than answered forever" \
	-- bash -c 'for i in $(seq 1 20); do
			qa_code POST /v1/auth/login "" "{\"email\":\"$1\",\"password\":\"wrong-$i\"}" >/dev/null
		done
		last=$(qa_code POST /v1/auth/login "" "{\"email\":\"$1\",\"password\":\"wrong-final\"}")
		[ "$last" = 429 ] || [ "$last" = 423 ]' _ "$BRUTE"
# The half that decides whether the limit is worth having: an attacker's last guess is a
# correct one, so a limit a correct password walks through is not a limit.
assert_green "the correct password is refused too while the address is throttled" \
	-- bash -c '[ "$(qa_code POST /v1/auth/login "" "{\"email\":\"$1\",\"password\":\"$2\"}")" = 429 ]' _ "$BRUTE" "$PW"
# And the half that decides whether anybody keeps it: a throttled address must not be
# distinguishable from a throttled address that does not exist, or the defence hands over the
# enumeration the login path refuses.
assert_green "a throttled unknown address answers exactly as a throttled real one" \
	-- bash -c 'ghost="ghost-$2@archicode.codes"
		for i in $(seq 1 20); do
			qa_code POST /v1/auth/login "" "{\"email\":\"$ghost\",\"password\":\"wrong-$i\"}" >/dev/null
		done
		a=$(qa_code POST /v1/auth/login "" "{\"email\":\"$1\",\"password\":\"x\"}")
		b=$(qa_code POST /v1/auth/login "" "{\"email\":\"$ghost\",\"password\":\"x\"}")
		[ "$a" = "$b" ] && [ "$a" = 429 ]' _ "$BRUTE" "$N"

# One-time codes go out as real email in production. Unlimited requests are somebody else's
# inbox, somebody else's deliverability reputation, and somebody else's bill.
assert_green "requesting a one-time code does not reveal whether the account exists" \
	-- bash -c 'a=$(qa_code POST /v1/auth/otp/request "" "{\"email\":\"$2\"}")
		b=$(qa_code POST /v1/auth/otp/request "" "{\"email\":\"nobody-$1@archicode.codes\"}")
		[ "$a" = "$b" ]' _ "$N" "$MAIL"
assert_green "one-time codes cannot be requested without limit" \
	-- bash -c 'flood="flood-$2@archicode.codes"
		for i in $(seq 1 10); do
			qa_code POST /v1/auth/otp/request "" "{\"email\":\"$flood\"}" >/dev/null
		done
		last=$(qa_code POST /v1/auth/otp/request "" "{\"email\":\"$flood\"}")
		[ "$last" = 429 ]' _ "$MAIL" "$N"

# ── a stolen token, and what ends it ─────────────────────────────────────────
assert_green "a token that never existed is refused" \
	-- bash -c '[ "$(qa_code GET /v1/auth/me "not-a-real-token-at-all-$1")" = 401 ]' _ "$N"
assert_green "logging out ends the token immediately" \
	-- bash -c 't=$(qa_login "$1" "$2"); [ "$(qa_code GET /v1/auth/me "$t")" = 200 ] || exit 1
		qa_code POST /v1/auth/logout "$t" "{}" >/dev/null
		[ "$(qa_code GET /v1/auth/me "$t")" = 401 ]' _ "$MAIL" "$PW"
# A password change is what somebody does when they think a session has been taken, so it has
# to reach the sessions they cannot see.
assert_green "changing the password ends every OTHER session, not just this one" \
	-- bash -c 'one=$(qa_login "$1" "$2"); two=$(qa_login "$1" "$2")
		[ -n "$one" ] && [ -n "$two" ] || exit 1
		[ "$(qa_code GET /v1/auth/me "$two")" = 200 ] || { printf "the second session never worked\n"; exit 1; }
		qa_code POST /v1/auth/passwd "$one" "{\"current_password\":\"$2\",\"new_password\":\"$2-rotated\"}" >/dev/null
		[ "$(qa_code GET /v1/auth/me "$two")" = 401 ]' _ "$MAIL" "$PW"
# The login has to succeed first. Without that control an empty token makes the accept fail
# for the wrong reason, and a refusal of nobody proves nothing about refusing a guess.
# Asserted as "not accepted" rather than as one status. The answer is 404 rather than 401 and
# both are correct: an invite that does not exist is not found. Pinning the exact code would
# make this fail on a wording change while the property held, which is how an assertion stops
# being read and starts being edited.
assert_green "an invite token cannot be guessed" \
	-- bash -c 't=$(qa_login "$1" "$2-rotated")
		[ -n "$t" ] || { printf "the rotated password did not log in, so nothing was tested\n"; exit 1; }
		[ "$(qa_code GET /v1/auth/me "$t")" = 200 ] || { printf "the session does not work\n"; exit 1; }
		c=$(qa_code POST /v1/orgs/invites/accept "$t" "{\"token\":\"guessed-$3\"}")
		case "$c" in 200 | 201 | 204) printf "a guessed invite token was ACCEPTED (%s)\n" "$c"; exit 1 ;; esac' _ "$MAIL" "$PW" "$N"

# ── the door itself ──────────────────────────────────────────────────────────
# Every organisation, team, project and grant verb authenticates with a SESSION. Until this
# landed, the only way to mint one through the CLI was the GitHub device flow, so a
# deployment without a GitHub app configured could not reach the group model at all — the
# whole feature complete, tested, and unreachable. A password login is the second door and
# for most deployments the only one.
assert_green "a session can be obtained with an email and a password, without GitHub" \
	-- bash -c 'qa_actor_reset doorway >/dev/null
		mail="door-$2@archicode.codes"
		qa_signup "$mail" "door-pw-$2" >/dev/null
		out=$(QA_ACCOUNT_PASSWORD="door-pw-$2" qa_actor doorway "$1" "auth login --password --email $mail" 2>&1) || { printf "%s\n" "$out" | tail -3; exit 1; }
		[ -s "$QA_RESULTS/actors/doorway/session.tok" ]' _ "$W" "$N"
assert_green "the session that login saved is one the authority accepts" \
	-- bash -c '[ "$(qa_code GET /v1/auth/me "$(cat "$QA_RESULTS/actors/doorway/session.tok")")" = 200 ]'
assert_green "the saved session file is readable only by its owner" \
	-- bash -c 'm=$(stat -c %a "$QA_RESULTS/actors/doorway/session.tok") || exit 1
		[ $((0$m & 0077)) -eq 0 ] || { printf "mode %s\n" "$m"; exit 1; }'

# ── on the network: the layer that holds when a credential is already stolen ─
# Everything above assumes the attacker had to take something. This is the assumption failing:
# they have a valid session, and the question is whether the address it arrives from matters.
#
# The shape asserted here is an ALLOW list on the organisation, not a block list. A block list
# is a list of the attackers you have already met; an allow list is the set of places your
# people actually work from, and it refuses everywhere else by default. Refusing by default is
# the only version that helps against an attacker you have not met yet.
#
# It has to fail CLOSED and be visible: an operator who cannot see the policy will not trust
# it, and one who cannot add their own address will disable it.
assert_spec "an organisation can declare which networks may use its credentials" \
	-- bash -c 'c=$(qa_code PUT "/v1/orgs/probe-$1/network-policy" "" "{\"allow\":[\"203.0.113.0/24\"]}")
		[ "$c" != 404 ] && [ "$c" != 405 ]' _ "$N"
assert_spec "a valid session arriving from an address outside the policy is refused" \
	-- bash -c 'false  # not built: no network policy exists to be outside of'
assert_spec "the refusal names the policy rather than looking like a bad password" \
	-- bash -c 'false  # not built — an operator locked out by policy must be able to tell'

spec_end
