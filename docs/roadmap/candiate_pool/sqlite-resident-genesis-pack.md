# SQLite-resident genesis pack

> **Status:** Deferred candidate, recorded 2026-09-02. Do not implement this
> design unless the admitted direct-streaming initialization path remains below
> 200 MB/s at the 100,000-file tier and SQLite object admission is still a
> measured dominant cost. This document is not a release commitment, product
> contract, schema proposal, or authorization to change production behavior.

## Decision summary

If LayerFS eventually needs physical packing, keep one durable Store per
project:

```text
project/
└── store.sqlite
```

Store immutable genesis-pack payloads inside SQLite as bounded BLOB segments.
Do not add a persistent pack directory or sidecar file, and do not represent a
pack as one appendable giant BLOB. Preserve canonical bytes, `ObjectId`s,
filesystem roots, Layer and Commit identities, SDK semantics, and exact read
authentication.

The smallest candidate uses SQLite itself as the object locator:

```text
store.sqlite
├── objects          loose roots and ordinary Workspace Commit objects
├── packs            BUILDING/SEALED pack descriptors
├── pack_segments    approximately 4 MiB immutable payload BLOBs
└── packed_objects   ObjectId -> segment, offset, length
```

Start with one genesis pack and a compact `WITHOUT ROWID` locator row per
unique packed object. Do not build a custom packed index, background packer,
general compactor, new worker pool, or packed-Commit path unless later evidence
independently admits it.

## Why this candidate exists

The provisional 100,000-file namespace profile observed approximately:

- 500 MB of logical fixture data;
- 422,000 unique canonical objects;
- 543 MB of unique canonical bytes;
- 1.13 million canonical put attempts;
- 661 MB of Store growth;
- 1.06 seconds of SQLite bind/step work; and
- 4.23--4.30 seconds of complete initialization, or 116--118 MB/s.

These measurements suggest that one independently indexed SQLite payload row
per canonical object can become material at high object counts. They do not
yet admit a format change. Direct streaming is the first candidate because it
can remove intermediate segment write/read amplification without changing the
Store schema or read path.

Physical packing is reconsidered only when a retained direct-streaming result
shows both:

1. less than 200 MB/s effective logical initialization throughput at 100,000
   files; and
2. SQLite object admission remains a dominant phase with at least 30% total
   wall time recoverable by packing.

## Existing contract affected

The current exact v4 schema stores each canonical object as:

```sql
CREATE TABLE objects (
    object_id BLOB PRIMARY KEY
        CHECK (length(object_id) = 32),
    bytes BLOB NOT NULL
) STRICT;
```

`layers.root_id` and `commits.root_id` are foreign keys to `objects.object_id`.
Schema verification also expects the exact v4 manifest. A pack implementation
therefore requires an explicitly admitted v5 Store format or dual-version
support; it is not a private optimization that can be added silently.

Keep each published Layer or Commit root object loose in `objects`. Its
descendants may resolve from a sealed pack. This retains the existing root
foreign-key protection while avoiding hundreds of thousands of loose payload
rows.

## Candidate physical layout

The following is explanatory rather than frozen SQL:

```sql
CREATE TABLE packs (
    pack_id INTEGER PRIMARY KEY,
    state INTEGER NOT NULL,
    object_count INTEGER NOT NULL,
    encoded_bytes INTEGER NOT NULL,
    checksum BLOB
) STRICT;

CREATE TABLE pack_segments (
    pack_id INTEGER NOT NULL
        REFERENCES packs(pack_id) ON DELETE CASCADE,
    segment_number INTEGER NOT NULL,
    bytes BLOB NOT NULL,
    PRIMARY KEY (pack_id, segment_number)
) STRICT, WITHOUT ROWID;

CREATE TABLE packed_objects (
    object_id BLOB PRIMARY KEY
        CHECK (length(object_id) = 32),
    pack_id INTEGER NOT NULL,
    segment_number INTEGER NOT NULL,
    byte_offset INTEGER NOT NULL,
    byte_length INTEGER NOT NULL,
    FOREIGN KEY (pack_id, segment_number)
        REFERENCES pack_segments(pack_id, segment_number)
) STRICT, WITHOUT ROWID;
```

The exact schema must additionally constrain state values, nonnegative ranges,
pack equations, segment lengths, and visibility. It must be derived only after
the candidate is admitted.

### Why bounded segments

Do not append with repeated `UPDATE bytes = bytes || ?`: repeated whole-BLOB
copying can become quadratic. Do not reserve one hundreds-of-megabytes BLOB:
its final length is not known at the start of streaming, its transaction and
recovery behavior are harder to bound, and it concentrates random-access and
corruption risk.

Begin with the existing 4 MiB admission boundary. Typical CDC payload objects
are no larger than 32 KiB, so many fit in one segment. A legal canonical object
larger than the segment target receives a dedicated oversized segment rather
than being truncated or split ambiguously. Test another segment size only when
measured reads or writes show that 4 MiB is material.

### Why a SQLite locator first

A compact `WITHOUT ROWID` locator keeps exact deduplication, collision checks,
and random lookup inside SQLite. It needs no persistent sidecar, full-pack
temporary copy, global in-memory index, external sort, or custom index reader.

It retains one small row per unique object and therefore cannot eliminate all
file-count sensitivity. A custom sorted index stored in a few hundred BLOB
pages may remove more row work, but it requires bounded sorting/spill,
additional recovery rules, and a new lookup algorithm. Build it only if the
compact locator is itself measured as the remaining bottleneck and replacing
it predicts at least a further 30% wall-time improvement.

## Initialization flow

Use no worker pool. The intended shape is one import producer plus the SDK
caller as the sole SQLite admission owner:

```text
source directory
  -> one producer: enumerate, read, CDC, hash, encode canonical objects
  -> bounded owned batch handoff
  -> caller: exact lookup/dedup and append to a segment buffer
  -> bounded transaction: insert segment and locator rows
  -> repeat
  -> insert final root loosely
  -> validate pack equations and checksum
  -> one final transaction: SEALED + genesis Layer + LayerStack
```

`BUILDING` packs are never readable. Normal readers search only `SEALED` packs.
Failure before final publication can leave durable but unreachable BUILDING
rows; reconnect cleanup removes them transactionally after proving that no
visible Layer or Commit references them.

Workspace Commit remains unchanged and stores its localized candidate objects
loosely. Packing is initialization-only. It does not replace candidate staging,
Branch-head compare-and-swap, reconciliation, or LayerStack publication.

## Read path

Central object access must resolve the union of loose and packed objects:

```text
requested ObjectId
  -> loose objects lookup
  -> sealed packed_objects locator
  -> checked segment range read
  -> authenticate canonical bytes against the requested ObjectId
```

Authentication is mandatory on every single, batch, cached, and reopened read.
Pack lookup must not add one pair of SQLite round trips per object. For an
existing batch of up to 128 ObjectIds:

1. fetch locators together;
2. group locators by segment;
3. read requested ranges in segment order;
4. restore caller order; and
5. authenticate every canonical object.

The minimum initial range read may use SQLite's built-in BLOB `substr()`. Add
rusqlite incremental-BLOB support only if a representative read benchmark
proves that `substr()` is materially worse.

Every direct `objects` query must be audited. Object membership, collision
checks, canonical-storage accounting, reachability, integrity scans, snapshot
reads, Commit deduplication, and monitoring must all understand the loose plus
sealed-pack union.

## Resource expectations

These are candidate targets, not admitted results.

### Time

| Design | Estimated 100,000-file initialization | Effective logical throughput |
| --- | ---: | ---: |
| Current object-per-row diagnostic | 4.23--4.30 s | 116--118 MB/s |
| Direct streaming with existing rows | 1.9--2.7 s | 185--265 MB/s |
| SQLite-resident segments and locators | 1.7--2.5 s | 200--295 MB/s |
| Custom packed index, only if later admitted | 1.3--2.0 s | 250--385 MB/s |

No estimate substitutes for a retained fresh-process result. Initialization
must remain at least linear in files plus bytes because every path and file must
be enumerated and authenticated. The goal is a small per-file constant plus
near-sequential byte cost, not zero file-count cost.

### Durable storage

For the provisional 100,000-file profile:

```text
unique canonical payload                 approximately 543 MB
compact locator/index storage            approximately 25--45 MB
pack and SQLite page overhead             approximately 10--30 MB
estimated final Store                     approximately 578--618 MB
current observed Store growth             approximately 661 MB
```

Record final and peak `store.sqlite` size, SQLite freelist bytes, temporary
bytes, logical bytes, unique canonical bytes, packed bytes, and physically
duplicated bytes. Reachable-byte accounting must not hide a larger database or
temporary spill. The initial locator design should require neither a persistent
second storage nor a full-pack temporary copy.

### Memory and CPU

Target approximately 6--10 MiB of LayerFS-managed initialization memory:

```text
segment buffer                    approximately 4 MiB
stream handoff batch              approximately 1 MiB
pending locators/dedup state      less than 1 MiB
CDC scratch                       less than 0.1 MiB per producer
query/result and misc. buffers    approximately 1--3 MiB
```

This is not a 10 MiB complete-process RSS promise. The current Store alone
configures a 32 MiB SQLite page cache; runtime, allocator, libraries, and FUSE
state are additional. Report supervisor-observed complete-process RSS and
LayerFS-managed incremental memory separately. Do not increase workers, CPU,
RSS, page cache, I/O, or storage for a wall-time gain smaller than 30%.

## Risks

### 1. Read and Workspace-create regression

A packed read adds locator and range-resolution work. A naive pair of SQL
queries per object can recreate severe round-trip amplification. Batch locator
queries and coalesce reads by segment. Reject the candidate if representative
Workspace creation, traversal, random small reads, or a 100 MB sequential read
regresses by more than approximately 15--20% without a larger complete-
lifecycle win.

### 2. v4/v5 compatibility and migration

The exact v4 schema cannot accept pack tables. Migration may temporarily need
old rows, new segments, and SQLite journal/freelist space, approaching 1.5--2x
the original database. Do not silently migrate during ordinary `connect()`
until crash recovery, peak disk, rollback, old-client rejection, and reopen are
specified and proved. Prefer new-Store evidence before admitting migration.

### 3. Incorrect offsets or lengths

A locator can address the wrong object, exceed a segment, overflow arithmetic,
or reference a missing or BUILDING pack. Use checked conversions and checked
`offset + length`, require an exact returned length, and authenticate bytes
against the requested ObjectId. Any mismatch is an integrity error, never a
fallback or partial read.

### 4. Larger corruption blast radius

Damage to one segment can make many canonical objects unavailable. Per-object
authentication prevents silent substitution but cannot recover data. Record
segment and manifest checksums for diagnosis. Backup or source reconstruction,
not parity or replication in this candidate, owns recovery.

### 5. Hidden failed-build rows

Bounded transactions can leave durable BUILDING segments after later failure.
They must remain invisible, be cascade-deletable, and be cleaned only while the
Store operation gate proves that no initializer is active. Inject failure at
each segment and final-publication boundary.

### 6. SQLite file growth and freelist retention

Deleting a failed or superseded pack usually returns pages to SQLite's freelist
without shrinking `store.sqlite`. Repeated failure must not leave an apparently
small reachable Store inside a much larger file. Measure physical size and
freelist. Do not run foreground `VACUUM`; it rewrites the database and may need
substantial temporary disk.

### 7. Same-project mutation blocking

The Store has one SQLite connection and FIFO mutation gate. Initialization of a
new empty project can hold it safely, but foreground repacking of an active
project would block commits. The candidate is genesis-only. It does not admit
background packing, commit packing, or automatic compaction.

### 8. Coarse garbage collection

One dead packed object cannot release physical space independently. This is
acceptable for a reachable immutable genesis Layer but unsuitable for frequent
small Commit churn. Keep later commits loose. Admit a compactor only after
measured unreachable storage justifies its concurrency, memory, I/O, and crash
model.

### 9. Remaining per-object locator cost

Approximately 422,000 compact locator rows still impose file-count-sensitive
B-tree work. The candidate may improve payload layout without reaching the
upper throughput target. Do not declare success from segment-write speed alone;
measure the complete public initialization boundary. Do not jump to a custom
index unless the locator phase is proven dominant.

### 10. SQLite large-BLOB behavior

Overflow-page traversal, `substr()` behavior, and cache pollution may vary with
access locality. Start with 4 MiB segments and the existing 32 MiB SQLite cache.
Compare other segment sizes or incremental-BLOB reads only after phase and I/O
evidence identifies this path.

### 11. Honest memory accounting

Segment buffers can stay below 10 MiB while the complete process remains much
larger because SQLite and the runtime are real costs. Retain complete-process
RSS, user/system CPU, read/write bytes, database growth, and all temporary
storage. Do not relabel excluded costs as optimization.

### 12. Existing durability policy

The Store currently uses `journal_mode=MEMORY` and `synchronous=OFF`. Packing
does not create that policy and must not claim stronger power-loss durability
than it provides. Final publication must still be transactionally invisible or
complete during normal process execution, and all reopened bytes remain
content-authenticated.

## Concurrency model

Retain one Store per project. Multiple Branches and Workspaces share that
Store; multiple projects have independent Store connections and operation
gates. The first pack implementation has:

- at most one genesis pack per project;
- one producer and one SQLite caller, not a pool;
- no concurrently mutable pack;
- no same-Branch lease change;
- no Commit staging or CAS change;
- no cross-project global pack index; and
- no server requirement beyond routing each project to its Store.

## Admission gates

Do not reactivate this candidate until direct streaming has a retained 100,000-
file result. Once reactivated, admit production work only if a prototype proves
all of the following:

- at least 200 MB/s effective logical initialization throughput at 100,000
  files;
- at least 30% lower initialization wall time than the admitted direct-stream
  path;
- no larger final durable Store than the current representation;
- complete peak-disk, freelist, temporary-I/O, CPU, and RSS accounting;
- no material representative read or Workspace-create regression;
- exact loose/packed deduplication and collision behavior;
- exact reopen identity and canonical-root equality;
- no visible partial LayerStack under injected failure;
- deterministic BUILDING-pack cleanup;
- unchanged Workspace Commit staging, reconciliation, and Branch CAS; and
- an explicit v4 compatibility or migration decision.

Reject or continue deferring the design when any required result is
unavailable. Do not weaken authentication, durability claims, schema custody,
or benchmark boundaries to make the candidate pass.

## Explicit non-goals

- no implementation as part of the current namespace admission issue;
- no persistent external pack directory or sidecar;
- no one-giant-BLOB append protocol;
- no custom packed index before compact locators are measured;
- no background worker or worker pool;
- no packing of ordinary Workspace Commits;
- no automatic compaction, garbage collection, or `VACUUM`;
- no canonical-format, `ObjectId`, root, SDK, CLI, daemon, or FUSE change;
- no claimed throughput, RSS, or storage win without retained measurement; and
- no release scheduling merely because this candidate is documented.

## Files to read if reactivated

- [Roadmap architecture](../architecture.md)
- [Namespace optimization specification](../0.1/0.1.1/namespace-optimization-spec.md)
- [Store-footprint candidate](../0.1/0.1.2/store-footprint-efficiency.md)
- [Current v4 schema](../../../crates/layerfs-layerstack-store/sql/schema/v4.sql)
- [Schema validation and SQLite configuration](../../../crates/layerfs-layerstack-store/src/schema.rs)
- [Canonical object admission and reads](../../../crates/layerfs-layerstack-store/src/objects.rs)
- [LayerStack initialization](../../../crates/layerfs-layerstack-store/src/layerstack.rs)
- [Workspace snapshot reads](../../../crates/layerfs-layerstack-store/src/workspace.rs)
