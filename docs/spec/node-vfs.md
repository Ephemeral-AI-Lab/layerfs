# Node VFS specification

| Field | Value |
| --- | --- |
| Status | Draft |
| Target | `@ephemeralai/fs-node-vfs` version 0.1 |
| Last updated | 2026-08-10 |

This document defines the Node.js virtual filesystem provider for Ephemeral
AI FS. It is normative for provider construction, synchronous range I/O,
write sessions, memory bounds, durability, metrics, and conformance.

The words MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY have the meanings stated
in the repository-level [`SPEC.md`](../../SPEC.md).

## Scope and ownership

`@ephemeralai/fs-node-vfs` MUST expose a Node-compatible provider backed by
the same Ephemeral AI FS core and schema as the asynchronous public API. The
provider MUST use the Node.js SQLite adapter. It MUST NOT implement a second
namespace, content model, branch model, or error model.

The package owns:

- synchronous Node provider operations;
- direct range reads and writes;
- open write-session state;
- sequential-write coalescing;
- per-session and provider-wide memory accounting;
- read-after-write visibility inside one provider;
- flush, synchronization, close, and abort behavior; and
- provider capabilities, observations, and metrics.

The package does not own:

- FUSE mounting or unmounting;
- FUSE flag parsing or kernel handle allocation;
- container, process, or mount-point lifecycle;
- remote procedure call transport;
- replication policy or branch publication; or
- SQLite schema, chunking, manifests, pages, revisions, or collection.

Those filesystem semantics remain in `@ephemeralai/fs`. Ephemeral AI Computer
owns FUSE and process integration. Computer MUST be able to replace its DOFS
provider with this provider without reimplementing buffering, range I/O,
SQLite batching, or filesystem semantics. The expected Computer integration
is factory selection and handle forwarding. Together with engine selection,
replication, and the branch handshake, it shares one aggregate budget of no
more than 100 net-new Computer production lines.

## Package dependencies

The package MAY depend on `@ephemeralai/fs` and
`@ephemeralai/fs-sqlite-node`. It MUST NOT depend on Ephemeral AI Computer,
DOFS, Cloudflare runtime types, a FUSE binding, or an RPC implementation.

The provider MUST invoke supported core entry points for all persisted work.
It MUST NOT issue SQL against Ephemeral AI FS tables or interpret stored
manifests, object rows, page indexes, revisions, or replication cursors.

## Public types

The following TypeScript is normative in meaning. Exported names MAY change
before the first unstable package release.

```ts
import type {
  EphemeralFS,
  FileStat,
  FilesystemError,
  RuntimeLimits,
} from "@ephemeralai/fs";
import type {
  NodeSqliteDatabaseAdapter,
} from "@ephemeralai/fs-sqlite-node";

export type CowPageBytes = 4096 | 8192 | 16384;

export interface OpenNodeVfsOptions {
  readonly database: NodeSqliteDatabaseAdapter;
  readonly branchId?: string;
  readonly runtime?: Partial<RuntimeLimits>;
  readonly observer?: NodeVfsObserver;
  readonly ownsDatabase?: boolean;
}

export interface NodeVfsCapabilities {
  readonly cowPageBytes: CowPageBytes;
  readonly runtime: Readonly<RuntimeLimits>;
  readonly preferredReadBytes: number;
  readonly supportsDirectRangeIo: true;
  readonly supportsWriteSessions: true;
  readonly supportsDataSync: boolean;
}

export interface OpenWriteOptions {
  readonly create?: boolean;
  readonly exclusive?: boolean;
  readonly truncate?: boolean;
  readonly mode?: number;
}

export interface FlushOptions {
  readonly dataOnly?: boolean;
}

export interface NodeWriteSession {
  readonly id: string;
  readonly path: string;
  readRangeSync(
    position: number,
    length: number,
  ): Uint8Array;
  writeSync(
    content: Uint8Array,
    position: number,
  ): number;
  truncateSync(size: number): void;
  statSync(): FileStat;
  flushSync(options?: FlushOptions): void;
  closeSync(): void;
  abortSync(): void;
}

export interface NodeVfsProvider {
  readonly capabilities: NodeVfsCapabilities;
  readonly metrics: NodeVfsMetrics;

  existsSync(path: string): boolean;
  statSync(path: string): FileStat;
  lstatSync(path: string): FileStat;
  readdirSync(path: string): string[];
  readlinkSync(path: string): string;
  readRangeSync(
    path: string,
    position: number,
    length: number,
  ): Uint8Array;

  openWriteSync(
    path: string,
    options?: OpenWriteOptions,
  ): NodeWriteSession;

  mkdirSync(
    path: string,
    options?: { recursive?: boolean; mode?: number },
  ): void;
  chmodSync(path: string, mode: number): void;
  linkSync(existingPath: string, newPath: string): void;
  symlinkSync(target: string, path: string): void;
  renameSync(oldPath: string, newPath: string): void;
  unlinkSync(path: string): void;
  rmdirSync(path: string): void;

  syncSync(): void;
  closeSync(): void;
}

export interface NodeVfsHandle {
  readonly filesystem: EphemeralFS;
  readonly provider: NodeVfsProvider;
  close(): Promise<void>;
}

export declare function openNodeVfs(
  options: OpenNodeVfsOptions,
): Promise<NodeVfsHandle>;
```

`NodeSqliteDatabaseAdapter` is the concrete Node adapter handle, not a raw
SQLite connection. The package MAY use an internal synchronous core port
carried by that adapter. That port MUST execute the same validation,
transactions, and mutations as `EphemeralFS`; it MUST NOT be public SQL access.

The public provider MAY expose additional Node compatibility methods such as
bounded `readFileSync` and `writeFileSync`. Such methods MUST delegate to the
operations above and MUST preserve the portable filesystem contract.

## Opening and lifecycle

`openNodeVfs` MUST open or validate the core filesystem before returning. The
returned `filesystem` and `provider` MUST address the same database,
filesystem identity, branch view, limits, and persisted format settings.

Opening MUST fail if the database is not a compatible Ephemeral AI FS
database. The package MUST NOT initialize, migrate, or open a DOFS database.
It MUST NOT fall back to another engine.

`NodeVfsHandle.close()` MUST stop admitting new provider operations, close all
provider sessions according to the close rules below, close the filesystem,
and close the adapter when `ownsDatabase` is true. It MUST be idempotent.

`NodeVfsProvider.closeSync()` MUST fail with `EBUSY` while any write session
has dirty data. A host MUST flush, close, or abort those sessions first. Once
provider close succeeds, later provider calls MUST fail with `EBADF`.

## Path and error behavior

Provider paths MUST use the canonical absolute POSIX rules in the filesystem
API specification. The provider MUST return the same metadata and apply the
same link, rename, and directory semantics as `EphemeralFilesystem`.

Synchronous provider operations MUST throw `FilesystemError` with the same
stable error code and precedence as the portable operation. Node-specific
wrappers MAY add `path`, `dest`, and `syscall` properties. They MUST NOT
replace the stable filesystem code with message parsing or host errno values.

The provider MUST validate numeric positions, lengths, modes, and limits
before allocation. Arithmetic overflow MUST fail with `EINVAL` or the more
specific portable resource error before visible state changes.

## Direct range reads

`readRangeSync` MUST read only the requested range. It MUST NOT materialize
the complete file, even when the file is already cached by SQLite or the
operating system. Its EOF, zero-length, type-checking, and error behavior MUST
match `EphemeralFilesystem.readRange`.

The returned value MUST be a new `Uint8Array` and MUST NOT alias SQLite,
manifest, object-cache, write-session, or caller-owned mutable memory. The
requested length MUST be bounded by the core materialization limit.

The implementation SHOULD fetch and verify only manifest entries and objects
needed for the requested range. The provider MUST NOT own a second content or
verification cache; it uses the core's shared byte-bounded caches.

Reads through a write session MUST merge persisted content and admitted dirty
state without copying the whole file. A read that intersects no dirty range
MUST use the direct persisted range path.

## Write-session model

`openWriteSync` creates one provider handle. It does not create a public core
file descriptor. FUSE or another host maps its own handle and flags to
`OpenWriteOptions` and retains ownership of that mapping.

`create`, `exclusive`, `truncate`, and `mode` MUST have the same observable
meaning as the corresponding portable filesystem options. The provider MAY
stage a new inode until its first flush, but every operation through the same
provider MUST observe that pending inode. A different process or filesystem
instance is not required to observe unflushed state.

Several sessions MAY open the same path. Their admitted writes MUST have one
deterministic provider order. A later session MUST observe writes admitted by
an earlier session through the same provider. Rename and unlink MUST either
update all affected open sessions atomically or fail with `EBUSY`; they MUST
not orphan dirty data under an unreachable path.

The provider MUST NOT allocate a buffer proportional to file size. A session
MAY retain dirty ranges, a sequential buffer, chunker state, and bounded
metadata. It MUST use core range-write, streamed-write, or staging operations
to release resident bytes.

## Sequential-write coalescing

A write is sequential when its position is the logical end of the currently
buffered sequential range for that session. The provider SHOULD coalesce
contiguous sequential writes before invoking content chunking and SQLite
metadata updates.

The default runtime `maxWriteSessionBytes` is 16 MiB. A sequential buffer MUST
never grow beyond the effective session limit. When admitting another write
would cross the limit, the provider MUST stage or commit a bounded prefix
through the core before accepting the new bytes.

The core owns FastCDC state, object hashing, manifest construction, leases,
and final namespace transactions. The provider MUST NOT split a stream by
inventing fixed content boundaries at buffer flushes. A core streaming writer
MAY carry FastCDC state across several provider-buffer flushes.

A discontinuous write MAY flush the sequential buffer and use a core range
write. Small overwrites MUST remain eligible for the core copy-on-write page
path. The provider MUST NOT turn a small random overwrite into a whole-file
rewrite or a full FastCDC scan.

Coalescing MUST preserve call order. `writeSync` MUST return the admitted byte
count only after the input bytes are copied, committed, or durably staged. A
later mutation of the caller's array MUST NOT change admitted data.

## Copy-on-write page configuration

The core filesystem supports copy-on-write page sizes of 4 KiB, 8 KiB, and
16 KiB. Version 0.1 defaults to 8 KiB. The chosen value is a persisted,
creation-time filesystem format setting and is independent of FastCDC minimum,
average, and maximum chunk sizes.

The provider MUST read `cowPageBytes` from immutable core capabilities. It
MUST NOT accept a separate page-size option, infer the value from writes, or
reinterpret page indexes. Reopening with a conflicting format request is a
core schema error.

Changing the page size requires a core-defined migration that materializes or
rewrites existing overlays. The Node VFS package MUST NOT perform that
migration. Benchmarks MUST report the selected page size.

## Provider-wide memory bound

The default runtime `maxPendingWriteBytes` is 64 MiB. It includes all capacity
owned by active sequential buffers, dirty-range payloads, pending creates, and
retryable failed flushes across the provider. It is not a per-file limit, and
it remains subject to the aggregate `maxManagedResidentBytes` limit.

Both effective memory limits MUST be positive safe integers. The pending-write
limit MUST be at least the session limit. Invalid options MUST fail before the
provider opens. Implementations MAY choose smaller defaults for a constrained
runtime but MUST report the effective values through capabilities.

The provider and the core instance it opens MUST use one admission controller.
Provider buffers, core hashing and rechunking windows, query results,
replication buffers, and core caches MUST NOT each enforce independent
aggregate allowances over the same `maxManagedResidentBytes` configuration.

Before a write would cross the pending-write or aggregate limit, the provider
MUST synchronously flush one or more eligible sessions. It SHOULD prefer the
largest or oldest dirty session while preserving ordering. If forced flush
fails, the provider MUST surface that underlying error. If transient pressure
cannot be relieved synchronously, it MUST fail with `EAGAIN`. If one operation
cannot fit an otherwise empty configured budget, it MUST fail with `EFBIG`.
Memory pressure alone MUST NOT produce `ENOSPC` or SQLite-contention `EBUSY`.

This synchronous pressure response is the provider's backpressure mechanism.
A future asynchronous provider MAY wait for capacity, but it MUST preserve the
same admission rule. Computer MUST NOT add another whole-file buffer above the
provider.

Metadata collections MUST also be bounded. The provider MUST cap open session
count with `RuntimeLimits.maxOpenNodeVfsSessions` and cap dirty-range count and
retained path metadata within the managed resident-memory limit. Opening a new
session beyond a count or byte cap MUST fail with `EAGAIN` without changing the
filesystem. Close or abort MUST release the session slot exactly once.

## Read-after-write behavior

Once `writeSync` succeeds, subsequent reads and stats through the same session
MUST observe the admitted bytes and resulting logical size. Reads and stats
through another handle from the same provider MUST also observe them.

Read-after-write visibility does not imply durability. Other filesystem
instances, replication peers, and Computer's authoritative side observe the
change only after the relevant core transaction or staging checkpoint makes
it visible to them.

An overlapping later write wins according to provider admission order. A
truncate MUST immediately affect session reads and stats. Growing a file MUST
read as zero-filled through the core sparse-file semantics.

## SQLite batching and atomicity

The provider MUST send prepared mutations to a supported core batching API.
The core remains responsible for SQL generation, binding limits, object
verification, leases, atomic namespace changes, and transaction retries.

One sequential buffer flush SHOULD hash and stage its objects in bounded work
and use one SQLite metadata transaction when adapter limits permit. It MUST
not execute one metadata transaction per incoming FUSE write merely because
the host divided a sequential stream into smaller buffers.

When a batch exceeds SQLite binding, BLOB, transaction, or configured limits,
the core MAY stage immutable objects in several bounded transactions under a
durable lease. The final visible namespace or file-value change MUST retain
the atomicity defined by the filesystem API.

The provider MUST NOT report bytes as flushed if the core has only retained
them in process memory. Durable staging protected by a core lease MAY count as
flushed only when the session can resume or fail safely after process restart
according to the core streaming-write contract.

## Flush, synchronization, and close

`flushSync` MUST attempt to make every write admitted by that session durable
through the core. It MUST preserve read-after-write state while it runs.

`flushSync({ dataOnly: true })` MAY omit a separate host-specific metadata
sync only when `supportsDataSync` is true. It MUST still persist content and
all SQLite metadata required to recover that content. When the distinction is
unsupported, data-only flush MUST have full flush behavior.

`NodeVfsProvider.syncSync()` MUST flush every dirty provider session in
deterministic order. One session failure MUST stop the operation and surface
that error. Sessions already committed remain committed; other dirty sessions
remain retryable. The method MUST NOT claim cross-file transaction atomicity.

`closeSync` on a write session MUST perform a full flush, then release session
state. Successful close is idempotent. Later operations on the session MUST
fail with `EBADF`.

`abortSync` MUST discard uncommitted session state and release its resident
memory. It MUST NOT roll back data from a previously successful flush. Abort
is idempotent and later session operations MUST fail with `EBADF`.

## Failure behavior

A failed `writeSync` MUST admit either all input bytes or none. Previously
admitted dirty bytes remain readable and retryable. The provider MUST NOT
retain an unaccounted caller buffer after failure.

A failed `flushSync` or provider `syncSync` MUST:

- preserve every uncommitted dirty byte in accounted session state;
- preserve read-after-write behavior;
- leave the session open and retryable;
- expose the stable core or adapter error; and
- avoid advancing a durability metric for the failed work.

A failed session `closeSync` MUST have the same state as a failed full flush.
It MUST leave the session open so a host can retry close or call `abortSync`.
The host MUST surface the close failure; it MUST NOT silently abort.

When a FUSE `fsync` operation is mapped to `flushSync`, Computer MUST return
the mapped failure to the kernel. FUSE release policy remains Computer-owned,
but a failed release MUST NOT be reported as success while the provider still
owns uncommitted data.

If SQLite reports an ambiguous commit outcome, the core MUST resolve it using
its idempotency and recovery rules before the provider reports success or a
retryable failure. The provider MUST NOT guess from an exception message.

## Capabilities and metrics

Capabilities MUST be immutable for the provider lifetime. They MUST reflect
the persisted filesystem format and effective runtime budgets, not requested
values that were reduced or rejected during open.

```ts
export interface NodeVfsMetricsSnapshot {
  readonly openSessions: number;
  readonly dirtySessions: number;
  readonly residentWriteBytes: number;
  readonly peakResidentWriteBytes: number;
  readonly residentControlBytes: number;
  readonly peakManagedResidentBytes: number;
  readonly stagedLogicalBytes: number;
  readonly admittedWriteBytes: number;
  readonly flushedWriteBytes: number;
  readonly flushCount: number;
  readonly forcedFlushCount: number;
  readonly failedFlushCount: number;
  readonly rejectedWriteCount: number;
  readonly directReadBytes: number;
  readonly coreBatchCount: number;
}

export interface NodeVfsMetrics {
  snapshot(): NodeVfsMetricsSnapshot;
}

export type NodeVfsObservation =
  | { readonly kind: "session-open"; readonly sessionId: string }
  | { readonly kind: "session-close"; readonly sessionId: string }
  | { readonly kind: "forced-flush"; readonly bytes: number }
  | { readonly kind: "flush-failed"; readonly code: string }
  | { readonly kind: "memory-rejected"; readonly bytes: number };

export type NodeVfsObserver = (
  event: NodeVfsObservation,
) => void;
```

`residentWriteBytes` MUST measure allocated resident capacity, not only logical
dirty length. It MUST never exceed runtime `maxPendingWriteBytes` after a
public operation returns. `peakResidentWriteBytes` MUST use the same
definition.

`flushedWriteBytes` counts logical bytes made durable, even when CAS reuse
stores no new payload. Core storage accounting remains the authority for
physical SQLite, object, manifest, page, and reclaimable bytes.

The observer is optional and synchronous. It MUST NOT receive file content.
Observer failure MUST NOT change provider behavior and SHOULD be reported
through the core observation-error policy.

## Computer integration contract

Computer SHOULD perform only these Node-side steps:

1. open the selected engine and database;
2. obtain the engine's Node provider;
3. map Computer-owned FUSE handles and flags to provider sessions;
4. forward range, namespace, flush, and close operations;
5. expose provider metrics; and
6. close the provider during execution-backend shutdown.

Computer MUST NOT inspect Ephemeral AI FS tables, manifests, chunks, pages,
or write-session buffers. It MUST NOT duplicate the provider's memory cache.
The DOFS comparison engine may use its own provider behind the same
Computer-owned forwarding boundary.

## Conformance requirements

`@ephemeralai/fs-testkit` MUST provide a Node VFS suite. The suite MUST run
against a real file-backed SQLite database and MUST cover at least:

1. direct range reads from the start, middle, end, and beyond EOF;
2. proof that a range read does not materialize the complete file;
3. sequential writes split into irregular host buffer sizes;
4. discontinuous and overlapping writes;
5. read-after-write through the same and a second provider handle;
6. truncate growth, shrink, and zero-filled gaps;
7. pending create, exclusive create, rename, unlink, and hard links;
8. flush, data-only flush, provider sync, close, retry, and abort;
9. injected SQLite failure before, during, and after a core batch;
10. process restart after successful flush and after unflushed writes;
11. session, pending-write, and aggregate resident-memory limits with several
    concurrent writers;
12. forced flush at the exact session and pending-write boundaries;
13. immutable capabilities and exact memory metrics;
14. all 4 KiB, 8 KiB, and 16 KiB copy-on-write page formats; and
15. no page-size interpretation or FastCDC implementation in this package.

Fault tests MUST prove that a failed flush remains readable and retryable,
that abort releases its entire accounted capacity, and that resident bytes
never cross the configured aggregate limit.

## Benchmark and release gates

Benchmarks MUST use fresh databases and report the Node.js version, SQLite
adapter, page size, FastCDC profile, logical bytes, physical SQLite growth,
transaction count, elapsed-time distribution, and peak resident bytes.

The version 0.1 release suite MUST include these workloads:

- one one-byte overwrite in a 100 MiB file at each supported page size;
- 1,000 small overwrites in a 100 MiB file;
- a bounded-range-loop 100 MiB sequential read;
- one 100 MiB sequential FUSE-style materialization;
- 100 files of 1 MiB each; and
- interleaved writes across 1, 16, and 64 sessions under the default 128 MiB
  aggregate and 64 MiB pending-write budgets.

The small-edit workloads MUST NOT read, hash, or materialize the complete
file. Their retained branch payload MUST remain proportional to the persisted
copy-on-write page size. Repeated writes to one page MUST update bounded
page-local state rather than append one full page per call.

The sequential read MUST use direct bounded range I/O and remain within the
aggregate resident-memory budget. Core `readStream` behavior belongs to the
portable filesystem suite, not this provider suite.

The 100 MiB materialization MUST use sequential coalescing and stay within the
16 MiB session, 64 MiB pending-write, and 128 MiB aggregate defaults. It MUST
NOT retain a 100 MiB process-local file buffer.

Numeric performance thresholds and trial methodology are owned by
[`performance-and-resource-limits.md`](./performance-and-resource-limits.md).
Storage tests MUST include incompressible, partially repeated, and fully
repeated content so CAS gains are not inferred from one repetitive fixture.

## Release condition

`@ephemeralai/fs-node-vfs` is ready for Computer integration only when:

- every conformance and fault-injection case passes;
- direct and session reads never require whole-file materialization;
- all dirty memory is included in the pending-write class and one enforced
  aggregate resident budget;
- SQLite work passes through supported core batching APIs;
- page size is reported from, and interpreted only by, the core;
- the benchmark gates pass against the retained DOFS control; and
- Computer can select and open the provider without filesystem logic of its
  own.
