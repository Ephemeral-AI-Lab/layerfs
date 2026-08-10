# Performance and resource limits

| Field | Value |
| --- | --- |
| Status | Draft |
| Scope | Performance, memory, buffering, and release gates |
| Last updated | 2026-08-10 |

This document defines normative performance and resource-safety requirements
for Ephemeral AI FS version 0.1. The words MUST, MUST NOT, SHOULD, SHOULD NOT,
and MAY have the meanings established in [`SPEC.md`](../../SPEC.md).

Correctness, durability, and integrity remain governed by the filesystem,
storage, and branch specifications. An optimization MUST NOT weaken those
contracts. Companion specifications and fixtures MUST use the configurable
copy-on-write page values and 8 KiB default defined below.

## 1. Scope and responsibilities

The portable core, Node virtual filesystem provider, and Computer FUSE bridge
have distinct responsibilities.

### 1.1 Portable core

`@ephemeralai/fs` MUST own:

- SQLite-backed namespace, objects, manifests, overlays, and revisions;
- persisted copy-on-write page configuration;
- aggregate memory accounting for each open filesystem instance;
- bounded caches, query batches, rechunking windows, and stream buffers;
- backpressure for portable read and write streams;
- atomic visibility and read-after-write semantics; and
- portable operation and resource metrics.

The core MUST NOT import Node.js filesystem, FUSE, Computer, container, or
remote procedure call types.

### 1.2 Node virtual filesystem provider

`@ephemeralai/fs-node-vfs` MUST own:

- translation from Node-style open, read, write, flush, and close operations;
- bounded sequential write sessions across repeated write callbacks;
- per-session reservations from the core instance's aggregate write budget;
- discontinuity detection and ordered flushes; and
- provider metrics such as open-session count, flush count, and callback size.

The provider MUST delegate durable storage and final atomic visibility to the
core. It MUST NOT create a second namespace, content store, or recovery model.

### 1.3 Computer FUSE bridge

Ephemeral AI Computer MUST own:

- FUSE protocol negotiation and request dispatch;
- kernel cache flags, readahead, maximum-request negotiation, and invalidation;
- mapping FUSE handles to Node virtual filesystem sessions;
- process-wide limits across concurrently mounted workspaces; and
- paired Ephemeral AI FS and DOFS benchmark orchestration.

FUSE flags and kernel-specific behavior MUST NOT appear in the portable core.
Computer MUST select one filesystem engine for a workspace lifetime and MUST
not use DOFS as an automatic fallback.

## 2. SQLite-only durable storage

Version 0.1 MUST keep SQLite as the complete durable storage foundation.

- Namespace, metadata, content objects, manifests, branch overlays, staging,
  leases, revisions, publication results, and maintenance state MUST be stored
  through the portable SQLite adapter.
- Content object payloads MUST be SQLite BLOB values addressed by SHA-256.
- Identical verified object bytes MUST have one durable object row within a
  filesystem.
- New object writes SHOULD use bounded `INSERT OR IGNORE` batches or equivalent
  conflict-safe insertion.
- Derived indexes and caches MAY exist in memory, but loss of all such state
  MUST be recoverable by reopening SQLite.
- An external pack file, object store, memory-mapped payload file, or second
  database MUST NOT be required by the version 0.1 format.
- SQLite WAL, rollback journals, temporary files, and adapter-owned database
  files are part of SQLite operation and are not external payload stores.

Physical SQLite file size MUST be reported separately from logical bytes,
retained payload, reclaimable payload, and free database pages. Garbage
collection MUST NOT claim that physical file size shrank unless the measured
SQLite file boundary actually shrank.

## 3. Persisted copy-on-write page size

Copy-on-write page size and FastCDC object size are independent parameters.
Changing one MUST NOT reinterpret the other.

```ts
export type CowPageBytes = 4096 | 8192 | 16384;

export interface StorageFormatOptions {
  readonly cowPageBytes?: CowPageBytes;
}
```

The version 0.1 default `cowPageBytes` value is 8,192 bytes. A filesystem MAY
be explicitly created with 4,096 or 16,384 bytes.

The effective value:

- MUST be persisted in filesystem metadata at creation;
- MUST be exposed through immutable filesystem capabilities;
- MUST be inherited by every branch in that filesystem;
- MUST define page indexes and complete page-overlay row lengths;
- MUST be checked when each writer opens the filesystem; and
- MUST NOT change for an existing filesystem without a format migration.

An omitted open option MUST use the persisted value for an existing
filesystem and the 8 KiB default for a new filesystem. An explicit value that
does not match existing metadata MUST fail before admitting operations.

FastCDC minimum, average, and maximum values remain manifest parameters. A
4 KiB, 8 KiB, or 16 KiB overlay page MAY intersect part of a larger FastCDC
object. Page size MUST NOT alter object identity or manifest encoding.

Conformance tests MUST run page-overlay correctness cases with all three page
sizes. Performance reports MUST identify the selected page size.

## 4. Accounted memory

Memory boundedness is defined by tracked allocation capacity, not by object or
entry count.

### 4.1 Resource configuration

The core MUST accept limits equivalent to the following shape:

```ts
export interface RuntimeLimits {
  readonly maxManagedResidentBytes: number;
  readonly maxCacheBytes: number;
  readonly maxPendingWriteBytes: number;
  readonly maxWriteSessionBytes: number;
  readonly maxPrefetchBytes: number;
  readonly maxQueryBatchBytes: number;
  readonly maxPreparedResultBytes: number;
  readonly maxConcurrentStreams: number;
  readonly maxConcurrentOperations: number;
  readonly maxOpenBranchHandles: number;
  readonly maxOpenNodeVfsSessions: number;
}
```

All values MUST be positive safe integers. The class limits MAY sum to more
than `maxManagedResidentBytes`; the aggregate limit always controls admission.
The effective values MUST be exposed through capabilities.

The default resource profile is:

- 128 MiB of aggregate managed memory;
- 64 MiB of content and manifest caches;
- 64 MiB of aggregate pending writes;
- 16 MiB per sequential write session;
- 1 MiB of aggregate speculative prefetch;
- 2 MiB in one query-result batch;
- 64 MiB in prepared result values;
- 64 concurrent streams;
- 256 admitted operations;
- 1,024 open branch handles; and
- 256 open Node VFS sessions.

A Node or Computer host MUST additionally configure finite process-level
limits equivalent to:

```ts
export interface HostMemoryLimits {
  readonly maxProcessResidentBytes: number;
  readonly runtimeHeadroomBytes: number;
  readonly maxTransportBytes: number;
  readonly maxFuseBridgeBytes: number;
}
```

Across every filesystem instance and connection in that process, admission
MUST preserve this relationship:

```text
managed filesystem reservations
+ SQLite cache targets
+ SQLite mmap limits
+ transport reservations
+ FUSE bridge reservations
+ fixed runtime headroom
<= maxProcessResidentBytes
```

The reference single-workspace Node and Computer profile is 256 MiB:

- 128 MiB of aggregate managed filesystem reservations;
- a 16 MiB SQLite cache target and zero-byte SQLite mmap limit;
- 20 MiB of host transport reservation;
- 4 MiB of FUSE bridge reservation; and
- 88 MiB of JavaScript, SQLite statement, codec, and native runtime headroom.

This profile is justified by useful concurrency without per-handle
multiplication: a 64 MiB result can coexist with bounded query and control
work, four full 16 MiB write sessions fit before forced flushing, and a
maximum compact manifest can cross one bounded replication envelope. The
sub-limits are admission ceilings and MUST NOT be preallocated.

Several workspaces in one process share one explicitly configured process
budget. They MUST NOT each assume an independent 256 MiB allowance. Smaller
profiles are conformant when they can hold one maximum required value, retain
cleanup reserve, and pass progress and fault tests with earlier backpressure.

A Durable Object adapter with runtime-managed native memory MUST say that an
exact process bound is unavailable. Computer must then use the platform's
isolate limit, conservative managed limits, and measured resident memory; it
must not report the core counter as total process memory.

Opening MUST reject a configuration that cannot hold one maximum content
object, one COW page, and the minimum metadata needed to make progress. The
accepted manifest-entry limit MUST also ensure that a canonical manifest can
be decoded within the aggregate limit.

The filesystem `maxMaterializedBytes` value MUST NOT exceed
`maxPreparedResultBytes`. Each admitted operation and open branch handle MUST
consume a count slot and reserve bounded control state. Completion, rejection,
cancellation, or close MUST release it.

### 4.2 Accounted classes

The core MUST account for the allocated capacity of:

- content-object and verified-object caches;
- decoded manifest and namespace caches;
- pending portable write-stream data;
- Node virtual filesystem write reservations;
- FastCDC scan and local-rechunking windows;
- speculative prefetch not yet requested by a consumer;
- materialized SQLite query results retained by the core; and
- replication requests, responses, codec blocks, digest state, inventory
  pages, cursors, results, and retry-retained buffers; and
- internal copies made for hashing, stitching, or transaction preparation.

If two values share one backing allocation, that allocation MAY be counted
once. If the implementation cannot prove that they share storage, it MUST
count both allocated capacities.

Caller-owned input before admission, bytes already transferred to a consumer,
JavaScript runtime metadata, native SQLite page caches, host transport and
FUSE buffers before core admission, and operating-system page caches are
outside tracked managed memory. They remain inside the host process budget
where applicable. Benchmark gates MUST record process peak resident memory so
that large untracked growth is visible.

Being outside the exact core counter does not make native SQLite memory
unlimited. The Node adapter MUST use and report finite page-cache and
memory-map settings; its default cache target is 16 MiB and its default memory
map limit is zero. Storage-scale temporary work MUST be file-backed. A
runtime-owned Durable Object cache MUST be reported as runtime-managed.
Release tests MUST fail when process resident memory grows with total database
rows after the managed cache has reached steady state, except for a documented
runtime effect reproduced without Ephemeral AI FS process caches.

### 4.3 Reservation and release

Before creating or growing an accounted allocation, an operation MUST reserve
the corresponding bytes from both its class limit and the aggregate limit.

When a reservation is unavailable, the implementation MUST do one of the
following without exceeding a limit:

1. evict unpinned cache entries;
2. bypass cache or prefetch admission;
3. flush an ordered pending-write batch;
4. stop pulling a stream and apply backpressure; or
5. wait for an existing reservation to be released.

The core MUST NOT solve pressure by allocating first and accounting later. A
non-streaming caller value above its configured operation limit MUST fail with
the existing resource-limit error before mutation.

Success, cancellation, close, stream error, failed SQLite work, and exhausted
retry policy MUST release every reservation exactly once. Retrying a failed
flush MUST reuse or reacquire accounted data without duplicating its
reservation.

All handles within one filesystem instance share one aggregate budget. A host
opening several instances MUST configure or enforce a host-level aggregate
whose value is no greater than its process budget. Computer MUST apply this
rule across mounted workspaces.

### 4.4 Cache behavior

Caches MUST be weighted by retained byte capacity. Entry-count-only eviction
is not sufficient.

Cache entries that are required by an admitted operation may be pinned only
for that operation's bounded lifetime. A sequential scan larger than the cache
SHOULD bypass normal admission or use a scan-resistant policy so that it does
not evict the complete hot working set.

Digest-verification caching MUST remain process-local and bounded. Evicting a
verification result MUST affect performance only, never integrity semantics.

### 4.5 Storage working sets

The storage layer MUST NOT use process memory as a mirror of SQLite. It MUST
NOT retain a complete object-location index, namespace, revision graph,
changed-path set, replication inventory, or garbage-collection mark graph.

Enumeration MUST use bounded keyset pages. Durable marks, cursors, staging,
and checkpoints remain in SQLite and count against storage and maintenance
quotas. FastCDC, hashing, and manifest work retain only the current bounded
window, object, manifest, and query batch plus explicitly reserved cache
entries.

Increasing total stored rows or logical file size MUST increase total work but
MUST NOT increase managed-memory high-water beyond the configured aggregate
and one admitted bounded value. Tests MUST exercise millions of SQLite rows
with deliberately small query and memory limits.

## 5. Lazy, non-materializing reads

A read-only operation MUST NOT copy a file into another layer or create
durable namespace, object, manifest, overlay, staging, or lease state except
for a lease explicitly required by the snapshot-stream contract.

An unchanged branch path MUST read directly from its selected immutable base
manifest. Reading it MUST NOT create branch-local pages or a materialized
branch manifest.

`readFile` and other byte-array APIs remain subject to
`maxMaterializedBytes`. A larger request MUST use a bounded range or stream.

A streaming read MUST:

- decode manifests within the managed-memory budget;
- fetch objects in batches bounded by query count, binding count, BLOB size,
  `maxQueryBatchBytes`, and aggregate memory;
- emit nonempty chunks no larger than the effective preferred stream size;
- stop fetching when consumer backpressure is active;
- retain no more than the configured prefetch allowance beyond demanded data;
- preserve one selected snapshot for the entire stream; and
- release leases, pins, and reservations on completion, cancellation, error,
  or close.

Sequential read optimizations MAY batch lookups or prefetch adjacent objects.
They MUST NOT require the full file, full object index, or all object payloads
to be resident at once.

## 6. Bounded sequential write sessions

Sequential write coalescing exists to amortize hashing, object insertion, and
SQLite metadata work across small host callbacks. It is not permission to
buffer a complete file.

### 6.1 Session behavior

The Node virtual filesystem provider MAY open one sequential write session for
one file handle. A session MUST:

- reserve every buffered byte from the per-session, aggregate pending-write,
  and aggregate managed-memory budgets;
- coalesce only writes whose offsets are contiguous with the buffered range;
- preserve callback order and read-after-write visibility;
- flush before applying a noncontiguous or overlapping write whose order
  cannot be represented by the current buffer;
- flush on explicit flush, synchronization, close, budget pressure, or error
  recovery;
- make close idempotent; and
- report a failed flush to the host without acknowledging durability.

A callback larger than the per-session limit MUST be processed as bounded
streaming batches. It MUST NOT receive an equal-sized internal copy.

The provider MUST NOT reserve its maximum independently for every handle. All
sessions draw from the same instance aggregate, so concurrency produces
backpressure or earlier flushes rather than memory multiplication.

### 6.2 SQLite and atomicity

FastCDC scanning MUST retain no more than the configured scan window and
bounded batch state. Object payloads MAY be staged in several SQLite
transactions under the storage specification's durable staging lease.

The core MUST NOT keep a SQLite transaction open while waiting for another
host callback or stream chunk. The final namespace and manifest reference MUST
become visible atomically. Before that transaction commits, readers outside
the session MUST observe the prior committed value.

A flush failure MUST leave the prior committed value valid. Buffered data MAY
remain available for a bounded retry only while its reservation remains held.
After close reports success, reopen and engine recreation MUST return the
complete committed bytes.

### 6.3 FUSE behavior

Computer MAY negotiate kernel cache retention, larger requests, asynchronous
read support, and writeback caching when the platform supports them. It MUST
invalidate affected ranges after committed writes, truncation, rename,
unlink, or engine replacement.

Computer MUST NOT acknowledge FUSE `flush`, `fsync`, or `release` success
before the Node virtual filesystem provider has satisfied the corresponding
durability contract. Kernel request concurrency MUST remain bounded by the
Computer process budget and provider session budget.

## 7. Metrics

Metrics MUST be observational and MUST NOT change transaction outcomes. Each
benchmarkable operation MUST be able to report or contribute to:

- logical bytes requested and returned;
- adapter BLOB bytes read and submitted;
- bytes hashed and locally rechunked;
- objects found, inserted, reused, and verified;
- COW page size, pages read, pages created, and pages upserted;
- manifest bytes decoded;
- SQLite query and transaction counts;
- full-scan fallback count;
- write flush count and flushed bytes;
- current and high-water cache bytes;
- current and high-water pending-write bytes;
- current and high-water prefetch bytes;
- current and high-water total managed bytes;
- backpressure count and duration;
- cache hits, misses, admissions, bypasses, and evictions;
- operation elapsed time; and
- failure or cancellation classification.

Storage snapshots MUST continue to distinguish logical bytes, unique retained
object bytes, manifest bytes, overlay bytes, reclaimable bytes, database free
pages, WAL bytes when observable, and physical SQLite bytes.

The Node virtual filesystem provider MUST additionally expose active and peak
session counts, callback-size distribution, contiguous-run length, and flush
reason. Computer MUST record FUSE request counts, request sizes, cache mode,
mount options, engine selection, and process peak resident memory.

## 8. Benchmark method

Release measurements MUST use checked-in deterministic fixture generators.
Every storage-sensitive workload MUST have both:

- duplicate-heavy content that can demonstrate reuse; and
- deterministic pseudorandom content that prevents misleading deduplication.

Each comparison MUST use a fresh isolated SQLite database created from the
same logical fixture. A paired Computer comparison MUST use a separate DOFS
database and MUST NOT share physical files or warm engine state.

Reports MUST identify:

- adapter, engine, commit, runtime, operating system, and hardware;
- COW page size and FastCDC parameters;
- all resource limits;
- cold, reopen, and warm-cache state;
- whether operating-system cache dropping succeeded;
- trial count and p50, p95, minimum, and maximum elapsed time;
- every metric required by the applicable release gate; and
- physical storage measured before and after checkpoint or vacuum operations.

At least 20 measured trials are required for latency distributions. Setup,
fixture creation, and teardown are outside the timed region. Correctness MUST
be verified after every trial by byte comparison or a separately computed
fixture digest.

## 9. Release gates

The counter and memory gates below are portable and mandatory on both SQLite
adapters. Latency gates compare the candidate with the last accepted result on
the same runner and benchmark profile. The first conformant version 0.1
release establishes that latency baseline.

A candidate MUST NOT regress p50 or p95 latency by more than 10 percent for a
gate workload unless the change is approved with a recorded correctness or
resource-safety justification and a new accepted baseline.

### 9.1 Small-edit gate

For each 4 KiB, 8 KiB, and 16 KiB page configuration, the suite MUST create a
100 MiB file and perform one one-byte private overwrite in its middle.

Before publication or explicit materialization:

- exactly one page overlay MUST contain the changed byte;
- no complete file value or new complete manifest may be retained;
- adapter content bytes read and bytes hashed MUST be bounded by the
  intersecting FastCDC object set and one COW page, not by file size;
- managed memory MUST remain within every configured limit; and
- reopen MUST reconstruct the exact branch value.

The suite MUST then overwrite the same byte 1,000 times. Retained overlay page
count and payload MUST remain one page. It MUST also run 1,000 scattered and
1,000 clustered edits and report page amplification, query count, transaction
count, retained bytes, p50, p95, and memory high-water.

The 8 KiB default MUST remain in the three-size comparison even if another
page size is fastest for a particular workload. Changing the default requires
a format decision and updated conformance fixtures.

### 9.2 Large-read gate

The suite MUST read both duplicate-heavy and pseudorandom 100 MiB files by:

1. one cold `readStream` pass;
2. reopen and a second pass;
3. one warm pass; and
4. bounded random range reads.

Each pass MUST produce exact bytes and perform zero durable content, manifest,
namespace, branch, or staging mutations. A stream MAY create its bounded lease
metadata and adapter-owned temporary journal records; those bytes MUST be
reported separately and released or expired by bounded maintenance.

The stream MUST pass under a test aggregate-memory limit equal to the greater
of 32 MiB or `maxManifestBytes + maxChunkBytes + preferredStreamChunkBytes +
2 MiB` of decoder and query headroom, regardless of file size. Tracked managed
memory MUST NOT exceed that limit. Increasing the fixture to 1 GiB MUST NOT
increase high-water managed memory by more than one preferred output chunk and
one maximum content object.

The report MUST include bytes read from SQLite, query count, cache behavior,
prefetch high-water, total managed-memory high-water, process peak resident
memory, throughput, p50, and p95.

### 9.3 Sequential and FUSE materialization gate

The core and Node virtual filesystem suites MUST measure:

- one 100 MiB sequential file creation;
- 100 files of 1 MiB each;
- one complete 100 MiB rewrite;
- 1,000 small writes followed by one flush; and
- interleaved sequential writes across 1, 16, and 64 sessions.

Each workload MUST run with duplicate-heavy and pseudorandom content. The
result MUST survive close and engine recreation. No session or aggregate
memory limit may be exceeded, including the 64-session case.

The report MUST include callback sizes, contiguous-run lengths, flush reasons,
object reuse, bytes hashed, SQLite query and transaction counts, write
amplification, p50, p95, and tracked and resident-memory high-water.

Computer MUST run the same common materialization fixture through Ephemeral AI
FS and the explicitly selected DOFS comparison engine. The paired comparison
is a release artifact, not permission for an automatic fallback. Branch-only
workloads MAY report DOFS as unsupported.

On the reference Computer runner, the Ephemeral AI FS bounded-range read MUST
reach at least 80 percent of the DOFS median throughput on the same fixture.
Its 100 MiB materialization median MUST take no more than 1.10 times the DOFS
median. These ratios are comparison gates only; correctness, durability,
memory, and no-materialization gates remain mandatory even when DOFS itself
does not satisfy them.

Computer's FUSE run MUST additionally verify flush, synchronization, release,
unmount, remount, truncation, and read-after-write behavior. A performance
result is invalid if a mount remains active, a background flush is pending, or
the final fixture digest does not match.

### 9.4 Concurrency and release gate

With deliberately small budgets, the suite MUST run 64 concurrent readers,
64 concurrent sequential writers, and a mixed read/write workload. It MUST
prove that:

- aggregate tracked memory remains at or below its configured limit;
- backpressure occurs instead of per-handle memory multiplication;
- cancellation and close release every reservation;
- injected SQLite busy and commit failures do not duplicate buffers;
- cache eviction never changes returned bytes; and
- the filesystem remains usable after all fault injections.

## 10. Non-gates and future work

Version 0.1 does not require external pack files, compression, BLAKE3, memory
mapping, adaptive path-dependent FastCDC parameters, or unlimited asynchronous
FUSE request dispatch. These ideas require separate format, portability,
memory, and crash-consistency evidence before becoming release requirements.

Absolute latency from an AgentFS, WSL2, or local-machine experiment is design
evidence, not a portable Ephemeral AI FS release threshold. Reproducible
complexity, byte, memory, durability, and regression gates in this document
control acceptance.
