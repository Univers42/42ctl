# 6. Personal secrets and notes

Everything in this chapter is sealed to **your identity** and needs a **contract**
(`auth login --tenant`). Nobody else can read it — not a teammate, not the operator — unless you
share one secret explicitly.

## 6.1 The personal vault

The vault maps **paths** to values. A path is any `/`-separated name you choose, such as
`app/DATABASE_URL` or `tools/deploy.key`. A value is any sequence of bytes up to about 4 MiB: a
password, a whole file, a binary key.

### Storing

```sh
$ printf 'postgres://app:s3cret@db/app' | 42ctl vault set app/DATABASE_URL
pushed app/DATABASE_URL (v1)
$ 42ctl vault set tools/deploy.key --file ./deploy.key
pushed tools/deploy.key (v1)
```

`vault set` reads the value from standard input, or from `--file FILE`. Prefer `--file` or a pipe
to typing a secret as an argument: arguments are visible to other users in the process list and
are kept in your shell history. Setting a path that exists stores a **new version**; old versions
are kept.

### Reading

```sh
$ 42ctl vault get app/DATABASE_URL
postgres://app:s3cret@db/app
$ 42ctl vault get app/DATABASE_URL --version 1
```

The value is written to standard output byte for byte, with no trailing newline added, so
`42ctl vault get tools/deploy.key > deploy.key` reproduces a binary file exactly. `--version N` reads
an older version (`0`, the default, is the latest).

### Listing

```sh
$ 42ctl vault ls
tools/GITHUB_TOKEN	2	1789396676
tools/deploy.key	1	1789396676
$ 42ctl vault ls app/
$ 42ctl vault ls app/ -q
app/DATABASE_URL
$ 42ctl vault ls --all
```

Without `--format`, each line is `PATH`, the latest version and the Unix time of the last change,
separated by tabs; with `--format` the columns are `Path`, `Version` and `Updated`. 42ctl keeps its own records in the vault too — your
notes and the manifests of your pushed trees, under `__42ctl/`. **They are left out unless `--all`**,
because this listing is what you feed to `vault rm`, and removing one of them loses the notes or the
pushed tree it holds.

### Removing

```sh
$ 42ctl vault rm app/DATABASE_URL tools/deploy.key
$ 42ctl vault rm $(42ctl vault ls dev/ -q)
```

Every path is attempted even if an earlier one fails, and the command fails once at the end naming
the paths it could not remove. A path under `__42ctl/` is refused; remove a note with `note rm`.

> Removal is permanent. There is no bin and no undo.

### Rotating

```sh
$ 42ctl vault rotate app/DATABASE_URL
```

Re-seals the current value under a fresh data key, as a new version. The value does not change.
Rotate a secret's **value** — a new database password — by setting it again.

## 6.2 Sharing one secret with one person

```sh
$ 42ctl keys export-pub                                     # the recipient runs this, and sends you the address
$ 42ctl vault share app/DATABASE_URL --to v42:yTN_EpS2M_nD1eB7UaTiUU1TzfdDopHg-Vg5P555_f9hFrwquLu_MxgdmGsuvTjovBNs_VzMcMi-fx8nxcWDHA
```

`share` decrypts your copy locally and seals a new copy to the recipient's key. The copy is placed
in **their** vault, under your principal: they read it with

```sh
$ 42ctl vault get shared/5aa9cd1b2cd46c6443a220d1ecaa520d/app/DATABASE_URL
```

where the long hexadecimal part is **your** principal (`42ctl auth whoami`). The copy is a snapshot:
changing your secret afterwards does not change theirs — share it again. Sharing gives them a copy,
so it cannot be taken back.

For anything a whole team uses, do not share one by one: use an environment (chapter 9).

## 6.3 Importing and exporting `.env` files

```sh
$ 42ctl vault import srcs/.env --prefix inception
$ 42ctl vault export --prefix inception
DOMAIN_NAME=ada.42.fr
MYSQL_USER=wpuser
```

`import` stores each `KEY=VALUE` line as the secret `PREFIX/KEY` (or `KEY` with no prefix). It skips
blank lines and lines starting with `#`, and takes each value **literally** after trimming spaces:
it does not remove quotes, expand variables, understand an `export ` prefix, or join multi-line
values. For a file that must come back exactly as written, store the whole file instead —
`vault set NAME --file FILE` — or push the project tree (chapter 7).

`export` prints `KEY=value` for every secret under the prefix, where `KEY` is the last segment of
the path. Two secrets with the same last segment under different prefixes export under the same
`KEY`, so export one prefix at a time. 42ctl's own records are never exported.

## 6.4 The audit chain

```sh
$ 42ctl vault audit
seq 1 ts 1789396675 push 0ff764a57f9a6adbbf2760656f19f87e/tools/GITHUB_TOKEN hash=af67620b453f
seq 2 ts 1789396676 push 0ff764a57f9a6adbbf2760656f19f87e/tools/deploy.key hash=b0082e452f97
seq 3 ts 1789396676 rotate 0ff764a57f9a6adbbf2760656f19f87e/tools/GITHUB_TOKEN hash=7775827e0514
$ 42ctl vault audit --since 1789390000
```

Every write to your vault appends an entry to a hash-chained log on the server: its sequence number,
Unix time, action (`push`, `rotate`, …), the target under your principal, and the start of the entry's
hash. `--since EPOCH` starts at a Unix time.

> The server links each entry to the one before it, but **no client verifies that chain yet**: 42ctl
> prints the entries and does not check the links. Treat the audit log as a record of what happened,
> not as proof that nobody altered it.

## 6.5 Records: `db`

```sh
$ 42ctl db ls
$ 42ctl db get app/DATABASE_URL
```

`db` is the read-only, record-level view of the same store: `db ls` lists **everything** readable,
42ctl's own records included, and `db get` decrypts one. There is no `db set` or `db rm`.

## 6.6 Notes

Notes are small encrypted documents — a runbook, an onboarding page — attached to a **project**.

```sh
$ 42ctl note add runbook.md --file ./RUNBOOK.md
$ printf 'Rotate the API key on the 1st.' | 42ctl note add reminders
$ 42ctl note ls
runbook.md
reminders
$ 42ctl note get runbook.md
$ 42ctl note rm reminders
```

Inside a directory that holds a `.42ctl/project.json` marker (chapter 7), the project is taken from
it. Elsewhere, name it: `--project api`. A note is sealed to you like everything in this chapter; it
is not part of an environment, and `env pull` does not restore notes.

## 6.7 Large values

A single value above about 4 MiB is refused by the vault. Large **files** in a project tree are split
into chunks and stored in an object store instead (chapter 7, *Large files*).
