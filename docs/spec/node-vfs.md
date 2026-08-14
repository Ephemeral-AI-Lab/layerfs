# Node VFS specification

| Field        | Value                                  |
| ------------ | -------------------------------------- |
| Status       | Draft                                  |
| Target       | `@ephemeralai/fs-node-vfs` version 0.1 |
| Last updated | 2026-08-10                             |

This document defines the Node.js virtual filesystem provider for Ephemeral AI FS. It is
normative for provider construction, synchronous range I/O, file sessions, memory
bounds, durability, metrics, and conformance.

The words MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY have the meanings stated in the
repository-level [`SPEC.md`](../../SPEC.md).

## Scope and ownership

`@ephemeralai/fs-node-vfs` MUST expose a Node-compatible provider backed by the same
Ephemeral AI FS core and schema as the asynchronous public API. The provider MUST use
the Node.js SQLite adapter. It MUST NOT implement a second namespace, content model,
branch model, or error model.

The package owns:

- synchronous Node provider operations;
- direct range reads and writes;
- open read- and write-session state;
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

Those filesystem semantics remain in `@ephemeralai/fs`. Ephemeral AI Computer owns FUSE
and process integration. Computer MUST be able to replace its DOFS provider with this
provider without reimplementing buffering, range I/O, SQLite batching, or filesystem
semantics. The expected Computer integration is factory selection and handle forwarding.
Together with engine selection, replication, and the branch handshake, it shares one
aggregate budget of no more than 100 net-new Computer production lines.

## Package dependencies

The package MAY depend on `@ephemeralai/fs` and `@ephemeralai/fs-sqlite-node`. It MUST
NOT depend on Ephemeral AI Computer, DOFS, Cloudflare runtime types, a FUSE binding, or
an RPC implementation.

The provider MUST invoke supported core entry points for all persisted work. It MUST NOT
issue SQL against Ephemeral AI FS tables or interpret stored manifests, object rows,
page indexes, revisions, or replication cursors.

## Public types

The following TypeScript is normative in meaning. Exported names MAY change before the
first unstable package release.

```ts
import type {
  EphemeralFS,
  FileStat,
  FilesystemError,
  RuntimeLimits,
} from "@ephemeralai/fs";
import type { NodeSQLiteDriver } from "@ephemeralai/fs-sqlite-node";

export type CowPageBytes = 4096 | 8192 | 16384;

export interface OpenNodeVfsOptions {
  readonly database: NodeSQLiteDriver;
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

export interface OpenFileOptions {
  readonly writable?: boolean;
  readonly create?: boolean;
  readonly exclusive?: boolean;
  readonly truncate?: boolean;
  readonly mode?: number;
}

export interface FlushOptions {
  readonly dataOnly?: boolean;
}

export interface NodeFileSession {
  readonly id: string;
  readonly path: string;
  readonly writable: boolean;
  readIntoSync(
    destination: Uint8Array,
    destinationOffset: number,
    position: number,
    length: number,
  ): number;
  readRangeSync(position: number, length: number): Uint8Array;
  writeSync(content: Uint8Array, position: number): number;
  truncateSync(size: number): void;
  statSync(): FileStat;
  /** Persist a bounded hidden prefix without satisfying fsync. */
  stagePrefixSync(): void;
  /** Atomically install all admitted bytes and satisfy durability. */
  commitVisibleSync(options?: FlushOptions): void;
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
  readRangeSync(path: string, position: number, length: number): Uint8Array;

  openFileSync(path: string, options?: OpenFileOptions): NodeFileSession;

  mkdirSync(path: string, options?: { recursive?: boolean; mode?: number }): void;
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

`NodeSQLiteDriver` is the concrete Node SQLite driver handle, not a raw connection. The
provider uses the supported core Node VFS integration bridge. The core creates that
semantic bridge; the SQLite driver only supplies callback-scoped transactions. The
bridge MUST execute the same validation, admission, transactions, and mutations as
`EphemeralFS` and MUST NOT expose SQL, schema, repositories, CAS insertion, or COW
mutation.

The public provider MAY expose additional Node compatibility methods such as bounded
`readFileSync` and `writeFileSync`. Such methods MUST delegate to the operations above
and MUST preserve the portable filesystem contract.

## Opening and lifecycle

`openNodeVfs` MUST open or validate the core filesystem before returning. The returned
`filesystem` and `provider` MUST address the same database, filesystem identity, branch
view, limits, and persisted format settings.

When `branchId` is present, the provider MUST bind every namespace, metadata, range,
write-session, flush, and sync operation to exactly that active private branch. A
missing branch MUST fail opening with `ENOENT`; a terminal branch MUST fail with
`EROFS`. Neither case may fall back to main. Reopening after process restart MUST bind
the same branch or fail.

An execution replica's main view is read-only. A writable open or namespace mutation on
replica main MUST fail with `EROFS` before creating provider-visible pending state. A
writable execution provider MUST therefore select an active private branch.

The M8 composition API MUST also allow this provider to be derived from the same
core-owned runtime as replication and the portable filesystem. That derived provider
MUST share the runtime's cache, mutation coordinator, and aggregate admission
controller. Opening an independent core instance over the same SQLite database for FUSE
and replication is not a supported Computer configuration.

Opening MUST fail if the database is not a compatible Ephemeral AI FS database. The
package MUST NOT initialize, migrate, or open a DOFS database. It MUST NOT fall back to
another engine.

`NodeVfsHandle.close()` MUST stop admitting new provider operations, close all provider
sessions according to the close rules below, close the filesystem, and close the adapter
when `ownsDatabase` is true. It MUST be idempotent.

`NodeVfsProvider.closeSync()` MUST fail with `EBUSY` while any file session has dirty
data. A host MUST commit, close, or abort those sessions first. Once provider close
succeeds, later provider calls MUST fail with `EBADF`.

## Path and error behavior

Provider paths MUST use the canonical absolute POSIX rules in the filesystem API
specification. The provider MUST return the same metadata and apply the same link,
rename, and directory semantics as `EphemeralFilesystem`.

Synchronous provider operations MUST throw `FilesystemError` with the same stable error
code and precedence as the portable operation. Node-specific wrappers MAY add `path`,
`dest`, and `syscall` properties. They MUST NOT replace the stable filesystem code with
message parsing or host errno values.

The provider MUST validate numeric positions, lengths, modes, and limits before
allocation. Arithmetic overflow MUST fail with `EINVAL` or the more specific portable
resource error before visible state changes.

## Direct range reads

`openFileSync` with `writable` absent or false creates a pinned read session. It
captures a stable inode identity, selected revision or branch generation, durable lease,
and bounded manifest cursor. Repeated FUSE reads through that handle MUST reuse this
selection rather than repeat path resolution or manifest root setup for every callback.
Close releases the cursor, lease, reservations, and session slot exactly once.

`readIntoSync` MUST validate destination bounds before reading and copy exact bytes
directly into the caller-provided destination. It returns the number of bytes read and
MUST NOT allocate an equal-sized intermediate array. Its snapshot, EOF, type, and error
behavior matches `readRange`. A caller that needs an owned result may use
`readRangeSync`, which is a bounded convenience implemented over `readIntoSync`.

`readRangeSync` MUST read only the requested range. It MUST NOT materialize the complete
file, even when the file is already cached by SQLite or the operating system. Its EOF,
zero-length, type-checking, and error behavior MUST match
`EphemeralFilesystem.readRange`.

The returned value MUST be a new `Uint8Array` and MUST NOT alias SQLite, manifest,
object-cache, write-session, or caller-owned mutable memory. The requested length MUST
be bounded by the core materialization limit.

The implementation SHOULD fetch and verify only manifest entries and objects needed for
the requested range. The provider MUST NOT own a second content or verification cache;
it uses the core's shared byte-bounded caches.

Reads through a write session MUST merge persisted content and admitted dirty state
without copying the whole file. A read that intersects no dirty range MUST use the
direct persisted range path.

## Write-session model

`openFileSync({ writable: true })` creates one provider handle. It does not create a
public core file descriptor. FUSE or another host maps its own handle and flags to
`OpenFileOptions` and retains ownership of that mapping.

Calling `writeSync`, `truncateSync`, `stagePrefixSync`, `commitVisibleSync`, or
`flushSync` on a session whose `writable` value is false MUST fail with `EBADF` before
changing session or durable state.

`create`, `exclusive`, `truncate`, and `mode` MUST have the same observable meaning as
the corresponding portable filesystem options. The provider MAY stage a new inode until
its first flush, but every operation through the same provider MUST observe that pending
inode. A different process or filesystem instance is not required to observe unflushed
state.

Several sessions MAY open the same inode. A provider-wide per-inode coordinator MUST
assign one monotonic admission sequence to every write and truncate. All provider
sessions and path reads observe the admitted sequence in order. A session commit MUST
include or wait for every earlier same-inode admission; it MUST NOT install bytes
prepared from a stale base after a later sequence has committed. Later dirty state MUST
rebase on a committed predecessor or fail before any visible mutation.

Rename and unlink MUST coordinate by inode identity. They MUST either update all
affected open sessions atomically or fail with `EBUSY`; they MUST not orphan dirty data
under an unreachable path. The coordinator's metadata and dirty views participate in the
shared resident-memory and session-count limits.

The provider MUST NOT allocate a buffer proportional to file size. A session MAY retain
dirty ranges, a sequential buffer, chunker state, and bounded metadata. It MUST use core
range-write, streamed-write, or staging operations to release resident bytes.

## Sequential-write coalescing

A write is sequential when its position is the logical end of the currently buffered
sequential range for that session. The provider SHOULD coalesce contiguous sequential
writes before invoking content chunking and SQLite metadata updates.

The default runtime `maxWriteSessionBytes` is 16 MiB. A sequential buffer MUST never
grow beyond the effective session limit. When admitting another write would cross the
limit, the provider MUST call `stagePrefixSync` for a bounded prefix through the core
before accepting the new bytes. Hidden staging releases resident memory but does not
change the visible file value and does not satisfy flush or synchronization.

The core owns FastCDC state, object hashing, manifest construction, leases, and final
namespace transactions. The provider MUST NOT split a stream by inventing fixed content
boundaries at buffer flushes. A core streaming writer MAY carry FastCDC state across
several provider-buffer flushes.

A discontinuous write MAY flush the sequential buffer and use a core range write. Small
overwrites MUST remain eligible for the core copy-on-write page path. The provider MUST
NOT turn a small random overwrite into a whole-file rewrite or a full FastCDC scan.

Coalescing MUST preserve call order. `writeSync` MUST return the admitted byte count
only after the input bytes are copied, committed, or durably staged. A later mutation of
the caller's array MUST NOT change admitted data.

The provider SHOULD obtain bounded pooled slabs from the core's shared admission
controller. It MAY transfer ownership of an admitted slab to the core streaming writer
so FastCDC and CAS hashing consume it without another complete copy. Ownership transfer
must be explicit: exactly one layer releases the slab, and cancellation, staging
failure, retry exhaustion, and close release it once.

## Copy-on-write page configuration

The core filesystem supports copy-on-write page sizes of 4 KiB, 8 KiB, and 16 KiB.
Version 0.1 defaults to 8 KiB. The chosen value is a persisted, creation-time filesystem
format setting and is independent of FastCDC minimum, average, and maximum chunk sizes.

The provider MUST read `cowPageBytes` from immutable core capabilities. It MUST NOT
accept a separate page-size option, infer the value from writes, or reinterpret page
indexes. Reopening with a conflicting format request is a core schema error.

Changing the page size requires a core-defined migration that materializes or rewrites
existing overlays. The Node VFS package MUST NOT perform that migration. Benchmarks MUST
report the selected page size.

## Provider-wide memory bound

The default runtime `maxPendingWriteBytes` is 64 MiB. It includes all capacity owned by
active sequential buffers, dirty-range payloads, pending creates, and retryable failed
flushes across the provider. It is not a per-file limit, and it remains subject to the
aggregate `maxManagedResidentBytes` limit.

Both effective memory limits MUST be positive safe integers. The pending-write limit
MUST be at least the session limit. Invalid options MUST fail before the provider opens.
Implementations MAY choose smaller defaults for a constrained runtime but MUST report
the effective values through capabilities.

The provider and the core instance it opens MUST use one admission controller. Provider
buffers, core hashing and rechunking windows, query results, replication buffers, and
core caches MUST NOT each enforce independent aggregate allowances over the same
`maxManagedResidentBytes` configuration.

Before a write would cross the pending-write or aggregate limit, the provider MUST
synchronously flush one or more eligible sessions. It SHOULD prefer the largest or
oldest dirty session while preserving ordering. If forced flush fails, the provider MUST
surface that underlying error. If transient pressure cannot be relieved synchronously,
it MUST fail with `EAGAIN`. If one operation cannot fit an otherwise empty configured
budget, it MUST fail with `EFBIG`. Memory pressure alone MUST NOT produce `ENOSPC` or
SQLite-contention `EBUSY`.

This synchronous pressure response is the provider's backpressure mechanism. A future
asynchronous provider MAY wait for capacity, but it MUST preserve the same admission
rule. Computer MUST NOT add another whole-file buffer above the provider.

Metadata collections MUST also be bounded. The provider MUST cap open session count with
`RuntimeLimits.maxOpenNodeVfsSessions` and cap dirty-range count and retained path
metadata within the managed resident-memory limit. Opening a new session beyond a count
or byte cap MUST fail with `EAGAIN` without changing the filesystem. Close or abort MUST
release the session slot exactly once.

## Read-after-write behavior

Once `writeSync` succeeds, subsequent reads and stats through the same session MUST
observe the admitted bytes and resulting logical size. Reads and stats through another
handle from the same provider MUST also observe them.

Read-after-write visibility does not imply durability. Other filesystem instances,
replication peers, and Computer's authoritative side observe the change only after the
relevant core transaction or staging checkpoint makes it visible to them.

An overlapping later write wins according to provider admission order. A truncate MUST
immediately affect session reads and stats. Growing a file MUST read as zero-filled
through the core sparse-file semantics.

## SQLite batching and atomicity

The provider MUST send prepared mutations to a supported core batching API. The core
remains responsible for SQL generation, binding limits, object verification, leases,
atomic namespace changes, and transaction retries.

One sequential buffer flush SHOULD hash and stage its objects in bounded work and use
one SQLite metadata transaction when adapter limits permit. It MUST not execute one
metadata transaction per incoming FUSE write merely because the host divided a
sequential stream into smaller buffers.

When a batch exceeds SQLite binding, BLOB, transaction, or configured limits, the core
MAY stage immutable objects in several bounded transactions under a durable lease. The
final visible namespace or file-value change MUST retain the atomicity defined by the
filesystem API.

The provider MUST NOT report bytes as flushed if the core has only retained them in
process memory or hidden staging. Durable staging protected by a core lease may release
resident capacity and support restart recovery, but only `commitVisibleSync` installs a
visible file value and satisfies flush or fsync.

## Flush, synchronization, and close

`stagePrefixSync` MUST durably attach its bounded prepared prefix to the session's
staging lease and preserve resumable state. It MUST NOT advance the visible inode,
revision, or durability-complete metric.

`commitVisibleSync` MUST make every write admitted through the provider up to the
session's required per-inode sequence durable and atomically visible. It MUST preserve
read-after-write state while it runs. `flushSync` is the Node compatibility name for the
same operation and MUST delegate exactly to `commitVisibleSync`; it is not a staging
operation.

`flushSync({ dataOnly: true })` MAY omit a separate host-specific metadata sync only
when `supportsDataSync` is true. It MUST still persist content and all SQLite metadata
required to recover that content. When the distinction is unsupported, data-only flush
MUST have full flush behavior.

`NodeVfsProvider.syncSync()` MUST flush every dirty provider session in deterministic
order. One session failure MUST stop the operation and surface that error. Sessions
already committed remain committed; other dirty sessions remain retryable. The method
MUST NOT claim cross-file transaction atomicity.

`closeSync` on a write session MUST perform a full flush, then release session state.
Successful close is idempotent. Later operations on the session MUST fail with `EBADF`.

`abortSync` MUST discard uncommitted session state and release its resident memory. It
MUST NOT roll back data from a previously successful flush. Abort is idempotent and
later session operations MUST fail with `EBADF`.

## Failure behavior

A failed `writeSync` MUST admit either all input bytes or none. Previously admitted
dirty bytes remain readable and retryable. The provider MUST NOT retain an unaccounted
caller buffer after failure.

A failed `flushSync` or provider `syncSync` MUST:

- preserve every uncommitted dirty byte in accounted session state;
- preserve read-after-write behavior;
- leave the session open and retryable;
- expose the stable core or adapter error; and
- avoid advancing a durability metric for the failed work.

A failed session `closeSync` MUST have the same state as a failed full flush. It MUST
leave the session open so a host can retry close or call `abortSync`. The host MUST
surface the close failure; it MUST NOT silently abort.

When a FUSE `fsync` operation is mapped to `commitVisibleSync`, Computer MUST return the
mapped failure to the kernel. FUSE release policy remains Computer-owned, but a failed
release MUST NOT be reported as success while the provider still owns uncommitted data.

If SQLite reports an ambiguous commit outcome, the core MUST resolve it using its
idempotency and recovery rules before the provider reports success or a retryable
failure. The provider MUST NOT guess from an exception message.

## Capabilities and metrics

Capabilities MUST be immutable for the provider lifetime. They MUST reflect the
persisted filesystem format and effective runtime budgets, not requested values that
were reduced or rejected during open.

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

export type NodeVfsObserver = (event: NodeVfsObservation) => void;
```

`residentWriteBytes` MUST measure allocated resident capacity, not only logical dirty
length. It MUST never exceed runtime `maxPendingWriteBytes` after a public operation
returns. `peakResidentWriteBytes` MUST use the same definition.

`flushedWriteBytes` counts logical bytes made durable, even when CAS reuse stores no new
payload. Core storage accounting remains the authority for physical SQLite, object,
manifest, page, and reclaimable bytes.

The observer is optional and synchronous. It MUST NOT receive file content. Observer
failure MUST NOT change provider behavior and SHOULD be reported through the core
observation-error policy.

## Computer integration contract

Computer SHOULD perform only these Node-side steps:

1. open the selected engine's one shared filesystem runtime;
2. obtain the provider for the exact active execution `branchId`;
3. map Computer-owned FUSE handles and flags to provider sessions;
4. forward range, namespace, flush, and close operations;
5. expose provider metrics; and
6. close the provider and runtime during execution-backend shutdown.

Computer MUST NOT inspect Ephemeral AI FS tables, manifests, chunks, pages, or
write-session buffers. It MUST NOT duplicate the provider's memory cache. The DOFS
comparison engine may use its own provider behind the same Computer-owned forwarding
boundary.

## Conformance requirements

`@ephemeralai/fs-testkit` MUST provide a Node VFS suite. The suite MUST run against a
real file-backed SQLite database and MUST cover at least:

1. direct range reads from the start, middle, end, and beyond EOF;
2. proof that path and pinned-session range reads do not materialize the complete file;
3. `readIntoSync` writes only the requested destination range, allocates no equal-sized
   intermediate value, and preserves its pinned snapshot;
4. sequential writes split into irregular host buffer sizes;
5. discontinuous and overlapping writes;
6. read-after-write through the same and a second provider handle;
7. three same-inode sessions in every flush order, proving monotonic admission and no
   stale-base lost update;
8. truncate growth, shrink, and zero-filled gaps;
9. pending create, exclusive create, rename, unlink, and hard links;
10. hidden prefix staging versus visible commit and FUSE fsync;
11. flush, data-only flush, provider sync, close, retry, and abort;
12. injected SQLite failure before, during, and after a core batch;
13. process restart after staging, successful commit, and unflushed writes;
14. session, pending-write, and aggregate resident-memory limits with several concurrent
    writers;
15. forced staging at the exact session and pending-write boundaries;
16. immutable capabilities and exact memory metrics;
17. all 4 KiB, 8 KiB, and 16 KiB copy-on-write page formats; and
18. no page-size interpretation, FastCDC implementation, SQL, or repository access in
    this package;
19. an active branch mount that sees its base-main content while its private mutations
    remain invisible to main and sibling branches and sibling-private mutations remain
    invisible to it, including reconnect to the same branch after restart;
20. opening a missing or terminal branch fails without main fallback, and replica-main
    writes fail with `EROFS` before mutation;
21. the provider and replication endpoint derive from one runtime and remain within one
    aggregate managed-memory budget; and
22. incoming replication activation invalidates new-open caches while preserving pinned
    read snapshots and serializing or rejecting dirty writers without lost updates.

Fault tests MUST prove that a failed flush remains readable and retryable, that abort
releases its entire accounted capacity, and that resident bytes never cross the
configured aggregate limit.

## Benchmark and release gates

Benchmarks MUST use fresh databases and report the Node.js version, SQLite adapter, page
size, FastCDC profile, logical bytes, physical SQLite growth, transaction count,
elapsed-time distribution, and peak resident bytes.

The version 0.1 release suite MUST include these workloads:

- one one-byte overwrite in a 100 MiB file at each supported page size;
- 1,000 small overwrites in a 100 MiB file;
- a bounded-range-loop 100 MiB sequential read;
- one 100 MiB sequential FUSE-style materialization;
- 100 files of 1 MiB each; and
- interleaved writes across 1, 16, and 64 sessions under the default 128 MiB aggregate
  and 64 MiB pending-write budgets.

The small-edit workloads MUST NOT read, hash, or materialize the complete file. Their
retained branch payload MUST remain proportional to the persisted copy-on-write page
size. Repeated writes to one page MUST update bounded page-local state rather than
append one full page per call.

The sequential read MUST use direct bounded range I/O and remain within the aggregate
resident-memory budget. Core `readStream` behavior belongs to the portable filesystem
suite, not this provider suite.

The 100 MiB materialization MUST use sequential coalescing and stay within the 16 MiB
session, 64 MiB pending-write, and 128 MiB aggregate defaults. It MUST NOT retain a 100
MiB process-local file buffer.

Numeric performance thresholds and trial methodology are owned by
[`performance-and-resource-limits.md`](./performance-and-resource-limits.md). Storage
tests MUST include incompressible, partially repeated, and fully repeated content so CAS
gains are not inferred from one repetitive fixture.

## Release condition

`@ephemeralai/fs-node-vfs` is ready for Computer integration only when:

- every conformance and fault-injection case passes;
- direct and session reads never require whole-file materialization;
- all dirty memory is included in the pending-write class and one enforced aggregate
  resident budget;
- SQLite work passes through supported core batching APIs;
- page size is reported from, and interpreted only by, the core;
- the benchmark gates pass against the retained DOFS control; and
- Computer can select and open the exact active branch provider from its shared runtime
  without filesystem logic of its own; and
- missing or terminal branch reconnect never falls back to main, replica main remains
  read-only, and live replication activation passes the pinned-reader and dirty-writer
  conformance cases.
