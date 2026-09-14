# 8. Organisations and permissions

Every command in this chapter talks to the authority and needs a **session**
(`auth login --password --email EMAIL`).

## 8.1 The model

```text
organisation acme ─┬─ members: ada (owner) · bea (member) · cid (member)
                   ├─ team devs ── members: bea
                   └─ project api ─┬─ environments: dev · prod
                                   ├─ group auditors ── members: cid
                                   └─ grants
                                        team devs      write  on prod
                                        group auditors read   on prod
                                        user ada       admin  (every environment)
```

- An **organisation** is the unit of membership. Only its members can be put in its teams and
  groups, and receive its grants.
- A **team** is a set of members across the organisation, useful when the same people work on
  several projects.
- A **project** holds **environments** (`dev`, `staging`, `prod` — any names) and **groups**, which are
  sets of members scoped to that one project.
- A **grant** gives a **user**, a **team** or a **group** a **role** on the project, for **every
  environment** or for **one**.

A person's rights on an environment are the union of every grant that reaches them: directly, through
a team they belong to, or through a group they belong to. Grants are evaluated when they are used, so
somebody who leaves a team or the organisation stops being reached by its grants at once.

## 8.2 Roles

Three separate vocabularies, each a closed set; anything else is refused.

| Where | Roles |
|---|---|
| organisation | `owner`, `admin`, `member` |
| team | `admin`, `member` |
| project grant | `admin`, `write`, `read` |

### What an organisation role allows

| Action | owner | admin | member |
|---|:-:|:-:|:-:|
| list members, teams, projects, environments, grants | ✓ | ✓ | ✓ |
| invite a member or admin | ✓ | ✓ | |
| invite an owner | ✓ | | |
| create a team, a project, an environment, a group | ✓ | ✓ | |
| add or remove team and group members | ✓ | ✓ | |
| grant and revoke on a project | ✓ | ✓ | |
| initialise, provision and rotate environment keys | ✓ | ✓ | |
| remove a member | ✓ | ✓ | |
| remove an admin or an owner | ✓ | | |
| leave the organisation, a team or a group | ✓ | ✓ | ✓ |

Nobody can be trapped: removing **yourself** is always allowed. The **last owner** of an organisation
cannot be removed by any route, because an organisation without an owner could never be administered
again.

### What a project role allows on an environment

| Project role | Read shared secrets and restore the tree | Store secrets and push the tree |
|---|:-:|:-:|
| `read` | ✓ | |
| `write` | ✓ | ✓ |
| `admin` | ✓ | ✓ |

The distinction is enforced twice: the authority records it in the key it wraps to you, and the vault
server refuses a write from a key wrapped for reading. Administering the project — its environments,
groups and grants — is an **organisation** administrator's right, not a project role.

## 8.3 Identifiers

| Option | Accepts |
|---|---|
| `--org` | the organisation's slug or id |
| `--team` | the team's slug or id, within `--org` |
| `--project` | the project's slug or id. With `--org`, it is looked up in that organisation; without it, among the organisations you belong to — and a slug you hold in two of them is refused: give the id |
| `--env` | the environment's name or id, within the project |
| `--user` | an account id, or the email address of a member of the organisation |
| `--group` | the group's id, printed by `group create` |

A reference that matches nothing is refused with a message naming which one missed.

## 8.4 Organisations and invitations

```sh
$ 42ctl org create --slug acme --name 'ACME'
$ 42ctl org member ls --org acme
$ 42ctl org invite --org acme --email bea@example.com --role member
$ 42ctl invite accept --token TOKEN
$ 42ctl invite show --id INVITE_ID
```

`org create` makes you its **owner**. A slug is unique across the deployment and must be a short,
lower-case, URL-safe name.

`org invite` prints an `invite_id` and a one-time **token**. The token is single-use, bound to the
address it was sent to — only the account holding that address can accept it — and it expires.
Deliver it yourself (chat, email); the invitee runs `invite accept` after signing up and signing in.
An administrator may invite at their own standing or below: an admin can invite admins and members,
only an owner can invite an owner.

## 8.5 Teams

```sh
$ 42ctl team create --org acme --slug devs --name 'Developers'
$ 42ctl team ls --org acme
$ 42ctl team member add --org acme --team devs --user bea@example.com
$ 42ctl team member add --org acme --team devs --user bea@example.com --role admin
$ 42ctl team invite --org acme --team devs --email dan@example.com --role member
$ 42ctl team grant --org acme --team devs --project api --role write --env prod
```

A team member must already belong to the organisation. `team invite` prints a token that, when
accepted, adds an existing member of the organisation to the team; it is refused for somebody who has
not joined the organisation yet, so invite them to the organisation first. `team grant` grants the whole team a role; it is removed like any
other grant (§8.8).

## 8.6 Projects, environments and groups

```sh
$ 42ctl project create --org acme --slug api --name 'API'
$ 42ctl project ls --org acme
$ 42ctl env create --project api --name prod
$ 42ctl env ls --project api
$ 42ctl group create --project api
id    2f6d0b8e-…
$ 42ctl group member add --group GROUP_ID --user cid@example.com
$ 42ctl group invite --group GROUP_ID --email eve@example.com
```

A project must exist before its environments, groups and grants. A group's name is derived by the
server; use its **id**.

## 8.7 Grants

```sh
$ 42ctl project grant add --org acme --project api --user bea@example.com --role write --env prod
$ 42ctl project grant add --org acme --project api --group GROUP_ID --role read --env prod
$ 42ctl project grant add --org acme --project api --user ada@example.com --role admin
$ 42ctl project grant ls --org acme --project api
$ 42ctl project grant rm --org acme --project api --grant GRANT_ID
```

`project grant ls` lists the live grants with the columns `GrantID`, `Kind` (`user`, `team` or
`group`), `Grantee` (the account id, the team's slug, or the group's name), `Role` and `Env` (empty
for a project-wide grant); the `GrantID` is what `project grant rm` takes.

`project grant add` names exactly one grantee, `--user` or `--group`; a team is granted with `team
grant`. Without `--env`, the grant covers every environment of the project, including ones created
later.

A grant **authorizes**; it does not deliver a key. After adding a grant, an administrator runs `env
keys sync` for the environment (chapter 9) so that the new member receives it — and the member must
have run `keys enroll` first.

## 8.8 Removing people

```sh
$ 42ctl team member rm --org acme --team devs --user bea@example.com
$ 42ctl group member rm --group GROUP_ID --user cid@example.com
$ 42ctl project grant rm --org acme --project api --grant GRANT_ID
$ 42ctl org member rm --org acme --user bea@example.com
```

| Command | Removes | Leaves |
|---|---|---|
| `team member rm` | the team's grants reaching that person | their organisation membership, their direct grants, other teams |
| `group member rm` | the group's grants reaching that person | everything else |
| `project grant rm` | that grant, for everyone it reached | everything else; the revoked grant is kept as a record |
| `org member rm` | the membership and **everything derived from it**: teams, groups, published public key, direct grants | nothing in that organisation |

Each command accepts several `--user` (or `--grant`) values and attempts every one, even after a
refusal.

> **Removal ends authorization, not a key already held.** A person who was provisioned with an
> environment's key keeps the copy on their machine, and with it everything that was sealed to that
> key before. Every removal command says so. To make sure they cannot read anything written from now
> on, **rotate the environment** (`env keys rotate`, chapter 9). Anything they could read before,
> they may already have — no system can take that back.

`--user` also composes with listings, to remove many at once:

```sh
$ 42ctl org member rm --org acme --user $(42ctl org member ls --org acme -q --filter Role=member)
```

## 8.9 What cannot be deleted

There is no command to delete an organisation, a team, a project, an environment or a group. They are
administrative records; remove the people and grants instead. There is no command to rename a slug.
