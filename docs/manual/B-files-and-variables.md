# Appendix B. Files and environment variables

## B.1 Files 42ctl reads and writes

| Path | Written by | Contents | Mode |
|---|---|---|---|
| `~/.config/42ctl/config.json` (or `$FT_CONFIG`) | `config endpoint`, `config profile` | profiles and endpoints — no secret | your umask |
| `~/.config/42ctl/keystore.v42` (or `$FT_KEYSTORE`) | `keys init`, `keys recover` | the key pair, sealed with the passphrase | `0600` |
| `session-PROFILE.tok` beside the config (or `$FT_SESSION`) | `auth login` | the account session | `0600` |
| `contract-PROFILE.tok` beside the config (or `$FT_CONTRACT`) | `auth login --tenant` | the vault contract | `0600` |
| `PROJECT/.42ctl/project.json` | the first `push` or `note add` without one | project id and scan patterns — commit it | as created |
| `PROJECT/.42ctl/sync.json` | `push`, `pull --apply` | the per-machine merge base: digests and revisions — ignore it in git | as created |
| restored files | `pull --apply`, `env pull --apply` | your secrets | recorded mode (personal); owner-only (environment) |
| `NAME.bak` | `--backup` | the file a restore replaced | as it was |
| `NAME.remote` | `pull --apply` on a binary conflict | the stored version beside yours | as recorded |

`~/.config` is `$XDG_CONFIG_HOME` when that variable is set.

## B.2 Environment variables

| Variable | Used for |
|---|---|
| `FT_PROFILE` | the profile, like `--profile` |
| `FT_CONFIG` | the configuration file; session and contract files sit beside it |
| `FT_KEYSTORE` | the keystore file |
| `FT_SESSION`, `FT_CONTRACT` | the session and contract files |
| `FT_PASSPHRASE` | the keystore passphrase, for automation |
| `FT_PASSWORD` | the account password, for automation — a different secret |
| `FT_LOGIN_EMAIL` | `--email` for `auth signup`, `auth login`, `keys escrow`, `keys recover` |
| `FT_REGISTER_TOKEN` | `--token` for `auth signup` and `auth login` |
| `FT_S3_KEY`, `FT_S3_SECRET` | the object store's credential, never stored |
| `FLY_API_TOKEN` | `42ctl cloud` only; read when a command runs, never stored |
| `NO_COLOR`, `CLICOLOR_FORCE` | colour off; colour on even when piped |
| `XDG_CONFIG_HOME` | the base of the default configuration directory |

## B.3 Installer variables

| Variable | Option |
|---|---|
| `FT_VERSION` | `--version vX.Y.Z` |
| `FT_BIN_DIR` | `--bin-dir DIR` |
| `FT_NO_MODIFY_PATH` | `--no-modify-path` |
