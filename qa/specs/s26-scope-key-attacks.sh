#!/usr/bin/env bash
# s26 — attacking the key bridge.
#
# The control plane decides who is authorized. The data plane stores the wraps. Nothing
# ties them together except a derived scope id, and that is where the seams are.
#
# Three properties are tested. Whether a scope id from one organisation can collide with
# another's. Whether a recovered scope secret is checked against the key the environment
# actually advertises. And whether depositing a wrap into somebody else's namespace is
# authorized at all.
#
# The first is behavioural, driven entirely through the CLI with two ordinary users doing
# nothing unusual. The last two are source-level guards, because exercising them needs a
# raw gRPC client the battery does not have; they are labelled as such rather than
# dressed up as behavioural tests.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s26-scope-key-attacks"
qa_require_docker_stack
qa_require_cmd curl
trap 'qa_server_cleanup; [ "${QA_KEEP_SERVER:-0}" = "1" ] || qa_authority_down' EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
assert_green "the authority is listening" -- qa_authority_up
[ "$_qa_regression" -eq 0 ] || { spec_end; exit 1; }

N="$$-$(date +%s)"
W="$QA_RESULTS/s26"; rm -rf "$W"; mkdir -p "$W"
# The SAME project UUID in two unrelated organisations. Nothing forbids this: project
# creation honours a client-supplied UUID, which is what makes the two halves of
# "project" reconcilable, and is also what makes this reachable.
SHARED_UUID="eeeeeeee-ffff-4aaa-8bbb-$(printf '%012d' $$)"
SECRET_A='OWNER=org-alpha-secret-0001'
printf '%s\n' "$SECRET_A" >"$W/a.txt"
printf 'OWNER=org-omega-secret-0002\n' >"$W/m.txt"

for who in alice mallory bob; do qa_actor_reset "$who"; done
A_ID="$(qa_actor_account alice "s26a-$N@archicode.codes" "pw-a-$N")"; A_TOK="$(cat "$QA_RESULTS/actors/alice/session.tok")"
M_ID="$(qa_actor_account mallory "s26m-$N@archicode.codes" "pw-m-$N")"; M_TOK="$(cat "$QA_RESULTS/actors/mallory/session.tok")"
B_ID="$(qa_actor_account bob "s26b-$N@archicode.codes" "pw-b-$N")"; B_TOK="$(cat "$QA_RESULTS/actors/bob/session.tok")"

# ── what actually protects the scope namespace ───────────────────────────────
#
# The scope id is blake3 over the project UUID and the environment name, with no
# organisation in it, and the wrap store is keyed by (member, scope, epoch) globally.
# Two organisations using one project UUID would therefore write to the same row for the
# same person. That is prevented, but by project ids being globally unique rather than by
# the derivation itself — so the uniqueness rule is load-bearing for the crypto and is
# pinned here.
ORG_A="alpha-$N"; ORG_M="omega-$N"
qa_api POST /v1/orgs "$A_TOK" "{\"slug\":\"$ORG_A\",\"name\":\"A\"}" >/dev/null
qa_api POST /v1/orgs "$M_TOK" "{\"slug\":\"$ORG_M\",\"name\":\"M\"}" >/dev/null
qa_api POST "/v1/orgs/$ORG_A/projects" "$A_TOK" "{\"id\":\"$SHARED_UUID\",\"slug\":\"p\",\"name\":\"P\"}" >/dev/null

assert_green "a project UUID already used by another organisation is refused" \
	-- bash -c '[ "$(qa_code POST "/v1/orgs/$2/projects" "$1" "{\"id\":\"$3\",\"slug\":\"p\",\"name\":\"P\"}")" = 409 ]' \
	_ "$M_TOK" "$ORG_M" "$SHARED_UUID"

# The refusal blames the slug, but the slug is free in that organisation: the same
# request without an id succeeds with the same slug. An operator reading this message
# would go and rename a slug that was never the problem.
assert_spec "the refusal names the real conflict rather than blaming the slug" \
	-- bash -c 'r=$(qa_api POST "/v1/orgs/$2/projects" "$1" "{\"id\":\"$3\",\"slug\":\"other\",\"name\":\"P\"}"); grep -qiE "project id|already exists|id already" <<<"$r"' \
	_ "$M_TOK" "$ORG_M" "$SHARED_UUID"
assert_green "the same slug with a fresh id is accepted, proving the slug was free" \
	-- bash -c '[ "$(qa_code POST "/v1/orgs/$2/projects" "$1" "{\"slug\":\"p\",\"name\":\"P\"}")" = 201 ]' \
	_ "$M_TOK" "$ORG_M"

# ── multi-tenancy: one person, two organisations, two environments ───────────
M_UUID="ffffffff-aaaa-4bbb-8ccc-$(printf '%012d' $$)"
qa_api POST "/v1/orgs/$ORG_M/projects" "$M_TOK" "{\"id\":\"$M_UUID\",\"slug\":\"mp\",\"name\":\"MP\"}" >/dev/null
EA="$(qa_json "$(qa_api POST "/v1/projects/$SHARED_UUID/environments" "$A_TOK" '{"name":"prod"}' | cut -f2-)" id)"
EM="$(qa_json "$(qa_api POST "/v1/projects/$M_UUID/environments" "$M_TOK" '{"name":"prod"}' | cut -f2-)" id)"

for pair in "$ORG_A:$A_TOK" "$ORG_M:$M_TOK"; do
	slug="${pair%%:*}"; tok="${pair#*:}"
	t="$(qa_json "$(qa_api POST "/v1/orgs/$slug/invites" "$tok" "{\"email\":\"s26b-$N@archicode.codes\",\"role\":\"member\"}" | cut -f2-)" token)"
	qa_api POST /v1/orgs/invites/accept "$B_TOK" "{\"token\":\"$t\"}" >/dev/null
done
assert_green "one person can belong to two organisations at once" \
	-- bash -c 'grep -qF "$3" <<<"$(qa_api GET "/v1/orgs/$2/members" "$1")"' _ "$A_TOK" "$ORG_A" "$B_ID"

qa_actor alice "$W" "keys enroll --org $ORG_A" >/dev/null 2>&1
qa_actor mallory "$W" "keys enroll --org $ORG_M" >/dev/null 2>&1
qa_actor bob "$W" "keys enroll --org $ORG_A" >/dev/null 2>&1
qa_actor bob "$W" "keys enroll --org $ORG_M" >/dev/null 2>&1
qa_actor alice "$W" "vault env-init --org $ORG_A --project $SHARED_UUID --env prod" >/dev/null 2>&1
qa_actor mallory "$W" "vault env-init --org $ORG_M --project $M_UUID --env prod" >/dev/null 2>&1
qa_api POST "/v1/orgs/$ORG_A/projects/$SHARED_UUID/grants" "$A_TOK" \
	"{\"grantee_kind\":\"user\",\"grantee_id\":\"$B_ID\",\"project_role\":\"write\",\"env_id\":\"$EA\"}" >/dev/null
qa_api POST "/v1/orgs/$ORG_M/projects/$M_UUID/grants" "$M_TOK" \
	"{\"grantee_kind\":\"user\",\"grantee_id\":\"$B_ID\",\"project_role\":\"write\",\"env_id\":\"$EM\"}" >/dev/null
qa_actor alice "$W" "vault sync-keys --org $ORG_A --project $SHARED_UUID --env prod" >/dev/null 2>&1
qa_actor mallory "$W" "vault sync-keys --org $ORG_M --project $M_UUID --env prod" >/dev/null 2>&1
qa_actor alice "$W" "vault set-env --org $ORG_A --project $SHARED_UUID --env prod app/c < /project/a.txt" >/dev/null 2>&1
qa_actor mallory "$W" "vault set-env --org $ORG_M --project $M_UUID --env prod app/c < /project/m.txt" >/dev/null 2>&1

assert_green "bob reads the first organisation's secret" \
	-- bash -c 'qa_actor bob "$1" "vault get-env --org $2 --project $3 --env prod app/c" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG_A" "$SHARED_UUID" "$SECRET_A"
assert_green "being provisioned by a second organisation does not break the first" \
	-- bash -c 'qa_actor bob "$1" "vault get-env --org $2 --project $3 --env prod app/c" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG_A" "$SHARED_UUID" "$SECRET_A"
assert_green "bob reads the second organisation's secret too" \
	-- bash -c 'qa_actor bob "$1" "vault get-env --org $2 --project $3 --env prod app/c" 2>/dev/null | grep -q "org-omega-secret-0002"' \
	_ "$W" "$ORG_M" "$M_UUID"
assert_green "neither administrator can read the other organisation's secret" \
	-- bash -c '! qa_actor mallory "$1" "vault get-env --org $2 --project $3 --env prod app/c" 2>/dev/null | grep -qF "$4"' \
	_ "$W" "$ORG_A" "$SHARED_UUID" "$SECRET_A"

# ── source-level guards ──────────────────────────────────────────────────────
# These describe defences that do not exist yet. They are greps, and they are labelled
# as greps, because triggering them needs a raw gRPC client this battery does not have.

# The client already holds the environment's advertised scope public key in Ctx. A
# recovered scope secret is an X25519 secret, so deriving its public half and comparing
# is a few lines and needs no protocol change. Without it, a wrap planted by anyone is
# indistinguishable from the administrator's.
# Pinned to the behaviour, not to a line: recovery must consult the advertised key, and
# the file must derive a public half from the recovered secret to compare against it.
# The check itself lives in a helper, which is why an assertion aimed at the body of
# recover_scope_secret went red on an improvement — the third time I have made that
# mistake, so this one deliberately spans the whole path.
assert_green "a recovered scope secret is checked against the advertised scope public key" \
	-- bash -c 'f="$1/src/cmd/scope_recover.rs"
		sed -n "/fn recover_scope_secret/,/^}/p" "$f" | grep -q advertised &&
		grep -qE "PublicKey::from|x25519_pub" "$f"' _ "$C42_ROOT"
assert_green "the mismatch is reported to the operator with what to do about it" \
	-- bash -c 'grep -qi "sync-keys" "$1/src/cmd/scope_recover.rs"' _ "$C42_ROOT"

# The server still accepts a wrap into any member's namespace and overwrites what is
# there; its own doc comment states the caller need not own member_id. The severity is
# now much lower than it was: a planted wrap no longer opens silently, because recovery
# compares the key against what the environment advertises and names the problem. What
# remains is that anyone with an account can overwrite a member's wrap and lock them out
# until an administrator reconciles.
# The first version asserted the ABSENCE of a comment saying the caller need not own the
# member id, so the way to clear the red was to delete the comment. That is the purest form
# of an assertion testing text near a property instead of the property: the honest note
# describing the gap was the only thing holding the red, and removing it changed nothing
# about who may deposit.
#
# It now looks for the guard, with comments stripped: the handler must relate the caller to
# the member id it is writing under. Verified against three versions — today's, today's with
# that comment reworded away, and one carrying an ownership check — and only the last is
# green.
#
# The behavioural version would have actor A deposit a wrap into actor B's namespace and
# require a refusal, and it is NOT written because 42ctl offers no verb that makes that
# request. `sync-keys` is the only path and it needs the scope secret, which a non-member
# cannot recover — so the test would go red for the missing secret and prove nothing about
# the deposit check. Building the hostile request into the product to test for it would be
# worse than the static assertion.
assert_green "the wrap-deposit handler is where this spec looks for it" \
	-- bash -c 'sed -n "/async fn op_wrap_scope_key/,/^    }/p" "$1/crates/vault42-server/src/ops_scope.rs" |
		grep -q "store_one_rewrap"' _ "${VAULT42_DIR:-$C42_ROOT/qa/.cache/vault42}"
assert_spec "depositing a wrap into another member's namespace is authorized" \
	-- bash -c 'body=$(sed -n "/async fn op_wrap_scope_key/,/^    }/p" "$1/crates/vault42-server/src/ops_scope.rs" | sed "s|//.*||")
		grep -qE "caller.*member_id|member_id.*caller" <<<"$body"' \
	_ "${VAULT42_DIR:-$C42_ROOT/qa/.cache/vault42}"

# The derivation takes the project UUID and the environment name only. Defence in depth:
# today it is safe because project ids are globally unique, and hashing the organisation
# would make it safe by construction, so relaxing that uniqueness later could never merge
# two environments' key material under one id. The server cannot detect such a collision —
# the scope id arrives as an opaque key and every write under it is legitimate.
#
# The first version of this grepped the function body for the substring "org", which a
# COMMENT satisfies. A note reading "the org is deliberately NOT part of this derivation"
# would have turned it green while documenting the opposite of what it asserts, so someone
# tidying the function could clear the red by explaining why it is not done. Comments are
# stripped before the search now, and the search is for the derivation itself: an org in the
# signature and an org fed to the hasher. Verified against three versions of the function —
# today's, one carrying exactly that comment, and a plausible fixed one — and only the last
# is green.
#
# The control is a separate GREEN assertion rather than a line inside this one. A spec
# assertion that is red because the file moved looks exactly like one red because the
# feature is unbuilt, and that is the failure mode this battery keeps producing.
assert_green "the scope id derivation is where this spec looks for it" \
	-- bash -c 'body=$(sed -n "/pub fn scope_id/,/^}/p" "$1/src/adapters/scope.rs")
		grep -q "pub fn scope_id" <<<"$body" || { printf "scope_id was not found, so the spec below reads nothing\n"; exit 1; }
		grep -q "hasher.update" <<<"$body"' _ "$C42_ROOT"
assert_spec "the scope id is namespaced by organisation rather than relying on unique project ids" \
	-- bash -c 'body=$(sed -n "/pub fn scope_id/,/^}/p" "$1/src/adapters/scope.rs" | sed "s|//.*||")
		grep -qE "pub fn scope_id\(.*org" <<<"$body" || exit 1
		grep -qE "hasher\.update\([^)]*org" <<<"$body"' _ "$C42_ROOT"

spec_end
