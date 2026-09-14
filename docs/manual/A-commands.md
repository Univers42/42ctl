# Appendix A. Command index

Every command 42ctl accepts, with its arguments, grouped as `42ctl help commands` groups them. Square
brackets mark what is optional, `...` what may be repeated, and every command also accepts
`--profile NAME` and `--help`. `42ctl COMMAND --help` prints the full description of any one of them.
A unit test in 42ctl fails when a command exists that this index does not list.


## auth

Log in / out of the platform and inspect who you are

| Command | Description |
|---|---|
| `42ctl auth login [--tenant <NAME>] [--token <TOKEN>] [--email <EMAIL>] [--github] [--password]` | Sign in to save a session, and with --tenant take a contract for this identity |
| `42ctl auth signup --email <EMAIL> [--token <TOKEN>]` | Create an account on the authority with an email and a password |
| `42ctl auth passwd` | Change this account's password, which revokes every session it has |
| `42ctl auth mfa [--on] [--off]` | Turn this account's email second factor on or off |
| `42ctl auth me` | Show the account the saved session belongs to |
| `42ctl auth logout` | Forget the saved contract / session for this profile |
| `42ctl auth whoami` | Show the current principal, tenant and address |
| `42ctl auth status` | Show whether this profile is logged in |

## account

The calling account itself, including its irreversible deletion

| Command | Description |
|---|---|
| `42ctl account show` | Show the calling account: its id, its email and whether a second factor is required |
| `42ctl account delete [--yes]` | Permanently delete the calling account |

## keys

Your local zero-knowledge identity: create, export, enroll, escrow, recover

| Command | Description |
|---|---|
| `42ctl keys init [--force]` | Generate a new local identity (X25519 + Ed25519), sealed by a passphrase |
| `42ctl keys export-pub` | Print this identity's shareable public address |
| `42ctl keys enroll --org <SLUG>` | Publish your public keys to an org so its admins can share env keys with you |
| `42ctl keys escrow --email <EMAIL>` | Back up the passphrase-sealed keystore to the authority (for a second machine) |
| `42ctl keys recover --email <EMAIL>` | Restore the keystore on a new machine: emailed code → fetch → unlock locally |

## vault

Your own secrets, sealed on your machine — get, set, ls, rm, share, import, export …  (alias: secrets)

| Command | Description |
|---|---|
| `42ctl vault get <PATH> [--version <N>]` | Fetch a secret and decrypt it locally to stdout |
| `42ctl vault set <PATH> [--file <FILE>]` | Seal a value (stdin, or --file) and store it at PATH |
| `42ctl vault ls [PREFIX] [--all] [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | List your secrets under an optional prefix  (alias: list) |
| `42ctl vault rm <PATH>...` | Remove one or more secrets |
| `42ctl vault gc [--apply] [--grace-hours <HOURS>]` | Remove stored chunks that no version of any manifest still references |
| `42ctl vault rotate <PATH>` | Re-seal a secret under a fresh data key |
| `42ctl vault share <PATH> --to <ADDRESS>` | Re-seal a secret so another identity can read it |
| `42ctl vault audit [--since <EPOCH>]` | Stream this identity's audit log (hash-linked by the server; not verified by this client) |
| `42ctl vault import <FILE> [--prefix <PREFIX>]` | Import a .env file, sealing each KEY=VALUE as KEY, or as PREFIX/KEY with --prefix |
| `42ctl vault export [--prefix <PREFIX>]` | Export your secrets under a prefix as KEY=value lines |

## push · pull

| Command | Description |
|---|---|
| `42ctl push [--project <NAME>] [--prune]` | Upload the project's env tree to the vault, sealed to you (path-aware, byte-exact) |
| `42ctl pull [--project <NAME>] [--apply] [--force] [--backup] [--at <VERSION>]` | Download the project's tree back — a dry-run until you pass --apply |

## note

Small encrypted notes that travel with the project

| Command | Description |
|---|---|
| `42ctl note add <PATH> [--project <NAME>] [--file <FILE>]` | Seal a note (stdin, or --file) at PATH within the project |
| `42ctl note get <PATH> [--project <NAME>]` | Fetch and decrypt the note at PATH to stdout |
| `42ctl note ls [--project <NAME>] [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | List the project's notes  (alias: list) |
| `42ctl note rm <PATH>... [--project <NAME>]` | Remove one or more notes |

## db

RBAC-checked encrypted records, decrypted client-side

| Command | Description |
|---|---|
| `42ctl db get <PATH>` | Read one encrypted record and decrypt it locally |
| `42ctl db ls [PREFIX] [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | List readable records under a prefix  (alias: list) |

## org

Organisations: create, member ls / rm, invite

| Command | Description |
|---|---|
| `42ctl org create --slug <SLUG> --name <TEXT>` | Create an org |
| `42ctl org member ls --org <SLUG> [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | List an org's members  (alias: list) |
| `42ctl org member rm --org <SLUG> --user <USER>...` | Remove members from an org, with every membership derived from it |
| `42ctl org invite --org <SLUG> --email <EMAIL> --role <ROLE>` | Invite an email to an org with a role (prints the one-time token) |

## team

Teams inside an org: create, ls, member add / rm, invite, grant

| Command | Description |
|---|---|
| `42ctl team create --org <SLUG> --slug <SLUG> --name <TEXT>` | Create a team under an org |
| `42ctl team ls --org <SLUG> [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | List an org's teams  (alias: list) |
| `42ctl team member add --org <SLUG> --team <SLUG> --user <USER> [--role <ROLE>]` | Add a user to a team |
| `42ctl team member rm --org <SLUG> --team <SLUG> --user <USER>...` | Remove members from a team, leaving their org membership intact |
| `42ctl team invite --org <SLUG> --team <SLUG> --email <EMAIL> [--role <ROLE>]` | Invite an email to a team (prints the one-time token) |
| `42ctl team grant --org <SLUG> --team <SLUG> --project <NAME> --role <ROLE> [--env <NAME>]` | Grant a team a role on a project (optionally one environment only) |

## group

Project groups: create, member add / rm, invite

| Command | Description |
|---|---|
| `42ctl group create --project <NAME>` | Create a project's group (the server derives the name) |
| `42ctl group member add --group <ID> --user <USER>` | Add a user to a group |
| `42ctl group member rm --group <ID> --user <USER>...` | Remove members from a group |
| `42ctl group invite --group <ID> --email <EMAIL>` | Invite an email to a group (prints the one-time token) |

## env

Environments and what a team shares through one: keys, secrets, the file tree

| Command | Description |
|---|---|
| `42ctl env create --project <NAME> --name <NAME>` | Create an environment under a project |
| `42ctl env ls --project <NAME> [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | List a project's environments  (alias: list) |
| `42ctl env init --org <SLUG> --project <NAME> --env <NAME>` | [admin] Bootstrap an environment's shared key: generate, publish, self-wrap |
| `42ctl env push --org <SLUG> --project <NAME> --env <NAME> [--private <PATTERN>]... [--label <KEY=VALUE>]...` | Push the project's file tree to an environment, shared with everyone granted it |
| `42ctl env pull --org <SLUG> --project <NAME> --env <NAME> [--only <PATTERN>]... [--apply] [--backup]` | Restore an environment's file tree here — a dry-run until you pass --apply |
| `42ctl env files --org <SLUG> --project <NAME> --env <NAME> [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | List an environment's files from its manifests — no file is fetched |
| `42ctl env secret set <PATH> --org <SLUG> --project <NAME> --env <NAME>` | Seal a value (stdin) to the environment's shared key and store it at PATH |
| `42ctl env secret get <PATH> --org <SLUG> --project <NAME> --env <NAME>` | Fetch + decrypt an environment secret at PATH to stdout |
| `42ctl env keys ls --org <SLUG> --project <NAME> --env <NAME> [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | Show each member's key state: active / pending-provision / pending-enrollment |
| `42ctl env keys sync --org <SLUG> --project <NAME> --env <NAME>` | [admin] Wrap the environment's key to every authorized member still missing it |
| `42ctl env keys rotate --org <SLUG> --project <NAME> --env <NAME>` | [admin] Rotate the environment's key: re-seal everything, re-wrap to current members |

## project

Projects inside an org: create, ls, grant ls / add / rm

| Command | Description |
|---|---|
| `42ctl project create --org <SLUG> --slug <SLUG> --name <NAME>` | [admin] Create a project under an org |
| `42ctl project ls --org <SLUG> [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | List an org's projects  (alias: list) |
| `42ctl project grant ls --org <SLUG> --project <NAME> [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | List a project's live grants, with the ids `project grant rm` takes  (alias: list) |
| `42ctl project grant add --org <SLUG> --project <NAME> [--user <USER>] [--group <ID>] --role <ROLE> [--env <NAME>]` | Grant a user, or one of the project's groups, a role on a project (optionally one environment only) |
| `42ctl project grant rm --org <SLUG> --project <NAME> --grant <ID>...` | Revoke grants, so they authorize nobody from now on |

## invite

Accept or inspect an invite by token / id

| Command | Description |
|---|---|
| `42ctl invite accept --token <TOKEN>` | Accept an invite with its one-time token |
| `42ctl invite show --id <ID>` | Show an invite by its id |

## config

Profiles and endpoints (one per org / environment)

| Command | Description |
|---|---|
| `42ctl config profile [NAME]` | List profiles, or switch to / create NAME (inherits the current endpoints) |
| `42ctl config endpoint [--server <URL>] [--authority <URL>] [--grobase <URL>] [--blobstore <URL>] [--bucket <NAME>] [--region <REGION>]` | Set this profile's endpoints |
| `42ctl config show` | Print the resolved configuration for this profile |

## version · update · help

| Command | Description |
|---|---|
| `42ctl version` | Print the version, commit and build target |
| `42ctl update [--check] [--version <X.Y.Z>]` | Update 42ctl to the latest GitHub release (SHA-256 verified before the swap) |
| `42ctl help [TOPIC]` | The guided walkthrough — `42ctl help <topic>` for one subject |

## cloud

The vault42 deployment: machines, volumes, network, secrets, health

| Command | Description |
|---|---|
| `42ctl cloud apps [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | The fly apps this profile's endpoints point at |
| `42ctl cloud status [--app <NAME>] [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | Deployment status for both apps |
| `42ctl cloud health [--no-wake] [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | Check the whole deployment: endpoints, keys, machines, volumes, snapshots |
| `42ctl cloud machine ls [--app <NAME>] [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | List the machines of both apps |
| `42ctl cloud machine inspect <ID>... [--app <NAME>]` | Everything fly knows about one or more machines, as JSON |
| `42ctl cloud machine ports [--app <NAME>] [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | The published ports of each machine |
| `42ctl cloud machine top <ID> [--app <NAME>] [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | The processes running inside a machine |
| `42ctl cloud machine events <ID> [--app <NAME>] [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | What has happened to a machine, newest first |
| `42ctl cloud machine logs [ID] [--app <NAME>] [--no-tail]` | Stream a machine's logs |
| `42ctl cloud machine start <ID>... [--app <NAME>] [--dry-run]` | [admin] Start one or more stopped machines |
| `42ctl cloud machine stop <ID>... [--app <NAME>] [--dry-run]` | [admin] Stop one or more machines |
| `42ctl cloud machine restart <ID>... [--app <NAME>] [--dry-run]` | [admin] Restart one or more machines |
| `42ctl cloud machine suspend <ID>... [--app <NAME>] [--dry-run]` | [admin] Suspend one or more machines, keeping their memory |
| `42ctl cloud machine wait <ID> [--state <STATE>] [--app <NAME>]` | Wait until a machine reaches a state |
| `42ctl cloud volume ls [--app <NAME>] [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | List the volumes of both apps |
| `42ctl cloud volume inspect <ID>... [--app <NAME>]` | Everything fly knows about one or more volumes, as JSON |
| `42ctl cloud volume snapshots <ID> [--app <NAME>] [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | List a volume's snapshots, newest first |
| `42ctl cloud volume snapshot <ID> [--app <NAME>] [--dry-run]` | [admin] Take a snapshot of a volume now |
| `42ctl cloud secret ls [--app <NAME>] [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | List an app's secret NAMES and digests |
| `42ctl cloud net ips [--app <NAME>] [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | The addresses an app answers on |
| `42ctl cloud net certs [--app <NAME>] [--format <TEMPLATE>] [--filter <KEY=VALUE>]... [--quiet]` | The certificates an app serves |
