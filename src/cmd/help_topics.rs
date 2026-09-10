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
//! other line is prose. Content only — no logic lives here.

/// Every topic as `(name, one-line summary, body)`, in the order `help` lists them.
pub const TOPICS: &[(&str, &str, &str)] = &[
    (
        "quickstart",
        "first run on a fresh machine, step by step",
        QUICKSTART,
    ),
    ("sync", "push / pull your project's .env tree", SYNC),
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
    ("teams", "orgs, teams, groups, environments, invites", TEAMS),
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

## Get started (three commands)
$ 42ctl keys init                                        # local identity, sealed by a passphrase
$ 42ctl auth login --tenant <tenant> --email you@x.com   # 6-digit code by email → login contract
$ 42ctl push --project <name>                            # seal + upload your project's *.env tree

## Command groups
    identity & access    auth   keys   account   config
    secrets              vault (secrets)   push   pull   note   db
    teams & rbac         org   team   group   env   project   invite
    maintenance          version   update   help   unseal

## Topics";

const QUICKSTART: &str = "\
Everything below runs on a fresh machine. Each step is safe to re-run.

## 1. Point 42ctl at your platform
$ 42ctl config show                                       # the built-in default is already this
$ 42ctl config endpoint --server https://vault42-server.fly.dev --authority https://vault42-authority.fly.dev

## 2. Create your identity
$ 42ctl keys init                                         # prompts for a NEW passphrase (never stored)
$ 42ctl keys export-pub                                   # your public address — share it freely
! Lose the passphrase and the data sealed to this identity is gone. By design.

## 3. Log in
$ 42ctl auth signup --email you@x.com                     # create the account (password prompted)
$ 42ctl auth login --password --email you@x.com           # sign in: saves the session teams need
$ 42ctl auth login --password --email you@x.com --tenant <tenant>   # session AND contract
$ 42ctl auth whoami                                       # principal + address + 'contract: bound'
! Two different credentials. Org, team, project and grant verbs need the SESSION, which only
! --password or --github mints. The vault verbs need the contract. Most people want both.

## 4. Sync a project
$ cd <project>
$ 42ctl push --project <name>                             # seals every *.env* file, uploads
$ 42ctl pull --project <name>                             # dry-run: what would change
$ 42ctl pull --project <name> --apply                     # materialise the tree

## Next
$ 42ctl help sync                                         # conflicts, --force, --backup
$ 42ctl help keys                                         # carry the identity to a second machine
$ 42ctl help teams                                        # share with your team";

const SYNC: &str = "\
`push` and `pull` move your project's *.env tree (and *.secrets) through the vault.
The real file paths live in an ENCRYPTED manifest; the server only sees opaque
blob ids. Files come back byte-exact, with their Unix mode.

## Where is 'the project'?
A project is rooted where a `.42ctl/` marker directory lives. It holds a stable
project id shared by every machine that pulls the same tree. `--project <name>`
names it; without the flag, 42ctl looks for `.42ctl/` in the current directory.

## Push
$ 42ctl push --project <name>                             # scans, seals, uploads changed files

## Pull is a dry-run until you say --apply
$ 42ctl pull --project <name>                             # lists: create / update / conflict / in-sync
$ 42ctl pull --project <name> --apply                     # writes the files
$ 42ctl pull --project <name> --apply --backup            # keeps a .bak of anything overwritten

## Conflicts (3-way, like git)
  unchanged here, changed remotely   → fast-forward, remote wins
  changed here, unchanged remotely   → keep local, nothing written
  changed on both sides              → the file gets <<<<<<< / ======= / >>>>>>> markers
$ 42ctl pull --project <name> --apply --force             # take remote everywhere, no markers
! A path that could escape the project root (../, absolute, drive letter) is refused, never sanitised.";

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
$ 42ctl keys escrow --email you@x.com                     # machine A: OTP → upload the SEALED keystore
$ 42ctl keys recover --email you@x.com                    # machine B: OTP → fetch → unlock with the passphrase
The escrow holds ciphertext only. Without the passphrase it is noise.

## Join an org
$ 42ctl keys enroll --org <slug>                          # publish your public keys so admins can share env keys

## Non-interactive (CI)
  FT_PASSPHRASE=…   supplies the passphrase; it is read from the environment, never echoed";

const TEAMS: &str = "\
Access is organised as  org → team → project → environment.  These verbs talk to
grobase and need a GitHub-backed session:
$ 42ctl auth login --github                               # device flow: open the URL, enter the code

## Orgs
$ 42ctl org create --slug acme --name 'ACME Inc'
$ 42ctl org members --org acme
$ 42ctl org invite --org acme --email dev@x.com --role member    # prints a one-time token
$ 42ctl invite accept --token <token>                     # the invitee redeems it

## Teams inside an org
$ 42ctl team create --org acme --slug backend --name Backend
$ 42ctl team add-member --org acme --team backend --user dev@x.com
$ 42ctl team grant-project --org acme --team backend --project api --role write --env prod

## Projects, environments, direct grants
$ 42ctl env create --project api --name prod
$ 42ctl env list --project api
$ 42ctl project grant --org acme --project api --user dev@x.com --role read

## GitHub App (mirror your GitHub org into RBAC)
$ 42ctl org github connect acme                           # prints the install URL
$ 42ctl org github link acme <github-org>
$ 42ctl org github sync acme                              # teams / members / repos → RBAC";

const SCOPES: &str = "\
A scope is one environment's SHARED key. Secrets sealed to it are readable by every
authorized member — without the server ever holding the key. An admin creates it,
members enroll their public keys, the admin wraps the scope key to each of them.

## Admin: bootstrap and keep members in sync
$ 42ctl vault env-init --org acme --project api --env prod        # generate + publish + self-wrap
$ 42ctl vault scope-status --org acme --project api --env prod    # who is active / pending
$ 42ctl vault sync-keys --org acme --project api --env prod       # wrap the key to new members

## Member: enroll once, then read and write
$ 42ctl keys enroll --org acme                                     # publish your public keys
$ echo -n 'postgres://…' | 42ctl vault set-env --org acme --project api --env prod DATABASE_URL
$ 42ctl vault get-env --org acme --project api --env prod DATABASE_URL

## Share a whole TREE with the team, and get it back
$ 42ctl vault push-env  --org acme --project api --env prod        # seal every scanned file
$ 42ctl vault pull-env  --org acme --project api --env prod        # PREVIEW: writes nothing
$ 42ctl vault pull-env  --org acme --project api --env prod --apply
Files come back byte-exact, at their original paths, with any missing directory recreated
owner-only. A restore into an empty tree rebuilds the whole shape.

## Fetch only part of it
$ 42ctl vault pull-env … --only 'secrets/*'               # just that directory
$ 42ctl vault pull-env … --only 'secrets/ca.*'            # just the CA material
$ 42ctl vault pull-env … --only 'srcs/.env'               # one exact file
$ 42ctl vault pull-env … --only '*.crt' --only 'srcs/.env'  # repeat to union
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
Notes are small encrypted documents that ride the same manifest as your .env files —
runbooks, onboarding, the one-liner nobody remembers. Same zero-knowledge guarantee.

$ 42ctl note add onboarding.md --project api --file ./onboarding.md
$ echo 'rotate the API key on the 1st' | 42ctl note add reminders --project api
$ 42ctl note ls --project api
$ 42ctl note get onboarding.md --project api
$ 42ctl note rm reminders --project api

Inside a directory that has a `.42ctl/` marker, `--project` is optional.";

const CONFIG: &str = "\
A profile is one org / environment: its endpoints plus its own login contract and
session. Switch profiles to switch worlds; nothing leaks between them.

## Profiles
$ 42ctl config profile                                    # list (the active one is marked)
$ 42ctl config profile staging                            # switch to / create 'staging'
$ 42ctl --profile staging vault ls                        # one-off, without switching
$ 42ctl config show                                       # resolved endpoints

## Endpoints
$ 42ctl config endpoint --server <vault42 URL> --authority <authority URL>
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
  FT_PROFILE       profile name (same as --profile)
  FT_CONFIG        config file (default ~/.config/42ctl/config.json); tokens sit beside it
  FT_PASSPHRASE    keystore passphrase for non-interactive use (CI)
  FT_PASSWORD      ACCOUNT password for non-interactive use — a different secret
  FT_LOGIN_EMAIL   default --email for login / escrow / recover
  FT_KEYSTORE      keystore file path override
  FT_S3_KEY        object-store credential for large files
  FT_S3_SECRET     its secret — never written to the config file
  NO_COLOR         plain output; CLICOLOR_FORCE keeps colour when piped";

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
$ 42ctl vault rotate-scope …                              # after anyone leaves a team
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
