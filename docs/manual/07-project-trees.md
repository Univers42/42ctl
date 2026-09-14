# 7. Project trees

A project's secrets are rarely one value. They are files — `srcs/.env`, `secrets/db_password.txt`,
`secrets/server.key` — at paths the project's own tooling expects. This chapter explains how 42ctl
decides which files those are, how it records where they were, and how it puts every one of them
back at exactly the right place. It then covers the **personal** commands, `push` and `pull`, which
seal a tree to you alone. Sharing a tree with a team uses the same finding and restoring rules and is
covered in chapter 9.

## 7.1 The project marker

A project is a directory holding **`.42ctl/project.json`**:

```json
{
  "project_id": "43c3a63c-f358-5053-a29d-2c0b9e722fcc",
  "patterns": [
    "*.env*",
    "*.secrets"
  ]
}
```

| Field | Meaning |
|---|---|
| `project_id` | the project's name in your vault. Every machine must use the **same** id to see the same tree. |
| `patterns` | file-**name** patterns of the files that are secrets (§7.2) |

The first `push` or `note add` in a directory without a marker **creates** one, with the default
patterns and an id derived from that directory's absolute path. Because a different path on another
machine would derive a different id, **commit `.42ctl/project.json` to the project's repository**. It
contains no secret. Alternatively, pass the same `--project NAME` everywhere; a name given that way
replaces the marker's id *and* its patterns, which then fall back to the defaults.

### Where the project root is

42ctl finds the root the way git finds a repository: the **nearest directory holding
`.42ctl/project.json`, looking upward from the current directory, and no higher than the root of the
git repository you are in**. So `42ctl push` or `42ctl env pull` run from `srcs/` acts on the whole
project, and restores `secrets/` at the root, not under `srcs/`. Outside a git repository only the
current directory is considered: a marker left in a parent directory by accident is never used.

## 7.2 Which files are secrets

A file is taken when **either** of these holds:

1. **Its name matches one of the marker's `patterns`.** The defaults are `*.env*` (which matches
   `.env`, `srcs/.env`, `.env.production`, `.env.example`, `.env.local`) and `*.secrets`.
2. **It lives anywhere under a directory named `secrets/` or `.secrets/`**, at any depth, whatever
   its name. Docker secrets are named for what they hold — `db_password.txt`, `server.key` — never for
   being secret, so the directory is what marks them.

and **none** of these holds:

| Skipped | Why |
|---|---|
| symbolic links | a link could point anywhere on the machine |
| the `.42ctl/` directory | 42ctl's own state |
| names containing `.bak`, or ending in `.stale` | backups and deliberately stale copies are not secrets |
| the directories `node_modules`, `.git`, `target`, `dist`, `build`, `vendor`, `.cache`, `coverage`, `__pycache__`, `.next`, `.venv`, `.claude`, `.vault` | dependency, build and tool trees |

One exception pierces the skipped directories: **a git repository directly inside one** — a submodule
at `vendor/libfoo/`, say — is scanned as a project of its own, so an orchestrator repository does not
silently lose its submodules' secrets.

When a skipped directory holds something that *would* have been taken, `push` names it, so an
omission is a question you are asked rather than a surprise at restore time.

### Patterns

A pattern's only wildcard is `*`, which matches any run of characters. Every other character matches
itself, and matching is case-sensitive.

- In the **marker**, a pattern is matched against a file's **name** only. A pattern containing `/`
  can never match a name, so it is refused when the marker is read: keep such a file under `secrets/`
  instead.
- In **`env pull --only`** and **`env push --private`** (chapter 9), a pattern is matched against the
  file's **relative path**, where `*` also matches `/`: `secrets/*`, `secrets/*.txt`, `*.crt`,
  `srcs/.env`.

To take more files, add patterns to the marker and commit it:

```json
{ "project_id": "…", "patterns": ["*.env*", "*.secrets", "*.pem", "credentials.json"] }
```

## 7.3 What is stored, and where it is put back

For every file taken, 42ctl records in the **manifest** its relative path (always with `/`, relative
to the project root), its size, its mode, and the revision it was stored at. The file's bytes are
sealed separately, under an **opaque id** that reveals nothing of the name.

```text
 on your disk                     in your vault (what the server sees)
 srcs/.env                  ──▶   __42ctl/b/PROJECT_ID/3f9a…   sealed bytes
 secrets/db_password.txt    ──▶   __42ctl/b/PROJECT_ID/b71c…   sealed bytes
 secrets/server.key         ──▶   __42ctl/b/PROJECT_ID/0de4…   sealed bytes
                                  __42ctl/m/PROJECT_ID         sealed manifest:
                                     srcs/.env           → 3f9a…  rev 3  mode 0644  1896 bytes
                                     secrets/server.key  → 0de4…  rev 1  mode 0600   302 bytes
```

The server sees the project id, the opaque ids, sizes of ciphertext and when things change. It never
sees a file name, a path, a mode or a byte of content. (A shared environment stores the same shape
under the environment's own space, chapter 9.)

A restore reads the manifest, and for each entry:

1. **validates the stored path before touching the disk.** A path that is absolute, contains `..`,
   or could otherwise escape the project root is an error — the whole restore stops, nothing is
   sanitised into a different place;
2. refuses to write **through a symbolic link** in any existing parent directory;
3. creates each missing parent directory with mode **0700**, leaving existing directories as they are;
4. writes the file atomically — to a temporary name beside it, then renamed into place — so a
   restore interrupted half-way never leaves a truncated file;
5. sets its mode: for a personal tree the recorded mode; for a shared environment the recorded mode
   **narrowed to the owner** (chapter 9);
6. fetches the bytes **at the revision the manifest recorded**, so what comes back is one consistent
   tree even if a later push was interrupted.

Files are processed in path order. The bytes come back identical: 42ctl adds, removes and converts
nothing, line endings included.

## 7.4 Personal sync: `push`

```sh
$ cd ~/src/inception
$ 42ctl push
$ 42ctl push --prune
```

`push` scans the project (§7.2), seals each file to your identity, reports
`pushed N file(s) + manifest for project PROJECT_ID`, and writes the manifest **last** — so an interrupted push leaves stored files that nothing names, never a manifest that names
files that were not stored.

Without `--prune`, the manifest keeps entries for files that have since disappeared from your disk, so
a file deleted on one machine is not deleted from the vault. `--prune` makes the stored tree mirror
the disk exactly.

## 7.5 Personal sync: `pull`

```sh
$ 42ctl pull                          # a dry run: what would change, and nothing is written
$ 42ctl pull --apply                  # write
$ 42ctl pull --apply --backup         # keep NAME.bak of every file that is replaced
$ 42ctl pull --apply --force          # take the stored version even where you changed the file
$ 42ctl pull --at 7                   # preview the tree as manifest version 7 recorded it
$ 42ctl pull --at 7 --apply --force   # restore it over local changes
```

**`pull` never writes without `--apply`.** The dry run lists, for every file, what `--apply` would do.

`pull` reconciles each file three ways, using `.42ctl/sync.json` — the state of your last push or pull
on this machine — as the common ancestor, exactly as git uses its index:

| Situation | Dry-run label | What `--apply` does |
|---|---|---|
| no local file | `would create` | writes the stored file |
| local equals stored | `in sync` | nothing |
| you have not changed it since the last sync | `would fast-forward (local unchanged)` | writes the stored file |
| you changed it; the stored one has not changed | `would keep local (ahead)` | nothing — push it when ready |
| both changed, text | `would conflict (write markers)` | writes the file with `<<<<<<< local (on disk)` / `=======` / `>>>>>>> remote` markers |
| both changed, binary | `would conflict (binary sidecar)` | keeps your file, writes the stored one beside it as `NAME.remote` |

Resolve a text conflict by editing the file, then `push`. Delete a `NAME.remote` sidecar once you have
chosen: it is not skipped by the scan, and would otherwise be pushed. `--force` skips reconciliation
and takes every stored file; combine it with `--backup` to keep what it replaces.

`--at N` restores the tree as manifest version `N` recorded it, fetching each file **at the revision
that version recorded** — old names with old contents, never old names with today's contents.

## 7.6 Large files

A file larger than **4 MiB minus 64 KiB** does not fit in one envelope. 42ctl splits it into chunks,
seals each chunk, and stores them in an S3-compatible object store; the vault keeps only the chunk
list. The chunks' names are keyed digests of their contents, so the store learns nothing, and a
second push of an unchanged file uploads nothing.

Configure a store once per profile (chapter 4) and provide its credential in the environment:

```sh
$ 42ctl config endpoint --blobstore https://s3.example.org --bucket acme-chunks
$ FT_S3_KEY=AKIA… FT_S3_SECRET=… 42ctl push
```

Without a configured store, a push containing a large file is **refused, before anything is
uploaded**, and says which setting is missing — it never stores part of a tree.

Chunks are never overwritten, so replaced versions accumulate in the store. `vault gc` removes the
ones no version of any manifest still refers to:

```sh
$ 42ctl vault gc                       # a dry run: what would be removed
$ 42ctl vault gc --apply
$ 42ctl vault gc --apply --grace-hours 72
```

It walks every version of every manifest, refuses to run when it finds none, and never removes a chunk
younger than the grace period (24 hours by default), which could belong to a push still in progress.
