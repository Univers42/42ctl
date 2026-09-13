#!/usr/bin/env python3
# *************************************************************************** #
#                                                                             #
#   coverage.py                                                               #
#                                                                             #
#   Which 42ctl commands the QA battery actually RAN.                         #
#                                                                             #
#   Usage:  coverage.py COMMANDS TRACE [--require-all] [--require-flags]      #
#                                                                             #
#   COMMANDS is `42ctl help commands` output — every runnable command the     #
#   parser knows, so a verb added tomorrow is counted without editing this.   #
#   TRACE is the file 42ctl appends to when FT_TRACE_COMMANDS names it: one   #
#   line per invocation that PARSED — its command path, a tab, the flags it   #
#   was given — never a value. A verb only mentioned in a spec, or only asked #
#   for --help, is not in it.                                                 #
#                                                                             #
#   A command is covered when it ran at least once — including runs that     #
#   were refused, because a refusal is a behaviour the specs assert on.       #
#                                                                             #
# *************************************************************************** #
import collections
import re
import sys

# Verbs a hermetic battery may never reach. Empty on purpose: a verb that can only be refused
# still runs, and a refusal is behaviour the specs assert on. Add a name only with its reason.
UNREACHABLE: set = set()

ENTRY = re.compile(r"^    42ctl ((?:[a-z][a-z0-9-]*)(?: [a-z][a-z0-9-]*)*)(.*)$")
CONTINUED = re.compile(r"^ {9,}[\[<-]")
FLAG = re.compile(r"--[a-z][a-z0-9-]*")


def commands(path):
    """Every command path `help commands` lists, mapped to the flags its synopsis offers.

    A synopsis wraps onto lines indented under its first argument; the summary under it is
    indented eight spaces and starts with a word, which is how the two are told apart.
    """
    found, current = {}, None
    for line in open(path, encoding="utf-8"):
        entry = ENTRY.match(line)
        if entry:
            current = entry.group(1)
            found.setdefault(current, [])
            found[current] += FLAG.findall(entry.group(2))
        elif current and CONTINUED.match(line):
            found[current] += FLAG.findall(line)
        else:
            current = None
    return {path: list(dict.fromkeys(flags)) for path, flags in found.items()}


def runs(path):
    """How often each command ran, and every flag each one was given at least once."""
    ran, given = collections.Counter(), collections.defaultdict(set)
    for line in open(path, encoding="utf-8"):
        command, _, flags = line.rstrip("\n").partition("\t")
        if command.strip():
            ran[command.strip()] += 1
            given[command.strip()].update(flags.split())
    return ran, given


def main():
    known = commands(sys.argv[1])
    if not known:
        print("coverage: no commands read — is the first argument `help commands` output?")
        sys.exit(2)
    ran, given = runs(sys.argv[2])
    missing = [path for path in known if ran[path] == 0]
    unflagged = {path: [f for f in flags if f not in given[path]] for path, flags in known.items()}
    offered = sum(len(flags) for flags in known.values())
    untried = sum(len(flags) for flags in unflagged.values())
    for path in known:
        gap = f"   never given: {' '.join(unflagged[path])}" if unflagged[path] else ""
        print(f"{ran[path]:5d}  {path}{gap}")
    print(f"\n{len(known) - len(missing)} of {len(known)} commands ran at least once")
    print(f"{offered - untried} of {offered} flags were given at least once")
    if missing:
        print("never ran: " + ", ".join(missing))
    required = [path for path in missing if path not in UNREACHABLE]
    if "--require-all" in sys.argv and required:
        sys.exit(1)
    if "--require-flags" in sys.argv and untried:
        sys.exit(1)


if __name__ == "__main__":
    main()
