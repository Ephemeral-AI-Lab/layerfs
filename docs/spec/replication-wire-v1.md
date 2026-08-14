# Replication wire format version 1

| Field    | Value                     |
| -------- | ------------------------- |
| Status   | Normative                 |
| Protocol | `efs-replication-v1`      |
| Codec    | `EFS_REPLICATION_V1_WIRE` |

This document freezes the canonical byte encoding used by `@ephemeralai/fs-replication`
version 1. It is normative together with [`replication.md`](./replication.md). The words
MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY have the meanings stated in
[`SPEC.md`](../../SPEC.md).

An implementation MUST NOT persist a version 1 receipt, cursor digest, authorization
digest, capability digest, or terminal result using another encoding. A
wire-incompatible change requires a new protocol version and new golden vectors.

## 1. Primitive encoding

All multibyte integers use unsigned big-endian encoding. The codec uses these
primitives:

| Name       | Encoding                                                                   |
| ---------- | -------------------------------------------------------------------------- |
| `uint8`    | One unsigned byte                                                          |
| `uint16`   | Two unsigned big-endian bytes                                              |
| `uint32`   | Four unsigned big-endian bytes                                             |
| `uint64`   | Eight unsigned big-endian bytes                                            |
| `boolean`  | Exactly `0x00` for false or `0x01` for true                                |
| `digest32` | Exactly 32 raw SHA-256 bytes                                               |
| `bytes`    | `uint32` byte length followed by exactly that many bytes                   |
| `text`     | `uint32` byte length followed by exactly that many well-formed UTF-8 bytes |
| `optional` | `0x00`, or `0x01` followed by the encoded present value                    |
| `array`    | `uint32` element count followed by the elements in declaration order       |

Decoded `uint64` values MUST be JavaScript safe integers. A larger value is
`ProtocolMismatch`. Unless a field below gives another bound, text contains between one
and 256 UTF-8 bytes, an array contains at most 64 entries, and a byte value is bounded
by the enclosing negotiated envelope.

Text is encoded byte for byte. It is not NFC-, NFD-, case-, path-, or locale-normalized.
An encoder MUST reject an unpaired UTF-16 surrogate instead of substituting U+FFFD. A
decoder MUST use fatal UTF-8 validation. Empty text is not canonical for any version 1
field. A length prefix distinguishes absent, empty bytes, and present text. Branch and
operation identifiers are further limited to 200 UTF-8 bytes.

A session identifier is exactly 128 random bits encoded as 32 lowercase ASCII hex
digits. The package generates and validates it; a caller-supplied label is not a session
identifier.

Unknown enum tags, unknown optional tags, noncanonical booleans, unsafe integers,
declared-length mismatches, truncated values, and trailing bytes are `ProtocolMismatch`.
Version 1 has no ignored fields, extension map, padding, or trailer.

## 2. Envelope

Every value is carried in one envelope:

| Offset | Size | Field             | Required value                           |
| -----: | ---: | ----------------- | ---------------------------------------- |
|      0 |    4 | magic             | ASCII `EFSR`                             |
|      4 |    2 | wire version      | `1`                                      |
|      6 |    1 | envelope tag      | One tag from the table below             |
|      7 |    1 | flags             | `0`                                      |
|      8 |    4 | payload byte size | Exact number of following bytes          |
|     12 |    N | payload           | The tagged value, with no trailing bytes |

Envelope tags are:

| Tag    | Payload                    |
| ------ | -------------------------- |
| `0x01` | capabilities               |
| `0x02` | authorization              |
| `0x03` | batch                      |
| `0x04` | cursor binding             |
| `0x05` | revision fragment          |
| `0x06` | checkpoint fragment        |
| `0x07` | branch-generation fragment |
| `0x08` | terminal result            |
| `0x09` | semantic error             |
| `0x0a` | batch acknowledgement      |

The receiver MUST apply the pre-negotiation 64 KiB limit or the negotiated request or
response limit before decoding the envelope. The payload length MUST equal the remaining
input exactly. An unknown envelope tag, nonzero flag, different magic, different wire
version, or trailing byte is rejected.

Decoded byte values are borrowed views into the caller-supplied envelope. The caller
MUST transfer immutable ownership of that envelope for the lifetime of the decoded value
and MUST release it before constructing a response larger than the mutating
acknowledgement bound. A conforming decoder MUST NOT copy every byte field into a second
complete envelope representation.

## 3. Global plans and phases

A plan begins with one tag and, for a branch flow, one `text(branchId)`:

| Tag    | Flow                          | Following value  |
| ------ | ----------------------------- | ---------------- |
| `0x01` | `authority-main-to-replica`   | none             |
| `0x02` | `authority-branch-to-replica` | `text(branchId)` |
| `0x03` | `replica-branch-to-authority` | `text(branchId)` |
| `0x04` | `replica-branch-to-replica`   | `text(branchId)` |

The one-byte phase tags are:

| Tag    | Phase                    |
| ------ | ------------------------ |
| `0x01` | `handshake`              |
| `0x02` | `plan-selection`         |
| `0x03` | `content-offer`          |
| `0x04` | `missing-content`        |
| `0x05` | `content-transfer`       |
| `0x06` | `state-transfer`         |
| `0x07` | `activation`             |
| `0x08` | `result-acknowledgement` |
| `0x09` | `cleanup`                |

## 4. Capabilities

The capability payload contains these fields in this exact order:

1. `array(text(protocolVersion))`.
2. `uint8 hostProfile`, exactly `0x01` for `computer-efs-carrier-v1`.
3. `uint8 provisioningState`: `0x00` bound or `0x01` unbound replica.
4. `optional(text(filesystemId))`.
5. `optional(text(authorityId))`.
6. `optional(uint32 applicationId)`.
7. `optional(uint32 filesystemSchemaVersion)`.
8. `uint32 storageUserVersion`.
9. `uint8 storageMigrationState`, exactly `0x00` for `none`.
10. `array(uint32 readableFilesystemSchemaVersion)`.
11. `uint32 writableFilesystemSchemaVersion`.
12. `uint8 role`: `0x01` main authority or `0x02` replica.
13. `uint32 hashAlgorithmCount`, exactly `1`, followed by `0x01` for SHA-256.
14. `optional(text(activeManifestFormat))`.
15. `array(text(supportedManifestFormat))`.
16. `optional(text(activeChunkerFormat))`.
17. `array(text(supportedChunkerFormat))`.
18. `optional(fastCdcConfiguration)`.
19. `array(fastCdcConfiguration)` for supported configurations.
20. `optional(uint32 copyOnWritePageBytes)`.
21. `array(uint32 supportedCopyOnWritePageBytes)`.
22. The ten feature booleans below.
23. The 21 replication-limit `uint64` values below.
24. The ten storage-capability `uint64` values below.

A FastCDC configuration is `uint32 minimum`, `uint32 average`, and `uint32 maximum`. The
decoder rejects zero minimum, `minimum > average`, `average > maximum`, or a target
average that is not a power of two. Every COW page value is exactly 4,096, 8,192, or
16,384.

Features are ten canonical booleans in this exact order:

1. `authorityMainToReplica`;
2. `authorityBranchToReplica`;
3. `replicaBranchToAuthority`;
4. `replicaBranchToReplica`;
5. `checkpointBootstrap`;
6. `segmentedMerkleManifestTransfer`;
7. `durableStagingLeases`;
8. `physicalRestartRecovery`;
9. `terminalResultReplication`; and
10. `freshReplicaProvisioning`.

Replication limits are `uint64` values in this exact order:

1. `maxBatchEntries`;
2. `maxBatchBytes`;
3. `maxRequestBytes`;
4. `maxResponseBytes`;
5. `maxBufferedBytes`;
6. `maxInFlightBatches`;
7. `maxConcurrentSessions`;
8. `maxStagingBytesPerSession`;
9. `maxReplicationSessionRows`;
10. `maxReplicationMetadataBytes`;
11. `maxReceiptsPerSession`;
12. `maxReceiptBytesPerSession`;
13. `maxCursorBytes`;
14. `maxTerminalResultBytes`;
15. `maxCursorAgeMs`;
16. `stagingLeaseMs`;
17. `resultRetentionMs`;
18. `maxRetryAttempts`;
19. `maxRetryElapsedMs`;
20. `minRetryDelayMs`; and
21. `maxRetryDelayMs`.

Storage capabilities are `uint64` values in this exact order:

1. `maxBlobBytes`;
2. `maxManifestNodeBytes`;
3. `maxManifestDepth`;
4. `maxManagedPayloadBytes`;
5. `maxStagingPayloadBytes`;
6. `maxMaintenanceBytes`;
7. `maintenanceReserveBytes`;
8. `maxPermanentIdentifiers`;
9. `maxFinalTransactionRows`; and
10. `maxFinalTransactionBytes`.

The capability digest additionally binds all 21 effective replication-limit `uint64`
values in declaration order after the capability payload. It is:

```text
SHA-256(ASCII("efs-replication-v1/capabilities\0") || capabilityPayload || effectiveLimits)
```

The limits inside `capabilityPayload` are the peer's advertisement. The appended limits
are the negotiated effective values, so both the offer and the result are durable
session bindings.

## 5. Authorization

The authorization payload contains these fields in exact order:

1. `text(principalId)`;
2. `text(hostScopeId)`;
3. `text(expectedFilesystemId)`;
4. `text(expectedAuthorityId)`;
5. `text(policyVersion)`;
6. `uint8 hostProfile`, exactly `0x01`;
7. authorization ceiling values in the replication-limit order above, omitting only
   `minRetryDelayMs`;
8. `uint64 minRetryDelayMsFloor`;
9. `array(plan)` of allowed global plans; and
10. all 21 effective replication-limit `uint64` values in declaration order.

Allowed plans MUST be sorted lexicographically by their complete encoded plan bytes.
Duplicates are rejected. This makes caller array order irrelevant while preserving raw
UTF-8 branch-identifier bytes.

The package, not the caller, computes:

```text
SHA-256(ASCII("efs-replication-v1/authorization\0") || authorizationPayload)
```

Changing the principal, host scope, filesystem, authority, policy, profile, allowed
plan, authorization policy, or effective limit therefore changes the digest.

## 6. Batch

The batch payload contains these fields in exact order:

1. `text(sessionId)`;
2. `plan`;
3. `uint8 phase`;
4. `uint64 sequence`;
5. `digest32 priorCursorDigest`;
6. `uint32 entryCount`;
7. `uint64 payloadByteCount`;
8. `digest32 payloadDigest`; and
9. the record sequence.

The record sequence is `uint32 recordCount` followed by each record as `uint8 tag`,
`uint32 recordPayloadLength`, and that exact record payload. `entryCount` MUST equal
`recordCount`, which is at most 256. `payloadByteCount` is the sum of record payload
lengths. It excludes the record count, record tags, and record-length prefixes.

The batch payload digest is:

```text
SHA-256(
  ASCII("efs-replication-v1/batch-payload\0") ||
  uint32(recordCount) ||
  recordTag1 || uint32(recordPayloadLength1) || recordPayload1 ||
  ...
)
```

The encoder and decoder calculate this digest incrementally. Encoding a digest MUST NOT
materialize a second complete record sequence.

Record tags and payloads are:

| Tag    | Kind                       | Payload fields in order                                                                                                 |
| ------ | -------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `0x01` | object descriptor          | `digest32`, `uint64 byteLength`                                                                                         |
| `0x02` | object payload             | `digest32`, `uint64 byteLength`, `bytes`                                                                                |
| `0x03` | manifest-root descriptor   | `text format`, `digest32`, `uint64 encodedLength`, `uint64 logicalFileLength`, `uint64 entryCount`, `digest32 rootNode` |
| `0x04` | manifest-node descriptor   | `digest32`, `uint8 nodeKind`, `uint64 encodedLength`, `uint64 logicalSpan`, `uint64 entryCount`                         |
| `0x05` | missing content            | `uint8 contentKind`, `digest32`                                                                                         |
| `0x06` | revision fragment          | The revision-fragment payload in section 8                                                                              |
| `0x07` | checkpoint fragment        | The checkpoint-fragment payload in section 8                                                                            |
| `0x08` | branch-generation fragment | The branch-generation-fragment payload in section 8                                                                     |
| `0x09` | terminal result            | The terminal-result payload in section 9                                                                                |

Manifest node kind is `0x01` leaf or `0x02` internal. Missing-content kind is `0x01`
object, `0x02` manifest root, or `0x03` manifest node. Object payload declared length
MUST equal its byte length, and SHA-256 of its bytes MUST equal its digest.

## 7. Cursor binding

The cursor-binding payload contains:

1. `text(sessionId)`;
2. `digest32 ownerNonceDigest`;
3. `text(sourceFilesystemId)`;
4. `text(destinationFilesystemId)`;
5. `plan`;
6. `text(selectedIdentity)`;
7. `optional(uint64 selectedGeneration)`;
8. `uint8 phase`;
9. `uint64 nextSequence`; and
10. `digest32 capabilityDigest`.

`ownerNonceDigest` is
`SHA-256(ASCII("efs-replication-v1/owner-nonce\0") || ownerNonce16)`. The raw owner
nonce MUST NOT appear in a carrier envelope.

Its digest is:

```text
SHA-256(ASCII("efs-replication-v1/cursor-binding\0") || cursorBindingPayload)
```

The cursor binding is durable internal state used to authenticate an opaque random
public lookup token. It is not itself a credential or a public inventory cursor.

## 8. Batch acknowledgement and durable replay

A batch-acknowledgement payload contains:

1. `text(sessionId)`;
2. `uint64 sequence`;
3. `uint8 phase`;
4. `digest32 batchEnvelopeDigest`;
5. `uint8 nextPhase`;
6. `bytes(cursor)`, between 16 and 256 bytes;
7. `digest32 cursorDigest`;
8. `digest32 chainDigest`;
9. `uint64 acceptedEntries`;
10. `uint64 acceptedBytes`; and
11. `uint64 stagedBytes`.

`nextPhase` MUST equal `phase` or its immediate successor in the phase table.
`cursorDigest` MUST equal SHA-256 of `cursor`. The cursor is an opaque,
collision-resistant random lookup token; it MUST NOT encode the cursor binding or an
inventory.

The full batch-envelope digest is calculated incrementally as:

```text
SHA-256(
  ASCII("efs-replication-v1/batch-envelope\0") ||
  completeCanonicalBatchEnvelope
)
```

This digest binds the envelope magic, version, tag, flags, declared payload length,
session, exact plan, phase, sequence, prior cursor, counts, payload digest, and every
record byte. A receipt row is keyed by its durable session and sequence, stores this
digest, and stores the exact canonical batch-acknowledgement envelope. The destination
MUST commit the receipt, cursor, counters, and filesystem effects in one transaction
before returning those acknowledgement bytes.

On replay, the destination recomputes the full batch-envelope digest incrementally. An
equal digest returns the stored acknowledgement bytes exactly without rerunning effects.
A different digest is `BatchReplayMismatch`. Storing only the record-sequence payload
digest is insufficient. A receipt MUST NOT store JSON or another ad hoc encoding in the
acknowledgement column.

The initial receipt-chain digest is 32 zero bytes. After accepting a batch it becomes:

```text
SHA-256(
  ASCII("efs-replication-v1/receipt-chain\0") ||
  priorChainDigest ||
  uint64(sequence) ||
  fullBatchEnvelopeDigest
)
```

The receipt chain therefore remains an exact bounded summary after safe receipt
compaction; it MUST NOT use only the record-sequence payload digest.

## 9. Revision, checkpoint, and branch fragments

A revision fragment contains:

1. `text(revisionId)`;
2. `optional(text(parentRevisionId))`;
3. `uint32 fragmentIndex`;
4. `uint32 fragmentCount`; and
5. `bytes(fragmentBytes)`.

A checkpoint fragment contains:

1. `text(checkpointId)`;
2. `text(revisionId)`;
3. `uint32 fragmentIndex`;
4. `uint32 fragmentCount`; and
5. `bytes(fragmentBytes)`.

A branch-generation fragment contains:

1. `text(branchId)`;
2. `text(baseRevision)`;
3. `uint64 generation`;
4. `digest32 generationDigest`;
5. `uint32 fragmentIndex`;
6. `uint32 fragmentCount`; and
7. `bytes(fragmentBytes)`.

For every fragment, `fragmentCount` is positive and `fragmentIndex < fragmentCount`.
`fragmentBytes` is a bounded semantic fragment produced and accepted through the typed
core replication bridge. It is not SQL, a table row API, a raw manifest insertion API,
or a standalone COW mutation API. Its phase-specific semantic schema MUST be frozen
before a transfer implementation persists receipts for that phase.

## 10. Terminal results

A terminal-result payload contains:

1. `text(operationId)` with the 200-byte operation-identifier bound;
2. `optional(text(branchId))` with the 200-byte branch-identifier bound;
3. `optional(uint64 generation)`;
4. `optional(digest32 generationDigest)`;
5. `digest32 resultDigest`; and
6. `bytes(resultBytes)`, at most 1 MiB.

Generation and generation digest MUST be absent together or present together. SHA-256 of
`resultBytes` MUST equal `resultDigest`.

## 11. Semantic errors

A semantic-error payload contains:

1. `uint8 errorCode`;
2. `optional(uint8 phase)`;
3. `optional(text(sessionId))`;
4. `text(message)` with a 4 KiB UTF-8 bound; and
5. `boolean retryable`.

Error tags are assigned in this exact order:

| Tag    | Code                     | Tag    | Code                  |
| ------ | ------------------------ | ------ | --------------------- |
| `0x01` | `ProtocolMismatch`       | `0x0d` | `BranchDiverged`      |
| `0x02` | `FilesystemMismatch`     | `0x0e` | `CursorMismatch`      |
| `0x03` | `AuthorityMismatch`      | `0x0f` | `CursorExpired`       |
| `0x04` | `SchemaMismatch`         | `0x10` | `BatchReplayMismatch` |
| `0x05` | `CapabilityMismatch`     | `0x11` | `StagingExpired`      |
| `0x06` | `IncompatibleLimit`      | `0x12` | `IntegrityFailure`    |
| `0x07` | `UnauthorizedScope`      | `0x13` | `ResourceLimit`       |
| `0x08` | `ProvisioningRejected`   | `0x14` | `Busy`                |
| `0x09` | `OperationMismatch`      | `0x15` | `TransportFailure`    |
| `0x0a` | `MainDiverged`           | `0x16` | `RetryExhausted`      |
| `0x0b` | `BaseRevisionMissing`    | `0x17` | `Aborted`             |
| `0x0c` | `BranchIdentityMismatch` | `0x18` | `Closed`              |

The high-level driver reconstructs `ReplicationError` from this value. It MUST NOT rely
on an RPC carrier preserving a thrown JavaScript error object. `retryable` MUST be
`true` only for `Busy` and `TransportFailure`, and MUST be `false` for every other error
code. An encoder or decoder MUST reject a record whose flag disagrees with this fixed
policy; retry eligibility is then further bounded by the negotiated durable retry
policy.

## 12. Golden vectors

The checked-in fixture uses the exact values in
`tests/replication/protocol-fixtures.mjs`. SHA-256 of each complete envelope is:

| Envelope                   | SHA-256                                                            |
| -------------------------- | ------------------------------------------------------------------ |
| capabilities               | `e9920dd70e5f3f2bbc7654e15728ff01cccdec00e174a19792dbe8931147edc5` |
| authorization              | `bb4c8a84bc18d4a47f6c591b3a231b85c90b1591dd8a7ee6f12a46e18dd5dd08` |
| batch                      | `bbedb4e7c274d1fba9d608253e5fb6ad88a14516140e2906b0fcb858b78305c3` |
| batch acknowledgement      | `84092a2308dd3c74ab6d70c15ae42c330ebdad4468fb2fe4c86700b3a9911708` |
| cursor                     | `949991cb1e965e6cf5b185c2ad221f3e64f5b80dda3db3659fbee01b1684bb5d` |
| revision fragment          | `de66dd9a0b1e790c23b19e6561fd5c80cf3fe7350ac89a3d70d54ac5fa5afd5b` |
| checkpoint fragment        | `abca64bd9b379af8e2ba9565108745f464ea0082e5ed518c22b60e3d01f71c97` |
| branch-generation fragment | `8fc7c0d226e21a066655416850ad5a7fa5d083f20f2351cbebf6592e1f73c994` |
| terminal result            | `c67257e11d93c8ba04e2ba85adfda5d2218db6ec83f9792ee85463a7fa9f00fd` |
| semantic error             | `76f49d891c3b99a3058b4d0cda5f17a85f5de934f04606d7afa173a790ade7fb` |

The derived digest vectors are:

| Digest                | SHA-256                                                            |
| --------------------- | ------------------------------------------------------------------ |
| capability digest     | `3eaeb8228e026edad086e7bbad10e33245530c2796bd2307cfc8d9fb93e3772a` |
| authorization digest  | `d8cd3907231f41557774ec354d4ffc26ec7f18b0085bd5ace68063211878f48f` |
| batch payload digest  | `dcf0bdbc12445c02e39799deb7326af9eec2128c5c3850660be3a562d5d3d257` |
| batch-envelope digest | `cb4d2914e8dbd2edbbffbc35c00e14e01c62c91c5e552ca01a254abb4e3318b1` |
| receipt-chain digest  | `9f01ca484c9e6b850d3fd8be2dde83926d9b08b4cee475aa0a7913cd2ef889ea` |
| cursor-binding digest | `faeeb127c6ae299d38aa2cc79be0fecc8a54c95bf647baba3aeafd5e5460b16e` |

The conformance suite MUST match every vector, re-encode every decoded value
identically, and reject corrupt magic, version, tags, flags, lengths, UTF-8, booleans,
optionals, digests, fragment ranges, duplicates, unsafe integers, trailing bytes, and
values one byte above their applicable limit.
