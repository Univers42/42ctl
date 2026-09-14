# Appendix C. Glossary

**account** — an email address and password on the authority. Organisations, teams and grants refer to
accounts. Distinct from an identity.

**address** — the `v42:…` encoding of an identity's two public keys; what `vault share --to` takes.

**authority** — `vault42-authority`, the service holding accounts, organisations, permissions and public
keys, and issuing contracts. It never holds a secret.

**chunk** — a sealed piece of a file larger than about 4 MiB, stored in the object store under a keyed
digest of its contents.

**contract** — a token signed by the authority binding an identity's key to a tenant. The vault server
requires it on every request.

**envelope** — what the server stores: ciphertext, the data key wrapped for each recipient, and the
author's signature.

**environment** — a named stage of a project (`dev`, `prod`), with its own scope key.

**epoch** — the generation of an environment's scope key; rotation increments it.

**grant** — a user, team or group, given a role (`admin`, `write`, `read`) on a project, for every
environment or one.

**group** — a set of an organisation's members scoped to one project.

**identity** — the Ed25519 and X25519 key pair in your keystore. It seals, opens and signs.

**keystore** — the file holding the identity, sealed with the passphrase.

**manifest** — the sealed list, for a pushed tree, of each file's relative path, mode, size, revision and
opaque storage id.

**marker** — `.42ctl/project.json`, which makes a directory a project.

**object store** — an S3-compatible service holding chunks.

**organisation** — the unit of membership, with owners, admins and members.

**passphrase** — the secret that unlocks the keystore; never sent anywhere, never recoverable.

**principal** — the fingerprint of an identity's signing key: the vault's name for you.

**private file** — a file in a shared environment's tree sealed to the person who pushed it, and invisible
to everyone else.

**profile** — a named set of endpoints with its own session and contract.

**project** — within an organisation, the parent of environments, groups and grants; locally, a directory
with a marker.

**revision** — the version number of a stored value; every write creates the next.

**rotation** — replacing an environment's scope key with a new epoch wrapped only to current members, and
moving its content to it.

**scope key** — an environment's key pair; shared content is sealed to its public half.

**session** — a bearer token for an account, obtained by signing in.

**team** — a set of an organisation's members across projects.

**tenant** — a name claimed by an account and bound to an identity by a contract.

**vault server** — `vault42-server`, the gRPC service storing envelopes.

**wrap** — an environment's private scope key sealed to one member's public key, recording their role.
