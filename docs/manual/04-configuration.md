# 4. Configuration

A fresh installation needs no configuration: it points at the public deployment. This chapter is
for pointing 42ctl somewhere else, keeping several deployments or identities apart, and knowing
exactly what 42ctl keeps on disk.

## 4.1 Profiles

A **profile** is a named, independent world: its own endpoints, its own session and its own
contract. Nothing leaks from one profile to another. The profile in use is, in order of precedence:

1. `--profile NAME` on the command line;
2. the `FT_PROFILE` environment variable;
3. the profile last selected with `42ctl config profile NAME`;
4. `default`.

```sh
$ 42ctl config profile                    # list profiles; the active one is marked
$ 42ctl config profile staging            # switch to 'staging', creating it from the current endpoints
$ 42ctl --profile staging auth status     # use 'staging' for one command without switching
```

A profile shares the **keystore** with every other profile unless you point `FT_KEYSTORE`
elsewhere: one identity, several deployments. To be two different people, use two keystores.

## 4.2 Endpoints

```sh
$ 42ctl config show
profile   default
server    https://vault42-server.fly.dev
authority https://vault42-authority.fly.dev
control-plane https://vault42-authority.fly.dev
$ 42ctl config endpoint --server https://vault.example.org --authority https://auth.example.org
```

| Option of `config endpoint` | What it names |
|---|---|
| `--server URL` | vault42-server, the gRPC store of sealed envelopes |
| `--authority URL` | vault42-authority: accounts, organisations, permissions, contracts |
| `--grobase URL` | an override for the email-code and escrow routes only; leave it unset — the authority serves them |
| `--blobstore URL` | an S3-compatible object store for files above 4 MiB (its root, without the bucket) |
| `--bucket NAME` | the bucket in that store |
| `--region REGION` | the signing region (default `us-east-1`, which every S3-compatible server accepts) |

Only the options you pass change; the rest of the profile is kept. `http://` endpoints are
accepted, which is what a laboratory deployment on `127.0.0.1` uses; anything reachable by other
people must be `https://`.

The object store's **credential is never written to the configuration**. 42ctl reads it from
`FT_S3_KEY` and `FT_S3_SECRET` at the moment it uploads or downloads a chunk:

```sh
$ 42ctl config endpoint --blobstore https://s3.example.org --bucket acme-chunks --region eu-west-3
```

## 4.3 What 42ctl keeps on disk

All in `~/.config/42ctl/` (more precisely `$XDG_CONFIG_HOME/42ctl/` when that variable is set). Every
file holding a credential is written readable by you alone (`0600`), and narrowed to that if it was
wider:

| File | Contents | Secret? |
|---|---|---|
| `config.json` | profiles and endpoints; written the first time you change something | no — safe to read and share |
| `keystore.v42` | your key pair, sealed with your passphrase (Argon2id + AEAD) | **yes** — useless without the passphrase, irreplaceable with it |
| `session-PROFILE.tok` | the account session for that profile | **yes** — a bearer credential; `auth logout` deletes it |
| `contract-PROFILE.tok` | the vault contract for that profile | **yes** — bound to your key; `auth logout` deletes it |

Inside a project, 42ctl uses a `.42ctl/` directory:

| File | Contents | Commit it? |
|---|---|---|
| `.42ctl/project.json` | the project id and the patterns that say which files are secrets (chapter 7) | **yes** — it holds no secret, and every teammate needs the same one |
| `.42ctl/sync.json` | the merge base of your last personal `pull` or `push`: BLAKE3 digests and revisions | no — it is per machine; ignore it |

## 4.4 Environment variables

| Variable | Replaces | Notes |
|---|---|---|
| `FT_PROFILE` | `--profile` | |
| `FT_CONFIG` | the configuration file path | session and contract files are kept beside it |
| `FT_KEYSTORE` | the keystore path | |
| `FT_SESSION` | the session file path | |
| `FT_CONTRACT` | the contract file path | |
| `FT_PASSPHRASE` | typing the **keystore passphrase** | for automation only (chapter 11) |
| `FT_PASSWORD` | typing the **account password** | a different secret, deliberately a different variable |
| `FT_LOGIN_EMAIL` | `--email` on `auth signup`, `auth login`, `keys escrow`, `keys recover` | |
| `FT_REGISTER_TOKEN` | `--token` on `auth signup` and `auth login` | the operator's invitation token |
| `FT_S3_KEY`, `FT_S3_SECRET` | — | object-store credential for large files; never stored |
| `NO_COLOR` | — | plain output; `CLICOLOR_FORCE` keeps colour when output is piped |

> `FT_PASSPHRASE` and `FT_PASSWORD` are two different secrets on purpose. A single variable for
> both would silently make your keystore passphrase and your account password the same value in
> every automated run.
