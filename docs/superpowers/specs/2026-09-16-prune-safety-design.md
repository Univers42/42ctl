# Prune safety, store-error accuracy, and config read-back

Date: 2026-09-16
Status: implemented; risk verdict PROCEED-WITH-CONDITIONS, all conditions met
Scope: `42ctl` client only. `vault42` (server/authority) needs no change.

## Context

`42ctl push --prune` mirrors the working tree into the manifest: entries whose
file is "no longer scanned" are dropped. The scan deliberately declines to
descend some directories, so "not scanned" and "not present" are different
statements about the tree. Prune currently treats them as the same statement.

This was found in production, not in review. Pushing from a plain restored
directory scanned 33 of 39 files, declined all of `vendor/`, and reported
success. With `--prune` that silently removes six entries from the vault,
including secrets, and exits 0.

## Problem 1 — prune deletes entries the scan merely failed to see

`src/ops/sync.rs:67-73`:

```rust
let pruned = if prune {
    let before = manifest.entries.len();
    manifest.entries.retain(|e| scanned.contains(&e.relative_path));
    before - manifest.entries.len()
} else { 0 };
```

`scanned` is a lower bound on the tree, not a census of it. `walk`
(`src/core/project.rs:265`) skips a directory on the skip list unless it is a
git repository root (`walk_repositories_inside`, `src/core/project.rs:207`), so
a vendored tree that is not a checkout contributes nothing to `scanned` while
its manifest entries remain eligible for deletion.

The existing warning does not cover this:

- `report_declined` (`src/ops/sync.rs:437`) runs *after* the prune has already
  been computed.
- It fires only when `holds_a_candidate` (`src/core/project.rs:306`) returns
  true, which bails out when a directory has more than `DECLINE_PROBE_LIMIT`
  (64) children (`src/core/project.rs:299`) and probes only two levels deep.

So the loudest case — a large declined directory — is precisely the silent one.

## Problem 2 — a missing credential is reported as a missing object store

`BlobStore::from_profile` (`src/adapters/blobstore.rs:86`) returns `Option` and
reaches the credential through `from_env("FT_S3_KEY")?`. A configured profile
with no credential in the environment yields the same `None` as a profile with
no object store at all, so `src/ops/sync.rs:292` reports:

    <path> is stored as N chunk(s) and this profile names no object store

Observed against a profile whose `config.json` demonstrably names endpoint,
bucket and region. The message sends the operator to re-run `config endpoint`,
which is the one action that cannot help.

## Problem 3 — `config endpoint` with no flags writes and claims an update

`set_endpoint` (`src/cmd/config.rs:76`) assigns only the flags supplied, then
saves unconditionally and prints `updated endpoints for '<profile>'`. Invoked
with no flags it rewrites the file with identical content and reports an update
that did not happen. There is no read-only view of the current endpoints.

## Approach

**Chosen: prune on filesystem absence, not scan absence.**

Before dropping entry `E`, stat `root.join(E.relative_path)`. If a regular file
is present there, the scan declined or skipped it — keep the entry. If nothing
is present, the file is genuinely gone — prune it.

The failure direction inverts: the worst case becomes a retained stale entry
rather than a destroyed live secret. That is the correct bias for a secrets
tool.

**Scope of the claim.** This fixes `push --prune` at `src/ops/sync.rs:67-75`.
It does NOT fix the other two sites of the same class, which are tracked
separately and must not be implied fixed by this PR's release text:

- `env push` rebuilds the shared manifest from the same lower-bound scan
  (`src/cmd/scope_tree.rs:83` discards `.declined`; `:142` builds a fresh
  `Manifest::new` and `:157-159` overwrites `TREE_MANIFEST`). This deletes
  entries from a whole team's tree with no flag and no warning — a strictly
  larger blast radius than the bug fixed here.
- `scope_private::push` does the same rebuild at `src/cmd/scope_private.rs:137`.

Tracked as [#12](https://github.com/Univers42/42ctl/issues/12). The withdrawal
path this guard removes is tracked as
[#13](https://github.com/Univers42/42ctl/issues/13).

**Notes are a separate deletion bug at the same line.** `Kind::Note` entries
share this manifest (`src/ops/notes.rs:56-59`) with a `relative_path` the scan
can never produce. `pull` and `ensure_revisions_recorded` filter on `kind`
(`src/ops/sync.rs:230`, `:408`); the prune does not. So `push --prune` deletes
every note today, and the existence predicate alone would still delete them,
because a note has no file on disk. A `kind` filter is required.

**Deliberate withdrawal becomes harder, and that is a real cost.** Under this
predicate an entry whose file is present but no longer scanned — a narrowed
pattern, a `*.bak`/`*.stale` excluded by `skip_file`
(`src/core/project.rs:256-258, 286`) — can never be pruned. `Manifest::remove`
(`src/core/manifest.rs:126-131`) has exactly one caller, note removal
(`src/ops/notes.rs:160`); there is no `rm` verb for project files. So "take this
secret back out of the vault" becomes "delete the local file first". This is
documented in help text and tracked, not left as an emergent property.

**Rejected: refuse to prune whenever anything was declined.** Does not fix the
class. `scan.declined` is empty in exactly the >64-children case that is most
dangerous, and it would block every legitimate prune in a repository that
permanently carries a vendored directory.

**Rejected: interactive confirmation of the drop list.** Asks a human to
validate a list the tool computed incorrectly, and cannot run in CI.

## Non-goals

- Adding `*.pem` / `*.key` / `*.crt` to the default scan patterns. The defaults
  are deliberately narrow; widening them sweeps vendored `certifi/cacert.pem`
  bundles into the vault. Certificate coverage is per-project configuration in
  `.42ctl/project.json`, which `project::open` already honours
  (`src/core/project.rs:52`).
- Reading S3 credentials from the vault. Deferred to PR 2; it requires plumbing
  a session into a constructor that is currently synchronous and session-free.

## Design

### Prune guard

`cmd_push` computes the prune with a predicate that consults the filesystem:
an entry is dropped only when it was not scanned **and** no regular file exists
at its path under the project root. Entries kept for the second reason are
counted and reported on a non-fatal line, so the operator learns that the tree
is incomplete on the run that would otherwise have destroyed data.

### Store availability

`from_profile` distinguishes the two gaps it can hit and the caller renders the
accurate message: a profile with no location keeps today's wording; a profile
with a location but no credential names the bucket and the two environment
variables to set. Error text carries variable *names* only, never any value.

### Config read-back

`config endpoint` with no flags prints the profile's current endpoints and does
not save. With any flag it behaves exactly as today.

## Testing

Unit tests beside the code, in the existing table-driven idiom, under
`cargo test --locked`:

- a `Kind::Note` entry survives a prune (it has no file and never had one)
- an entry whose file is gone is pruned — the guard does not disable prune
- an unscanned entry whose file is present is kept **and named**
- a symlink at the entry path is not presence (the scan skips symlinks)
- a directory at the entry path is not presence
- a stored path that escapes the root is kept and never stat'd
- a missing credential and a missing location render differently, and neither
  refusal carries a credential value

End-to-end in `qa/specs/s17-prune-declined.sh`: store a file under `vendor/`
while it is a repository, remove the marker so the directory becomes declined,
prune, and assert the entry survived — then delete a file for real and assert
it is still pruned.

Gate: `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets --
-D warnings`, `cargo test --locked`. Green at 216 passed, 0 failed.

## Staging

- **PR 1** (this spec): prune guard, store-error split, config read-back.
- **PR 2**: S3 credentials resolved from the vault so a fresh machine needs only
  the account.
- **Not a 42ctl change**: certificate patterns in the consuming project's
  `.42ctl/project.json`.
