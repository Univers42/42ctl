# 9. Shared environments

An environment is where a team keeps what it shares: single credentials, and whole project trees.
Its commands need a **session**, a **contract** and your **identity**.

## 9.1 How an environment's key works

Each environment has its own key pair, the **scope key**. Anything shared in the environment is sealed
to its public half. Its private half is never stored in the clear anywhere: it is **wrapped** — sealed
again — separately for each member who may use it, with their own public key.

```text
                         ┌────────────── the authority ───────────────┐
 ada: env init  ───────▶ │ prod's PUBLIC scope key, epoch 1           │
                         │ grants: devs write · auditors read         │
 bea, cid: keys enroll ▶ │ bea's and cid's public keys                │
                         └────────────────────────────────────────────┘
 ada: env keys sync ───▶ ┌────────────── the vault server ────────────┐
                         │ prod's private key, wrapped to ada (write)  │
                         │                     wrapped to bea (write)  │
                         │                     wrapped to cid (read)   │
                         │ secrets and trees sealed to prod's key      │
                         └────────────────────────────────────────────┘
```

- **`env init`** (an administrator, once per environment) generates the key pair, publishes the public
  half to the authority and wraps the private half to the administrator.
- **`keys enroll --org ORG`** (each member, once per organisation) publishes the member's public keys.
- **`env keys sync`** (an administrator) wraps the private half to every member a grant authorizes who
  has enrolled and does not hold it yet. Each wrap records the member's role, so a key wrapped for
  reading cannot be used to write.
- **`env keys rotate`** (an administrator) replaces the key pair with a new one — a new **epoch** —
  and wraps it only to the members authorized now (§9.7).

Anybody may *seal to* the public key. Only a member holding a wrap can *open* what was sealed.

## 9.2 Setting an environment up

```sh
$ 42ctl env create --project api --name prod
$ 42ctl team grant --org acme --team devs --project api --role write --env prod
$ 42ctl env init --org acme --project api --env prod
$ 42ctl env keys sync --org acme --project api --env prod
$ 42ctl env keys ls --org acme --project api --env prod
Member                                Pubkey  Provisioned  State
6575d73b-034d-4328-858f-abc0cb1bcd08  true    true         active
21b57d36-dd14-426a-839a-d5af85e978e4  true    false        pending-provision
4e55a44a-1857-4aa0-b710-cc2e46f82e01  false   false        pending-enrollment
```

`env keys ls` is the command that answers *"why can my colleague not read prod?"*:

| State | Meaning | Remedy |
|---|---|---|
| `active` | holds the current key | — |
| `pending-provision` | authorized and enrolled, not yet wrapped | an administrator runs `env keys sync` |
| `pending-enrollment` | authorized, but has not published a public key | **they** run `keys enroll --org acme` |

Run `env keys sync` whenever somebody new is granted or enrolls. It reports how many members it
provisioned and how many it skipped for want of a public key.

## 9.3 Shared secrets

```sh
$ printf 's3cret' | 42ctl env secret set DB_PASSWORD --org acme --project api --env prod
set DB_PASSWORD (v1)
$ 42ctl env secret get DB_PASSWORD --org acme --project api --env prod
s3cret
$ cat ./server.key | 42ctl env secret set tls/server.key --org acme --project api --env prod
```

`env secret set` reads the value from standard input — any bytes, a whole file included — and needs a
**write** role; `env secret get` writes
it to standard output and needs any role. Setting a path again stores a new version.

## 9.4 Sharing a whole tree

### Publishing: `env push`

```sh
$ cd ~/src/inception
$ 42ctl env push --org acme --project api --env prod
pushed 11 shared and 1 private file(s) to environment 'prod'
$ 42ctl env push --org acme --project api --env prod --label app=inception --label stage=prod
$ 42ctl env push --org acme --project api --env prod --private 'secrets/me.*'
```

`env push` scans the project exactly as `push` does — the same marker, the same patterns, the same
`secrets/` directories and skip rules (chapter 7) — and needs a **write** role. It then:

1. separates the files that are **private** (§9.5) from the shared ones;
2. seals every shared file to the environment's key, and every private file to you alone;
3. writes the shared manifest, then your private manifest, **last**.

Each push **replaces** the environment's tree with the tree on your disk. A file you do not have is no
longer in the environment after your push; there is no merge. Pull before you push when others
publish to the same environment.

### Inventory: `env files`

```sh
$ 42ctl env files --org acme --project api --env prod
Path                          Size  Mode  Kind  Private  Labels
secrets/ca.crt                806   0644  file  false    {"app":"inception","stage":"prod"}
secrets/server.key            302   0600  file  false    {"app":"inception","stage":"prod"}
srcs/.env                     1896  0644  file  false    {"app":"inception","stage":"prod"}
srcs/.env.local               27    0644  file  true     {"app":"inception","stage":"prod"}
$ 42ctl env files --org acme --project api --env prod --filter Private=true -q
srcs/.env.local
```

`env files` reads the manifests and fetches no file. `Size` and `Mode` are as recorded at push;
`Private=true` marks your own private files. Another member's private files are not listed at all.

### Restoring: `env pull`

```sh
$ git clone https://github.com/acme/api.git && cd api
$ 42ctl env pull --org acme --project api --env prod
env pull  dry-run — re-run with --apply to write
secrets/ca.crt 806 byte(s)
secrets/server.key 302 byte(s)
srcs/.env 1896 byte(s)
11 file(s) from environment 'prod'
$ 42ctl env pull --org acme --project api --env prod --apply
$ 42ctl env pull --org acme --project api --env prod --only 'secrets/*.txt' --apply
$ 42ctl env pull --org acme --project api --env prod --apply --backup
```

`env pull` needs any role. Without `--apply` it is a **preview**: it names every file and its size, and
writes nothing — not even a directory. With `--apply` it restores every file by the rules of chapter 7
§7.3, with two differences that matter on a shared path:

- **Modes are narrowed to the owner.** Anybody who may write the environment may write its manifest,
  so a mode is hostile input: an entry asking for `0777` on a private key would otherwise restore it
  world-readable with perfectly correct bytes. `0644` comes back `0600`, `0755` comes back `0700`.
- **There is no reconciliation.** Unlike the personal `pull`, `env pull --apply` **overwrites** the
  files it restores. Pass `--backup` to keep each file it replaces as `NAME.bak` — `server.crt.bak`
  and `server.key.bak` side by side — and preview first.

`--only PATTERN` (repeatable) restores only the files whose **relative path** matches: `secrets/*`,
`secrets/*.txt`, `*.crt`, `srcs/.env`. Files not selected are left exactly as they are on disk. A
partial restore reports how many files it left out, and **a pattern that matches nothing is an error**,
so a typo cannot pass for a clean restore.

### What a restore guarantees

- **One push's tree, whole.** Each file is read at the revision the manifest recorded. A push that
  dies part-way — a network failure, an unreachable object store — has written files the manifest does
  not name, and a pull never sees them.
- **Concurrent pushes are refused, not merged.** When two members push the same environment at the
  same time, the push that lands second is refused and publishes nothing:

  ```text
  error: environment 'prod' was pushed by somebody else while this push ran, so nothing of this
  push was published — `42ctl env pull` shows what they published, and pushing again replaces it
  with this tree
  ```

- **Paths cannot escape.** A stored path that is absolute or climbs out with `..` stops the restore
  before anything is written, and nothing is written through a symbolic link.

## 9.5 Private files inside a shared environment

Some files in a project tree belong to one person: a `.env.local` with personal overrides, a personal
key. They travel **with** the tree but are sealed to the person who pushed them.

- **Every file matching `*.local` is private**, always, with or without flags.
- `--private PATTERN` (repeatable) makes more files private, matched on the relative path.
- A private file is sealed to **you** and named only in **your** private manifest. Another member who
  pulls receives neither its bytes nor its name, and their `env files` does not list it.
- **Your** `env pull` restores both sets. Where your private file and a shared file have the same path,
  **yours wins**, and the pull says so: `srcs/.env.local: your private copy shadows the shared one`.
- A private file is sealed whole: one larger than about 4 MiB is refused **before anything is uploaded**,
  naming the file. Push it shared, or split it.
- A private manifest is honoured only if **you** wrote it. A writer can seal bytes to your public key;
  a "private" file somebody else authored is refused, and the restore fails rather than write it.
- Private files survive rotation: they stay sealed to you, and your pull keeps finding them (§9.7).

Every member can keep their own `srcs/.env.local` in the same environment; each gets back their own.

## 9.6 Labels

```sh
$ 42ctl env push --org acme --project api --env prod --label app=inception --label team=backend
$ 42ctl env files --org acme --project api --env prod --filter label=team=backend
$ 42ctl env files --org acme --project api --env prod --format '{{.Path}} {{.Labels.app}}'
```

`--label KEY=VALUE` (repeatable) tags **every** file of that push, shared and private. Labels are per
push, not per file: to label two groups of files differently, keep them in different environments.

## 9.7 When somebody leaves

```sh
$ 42ctl group member rm --group GROUP_ID --user cid@example.com
$ 42ctl env keys rotate --org acme --project api --env prod
new_epoch 2
resealed  13
rewrapped 2
rotated scope (revoked members lose access by absence at the new epoch)
```

Removing the person stops them being *authorized*. **Rotating** is what stops them *reading anything
written from now on*: the environment gets a new key pair, wrapped only to the members authorized at
that moment, and its content is moved to the new key. Rotation:

- re-seals every shared secret and every shared file to the new key, at the revisions the manifest
  names, and re-chunks large files under the new key;
- leaves each member's **private** files where they are — they were never sealed to the environment's
  key — and each member's pull keeps restoring their own;
- carries across a push that lands **while** it runs (the summary then shows `carried_late`), and a
  push that lands **after** it finished is refused with *had its key rotated while this push ran — push
  again*.

The person who left keeps what they could already read. Rotate after every departure that matters, and
change at their source any credential they could have copied — a database password, an API token —
because rotation re-seals values, it does not change them.

## 9.8 Large files in an environment

A file larger than about 4 MiB is chunked to the object store as in chapter 7, sealed to the
**environment**, so that every member can open it. Every member who pushes or pulls such a file needs
the object store configured in their profile, and its credential in `FT_S3_KEY` / `FT_S3_SECRET`.
Identical content pushed by two members is stored once.

## 9.9 Personal or shared?

| | `push` / `pull` | `env push` / `env pull` |
|---|---|---|
| Sealed to | you | the environment (private files: you) |
| Readable by | you, on any machine with your identity | every member granted the environment |
| Needs | a contract | a session, a contract, a grant, a wrap |
| Restore | reconciles: fast-forward, keep local, conflict markers | overwrites; `--backup` keeps copies |
| Subset | — (use `--at` for history) | `--only PATTERN` |
| Concurrent writers | you only | refused, never merged |
