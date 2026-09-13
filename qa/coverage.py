#!/usr/bin/env python3
# *************************************************************************** #
#                                                                             #
#   coverage.py                                                               #
#                                                                             #
#   Which 42ctl commands the QA battery actually RAN.                         #
#                                                                             #
#   Usage:  python3 qa/coverage.py COMMANDS TRACE [--require-all]             #
#                                                                             #
#   COMMANDS is `42ctl help commands` output — every runnable command the     #
#   parser knows, so a verb added tomorrow is counted without editing this.   #
#   TRACE is the file 42ctl appends to when FT_TRACE_COMMANDS names it: one   #
#   command path per invocation that PARSED, never an argument. A verb only   #
#   mentioned in a spec, or only asked for --help, is not in it.              #
#                                                                             #
#   A command is covered when it ran at least once — including runs that     #
#   were refused, because a refusal is a behaviour the specs assert on.       #
#                                                                             #
# *************************************************************************** #
import collections
import re
import sys

# Verbs a hermetic battery may never reach. Empty on purpose: the GitHub verbs cannot COMPLETE
# without a GitHub App on the authority, but they can run and be refused, and s42 runs them —
# a refusal is behaviour worth asserting. Add a name here only with the reason beside it.
UNREACHABLE: set = set()


def commands(path):
    """Every command path `help commands` lists, in the order it lists them."""
    found = []
    for line in open(path, encoding="utf-8"):
        match = re.match(r"^    42ctl ((?:[a-z][a-z0-9-]*)(?: [a-z][a-z0-9-]*)*)", line)
        if match:
            found.append(match.group(1))
    return list(dict.fromkeys(found))


def main():
    known = commands(sys.argv[1])
    ran = collections.Counter(
        line.strip() for line in open(sys.argv[2], encoding="utf-8") if line.strip()
    )
    if not known:
        print("coverage: no commands read — is the first argument `help commands` output?")
        sys.exit(2)
    missing = [path for path in known if ran[path] == 0]
    for path in known:
        print(f"{ran[path]:5d}  {path}")
    required = [path for path in missing if path not in UNREACHABLE]
    print(f"\n{len(known) - len(missing)} of {len(known)} commands ran at least once")
    if missing:
        print("never ran: " + ", ".join(missing))
    if "--require-all" in sys.argv and required:
        sys.exit(1)


if __name__ == "__main__":
    main()
