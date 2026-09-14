# 10. Credentials cookbook

Every recipe here uses only the mechanisms of chapters 6, 7 and 9. What differs between kinds of
credential is where they sit on disk and who should be able to read them.

## 10.1 Where does a credential belong?

| The credential is… | Keep it in | With |
|---|---|---|
| yours alone, one value (a personal token) | your personal vault | `vault set` / `vault get` |
| a file your project needs, used only by you | your personal tree | `push` / `pull` |
| one value the whole team uses (a registry token, a webhook secret) | an environment | `env secret set` / `env secret get` |
| files the whole team's checkout needs (`.env`, Docker secrets, certificates) | an environment's tree | `env push` / `env pull` |
| a file only you need **inside** a shared checkout (`.env.local`, your own key) | the same tree, private | `*.local`, or `env push --private` |
| different for each stage | one environment per stage | `dev`, `staging`, `prod` with separate grants |

Separate environments are separate keys. Granting a contractor `read` on `dev` gives them nothing of
`prod`.

## 10.2 Environment files

A project usually has three kinds:

| File | Contents | In git? | In the vault? |
|---|---|---|---|
| `.env.example` | names and harmless defaults | **yes** | taken by `*.env*` too — harmless |
| `srcs/.env`, `.env`, `.env.production` | the real values | **never** | shared, in the environment |
| `.env.local`, `srcs/.env.local` | one person's overrides | **never** | private, sealed to that person |

```sh
$ 42ctl env push --org acme --project api --env prod
$ 42ctl env pull --org acme --project api --env prod --apply
```

Keep whole files whole. `vault import` flattens a `.env` into one secret per key and does not
understand quoting or multi-line values (chapter 6); a tree push restores the file byte for byte.

## 10.3 Docker Compose secrets

Compose mounts secrets from files, conventionally in a `secrets/` directory. 42ctl takes **every file
under any `secrets/` directory**, whatever its name, so nothing needs configuring:

```yaml
# docker-compose.yml
secrets:
  db_password:
    file: ../secrets/db_password.txt
  server_key:
    file: ../secrets/server.key
```

```sh
$ 42ctl env push --org acme --project api --env prod          # secrets/ goes with srcs/.env
$ 42ctl env pull --org acme --project api --env prod --apply  # secrets/ comes back 0700, its files 0600
$ docker compose -f srcs/docker-compose.yml up -d
```

Chapter 13 does exactly this with a real project and proves the restored stack's credentials are the
ones that were pushed.

## 10.4 Passwords, API tokens, connection strings

One value, used by scripts and deployments:

```sh
$ printf '%s' "$NEW_TOKEN" | 42ctl env secret set CI_REGISTRY_TOKEN --org acme --project api --env prod
$ 42ctl env secret get CI_REGISTRY_TOKEN --org acme --project api --env prod
```

Use it without writing it to disk or showing it in the process list:

```sh
$ docker login registry.example.org -u ci --password-stdin < <(42ctl env secret get CI_REGISTRY_TOKEN --org acme --project api --env prod)
```

Never put a secret in a command's arguments (`--password s3cret`): arguments are visible to every user
of the machine and are kept in shell history. `env secret set` and `vault set` read from standard input
for that reason.

## 10.5 TLS certificates and private keys

```text
secrets/ca.crt      the certificate authority's certificate   shared
secrets/ca.key      the certificate authority's private key   consider keeping it private
secrets/server.crt  the server's certificate                   shared
secrets/server.key  the server's private key                   shared (the server needs it)
```

Everyone who deploys needs `server.crt` and `server.key`. Fewer people need to **issue** certificates,
so a CA's private key is a good candidate for a private file, or for its own restricted environment:

```sh
$ 42ctl env push --org acme --project api --env prod --private 'secrets/ca.key'
```

Restored keys come back owner-only (`0600`), which is what TLS servers and `ssh` insist on. After a
restore, `openssl x509 -in secrets/server.crt -noout -pubkey` and `openssl pkey -in secrets/server.key
-pubout` print the same key: the pair is intact.

## 10.6 SSH keys

```sh
$ mkdir -p secrets/ssh && cp ~/.ssh/deploy_ed25519 secrets/ssh/
$ 42ctl env push --org acme --project api --env prod
$ 42ctl env pull --org acme --project api --env prod --only 'secrets/ssh/*' --apply
$ ssh -i secrets/ssh/deploy_ed25519 deploy@host.example.org
```

A personal key used only by you belongs in your personal vault or tree, not in a shared environment:

```sh
$ 42ctl vault set ssh/id_ed25519 --file ~/.ssh/id_ed25519
$ 42ctl vault get ssh/id_ed25519
```

## 10.7 Cloud credential files, kubeconfigs, keystores

Service-account JSON files, `kubeconfig`, `.p12` and `.jks` keystores, GPG keyrings: put them under a
`secrets/` directory, or add a name pattern to the marker (chapter 7) and commit the marker:

```json
{ "project_id": "…", "patterns": ["*.env*", "*.secrets", "*.p12", "kubeconfig", "service-account.json"] }
```

They are restored byte for byte; binary files are never altered.

## 10.8 Large artefacts

A database dump, a model file or an archive larger than about 4 MiB is chunked to the object store
(chapter 7, *Large files*). Every member who pushes or pulls it needs the store configured and its
credential in `FT_S3_KEY` / `FT_S3_SECRET`. A private file cannot be larger than 4 MiB.

## 10.9 Public and private repositories

42ctl lets a project's **code be public while its secrets are not**, and lets a new checkout of either
kind become a working project in one command.

**What to commit:**

- `.42ctl/project.json` — so every checkout names the same project and the same patterns;
- `.env.example` and documentation of which variables exist.

**What never to commit** — put it in `.gitignore` before the first push:

```gitignore
# secrets live in vault42, restored by: 42ctl env pull … --apply
secrets/
.env
srcs/.env
.env.local
srcs/.env.local
*.bak
*.remote
.42ctl/sync.json
```

Check that nothing secret is tracked:

```sh
$ git status --porcelain --untracked-files=all
$ git check-ignore -v secrets/server.key srcs/.env
```

**A new checkout — a teammate, a CI runner, your laptop after a reinstall:**

```sh
$ git clone https://github.com/acme/api.git && cd api
$ 42ctl env pull --org acme --project api --env prod
$ 42ctl env pull --org acme --project api --env prod --apply
```

The same holds for a private repository; 42ctl does not care which. What a private repository adds is
defence in depth, never a reason to commit a secret: repositories are cloned, forked, mirrored and made
public, and their history keeps every file ever committed.

> If a secret was ever committed, removing the file is not enough: it stays in history and in every
> clone. **Change the credential at its source**, then store the new value with 42ctl.

## 10.10 CI and other automation

A pipeline should be a **member of its own**, with the least role it needs — usually `read` on one
environment — and its own identity, so that it can be removed and rotated out without touching people.

Once, on a trusted machine, in a directory of its own so that the pipeline's session and contract
never replace yours:

```sh
$ mkdir ci && export FT_CONFIG="$PWD/ci/config.json" FT_KEYSTORE="$PWD/ci/keystore.v42"
$ 42ctl keys init
$ 42ctl auth signup --email ci-api@example.com
$ 42ctl auth login --password --email ci-api@example.com --tenant ci-api
$ 42ctl invite accept --token TOKEN
$ 42ctl keys enroll --org acme
$ base64 -w0 ci/keystore.v42
```

An administrator grants it `read` on `prod` and runs `env keys sync`. Store four values in the CI
system's secret store: the base64 keystore, its passphrase, the account's email and its password. Then,
in the pipeline:

```yaml
# GitHub Actions
- name: restore secrets
  env:
    FT_KEYSTORE: ${{ runner.temp }}/keystore.v42
    FT_PASSPHRASE: ${{ secrets.C42_PASSPHRASE }}
    FT_PASSWORD: ${{ secrets.C42_PASSWORD }}
    FT_LOGIN_EMAIL: ${{ secrets.C42_EMAIL }}
    C42_KEYSTORE_B64: ${{ secrets.C42_KEYSTORE_B64 }}
  run: |
    curl -fsSL https://raw.githubusercontent.com/Univers42/42ctl/main/install.sh | sh
    printf '%s' "$C42_KEYSTORE_B64" | base64 -d > "$FT_KEYSTORE"
    ~/.local/bin/42ctl auth login --password --tenant ci-api
    ~/.local/bin/42ctl env pull --org acme --project api --env prod --apply
```

`FT_PASSPHRASE` unlocks the keystore without a prompt; `FT_PASSWORD` answers the account password
prompt. When the pipeline's access must end, remove its account from the organisation and rotate the
environment (chapter 9).

## 10.11 Changing a credential

42ctl rotates **keys**; it cannot change the credential a key protects. When a value may have leaked —
or somebody who could read it leaves:

1. change it at its source: the database password, the API token, the certificate;
2. store the new value: `env secret set`, or edit the file and `env push`;
3. if a person left, remove them and run `env keys rotate` (chapter 9).
