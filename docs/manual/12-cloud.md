# 12. Operating the deployment

`42ctl cloud` inspects and operates the fly.io deployment vault42 runs on. It is for operators. It
does not install or configure a deployment — that is the subject of the operator's manual in the
vault42 repository — and **it never deletes anything**: no command here destroys an application, a
machine, a volume or a snapshot.

## 12.1 Requirements

- **flyctl**: 42ctl runs `fly` or `flyctl` from `PATH`, or, when neither is installed, a flyctl container
  pinned by digest. It behaves exactly as the flyctl command it runs.
- **`FLY_API_TOKEN`** in the environment. It is read when a command runs and is never written anywhere.
  A **read-only** token is enough for everything except the commands marked *admin*; those need a token
  allowed to change machines.

The applications are derived from the profile's endpoints: `https://vault42-server.fly.dev` means the
fly application `vault42-server`. Nothing needs configuring.

## 12.2 Is it well?

```sh
$ 42ctl cloud apps
$ 42ctl cloud status
$ 42ctl cloud health
$ 42ctl cloud health --no-wake
```

`cloud health` checks what flyctl cannot: that the authority answers, that its contract key is 64
hexadecimal characters, that the server has the scope-key feature enabled, that an encrypted volume is
attached, and how old the newest snapshot is. **It exits non-zero when a check fails**, so it can gate a
script; warnings do not change the exit status.

> The HTTP probes **wake** a scale-to-zero machine, which costs a few seconds of compute.
> `--no-wake` skips them.

The deployed release is also one request away, and costs nothing when the machine is already awake:

```sh
curl -fsS https://vault42-authority.fly.dev/version
```

## 12.3 Machines

```sh
$ 42ctl cloud machine ls
$ 42ctl cloud machine ls --app vault42-server --format 'table {{.ID}}\t{{.State}}\t{{.Volume}}'
$ 42ctl cloud machine inspect MACHINE_ID
$ 42ctl cloud machine ports
$ 42ctl cloud machine events MACHINE_ID --app vault42-server
$ 42ctl cloud machine logs --app vault42-server --no-tail
$ 42ctl cloud machine top MACHINE_ID --app vault42-server
```

`machine top` lists the processes inside a running machine; it is the one verb that calls fly's Machines
API directly, because flyctl cannot report it.

**Lifecycle** (*admin*). Each takes several machine ids, prints the flyctl command it is about to run,
and with `--dry-run` stops there:

```sh
$ 42ctl cloud machine stop MACHINE_ID --app vault42-server --dry-run
$ 42ctl cloud machine stop MACHINE_ID --app vault42-server
$ 42ctl cloud machine start MACHINE_ID --app vault42-server
$ 42ctl cloud machine restart MACHINE_ID --app vault42-server
$ 42ctl cloud machine suspend MACHINE_ID --app vault42-server
$ 42ctl cloud machine wait MACHINE_ID --state started --app vault42-server
```

A stopped machine costs nothing but its volume, and starts again on the next request.

## 12.4 Volumes and snapshots

```sh
$ 42ctl cloud volume ls
$ 42ctl cloud volume inspect VOLUME_ID --app vault42-authority
$ 42ctl cloud volume snapshots VOLUME_ID --app vault42-authority
$ 42ctl cloud volume snapshot VOLUME_ID --app vault42-authority --dry-run
$ 42ctl cloud volume snapshot VOLUME_ID --app vault42-authority
```

A volume is the **only** copy of its application's database. The authority's volume also holds its
**contract signing key**: losing it invalidates every contract ever issued. Check a snapshot's age before
anything risky, and take one (*admin*) first.

## 12.5 Secrets, addresses, certificates

```sh
$ 42ctl cloud secret ls
$ 42ctl cloud net ips
$ 42ctl cloud net certs
```

`secret ls` shows each application secret's **name** and a digest of its value — never the value. A
changed digest is how to tell that a secret was rotated.

## 12.6 What is deliberately missing

There is no `cloud app destroy`, `machine destroy`, `volume destroy`, `snapshot delete` or `secret set`.
Destroying and re-creating infrastructure, and changing the deployment's own secrets, are done through
the vault42 repository's release pipeline or directly with flyctl, where they are reviewed and
recorded — see the operator's manual.
