# The cloud controller

`42ctl cloud` drives the fly.io deployment that vault42 runs on. It does **not** re-implement
Fly's API: every verb shells out to the same `flyctl` an operator would type, so 42ctl inherits
its behaviour, its flags and its fixes instead of maintaining a parallel client that drifts from
them. vault42's own `deploy.yml` already drives flyctl the same way.

What 42ctl adds is the part flyctl cannot know: which two apps make up a vault42 deployment, and
what "healthy" means for them.

## Which flyctl runs

In order:

1. `FT_FLYCTL` — an absolute path. Also the test hook.
2. `fly` on `PATH`.
3. `flyctl` on `PATH`.
4. `docker run --rm -i -e FLY_API_TOKEN flyio/flyctl@sha256:39a83e2c…` — the same version
   vault42's deploy workflow pins, **by digest**. A program handed an org-wide token is not
   something to resolve through a mutable tag.

`FLY_API_TOKEN` is read from the environment at the moment of use. For the container form it is
passed **by name** (`-e FLY_API_TOKEN`), never as a value, so it does not reach an argument list,
`ps`, or `docker inspect`. Captured output is scrubbed of it before it can be folded into an
error chain. It is never written to `config.json`, which is plain JSON the user is invited to
read and share.

## Which apps

Derived from the profile's endpoints, so there is nothing to configure:

| endpoint | app |
|---|---|
| `https://vault42-server.fly.dev` | `vault42-server` |
| `https://vault42-authority.fly.dev` | `vault42-authority` |

Only a `*.fly.dev` host yields a name, and only at the first label — `evil.vault42-server.fly.dev`
is not the app. A profile pointing at a local stack names no app, and the refusal says to pass
`--app`. A verb that can only act on one app (`logs`, `wait`, a lifecycle verb) refuses when the
profile names two, rather than picking one.

## Verified flyctl surface

Checked against `flyio/flyctl:v0.4.101`. Re-run `--help` before assuming a flag on a newer one.

| verb | flags used |
|---|---|
| `machine list` | `--app --json -q` |
| `machine start\|stop\|restart\|suspend` | `--app` (`stop`/`suspend` also take `--wait-timeout`) |
| `machine wait` | `--app --state --wait-timeout` |
| `machine status` | `--app --display-config` — **no `--json`**, so `inspect` reads `machine list --json` instead |
| `machine destroy` | `--app --force` |
| `volumes list\|show` | `--app --json` |
| `volumes create` | `--app --region --size --yes --json --snapshot-retention` |
| `volumes extend\|fork\|destroy` | `--app --json` (`extend`/`destroy` take `--yes`) |
| `volumes snapshots list\|create` | `--app --json` |
| `secrets list` | `--app --json` |
| `secrets import\|unset\|deploy` | `--app --stage --detach` |
| `status` / `releases` / `scale show` | `--app --json` |
| `logs` | `--app --machine --no-tail --json` |
| `ips list` / `certs list` | `--app --json` |
| `apps list` / `orgs list` / `apps create` | `--json` (`create` takes `--org --name`) |
| `config show` | `--app --local --config --toml --yaml` (JSON by default) |
| `checks list` | `--app --json` |

**Not available, and why:**

- `machine stats` / `top` — flyctl has no command. They would need direct calls to Fly's
  Prometheus endpoint and the Machines `/ps` route, the only two places where delegation is not
  possible. Not shipped yet; when they are, they will say in their help that they bypass flyctl.
- `machine exec … ps` is not a substitute: both images are distroless and have no shell.

## `cloud health`

The checks, and what each one is protecting against:

| check | fails when | why it matters |
|---|---|---|
| machine count | ≠ 1 per app | both apps keep SQLite **on a volume**; a second machine is two writers on one file, not redundancy |
| machine state | not `started` / `stopped` / `suspended` | anything else is a machine mid-failure |
| scope keys | `VAULT42_SCOPE_KEYS_ENABLED` is not `1` on the server | every `env init`, `env secret set`, `env keys sync`, `env keys rotate` answers `UNIMPLEMENTED`, which reads as a client bug |
| volume | detached, or not encrypted | the volume is the only copy of the database |
| snapshot age | > 7 days (warns > 36 h, or when there is none) | says how much a mistake would cost |
| authority `/healthz` | not `200 ok` | the control plane is down |
| contract key | not 64 lowercase hex | **the failure that looks healthy from outside**: the authority answers, the server starts, and every contract it issues is worthless |

Warnings never affect the exit status, so this works as a gate without flapping on an ageing
snapshot. `--no-wake` skips the two HTTP probes, which would otherwise start a scale-to-zero
machine just to be asked whether it is alive.

**Not yet checked, deliberately:** whether the server's pinned `VAULT42_CONTRACT_PUBKEY` equals
the authority's current key. `fly secrets list` returns a digest whose algorithm is undocumented,
so comparing it to a hash of the key would be guesswork presented as a verdict. The honest test is
end-to-end — sign a `Whoami` with a contract and see it accepted — which needs the caller's
passphrase. Until that lands, the check is absent rather than green.

## Things that will bite

- **Deleting the authority's volume destroys the contract signing key.** `fly.authority.toml`
  keeps `VAULT42_AUTHORITY_KEY` at `/data/authority.key`, on the volume. Every contract ever
  issued dies with it.
- **flyctl's machine leases** protect one machine update. They do **not** protect the
  stage-secrets → deploy → stage-key → deploy sequence. vault42's `deploy.yml` and `scenario.yml`
  share a `fly-apps` concurrency group for exactly that reason; a local deploy races them.
- **Every mutating verb echoes its flyctl command to stderr before running**, and `--dry-run`
  prints that line and stops. The echoed line is exactly what an operator could paste back.
