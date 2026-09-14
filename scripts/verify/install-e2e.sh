#!/bin/sh
# *************************************************************************** #
#                                                                            #
#   install-e2e.sh                                                           #
#                                                                            #
#   Proves a published release installs and updates on a machine that has    #
#   never seen 42ctl: a non-root user with an empty HOME, through the same    #
#   one-liner the README gives, into ~/.local/bin.                            #
#                                                                            #
#   install-e2e.sh WANT OLDER [INSTALLER]                                     #
#     WANT       the tag `releases/latest` must now be (e.g. v0.1.9)          #
#     OLDER      an earlier tag to install first and update from              #
#     INSTALLER  install.sh to use (a URL, or a path; default: main's)        #
#                                                                            #
#   Exit 0 only if: a fresh install lands WANT in ~/.local/bin; a pinned      #
#   install lands OLDER; `42ctl update` takes OLDER to WANT in place; and a    #
#   second `update` says there is nothing to do. Needs curl or wget.          #
#                                                                            #
# *************************************************************************** #
set -u

WANT=${1:?usage: install-e2e.sh WANT OLDER [INSTALLER]}
OLDER=${2:?usage: install-e2e.sh WANT OLDER [INSTALLER]}
INSTALLER=${3:-https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh}
BIN="${HOME}/.local/bin/42ctl"

step() { printf '\n== %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

# Run install.sh with ARGS, from a URL through curl or wget, or from a local path.
installer() {
	case "$INSTALLER" in
	http*)
		if command -v curl >/dev/null 2>&1; then
			curl -fsSL "$INSTALLER" | sh -s -- "$@"
		else
			wget -qO- "$INSTALLER" | sh -s -- "$@"
		fi ;;
	*) sh "$INSTALLER" "$@" ;;
	esac
}

# The version the installed binary reports, as a tag.
installed() { "$BIN" version | awk 'NR == 1 { print "v" $2 }'; }

step "a fresh install lands the latest release in ~/.local/bin"
[ ! -e "$BIN" ] || fail "$BIN exists before the install — this is not a fresh machine"
installer || fail "the installer failed"
[ -x "$BIN" ] || fail "no executable at $BIN"
[ "$(installed)" = "$WANT" ] || fail "installed $(installed), releases/latest should be $WANT"

step "a pinned install lands the older release over it"
installer --version "$OLDER" || fail "the pinned install failed"
[ "$(installed)" = "$OLDER" ] || fail "installed $(installed), pinned $OLDER"

step "update --check sees the new release"
"$BIN" update --check | tee /tmp/check.out
grep -q "available ${WANT#v}" /tmp/check.out || fail "update --check does not offer $WANT"

step "update replaces the binary in place"
"$BIN" update || fail "update failed"
[ "$(installed)" = "$WANT" ] || fail "after update: $(installed), want $WANT"

step "a second update has nothing to do"
"$BIN" update | tee /tmp/again.out
grep -q "up to date" /tmp/again.out || fail "a second update did not say it is up to date"

printf '\nOK: %s installs fresh and updates from %s on %s (%s)\n' "$WANT" "$OLDER" \
	"$(. /etc/os-release 2>/dev/null && printf '%s' "$ID")" "$(uname -m)"
