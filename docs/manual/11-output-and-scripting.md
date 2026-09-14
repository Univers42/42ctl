# 11. Output and scripting

## 11.1 Where output goes

- **Standard output** carries results only: a secret's value, a listing, a JSON document.
- **Standard error** carries everything else: progress, warnings, notices such as a private file
  shadowing a shared one, and errors. `--format json` therefore stays parseable.
- Colour is used only on a terminal. `NO_COLOR=1` turns it off; `CLICOLOR_FORCE=1` keeps it when piped.

## 11.2 Shaping a listing

Every command that lists rows — `vault ls`, `db ls`, `note ls`, `org member ls`, `team ls`, `project
ls`, `env ls`, `project grant ls`, `env keys ls`, `env files`, and the `cloud` listings — takes the same
three options.

### `--format`

```sh
$ 42ctl project ls --org acme --format json
$ 42ctl project ls --org acme --format '{{.Slug}} {{.ID}}'
$ 42ctl env files --org acme --project api --env prod --format '{{.Path}} {{.Labels.app}}'
$ 42ctl org member ls --org acme --format '{{json .}}'
$ 42ctl env files --org acme --project api --env prod --format 'table {{.Path}}\t{{.Size}}'
```

| Form | Output |
|---|---|
| no `--format` | the aligned table (for `vault ls` and `db ls`: tab-separated `path version updated`) |
| `json` | one JSON array of row objects |
| `{{.Field}}` template | one line per row; fields are the column names as printed; `{{.Labels.key}}` descends; an unknown field renders as nothing |
| `{{json .}}` | one JSON object per line |
| `table TEMPLATE` | an aligned table whose headings come from the template; separate columns with `\t` |

### `--filter`

```sh
$ 42ctl org member ls --org acme --filter Role=admin
$ 42ctl env files --org acme --project api --env prod --filter Private=true
$ 42ctl env files --org acme --project api --env prod --filter label=app=inception --filter label=stage=prod
```

`--filter KEY=VALUE` keeps the rows whose column `KEY` — matched whatever its case — shows exactly
`VALUE`. `label=K=V` matches a label. Repeated filters must **all** hold.

Two refusals protect scripts: a `KEY` that names no column is refused, naming the columns that exist;
and **a filter that keeps no row is an error** (`no row matches …`, exit status 1), so a typo can never
read as "there is nothing here".

### `-q`, `--quiet`

Prints only the first column, one value per line, ready for composition. It cannot be combined with
`--format`.

```sh
$ 42ctl vault rm $(42ctl vault ls dev/ -q)
$ 42ctl org member rm --org acme --user $(42ctl org member ls --org acme -q --filter Role=member)
$ 42ctl project grant rm --org acme --project api --grant $(42ctl project grant ls --org acme --project api -q --filter Kind=group)
```

### The columns

| Listing | Columns |
|---|---|
| `vault ls`, `db ls` | `Path` `Version` `Updated` |
| `note ls` | `Note` |
| `org member ls` | `UserID` `Role` `Joined` |
| `team ls`, `project ls` | `ID` `Slug` `Name` |
| `env ls` | `ID` `Name` |
| `project grant ls` | `GrantID` `Kind` `Grantee` `Role` `Env` |
| `env keys ls` | `Member` `Pubkey` `Provisioned` `State` |
| `env files` | `Path` `Size` `Mode` `Kind` `Private` `Labels` |

The `cloud` listings are described in chapter 12; `42ctl COMMAND --filter NoSuchColumn=x` names any
listing's columns.

## 11.3 Exit status

| Status | Meaning |
|---|---|
| `0` | success |
| `1` | the command ran and failed: refused, not found, a conflict, a filter matching nothing, a `cloud health` check failing |
| `2` | the command line itself is wrong: an unknown command or option, a missing required argument |
| `141` | the reader of standard output went away (`42ctl … \| head -1`); nothing is printed |

Commands that act on several targets — `vault rm`, `note rm`, `org member rm`, `team member rm`, `group
member rm`, `project grant rm` — attempt every target, then exit `1` once if any failed, naming them.

## 11.4 Running without a terminal

42ctl prompts for three things. In automation, supply each through the environment:

| Prompt | Variable |
|---|---|
| keystore passphrase | `FT_PASSPHRASE` |
| account password | `FT_PASSWORD` |
| an invitation token at sign-up | `FT_REGISTER_TOKEN` (or `--token`) |

A second factor's emailed code cannot be supplied non-interactively; an account used by automation
should not have one (chapter 10, *CI*).

Keep each automated identity's files apart from a person's with `FT_CONFIG` and `FT_KEYSTORE`, or give it
its own `HOME`. Every command is safe to run concurrently in different directories; two `env push` to the
same environment at the same time are resolved as chapter 9 describes.

## 11.5 Useful compositions

```sh
$ 42ctl env secret get CI_REGISTRY_TOKEN --org acme --project api --env prod | docker login registry.example.org -u ci --password-stdin
$ 42ctl env files --org acme --project api --env prod --format json | jq -r '.[] | select(.Private) | .Path'
$ 42ctl env keys ls --org acme --project api --env prod --filter State=pending-enrollment -q
$ 42ctl cloud health --no-wake && echo deployment healthy
```
