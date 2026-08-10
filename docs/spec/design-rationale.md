# Design rationale

| Field | Value |
| --- | --- |
| Status | Non-normative rationale |
| Scope | Architecture, performance, and rejected alternatives |
| Last updated | 2026-08-10 |

This document explains why the version 0.1 specifications choose their current
boundaries. Normative behavior lives in the companion specifications. If this
document conflicts with a normative requirement, the normative requirement
wins and the contradiction must be corrected.

## 1. Workloads that shape the design

Ephemeral AI FS is optimized around three paths that appear together in agent
workspaces:

1. an agent changes a few bytes in a large private file;
2. a tool scans a large file without changing it; and
3. a container materializes many sequential bytes through FUSE.

A design that optimizes only one path is insufficient. Whole-file copy-on-write
helps sequential code but makes tiny edits expensive. Very small fixed chunks
reduce edit amplification but increase SQLite rows and transaction overhead.
Unbounded write-behind improves one stream but fails under many open handles.

The specifications therefore combine sparse page overlays, larger immutable
FastCDC objects, lazy range reads, bounded sequential write sessions, and
batched SQLite work.

## 2. Evidence carried forward

A prior AgentFS FUSE experiment isolated several useful workload-shaping
changes. Its environment and storage format differ from this project, so the
numbers are indicative evidence rather than Ephemeral AI FS release gates.

- Stopping read-only open from copying a base file reduced 100 one MiB first
  reads from about 604 ms to 201 ms. Database growth fell from about
  123.6 MiB to 0.21 MiB.
- Sparse 4 KiB copy-on-write replaced whole-file copy-up. A one-byte edit in
  a 100 MiB file fell from about 897 ms to about 3 ms.
- Batching adjacent reads and aligned writes avoided one SQLite transaction
  for every small host callback.
- A bounded 16 MiB contiguous-write buffer reduced one 100 MiB
  materialization from about 470 ms to 433 ms.
- Sharing one SQLite commit across small writes improved one thousand edits
  from about 162 ms to 140 ms in the compared paths.

The same experiment also established important cautions:

- one 16 MiB buffer per handle was not globally memory-bounded;
- an in-memory index was not byte-bounded;
- append-only pack storage had no completed compaction policy;
- asynchronous concurrent reads regressed in that implementation;
- duplicate-heavy fixtures overstated general storage savings; and
- proposed BLAKE3 and adaptive chunk changes were not measured.

Version 0.1 adopts the demonstrated access patterns, not the experiment's
literal storage layout.

## 3. Why SQLite remains authoritative

SQLite is a fixed architectural requirement, not a temporary implementation
detail. Both the Node.js and Durable Object adapters need the same transaction,
recovery, and integrity model.

Version 0.1 stores namespace rows, objects, manifests, overlays, revisions,
leases, cursors, results, and maintenance state through SQLite. Keeping content
objects as SQLite BLOBs provides:

- one crash-recovery authority;
- atomic constraints between content and metadata;
- identical portable behavior across both required adapters;
- no external payload file to reconcile after failure; and
- simpler backup, migration, verification, and fault injection.

Implementation starts against ordinary local SQLite. The Node adapter binds a
normal SQLite driver to the portable transaction contract. The Cloudflare
adapter binds the same contract to the private embedded SQLite database that
Cloudflare exposes through
[`ctx.storage.sql`](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/).
Neither adapter owns schema or filesystem behavior. The Cloudflare adapter is
needed only when Computer's authoritative workspace runs in a Durable Object,
where a native Node SQLite file handle is not the storage interface.

An external pack file may reduce some Node.js BLOB overhead, but it has no
portable Durable Object equivalent and adds ordering, compaction, and orphan
recovery protocols. It is therefore excluded from the version 0.1 required
format. A future optional payload tier must keep SQLite as its authoritative
index and recovery journal and requires a separate format specification.

## 4. Why copy-on-write pages and FastCDC are separate

Copy-on-write pages answer: how much private state should one equal-length
overwrite retain? FastCDC answers: how should an immutable complete file reuse
content after structural shifts? They solve different problems and must not
share one size setting.

Version 0.1 accepts 4, 8, and 16 KiB copy-on-write pages and defaults to 8 KiB:

| Page size | Primary advantage | Primary cost |
| --- | --- | --- |
| 4 KiB | Minimum isolated-edit payload | More rows and page operations |
| 8 KiB | Balanced mixed-edit profile | Twice the isolated payload of 4 KiB |
| 16 KiB | Fewer rows for clustered writes | Larger isolated-edit payload |

The value is persisted because it interprets page indexes. It cannot change on
reopen or by branch. Conformance runs at all three sizes, and benchmark reports
must make the selected value visible.

FastCDC uses larger content objects to keep manifests and SQLite operations
compact. Its parameters remain persisted with the content format. Small branch
edits do not immediately rechunk a complete file; publication or bounded
materialization converts the final view into canonical FastCDC objects.

## 5. Why reads do not materialize

Reading an immutable base manifest already identifies the selected bytes.
Creating another file value, branch manifest, or object set during a read adds
write amplification and first-byte latency without improving correctness.

Range reads therefore fetch only intersecting manifest entries and objects.
Snapshot streams root their selected manifest and exact branch overlay rows,
then verify objects lazily before emitting them. They do not enumerate one
lease row per object and do not materialize a branch merely to open a stream.

This rule is structural rather than a timing promise: increasing a file from
100 MiB to one GiB may increase total scan time, but it must not increase the
managed-memory high-water mark beyond bounded stream and object windows.

## 6. Why write sessions are bounded twice

FUSE may divide one sequential file creation into many callbacks. Running
FastCDC, hashing, object insertion, and metadata transactions for every
callback wastes work. The Node virtual filesystem provider may therefore
coalesce contiguous writes.

A per-session limit alone is unsafe because many handles can each allocate the
maximum. Sessions reserve from both a 16 MiB per-session limit and a 64 MiB
aggregate pending-write limit, which is itself inside the core-wide resident
budget. Pressure causes an early flush or backpressure before allocation.

The provider preserves write order, makes admitted bytes visible to its own
handles, and reports flush failures. The core carries FastCDC state and stages
SQLite objects in bounded transactions before one atomic visible update.
Computer does not add another whole-file buffer above this layer.

The 128 MiB managed default is one shared ceiling, not a promise to every
handle. A 256 MiB single-workspace host profile leaves separate bounded room
for Node SQLite, transport, FUSE, and runtime-native overhead. Several mounted
workspaces share the host ceiling. Node SQLite defaults to a 16 MiB cache
target and zero-byte memory mapping so native storage memory cannot silently
grow with database size.

## 7. Why manifests remain compact and bounded

Version 0.1 uses one canonical compact manifest BLOB rather than one SQLite row
per content object. One lookup and sequential decode are favorable for the
large-read path, while branch page overlays avoid creating a new manifest for
every private small edit.

The BLOB is capped by `maxManifestBytes`, defaults to 16 MiB, and participates
in aggregate memory accounting. This bounds its worst-case decode allocation.
The local rechunker reuses unchanged content objects even though the final
canonical manifest metadata is encoded again.

A segmented manifest tree could share metadata leaves for extremely large
files, but would add queries, node verification, garbage-collection roots, and
a more complex persisted format. It is deferred until measured manifest-byte
amplification justifies that cost. Such a change requires a new manifest format
identifier and migration fixtures.

## 8. Package boundary justification

```text
@ephemeralai/fs
    <- sqlite-node
    <- sqlite-cloudflare
    <- replication
    <- node-vfs

Ephemeral AI Computer
    -> selects an engine
    -> carries replication messages
    -> maps FUSE handles to node-vfs sessions
```

The core owns filesystem meaning and persisted state. SQLite adapters normalize
driver behavior. Replication owns batching, cursors, retry, and validation but
not transport. Node VFS owns handles and bounded write sessions but not FUSE.
Computer owns routing, mounts, processes, and the optional DOFS comparison.

These entry points keep Computer integration to factories and forwarding. If
Computer must interpret manifests, collect hashes, manage replication cursors,
or buffer file contents, the package boundary is incomplete.

## 9. Rejected version 0.1 alternatives

- Replacing SQLite is outside scope.
- External pack files are not a required payload store.
- A complete in-memory object-location index is not permitted.
- Whole-file read copy-up or whole-file FUSE buffers are not permitted.
- Per-handle memory allowances cannot bypass one aggregate budget.
- BLAKE3 does not replace SHA-256 in version 0.1.
- FUSE types and flags do not enter the portable core.
- Unlimited read concurrency is not assumed to improve throughput.
- Pre-compaction physical size is not treated as steady-state storage.

## 10. How claims become falsifiable

The performance and resource specification defines checked-in workloads for:

- isolated, repeated, scattered, and clustered small edits;
- cold, reopened, and warm large sequential reads;
- one large and many smaller FUSE materializations;
- interleaved writes under aggregate pressure;
- duplicate-heavy and deterministic incompressible content; and
- replication of complete files and small changes.

Every run records latency distributions, throughput, managed-memory high-water
marks, process resident memory, SQLite queries and transactions, BLOB bytes,
hashing and rechunking work, object reuse, overlay bytes, database bytes, and
write-ahead-log bytes where observable.

Correctness and hard resource bounds are absolute gates. Latency is compared
with the last accepted result on the same pinned runner and, for common
Computer workloads, with an explicitly selected isolated DOFS run. A
performance regression cannot be hidden by changing fixtures, warming one
engine with another, or replacing SQLite.
