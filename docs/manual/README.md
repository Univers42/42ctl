# The 42ctl Manual

**Zero-knowledge secrets, identities and permissions for a team — with `42ctl` on your machine and
`vault42` on the server.**

This edition documents 42ctl 0.1.10 and vault42 0.2.2, and later releases until it says otherwise.
Run `42ctl version` to see which 42ctl you have, and
`curl https://vault42-authority.fly.dev/version` to see which vault42 a deployment runs.

The manual has two halves. **This half** is for everybody who uses a vault: installing the client,
identities and accounts, personal secrets, project trees, organisations and permissions, shared
environments, and every kind of credential a project carries. **The operator's half** —
building a new vault42 deployment from nothing, configuring it, releasing, backing it up — lives
beside the server, in [the vault42 repository](https://github.com/Univers42/vault42/tree/develop/docs/manual).

---

## Contents

| | Chapter | What it answers |
|---|---|---|
| 1 | [Overview](01-overview.md) | What the pieces are, what the server can and cannot see, the three credentials you hold |
| 2 | [Installing and updating](02-installing.md) | The installer, verifying a release, `42ctl update`, the Docker image |
| 3 | [A first session](03-first-session.md) | From nothing to a secret a teammate can read, in ten commands |
| 4 | [Configuration](04-configuration.md) | Profiles, endpoints, files on disk, environment variables |
| 5 | [Identity and accounts](05-identity-and-accounts.md) | Keys, sign-up, sessions, contracts, second factor, a second machine, deletion |
| 6 | [Personal secrets and notes](06-personal-secrets.md) | `vault`, sharing one secret, import and export, history, audit, `note`, `db` |
| 7 | [Project trees](07-project-trees.md) | How files are found, how they are put back, `push` and `pull`, conflicts, large files |
| 8 | [Organisations and permissions](08-organisations.md) | Organisations, invitations, teams, groups, projects, environments, roles, grants, removal |
| 9 | [Shared environments](09-environments.md) | Environment keys, shared secrets, sharing a whole tree, private files, rotation |
| 10 | [Credentials cookbook](10-credentials.md) | `.env` files, passwords, TLS, SSH, tokens, binaries, public and private repositories, CI |
| 11 | [Output and scripting](11-output-and-scripting.md) | `--format`, `--filter`, `-q`, composition, exit statuses, non-interactive use |
| 12 | [Operating the deployment](12-cloud.md) | `42ctl cloud`: health, machines, volumes, snapshots — and what 42ctl never does |
| 13 | [Walkthrough: Inception](13-walkthrough.md) | A real public project's secrets through a real deployment, end to end |
| 14 | [Security model](14-security-model.md) | What each party learns, what removal does and does not do, the supply chain |
| 15 | [Diagnostics](15-diagnostics.md) | Every refusal you are likely to meet, what it means, what to do |
| A | [Command index](A-commands.md) | Every command on one line, by group |
| B | [Files and environment variables](B-files-and-variables.md) | Every path 42ctl reads or writes, every variable it honours |
| C | [Glossary](C-glossary.md) | The vocabulary, defined once |

---

## Conventions

A line in a command block that begins with `$ ` is something you type; the `$ ` is not part of it.
Every other line in the block is what the command prints.

```sh
$ 42ctl version
42ctl 0.1.10 (…)
target    x86_64-unknown-linux-musl
```

Words in CAPITALS inside a command are **placeholders** you replace: `TOKEN`, `EMAIL`, `GROUP_ID`.
Lower-case names such as `acme`, `api` and `prod` are example values used consistently throughout:
the organisation **acme** has a project **api** with environments **dev** and **prod**, and its
people are **ada@example.com** (who founded it), **bea@example.com** (a developer) and
**cid@example.com** (an auditor).

A `# comment` at the end of a command line explains it and is not typed.

> A block like this one is a **warning**: something that cannot be undone, or a mistake that
> fails silently.

## How this manual is kept true

Two mechanisms, both in the 42ctl repository, stop it drifting from the program it describes:

- **Every `$ 42ctl …` line in these chapters is parsed by the real command parser** in 42ctl's test
  suite (`every_manual_example_command_parses`). A renamed command, a removed flag or a missing
  required argument fails the build before it reaches a reader.
- **The walkthrough in chapter 13 is executed**, command for command, by
  `qa/live/inception.sh` against a live deployment, and every behaviour this manual states as a
  guarantee is asserted by the QA battery in `qa/specs/`.

What the parser cannot check is whether a command *succeeds* in a given situation; that is what the
walkthrough and the battery are for.

## Reporting a problem

Open an issue on [github.com/Univers42/42ctl](https://github.com/Univers42/42ctl/issues) for the
client, or [github.com/Univers42/vault42](https://github.com/Univers42/vault42/issues) for the
server. Include `42ctl version`, the exact command, and its complete output. **Never paste a
secret, a token, a passphrase or the contents of `~/.config/42ctl` into an issue.**
