# Replication specification

This document defines replication for Ephemeral AI FS version 0.1. It is
normative for `@ephemeralai/fs-replication`.

The words MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY have the meanings stated
in the repository-level [`SPEC.md`](../../SPEC.md).

## 1. Scope

Replication moves verified filesystem state between Ephemeral AI FS databases.
It supports these version 0.1 flows:

- pull authoritative main revisions into a read-write execution replica;
- push one private branch from an execution replica to the main authority;
- pull a private branch into another approved replica; and
- resume any of those flows after a process, transport, or peer failure.

Every source and destination remains a SQLite-backed Ephemeral AI FS. SQLite is
the authority for revisions, branches, objects, manifests, replication
receipts, staging leases, and cursors. An acknowledgement held only in process
memory is never authoritative.

The replication package owns:

- capability negotiation;
- bounded batch construction and validation;
- object and manifest negotiation;
- revision and branch transfer;
- durable cursors, receipts, and staging;
- retry, replay, and crash recovery;
- resource admission and backpressure; and
- replication observations and stable errors.

The replication package does not own:

- network connections, HTTP, WebSockets, or remote procedure calls;
- authentication, authorization, encryption, or peer discovery;
- container or process lifecycle;
- automatic conflict resolution;
- SQLite replacement or a second source of truth; or
- publication of a branch after it reaches the authority.

A host MUST authenticate and authorize a peer before forwarding a replication
message. Replication identifiers and cursor secrets are not credentials.

## 2. Package boundary

The package MUST expose a host-neutral surface equivalent to:

```ts
interface ReplicationTransport {
  exchange(
    request: Uint8Array,
    options?: { signal?: AbortSignal },
  ): Promise<Uint8Array>;
}

interface ReplicationEndpoint {
  exchange(request: Uint8Array): Promise<Uint8Array>;
  close(): Promise<void>;
}

interface ReplicationPlan {
  readonly pullMain?: boolean;
  readonly pushBranchId?: string;
  readonly pullBranchId?: string;
}

interface ReplicateOptions {
  readonly filesystem: EphemeralFS;
  readonly transport: ReplicationTransport;
  readonly plan: ReplicationPlan;
  readonly signal?: AbortSignal;
}

declare function createReplicationEndpoint(options: {
  filesystem: EphemeralFS;
  policy: ReplicationPolicy;
}): ReplicationEndpoint;

declare function replicate(
  options: ReplicateOptions,
): Promise<ReplicationResult>;
```

Names may change before the first release candidate. The division of ownership
is normative. A host provides one request-response transport function. The
package performs the handshake, batch loop, validation, durable application,
retry, and final result construction.

`ReplicationEndpoint.exchange` MUST be safe to expose through an existing host
RPC mechanism. Its `Uint8Array` is one package-defined bounded canonical
envelope. Before negotiation, a host transport MUST reject a wire frame larger
than 64 KiB before buffering or decoding it. After negotiation, it enforces the
negotiated envelope limit. The package MUST NOT require host code to decode the
envelope or inspect filesystem tables, content hashes, revision deltas, branch
overlays, leases, receipts, or cursors.

## 3. Roles and authority

Each opened endpoint has one role:

`main-authority`
: Owns the accepted main history for one filesystem identity. It may export
  main revisions and may accept private branch imports.

`replica`
: Holds an exact replicated prefix of authoritative main and may own private
  branches. It may import main revisions and export or import approved private
  branches. It MUST NOT originate an authoritative main revision.

A filesystem identity MUST have at most one configured `main-authority` in one
deployment. Detecting two configured authorities is a host responsibility.
Peers MUST still reject divergent main histories.

Replication MUST NOT merge two main histories. A replica may advance only from
its current authoritative prefix to a later prefix from the same authority. A
destination with local main changes that are not an exact prefix MUST fail with
`MainDiverged` before changing visible state.

Branch publication remains an explicit filesystem operation at the authority.
Importing a branch MUST NOT publish it implicitly.

## 4. Capability handshake

Every session MUST start with a handshake before content negotiation. The
handshake MUST include at least:

```ts
interface ReplicationCapabilities {
  readonly protocolVersions: readonly string[];
  readonly filesystemId: string;
  readonly applicationId: number;
  readonly schemaVersion: number;
  readonly role: "main-authority" | "replica";
  readonly hashAlgorithms: readonly ["sha256"];
  readonly manifestFormats: readonly string[];
  readonly chunkerFormats: readonly string[];
  readonly fastCdc: FastCdcConfiguration;
  readonly copyOnWritePageBytes: 4096 | 8192 | 16384;
  readonly features: ReplicationFeatures;
  readonly limits: ReplicationLimits;
  readonly storage: ReplicationStorageCapabilities;
}

interface ReplicationStorageCapabilities {
  readonly maxBlobBytes: number;
  readonly maxManifestBytes: number;
  readonly maxManagedPayloadBytes: number;
  readonly maxStagingPayloadBytes: number;
  readonly maxMaintenanceBytes: number;
  readonly maintenanceReserveBytes: number;
  readonly maxPermanentIdentifiers: number;
  readonly maxFinalTransactionRows: number;
  readonly maxFinalTransactionBytes: number;
}
```

The protocol identifier for this document is `efs-replication-v1`.

The filesystem identifier, application identifier, schema compatibility,
hash algorithm, manifest format, chunker format, FastCDC parameters, and
copy-on-write page size affect interpretation of persisted state. A peer MUST
reject an incompatible value before creating a cursor or staging lease.

The copy-on-write page size is independent from FastCDC minimum, average, and
maximum chunk sizes. A new filesystem MUST persist one page size from 4, 8, or
16 KiB. The balanced default is 8 KiB. Replication MUST advertise the persisted
value and MUST NOT translate pages between sizes during transfer.

Peers MAY support several readable manifest or chunker formats. They MUST agree
on the exact persisted format of every transferred value. Negotiation MUST NOT
silently reinterpret or rewrite an existing revision.

Feature flags MUST state support for:

- main revision pull;
- checkpoint bootstrap;
- branch push;
- branch pull;
- compact manifest transfer;
- durable staging leases; and
- physical restart recovery.

The effective session limits are the minimum compatible values from both peers.
A handshake MUST fail with `IncompatibleLimit` when one object, manifest,
or required protocol record cannot fit within those limits.

## 5. Resource limits

Each endpoint MUST expose effective limits equivalent to:

```ts
interface ReplicationLimits {
  readonly maxBatchEntries: number;
  readonly maxBatchBytes: number;
  readonly maxBufferedBytes: number;
  readonly maxInFlightBatches: number;
  readonly maxConcurrentSessions: number;
  readonly maxStagingBytesPerSession: number;
  readonly maxReplicationSessionRows: number;
  readonly maxReplicationMetadataBytes: number;
  readonly maxReceiptsPerSession: number;
  readonly maxReceiptBytesPerSession: number;
  readonly maxCursorBytes: number;
  readonly maxTerminalResultBytes: number;
  readonly maxCursorAgeMs: number;
  readonly stagingLeaseMs: number;
  readonly resultRetentionMs: number;
  readonly maxRetryAttempts: number;
  readonly maxRetryElapsedMs: number;
  readonly minRetryDelayMs: number;
  readonly maxRetryDelayMs: number;
}
```

All values MUST be positive safe integers. Version 0.1 MUST use one in-flight
batch per session. The default `maxBatchEntries` is 256. `maxBatchBytes`
defaults to the configured `maxManifestBytes` plus 64 KiB of protocol
overhead, which is 16 MiB plus 64 KiB under storage defaults.
`maxBufferedBytes` defaults to `maxBatchBytes + 1 MiB`. These are admission
ceilings, not eager allocations. Other defaults are 16 concurrent sessions,
64 MiB of durable staging per session, a 24-hour maximum
cursor age, and the filesystem's `StorageLimits.stagingLeaseMs`, which
defaults to 15 minutes. Aggregate durable staging remains constrained by
`maxStagingPayloadBytes`, so per-session allowances never multiply past the
filesystem quota.

The remaining defaults are 10,000 active or retained session rows, 64 MiB of
aggregate replication metadata, 100,000 receipts and 16 MiB of receipt records
per session, 256-byte public cursors, 1 MiB terminal results, and 30-day result
retention. Retry defaults are eight attempts over at most five minutes with
delays bounded from 100 milliseconds through 10 seconds. A host MAY configure
lower values that can still contain the largest required atomic protocol
record.

Before negotiation, a version 0.1 protocol envelope is limited to 64 KiB,
64 capability entries, 256 UTF-8 bytes per identifier or format string, and
4 KiB of error text. The transport MUST reject an oversized envelope, array,
string, or byte value before fully decoding or allocating it. A public cursor
is a bounded random lookup token, not an encoded replication inventory.

`maxBatchBytes` counts all binary payload bytes and UTF-8 bytes in application
values. `maxBatchEntries` separately bounds object descriptors, object payloads,
manifest records, revision rows, namespace rows, overlay rows, expectations,
and result rows. Fixed protocol fields MUST have documented finite maxima.

All session, cursor, receipt, digest-chain, fragment-summary, export-snapshot,
and terminal-result rows count against both `maxReplicationMetadataBytes` and
the filesystem's `maxMaintenanceBytes`. Staged content counts against
`maxStagingBytesPerSession`, `maxStagingPayloadBytes`, and
`maxManagedPayloadBytes`. Admission MUST preserve `maintenanceReserveBytes`
and serialize concurrent quota checks in the transaction that accepts rows.

The core MUST validate declared lengths before allocating, decoding, hashing,
or binding a value. It MUST reject a batch whose actual length differs from its
declaration. It MUST NOT allocate from an untrusted count before validating the
count against both effective limits.

All live replication-owned buffers for one session, including a received
message, a produced response, hash input, decoded records, and queued output,
MUST fit within `maxBufferedBytes`. A package-wide admission controller MUST
also bound aggregate session buffers. Starting work above the configured
aggregate limit MUST wait with backpressure or fail with `ResourceLimit`.

The package-wide controller MUST reserve those buffers from the opened
filesystem's `RuntimeLimits.maxManagedResidentBytes`. Encoded results also count
against `maxPreparedResultBytes`; query pages count against
`maxQueryBatchBytes`; and incoming or outgoing staged payload copies count
against `maxPendingWriteBytes`. Replication limits are narrower protocol
limits, not an additional memory allowance. Concurrent sessions MUST share
the same aggregate reservations as filesystem streams and Node VFS sessions.

## 6. Sessions and cursors

A session is identified by at least 128 bits of collision-resistant randomness
and a secret owner nonce. Both peers MUST persist their side of the session in
their own SQLite database. Durable state includes direction, scope, peer
identity, negotiated protocol and limits, phase, cursor position, selected
head or branch generation, sequence and payload digest, cumulative result
counters, retry budget, leases, and expiry.

The source MUST acquire a durable outbound export lease. A main export lease
roots the selected revision. Exporting a mutable branch MUST first capture an
immutable generation snapshot in bounded SQLite batches and root that
snapshot. Enumeration progress is a SQLite keyset cursor. A session MUST NOT
hold a SQLite read transaction open across transport exchanges or rely on a
process-local snapshot to protect export content.

A public cursor is opaque. It MUST bind to:

- the session and owner nonce;
- the source and destination filesystem identities;
- the direction and scope;
- the selected main head or branch identity and generation;
- the protocol phase and next sequence number; and
- the negotiated capability digest.

A cursor MUST NOT contain the only copy of progress. A peer MUST resolve it
against durable SQLite state. A cursor presented to another session, scope,
filesystem, generation, or capability set MUST fail with `CursorMismatch`.

Session progress MUST advance in the same transaction that durably accepts a
batch. A response MUST be returned only after that transaction commits. Losing
the response therefore causes replay, not ambiguous progress.

Cursor expiry MUST atomically mark inbound staging and outbound export leases
non-rooting, but MUST NOT change visible main or branch state. Physical row
cleanup MUST then run in mandatory bounded maintenance batches. A session
resumed after expiry MUST negotiate missing content again or fail with
`CursorExpired`.

## 7. Batch contract and idempotency

Every mutating batch MUST contain:

- session identifier;
- direction and scope;
- phase;
- monotonically increasing sequence number;
- prior cursor digest;
- entry count and payload byte count;
- canonical payload digest; and
- records for exactly one protocol phase.

The package MUST define one deterministic, length-prefixed canonical encoding
for batch digest calculation. The encoding MUST distinguish record type,
integer width, null, empty bytes, empty text, and absent optional fields.
Encoding and SHA-256 calculation MUST be incremental or use bounded codec
blocks; it MUST NOT allocate a second complete batch representation. Golden
vectors MUST cover every record type before a stable release.

The destination MUST record one receipt for each accepted sequence. Replaying
the same sequence and payload digest MUST return the original acknowledgement
without duplicating a row or advancing progress again. Reusing a sequence with
a different digest, count, byte length, cursor, or phase MUST fail with
`BatchReplayMismatch` and MUST NOT change state.

A batch is atomic. A limit error, integrity error, constraint failure, busy
failure, injected crash, or abort MUST leave its receipt, cursor, staging
membership, and accepted records at their previous committed values.

Receipts MUST be compacted in bounded batches after a later durable checkpoint
covers them or before a receipt quota would be exceeded. The session MUST
retain a bounded digest-chain summary sufficient to reject an old batch that
could otherwise be mistaken for new work. It MUST reject new work with
`ResourceLimit` when safe compaction cannot restore quota headroom.

The batch-acceptance transaction MUST update durable cumulative counters and
this summary:

```text
chainDigest = SHA256(previousDigest || sequence || batchDigest)
acceptedEntries += batchEntries
acceptedBytes += batchBytes
```

A terminal result MUST be stored in bounded canonical form. Retrying a lost
final response before result retention expires MUST return exactly that
stored result.

## 8. Transfer phases

A session MUST use only the phases required by its plan, in this order:

1. handshake;
2. scope selection;
3. immutable-content offer;
4. missing-content request;
5. immutable-content transfer;
6. revision, checkpoint, or branch-state transfer;
7. final validation and atomic activation;
8. result acknowledgement; and
9. bounded staging and receipt cleanup.

A peer MUST reject a phase transition that skips required validation. It MAY
repeat an earlier idempotent phase after reconnect when durable progress proves
that doing so is safe.

Only one side sends a mutating batch at a time. Request-response flow control is
the version 0.1 backpressure mechanism. A sender MUST NOT prepare the next full
batch while a prior mutating batch is unacknowledged.

## 9. Object and manifest negotiation

Immutable content negotiation MUST operate on bounded pages of descriptors.
An object descriptor contains its SHA-256 hash and byte length. A manifest
descriptor contains its format, SHA-256 hash, encoded length, and logical file
length. Compact manifest version 1 is one bounded BLOB, not a block tree.

The sender first offers descriptors. The receiver returns only missing or
unverified identities. The sender MUST NOT send bytes that were not requested,
except when a negotiated small-value optimization fits the same batch limits.

The receiver MUST verify before accepting immutable content:

- the descriptor and payload lengths;
- the SHA-256 digest;
- the object or manifest format;
- manifest structure, ordering, and checked size arithmetic; and
- every adapter BLOB and binding capability.

An existing digest is deduplication only after its stored value is verified.
A mismatch is `IntegrityFailure`; the receiver MUST NOT overwrite either value.

Each accepted object or manifest and its staging-lease membership MUST commit
in one SQLite transaction. A compact version 1 manifest MUST fit one batch;
the receiver MUST verify its complete canonical BLOB before insertion. A
future segmented manifest format requires a separately negotiated format and
fragment contract. Accepted immutable content MUST remain invisible to main
and branch namespace state until final activation. Orphaned immutable content
is safe for later bounded garbage collection.

Negotiation MUST be storage proportional to missing immutable content. It MUST
NOT copy an object merely because its path, inode, revision, or branch changed.

## 10. Main revision replication

The source main head selection MUST occur in one SQLite read snapshot. The
session MUST bind to that selected head. Revisions committed after selection
belong to a later session or continuation and MUST NOT appear halfway through
the selected transfer.

The destination MUST prove that its main head is an ancestor prefix of the
selected source head. The source MUST then send every required immutable
revision header and namespace delta in parent order. A destination MUST NOT
install a revision with a missing or different parent.

Revision identifiers and all durable conflict tokens MUST remain exact. The
receiver MUST NOT allocate replacement identifiers, resample timestamps, or
recompute writer metadata.

A revision larger than one batch MUST be staged as bounded fragments. No
fragment may update the visible head. One final short SQLite transaction MUST:

1. validate bounded durable summary rows, expected chain digest, and counts;
2. validate the expected destination head and source parent chain;
3. rely on indexed staged membership and foreign keys for verified references;
4. install revision and namespace rows within configured row and byte limits;
5. advance the destination head; and
6. mark that revision's staging lease non-rooting in constant row work.

The final transaction MUST NOT rescan or rehash payload, rebuild a digest over
all fragments, issue one statement per referenced object, or delete all
staging-membership rows. Later maintenance deletes membership rows in bounded
batches.

The final transaction MUST obey the filesystem's configured transaction row
and bound-byte limits. A revision that cannot meet them MUST fail with
`ResourceLimit`; replication MUST NOT weaken atomicity.

When the destination head is older than exported revision deltas, peers MAY use
a negotiated complete checkpoint. Checkpoint rows MUST be staged in bounded
transactions, validated by count and canonical digest, and made authoritative
only by a short final transaction. Incomplete checkpoint staging is never a
main or garbage-collection root except through its staging lease.

## 11. Branch replication

The identity of a replicated branch is the tuple:

```text
(filesystemId, branchId, baseRevision, generation)
```

The source MUST select one committed branch generation in a SQLite snapshot.
The transfer MUST include the branch state, base revision, namespace overlay,
file overlay, copy-on-write pages, structural patches, expectations, and exact
references to immutable manifests and objects needed by that generation.

The copy-on-write page size MUST equal the persisted filesystem page size from
the handshake. It MUST remain separate from FastCDC configuration. Page rows
MUST preserve exact page index and logical length.

The destination MUST already contain the exact base revision or reject the
import with `BaseRevisionMissing`. It MUST apply these identity rules:

- an unused branch identifier may be reserved for the imported branch;
- the same identity and generation is an idempotent replay;
- an existing lower generation may advance only from its exact prior digest;
- an existing higher generation rejects the stale import; and
- a used identifier bound to another base or history fails with
  `BranchIdentityMismatch`.

Replication MUST NOT merge two independently mutated copies of one branch.
Such copies fail with `BranchDiverged`. A host must use separate branch
identifiers or publish and create a later branch.

A branch generation larger than one batch MUST be staged in bounded fragments.
One final SQLite transaction MUST validate bounded summaries, base revision,
generation predecessor, expectations, page size, and indexed verified
membership before making the generation visible. A new branch identifier MUST
reserve from `maxPermanentIdentifiers` in that transaction. The transaction
MUST NOT rescan or rehash the complete generation. A crash before commit
leaves the prior generation unchanged.

Importing a terminal branch MAY preserve its terminal metadata for replay, but
MUST NOT resurrect it as active. Importing an active branch to a main authority
does not publish it.

## 12. Staging leases and cleanup

Before accepting the first immutable or mutable staged row, a destination MUST
create a durable replication staging lease. The lease MUST bind to the session,
owner nonce, peer identities, direction, scope, selected head or branch
generation, and capability digest.

Every staged allocation and its membership MUST commit atomically. Lease
renewal MUST compare the owner nonce and prior expiry in one transaction. It
MUST NOT revive an expired or released lease.

Final activation MUST convert staged rows into visible reachable state and
change the lease to a non-rooting released state in constant row work. It MUST
NOT delete every membership row in that transaction. Cleanup after success or
abort is mandatory, idempotent, and runs in bounded maintenance batches. A
process crash may retain staging only until lease expiry.

Expired staging MUST never authorize deletion by itself. Garbage collection
continues to use the filesystem's generation-safe, high-water-mark rules. A
replication session MUST reserve enough configured staging capacity and cleanup
headroom before accepting payload.

Effective payload admission is the minimum remaining capacity across the
session staging limit, filesystem staging quota, filesystem managed-payload
quota, replication metadata quota, and capacity excluding the maintenance
reserve. The accepting transaction MUST serialize that quota decision across
concurrent sessions.

## 13. Crash, retry, and cancellation

SQLite transaction recovery is authoritative after interruption. On restart:

- committed receipts and cursor progress remain replayable;
- committed immutable staging remains protected by an unexpired lease;
- an incomplete batch has no receipt or progress;
- a completed final activation is visible exactly once;
- an incomplete activation leaves the prior main or branch generation visible;
  and
- process-local queues and acknowledgements are discarded.

A transport error before an acknowledgement is ambiguous to the sender and
MUST cause the same sequence to be replayed. A transport error after an
acknowledgement affects only later work.

Every transport attempt MUST atomically consume the session's durable attempt
and elapsed-time budget. Delay must remain between `minRetryDelayMs` and
`maxRetryDelayMs`. Restart MUST NOT reset either budget. Exceeding
`maxRetryAttempts` or `maxRetryElapsedMs` fails with `RetryExhausted`.
Process-local request, response, and codec buffers MUST be released between
attempts; durable SQLite session and staging state is the only retained retry
state.

Transient SQLite busy failures MAY be retried using the filesystem adapter's
bounded policy. A retry MUST rerun a pure database transaction and MUST NOT
duplicate an externally visible callback or observer event.

Abort stops creating new requests. An in-flight exchange MAY finish. If its
batch commits, its durable cursor is the resume point. Abort MUST attempt
bounded lease release, but failure to release MUST NOT hide the abort result.

After `resultRetentionMs`, maintenance MUST atomically mark the terminal
result and remaining session leases non-rooting. It MUST delete terminal
results, receipts, cursors, export snapshots, and staging membership in later
bounded batches while preserving cleanup reserve.

## 14. Backpressure and memory safety

The high-level driver MUST wait for each response before submitting another
mutating batch. An endpoint MUST finish validating and committing a request
before resolving its response. A transport that internally buffers messages
MUST still honor the negotiated buffer limit.

Content hashing MUST be incremental or operate on one bounded object. Manifest,
revision, checkpoint, and branch enumeration MUST use SQLite keyset cursors and
bounded queries. OFFSET pagination and unbounded `IN` lists MUST NOT be used.

The implementation MUST NOT materialize a complete filesystem, large revision,
large branch, or complete missing-object set in memory. It MUST NOT call an
adapter `all` operation without a finite result bound derived from negotiated
limits.

Slow receivers naturally stop senders through the request-response loop. A
slow destination MUST NOT cause the sender to retain an unbounded queue. A
session above its staging quota MUST stop requesting content and return
`ResourceLimit` without changing visible state.

## 15. Errors

The package MUST expose a stable `ReplicationError` with at least these codes:

```ts
type ReplicationErrorCode =
  | "ProtocolMismatch"
  | "FilesystemMismatch"
  | "SchemaMismatch"
  | "CapabilityMismatch"
  | "IncompatibleLimit"
  | "UnauthorizedScope"
  | "MainDiverged"
  | "BaseRevisionMissing"
  | "BranchIdentityMismatch"
  | "BranchDiverged"
  | "CursorMismatch"
  | "CursorExpired"
  | "BatchReplayMismatch"
  | "StagingExpired"
  | "IntegrityFailure"
  | "ResourceLimit"
  | "Busy"
  | "TransportFailure"
  | "RetryExhausted"
  | "Aborted"
  | "Closed";
```

Protocol, identity, schema, capability, divergence, replay, and integrity
errors are not automatically retryable. Busy and transport errors are
retryable only within the negotiated durable retry policy. Resource failure is
retryable only after capacity or configuration changes.

An error MUST identify its phase and session when safe. It MUST NOT include
file content, lease owner nonces, cursor secrets, authentication material, or
unrequested path data.

## 16. Observability

The package MUST expose a result and optional observer events containing:

- session, direction, scope, and selected source head or branch generation;
- outcome and final durable cursor;
- offered, requested, transferred, reused, and rejected object counts;
- offered, transferred, and reused manifest counts;
- logical content bytes and transferred payload bytes;
- SQLite BLOB bytes submitted and retained;
- revision, checkpoint, namespace, overlay, and expectation row counts;
- batch, query, transaction, busy-retry, and replay counts;
- peak replication-owned buffered bytes;
- staging bytes and cleanup bytes;
- handshake, first-progress, transfer, activation, and total elapsed time; and
- deduplication and write-amplification ratios with stated byte boundaries.

Elapsed time MUST use a monotonic clock. Observers MUST run outside
authoritative transactions. Observer failure MUST NOT alter a result, retry,
receipt, cursor, or transaction outcome.

Physical database, WAL, and freelist bytes SHOULD be reported when the adapter
can measure them. They MUST remain distinct from logical and payload bytes.

## 17. Required invariants

Every implementation MUST preserve these invariants:

1. SQLite contains the only authoritative replication progress.
2. One accepted batch sequence binds to one canonical payload digest.
3. Visible main is an exact prefix of one authority's revision history.
4. Main replication never merges divergent histories.
5. One branch identifier binds to one base and one generation history.
6. Branch replication never merges independently mutated generations.
7. A visible revision or branch generation references only verified content.
8. A partial batch, revision, checkpoint, or branch generation is never visible.
9. Staged content is protected by one valid durable lease or is reclaimable.
10. Cursor progress and the corresponding applied batch commit together.
11. Replaying an acknowledged batch cannot add rows or advance state again.
12. Batch and aggregate buffers remain within effective resource limits.
13. Copy-on-write page size is persisted, advertised, and not FastCDC state.
14. A transport or process failure changes neither filesystem semantics nor
    identifier identity.
15. Host code is not required to implement or interpret replication protocol
    phases.
16. A verified immutable identity already present at the receiver contributes
    zero retransmitted payload bytes.
17. Sequential transfer never materializes a complete large file in one
    replication-owned buffer.

## 18. Conformance suite

The shared replication testkit MUST run against Node.js SQLite and Durable
Object SQLite adapters. Adapter hooks MUST support physical reopen, abrupt
restart, fault injection, controlled corruption, small capability overrides,
and a transport that can drop, duplicate, delay, and reorder responses.

The suite MUST cover:

1. Negotiate matching capabilities and reject each persisted mismatch.
2. Treat 4, 8, and 16 KiB copy-on-write pages independently from each tested
   FastCDC configuration, and reject a page-size mismatch before staging.
3. Transfer empty, one-object, deduplicated, and multi-batch files exactly.
4. Resume every phase after dropping its request or response.
5. Replay every accepted batch and observe the original acknowledgement.
6. Reuse a sequence with changed bytes and observe `BatchReplayMismatch`.
7. Inject failure after every statement in batch and activation transactions.
8. Kill and reopen each peer before and after every durable acknowledgement.
9. Pull a linear main prefix and reject a divergent destination without writes.
10. Bootstrap from a bounded checkpoint and reject incomplete staging.
11. Push a branch with namespace changes, pages, patches, expectations, hard
    links, symlinks, and branch-only immutable content.
12. Replay the same branch generation and reject stale, reused, and divergent
    identities.
13. Expire, renew, release, and race staging leases with garbage collection.
14. Corrupt object, manifest, delta, overlay, cursor, and receipt bytes and
    expose `IntegrityFailure` without partial visibility.
15. Force one-entry and small-byte batches and prove complete progress.
16. Apply backpressure with a slow peer and assert one in-flight mutating batch.
17. Run maximum concurrent sessions and assert aggregate buffer and staging
    limits without process-memory growth proportional to total source size.
18. Transfer a 100 MiB file and assert peak replication-owned buffers remain
    within the negotiated bound.
19. Transfer a one-byte edit to a 100 MiB file and assert unchanged content
    objects are negotiated as already present; transfer one new bounded
    canonical version 1 manifest.
20. Compare interrupted and uninterrupted final SQLite databases by namespace,
    revision, branch, content, cursor, receipt, and accounting state.
21. Restart the source during main and mutable-branch export; prove its
    selected snapshot, cursor, digest, and outbound lease recover from SQLite.
22. Exhaust session, receipt, cursor, metadata, staging, managed-payload, and
    maintenance quotas independently and observe bounded failure and cleanup.
23. Prove final activation uses bounded summary validation and constant-row
    lease release at the maximum revision and branch limits.
24. Exhaust retry attempts and elapsed time across restart, release every
    process buffer between attempts, and replay the retained terminal result.
25. Run replication with 64 streams, 64 Node VFS writers, maximum query pages,
    and garbage collection under one small managed-memory budget. The combined
    high-water MUST remain within that one budget.

Golden fixtures MUST cover the handshake capability digest, canonical batch
digest, cursor binding, one revision fragment, one checkpoint fragment, and one
branch-generation fragment.

## 19. Performance and release gates

Release benchmarks MUST use fresh isolated SQLite databases and reproducible
logical fixtures. They MUST report distributions, not one best run.

At minimum, release candidates MUST measure:

- a one-byte edit in a 100 MiB file;
- a sequential 100 MiB transfer without complete-file materialization;
- deduplicated transfer of an already present 100 MiB file;
- main catch-up across 1,000 small revisions;
- branch push with 100,000 changed paths at the configured result-byte limit;
- resume after a dropped response in every phase; and
- bounded garbage collection after abandoned replication staging; and
- end-to-end 100 MiB materialization through Computer's Node virtual
  filesystem or FUSE path after replication.

The one-byte edit MUST transfer the new bounded canonical manifest, only
missing content-object payloads, bounded revision metadata, and protocol
overhead. It MUST NOT retransmit unchanged content-object payload bytes.
Sequential transfer throughput and first-progress latency MUST be reported
separately. The sequential workload MUST prove that neither peer called a
complete-file materialization API or retained buffers proportional to file
size.

The Node virtual filesystem or FUSE workload is an integration release gate,
not a transport responsibility. It MUST read the replicated bytes through the
same path used by execution processes and compare their SHA-256 digest with the
source fixture.

Every benchmark MUST report p50, p95, and p99 elapsed time, peak
replication-owned buffered bytes, transferred payload, retained payload, SQLite
BLOB bytes submitted, query and transaction counts, and physical database and
WAL growth where measurable.

A release MUST NOT regress p95 elapsed time, peak buffered bytes, transferred
bytes, or SQLite BLOB bytes by more than 10 percent from the checked-in accepted
baseline without an approved benchmark record explaining the tradeoff. Peak
replication-owned buffered bytes MUST never exceed the negotiated limit.

## 20. Computer integration constraint

Ephemeral AI Computer should need only to:

1. create one endpoint around its selected Ephemeral AI FS instance;
2. expose `endpoint.exchange` through its existing authenticated RPC path; and
3. call `replicate` with that transport and an explicit plan.

All handshake, cursor, batching, negotiation, staging, retry, and validation
logic belongs to `@ephemeralai/fs-replication`. The Computer integration MUST
NOT import replication internals or filesystem schema modules.

Engine selection, authoritative opening, replication, Node VFS forwarding,
and the branch handshake share one production integration target of no more
than 100 net-new lines in the Computer repository. Tests, generated bindings,
and benchmark fixtures are excluded. Exceeding that aggregate target is
evidence that an Ephemeral AI FS package lacks a required host-neutral
abstraction and requires design review.
