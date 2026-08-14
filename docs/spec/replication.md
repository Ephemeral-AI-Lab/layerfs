# Replication specification

This document defines replication for Ephemeral AI FS version 0.1. It is normative for
`@ephemeralai/fs-replication`.

The words MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY have the meanings stated in the
repository-level [`SPEC.md`](../../SPEC.md).

## 1. Scope

Replication moves verified filesystem state between Ephemeral AI FS databases. It
supports these version 0.1 flows:

- copy authoritative main revisions from the main authority into an execution replica;
- copy one private branch from the main authority into an approved execution replica;
- copy one active private branch generation from an execution replica back to the main
  authority;
- copy a private branch into another approved replica; and
- resume any of those flows after a process, transport, or peer failure.

Every source and destination remains a SQLite-backed Ephemeral AI FS. SQLite is the
authority for revisions, branches, objects, manifests, replication receipts, staging
leases, and cursors. An acknowledgement held only in process memory is never
authoritative.

Replication copies the exact selected namespace. A host MUST NOT add path-ignore or
path-rewrite rules to this protocol. Execution scratch such as `node_modules` MUST
either be ordinary branch content or live on an explicitly separate scratch mount with
separate lifecycle, quota, and durability semantics.

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

A host MUST authenticate and authorize a peer before forwarding a replication message.
Replication identifiers and cursor secrets are not credentials.

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

type ReplicationPlan =
  | { readonly flow: "authority-main-to-replica" }
  | {
      readonly flow: "authority-branch-to-replica";
      readonly branchId: string;
    }
  | {
      readonly flow: "replica-branch-to-authority";
      readonly branchId: string;
    }
  | {
      readonly flow: "replica-branch-to-replica";
      readonly branchId: string;
    };

interface AuthorizedReplicationPeer {
  readonly principalId: string;
  readonly hostScopeId: string;
  readonly expectedFilesystemId: string;
  readonly expectedAuthorityId: string;
  readonly policyVersion: string;
  readonly hostProfile: "computer-efs-carrier-v1";
  readonly limitPolicy: ReplicationLimitPolicy;
  readonly allowedPlans: readonly ReplicationPlan[];
}

interface ReplicationLimitPolicy {
  readonly ceilings: Omit<ReplicationLimits, "minRetryDelayMs">;
  readonly minRetryDelayMsFloor: number;
}

interface ReplicationFilesystemBridge {
  readonly capabilities: ReplicationCapabilities;
  openOrResumeSession(
    binding: ReplicationSessionBinding,
  ): Promise<ReplicationSessionState>;
  captureExport(
    sessionId: string,
    plan: ReplicationPlan,
  ): Promise<ReplicationExportCursor>;
  readExportBatch(request: ReplicationBatchRequest): Promise<ReplicationBatch>;
  applyImportBatch(batch: ReplicationBatch): Promise<ReplicationBatchReceipt>;
  finalizeImport(request: ReplicationFinalizeRequest): Promise<ReplicationActivation>;
  recordAttempt(request: ReplicationAttemptRequest): Promise<ReplicationSessionState>;
  replayTerminalResult(request: ReplicationReplayRequest): Promise<ReplicationResult>;
  renewSessionLease(request: ReplicationLeaseRequest): Promise<void>;
  compactReceipts(request: ReplicationCompactionRequest): Promise<void>;
  maintainSessions(request: ReplicationMaintenanceRequest): Promise<void>;
  abortSession(sessionId: string): Promise<void>;
}

type ReplicationActivation =
  | { readonly kind: "main"; readonly revision: string }
  | {
      readonly kind: "branch";
      readonly branchId: string;
      readonly baseRevision: string;
      readonly generation: number;
      readonly generationDigest: string;
      readonly state: "active";
      readonly authorityResult: ReplicatedPublicationConflictResult | null;
    }
  | {
      readonly kind: "branch";
      readonly branchId: string;
      readonly baseRevision: string;
      readonly generation: number;
      readonly generationDigest: string;
      readonly state: "merged";
      readonly authorityResult: ReplicatedPublicationMergedResult;
    }
  | {
      readonly kind: "branch";
      readonly branchId: string;
      readonly baseRevision: string;
      readonly generation: number;
      readonly generationDigest: string;
      readonly state: "discarded";
      readonly authorityResult: ReplicatedDiscardResult;
    };

interface ReplicatedPublicationMergedResult {
  readonly kind: "publication";
  readonly operationId: string;
  readonly outcome: "merged";
  readonly resultDigest: string;
}

interface ReplicatedPublicationConflictResult {
  readonly kind: "publication";
  readonly operationId: string;
  readonly outcome: "conflict";
  readonly resultDigest: string;
}

interface ReplicatedDiscardResult {
  readonly kind: "discard";
  readonly operationId: string | null;
  readonly resultDigest: string;
}

interface ReplicationResult {
  readonly sessionId: string;
  readonly operationId: string;
  readonly plan: ReplicationPlan;
  readonly activation: ReplicationActivation;
  readonly finalCursor: string;
  readonly transferredBytes: number;
  readonly reusedBytes: number;
}

interface ReplicateOptions {
  readonly bridge: ReplicationFilesystemBridge;
  readonly transport: ReplicationTransport;
  readonly authorization: AuthorizedReplicationPeer;
  readonly plan: ReplicationPlan;
  readonly operationId: string;
  readonly resumeKey?: string;
  readonly signal?: AbortSignal;
}

declare function createReplicationEndpoint(options: {
  bridge: ReplicationFilesystemBridge;
  authorization: AuthorizedReplicationPeer;
}): ReplicationEndpoint;

type ReplicationRunResult =
  | { readonly status: "complete"; readonly result: ReplicationResult }
  | {
      readonly status: "pending";
      readonly resumeKey: string;
      readonly notBeforeMs: number;
      readonly reason: "busy" | "transport" | "backpressure";
    };

declare function replicate(options: ReplicateOptions): Promise<ReplicationRunResult>;
```

Names may change before the first release candidate. The division of ownership is
normative. A host provides one request-response transport function. The package performs
the handshake, batch loop, validation, durable application, retry, and final result
construction.

One plan describes exactly one global role flow and, for a branch flow, one branch. Its
meaning does not change with the initiating peer or endpoint coordinate system. Several
flows require several sessions. For Computer, the authority uses
`authority-main-to-replica` and `authority-branch-to-replica` before execution, then
`replica-branch-to-authority` returns the selected branch after execution. Branch
publication remains a separate authority-side filesystem operation.

`replicate()` always runs at the source named by the plan and the supplied bridge is
that source. The remote endpoint is the destination. For `replica-branch-to-replica`,
the initiating replica is the source and the endpoint replica is the destination.
Calling a plan from its destination role fails before durable session creation.

`operationId` is a bounded, caller-stable idempotency key. `resumeKey` is an opaque
lookup token returned by an earlier run; it is not a credential. A later process uses
the same operation identifier and resume key to find the durable session and retained
terminal result. A host may schedule the returned `notBeforeMs`, but it MUST NOT own a
second retry counter or reset the durable retry budget by creating a new session.

The bridge is created by `@ephemeralai/fs/integrations/replication`. Its types are
semantic and schema-free. It MUST expose typed core commands for authorized session
creation or resume, export capture, batch acceptance, durable attempt accounting,
terminal-result replay, lease lifecycle, receipt compaction, and bounded cleanup. It
MUST perform validation, resource admission, staging-certificate updates, and final
transactions through core operations. It MUST NOT expose SQL, tables, repositories,
standalone CAS insertion, or standalone COW mutation to the replication package.
`ReplicationSessionBinding` contains the operation identifier, exact plan, source and
destination identities and roles, package-computed authorization digest, capability
digest, effective limits, and retry policy. Every other bridge request names that
durable session and carries the expected owner nonce or sequence where applicable.

The public composition root MUST support one opened runtime handle from which the
portable filesystem, branch-scoped Node VFS, and replication bridge are derived. These
views MUST share one cache, mutation coordinator, and aggregate admission controller.
Opening independent core instances over the same database for Node VFS and replication
is not a supported Computer integration.

`ReplicationEndpoint.exchange` MUST be safe to expose through an existing host RPC
mechanism. Its `Uint8Array` is one package-defined bounded canonical envelope. The host
carrier MUST enforce a raw or decompressed outer-frame limit before parsing text,
decoding base64, decompressing an unbounded value, or constructing the replication
envelope. Before negotiation, the decoded envelope limit is 64 KiB. After negotiation,
the carrier enforces both its outer-frame limit and the negotiated decoded-envelope
limit. The carrier profile MUST account for encoding expansion and every transient copy
outside the replication package.

A text JSON or Cap'n Web carrier therefore needs a tested profile that defines raw,
decompressed, encoded, and decoded maxima. Direct binary transport tests alone are not
enough for Computer compatibility. The package MUST NOT require host code to decode the
replication envelope or inspect filesystem tables, content hashes, revision deltas,
branch overlays, leases, receipts, or cursors.

`computer-efs-carrier-v1` disables WebSocket compression for replication calls and
allows one exchange in flight per replication operation. Its post-negotiation decoded
request or response is at most 3 MiB, and a mutating acknowledgement is at most 64 KiB.
Its raw JSON/base64 WebSocket frame is at most 4 MiB plus 64 KiB of canonical RPC
framing. Because compression is disabled, raw and decompressed bytes are the same
backing value, not two retained copies. At most one raw frame, one decoded JavaScript
string charged at two bytes per code unit, one decoded envelope, one acknowledgement,
and 2 MiB of carrier scratch may coexist. Those maxima total 17.25 MiB and fit the 20
MiB transport reservation. A peer MUST reject `maxRequestBytes`, `maxResponseBytes`,
compression, or concurrency that exceeds this profile before starting replication.

All Computer replication operations in one host process share one 20 MiB aggregate
carrier admission controller. An exchange reserves its conservative simultaneous-copy
maximum before reading a frame and releases it exactly once. At most one maximum-sized
17.25 MiB exchange may be admitted process-wide; smaller exchanges may coexist only when
their total reservations remain at or below 20 MiB. The generic replication session
count does not multiply this carrier budget.

Protocol success and failure are canonical response-envelope values. The high-level
driver reconstructs `ReplicationError` from a bounded error value; it MUST NOT depend on
the host remote procedure call library preserving a thrown JavaScript error's prototype,
properties, or code. Carrier authentication, framing, and connection failures remain
host transport failures.

The host MUST construct both the initiating driver and inbound endpoint only after
authenticating the connection. Their immutable authorization binds the principal, host
workspace or scope, exact filesystem and authority identities, policy version, host
profile, allowed global plans, and effective limits. Provisioning therefore requires an
authenticated expected filesystem and authority identity even though the empty local
database is not yet bound. An endpoint MUST reject an envelope that attempts a different
plan.

The package computes the authorization digest from a bounded canonical record containing
every authorization field and effective limit in the version 1 wire encoding. It MUST
NOT accept a caller-supplied digest. The computed digest is stored with durable session
state so a resume under a different principal, policy, profile, plan, identity, or limit
fails before mutation. Cursor and owner nonce values never substitute for host
authentication.

`ReplicationEndpoint.close()` releases process buffers, observers, and transport-facing
resources. It MUST preserve resumable SQLite sessions and terminal results. Only an
explicit abort, expiry, or bounded maintenance transition makes durable state
non-rooting.

## 3. Roles and authority

Each opened endpoint has one role:

`main-authority` : Owns the accepted main history for one filesystem identity. It may
export main revisions, active or terminal private branch state, and authority-owned
publication results. It may accept only active private branch generations from an
authorized replica.

`replica` : Holds a read-only exact replicated prefix of authoritative main and may own
private branches. It may import main revisions and export or import approved private
branches. It MUST NOT originate an authoritative main revision. A replica-side public
filesystem or Node VFS opening main MUST reject mutation with `EROFS`; writable
execution MUST bind to an active private branch.

A filesystem identity MUST have at most one configured `main-authority` in one
deployment. Detecting two configured authorities is a host responsibility. Peers MUST
still reject divergent main histories.

Replication MUST NOT merge two main histories. A replica may advance only from its
current authoritative prefix to a later prefix from the same authority. A destination
with local main changes that are not an exact prefix MUST fail with `MainDiverged`
before changing visible state.

Branch publication remains an explicit filesystem operation at the authority. Importing
a branch MUST NOT publish it implicitly.

The version 1 role matrix is normative:

| Flow                          | Source role    | Destination role | Allowed state                           |
| ----------------------------- | -------------- | ---------------- | --------------------------------------- |
| `authority-main-to-replica`   | main authority | replica          | authoritative main prefix or checkpoint |
| `authority-branch-to-replica` | main authority | replica          | active or terminal branch and results   |
| `replica-branch-to-authority` | replica        | main authority   | active branch generation only           |
| `replica-branch-to-replica`   | replica        | replica          | active approved branch only             |

Every other role, flow, or branch-state combination fails with `UnauthorizedScope`
before cursor, lease, receipt, staging, or visible mutation. A replica never exports
main. A main authority never accepts terminal state or publication results from a
replica.

## 4. Capability handshake

Every session MUST start with a handshake before content negotiation. The handshake MUST
include at least:

```ts
interface ReplicationCapabilities {
  readonly protocolVersions: readonly string[];
  readonly hostProfile: "computer-efs-carrier-v1";
  readonly provisioningState: "bound" | "unbound-replica";
  readonly filesystemId: string | null;
  readonly authorityId: string | null;
  readonly applicationId: number | null;
  readonly filesystemSchemaVersion: number | null;
  readonly storageUserVersion: number;
  readonly storageMigrationState: "none";
  readonly readableFilesystemSchemaVersions: readonly number[];
  readonly writableFilesystemSchemaVersion: number;
  readonly role: "main-authority" | "replica";
  readonly hashAlgorithms: readonly ["sha256"];
  readonly activeManifestFormat: string | null;
  readonly supportedManifestFormats: readonly string[];
  readonly activeChunkerFormat: string | null;
  readonly supportedChunkerFormats: readonly string[];
  readonly fastCdc: FastCdcConfiguration | null;
  readonly supportedFastCdcConfigurations: readonly FastCdcConfiguration[];
  readonly copyOnWritePageBytes: 4096 | 8192 | 16384 | null;
  readonly supportedCopyOnWritePageBytes: readonly (4096 | 8192 | 16384)[];
  readonly features: ReplicationFeatures;
  readonly limits: ReplicationLimits;
  readonly storage: ReplicationStorageCapabilities;
}

interface ReplicationFeatures {
  readonly authorityMainToReplica: boolean;
  readonly authorityBranchToReplica: boolean;
  readonly replicaBranchToAuthority: boolean;
  readonly replicaBranchToReplica: boolean;
  readonly checkpointBootstrap: boolean;
  readonly segmentedMerkleManifestTransfer: boolean;
  readonly durableStagingLeases: boolean;
  readonly physicalRestartRecovery: boolean;
  readonly terminalResultReplication: boolean;
  readonly freshReplicaProvisioning: boolean;
}

interface ReplicationStorageCapabilities {
  readonly maxBlobBytes: number;
  readonly maxManifestNodeBytes: number;
  readonly maxManifestDepth: number;
  readonly maxManagedPayloadBytes: number;
  readonly maxStagingPayloadBytes: number;
  readonly maxMaintenanceBytes: number;
  readonly maintenanceReserveBytes: number;
  readonly maxPermanentIdentifiers: number;
  readonly maxFinalTransactionRows: number;
  readonly maxFinalTransactionBytes: number;
}
```

The protocol identifier for this document is `efs-replication-v1`. The required
new-write manifest format is `efs-merkle-manifest-v1` and the required chunker format is
`fastcdc-v1`.

`filesystemSchemaVersion` is the logical filesystem schema stored in `efs_meta`.
`storageUserVersion` is the adapter's durable SQLite schema version. They are separate
values and MUST NOT be compared or reported as one generic schema version.

Version 1 accepts exactly this initial compatibility row:

| Field                     | Accepted value                               |
| ------------------------- | -------------------------------------------- |
| protocol                  | `efs-replication-v1`                         |
| SQLite application ID     | `0x45414653`                                 |
| logical filesystem schema | `13`                                         |
| storage user version      | `13`, with no migration in progress          |
| hash                      | SHA-256                                      |
| manifest                  | `efs-merkle-manifest-v1`                     |
| chunker                   | `fastcdc-v1` with exact persisted parameters |
| copy-on-write page        | exact persisted 4, 8, or 16 KiB value        |
| Computer host profile     | `computer-efs-carrier-v1`                    |

An unbound replica is the only exception: it advertises application ID `0x45414653` and
storage user version `13`, but null filesystem, authority, logical schema,
active-format, FastCDC, and page values, plus the supported version 1 sets. Provisioning
adopts the authority's logical and format row exactly. It does not run a storage
migration.

Both bound and unbound version 1 endpoints advertise `storageMigrationState: "none"`.
Any other marker fails with `SchemaMismatch` before session creation. The capability
golden fixture includes the provisioning and migration-state fields explicitly.

Peers select the highest common protocol version deterministically. Version 1 does not
downgrade or migrate during replication. A protocol mismatch is `ProtocolMismatch`; an
application, logical schema, storage version, or migration-state mismatch is
`SchemaMismatch`; and a hash, manifest, chunker, page, or host-profile mismatch is
`CapabilityMismatch`. A future compatible row requires a normative spec amendment and
golden vectors before implementation advertises it. Independently deployed Computer
package versions interoperate only when both advertise `computer-efs-carrier-v1` and the
same accepted row.

The filesystem identifier, authority identifier, application identifier, schema
compatibility, hash algorithm, manifest format, chunker format, FastCDC parameters, and
copy-on-write page size affect interpretation of persisted state. A bound endpoint MUST
advertise non-null filesystem and authority identifiers. An unbound-replica endpoint
MUST advertise null identifiers and may negotiate only the authenticated fresh-replica
flow below. Every ordinary flow MUST reject an incompatible value before creating a
cursor or staging lease.

The copy-on-write page size is independent from FastCDC minimum, average, and maximum
chunk sizes. A new filesystem MUST persist one page size from 4, 8, or 16 KiB. The
balanced default is 8 KiB. Replication MUST advertise the persisted value and MUST NOT
translate pages between sizes during transfer.

Peers MAY support several readable manifest or chunker formats. They MUST agree on the
exact persisted format of every transferred value. Negotiation MUST NOT silently
reinterpret or rewrite an existing revision.

Feature flags MUST state support for:

- authority main to replica, including checkpoint bootstrap;
- authority active or terminal branch and publication results to replica;
- replica active branch to authority;
- approved replica active branch to replica;
- segmented Merkle manifest transfer;
- durable staging leases; and
- physical restart recovery.

`ReplicationFeatures` is encoded in exactly the interface declaration order above as ten
canonical version 1 booleans, each one byte `0x00` or `0x01`. Unknown trailing feature
fields are not accepted in protocol version 1. The capability digest and golden fixture
cover this exact order and reject any other length or boolean byte.

Limit negotiation uses each peer's advertised `ReplicationLimits`, its authenticated
`ReplicationLimitPolicy`, and the fixed host-profile limits. For every field except
`minRetryDelayMs`, the effective value is the minimum of the source advertisement,
destination advertisement, source authorization ceiling, destination authorization
ceiling, and host-profile ceiling. Effective `minRetryDelayMs` is the maximum of both
advertisements, both authorization floors, and the host-profile floor.

The package MUST then validate all values as positive safe integers and validate
`minRetryDelayMs <= maxRetryDelayMs`, one in-flight batch for version 1, request and
response fit within their carrier maxima, one batch plus canonical framing fits its
applicable request or response, simultaneous buffers fit `maxBufferedBytes`, required
atomic records fit their entry and byte limits, and staging plus maintenance reserve fit
the filesystem quotas. Any impossible combination is `IncompatibleLimit` before session
or lease creation. The complete effective limit record is encoded in declaration order,
included in both capability and authorization digests, and persisted with the durable
session so restart cannot renegotiate a different result.

### 4.1 Authenticated fresh-replica provisioning

A new execution replica MUST NOT open as an ordinary independent filesystem and then
pretend its randomly generated identity belongs to the authority. The public composition
root MUST instead support an explicit durable `unbound-replica` storage state. Its first
open accepts only a physically empty selected database, installs the Ephemeral AI FS
application identity and version 13 storage schema, and records an unbound marker
without creating filesystem identity, root inode, revision zero, main, or an active
format row.

The unbound schema may contain only its marker, authenticated provisioning sessions,
receipts, leases, and verified bounded staging. Reopen MUST recognize that exact state
and resume it after every accepted batch. It is still unbound even though its SQLite
database is no longer physically empty. A database containing any visible filesystem
genesis, foreign table, wrong application identity, unsupported storage version,
conflicting authority binding, or non-provisioning Ephemeral AI FS state is not an
unbound replica and MUST be rejected without further writes.

After host authentication and authorization, the first main bootstrap MUST atomically
adopt the authority's exact filesystem and genesis identity. The adopted state includes
the filesystem identifier, root inode, revision-zero metadata, timestamps, conflict
tokens, persisted page size, manifest and chunker formats, and writer profile. The same
transaction MUST bind the configured authority identity and persistent replica role.

The public filesystem composition API MUST create an unbound replica runtime only for a
selected physically empty database or the exact recognized durable unbound state. That
runtime exposes a provisioning-only replication bridge and no portable filesystem or
Node VFS view. After the bounded final provisioning transaction installs the complete
verified genesis, binds the authority, and changes the marker to bound, the caller
reopens or promotes it as a bound replica runtime. Until then, ordinary main or branch
catch-up, branch replication, and every filesystem operation fail before mutation.

Provisioning MUST fail without writes when the database is unrelated nonempty state,
belongs to DOFS or another engine, was already bound to another workspace or authority,
has an incompatible storage identity, or receives a plan outside the authorized host
scope. It MUST NOT accept a caller-supplied filesystem identifier without the complete
authenticated genesis record. Restart before the final activation reopens the same
durable unbound state and resumes its session and staging; restart after activation
opens the same bound replica. The conformance suite MUST restart after every accepted
provisioning batch and on both sides of final activation.

## 5. Resource limits

Each endpoint MUST expose effective limits equivalent to:

```ts
interface ReplicationLimits {
  readonly maxBatchEntries: number;
  readonly maxBatchBytes: number;
  readonly maxRequestBytes: number;
  readonly maxResponseBytes: number;
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

All values MUST be positive safe integers. Version 0.1 MUST use one in-flight batch per
session. The default `maxBatchEntries` is 256 and `maxBatchBytes` is 4 MiB.
`maxRequestBytes` defaults to 4 MiB plus 64 KiB framing; `maxResponseBytes` defaults to
the same value, while an acknowledgement to a mutating payload MUST remain at or below
64 KiB. `maxBufferedBytes` defaults to 10 MiB so one request, one response, and 2 MiB of
codec and query headroom fit simultaneously. These are admission ceilings, not eager
allocations. Other defaults are 16 concurrent sessions, 64 MiB of durable staging per
session, a 24-hour maximum cursor age, and the filesystem's
`StorageLimits.stagingLeaseMs`, which defaults to 15 minutes. Aggregate durable staging
remains constrained by `maxStagingPayloadBytes`, so per-session allowances never
multiply past the filesystem quota.

The Computer host profile negotiates both decoded request and response limits down to 3
MiB and permits one exchange per operation, as specified in the package-boundary carrier
profile. The generic binary defaults do not apply unchanged to that text carrier.

The remaining defaults are 10,000 active or retained session rows, 64 MiB of aggregate
replication metadata, 100,000 receipts and 16 MiB of receipt records per session,
256-byte public cursors, 1 MiB terminal results, and 30-day result retention. Retry
defaults are eight attempts over at most five minutes with delays bounded from 100
milliseconds through 10 seconds. A host MAY configure lower values that can still
contain the largest required atomic protocol record.

The mandatory 100 MiB release transfer exceeds the default per-session durable-staging
allowance. Its release profile MUST therefore configure at least 128 MiB for both the
per-session and aggregate staging ceilings. Durable SQLite staging is not counted as
resident buffering; codec, query, and transport copies remain subject to the negotiated
managed-memory limits.

Before negotiation, a version 0.1 protocol envelope is limited to 64 KiB, 64 capability
entries, 256 UTF-8 bytes per identifier or format string, and 4 KiB of error text. The
transport MUST reject an oversized envelope, array, string, or byte value before fully
decoding or allocating it. A public cursor is a bounded random lookup token, not an
encoded replication inventory.

`maxBatchBytes` counts all binary payload bytes and UTF-8 bytes in application values.
`maxBatchEntries` separately bounds object descriptors, object payloads, manifest
records, revision rows, namespace rows, overlay rows, expectations, and result rows.
Fixed protocol fields MUST have documented finite maxima.

All session, cursor, receipt, digest-chain, fragment-summary, export-snapshot, and
terminal-result rows count against both `maxReplicationMetadataBytes` and the
filesystem's `maxMaintenanceBytes`. Staged content counts against
`maxStagingBytesPerSession`, `maxStagingPayloadBytes`, and `maxManagedPayloadBytes`.
Admission MUST preserve `maintenanceReserveBytes` and serialize concurrent quota checks
in the transaction that accepts rows.

The core MUST validate declared lengths before allocating, decoding, hashing, or binding
a value. It MUST reject a batch whose actual length differs from its declaration. It
MUST NOT allocate from an untrusted count before validating the count against both
effective limits.

All live replication-owned buffers for one session, including a received message, a
produced response, hash input, decoded records, and queued output, MUST fit within
`maxBufferedBytes`. A package-wide admission controller MUST also bound aggregate
session buffers. Starting work above the configured aggregate limit MUST wait with
backpressure or fail with `ResourceLimit`.

Incoming envelope storage MUST be borrowed or transferred into the incremental decoder;
decoding MUST NOT create a second complete envelope. A mutating phase MUST release its
request payload before constructing any response larger than the 64 KiB acknowledgement
bound. A nonmutating phase that may require both large request and response MUST reserve
their declared maxima plus codec headroom before accepting the request.

The package-wide controller MUST reserve those buffers from the opened filesystem's
`RuntimeLimits.maxManagedResidentBytes`. Encoded results also count against
`maxPreparedResultBytes`; query pages count against `maxQueryBatchBytes`; and incoming
or outgoing staged payload copies count against `maxPendingWriteBytes`. Replication
limits are narrower protocol limits, not an additional memory allowance. Concurrent
sessions MUST share the same aggregate reservations as filesystem streams and Node VFS
sessions.

## 6. Sessions and cursors

A session is identified by at least 128 bits of collision-resistant randomness and a
secret owner nonce. Both peers MUST persist their side of the session in their own
SQLite database. Durable state includes the global plan, peer identity, negotiated
protocol and limits, phase, cursor position, selected head or branch generation,
sequence and payload digest, cumulative result counters, retry budget, leases, and
expiry.

The source MUST acquire a durable outbound export lease. A main export lease roots the
selected revision. Exporting a mutable branch MUST first capture an immutable generation
snapshot in bounded SQLite batches and root that snapshot. Enumeration progress is a
SQLite keyset cursor. A session MUST NOT hold a SQLite read transaction open across
transport exchanges or rely on a process-local snapshot to protect export content.

A public cursor is opaque. It MUST bind to:

- the session and owner nonce;
- the source and destination filesystem identities;
- the exact global plan;
- the selected main head or branch identity and generation;
- the protocol phase and next sequence number; and
- the negotiated capability digest.

The durable session MUST also bind the caller's operation identifier, authorization
digest, and retained resume key. Opening the same operation with the same authorization
returns its active session or retained terminal result. Reusing the operation identifier
with a different plan, peer, authorization, or capability digest fails without writes.
The public API MUST let a new process select this session directly; hosts MUST NOT scan
or interpret replication tables to recover it.

A cursor MUST NOT contain the only copy of progress. A peer MUST resolve it against
durable SQLite state. A cursor presented to another session, plan, filesystem,
generation, or capability set MUST fail with `CursorMismatch`.

Session progress MUST advance in the same transaction that durably accepts a batch. A
response MUST be returned only after that transaction commits. Losing the response
therefore causes replay, not ambiguous progress.

Cursor expiry MUST atomically mark inbound staging and outbound export leases
non-rooting, but MUST NOT change visible main or branch state. Physical row cleanup MUST
then run in mandatory bounded maintenance batches. A session resumed after expiry MUST
negotiate missing content again or fail with `CursorExpired`.

## 7. Batch contract and idempotency

Every mutating batch MUST contain:

- session identifier;
- global flow and branch identity, if any;
- phase;
- monotonically increasing sequence number;
- prior cursor digest;
- entry count and payload byte count;
- canonical payload digest; and
- records for exactly one protocol phase.

The package MUST define one deterministic, length-prefixed canonical encoding for batch
digest calculation. The encoding MUST distinguish record type, integer width, null,
empty bytes, empty text, and absent optional fields. Encoding and SHA-256 calculation
MUST be incremental or use bounded codec blocks; it MUST NOT allocate a second complete
batch representation. Golden vectors MUST cover every record type before a stable
release.

Before code may persist a receipt, a normative version 1 wire document MUST freeze the
envelope magic, protocol version field, byte order, integer widths, record tags and
ordering, string normalization and UTF-8 rejection rules, optional-field representation,
length domains, digest domain separators, and unknown-field behavior. Independently
versioned Node and Durable Object builds MUST produce the same bytes for every golden
vector. A software upgrade MUST NOT turn an acknowledged version 1 batch into
`BatchReplayMismatch`.

The destination MUST record one receipt for each accepted sequence. Replaying the same
sequence and payload digest MUST return the original acknowledgement without duplicating
a row or advancing progress again. Reusing a sequence with a different digest, count,
byte length, cursor, or phase MUST fail with `BatchReplayMismatch` and MUST NOT change
state.

A batch is atomic. A limit error, integrity error, constraint failure, busy failure,
injected crash, or abort MUST leave its receipt, cursor, staging membership, and
accepted records at their previous committed values.

Receipts MUST be compacted in bounded batches after a later durable checkpoint covers
them or before a receipt quota would be exceeded. The session MUST retain a bounded
digest-chain summary sufficient to reject an old batch that could otherwise be mistaken
for new work. It MUST reject new work with `ResourceLimit` when safe compaction cannot
restore quota headroom.

The batch-acceptance transaction MUST update durable cumulative counters and this
summary:

```text
chainDigest = SHA256(previousDigest || sequence || batchDigest)
acceptedEntries += batchEntries
acceptedBytes += batchBytes
```

A terminal result MUST be stored in bounded canonical form. Retrying a lost final
response before result retention expires MUST return exactly that stored result.

## 8. Transfer phases

A session MUST use only the phases required by its plan, in this order:

1. handshake;
2. global-plan and branch selection;
3. immutable-content offer;
4. missing-content request;
5. immutable-content transfer;
6. revision, checkpoint, or branch-state transfer;
7. final validation and atomic activation;
8. result acknowledgement; and
9. bounded staging and receipt cleanup.

A peer MUST reject a phase transition that skips required validation. It MAY repeat an
earlier idempotent phase after reconnect when durable progress proves that doing so is
safe.

Only one side sends a mutating batch at a time. Request-response flow control is the
version 0.1 backpressure mechanism. A sender MUST NOT prepare the next full batch while
a prior mutating batch is unacknowledged.

## 9. Object and manifest negotiation

Immutable content negotiation MUST operate on bounded pages of descriptors. An object
descriptor contains its SHA-256 hash and byte length. A manifest-root descriptor
contains its format, SHA-256 hash, encoded length, logical file length, entry count, and
root-node hash. A manifest-node descriptor contains its hash, kind, encoded length,
logical span, and entry count.

The sender first offers descriptors. The receiver returns only missing or unverified
identities. The sender MUST NOT send bytes that were not requested, except when a
negotiated small-value optimization fits the same batch limits.

The receiver MUST verify before accepting immutable content:

- the descriptor and payload lengths;
- the SHA-256 digest;
- the object, manifest-root, or manifest-node format;
- manifest structure, child spans, ordering, and checked size arithmetic; and
- every adapter BLOB and binding capability.

An existing digest is deduplication only after its stored value is verified. A mismatch
is `IntegrityFailure`; the receiver MUST NOT overwrite either value.

Each accepted object, manifest root, or manifest node, its staging-lease membership, and
staging-certificate batch-chain update MUST commit in one bounded SQLite transaction.
The receiver negotiates missing roots and nodes in bounded graph-frontier pages and
verifies every piece independently before insertion. It MUST NOT hold the complete
manifest graph or missing-object set in memory. Final activation validates the sealed
closure certificate in constant-row work. Accepted immutable content remains invisible
to main and branch namespace state until that activation. Orphaned immutable content is
safe for later bounded garbage collection.

Negotiation MUST be storage proportional to missing immutable content. It MUST NOT copy
an object merely because its path, inode, revision, or branch changed.

## 10. Main revision replication

The source main head selection MUST occur in one SQLite read snapshot. The session MUST
bind to that selected head. Revisions committed after selection belong to a later
session or continuation and MUST NOT appear halfway through the selected transfer.

The destination MUST prove that its main head is an ancestor prefix of the selected
source head. The source MUST then send every required immutable revision header and
namespace delta in parent order. A destination MUST NOT install a revision with a
missing or different parent.

Revision identifiers and all durable conflict tokens MUST remain exact. The receiver
MUST NOT allocate replacement identifiers, resample timestamps, or recompute writer
metadata.

A revision larger than one batch MUST be staged as bounded fragments. No fragment may
update the visible head. One final short SQLite transaction MUST:

1. validate bounded durable summary rows, expected chain digest, and counts;
2. validate the expected destination head and source parent chain;
3. rely on indexed staged membership and foreign keys for verified references;
4. install revision and namespace rows within configured row and byte limits;
5. advance the destination head; and
6. mark that revision's staging lease non-rooting in constant row work.

The final transaction MUST NOT rescan or rehash payload, rebuild a digest over all
fragments, issue one statement per referenced object, or delete all staging-membership
rows. Later maintenance deletes membership rows in bounded batches.

The final transaction MUST obey the filesystem's configured transaction row and
bound-byte limits. A revision that cannot meet them MUST fail with `ResourceLimit`;
replication MUST NOT weaken atomicity.

When the destination head is older than exported revision deltas, peers MAY use a
negotiated complete checkpoint. Checkpoint rows MUST be staged in bounded transactions,
validated by count and canonical digest, and made authoritative only by a short final
transaction. Incomplete checkpoint staging is never a main or garbage-collection root
except through its staging lease.

## 11. Branch replication

The identity of a replicated branch is the tuple:

```text
(filesystemId, branchId, baseRevision, generation)
```

The source MUST select one committed branch generation in a SQLite snapshot. The
transfer MUST include the branch state, base revision, namespace overlay, file overlay,
copy-on-write pages, structural patches, expectations, and exact references to immutable
manifests and objects needed by that generation.

The copy-on-write page size MUST equal the persisted filesystem page size from the
handshake. It MUST remain separate from FastCDC configuration. Page rows MUST preserve
exact page index and logical length.

The destination MUST already contain the exact base revision or reject the import with
`BaseRevisionMissing`. It MUST apply these identity rules:

- an unused branch identifier may be reserved for the imported branch;
- the same identity and generation is an idempotent replay;
- an existing lower generation may advance only from its exact prior digest;
- an existing higher generation rejects the stale import; and
- a used identifier bound to another base or history fails with
  `BranchIdentityMismatch`.

Replication MUST NOT merge two independently mutated copies of one branch. Such copies
fail with `BranchDiverged`. A host must use separate branch identifiers or publish and
create a later branch.

A branch generation larger than one batch MUST be staged in bounded fragments. One final
SQLite transaction MUST validate bounded summaries, base revision, generation
predecessor, expectations, page size, and indexed verified membership before making the
generation visible. A new branch identifier MUST reserve from `maxPermanentIdentifiers`
in that transaction. The transaction MUST NOT rescan or rehash the complete generation.
A crash before commit leaves the prior generation unchanged.

Only an authority-to-replica flow may import terminal branch state. It MUST apply the
authority's matching active-to-terminal transition and retained publication result
atomically, close the branch to new filesystem operations, and MUST NOT resurrect it as
active. A mismatched base or generation fails before mutation. Importing an active
branch to a main authority does not publish it.

An execution replica may export only an active branch generation to the main authority.
It MUST NOT originate merged or discarded state, a publication result, or authoritative
main metadata. Terminal branch state and durable publication results originate only at
the main authority and flow from the authority to an approved replica. Result records
MUST identify their operation, branch, exact generation, outcome, and retention class;
an identity collision with different bytes is `IntegrityFailure`.

A completed branch import returns the exact activated branch identity, generation, and
generation digest. The later authority-side publication call MUST compare both expected
generation and expected generation digest in its publication transaction. If the branch
changed after import, publication fails without changing main. Import success alone is
never permission to publish a later generation.

## 12. Staging leases and cleanup

Before accepting the first immutable or mutable staged row, a destination MUST create a
durable replication staging lease. The lease MUST bind to the session, owner nonce, peer
identities, global plan, selected head or branch generation, and capability digest.

Every staged allocation and its membership MUST commit atomically. Lease renewal MUST
compare the owner nonce and prior expiry in one transaction. It MUST NOT revive an
expired or released lease.

Final activation MUST convert staged rows into visible reachable state and change the
lease to a non-rooting released state in constant row work. It MUST NOT delete every
membership row in that transaction. Cleanup after success or abort is mandatory,
idempotent, and runs in bounded maintenance batches. A process crash may retain staging
only until lease expiry.

Expired staging MUST never authorize deletion by itself. Garbage collection continues to
use the filesystem's generation-safe, high-water-mark rules. A replication session MUST
reserve enough configured staging capacity and cleanup headroom before accepting
payload.

Effective payload admission is the minimum remaining capacity across the session staging
limit, filesystem staging quota, filesystem managed-payload quota, replication metadata
quota, and capacity excluding the maintenance reserve. The accepting transaction MUST
serialize that quota decision across concurrent sessions.

## 13. Crash, retry, and cancellation

SQLite transaction recovery is authoritative after interruption. On restart:

- committed receipts and cursor progress remain replayable;
- committed immutable staging remains protected by an unexpired lease;
- an incomplete batch has no receipt or progress;
- a completed final activation is visible exactly once;
- an incomplete activation leaves the prior main or branch generation visible; and
- process-local queues and acknowledgements are discarded.

A transport error before an acknowledgement is ambiguous to the sender and MUST cause
the same sequence to be replayed. A transport error after an acknowledgement affects
only later work.

Every transport attempt MUST atomically consume the session's durable attempt and
elapsed-time budget. Durable enforcement uses a persisted wall-clock deadline and
attempt records; a monotonic clock remains the source for per-process observations.
Clock rollback MUST NOT extend a recorded deadline. Delay must remain between
`minRetryDelayMs` and `maxRetryDelayMs`. Restart MUST NOT reset either budget. Exceeding
`maxRetryAttempts` or `maxRetryElapsedMs` fails with `RetryExhausted`. Process-local
request, response, and codec buffers MUST be released between attempts; durable SQLite
session and staging state is the only retained retry state.

Transient SQLite busy failures MAY be retried using the filesystem adapter's bounded
policy. A retry MUST rerun a pure database transaction and MUST NOT duplicate an
externally visible callback or observer event.

Abort stops creating new requests. An in-flight exchange MAY finish. If its batch
commits, its durable cursor is the resume point. Abort MUST attempt bounded lease
release, but failure to release MUST NOT hide the abort result.

After `resultRetentionMs`, maintenance MUST atomically mark the terminal result and
remaining session leases non-rooting. It MUST delete terminal results, receipts,
cursors, export snapshots, and staging membership in later bounded batches while
preserving cleanup reserve.

## 14. Backpressure and memory safety

The high-level driver MUST wait for each response before submitting another mutating
batch. An endpoint MUST finish validating and committing a request before resolving its
response. A transport that internally buffers messages MUST still honor the negotiated
buffer limit.

Content hashing MUST be incremental or operate on one bounded object. Manifest,
revision, checkpoint, and branch enumeration MUST use SQLite keyset cursors and bounded
queries. OFFSET pagination and unbounded `IN` lists MUST NOT be used.

The implementation MUST NOT materialize a complete filesystem, large revision, large
branch, or complete missing-object set in memory. It MUST NOT call an adapter `all`
operation without a finite result bound derived from negotiated limits.

Slow receivers naturally stop senders through the request-response loop. A slow
destination MUST NOT cause the sender to retain an unbounded queue. A session above its
staging quota MUST stop requesting content and return `ResourceLimit` without changing
visible state.

### 14.1 Live Node VFS and FUSE activation

Replication activation and Node VFS mutation MUST enter the same core mutation
coordinator. After activation returns, a new path lookup or file open sees the activated
main revision or branch generation. Provider namespace and metadata caches MUST be
invalidated as part of the activation boundary; a caller MUST NOT need to remount to see
committed state.

An already pinned read handle keeps its selected immutable snapshot until close. A dirty
write session keeps its admitted base and may not be silently rebased or overwritten by
incoming replication. The implementation MUST serialize activation behind compatible
sessions or return `Busy`, `MainDiverged`, or `BranchDiverged` before visibility
changes. It MUST NOT report successful activation and later discard a local dirty write.

Concurrent replication, filesystem streams, and Node VFS sessions share one admission
controller and one managed-memory ceiling. Conformance MUST state which operation waits
and which fails at each resource boundary; independent per-subsystem budgets are not
allowed.

## 15. Errors

The package MUST expose a stable `ReplicationError` with at least these codes:

```ts
type ReplicationErrorCode =
  | "ProtocolMismatch"
  | "FilesystemMismatch"
  | "AuthorityMismatch"
  | "SchemaMismatch"
  | "CapabilityMismatch"
  | "IncompatibleLimit"
  | "UnauthorizedScope"
  | "ProvisioningRejected"
  | "OperationMismatch"
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

Protocol, identity, schema, capability, divergence, replay, and integrity errors are not
automatically retryable. Busy and transport errors are retryable only within the
negotiated durable retry policy. Resource failure is retryable only after capacity or
configuration changes.

An error MUST identify its phase and session when safe. It MUST NOT include file
content, lease owner nonces, cursor secrets, authentication material, or unrequested
path data.

## 16. Observability

The package MUST expose a result and optional observer events containing:

- session, global plan, and selected source head or branch generation;
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

Elapsed time MUST use a monotonic clock. Observers MUST run outside authoritative
transactions. Observer failure MUST NOT alter a result, retry, receipt, cursor, or
transaction outcome.

Physical database, WAL, and freelist bytes SHOULD be reported when the adapter can
measure them. They MUST remain distinct from logical and payload bytes.

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
14. A transport or process failure changes neither filesystem semantics nor identifier
    identity.
15. Host code is not required to implement or interpret replication protocol phases.
16. A verified immutable identity already present at the receiver contributes zero
    retransmitted payload bytes.
17. Sequential transfer never materializes a complete large file in one
    replication-owned buffer.
18. An unbound replica exposes no filesystem view and can become bound only through one
    authenticated, empty-only, atomic provisioning transaction.
19. Durable resume remains bound to its original operation, principal, authorization,
    global plan, and branch identity and cannot reset its retry budget.
20. Execution-replica main never originates a mutation; writable execution targets one
    active private branch.
21. Branch publication after replication compares the exact imported generation and
    generation digest in its authoritative transaction.
22. Computer's replication bridge and branch Node VFS share one core-owned cache,
    mutation coordinator, admission controller, and aggregate managed-memory ceiling.

## 18. Conformance suite

The shared replication testkit MUST run against Node.js SQLite and Durable Object SQLite
adapters. Adapter hooks MUST support physical reopen, abrupt restart, fault injection,
controlled corruption, small capability overrides, and a transport that can drop,
duplicate, delay, and reorder responses.

The suite MUST cover:

1. Negotiate matching capabilities and reject each persisted mismatch.
2. Treat 4, 8, and 16 KiB copy-on-write pages independently from each tested FastCDC
   configuration, and reject a page-size mismatch before staging.
3. Transfer empty, one-object, deduplicated, and multi-batch files exactly.
4. Resume every phase after dropping its request or response.
5. Replay every accepted batch and observe the original acknowledgement.
6. Reuse a sequence with changed bytes and observe `BatchReplayMismatch`.
7. Inject failure after every statement in batch and activation transactions.
8. Kill and reopen each peer before and after every durable acknowledgement.
9. Pull a linear main prefix and reject a divergent destination without writes.
10. Bootstrap from a bounded checkpoint and reject incomplete staging.
11. Push a branch with namespace changes, pages, patches, expectations, hard links,
    symlinks, and branch-only immutable content.
12. Replay the same branch generation and reject stale, reused, and divergent
    identities.
13. Expire, renew, release, and race staging leases with garbage collection.
14. Corrupt object, manifest, delta, overlay, cursor, and receipt bytes and expose
    `IntegrityFailure` without partial visibility.
15. Force one-entry and small-byte batches and prove complete progress.
16. Apply backpressure with a slow peer and assert one in-flight mutating batch.
17. Run maximum concurrent sessions and assert aggregate buffer and staging limits
    without process-memory growth proportional to total source size.
18. Transfer a 100 MiB file and assert peak replication-owned buffers remain within the
    negotiated bound.
19. Transfer a one-byte edit to a 100 MiB file and assert unchanged content objects are
    negotiated as already present; transfer one new bounded canonical version 1
    manifest.
20. Compare interrupted and uninterrupted final SQLite databases by namespace, revision,
    branch, content, cursor, receipt, and accounting state.
21. Restart the source during main and mutable-branch export; prove its selected
    snapshot, cursor, digest, and outbound lease recover from SQLite.
22. Exhaust session, receipt, cursor, metadata, staging, managed-payload, and
    maintenance quotas independently and observe bounded failure and cleanup.
23. Prove final activation uses bounded summary validation and constant-row lease
    release at the maximum revision and branch limits.
24. Exhaust retry attempts and elapsed time across restart, release every process buffer
    between attempts, and replay the retained terminal result.
25. Run replication with 64 streams, 64 Node VFS writers, maximum query pages, and
    garbage collection under one small managed-memory budget. The combined high-water
    MUST remain within that one budget.
26. Provision a genuinely empty replica from an authenticated authority descriptor,
    restart after every accepted staging batch and on both sides of atomic adoption, and
    prove exact genesis identity. Resume only the recognized durable unbound state;
    reject unrelated nonempty state, a wrong engine, wrong workspace, or conflicting
    authority without further writes.
27. Exercise every legal and illegal flow, role, and branch combination. Each durable
    operation MUST contain exactly one flow and resume only through its original
    operation ID and authorization binding.
28. Run the Computer profile through its actual Cap'n Web text carrier. Bound raw and
    decompressed frames before JSON/base64 decoding, bound the decoded envelope,
    preserve canonical semantic errors, authenticate before exchange, and report carrier
    plus replication high-water memory.
29. Derive replication and a branch-scoped Node VFS from one runtime, keep replica main
    read-only, and prove branch isolation, same-branch reconnect, and failure without
    main fallback for missing or terminal branches.
30. Activate incoming state while pinned readers and dirty writers are open. Prove the
    specified snapshot, invalidation, serialization, and conflict behavior without a
    lost update.
31. Return the exact imported branch generation and digest, publish with both as
    expectations, reject an intervening mutation, and replay a lost publication response
    without a second publication.
32. Exercise supported and unsupported combinations of logical filesystem schema,
    storage user version, protocol version, and independently deployed Computer package
    versions. Every unsupported combination MUST fail before a mutation.

Golden fixtures MUST cover the handshake capability digest, canonical batch digest,
cursor binding, one revision fragment, one checkpoint fragment, and one
branch-generation fragment.

## 19. Performance and release gates

Release benchmarks MUST use fresh isolated SQLite databases and reproducible logical
fixtures. They MUST report distributions, not one best run.

At minimum, release candidates MUST measure:

- a one-byte edit in a 100 MiB file;
- a sequential 100 MiB transfer without complete-file materialization;
- deduplicated transfer of an already present 100 MiB file;
- main catch-up across 1,000 small revisions;
- replica-to-authority branch return with 100,000 changed paths at the configured
  result-byte limit;
- resume after a dropped response in every phase; and
- bounded garbage collection after abandoned replication staging;
- end-to-end 100 MiB materialization through a Node virtual filesystem after
  replication; and
- the Computer compatibility profile through the pinned Computer fork's actual Cap'n Web
  carrier and a real mounted FUSE filesystem.

The one-byte edit MUST transfer the new root envelope, only changed manifest nodes, only
missing CAS object payloads, bounded revision metadata, and protocol overhead. It MUST
NOT retransmit unchanged object payloads or unchanged manifest subtrees. Sequential
transfer throughput and first-progress latency MUST be reported separately. The
sequential workload MUST prove that neither peer called a complete-file materialization
API or retained buffers proportional to file size.

The Node virtual filesystem and FUSE workloads are integration release gates, not
transport responsibilities. They MUST read the replicated bytes through the same path
used by execution processes and compare their SHA-256 digest with the source fixture.
The Computer profile MUST additionally report raw carrier bytes, decoded envelope bytes,
base64 expansion, transport high-water memory, replication managed high-water memory,
process RSS, SQLite and WAL growth, and live RPC stubs after disconnect. The combined
process high-water MUST remain within Computer's configured process budget. Its evidence
MUST identify exact clean Ephemeral AI FS and Ephemeral AI Computer commits and bind
every command, log, carrier setting, and result artifact to those trees.

Every benchmark MUST report p50, p95, and p99 elapsed time, peak replication-owned
buffered bytes, transferred payload, retained payload, SQLite BLOB bytes submitted,
query and transaction counts, and physical database and WAL growth where measurable.

A release MUST NOT regress p95 elapsed time, peak buffered bytes, transferred bytes, or
SQLite BLOB bytes by more than 10 percent from the checked-in accepted baseline without
an approved benchmark record explaining the tradeoff. Peak replication-owned buffered
bytes MUST never exceed the negotiated limit.

## 20. Computer integration constraint

Ephemeral AI Computer should need only to:

1. authenticate the peer and bind its workspace, filesystem, role, global plan, branch,
   host profile, protocol, and limits;
2. create one shared runtime around its selected Ephemeral AI FS instance;
3. expose `endpoint.exchange` through a bounded carrier profile; and
4. call `replicate` with one explicit plan and schedule any returned pending wake-up.

All handshake, cursor, batching, negotiation, staging, retry, and validation logic
belongs to `@ephemeralai/fs-replication`. The Computer integration MUST NOT import
replication internals or filesystem schema modules.

Engine selection, authoritative opening, replication, Node VFS forwarding, and the
branch handshake share one production integration target of no more than 100 net-new
lines in the Computer repository. Tests, generated bindings, and benchmark fixtures are
excluded. Exceeding that aggregate target is evidence that an Ephemeral AI FS package
lacks a required host-neutral abstraction and requires design review.

The Computer compatibility gate MUST execute this sequence against the pinned local
Computer fork:

1. Authenticate and provision a truly empty persistent Node SQLite replica from an
   authoritative Cloudflare-adapter filesystem, including exact genesis identity.
2. Restart both peers during provisioning, main transfer, branch transfer, activation,
   and publication replay.
3. Transfer authority main to the replica and verify its digest through real FUSE.
4. Transfer one active private branch and mount exactly that `branchId`. Prove base-main
   visibility, invisibility of its private mutations to main and siblings, invisibility
   of sibling-private mutations to it, and rejection of replica-main writes.
5. Run shell and Git operations plus hard link, symbolic link, rename, mode, truncate,
   and range-write operations through FUSE; call `fsync`, restart, and remount the same
   branch.
6. Transfer the exact active branch generation back to the authority while dropping each
   request and response in turn. Assert one activation and deterministic resume.
7. Publish with the returned generation and digest expectations, replay a lost response,
   and verify the exact authoritative main namespace and digest.
8. Transfer the authority's terminal branch state and retained publication result back
   to the replica. Reconnect after success and after a lost publication response MUST
   reject the stale branch without falling back to main.
9. Exercise incoming activation with pinned readers and dirty writers, then expire and
   collect replication leases and prove zero live sessions, reservations, and stubs.
10. Delete the local replica database after an authority-synchronized active branch,
    authenticate and provision a new empty replica, retransmit main and that branch,
    remount it, and verify exact identity and digest without another authority
    activation.
11. Reject wrong authentication, workspace, filesystem, branch, schema, protocol, host
    profile, and engine inputs before any write.

Production cutover remains a later Computer integration milestone. M8 owns the
host-neutral contract and this compatibility proof so that cutover does not discover a
missing filesystem API.

The first compatibility profile is deliberately one main authority, one persistent
execution replica, one active private branch, and a newly provisioned Ephemeral AI FS
workspace. Multi-replica fan-out and legacy DOFS migration may extend that profile
later; they MUST NOT weaken its identity, durability, branch-isolation, carrier, or
memory requirements.
