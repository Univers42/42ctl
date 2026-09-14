#!/usr/bin/env python3
# *************************************************************************** #
#                                                                             #
#   listing.py                                                                #
#                                                                             #
#   Every listing, in every shape its output flags offer, checked against     #
#   the same rows. The rows come from `--format json`; each other shape must  #
#   say the same thing about them. Driven by qa/lib/listing.sh.               #
#                                                                             #
#   Usage:  listing.py plan   DIR        variants to run, as NAME<TAB>FLAGS   #
#           listing.py checks            the check names, in order            #
#           listing.py verify DIR CHECK  exit 0 when CHECK holds              #
#                                                                             #
#   DIR holds NAME.out / NAME.err / NAME.code for each variant that ran.      #
#                                                                             #
# *************************************************************************** #
import collections
import json
import os
import re
import shlex
import sys

UNKNOWN_KEY = "NoSuchColumnQA"
NOTHING = "qa-this-value-matches-nothing"
SEP = "|~|"
AGE = re.compile(r"\b\d+[smhd] ago\b")
ANSI = re.compile(r"\x1b\[[0-9;]*m")

CHECKS = {
    "json": "--format json is a list of rows",
    "rows": "the fixture gives it enough rows for a filter to exclude one",
    "columns": "a filter key that names no column is refused, naming the columns",
    "default": "the unshaped output shows every row",
    "template": "a bare template renders every column of every row",
    "json-line": "{{json .}} renders each row as the json form has it",
    "table-template": "a table template heads its columns with their names",
    "quiet": "-q prints the first column, one per line",
    "filter": "--filter keeps exactly the rows whose column shows that value",
    "filter-case": "a filter key matches its column whatever its case",
    "filter-and": "two filters must both hold, not either",
    "filter-label": "label=K=V matches a label, when the rows carry labels",
    "quiet-filter": "-q composes with --filter",
    "filter-none": "a filter that keeps no row is refused, and prints no row",
    "quiet-format": "-q with --format is refused",
}


def read(where, name, part):
    """One captured stream of a variant, or None when the variant never ran."""
    path = os.path.join(where, f"{name}.{part}")
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf-8", errors="replace") as handle:
        return ANSI.sub("", handle.read())


def code(where, name):
    """A variant's exit status."""
    text = read(where, name, "code")
    return int(text.strip()) if text and text.strip() else None


def display(value):
    """A field as the CLI shows it: strings bare, null empty, the rest as compact JSON."""
    if isinstance(value, str):
        return value
    if value is None:
        return ""
    return json.dumps(value, separators=(",", ":"))


def calm(value):
    """A value with relative ages levelled, so two runs a second apart compare equal."""
    if isinstance(value, str):
        return AGE.sub("<age>", value)
    if isinstance(value, list):
        return [calm(item) for item in value]
    if isinstance(value, dict):
        return {key: calm(item) for key, item in value.items()}
    return value


def rows(where):
    """The listing's rows from its json form; raises when they are not a list of objects."""
    parsed = json.loads(read(where, "base-json", "out") or "")
    if not isinstance(parsed, list) or not all(isinstance(row, dict) for row in parsed):
        raise ValueError(f"not a list of objects: {parsed!r:.200}")
    return parsed


def headers(where):
    """The columns, as the unknown-key refusal names them."""
    found = re.search(r"this list has (.+)$", read(where, "columns", "err") or "", re.M)
    return [name.strip() for name in found.group(1).split(",")] if found else []


def shown(row, column):
    """What a column shows for a row."""
    return display(row.get(column))


def discriminating(table, columns):
    """A column whose first-row value is stable and shared by some rows but not all."""
    for column in columns:
        want = shown(table[0], column)
        if not want or AGE.search(want):
            continue
        kept = [row for row in table if shown(row, column) == want]
        if len(kept) < len(table):
            return column, want
    for column in columns:
        want = shown(table[0], column)
        if want and not AGE.search(want):
            return column, want
    return None


def label(table):
    """A label the first row carries, as (key, value), or None."""
    labels = table[0].get("Labels") if table else None
    if isinstance(labels, dict):
        for key, value in sorted(labels.items()):
            if isinstance(value, str) and value:
                return key, value
    return None


def plan(where):
    """The variants that depend on the rows: their flags are built from what the rows hold."""
    table, columns = rows(where), headers(where)
    quote = shlex.quote
    print(f"template\t--format {quote(SEP.join('{{.' + c + '}}' for c in columns))}")
    print(f"json-line\t--format {quote('{{json .}}')}")
    pair = columns[:2]
    tabbed = "\\t".join("{{." + c + "}}" for c in pair)
    print(f"table-template\t--format {quote('table ' + tabbed)}")
    print("quiet\t-q")
    print("quiet-format\t-q --format json")
    print(f"filter-none\t--filter {quote(columns[0] + '=' + NOTHING)}")
    chosen = discriminating(table, columns)
    if chosen:
        column, want = chosen
        print(f"filter\t--format json --filter {quote(column + '=' + want)}")
        print(f"filter-case\t--format json --filter {quote(column.lower() + '=' + want)}")
        print(f"quiet-filter\t-q --filter {quote(column + '=' + want)}")
        other = next((c for c in columns if c != column and shown(table[0], c)
                      and not AGE.search(shown(table[0], c))), None)
        if other:
            both = f"--filter {quote(column + '=' + want)} --filter {quote(other + '=' + shown(table[0], other))}"
            print(f"filter-and\t--format json {both}")
        elif any(shown(row, column) != want for row in table):
            rival = next(shown(row, column) for row in table if shown(row, column) != want)
            both = f"--filter {quote(column + '=' + want)} --filter {quote(column + '=' + rival)}"
            print(f"filter-and\t--format json {both}")
    if label(table):
        key, value = label(table)
        print(f"filter-label\t--format json --filter {quote(f'label={key}={value}')}")


def ok_run(where, name):
    """Fail unless the variant ran and exited with a status the listing may answer with.

    That is 0, unless LISTING_OK_EXIT widens it: `cloud health` exits 1 as its verdict on the
    deployment and still prints every row. A refusal is then told apart by what it says.
    """
    status = code(where, name)
    if status is None:
        raise AssertionError(f"variant {name} never ran")
    if str(status) not in os.environ.get("LISTING_OK_EXIT", "0").split():
        raise AssertionError(f"{name} exited {status}: {read(where, name, 'err')!r:.300}")
    return read(where, name, "out")


def lines(text):
    """Non-empty lines."""
    return [line for line in (text or "").split("\n") if line != ""]


def same(got, want, what):
    """Fail with both sides when two multisets of rendered rows differ."""
    if collections.Counter(got) != collections.Counter(want):
        raise AssertionError(f"{what} differ\n  want: {sorted(want)!r:.600}\n  got:  {sorted(got)!r:.600}")


def expect_rows(where, name, kept):
    """The variant's json output is exactly `kept`."""
    got = json.loads(ok_run(where, name))
    same([json.dumps(calm(r), sort_keys=True) for r in got],
         [json.dumps(calm(r), sort_keys=True) for r in kept], "rows")


def chosen_filters(where):
    """The (column, value) pairs the plan filtered on, recovered from the variants file."""
    with open(os.path.join(where, "variants"), encoding="utf-8") as handle:
        flags = dict(line.rstrip("\n").split("\t", 1) for line in handle if "\t" in line)
    return flags


def pairs(flag_text):
    """Every --filter K=V in a flag string."""
    words = shlex.split(flag_text)
    return [words[i + 1].split("=", 1) for i, word in enumerate(words[:-1]) if word == "--filter"]


def keeps(row, column, want):
    """Whether a filter on `column` keeps `row`."""
    if column.lower() == "label":
        key, value = want.split("=", 1)
        return display((row.get("Labels") or {}).get(key)) == value
    match = next((c for c in row if c.lower() == column.lower()), column)
    return shown(row, match) == want


def verify(where, check):
    """Raise AssertionError unless `check` holds for the listing captured in `where`."""
    minimum = int(os.environ.get("LISTING_MIN_ROWS", "1"))
    if check == "json":
        ok_run(where, "base-json")
        if not rows(where):
            raise AssertionError("the json form is an empty list — nothing below would prove anything")
        return
    table, columns = rows(where), headers(where)
    flags = chosen_filters(where)
    if check == "rows":
        if len(table) < minimum:
            raise AssertionError(f"{len(table)} rows, the fixture must give at least {minimum}")
        if minimum > 1 and not any(len({shown(r, c) for r in table}) > 1 for c in columns):
            raise AssertionError("every column shows one value, so no filter can exclude a row")
        return
    if check == "columns":
        if code(where, "columns") in (None, 0) or UNKNOWN_KEY not in (read(where, "columns", "err") or ""):
            raise AssertionError(f"not refused by name: {read(where, 'columns', 'err')!r:.300}")
        if not columns or any(not any(c.lower() == k.lower() for k in table[0]) for c in columns):
            raise AssertionError(f"named columns {columns} are not all fields of the rows {list(table[0])}")
        return
    if check == "default":
        verify_default(where, table, columns)
        return
    if check == "template":
        same([AGE.sub("<age>", l) for l in lines(ok_run(where, "template"))],
             [calm(SEP.join(shown(r, c) for c in columns)) for r in table], "template lines")
        return
    if check == "json-line":
        same([json.dumps(calm(json.loads(l)), sort_keys=True) for l in lines(ok_run(where, "json-line"))],
             [json.dumps(calm(r), sort_keys=True) for r in table], "json lines")
        return
    if check == "table-template":
        out = lines(ok_run(where, "table-template"))
        if not out or out[0].split() != columns[:2]:
            raise AssertionError(f"header {out[:1]} is not {columns[:2]}")
        if len(out) != len(table) + 2 or set(out[1]) != {"─"}:
            raise AssertionError(f"want header, rule and {len(table)} rows, got {out!r:.400}")
        return
    if check == "quiet":
        same(lines(ok_run(where, "quiet")), [shown(r, columns[0]) for r in table], "ids")
        return
    if check == "quiet-format":
        if code(where, "quiet-format") in (None, 0) or "cannot be used with" not in (read(where, "quiet-format", "err") or ""):
            raise AssertionError(f"-q --format json was not refused as a conflict: {read(where, 'quiet-format', 'err')!r:.200}")
        return
    if check == "filter-none":
        if code(where, "filter-none") in (None, 0):
            raise AssertionError(f"accepted: {read(where, 'filter-none', 'out')!r:.200}")
        if "no row matches" not in (read(where, "filter-none", "err") or ""):
            raise AssertionError(f"refused without saying so: {read(where, 'filter-none', 'err')!r:.200}")
        if lines(read(where, "filter-none", "out")):
            raise AssertionError("a refused filter still printed rows")
        return
    if check in ("filter", "filter-case", "filter-and", "filter-label", "quiet-filter"):
        verify_filter(where, check, table, columns, flags)
        return
    raise AssertionError(f"unknown check {check}")


def verify_filter(where, check, table, columns, flags):
    """A filtering variant keeps exactly the rows its filters describe."""
    if check not in flags:
        if check == "filter-label":
            return
        raise AssertionError(f"no column gave a stable value to filter on: {columns}")
    wanted = pairs(flags[check])
    kept = [r for r in table if all(keeps(r, k, v) for k, v in wanted)]
    if not kept:
        if code(where, check) in (None, 0) or "no row matches" not in (read(where, check, "err") or ""):
            raise AssertionError(f"filters {wanted} hold for no row together, yet were not refused: "
                                 f"{read(where, check, 'out')!r:.300}")
        return
    if check == "quiet-filter":
        same(lines(ok_run(where, check)), [shown(r, columns[0]) for r in kept], "filtered ids")
        return
    expect_rows(where, check, kept)
    if check == "filter" and os.environ.get("LISTING_MIN_ROWS", "1") != "1" and len(kept) == len(table):
        if len({shown(r, wanted[0][0]) for r in table}) > 1:
            raise AssertionError("the filter excluded nothing though the column differs across rows")


def verify_default(where, table, columns):
    """The unshaped output: an aligned table, or tab-separated lines where that is the contract."""
    out = lines(ok_run(where, "default"))
    if os.environ.get("LISTING_DEFAULT") == "tsv":
        firsts = [line.split("\t")[0] for line in out]
        if any(len(line.split("\t")) != len(columns) for line in out):
            raise AssertionError(f"not {len(columns)} tab-separated fields per line: {out!r:.300}")
        same(firsts, [shown(r, columns[0]) for r in table], "first fields")
        return
    if not out or out[0].split() != columns:
        raise AssertionError(f"header {out[:1]} is not {columns}")
    if len(out) != len(table) + 2 or set(out[1]) != {"─"}:
        raise AssertionError(f"want header, rule and {len(table)} rows, got {len(out)} lines")
    same([" ".join(AGE.sub("<age>", line).split()) for line in out[2:]],
         [" ".join(calm(" ".join(shown(r, c) for c in columns)).split()) for r in table], "table rows")


def main():
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    if command == "checks":
        print(" ".join(CHECKS))
    elif command == "describe":
        print(CHECKS[sys.argv[2]])
    elif command == "plan":
        plan(sys.argv[2])
    elif command == "verify":
        try:
            verify(sys.argv[2], sys.argv[3])
        except (AssertionError, ValueError, KeyError, IndexError, json.JSONDecodeError) as failure:
            print(failure)
            sys.exit(1)
    else:
        print(__doc__ or "usage: listing.py plan|checks|describe|verify")
        sys.exit(2)


if __name__ == "__main__":
    main()
