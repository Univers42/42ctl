# 3. A first session

This chapter takes two people from nothing to a shared secret: **ada** founds the organisation
**acme** and stores a database password for its project **api** in the **prod** environment, and
**bea** — a teammate on another machine — reads it. Each step is explained in full in the chapter
named beside it.

## 3.1 Ada: an identity and an account (chapter 5)

```sh
$ 42ctl keys init
$ 42ctl auth signup --email ada@example.com
$ 42ctl auth login --password --email ada@example.com --tenant ada
$ 42ctl auth status
profile 'default': logged in — session and contract
```

`keys init` asks for a **new passphrase** twice; it seals the key pair on disk and is asked for
whenever the key is used. `auth signup` asks for the account's **password**, a different secret.
If the deployment restricts account creation, sign-up also needs the invitation token its operator
gives you: `--token TOKEN`.

`--tenant ada` claims a tenant name for this identity and saves the contract the vault requires. Any
name that is free will do; it is how the vault knows which key is yours.

## 3.2 Ada: a personal secret (chapter 6)

```sh
$ printf 'postgres://app:s3cret@db.internal/app' | 42ctl vault set app/DATABASE_URL
pushed app/DATABASE_URL (v1)
$ 42ctl vault get app/DATABASE_URL
postgres://app:s3cret@db.internal/app
```

Only ada can read this. The next steps share a secret with the team instead.

## 3.3 Ada: an organisation, a project, an environment (chapter 8)

```sh
$ 42ctl org create --slug acme --name 'ACME'
$ 42ctl project create --org acme --slug api --name 'API'
$ 42ctl env create --project api --name prod
$ 42ctl org invite --org acme --email bea@example.com --role member
invite_id 6f0c…
token     Kp3v…
invited bea@example.com to org 'acme' as 'member'
```

Send bea the **token**. It is single-use, bound to her address, and expires.

## 3.4 Bea: join (chapters 5 and 8)

On her own machine:

```sh
$ 42ctl keys init
$ 42ctl auth signup --email bea@example.com
$ 42ctl auth login --password --email bea@example.com --tenant bea
$ 42ctl invite accept --token TOKEN
$ 42ctl keys enroll --org acme
```

`keys enroll` publishes bea's **public** keys to the organisation, so an administrator can seal the
environment's key to her. Her private key never leaves her machine.

## 3.5 Ada: grant and provision (chapters 8 and 9)

```sh
$ 42ctl project grant add --org acme --project api --user bea@example.com --role read --env prod
$ 42ctl keys enroll --org acme
$ 42ctl env init --org acme --project api --env prod
$ 42ctl env keys sync --org acme --project api --env prod
$ 42ctl env keys ls --org acme --project api --env prod
```

- The **grant** says bea may read `prod`.
- `env init` generates `prod`'s key pair, publishes the public half and seals the private half to
  ada. It is run once per environment.
- `env keys sync` seals the environment's key to every member a grant authorizes who has enrolled.
- `env keys ls` shows each member as `active` once they hold the key.

## 3.6 Ada writes, bea reads (chapter 9)

```sh
$ printf 's3cret-prod-password' | 42ctl env secret set DB_PASSWORD --org acme --project api --env prod
```

On bea's machine:

```sh
$ 42ctl env secret get DB_PASSWORD --org acme --project api --env prod
s3cret-prod-password
```

The password travelled sealed to `prod`'s key. The server stored it without being able to read it,
and bea opened it with the copy of `prod`'s key that was sealed to her.

## 3.7 Where to go from here

- A whole project — `.env` files, certificates, keys — rather than one value: chapter 9, *Sharing a
  whole tree*, and the complete worked example in chapter 13.
- Removing bea again, so she cannot read what is written afterwards: chapter 9, *When somebody
  leaves*.
- Scripting any of this: chapter 11.
