# 5. Identity and accounts

## 5.1 Your identity

```sh
$ 42ctl keys init
$ 42ctl keys export-pub
v42:yTN_EpS2M_nD1eB7UaTiUU1TzfdDopHg-Vg5P555_f9hFrwquLu_MxgdmGsuvTjovBNs_VzMcMi-fx8nxcWDHA
$ 42ctl auth whoami
principal 5aa9cd1b2cd46c6443a220d1ecaa520d
address   v42:yTN_EpS2M_nD1eB7UaTiUU1TzfdDopHg-Vg5P555_f9hFrwquLu_MxgdmGsuvTjovBNs_VzMcMi-fx8nxcWDHA
contract  bound (profile 'default')
```

`keys init` generates an **Ed25519** signing key and an **X25519** encryption key and writes them to
`keystore.v42`, sealed with a passphrase you choose. It refuses to replace an existing keystore
unless you pass `--force` — **which destroys the old identity and everything sealed only to it**.

| Name | What it is | Who needs it |
|---|---|---|
| **principal** | the fingerprint of your signing key; the vault's name for you and the owner of your secrets | nobody — it is shown for diagnosis |
| **address** (`v42:…`) | both public keys, encoded | anybody who wants to `vault share` a secret with you |

Both are public. Only the passphrase and the keystore file are secret.

## 5.2 Keeping your identity safe

- **The passphrase cannot be reset.** Nobody holds a copy, and no server can open your keystore.
  Choose one you can keep, and keep it in a password manager.
- **Back the keystore up.** Either `keys escrow` (§5.5), which stores it — still sealed — on the
  authority, or copy `~/.config/42ctl/keystore.v42` to a safe place yourself.
- **On a shared machine**, run `42ctl auth logout` when you finish. It deletes the session and the
  contract; the keystore stays, protected by the passphrase.

## 5.3 Accounts

An **account** is an email address and a password on the authority. It is what organisations,
teams and grants refer to, and what a contract is issued to.

### Signing up

```sh
$ 42ctl auth signup --email ada@example.com
$ 42ctl auth signup --email ada@example.com --token TOKEN
```

The command asks for a password. A deployment whose operator restricts account creation requires
the invitation token they give you, as `--token` or `FT_REGISTER_TOKEN`.

Sign-up answers the same way whether or not the address already had an account, so that it cannot be
used to discover who is registered: *if ada@example.com was not already registered, it is now —
log in to obtain a session.*

### Signing in

```sh
$ 42ctl auth login --password --email ada@example.com                # a session only
$ 42ctl auth login --password --email ada@example.com --tenant ada   # a session and a contract
$ 42ctl auth login --tenant ada                                      # a contract, with the session you have
$ 42ctl auth login --github                                          # a session through GitHub's device flow
```

- **`--password --email`** asks for the password and saves a **session**.
- **`--tenant NAME`** claims the tenant `NAME` for your identity and saves a **contract**. It needs a
  session, because a contract is issued to an account. An account may hold up to 8 tenant names (the
  operator can change this), and may rebind a name it holds to a new key — which is how you recover
  after losing a keystore (§5.6).
- **`--github`** prints a code and a URL, waits while you approve it on GitHub, and signs you in to
  the account holding the **verified** email address GitHub reports. It works only where the
  operator has configured a GitHub application; it never creates an account.

If the account has a second factor (§5.4), sign-in asks for the 6-digit code emailed to you before it
saves anything.

### Knowing where you stand

| Question | Command | Answers with |
|---|---|---|
| Is this profile signed in, and with what? | `42ctl auth status` | `logged in — session and contract`, `— session only`, or `logged out` |
| Which account is the session for? | `42ctl auth me` | account id, email, whether a second factor is required |
| Which key, which address, is a contract bound? | `42ctl auth whoami` | principal, address, contract |

`42ctl account show` is `auth me` under the account's own name.

### Changing the password, signing out

```sh
$ 42ctl auth passwd
$ 42ctl auth logout
```

`auth passwd` asks for the current password and the new one, and **revokes every session of the
account**, including the one in use: sign in again afterwards. `auth logout` forgets the session and
contract of this profile on this machine.

## 5.4 A second factor

```sh
$ 42ctl auth mfa --on
$ 42ctl auth mfa --off
```

Off unless you turn it on. When on, every sign-in — password or GitHub — asks for a 6-digit code sent
to the account's address before a session is issued. Both turning it on and turning it off ask for a
code first, so a stolen session can neither lock you out nor remove the factor. It needs the
deployment to have outgoing mail configured.

## 5.5 The same identity on a second machine

A second machine that runs `keys init` is a **different person** to the vault: it cannot open what
the first identity sealed. To use the same identity, move the keystore.

**With escrow**, through the authority:

```sh
$ 42ctl keys escrow --email ada@example.com          # on the first machine
$ 42ctl keys recover --email ada@example.com         # on the second
```

Each asks for a code emailed to the address. `escrow` uploads the keystore still sealed by your
passphrase — the authority stores ciphertext it cannot open. `recover` downloads it and asks for the
same passphrase. Then sign in on the second machine as usual.

**By hand**, copy `~/.config/42ctl/keystore.v42` over a channel you trust (never by email or chat), to
the same place on the second machine.

Personal trees, personal secrets and environments are then all readable from both machines.

## 5.6 When something is lost

| Lost | Consequence | What to do |
|---|---|---|
| The **password** | nothing sealed is lost, but **there is no password reset** | sign in with `auth login --github` where the deployment offers it; otherwise sign up with a new address and ask to be invited again — your identity, and everything sealed to it, carries over unchanged |
| A **session** or **contract** | nothing | sign in again |
| The **passphrase** or the **keystore**, with no backup | **everything sealed only to that identity** — personal secrets, personal trees, private files in environments — is unrecoverable | create a new identity, rebind your tenant to it, and ask to be re-provisioned (below) |

Starting again after losing an identity:

```sh
$ 42ctl keys init --force
$ 42ctl auth login --password --email ada@example.com --tenant ada
$ 42ctl keys enroll --org acme
```

Rebinding the same tenant name to the new key is allowed for the account that holds it. `keys
enroll` replaces your published public key in each organisation you run it for; an administrator
then runs `env keys sync` for each environment you use, and everything **shared** with you is
readable again. What was sealed only to the lost identity is not.

## 5.7 Deleting an account

```sh
$ 42ctl account delete --yes
```

**Irreversible.** It deletes the account, every session and every organisation membership, and
**releases every tenant name the account claimed** so that anybody may claim them afterwards.
Without `--yes` it refuses and says what would be lost. It is also refused while the account is the
**last owner** of any organisation, which would leave that organisation with nobody able to
administer it: invite another owner first (`org invite --role owner`).

If what you want is a fresh key rather than a fresh start, do not delete: follow §5.6.
