# 42ctl — Verifying a download before you trust it

**`42ctl` decrypts your plaintext locally.** That makes its supply chain part of the *vault* threat
model: a tampered binary, through any channel, could exfiltrate keys or plaintext on your machine.
So every 42ctl release is signed (cosign keyless / sigstore), provenance-attested (SLSA), and
shipped with checksums and an SBOM — and **you should verify before you run it**. This guide shows
how, per channel. Verification is cheap; skipping it is the whole attack surface.

Trust anchors used below:

- **cosign keyless** — there is no published public key. Trust is the *signer identity*: the
  `Univers42/42ctl` release workflow, attested by the GitHub OIDC issuer
  `https://token.actions.githubusercontent.com`. You verify the certificate identity, not a key.
- **SLSA provenance** — a signed statement of *how* and *where* the artifact was built, queryable
  with `gh attestation verify` or `slsa-verifier`.
- **Checksums** — `SHA256SUMS` on the GitHub Release.

Throughout, substitute `vX.Y.Z` for the release you downloaded and
`<DOCKER_NS>` for the published Docker Hub namespace (printed on the release page).

---

## (a) cosign — verify the GitHub Release artifacts and the Docker image

Install cosign once: <https://docs.sigstore.dev/cosign/installation>.

The identity regex below matches the release workflow on a tag. Keyless verification fails closed
if the signer is anything other than that workflow run by that issuer.

### Release artifacts (blobs)

Each artifact ships with a detached signature (`*.sig`) and certificate (`*.pem`):

```sh
cosign verify-blob \
  --certificate-identity-regexp "^https://github.com/Univers42/42ctl/\.github/workflows/release\.yml@refs/tags/v.*$" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  --signature   42ctl-x86_64-unknown-linux-gnu.tar.gz.sig \
  --certificate 42ctl-x86_64-unknown-linux-gnu.tar.gz.pem \
  42ctl-x86_64-unknown-linux-gnu.tar.gz
# => Verified OK
```

### Docker image

```sh
cosign verify \
  --certificate-identity-regexp "^https://github.com/Univers42/42ctl/\.github/workflows/docker\.yml@refs/.*$" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  docker.io/<DOCKER_NS>/42ctl:vX.Y.Z
# => verified, prints the signature payload(s)
```

Pin by **digest** for production use (`docker.io/<DOCKER_NS>/42ctl@sha256:...`) so you verify and run
the exact bytes that were signed.

---

## (b) SLSA provenance — verify how it was built

Provenance proves the artifact came out of the 42ctl release pipeline (this repo, this workflow,
this commit) and was not hand-built or swapped.

### With the GitHub CLI (`gh attestation verify`)

```sh
gh attestation verify 42ctl-x86_64-unknown-linux-gnu.tar.gz --repo Univers42/42ctl
gh attestation verify oci://docker.io/<DOCKER_NS>/42ctl:vX.Y.Z --repo Univers42/42ctl
# => the artifact's provenance was verified against Univers42/42ctl
```

### With `slsa-verifier`

```sh
slsa-verifier verify-artifact 42ctl-x86_64-unknown-linux-gnu.tar.gz \
  --provenance-path 42ctl-x86_64-unknown-linux-gnu.tar.gz.intoto.jsonl \
  --source-uri github.com/Univers42/42ctl \
  --source-tag vX.Y.Z
# => PASSED: verified SLSA provenance
```

---

## (c) Checksums — verify the bytes

The release publishes `SHA256SUMS`. Download it alongside the artifacts and check:

```sh
sha256sum -c SHA256SUMS            # all listed files: OK
# or one artifact:
sha256sum -c 42ctl-x86_64-unknown-linux-gnu.tar.gz.sha256
```

Checksums catch corruption and naive tampering; cosign + provenance catch a *signed* impostor.
Do all three for anything you'll trust with plaintext.

---

## (d) npm provenance

The npm package `@universe42/42ctl` is published with `--provenance`, so npmjs.com shows a
**"Provenance"** panel on the package page linking the published tarball back to this repo, the
release workflow, and the commit. From the CLI:

```sh
npm view @universe42/42ctl     # confirm the version + repository
npm audit signatures           # verifies registry signatures + provenance for installed deps
```

`npm audit signatures` reports verified provenance/signatures for the installed package tree; a
missing or failed provenance attestation is a refusal signal.

---

## What the installers do for you

The official `curl | sh` / PowerShell installers and the `42ctl update` self-updater verify the
**signature + provenance + checksum** before placing or swapping the binary, and **refuse a tampered
artifact** — a failed verification changes nothing on disk (`42ctl update` is verify-before-swap).
Manual verification above is the same trust check you can run yourself; prefer the installers, but
never disable their verification.

---

## Reporting a vulnerability

Report security issues **privately** — do not open a public issue for an unfixed vulnerability.

- Preferred: open a GitHub **Security Advisory** via *Security → Report a vulnerability* on
  <https://github.com/Univers42/42ctl> (private disclosure).
- Email: **dev.pro.photo@gmail.com** with subject `42ctl security`.

Please include the version (`42ctl version`), the channel you installed from, and a reproduction.
We aim to acknowledge within 72 hours, ship a fix as a patched release, yank/deprecate affected
versions, and (where warranted) rotate publish credentials and publish a GHSA. See `RUNBOOK.md`
("Revoke / yank a compromised release") for the operator side.

## Threat-model note

`42ctl` performs **client-side decryption** of secrets and records (zero-knowledge: the server never
sees plaintext or your private key). The corollary is that the *binary on your machine* is a trusted
component of the vault: if it were tampered with, it could leak keys or plaintext locally regardless
of how strong the server-side crypto is. That is why the 42ctl supply chain is treated as part of
the vault threat model and why **every release is signed + provenance-attested and every installer
verifies before executing**. Verifying your download (a–d above) closes the last gap between the
audited build and the bytes you actually run.
