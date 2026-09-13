/* ************************************************************************** */
/*                                                                            */
/*                                                          :::      :::::::: */
/*   help_topics.rs                                       :+:      :+:    :+: */
/*                                                        +:+ +:+         +:+ */
/*   By: dlesieur <dev.pro.photo@gmail.com>                +#+  +:+       +#+ */
/*                                                          +#+#+#+#+#+   +#+ */
/*   Created: 2026/06/19 00:00:00 by dlesieur                      #+#    #+# */
/*   Updated: 2026/06/19 00:00:00 by dlesieur               ###   ########.fr */
/*                                                                            */
/* ************************************************************************** */

//! The text of `42ctl help` and its topics — the guided walkthrough for people who have
//! never seen the stack. Tiny line-oriented markup rendered by `cmd::help`: `## ` opens a
//! section, `$ ` is a command (an inline `  # …` is a comment), `! ` is a warning; every
//! other line is prose. Content only — no logic lives here. Every `$ 42ctl …` line must be
//! a complete command the parser accepts (a test in `cmd::help` parses each one), so write
//! the flags out rather than eliding them with `…`.

/// Every topic as `(name, one-line summary, body)`, in the order `help` lists them.
pub const TOPICS: &[(&str, &str, &str)] = &[
    (
        "kickoff",
        "the whole product end to end: you, a project, a team",
        KICKOFF,
    ),
    ("sync", "push / pull your project's env tree", SYNC),
    (
        "large",
        "files too big for the vault: chunks, resume, collection",
        LARGE,
    ),
    (
        "keys",
        "your identity, the passphrase, a second machine",
        KEYS,
    ),
    (
        "account",
        "signup, sessions vs contracts, password, MFA, deletion",
        ACCOUNT,
    ),
    (
        "teams",
        "orgs, teams, projects, environments, grants",
        TEAMS,
    ),
    (
        "scopes",
        "shared per-environment secrets for a whole team",
        SCOPES,
    ),
    (
        "notes",
        "encrypted notes that travel with the project",
        NOTES,
    ),
    (
        "config",
        "profiles, endpoints and the FT_* env knobs",
        CONFIG,
    ),
    (
        "security",
        "what is protected, and what is on you",
        SECURITY,
    ),
    (
        "update",
        "install, self-update and verify a release",
        UPDATE,
    ),
];

pub const OVERVIEW: &str = "\
Your secrets are encrypted on YOUR machine before they leave it. The server only
ever stores opaque ciphertext — it cannot read a key or a plaintext, and neither
can anyone who breaches it.

## Get started
$ 42ctl keys init                                         # local identity, sealed by a passphrase
$ 42ctl auth signup --email you@example.com               # your account (password prompted)
$ 42ctl auth login --password --email you@example.com --tenant <tenant>
$ 42ctl push --project <name>                             # seal + upload your project's env tree
The whole product, team included, step by step:   42ctl help kickoff
Every command with its arguments, in one list:    42ctl help commands

## Command groups
    identity & access    auth   account   keys   config
    secrets              vault (secrets)   push   pull   note   db
    teams & rbac         org   team   group   env   project   invite
    maintenance          version   update   help   unseal

## Topics";

const KICKOFF: &str = "\
The whole product in one sitting: from an empty machine to a team sharing a production
environment, and back out again when someone leaves. Replace acme / api / prod and the
addresses with yours. Every step is safe to re-run.

## 0. Install, and see where you point
$ curl -fsSL https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | sh
$ 42ctl version                                           # version, commit, build target
$ 42ctl config show                                       # the built-in endpoints; nothing to set

## 1. Your identity — it never leaves this machine
$ 42ctl keys init                                         # prompts for a NEW passphrase
$ 42ctl keys export-pub                                   # your v42:… address, safe to share
! Nothing can reset the passphrase: lose it and what is sealed to you is gone. See step 8.

## 2. An account, then both credentials in one login
$ 42ctl auth signup --email you@example.com               # password prompted; --token if gated
$ 42ctl auth login --password --email you@example.com --tenant yourname
$ 42ctl auth whoami                                       # principal, address, 'contract: bound'
  session    your ACCOUNT  →  org  team  group  env  project  invite  account
  contract   your KEY      →  vault  push  pull  note  db
  both       the scope verbs: vault env-init  sync-keys  push-env  pull-env …

## 3. A personal secret — sealed to you alone
$ printf 'sk_live_123' | 42ctl vault set app/STRIPE_KEY
$ 42ctl vault get app/STRIPE_KEY
$ 42ctl vault ls app/

## 4. Your project's env tree, off the disk and back
$ cd ~/code/api
$ 42ctl push --project api                                # every *.env*, *.secrets, secrets/**
$ 42ctl pull --project api                                # DRY RUN: what would change
$ 42ctl pull --project api --apply                        # write it back, byte-exact
$ 42ctl note add onboarding.md --project api --file ./onboarding.md

## 5. An organisation for the team
$ 42ctl org create --slug acme --name 'ACME Inc'
$ 42ctl project create --org acme --slug api --name API   # environments hang off a project
$ 42ctl env create --project api --name prod
$ 42ctl keys enroll --org acme                            # publish your PUBLIC keys to the org
$ 42ctl project grant --org acme --project api --user you@example.com --role admin

## 6. Share the tree with everyone granted prod
$ 42ctl vault env-init --org acme --project api --env prod    # the environment's shared key
$ 42ctl vault sync-keys --org acme --project api --env prod   # wrap it to every member
$ 42ctl vault push-env --org acme --project api --env prod    # *.local stays yours alone
$ 42ctl vault ls-env --org acme --project api --env prod      # what it holds; fetches nothing

## 7. Onboard a teammate
$ 42ctl org invite --org acme --email dev@example.com --role member    # prints a one-time token
$ 42ctl team create --org acme --slug backend --name Backend
$ 42ctl team grant-project --org acme --team backend --project api --role write --env prod
They run, on their machine:
$ 42ctl keys init
$ 42ctl auth signup --email dev@example.com
$ 42ctl auth login --password --email dev@example.com --tenant devname
$ 42ctl invite accept --token <token>
$ 42ctl keys enroll --org acme
You run:
$ 42ctl team add-member --org acme --team backend --user dev@example.com
$ 42ctl vault sync-keys --org acme --project api --env prod      # provisioned 1
$ 42ctl vault scope-status --org acme --project api --env prod   # them: active
They run:
$ 42ctl vault pull-env --org acme --project api --env prod           # preview, writes nothing
$ 42ctl vault pull-env --org acme --project api --env prod --apply   # the whole tree

## 8. Your second machine
$ 42ctl keys escrow --email you@example.com               # here: upload the SEALED keystore
$ 42ctl keys recover --email you@example.com              # there: code, fetch, same passphrase

## 9. Someone leaves
$ 42ctl org remove-member --org acme --user dev@example.com      # every derived membership too
$ 42ctl vault rotate-scope --org acme --project api --env prod   # ends access already held
! Removal stops future wraps; only a rotation ends access to a key they already hold.

## Next
$ 42ctl help commands                                     # every command and its arguments
$ 42ctl help scopes                                       # private files, labels, partial pulls
$ 42ctl vault push-env --help                             # the long form of any one command";

const SYNC: &str = "\
`push` and `pull` move your project's env tree through the vault, sealed to YOU alone:
every *.env* and *.secrets file, plus every file under a `secrets/` directory. The real
file paths live in an ENCRYPTED manifest; the server only sees opaque blob ids. Files
come back byte-exact, with their Unix mode. Run both from the project's root directory.

## Which project?
`--project <id>` is any name you choose; use the same one on every machine. Without it,
the first run here writes `.42ctl/project.json` with a generated id, and later runs in
this directory reuse it — but a second machine would generate a different one.

## Push
$ 42ctl push --project api                                # scans, seals, uploads changed files
$ 42ctl push --project api --prune                        # also drop entries whose file is gone

## Pull is a dry-run until you say --apply
$ 42ctl pull --project api                                # lists: create / update / conflict / in-sync
$ 42ctl pull --project api --apply                        # writes the files
$ 42ctl pull --project api --apply --backup               # keeps a .bak of anything overwritten
$ 42ctl pull --project api --at 7                         # preview the tree as of version 7

## Conflicts (3-way, like git)
  unchanged here, changed remotely   → fast-forward, remote wins
  changed here, unchanged remotely   → keep local, nothing written
  changed on both sides              → the file gets <<<<<<< / ======= / >>>>>>> markers
$ 42ctl pull --project api --apply --force                # take remote everywhere, no markers
! A path that could escape the project root (../, absolute, drive letter) is refused, never sanitised.
To share the tree with a team rather than keep it to yourself: 42ctl help scopes";

const LARGE: &str = "\
A file bigger than about 4 MiB cannot travel in one sealed envelope. It is split
into encrypted chunks that go to object storage, and the vault keeps only the
chunk list — so a gigabyte never lands on the server and never crosses a message
limit. The store holds ciphertext whose key it has never been offered.

## Point the profile at an object store
$ export FT_S3_KEY=... FT_S3_SECRET=...                  # never written to the config file
$ 42ctl config endpoint --blobstore https://<s3-host> --bucket <name>
! Without this, an oversized file is REFUSED. It is never half-transferred.

## Push and pull as usual
$ 42ctl push                                              # large files are chunked automatically
$ 42ctl pull --apply                                      # reassembled byte-exact
A second push sends only the chunks that changed, so an interrupted upload resumes
rather than starting over, and keeping old versions costs only what differs.

## Restore an older version
$ 42ctl pull --at 3 --apply --force                       # the tree as it stood at version 3
Each file comes back at the revision that version recorded, not today's contents.

## Reclaim space
$ 42ctl vault gc                                          # dry run: what nothing references
$ 42ctl vault gc --apply                                  # remove it
$ 42ctl vault gc --apply --grace-hours 72                 # spare anything younger than 3 days
Chunks are never overwritten, so every edit leaves its predecessors behind.
Collection walks EVERY version, so a chunk an older version still needs is kept.";

const KEYS: &str = "\
Your identity is one X25519 + Ed25519 keypair, generated locally and sealed by a
passphrase (Argon2id). It is what every secret is encrypted TO. The private key
never leaves the machine in the clear.

## Create / inspect
$ 42ctl keys init                                         # new identity (prompts for a passphrase)
$ 42ctl keys init --force                                 # replace it — the old identity is unrecoverable
$ 42ctl keys export-pub                                   # public address to give to people who share with you

## Second machine — no file copy
$ 42ctl keys escrow --email you@example.com               # machine A: code → upload the SEALED keystore
$ 42ctl keys recover --email you@example.com              # machine B: code → fetch → unlock with the passphrase
The escrow holds ciphertext only. Without the passphrase it is noise.

## Join an org
$ 42ctl keys enroll --org acme                            # publish your public keys so admins can share env keys

## Lost the keystore?
$ 42ctl keys init --force                                 # a new identity
$ 42ctl auth login --password --email you@example.com --tenant <tenant>   # the SAME one
An account may rebind its own tenant to a new key. What was sealed to the old key stays sealed.

## Non-interactive (CI)
  FT_PASSPHRASE=…   supplies the passphrase; it is read from the environment, never echoed";

const ACCOUNT: &str = "\
Your ACCOUNT lives on the authority: an email and a password. It is not your identity
(the keypair from `keys init`), and its password is not your passphrase.

## Create, sign in, look
$ 42ctl auth signup --email you@example.com               # password prompted
$ 42ctl auth signup --email you@example.com --token <invite>    # when sign-ups are gated
$ 42ctl auth login --password --email you@example.com     # a session
$ 42ctl auth login --password --email you@example.com --tenant <tenant>   # session AND contract
$ 42ctl auth login --github                               # a session, via the GitHub device flow
$ 42ctl auth status                                       # is this profile logged in?
$ 42ctl auth whoami                                       # local principal, address, contract
$ 42ctl auth me                                           # the account behind the session
$ 42ctl account show                                      # the same, from the account side

## Two credentials — every 'permission denied' is one of them
  session    your ACCOUNT  →  org  team  group  env  project  invite  account
  contract   your KEY      →  vault  push  pull  note  db
  both       the scope verbs: vault env-init  sync-keys  set-env  get-env  push-env  pull-env …
`auth login --tenant` on its own needs a session already: a contract is issued to an account.

## Password and second factor
$ 42ctl auth passwd                                       # revokes every session, this one too
$ 42ctl auth mfa --on                                     # an emailed code at every sign-in
$ 42ctl auth mfa --off                                    # both directions ask for a code first
$ 42ctl auth logout                                       # forget this profile's session and contract

## Deleting the account
$ 42ctl account delete --yes                              # IRREVERSIBLE — refuses without --yes
! Every session and org membership goes, and every tenant name it claimed is RELEASED to anyone.
! Want a fresh key rather than a fresh start? See `42ctl help keys`, 'Lost the keystore?'.";

const TEAMS: &str = "\
Access is organised as  org → team → project → environment.  These verbs need a
SESSION (see `42ctl help account`):
$ 42ctl auth login --password --email you@example.com     # or --github for the device flow

## Orgs
$ 42ctl org create --slug acme --name 'ACME Inc'
$ 42ctl org members --org acme
$ 42ctl org invite --org acme --email dev@example.com --role member    # prints a one-time token
$ 42ctl invite accept --token <token>                     # the invitee redeems it
$ 42ctl invite show --id <id>

## Projects and environments
$ 42ctl project create --org acme --slug api --name API   # must exist before envs, groups, grants
$ 42ctl project list --org acme
$ 42ctl env create --project api --name prod
$ 42ctl env list --project api

## Teams inside an org
$ 42ctl team create --org acme --slug backend --name Backend
$ 42ctl team list --org acme
$ 42ctl team add-member --org acme --team backend --user dev@example.com
$ 42ctl team invite --org acme --team backend --email dev@example.com

## Grants — who may do what on a project
$ 42ctl team grant-project --org acme --team backend --project api --role write --env prod
$ 42ctl project grant --org acme --project api --user dev@example.com --role read
$ 42ctl project grants --org acme --project api           # the live grants, with their ids
$ 42ctl project revoke-grant --org acme --project api --grant <id>
  roles      org: owner admin member  ·  team: admin member  ·  project: admin write read
  names      --org --team --project take a slug or id, --env a name or id, --user an id or email

## Groups
$ 42ctl group create --project api
$ 42ctl group add-member --group <id> --user dev@example.com
$ 42ctl group invite --group <id> --email dev@example.com

## Removing people
$ 42ctl team remove-member --org acme --team backend --user dev@example.com   # org membership stays
$ 42ctl group remove-member --group <id> --user dev@example.com
$ 42ctl org remove-member --org acme --user dev@example.com   # teams, groups, grants, pubkey too
! Removal ends AUTHORIZATION, not a key already held: rotate the environment (42ctl help scopes).

## GitHub App (mirror your GitHub org into RBAC; needs auth login --github)
$ 42ctl org github connect acme                           # prints the install URL
$ 42ctl org github link acme <github-org>
$ 42ctl org github sync acme                              # teams / members / repos → RBAC";

const SCOPES: &str = "\
A scope is one environment's SHARED key. Secrets sealed to it are readable by every
authorized member — without the server ever holding the key. An admin creates it,
members enroll their public keys, the admin wraps the scope key to each of them.
These verbs need both a session and a contract.

## Admin: bootstrap and keep members in sync
$ 42ctl vault env-init --org acme --project api --env prod        # generate + publish + self-wrap
$ 42ctl vault scope-status --org acme --project api --env prod    # who is active / pending
$ 42ctl vault sync-keys --org acme --project api --env prod       # wrap the key to new members

## Member: enroll once, then read and write
$ 42ctl keys enroll --org acme                                     # publish your public keys
$ printf 'postgres://…' | 42ctl vault set-env --org acme --project api --env prod DATABASE_URL
$ 42ctl vault get-env --org acme --project api --env prod DATABASE_URL

## Share a whole TREE with the team, and get it back
$ 42ctl vault push-env --org acme --project api --env prod         # seal every scanned file
$ 42ctl vault ls-env --org acme --project api --env prod           # what it holds; fetches nothing
$ 42ctl vault pull-env --org acme --project api --env prod         # PREVIEW: writes nothing
$ 42ctl vault pull-env --org acme --project api --env prod --apply
Files come back byte-exact, at their original paths, with any missing directory recreated
owner-only. A restore into an empty tree rebuilds the whole shape.

## Private files and labels inside the shared tree
$ 42ctl vault push-env --org acme --project api --env prod --private 'secrets/me.*' --label app=api
$ 42ctl vault ls-env --org acme --project api --env prod --filter Private=true --format '{{.Path}}'
*.local is ALWAYS private. A private file is sealed to you alone: a teammate sees neither its
bytes nor its name, and your own pull-env restores it.

## Fetch only part of it
$ 42ctl vault pull-env --org acme --project api --env prod --only 'secrets/*'
$ 42ctl vault pull-env --org acme --project api --env prod --only 'secrets/ca.*'
$ 42ctl vault pull-env --org acme --project api --env prod --only '*.crt' --only 'srcs/.env'
Anything not selected is left exactly as it is on disk, edits included. Drop --apply to
preview the same selection first. A pattern matching nothing is an ERROR, never a quiet
no-op, because the reason to select a subset is that the rest is too important to touch.

## Someone left the team
$ 42ctl vault rotate-scope --org acme --project api --env prod    # fresh key, re-sealed, re-wrapped
The removed member's old wrap opens nothing that is sealed after the rotation.

## Member states in scope-status
  active               has the current scope key
  pending-provision    enrolled, waiting for an admin's sync-keys
  pending-enrollment   no public key yet → they must run  42ctl keys enroll";

const NOTES: &str = "\
Notes are small encrypted documents that ride the same manifest as your env files —
runbooks, onboarding, the one-liner nobody remembers. Same zero-knowledge guarantee.

$ 42ctl note add onboarding.md --project api --file ./onboarding.md
$ echo 'rotate the API key on the 1st' | 42ctl note add reminders --project api
$ 42ctl note ls --project api
$ 42ctl note get onboarding.md --project api
$ 42ctl note rm reminders --project api

Inside a directory that has a `.42ctl/` marker, `--project` is optional.

## Records
$ 42ctl db ls                                             # records you may read
$ 42ctl db get <path>                                     # decrypt one locally
`db` reads only; there is no db set or db rm.";

const CONFIG: &str = "\
A profile is one org / environment: its endpoints plus its own login contract and
session. Switch profiles to switch worlds; nothing leaks between them.

## Profiles
$ 42ctl config profile                                    # list (the active one is marked)
$ 42ctl config profile staging                            # switch to / create 'staging'
$ 42ctl --profile staging vault ls                        # one-off, without switching
$ 42ctl config show                                       # resolved endpoints

## Endpoints
$ 42ctl config endpoint --server <vault42-url> --authority <authority-url>
  server      vault42-server — the gRPC store that holds ciphertext
  authority   the authority — login contracts, accounts, orgs, codes, escrow
  grobase     an override, normally unset: the authority serves those routes now
The built-in defaults are https://vault42-server.fly.dev and
https://vault42-authority.fly.dev, so a fresh install needs this only to override them.

## Where large files go
$ 42ctl config endpoint --blobstore https://<s3-host> --bucket <name> --region <region>
The CREDENTIAL is never stored here — this file is plain JSON. It is read from
FT_S3_KEY and FT_S3_SECRET at the moment of use. See `42ctl help large`.

## Environment knobs
  FT_PROFILE         profile name (same as --profile)
  FT_CONFIG          config file (default ~/.config/42ctl/config.json); tokens sit beside it
  FT_PASSPHRASE      keystore passphrase for non-interactive use (CI)
  FT_PASSWORD        ACCOUNT password for non-interactive use — a different secret
  FT_LOGIN_EMAIL     default --email for signup / login / escrow / recover
  FT_REGISTER_TOKEN  default --token for signup / login, when sign-ups are gated
  FT_KEYSTORE        keystore file path override
  FT_S3_KEY          object-store credential for large files
  FT_S3_SECRET       its secret — never written to the config file
  NO_COLOR           plain output; CLICOLOR_FORCE keeps colour when piped";

const SECURITY: &str = "\
## What 42ctl guarantees
  every plaintext is encrypted on your machine before it is sent
  the server stores ciphertext + opaque paths — it cannot read your data
  your private key exists only on your machine, sealed by your passphrase
  every write is signed and bound to your login contract (tamper-evident audit chain)
  pull refuses any path that could escape the project directory

## What is on you
! The passphrase. There is no reset. Lose it → the data is unrecoverable, by design.
! The machine. A compromised host can read what you decrypt on it.
! The binary. 42ctl decrypts plaintext, so a tampered binary IS a breach.
  Install only via the official installer or a verified release (42ctl help update).

## Good habits
$ 42ctl auth logout                                       # on a shared machine, when done
$ 42ctl vault audit                                       # review your own tamper-evident chain
$ 42ctl vault rotate-scope --org acme --project api --env prod    # after anyone leaves a team
$ 42ctl update --check                                    # stay on a current, signed build";

const UPDATE: &str = "\
Releases are static Linux binaries (x86_64, aarch64) published on GitHub, each with
a SHA256SUMS file and SLSA build provenance. Nothing is installed unverified.

## Install (any Linux distro, no root needed)
$ curl -fsSL https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | sh
  installs to ~/.local/bin (or /usr/local/bin when run as root) and prints the version
$ curl -fsSL https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | sh -s -- --version v0.2.0
$ curl -fsSL https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | sh -s -- --bin-dir /opt/bin

## Self-update
$ 42ctl update --check                                    # is there a newer release?
$ 42ctl update                                            # download, verify SHA-256, swap atomically
$ 42ctl update --version 0.3.1                            # pin (or roll back to) an exact release
$ 42ctl version                                           # version, commit, target
If the binary lives somewhere you cannot write (e.g. /usr/local/bin), run with sudo.

## Verify a release by hand
$ sha256sum -c --ignore-missing SHA256SUMS                # after downloading the asset + SHA256SUMS
$ gh attestation verify 42ctl-x86_64-unknown-linux-musl --repo Univers42/42ctl
See SECURITY.md in the repository for cosign / SLSA verification.";
