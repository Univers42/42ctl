# Using the vault through `42ctl`

A working manual for the whole surface: identity, accounts, organisations, teams, projects,
environments, permissions, secrets, project sync, previews, inspection, and deletion.

Everything here was run against a live deployment. Where a thing does not exist, this says so
rather than describing what it would look like — see [What you cannot do yet](#what-you-cannot-do-yet).

---

## 1. The one thing to understand first

**`42ctl` holds two unrelated credentials, and every "permission denied" you will hit is
really about which one is missing.**

| | The **session** | The **contract** |
|---|---|---|
| What it is | a bearer token for an ACCOUNT | a signed token binding your KEY to a tenant |
| Talks to | the authority (HTTP/JSON) | vault42-server (gRPC) |
| You get it with | `auth login --password` (or `--github`) | `auth login --tenant <name>` |
| Stored at | `session-<profile>.tok` | `contract-<profile>.tok` |
| Needed by | `org` `team` `group` `env` `project` `invite` `account` | `vault` `push` `pull` `note` `db` |

The scope-key verbs (`vault env-init`, `sync-keys`, `push-env`, …) need **both**, because they
bridge the two planes: membership comes from the authority, the wrapped keys live in the vault.

Get both in one command:

```sh
42ctl auth login --password --email you@example.com --tenant yourname
```

A third secret is neither of those: the **passphrase** that unlocks your local keystore. It
never leaves your machine and nothing can recover it.

---

## 2. Install and point it somewhere

```sh
42ctl config show                     # the resolved endpoints for this profile
42ctl config endpoint \
  --server    https://vault42-server.fly.dev \
  --authority https://vault42-authority.fly.dev
```

The two defaults above are built in, so a fresh install usually needs no configuration.

**Profiles** are independent worlds — endpoints, session and contract are per profile, and
nothing leaks between them:

```sh
42ctl config profile                  # list; the active one is marked
42ctl config profile staging          # switch to / create 'staging'
42ctl --profile staging vault ls      # one-off without switching
```

---

## 3. First five minutes

```sh
42ctl keys init                                              # local identity, prompts a NEW passphrase
42ctl keys export-pub                                        # your public address — share freely
42ctl auth signup --email you@example.com                    # create the account
42ctl auth login --password --email you@example.com --tenant yourname
42ctl auth whoami                                            # principal + address + 'contract: bound'
```

If the deployment gates account creation, signup takes the invite token your operator holds:

```sh
42ctl auth signup --email you@example.com --token <INVITE>   # or export FT_REGISTER_TOKEN
```

> Losing the passphrase loses everything sealed to that identity. By design — the server
> cannot help. Back the keystore up with `keys escrow` before you rely on it.

---

## 4. Identity — `keys`

| Command | What it does |
|---|---|
| `keys init` | Generate X25519 + Ed25519, sealed by a passphrase. `--force` overwrites (old identity unrecoverable). |
| `keys export-pub` | Print your `v42:…` address. Others need this to `vault share` with you. |
| `keys enroll --org <slug>` | Publish your PUBLIC keys to an org so its admins can wrap environment keys to you. Run once per org. |
| `keys escrow --email <you>` | Back the passphrase-sealed keystore up to the server, gated by an emailed code. The server stores ciphertext only. |
| `keys recover --email <you>` | Restore that keystore on a new machine: code → fetch → unlock locally. |

**Scenario — second machine.** `keys escrow` on the first, then on the second:
`keys recover`, enter the emailed code, type the same passphrase. You now have one identity on
two machines, and everything you pushed is readable on both. Without escrow, machine two is a
different person as far as the vault is concerned.

---

## 5. Account — `auth`, `account`

| Command | What it does |
|---|---|
| `auth signup --email <e> [--token <t>]` | Create the account. Deliberately says the same thing whether or not the address was already registered. |
| `auth login --password --email <e>` | Sign in, save the session. Add `--tenant <n>` to take the contract in the same command. |
| `auth login --github` | Same, via the GitHub device flow (needs a GitHub app on the authority). |
| `auth login --tenant <n>` | Claim a tenant and save the contract. **Needs a session already** — the contract is issued to an account. |
| `auth me` / `account show` | Account id, email, whether a second factor is required. |
| `auth whoami` | Local principal, address, and whether a contract is bound. |
| `auth status` | Whether this profile is logged in. |
| `auth passwd` | Change the password — revokes every session, including the one you are using. |
| `auth mfa --on` / `--off` | Turn the email second factor on or off for this account. |
| `auth logout` | Forget the saved contract and session for this profile. |
| `account delete --yes` | **Irreversible.** See below. |

### The second factor is opt-in, per account, and reversible

Off unless you turn it on. When on, every sign-in asks for a 6-digit emailed code before it
mints a session — the check lives inside the one function that mints one, so no login path can
skip it.

```sh
42ctl auth mfa --on         # asks for a code first, then enables
42ctl auth me               # 'second factor  required'
42ctl auth mfa --off        # asks for a code first, then disables
```

Both directions ask for a code on purpose: enabling without proving you hold the mailbox would
let a stolen session lock you out, and disabling without it would make the factor removable by
exactly the attacker it exists to stop. Needs the authority to have mail configured.

### `account delete` — read this before running it

Refuses without `--yes`. It removes the account, every session, and its org memberships, and
**releases every tenant name it claimed** — so anyone may claim those names afterwards and you
cannot take them back. Secrets sealed to your local identity stay sealed to a key the account
no longer authorises.

If you only want a **fresh key** rather than a fresh start, do not delete: re-run
`auth login --tenant <same name>` with the new identity. An account may rebind its own tenant
to a new key, which is the recovery path for a lost keystore.

---

## 6. The permission ladder — org → team → project → environment

Access is `org → team → project → environment`. Every verb here needs a **session**.

```sh
# Organisation
42ctl org create --slug acme --name 'ACME Inc'
42ctl org members --org acme
42ctl org invite --org acme --email dev@x.com --role member    # prints a one-time token
42ctl invite accept --token <TOKEN>                            # the invitee redeems it
42ctl invite show --id <ID>

# Project (must exist before environments, groups or grants)
42ctl project create --org acme --slug api --name 'API'
42ctl project list --org acme

# Environment
42ctl env create --project api --name prod
42ctl env list --project api

# Team
42ctl team create --org acme --slug backend --name Backend
42ctl team list --org acme
42ctl team add-member --org acme --team backend --user dev@x.com --role member
42ctl team invite --org acme --team backend --email dev@x.com --role member

# Grants — who may do what on a project
42ctl team grant-project --org acme --team backend --project api --role write --env prod
42ctl project grant --org acme --project api --user dev@x.com --role read
42ctl project grants --org acme --project api                  # the live grants, with their ids
42ctl project revoke-grant --org acme --project api --grant <GRANT_ID>

# Removal — authorization, not erasure (see §13)
42ctl org remove-member   --org acme --user dev@x.com
42ctl team remove-member  --org acme --team backend --user dev@x.com
42ctl group remove-member --group <GROUP_ID> --user dev@x.com

# Groups
42ctl group create --project api
42ctl group add-member --group <GROUP_ID> --user dev@x.com
42ctl group invite --group <GROUP_ID> --email dev@x.com
```

### The role vocabularies are three different closed sets

| Where | Accepted values |
|---|---|
| org role (`org invite --role`) | `owner`, `admin`, `member` |
| team role (`team add-member --role`) | `admin`, `member` |
| **project** role (`--role` on both grant verbs) | `admin`, `write`, `read` |

Project roles are **not** `reader`/`writer`. Anything outside the set is refused.

### Identifiers: what you may type

`--org`, `--team` and `--project` accept a **slug or a UUID**; `--env` accepts a **name or a
UUID**; `--user` accepts an **account id or an email**, and the email must belong to a member
of that organisation. A reference that resolves to nothing is refused with a message naming
which one missed — never a bare 400.

### `org github` — mirror a GitHub org into RBAC

```sh
42ctl org github connect acme          # prints the install URL + nonce
42ctl org github link acme <gh-org>    # link a GitHub org login
42ctl org github sync acme             # teams / members / repos → RBAC
```

Needs `auth login --github`.

---

## 7. Personal secrets — `vault`

Sealed to **your own identity**. Nobody else can read them, including teammates in the same
org, until you `share` explicitly. Needs a contract.

| Command | What it does |
|---|---|
| `vault set <PATH>` | Seal stdin (or `--file F`) and store it. |
| `vault get <PATH>` | Fetch and decrypt to stdout. `--version N` reads an older version (`0` = latest). |
| `vault ls [PREFIX]` | List your secrets. |
| `vault rm <PATH>` | Remove a secret. |
| `vault rotate <PATH>` | Re-seal under a fresh data key; contents unchanged. |
| `vault share <PATH> --to <v42:…>` | Re-seal so another identity can read it. |
| `vault import <FILE>` | Seal each `KEY=VALUE` of a `.env` as `<prefix>/KEY`. |
| `vault export --prefix <P>` | Print your secrets under a prefix as `KEY=value` lines. |
| `vault audit [--since <EPOCH>]` | Stream this identity's tamper-evident audit chain. |
| `vault gc [--apply] [--grace-hours N]` | Remove stored chunks no manifest version still references. |

```sh
printf 'postgres://user:pw@db/app' | 42ctl vault set app/DATABASE_URL
42ctl vault get app/DATABASE_URL
42ctl vault ls app/
42ctl vault share app/DATABASE_URL --to v42:0t1IayTf2os…
```

**`vault gc` is a dry run until `--apply`.** It walks *every* manifest version, refuses when it
found none, and never collects a chunk inside its grace period.

---

## 8. Project sync — `push` / `pull`

Moves your project's whole env tree, sealed to **your own identity**. The real relative paths
live in an encrypted manifest; the server sees only opaque blob ids.

```sh
cd <project>
42ctl push --project api             # seal + upload everything scanned
42ctl push --project api --prune     # also drop entries whose file is gone (mirror the tree)
42ctl pull --project api             # DRY RUN — reports what would change
42ctl pull --project api --apply     # write it
42ctl pull --project api --apply --backup    # keep a .bak of everything overwritten
42ctl pull --project api --apply --force     # take remote even where a local edit conflicts
42ctl pull --project api --at 7 --apply      # the tree as of manifest version 7
```

### What gets scanned

- every `*.env*` and `*.secrets` file, and
- **every regular file** under a directory named `secrets/` or `.secrets/`, at any depth,
  regardless of its name — because a Docker-secrets tree names files for what they are.

Skipped: `.42ctl/`, symlinks, backup/temp shadows (`*.bak*`), and dependency directories
(`node_modules`, `.venv`, `target`, …). A **submodule under `vendor/`** *is* scanned when it is
a real repository, so an orchestrator repo does not silently lose its children's secrets.

A skipped directory that held a candidate file is **reported**, so the next silent omission is
a question you get asked rather than something you discover at a restore.

### Conflicts

`pull` reconciles three ways using `.42ctl/sync.json` as the merge base, exactly as git uses
the index: fast-forward, no-op, or a real divergence with git-style conflict markers. A true
restore over local work is `--at N --apply --force`.

### Files larger than the transport ceiling

Above ~4 MiB a file is split, each chunk sealed and uploaded to an S3-compatible store:

```sh
42ctl config endpoint --blobstore https://<host> --bucket <name> --region <region>
export FT_S3_KEY=… FT_S3_SECRET=…
```

The credential is **never** written to the config file. Without a configured store an oversized
file is **refused**, naming the flag — never half-transferred.

---

## 9. Sharing a tree with the team — scope keys

`push`/`pull` seal to you alone, so a teammate cannot read a personally-pushed tree at all.
To share, seal to the **environment's** key instead.

### Admin: bootstrap once

```sh
42ctl vault env-init     --org acme --project api --env prod    # generate + publish + self-wrap
42ctl vault scope-status --org acme --project api --env prod    # who is active / pending
42ctl vault sync-keys    --org acme --project api --env prod    # wrap the key to new members
```

### Member: enroll once, then read and write

```sh
42ctl keys enroll --org acme
printf 'postgres://…' | 42ctl vault set-env --org acme --project api --env prod DATABASE_URL
42ctl vault get-env --org acme --project api --env prod DATABASE_URL
```

### The whole tree, shared

```sh
42ctl vault push-env --org acme --project api --env prod
42ctl vault pull-env --org acme --project api --env prod            # PREVIEW — writes nothing
42ctl vault pull-env --org acme --project api --env prod --apply
```

Files come back **byte-exact, at their original paths**, and any missing directory is
recreated. Restoring into an empty tree rebuilds the whole shape — six levels deep if that is
what was pushed.

Two things are deliberately *not* restored verbatim:

- **File modes are clamped to the owner** (`0600`-style). The manifest on this path is hostile
  input — anyone who may write the environment may write it — and an entry asking for `0777` on
  a private key would otherwise restore it world-readable with perfectly correct bytes.
- **Directories this creates are `0700`**, for the same reason: a `secrets/` recreated at the
  umask default lets any local process list which secrets a project keeps. Directories that
  already exist are left exactly as they are.

### Private files inside the shared tree

Some of what sits in a project tree is one person's — a `.env.local`, a personal key. Those
travel **with** the tree but are sealed to the pusher alone:

```sh
42ctl vault push-env --org acme --project api --env prod                 # *.local is private already
42ctl vault push-env --org acme --project api --env prod --private 'secrets/me.*' --label app=api
```

- **`*.local` is always private**, flag or not. `--private <PATTERN>` adds more (same grammar as
  `--only`, repeatable); nothing makes a `.local` file shared — rename it if it must be.
- A private file is stored under the same environment but sealed to **your** identity, and named
  only in a second manifest sealed the same way. A teammate who pulls gets neither its bytes nor
  its **path**: to them the environment does not contain it, and their `ls-env` does not list it.
- Your own `pull-env` restores both sets. On a path both name — a teammate pushed a shared
  `secrets/me.key` where you keep a private one — **your copy wins**, and the pull prints
  `secrets/me.key: your private copy shadows the shared one` on stderr.
- A private file is sealed whole: one above the chunking ceiling (about 4 MiB) is refused **before
  anything is uploaded**, by name. Split it, or push it shared.
- A private manifest is acted on only if **you** wrote it. Any writer can store bytes sealed to
  your published key; a "private" file authored by somebody else is refused and the pull fails
  closed rather than restore what they planted.
- `--label KEY=VALUE` (repeatable) tags **every** file of that push, shared and private; it is what
  `ls-env --filter label=…` selects on.

> **Upgrade hazard.** A client from before private files has no such concept and pushes
> `.env.local` into the *shared* manifest. After upgrading, do not `push-env` the same project
> from an old binary. The old-client compatibility gate proves an old client can *read* a new
> tree, never that it *writes* one safely.

### Somebody left

```sh
42ctl vault rotate-scope --org acme --project api --env prod
```

Fresh key at `epoch+1`, everything re-sealed, re-wrapped **only** to the remaining members. The
removed member's old wrap opens nothing sealed after the rotation. Their access ends by
absence — nothing needs to reach into their machine.

> Removal is authorization, not erasure: it cannot take back a key somebody already holds. It
> stops them being re-wrapped. Rotate if that distinction matters.

---

## 10. Previewing, and fetching only part of a tree

**Every restore is a dry run until `--apply`.** The dry run lists exactly what would be
written, at the paths it would be written to, and touches nothing — not even the directories.

```sh
42ctl vault pull-env --org acme --project api --env prod
#   pull-env  dry-run — re-run with --apply to write
#   secrets/ca.crt 806 byte(s)
#   …
#   12 file(s) from environment 'prod'
```

`--only <PATTERN>` narrows it, and is **repeatable**. It matches the stored relative path with
one optional leading and/or trailing `*`:

```sh
--only 'secrets/*'                    # a directory
--only 'secrets/ca.*'                 # a family
--only 'srcs/.env'                    # one exact file
--only '*.crt'                        # a kind, wherever it lives
--only '*.crt' --only 'srcs/.env'     # repeat to union
```

Preview the selection first, then apply exactly that:

```sh
42ctl vault pull-env … --only 'secrets/*'            # 9 of 12 file(s) — 3 not selected
42ctl vault pull-env … --only 'secrets/*' --apply
```

- Anything **not selected is left exactly as it is on disk**, local edits included.
- A partial restore says what it left behind, because a complete tree and one selection of
  several look identical afterwards.
- **A pattern that matches nothing is an error**, never an empty success — the reason to select
  a subset is that the rest is too important to touch, so a typo has to stop.

`pull` (personal) previews the same way; it has no `--only`, but `--at <VERSION>` previews any
historical version before you take it.

To see what an environment holds without even a dry run, `42ctl vault ls-env` reads the two
manifests and fetches no file at all — size, mode, labels and whether a file is yours alone,
at the same cost however large the tree is (§12).

---

## 11. Notes and records

```sh
42ctl note add onboarding.md --project api --file ./onboarding.md
printf 'rotate the API key on the 1st' | 42ctl note add reminders --project api
42ctl note ls  --project api
42ctl note get onboarding.md --project api
42ctl note rm  reminders --project api
```

Notes ride the same manifest as your env files and carry the same guarantee. Inside a directory
with a `.42ctl/` marker, `--project` is optional. `pull-env` does not restore notes — they are
a different kind in the manifest and are read with `note get`.

```sh
42ctl db ls [PREFIX]      # list readable records
42ctl db get <PATH>       # decrypt one locally
```

`db` is a **read-only** view of owner-scoped records through the same signed session. There is
no `db set`/`rm`.

---

## 12. Checking status — what is true right now

| Question | Command |
|---|---|
| Which endpoints am I using? | `42ctl config show` |
| Am I logged in? | `42ctl auth status` |
| Who am I locally, and do I hold a contract? | `42ctl auth whoami` |
| Which account is this session? | `42ctl auth me` (or `account show`) |
| Is my second factor on? | `42ctl auth me` → `second factor` |
| What secrets do I have? | `42ctl vault ls [prefix]` |
| What happened to them? | `42ctl vault audit --since <epoch>` |
| Who is in the org? | `42ctl org members --org acme` |
| What teams / projects / envs exist? | `42ctl team list --org acme`, `project list --org acme`, `env list --project api` |
| Who is granted what on a project? | `42ctl project grants --org acme --project api` |
| **Who can read this environment, and are they provisioned?** | `42ctl vault scope-status --org acme --project api --env prod` |
| **What does this environment hold, and which files are mine alone?** | `42ctl vault ls-env --org acme --project api --env prod` |
| What would a restore change? | `42ctl vault pull-env …` (no `--apply`) |
| What version am I running? | `42ctl version` |

### Shaping any list: `--format` and `--filter`

Every listing verb takes the Docker-style output flags, so the project "database" is
inspectable from the shell without touching the API:

```sh
42ctl vault ls-env --org acme --project api --env prod --format '{{.Path}} {{.Size}} {{.Labels.app}}'
42ctl vault ls-env --org acme --project api --env prod --format json
42ctl vault ls-env --org acme --project api --env prod --filter Private=true
42ctl vault ls-env --org acme --project api --env prod --filter label=app=api --filter label=team=backend
42ctl org members --org acme --filter Role=admin --format '{{.UserID}}'
```

- `{{.Field}}` substitutes a field; `{{.Labels.key}}` descends; `{{json .}}` prints the whole row
  as JSON. An unknown field renders empty, as in Docker. Without `--format` you get the table.
- `--filter KEY=VALUE` keeps rows whose field, as a string, equals the value; `label=K=V` looks
  inside `Labels`. Repeated filters are ANDed. **A filter that matches nothing is an error**
  (exit 1), for the same reason `--only` is: a typo must not look like an empty result.
- Warnings go to stderr, so `--format json` stays parseable.

| Command | Fields |
|---|---|
| `vault ls-env` | `Path` `Size` `Mode` `Kind` `Private` `Labels` |
| `vault ls`, `db ls` | `Path` `Version` `Updated` |
| `vault scope-status` | `Member` `Pubkey` `Provisioned` `State` |
| `org members` | `UserID` `Role` `Joined` |
| `team list`, `project list` | `ID` `Slug` `Name` |
| `env list` | `ID` `Name` |
| `project grants` | `GrantID` `Role` `Env` |
| `note ls` | `Note` |

`Size` is the plaintext length recorded at push, so a tree pushed before sizes existed shows `0`
until it is pushed again. `Mode` is the mode recorded at push; what a restore *writes* is still
clamped to the owner (§9).

### Reading `scope-status`

```
member                pubkey  provisioned  state
4088521c4ffae45da8fc  yes     yes          active
```

| State | Means | Fix |
|---|---|---|
| `active` | holds the current scope key | — |
| `pending-provision` | enrolled, waiting for a wrap | an admin runs `vault sync-keys` |
| `pending-enrollment` | no public key published yet | **they** run `keys enroll --org <slug>` |

This is the one command that answers "why can my colleague not read prod". `sync-keys` reports
`provisioned N` and `skipped N (no registered pubkey)` — a non-zero `skipped` is always
somebody who has not enrolled.

---

## 13. Deleting data

What the CLI can delete today:

| Target | Command | Reversible? |
|---|---|---|
| One secret | `vault rm <PATH>` | no |
| One note | `note rm <PATH>` | no |
| Unreferenced chunks | `vault gc --apply` | no (respects a grace period) |
| Manifest entries for vanished files | `push --prune` | re-push restores |
| Your saved credentials on this machine | `auth logout` | log in again |
| A member's org membership, with every derived one | `org remove-member --org X --user Y` | re-invite |
| A member's team membership only | `team remove-member --org X --team Y --user Z` | re-add |
| A member's group membership only | `group remove-member --group X --user Y` | re-add |
| One grant | `project revoke-grant --org X --project Y --grant Z` | re-grant |
| Your whole account | `account delete --yes` | **no** |

Every removal above answers with what it did **not** do, and the CLI prints it:

```
removed dev@x.com from org 'acme'
authorization removed; scope keys already held remain readable until the
environment is rotated — run `vault rotate-scope` for each environment
```

That warning is the whole subtlety of offboarding. Removal stops somebody being
**re**-wrapped; it cannot reach into their machine and take back a key they already hold. If
that matters, rotate — and rotation is what actually ends their access, by absence.

Members may always remove **themselves** from an org or a team, so nobody can be trapped.
Removing anyone else needs admin, and unseating an owner or admin needs owner. The last owner
of an organisation can be removed by no route at all, because an org with no owner cannot be
administered, invited to, or repaired.

**Access removal without deletion** is usually what you actually want: `vault rotate-scope`
ends a departed member's access at the next epoch without touching anything they hold.

There is deliberately **no crypto-shred verb** and no bulk "delete everything" — see below for
what is missing on purpose versus not yet built.

---

## 14. Running a server

Normally you do not: `vault42-server` and `vault42-authority` are deployed by the `deploy`
workflow in the vault42 repo, which releases the authority first (the server pins its public
key) and stages the authority's secrets from the repository's own Actions secrets.

For a local pair to develop against — authority first, then the server with the authority's key
pinned:

```sh
IMG=public.ecr.aws/docker/library/rust:1.96-slim-bookworm
docker run -d --name v42-authority --network v42 -p 127.0.0.1:8444:8444 \
  -v vault42-target:/target:ro -v v42-auth-data:/data \
  -e VAULT42_AUTHORITY_HOST=0.0.0.0 -e VAULT42_AUTHORITY_PORT=8444 \
  -e VAULT42_AUTHORITY_DB=/data/authority.db \
  -e VAULT42_AUTHORITY_KEY=/data/authority.key "$IMG" /target/debug/vault42-authority

KEY=$(curl -fsS http://127.0.0.1:8444/v1/contract-key | sed 's/.*"public_key":"//;s/".*//')
[ "${#KEY}" -eq 64 ] || { echo "refusing: the authority returned a ${#KEY}-char key"; exit 1; }

docker run -d --name v42-server --network v42 -p 127.0.0.1:8443:8443 \
  -v vault42-target:/target:ro -v v42-srv-data:/data \
  -e VAULT42_HOST=0.0.0.0 -e VAULT42_PORT=8443 -e VAULT42_DB=/data/vault42.db \
  -e VAULT42_CONTRACT_PUBKEY="$KEY" \
  -e VAULT42_SCOPE_KEYS_ENABLED=1 "$IMG" /target/debug/vault42-server

42ctl config endpoint --server http://127.0.0.1:8443 --authority http://127.0.0.1:8444
```

Two knobs decide whether anything works at all:

- **`VAULT42_SCOPE_KEYS_ENABLED=1`** — without it every scope and environment RPC answers
  `UNIMPLEMENTED`, so `env-init`, `sync-keys`, `set-env`, `get-env`, `push-env`, `pull-env` and
  `rotate-scope` all fail while everything else works perfectly.
- **`VAULT42_CONTRACT_PUBKEY`** — the authority's key. Unset, the server refuses to start
  unless you also say `VAULT42_ALLOW_UNGATED=1`, because an ungated server accepts any
  self-generated keypair. **Check the key before pinning it**: piping a failed `curl` straight
  into the environment writes an empty value.

Order is load-bearing. Deploying the server first pins it to a key that does not exist yet, and
every request is then rejected by a server that looks perfectly healthy.

### One-shot container

For CI or a machine without the binary — nothing is left running:

```sh
cd <project>
sh scripts/42ctl-oneshot.sh vault pull-env --org acme --project api --env prod --apply
```

The wrapper exists because a naive `docker run` gets three things wrong: `pull-env` **writes**
your files and the image's `nonroot` uid would own them, the keystore must be mounted rather
than baked in, and a vault on `127.0.0.1` is unreachable from a bridge namespace.

---

## 15. Environment variables

| Variable | What it is |
|---|---|
| `FT_PROFILE` | profile name (same as `--profile`) |
| `FT_CONFIG` | config file path (default `~/.config/42ctl/config.json`); tokens sit beside it |
| `FT_KEYSTORE` | keystore path override |
| `FT_PASSPHRASE` | **keystore** passphrase, for non-interactive use |
| `FT_PASSWORD` | **account** password — a different secret, deliberately a different variable |
| `FT_LOGIN_EMAIL` | default `--email` for login / escrow / recover |
| `FT_REGISTER_TOKEN` | invite token for `auth signup` |
| `FT_S3_KEY` / `FT_S3_SECRET` | object-store credential for large files; never written to disk |
| `FT_GIT_SHA` | stamped at build time; what `version` reports |
| `NO_COLOR` | plain output (`CLICOLOR_FORCE` keeps colour when piped) |

`FT_PASSPHRASE` and `FT_PASSWORD` are two different secrets on purpose. One variable serving
both would silently make them the same value in every automated run.

---

## 16. When something is refused

| Message | What it really means |
|---|---|
| `missing auth metadata` | No contract. Run `auth login --tenant <name>` (which needs a session first). |
| `registration refused: … issues a contract to an ACCOUNT` | You have no session. `auth signup` then `auth login --password`. |
| `no session for this profile` | Same, caught client-side before anything is signed. |
| `HTTP 401` on `auth signup` | The deployment gates account creation; you need `--token`. |
| `UNIMPLEMENTED` from a scope verb | The server was started without `VAULT42_SCOPE_KEYS_ENABLED`. |
| `no environment 'x' in project 'y'` | Create it: `env create --project y --name x`. |
| `no team "x" in this organization` | Wrong org, or the slug is wrong. |
| `no member "x@y.z" in this organization` | They have not joined — invite and have them accept first. |
| `project_role must be admin, write or read` | Not `reader`/`writer`. |
| `no file in environment 'prod' matches …` | Your `--only` pattern matched nothing. |
| `already has a scope key … with N provisioned member(s)` | A real bootstrap exists; use `rotate-scope`. |
| `advertises a scope key … that vault42 never received` | An interrupted `env-init`; it is completing at the next epoch. Informational. |
| `could not open the scope key — are you a wrapped member?` | You are granted but not provisioned. An admin runs `sync-keys`. |
| `refusing to write through a symlinked path` | An ancestor of a restore target is a symlink. |
| `this account already holds its maximum tenant names` | Quota (default 8). |

---

## What you cannot do yet

Stated plainly, because a manual that implies a verb exists costs more than one that admits it
does not.

- **`42ctl unseal` is a stub.** It prints a line and drives nothing; the server's unseal RPC
  authenticates and then always reports 100% unsealed, so there is no seal state to manage.
- **No `org`, `team`, `project`, `env` or `group` deletion.** Nothing removes an organisation,
  a team, a project, an environment or a group once created.
- **No variables verbs.** The authority serves org/project/environment variables with
  precedence and resolution; the CLI has no command for them.
- **No `db set` / `db rm`.** `db` reads only.
- **`--only` is pull-side only.** Not added to `push-env` on purpose: push rebuilds the
  manifest from a scan, so a filtered push would silently drop every unmatched file from the
  environment.
- **Labels are per push, not per file.** `--label` tags every file of that push; there is no
  pattern-scoped label. Push twice with different labels if two groups of files need different
  ones — each push rebuilds the manifest, so push everything each time.
- **`--private` is additive, and `*.local` cannot be shared.** Dropping a `--private` pattern
  and pushing again moves those files back to the shared tree; a `.local` file never moves.
  Rename it.
- **No crates.io, npm or Homebrew channel.** Distribution is `install.sh` and `42ctl update`,
  both reading GitHub Release assets.

---

## Three worked scenarios

### A. Take a project's secrets off disk and get them back

```sh
cd ~/Documents/inception
42ctl auth login --password --email you@example.com --tenant yourname
42ctl org create     --slug univers42 --name 'Univers42'
42ctl project create --org univers42 --slug inception --name 'Inception'
42ctl env create     --project inception --name prod
42ctl keys enroll    --org univers42
42ctl project grant  --org univers42 --project inception --user you@example.com --role admin
42ctl vault env-init  --org univers42 --project inception --env prod
42ctl vault sync-keys --org univers42 --project inception --env prod

42ctl vault push-env  --org univers42 --project inception --env prod    # 12 file(s)
rm -rf secrets srcs/.env .env.example                                   # now really gone

42ctl vault pull-env  --org univers42 --project inception --env prod            # preview
42ctl vault pull-env  --org univers42 --project inception --env prod --apply    # restore
sha256sum -c baseline.sha                                                       # byte-exact
```

`secrets/` is recreated at `0700`, every file at `0600`, every byte identical.

### B. Onboard a teammate to prod, then offboard them

```sh
# You (admin)
42ctl org invite --org acme --email dev@x.com --role member       # send the token
42ctl team create --org acme --slug backend --name Backend
42ctl team grant-project --org acme --team backend --project api --role write --env prod

# Them
42ctl auth signup --email dev@x.com
42ctl auth login --password --email dev@x.com --tenant devname
42ctl invite accept --token <TOKEN>
42ctl keys enroll --org acme                                      # publishes their pubkey

# You again
42ctl team add-member --org acme --team backend --user dev@x.com
42ctl vault scope-status --org acme --project api --env prod      # them: pending-provision
42ctl vault sync-keys    --org acme --project api --env prod      # provisioned 1
42ctl vault scope-status --org acme --project api --env prod      # them: active

# Them: the whole tree
42ctl vault pull-env --org acme --project api --env prod --apply

# Offboarding — they leave
42ctl project grants --org acme --project api                   # find the grant
42ctl project revoke-grant --org acme --project api --grant <ID>
42ctl team remove-member --org acme --team backend --user dev@x.com
42ctl org remove-member  --org acme --user dev@x.com            # takes every derived membership
42ctl vault rotate-scope --org acme --project api --env prod    # ends access already held
```

Order matters less than the last line. The first three stop them being re-wrapped; only the
rotation ends access to a key they already hold. `scope-status` afterwards shows them gone
from the member set that rotation re-wraps to.

### C. A shared `.env` and a private `.env.local`, in one project

```sh
# You: srcs/.env is the team's; srcs/.env.local is yours and is private by default
42ctl vault push-env --org acme --project api --env prod --label app=api
#   pushed 12 shared and 1 private file(s) to environment 'prod'
42ctl vault ls-env --org acme --project api --env prod --filter Private=true --format '{{.Path}}'
#   srcs/.env.local

# A teammate: the shared tree, and no trace of your file — not even its name
42ctl vault ls-env --org acme --project api --env prod --format '{{.Path}}'    # 12 lines
42ctl vault pull-env --org acme --project api --env prod --apply              # 12 file(s)

# You, on a fresh machine: everything, private file included
42ctl vault pull-env --org acme --project api --env prod --apply              # 13 file(s)
```

This is exactly the `inception` run this manual was checked against: 13 files deleted from
disk (the `secrets/` directory with them), `pull-env --apply`, 13 back byte-exact at their
recorded modes, the teammate's restore holding 12 with the sentinel absent, and the Docker
stack brought up cold through its compliance suite afterwards.

---

## See also

`42ctl help` for the guided walkthrough, `42ctl help <topic>` for one subject
(`quickstart` `sync` `keys` `teams` `scopes` `notes` `config` `security` `update`), and
`42ctl <command> --help` for any command's exact flags. `DECISIONS.md` records why the
architecture is what it is; `SECURITY.md` covers verifying a release.
