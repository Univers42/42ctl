# 13. Walkthrough: Inception

This chapter takes a real project through a real deployment: every kind of secret it has, every way a
team touches them, and the failures that must happen. It is not an illustration. The same commands, in
the same order, are run and checked by `qa/live/inception.sh` in the 42ctl repository, and §13.10
records the last run.

## 13.1 The project

[Inception](https://github.com/Univers42/Inception) is a **public** repository: a Docker Compose stack
of nginx, WordPress, MariaDB and bonus services. Its `make setup` generates the secrets the stack
needs, and its `.gitignore` keeps every one of them out of git:

| Path | What it is | Kind |
|---|---|---|
| `srcs/.env` | domain, database and user names, service settings | environment file |
| `secrets/db_password.txt`, `secrets/db_root_password.txt` | MariaDB passwords | Docker secret |
| `secrets/ftp_password.txt`, `secrets/api_db_password.txt` | service passwords | Docker secret |
| `secrets/credentials.txt` | the WordPress administrator's and editor's passwords | Docker secret |
| `secrets/ca.crt`, `secrets/ca.key` | a local certificate authority | certificate and private key |
| `secrets/server.crt`, `secrets/server.key` | nginx's TLS certificate and key, issued by that CA | certificate and private key |
| `srcs/.env.local` | one developer's overrides | private environment file |

Its committed marker, `.42ctl/project.json`, names the patterns `*.env*` and `*.secrets`; everything
under `secrets/` is taken regardless (chapter 7).

## 13.2 The people

| Person | Role in the story | Organisation | Access to `prod` |
|---|---|---|---|
| **ada** | leads the project, founds the organisation | owner | administrator |
| **bea** | develops | member, team `devs` | `write`, through the team |
| **cid** | audits | member, group `auditors` | `read`, through the group |
| **dan** | has an account, is not in the organisation | — | none |

Each has their own machine, identity, account and tenant. Below, `ORG` stands for the organisation slug
(the script uses `inception-` followed by a timestamp, so that runs never collide) and the addresses are
`@example.com`, which never receive mail.

## 13.3 Everyone: install, identity, account

```sh
curl -fsSL https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | sh
```

```sh
$ 42ctl keys init
$ 42ctl auth signup --email ada@example.com --token TOKEN
$ 42ctl auth login --password --email ada@example.com --tenant ada-inception
$ 42ctl auth status
profile 'default': logged in — session and contract
```

bea, cid and dan do the same with their own addresses and tenants. *Checked:* each ends with a session
and a contract.

## 13.4 Ada's personal vault

```sh
$ printf '%s' "$GITHUB_TOKEN" | 42ctl vault set tools/GITHUB_TOKEN
$ 42ctl vault set tools/deploy.key --file deploy.key
$ 42ctl vault rotate tools/GITHUB_TOKEN
$ 42ctl vault ls tools/ --format '{{.Path}}={{.Version}}'
tools/GITHUB_TOKEN=2
tools/deploy.key=1
$ 42ctl vault get tools/GITHUB_TOKEN --version 1
$ 42ctl vault import tools.env --prefix imported
$ 42ctl vault export --prefix imported
$ 42ctl vault share tools/GITHUB_TOKEN --to BEA_ADDRESS
$ 42ctl vault rm imported/API_KEY imported/API_URL
```

and bea, with ada's principal from `42ctl auth whoami`:

```sh
$ 42ctl vault get shared/ADA_PRINCIPAL/tools/GITHUB_TOKEN
```

*Checked:* a binary file comes back byte-exact; rotation changes nothing readable and the listing shows
version 2; version 1 stays readable; a `.env` imports under a prefix and exports back; bea reads the
shared copy **and cannot read ada's vault itself**; removed secrets are gone.

## 13.5 Ada clones the public repository and generates its secrets

```sh
git clone https://github.com/Univers42/Inception.git && cd Inception
make setup
git status --porcelain --untracked-files=all
printf 'WP_TITLE=Ada on her laptop\n' > srcs/.env.local
```

*Checked:* the fresh clone contains **no** secret; `make setup` produces all ten files of §13.1; `git
status` lists none of them, so the repository stays publishable.

## 13.6 Ada's personal sync

```sh
$ 42ctl push
```

On a second checkout of her own:

```sh
$ 42ctl pull
$ 42ctl pull --apply
```

*Checked:* the dry run writes nothing; `--apply` restores every file byte-exact, `.env.local` included.
A note travels too:

```sh
$ 42ctl note add runbook.md --file RUNBOOK.md
$ 42ctl note ls
$ 42ctl note get runbook.md
```

## 13.7 The organisation

Ada:

```sh
$ 42ctl org create --slug ORG --name 'Inception team'
$ 42ctl project create --org ORG --slug inception --name Inception
$ 42ctl env create --project inception --name prod
$ 42ctl env create --project inception --name dev
$ 42ctl org invite --org ORG --email bea@example.com --role member
$ 42ctl org invite --org ORG --email cid@example.com --role member
```

bea and cid each accept their token:

```sh
$ 42ctl invite accept --token TOKEN
```

Ada gives access by team and by group:

```sh
$ 42ctl team create --org ORG --slug devs --name Developers
$ 42ctl team member add --org ORG --team devs --user bea@example.com
$ 42ctl team grant --org ORG --team devs --project inception --role write --env prod
$ 42ctl group create --project inception
$ 42ctl group member add --group GROUP_ID --user cid@example.com
$ 42ctl project grant add --org ORG --project inception --group GROUP_ID --role read --env prod
$ 42ctl project grant ls --org ORG --project inception --format '{{.Kind}}:{{.Role}}'
team:write
group:read
```

## 13.8 Prod's key, a shared credential, the shared tree

Everyone in the organisation publishes their public keys; ada initialises and provisions `prod`:

```sh
$ 42ctl keys enroll --org ORG
$ 42ctl env init --org ORG --project inception --env prod
$ 42ctl env keys sync --org ORG --project inception --env prod
$ 42ctl env keys ls --org ORG --project inception --env prod --filter State=active -q
```

*Checked:* all three members are `active`.

A single credential, then the whole tree, with labels — ada's `.env.local` stays hers:

```sh
$ printf '%s' "$REGISTRY_TOKEN" | 42ctl env secret set CI_REGISTRY_TOKEN --org ORG --project inception --env prod
$ 42ctl env push --org ORG --project inception --env prod --label app=inception --label stage=prod
pushed 11 shared and 1 private file(s) to environment 'prod'
$ 42ctl env files --org ORG --project inception --env prod --filter Private=true -q
srcs/.env.local
```

bea, on a **fresh clone** of the public repository:

```sh
$ 42ctl env pull --org ORG --project inception --env prod
$ 42ctl env pull --org ORG --project inception --env prod --apply
$ 42ctl env pull --org ORG --project inception --env prod --only 'secrets/*.txt' --apply
$ 42ctl env pull --org ORG --project inception --env prod --apply --backup
$ 42ctl env secret get CI_REGISTRY_TOKEN --org ORG --project inception --env prod
```

*Checked, on bea's clone:*

- the dry run names every file and creates nothing — not even `secrets/`;
- `--apply` restores **every shared file byte-exact** (compared by SHA-256 with ada's originals);
- ada's `.env.local` is **absent**, and its name does not appear in bea's `env files`;
- `secrets/server.key` is `0600` and the recreated `secrets/` is `0700`;
- `openssl x509 -in secrets/server.crt -noout -pubkey` equals `openssl pkey -in secrets/server.key
  -pubout`: the restored key pair is intact;
- `openssl verify -CAfile secrets/ca.crt secrets/server.crt` succeeds: the chain is intact;
- `--only 'secrets/*.txt'` restores exactly the five password files and says what it left out; a
  pattern matching nothing is refused;
- `--backup` keeps both `secrets/server.crt.bak` and `secrets/server.key.bak`.

*Checked, the refusals:* cid, a reader, restores the same tree and reads the credential but **cannot
push** and **cannot overwrite** the credential; dan, an outsider, **restores nothing** and **reads
nothing**.

## 13.9 Two private overrides, then offboarding

bea writes her own `srcs/.env.local` and pushes; each of ada and bea, on fresh clones, gets back the
shared tree **and their own override, never the other's**.

cid leaves the auditors, and ada rotates `prod`:

```sh
$ 42ctl group member rm --group GROUP_ID --user cid@example.com
$ 42ctl env keys rotate --org ORG --project inception --env prod
```

*Checked:* cid now restores nothing and reads nothing; bea still restores the whole shared tree; ada
restores everything, her private override included; the shared credential survived the rotation.

## 13.10 Running it yourself, and the last recorded run

```sh
VAULT42_REGISTER_TOKEN=… bash qa/live/inception.sh /tmp/inception-walkthrough
```

It needs a Linux machine with bash, git, make, openssl, curl and hellish (Inception's Makefile runs on
nothing else), creates four accounts and one organisation on the deployment it points at, and exits
non-zero if any check fails. `C42_AUTHORITY` and `C42_SERVER` point it at another deployment, and
`C42_BIN` at another 42ctl build.

RESULTS_PLACEHOLDER
