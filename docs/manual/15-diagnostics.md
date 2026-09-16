# 15. Diagnostics

## 15.1 First questions

| Question | Command |
|---|---|
| Which 42ctl is this? | `42ctl version` |
| Which deployment, which profile? | `42ctl config show` |
| Signed in? With a session, a contract, both? | `42ctl auth status` |
| Which account, which key? | `42ctl auth me`, `42ctl auth whoami` |
| Is the deployment up, and which release? | `curl -fsS AUTHORITY/healthz`, `curl -fsS AUTHORITY/version` |
| Why can a member not read an environment? | `42ctl env keys ls --org ORG --project PROJECT --env ENV` |
| What does an environment hold? | `42ctl env files --org ORG --project PROJECT --env ENV` |
| What would a restore do? | the same `env pull` or `pull` **without** `--apply` |

## 15.2 Messages and what to do

### Signing in and credentials

| Message | Meaning | What to do |
|---|---|---|
| `not logged in — run 42ctl auth login --password --email <address>` | no session for this profile | sign in |
| `missing auth metadata` | no contract, so the vault refuses the request | `42ctl auth login --tenant NAME` |
| `registration refused: … issues a contract to an ACCOUNT` | `--tenant` without a session | `auth login --password --email EMAIL --tenant NAME` |
| `HTTP 401` from `auth signup` | the deployment restricts account creation | ask the operator for the invitation token: `--token TOKEN` |
| `this account already holds its maximum tenant names` | the per-account tenant quota (8 by default) | reuse a tenant name you hold |
| an error unlocking the keystore | the passphrase does not open `keystore.v42` | retype it; check `FT_PASSPHRASE`; it cannot be reset |
| `GitHub sign-in was refused (HTTP 401): …` | GitHub denied the code, or no account holds the verified address | approve the code; sign up with that address first |

### Organisations and grants

| Message | Meaning | What to do |
|---|---|---|
| `HTTP 404` from an organisation command | you are not a member, or the name is wrong — the two are indistinguishable on purpose | check the slug with a member; ask to be invited |
| `HTTP 403` | you are a member without the role the action needs | ask an administrator (chapter 8, *Roles*) |
| `no member "x@example.com" in this organization` | the person has not joined | invite them and have them accept first |
| `no team "x" in this organization` | wrong team slug, or wrong organisation | `42ctl team ls --org ORG` |
| `project "x" names a project in more than one of your organizations; use its id` | a slug you hold in two organisations, on a command without `--org` | pass the project's id from `project ls` |
| `project_role must be admin, write or read` | a role outside the set | `admin`, `write` or `read` — not `reader`/`writer` |
| `no environment 'x' in project 'y'` | the environment does not exist | `42ctl env create --project y --name x` |

### Environment keys

| Message | Meaning | What to do |
|---|---|---|
| `no scope key for this env — run env init first` | the environment was never initialised, or you hold no key for its current epoch | an administrator runs `env init` once; then `env keys sync` |
| `could not open the scope key — are you a wrapped member?` | you are granted but not provisioned | an administrator runs `env keys sync`; check `env keys ls` |
| `pending-enrollment` in `env keys ls` | that member never ran `keys enroll` | they run `42ctl keys enroll --org ORG` |
| `only a member of this scope may read or write its env secrets` | the vault holds no key of yours for this environment | as above; after a removal and rotation this is the intended result |
| `already has a scope key … with N provisioned member(s)` | `env init` on an initialised environment | use `env keys sync`, or `env keys rotate` for a new key |
| `UNIMPLEMENTED` from any `env` key or secret command | the server runs without `VAULT42_SCOPE_KEYS_ENABLED` | the operator enables it (operator's manual) |

### Pushing and restoring

| Message | Meaning | What to do |
|---|---|---|
| `… was pushed by somebody else while this push ran, so nothing of this push was published` | another push landed first | `env pull` to see theirs; push again to replace it |
| `… had its key rotated while this push ran` | a rotation finished before your push | push again |
| `…: the manifest names revision N, which this environment does not hold` | a tree rotated by a 42ctl older than 0.1.9 | push the tree again from a good copy |
| `no file in environment 'prod' matches …` | an `--only` pattern matched nothing; nothing was restored | fix the pattern; `env files -q` lists the paths |
| `no row matches …` | a `--filter` kept nothing | check the value; the listing without `--filter` shows what exists |
| `refusing to write through a symlinked path` | a directory on the restore path is a symbolic link | replace the link with a real directory |
| `illegal path form (nul/backslash/absolute/drive/unc)`, `empty or dot path component`, `path uses the reserved __42ctl prefix` | a manifest entry whose path could escape the project or is malformed | nothing was written; report it — a writer of the environment produced that manifest |
| `scan pattern … names a path, but patterns match file names` | a `/` in `.42ctl/project.json` | use a name pattern, or keep the file under `secrets/` |
| `… and this profile names no object store` | a file over 4 MiB with no endpoint/bucket configured | `config endpoint --blobstore URL --bucket NAME` |
| `… names object-store bucket `X`, but FT_S3_KEY and FT_S3_SECRET are not set` | the location IS configured; only the credential is missing | export `FT_S3_KEY` / `FT_S3_SECRET` — re-running `config endpoint` cannot help |
| `kept N manifest entries whose file is present but was not scanned` | the tree is incomplete (often a non-recursive clone), so `--prune` refused to drop them | push from a `git clone --recursive` checkout |
| `could not keep a backup of … — nothing was written` | `--backup` could not rename the existing file | check the directory's permissions and free space |
| `your private copy shadows the shared one` (a notice) | you hold a private file at a shared path | nothing — yours was restored; rename one if both are needed |
| `this is 42ctl's own record …` from `vault rm` | a path under `__42ctl/` | remove notes with `note rm`; trees are replaced by pushing |

## 15.3 Reporting a bug

Run the failing command again with `NO_COLOR=1`, keep its complete output, and open an issue with it,
`42ctl version` and `42ctl config show`. Remove anything secret first; 42ctl never prints a secret in an
error, but your shell's arguments might contain one.
