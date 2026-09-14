# 1. Overview

## 1.1 What the system is

**42ctl** is a single command-line program that runs on your machine. **vault42** is the service it
talks to. Between them they keep a team's secrets — environment files, passwords, keys,
certificates, tokens — so that the server stores them without ever being able to read them, and
the right people can put them back exactly where a project expects them.

A vault42 deployment has two services and, optionally, a third:

| Service | Protocol | What it holds | What it never holds |
|---|---|---|---|
| **vault42-authority** | HTTPS, JSON | accounts, sessions, organisations, teams, groups, projects, environments, grants, public keys, contracts | any secret, any private key, any plaintext |
| **vault42-server** | gRPC over HTTPS | sealed envelopes: your encrypted secrets, trees and notes | the keys to open them, file names, real paths |
| an **object store** (optional) | S3 | encrypted chunks of files larger than 4 MiB | keys, names, paths |

The public deployment is `https://vault42-authority.fly.dev` and `https://vault42-server.fly.dev`.
42ctl points at it out of the box. Chapter 12 and the operator's manual cover running your own.

## 1.2 Zero knowledge

Every encryption and decryption happens **inside 42ctl, on your machine**. What leaves it is an
*envelope*: ciphertext, the data key wrapped once for each recipient, and a signature by the author.

```text
   your machine                                          the server
  ┌───────────────────────────────┐                ┌────────────────────────┐
  │ srcs/.env ──seal──▶ envelope ──┼── gRPC/TLS ───▶│ stores the envelope     │
  │            (XChaCha20-Poly1305,│                │ checks the author's     │
  │             X25519 key wrap,   │                │ signature               │
  │             Ed25519 signature) │                │ cannot open it          │
  │ srcs/.env ◀─open── envelope ◀──┼── gRPC/TLS ────│                         │
  └───────────────────────────────┘                └────────────────────────┘
```

Consequences you should rely on:

- The server operator cannot read your secrets, and neither can anybody who steals its disk.
- **Real file names and paths never reach the server.** A project tree is stored under opaque ids;
  the names live only inside an encrypted manifest (chapter 7).
- **Nobody can recover a secret for you.** If the key that opens it is lost, it is gone. That is the
  property, not a limitation to work around (chapter 5, *Keeping your identity safe*).

## 1.3 The three things you hold

42ctl keeps three unrelated credentials. Nearly every refusal you meet is one of them missing.

| | **Identity** | **Session** | **Contract** |
|---|---|---|---|
| What it is | your key pair: Ed25519 to sign, X25519 to receive | a bearer token for your **account** | a signed statement binding your **key** to a tenant |
| Proves | *who wrote* and *who may open* an envelope | who you are to the authority | that your key may use the vault |
| Created by | `42ctl keys init` | `42ctl auth login --password --email EMAIL` | `42ctl auth login --tenant NAME` |
| Stored at | `~/.config/42ctl/keystore.v42`, sealed by your **passphrase** | `~/.config/42ctl/session-PROFILE.tok` | `~/.config/42ctl/contract-PROFILE.tok` |
| Needed by | everything that seals or opens | `org` `team` `group` `project` `env` `invite` `account` | `vault` `push` `pull` `note` `db` |

The environment verbs (`env push`, `env secret get`, …) need **all three**: membership comes from the
authority, the sealed data lives on the server, and only your identity can open it. One command
obtains both a session and a contract:

```sh
$ 42ctl auth login --password --email ada@example.com --tenant ada
```

The **passphrase** that unlocks your keystore is a fourth secret. It never leaves your machine and is
never sent anywhere.

## 1.4 Three places a secret can live

| Where | Commands | Sealed to | Who can read it |
|---|---|---|---|
| Your **personal vault**, one value per path | `vault set`, `vault get`, `vault share` | your identity | you, and anyone you `share` one secret with |
| Your **personal project tree** | `push`, `pull` | your identity | you, on any machine holding your identity |
| A **shared environment** of a project | `env secret set/get`, `env push`, `env pull` | the environment's key | every member granted that environment — except files marked private, which stay sealed to you |

Chapter 6 covers the first, chapter 7 the second, chapter 9 the third.

## 1.5 How permissions are organised

```text
organisation ─┬─ members (owner · admin · member)
              ├─ teams ─── members (admin · member)
              └─ projects ─┬─ groups ─── members
                           ├─ environments (dev, staging, prod …)
                           └─ grants: a user, a team or a group
                                      × a role (admin · write · read)
                                      × optionally one environment
```

A **grant** decides who may use an environment. It does not, by itself, give anybody a key: an
administrator runs `env keys sync`, which wraps the environment's key to every member the grants
authorize and who has published a public key (`keys enroll`). Removing a person removes their
authorization; **rotating the environment** (`env keys rotate`) is what ends their ability to open
anything written afterwards. Chapter 8 explains the model, chapter 9 the keys.

## 1.6 Getting help from the program

```sh
$ 42ctl help                      # the guided overview
$ 42ctl help kickoff              # the whole product, end to end
$ 42ctl help commands             # every command with its arguments
$ 42ctl env push --help           # one command, every flag, in full
```

`42ctl COMMAND -h` prints a one-screen summary; `--help` prints the long form with every
explanation. The command index in [appendix A](A-commands.md) is the same list, grouped.
