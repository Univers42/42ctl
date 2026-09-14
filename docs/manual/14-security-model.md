# 14. Security model

This chapter states what each party can learn and do. It is deliberately plain about limits: a manual
that overstates a guarantee is more dangerous than one that admits a gap.

## 14.1 The cryptography, in one paragraph

Every value is encrypted on your machine with a fresh random data key (**XChaCha20-Poly1305**). The data
key is wrapped for each recipient with an ephemeral **X25519** exchange and a derived one-time key. The
author signs the envelope's metadata and a hash of its ciphertext with **Ed25519**. Opening an envelope
checks the author's signature, the path it was sealed for and its revision **before** anything is
decrypted. The keystore on disk is sealed with a key derived from your passphrase by **Argon2id**. None
of this is implemented in 42ctl itself: it all comes from `vault42-core`, which composes audited
RustCrypto implementations.

## 14.2 What each party learns

| Party | Can learn | Cannot learn |
|---|---|---|
| **The vault server** (and anyone with its disk) | account-less principals (key fingerprints), opaque paths, ciphertext sizes, when writes happen, which principals hold a wrap for which environment | any secret, any file name or real path, any key that opens anything |
| **The authority** | email addresses, organisation structure, grants, public keys, tenant names, when you sign in | any secret, any private key, your passphrase |
| **The object store** | chunk sizes and keyed-digest names, when chunks are written | contents, which file a chunk belongs to |
| **A member with `read`** | every shared file and credential of that environment, as long as they hold its key | other members' private files, even their names; other environments |
| **A member with `write`** | the same, and may replace the environment's tree and credentials | other members' private files |
| **A former member, after rotation** | what they could read before they left | anything written after the rotation |
| **Anyone with your keystore but not your passphrase** | nothing | — |
| **Anyone with your keystore and passphrase** | everything sealed to you, and everything shared with you | — |

## 14.3 What 42ctl refuses, and why

| It refuses | Because |
|---|---|
| a restore path that is absolute, contains `..`, or would escape the project | a writer of an environment could otherwise write anywhere on a teammate's disk |
| writing through a symbolic link in a restored path | the same, by a planted link |
| a restored mode wider than the owner | a writer could otherwise make a private key world-readable with correct bytes |
| a "private" manifest or file not authored by you | anybody can seal to your public key, and would otherwise plant files you trust as yours |
| a chunk whose content does not match its keyed name | a member could otherwise poison deduplication for everyone storing the same bytes |
| an envelope older than the revision the manifest names | a server could otherwise roll a file back |
| a manifest from a newer 42ctl than itself | an older reader would misinterpret fields it does not know and restore the wrong bytes |
| the second of two overlapping pushes | a merged tree would contain files nobody pushed together |
| a `--filter` or `--only` that matches nothing | a typo must not look like "nothing to do" |

## 14.4 Removal and rotation

Removing somebody takes away their **authorization**: they are no longer re-provisioned, and the
authority refuses their administrative requests. It cannot take back a key already on their machine.
**Rotation** (`env keys rotate`) gives the environment a new key wrapped only to the people authorized
now, so nothing written afterwards is readable to the person who left. What they could read before, they
may have kept, and no system can make them forget it: change at its source every credential that
matters (chapter 10, *Changing a credential*).

## 14.5 What is on you

- **Your passphrase.** It cannot be reset. Lose it, and what is sealed only to you is lost.
- **Your machine.** 42ctl decrypts on it. Anything able to read your process's memory or your restored
  files can read your secrets.
- **Your restored files.** Once a secret is on disk it is protected by file permissions only. Restore
  into places you control, and delete what you no longer need.
- **The binary.** A tampered 42ctl can send your plaintext anywhere. Install only with the installer,
  `42ctl update`, or a release you have verified (chapter 2).

## 14.6 The supply chain

Every 42ctl release is built by GitHub Actions from a tagged commit, checksummed (`SHA256SUMS`), signed
keylessly with cosign by that workflow's identity, attested with SLSA build provenance, and accompanied
by an SBOM. The installer and `update` verify the checksum before anything is placed. Chapter 2 shows how
to verify the signature yourself.

## 14.7 Known limits

- **Metadata is visible.** The server sees how many secrets you have, their sizes, and when they change;
  the authority sees your organisation's structure. Only contents and names are hidden.
- **Deletion is not erasure.** Removing a secret deletes it from the vault; copies restored to disks and
  older versions shared before remain wherever they are.
- **A second factor protects sessions, not keys.** It stops a stolen password from signing in; it does
  nothing for a stolen keystore and passphrase.
- **No organisation-level secret policy exists**: 42ctl cannot force a member's restored files to be
  deleted, or stop a reader from copying what they can read.
