# Canonical-v2 private closure contract v1

Status: prospective format/profile/error authority for the canonical-v2 closure candidate. It does not authorize automatic migration or production integration.

## Identity and occurrence

- Canonical chunk bytes are the complete canonical `LFSO/Bytes` object.
- A chunk identity is `BLAKE3(canonical chunk bytes)`; a raw-byte digest is never a v2 chunk identity.
- An ordered occurrence is `u32be(raw_length) || canonical ObjectId[32]`.
- The ordered occurrence commitment is BLAKE3 derive-key with context `layerfs/canonical-v2/ordered-occurrence/v1` over every occurrence in order.
- File leaves contain at most 64 occurrences. File branches contain at most 64 children. Directory pages retain the selected 262,144-byte ceiling and mapping-profile field ceiling 8,388,608.
- Mapping profile ID is `94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b`.

## Native mapping envelope

Every versioned mapping value is framed directly as:

`LFSO || Bytes || LFS4MAP\0 || u16be(2) || tag || body`

Tags are: file root `01`, file leaf `02`, directory index `03`, directory metadata `04`, transition index `05`, delta page `06`, and file branch `07`. A v2 encoder or decoder must never create v1 bytes and rewrite a version byte.

File occurrence bytes are exactly `u32be(raw_length) || ObjectId[32]`. Branch/root bodies retain the version-independent child descriptor `u64be(cumulative_end) || ObjectId[32]`. Directory pages and the `m`/`t` wrapper remain ordinary canonical Directory objects. Transition and delta bodies retain their version-independent semantic grammar but use the native v2 envelope.

## Shared authority path

All v2 create, edit, scrub, reconstruction, range, reopen, and publication paths use the same canonical Bytes identity, native v2 mappings, profile-bound receipt, and SQLite CAS authority. A non-clone, non-copy `AdmittedOccurrence` is issued only after successful canonical admission, binds the canonical ID, raw/canonical lengths, Bytes role, store instance, validation authority, integrity epoch, profile, open, transaction, authority serial, and mutation serial, and is consumed once before its exact reference enters a mapping.

V2 rejoin equality is exactly `(raw_length, canonical ObjectId)`. Returned reconstruction/range bytes are unavailable until the complete canonical object authenticates, decodes as Bytes, and matches the occurrence length.

## Exact error precedence

1. Expected ObjectId mismatch: `IdentityMismatch` before grammar.
2. Canonical object/object-field limit: `ObjectLimitExceeded`.
3. Short outer header, declared object, mapping header, or body: `UnexpectedEof`.
4. Bad `LFSO`: `Unsupported`; invalid outer kind: `InvalidObjectKind`; valid non-Bytes outer role: `WrongLogicalRole`.
5. Outer surplus or inconsistent completed length: `TrailingBytes`.
6. Bad `LFS4MAP\0`: `InvalidMappingTag { tag: 0 }`.
7. Wrong version: `UnsupportedMappingVersion { version }`.
8. Wrong known tag: `WrongLogicalRole`; unknown tag: `InvalidMappingTag { tag }`.
9. Complete leaf raw length above 32,768: `ObjectLimitExceeded`; empty/over-64/nonfinal-partial leaf: `NonCanonicalPagePartition`; authenticated occurrence length mismatch: `ChunkLengthMismatch`.
10. Descending child ends: `NonCanonicalOrdering`; invalid node partition/level: `NonCanonicalPagePartition`; root/descendant summary mismatch: `LengthMismatch`.
11. Directory descriptor zero partition: `NonCanonicalPagePartition`; descriptor aggregate mismatch: `LengthMismatch`; duplicate actual entry: `NameCollision`.
12. Invalid transition parent discriminator: `InvalidMappingDiscriminator`; transition count limit: `ObjectLimitExceeded`; zero-presence partition mismatch: `NonCanonicalPagePartition`; genesis with operations: `DeltaConflict`.
13. Delta count zero: `NonCanonicalPagePartition`; delta count above 100,000: `ObjectLimitExceeded`; invalid path uses the canonical-path error; invalid operation uses `InvalidMappingDiscriminator`.
14. A v2 path does not emit `ChunkIdentityMismatch`; that name is reserved for the immutable legacy-v1 reader.

## Store/profile policy and precedence

- Empty store to v2 genesis is supported.
- V2 parent to v2 child is supported.
- Known nonempty v1 store to v2 returns `SchemaMigrationRequired` before RW open, PRAGMA, DDL, journal, or authority-sidecar mutation.
- V2 store to v1 returns `ProfileMismatch` before mutation.
- Unknown profile returns `ProfileMismatch`; malformed authority metadata returns its exact schema/record/authority error.
- Automatic migration and retained-history rewrite are unsupported.
- Validation-authority secrets and store-instance identities use OS cryptographic randomness. A new authority sidecar is created atomically with mode `0600`; an existing symlink, non-regular file, wrong size, or wrong mode is `ValidationAuthorityUnavailable`.

## Publication and durability

Before staging a v2 receipt/head, publication authenticates and native-v2-decodes the transition, requires its parent/child to match the requested publication, authenticates the direct namespace, and requires native-v2 file or directory roots. Receipt fields bind store instance, validation authority, integrity epoch, generation, profile, root, and transition.

One synchronous caller-thread writer transaction and one publication COMMIT are required. SQLite remains `FULL + DELETE`; ambiguous outcomes use fresh read-only reconciliation. Q is bounded and terminal zero. No worker, registry, migration framework, profile promotion, or production integration is part of this contract.
