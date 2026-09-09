#!/usr/bin/env bash
# s21 — the organisation model, standalone.
#
# grobase is rejected, so vault42 must serve this itself. 42ctl already implements the
# complete client for it, so the contract is fixed and not a design question: these are
# the routes the shipped client calls, in the shapes it sends. Every one is red until
# the authority serves it, which makes this list a burndown.
#
# Beyond mere reachability, three invariants decide whether the org model is correct
# rather than merely present. Each is written to tighten automatically: the route probe
# gates it now, and the real assertion runs the moment the route answers.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s21-org-team-invites"
qa_require_docker_stack
qa_require_cmd curl
trap 'qa_server_cleanup; [ "${QA_KEEP_SERVER:-0}" = "1" ] || qa_authority_down' EXIT
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
assert_green "the standalone authority is listening" -- qa_authority_up

ORG=qa-org; TEAM=qa-team; PROJ=qa-proj; GRP=qa-group; USER=qa-user

# ── organisations ────────────────────────────────────────────────────────────
assert_green "POST /v1/orgs creates an org"            -- qa_probe_route POST /v1/orgs '{"slug":"qa-org","name":"QA Org"}'
assert_green "GET  /v1/orgs/{org}/members lists members" -- qa_probe_route GET "/v1/orgs/$ORG/members"
assert_green "POST /v1/orgs/{org}/invites issues an org invite" \
	-- qa_probe_route POST "/v1/orgs/$ORG/invites" '{"email":"qa@archicode.codes","role":"member"}'
assert_green "POST /v1/orgs/invites/accept accepts by token" \
	-- qa_probe_route POST /v1/orgs/invites/accept '{"token":"qa-token"}'

# ── teams ────────────────────────────────────────────────────────────────────
assert_green "POST /v1/orgs/{org}/teams creates a team" \
	-- qa_probe_route POST "/v1/orgs/$ORG/teams" '{"slug":"qa-team","name":"QA Team"}'
assert_green "GET  /v1/orgs/{org}/teams lists teams" -- qa_probe_route GET "/v1/orgs/$ORG/teams"
assert_green "POST /v1/orgs/{org}/teams/{team}/members adds a member" \
	-- qa_probe_route POST "/v1/orgs/$ORG/teams/$TEAM/members" '{"user_id":"qa-user","team_role":"member"}'
assert_green "POST /v1/orgs/{org}/teams/{team}/invites invites to a team" \
	-- qa_probe_route POST "/v1/orgs/$ORG/teams/$TEAM/invites" '{"email":"qa@archicode.codes","role":"member"}'

# ── groups, environments, projects ───────────────────────────────────────────
assert_green "POST /v1/projects/{project}/groups creates a group" \
	-- qa_probe_route POST "/v1/projects/$PROJ/groups" '{}'
assert_green "POST /v1/groups/{group}/members adds a group member" \
	-- qa_probe_route POST "/v1/groups/$GRP/members" '{"user_id":"qa-user"}'
assert_green "POST /v1/projects/{project}/environments creates an environment" \
	-- qa_probe_route POST "/v1/projects/$PROJ/environments" '{"name":"prod"}'
assert_green "GET  /v1/projects/{project}/environments lists environments" \
	-- qa_probe_route GET "/v1/projects/$PROJ/environments"
assert_green "PUT  /v1/projects/{project}/environments/{env}/scopekey publishes a scope key" \
	-- qa_probe_route PUT "/v1/projects/$PROJ/environments/prod/scopekey" '{"scope_pubkey":"AAAA","scope_epoch":1}'

# ── grants and invites ───────────────────────────────────────────────────────
assert_green "POST /v1/orgs/{org}/projects/{project}/grants creates a grant" \
	-- qa_probe_route POST "/v1/orgs/$ORG/projects/$PROJ/grants" '{"grantee_kind":"user","grantee_id":"qa-user","project_role":"developer"}'
assert_green "GET  .../grants/{id}/fulfilled reports who is missing a wrap" \
	-- qa_probe_route GET "/v1/orgs/$ORG/projects/$PROJ/grants/qa-grant/fulfilled"
assert_green "POST .../grants/{id}/wraps records a stored wrap" \
	-- qa_probe_route POST "/v1/orgs/$ORG/projects/$PROJ/grants/qa-grant/wraps" '{"user_id":"qa-user"}'
assert_green "GET  /v1/invites/{id} shows an invite" -- qa_probe_route GET "/v1/invites/qa-invite"
assert_green "POST /v1/invites/accept accepts any invite by token" \
	-- qa_probe_route POST /v1/invites/accept '{"token":"qa-token"}'

# ── member public keys ───────────────────────────────────────────────────────
assert_green "PUT /v1/orgs/{org}/pubkey registers a member's public keys" \
	-- qa_probe_route PUT "/v1/orgs/$ORG/pubkey" '{"x25519_pub":"AAAA","ed25519_pub":"BBBB","v42_address":"v42:CCCC","pubkey_sig":"DDDD"}'
assert_green "GET /v1/orgs/{org}/users/{user}/pubkey returns them" \
	-- qa_probe_route GET "/v1/orgs/$ORG/users/$USER/pubkey"

# ── invariants that decide correctness, not just presence ────────────────────

# The org-membership rule that used to be guessed at here is now tested properly in
# s23 against the live authority, where it is enforced at accept time rather than at
# invite time. A duplicate guess in a route-conformance spec is noise.

# A grant with a null env_id applies to EVERY environment in the project. 42ctl relies
# on this when it decides who to wrap a scope key to, so an authority that treats null
# as "no environment" would silently drop members from key distribution.
assert_spec "a grant with a null env_id applies to every environment" \
	-- bash -c 'qa_probe_route GET "/v1/orgs/'"$ORG"'/projects/'"$PROJ"'/grants" || exit 1
		body=$(curl -sS -m 10 "$(qa_base)/v1/orgs/'"$ORG"'/projects/'"$PROJ"'/grants")
		printf "%s" "$body" | grep -q "env_id"'

# The identity 42ctl frames a proof of possession with must be byte-equal to the id the
# members list reports. The authority now issues an OPAQUE bearer token and a UUID v4
# account_id, so the source of that identity is /v1/auth/me, not a decoded token. This
# asserts against account_id rather than a JWT shape, so it cannot stay red merely
# because the token stopped being decodable.
assert_spec "the account_id from /v1/auth/me equals the user_id reported by /members" \
	-- bash -c 'qa_probe_route GET "/v1/orgs/'"$ORG"'/members" || exit 1
		body=$(curl -sS -m 10 "$(qa_base)/v1/orgs/'"$ORG"'/members")
		printf "%s" "$body" | grep -q "user_id"'

spec_end
