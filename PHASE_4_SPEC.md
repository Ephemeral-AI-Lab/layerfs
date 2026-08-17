# Phase 4 Specification — SQLite Durable Engine

Status: proposed
Required path: Phase 4A, SQLite BLOB baseline
Conditional path: Phase 4B, append-only carrier + disk-backed index + one
commit marker; see [PHASE_4B_APPEND_ONLY_SPEC.md](PHASE_4B_APPEND_ONLY_SPEC.md)
Platform for qualification: macOS on APFS

This specification turns the Phase 3 in-memory content, COW, and delta
semantics into a durable engine. Phase 4A is the required implementation and
the reference performance baseline. Phase 4B is not a second equal production
design: it is a measured follow-up that is allowed only if Phase 4A proves
that SQLite BLOB or journal behavior is the bottleneck. The append-only
candidate and its recovery/performance contract are specified separately so
that Phase 4A remains unchanged while the candidate is studied.

## 1. Goal

Build the smallest durable storage engine that can:

1. reopen an existing LayerFS store;
2. authenticate and reuse immutable canonical objects;
3. read complete objects or bounded byte ranges;
4. persist a delta and a new root atomically;
5. preserve the Phase 1 object format and Phase 3 COW/delta semantics;
6. provide enough counters and benchmark evidence to choose whether a pack
   carrier is justified.

The engine is a storage implementation. It does not become the owner of
content-addressing, CDC, COW, path semantics, or a projected filesystem.

## 2. Scope and non-goals

| In Phase 4 | Deferred |
|---|---|
| SQLite durable engine | PostgreSQL |
| SQLite BLOB object carrier | Custom database |
| Immutable object publication | Pack files, unless the 4A gate triggers 4B |
| Root and delta persistence | FUSE, overlayfs, native materialization |
| Exact object range reads | SDK and public endpoint design |
| Atomic capture transaction | Background workers and hidden queues |
| APFS benchmark and resource measurements | GC, compaction, and retention policy |
| Typed SQLite error mapping | WAL as a production option |

The phase must not add a generic database abstraction for hypothetical
backends. The engine boundary can be narrow and semantic today; a future
backend can implement the same semantics only when there is a measured need.

## 3. Frozen contracts from earlier phases

Phase 4 must consume, rather than redefine:

- canonical object bytes;
- object framing and decode limits;
- BLAKE3 object identity;
- CDC profile and chunk-size profiles, including the selected 8 KiB,
  16 KiB, and 32 KiB profiles;
- authenticated content-tree and COW behavior;
- delta meaning, ordering, and parent/child relationships.

The SQLite schema, row IDs, indexes, journal files, and pack locations are
implementation details. They must never become part of an object ID, root ID,
delta ID, or canonical byte sequence.

No SQL row may be trusted merely because its primary key matches a requested
ID. The engine must authenticate the stored canonical bytes before returning
them as an object.

## 4. Component boundary

~~~mermaid
flowchart LR
    SDK["layerfs-sdk"] --> VFS["layerfs-vfs"]
    VFS --> CORE["layerfs-core"]
    VFS --> ENGINE["layerfs-engine"]
    ENGINE --> CORE
    ENGINE --> SQLITE["SQLite file on APFS"]
    OS["layerfs-os"] -. host observations .-> VFS
~~~

### layerfs-core owns

- object encoding, decoding, and identity;
- CDC chunk boundaries;
- content-tree and chunk references;
- COW root and mutation semantics;
- delta construction and interpretation.

### layerfs-engine owns

- SQLite connection and transaction lifecycle;
- schema and migrations for the Phase 4 store;
- immutable object publication and authenticated lookup;
- root and delta records;
- bounded range extraction;
- atomic capture publication;
- typed storage errors and direct operation counters.

### layerfs-engine must not own

- a second CDC implementation;
- a second canonical encoder;
- path substring or full-text search;
- native filesystem projection;
- FUSE or overlayfs behavior;
- a public SDK;
- background compaction, GC, or a hidden worker pool.

## 5. Persistence-ready record boundary

Phase 3 values are useful in memory but their Rust memory layout is not an
on-disk format. Before the first SQLite write, define the smallest private
engine record boundary:

~~~text
logical root
  └── directory object
        └── file/content object
              └── chunk references
                    └── byte objects
~~~

The engine receives authenticated records in the following semantic form:

| Record | Required meaning |
|---|---|
| Object record | object ID, object kind, canonical length, canonical bytes |
| Root record | root ID, directory/content root reference, parent root reference |
| Delta record | delta ID, parent root, child root, ordered delta payload |

The exact Rust structs and private SQLite columns may differ. They must obey
these rules:

1. canonical bytes are produced by the existing core owner;
2. the engine verifies the ID against those bytes before publication;
3. an object is immutable after successful publication;
4. root and delta records refer only to authenticated object IDs;
5. reopening the database reconstructs the same semantic root and delta;
6. no debug representation, pointer, process-local sequence, or SQLite row ID
   is persisted as canonical content.

If a Phase 3 type does not yet have a stable persistence representation, add
that representation at this boundary before adding SQL tables. Do not solve
the gap by serializing Rust memory layout.

## 6. Required engine operations

The first public engine surface is intentionally small:

| Operation | Semantics |
|---|---|
| open | Open or create a store, apply the fixed SQLite profile, validate schema |
| load_root | Load and authenticate a named/current root |
| read_object_range | Read an exact byte range from one authenticated object |
| begin_capture | Begin one logical capture against an authenticated parent root |
| put_object_if_absent | Publish or reuse one immutable object without replacement |
| write_delta | Stage one authenticated delta record |
| commit_root | Publish one new root and make it visible atomically |

The API may use a capture guard or transaction object internally. It must not
expose a raw SQLite connection to core, VFS, or SDK callers.

### Object publication

For an object ID and canonical bytes:

1. validate the canonical bytes and recompute the BLAKE3 identity;
2. look up the object by ID using a prepared statement;
3. if absent, insert the bytes without replacement;
4. if present, authenticate and compare the stored bytes;
5. reuse only an authenticated equal occupant;
6. reject an unequal, malformed, replaced, or inaccessible occupant with the
   appropriate typed error.

The normal object path must not read or copy the object more than necessary.
Whole-object reads are allowed for validation; range reads must not silently
load an unrelated full object into an application buffer.

### Root and delta publication

A successful capture has one durable transaction boundary:

~~~text
validate parent
  -> publish/reuse all required objects
  -> persist the delta
  -> persist the child root
  -> advance the visible root
  -> commit
~~~

The visible root is advanced last within the transaction. A failure before
commit must leave the previous visible root valid and reopenable. A committed
root must refer only to authenticated durable records.

No callback, materialization, native filesystem operation, or unrelated
network operation is part of this transaction.

## 7. Phase 4A — SQLite BLOB reference baseline

Phase 4A uses SQLite as the durable catalog and the carrier for canonical
object bytes. It is the control implementation against which any pack
candidate is measured.

### SQLite profile

The effective settings must be applied and recorded at open:

~~~text
journal_mode = DELETE
synchronous  = FULL
temp_store   = FILE
mmap_size    = 0
~~~

WAL is disabled for this phase. The absence of a backup requirement is not a
performance claim; journal mode remains a benchmark variable for a later,
explicit experiment.

The implementation must:

- use one direct SQLite binding and prepared, parameterized statements;
- avoid an ORM and avoid string-built SQL values;
- use a bounded busy policy rather than an unbounded retry loop;
- keep the database and journal on the same APFS volume for qualification;
- record database, rollback-journal, temporary-file, and logical-engine bytes;
- validate the schema and profile on reopen.

### Minimum schema responsibilities

The physical schema is deliberately small. The implementation may choose
equivalent names and types, but it must provide these responsibilities:

| Responsibility | Required stored information |
|---|---|
| Store metadata | format/profile marker and schema version |
| Objects | object ID, kind/length metadata, canonical bytes |
| Roots | root ID and authenticated root object reference |
| Deltas | delta ID, parent/child root references, ordered payload |
| Visible root | current root/head reference, if not represented in metadata |

Do not add path indexes, search indexes, full-text search, generic metadata
tables, or speculative provider columns in Phase 4.

### Reads and memory

The engine must support:

- full object read for small object validation;
- exact bounded range read for large byte objects;
- reopening and reading without relying on process-local caches;
- a streaming or caller-supplied destination path for large extraction where
  the surrounding port supports it.

A decoded object record may occupy memory while it is being validated. That
does not authorize a source-sized in-memory ingest buffer. Source processing
must remain streaming at the core/content boundary, and the engine must not
retain every object of a large capture after its transaction step is complete.

SQLite's page cache and APFS cache are measured separately from application
buffers. Logical memory bounds are not the same as RSS/PSS; the benchmark
must report both where available.

### Concurrency

Phase 4 uses synchronous caller-thread operations. It must define and test:

- one writer transaction at a time;
- bounded reader behavior during a writer;
- busy/locked mapping;
- no hidden Rayon fan-out, worker pool, retry storm, or queue;
- exact counter baselines after each operation.

The initial engine may serialize writers with the smallest correct mechanism.
That is an explicit baseline, not evidence of final multi-writer throughput.

## 8. Phase 4B — conditional append-only carrier candidate

Phase 4B is allowed only after Phase 4A measurements show that small SQLite
BLOB writes, BLOB page churn, or rollback-journal amplification dominates the
target workload.

If triggered, the candidate follows
[PHASE_4B_APPEND_ONLY_SPEC.md](PHASE_4B_APPEND_ONLY_SPEC.md) and uses one
aligned append-only store log:

~~~text
store.log: object frames, immutable disk-index pages, roots, deltas,
           one commit marker per capture
~~~

The append-only log is an engine carrier and index, not a core algorithm. It
must preserve the same engine operations, object IDs, canonical bytes, error
semantics, and correctness tests. The candidate is rejected if it improves
ingest while worsening small-edit reuse, bounded reads, crash recovery, or
measured RSS beyond the Phase 4 budget.

No append-only carrier/index code is part of the Phase 4A acceptance gate.

## 9. Correctness invariants

The following are mandatory:

| Invariant | Required evidence |
|---|---|
| Immutable objects | equal reuse succeeds; unequal occupant fails; no overwrite |
| Authenticated reads | tampered bytes are rejected before semantic use |
| Stable identity | reopen returns the same IDs and canonical bytes |
| Root atomicity | failed capture never exposes a partial child root |
| Parent preservation | failed capture leaves the previous root unchanged |
| Delta linkage | every committed delta has valid authenticated parent/child refs |
| Range exactness | offset/length boundaries return exact bytes or typed bounds error |
| Idempotence | repeating the same capture reuses objects without duplication |
| Recovery | close/reopen preserves committed state and excludes uncommitted state |
| Resource discipline | connections, statements, transactions, temp files, and locks return to baseline |

Corruption, malformed rows, missing occupants, permission errors, short reads,
busy errors, constraint errors, no-space errors, and transaction failures must
retain typed distinctions. A generic “storage error” is not sufficient.

## 10. Measurement contract

All Phase 4 benchmarks run on macOS/APFS with the same machine, toolchain,
release profile, dataset generator, CDC profile, and SQLite settings.

### Required rows

| Row | Workload | Phase 4 meaning |
|---|---|---|
| P4-I1 | full ingest of a 100 MiB file | object publication and durable commit |
| P4-I2 | repeat the same ingest | authenticated reuse and transaction overhead |
| P4-R1 | full read after reopen | durable object extraction |
| P4-R2 | random bounded ranges after reopen | range-read path |
| P4-E1 | one-byte edit of 16/100/512 MiB logical files | persisted small-edit diagnostic |
| P4-E2 | edit at beginning, middle, and end | reuse and suffix/closure behavior |
| P4-C1 | one writer plus bounded readers | lock and visibility behavior |
| P4-C2 | bounded concurrent readers | read scalability without hidden fan-out |

P4-I1 through P4-R2 are required for the Phase 4A gate. P4-E1 through P4-C2
are required diagnostics and correctness evidence; their target is recorded
before optimization rather than guessed in advance.

### Metrics

Every row records:

- wall time, CPU time, throughput, and p50/p95 latency where repeated;
- logical input/output bytes;
- CDC bytes, chunk count, object count, and reused-object count;
- SQLite transaction and statement counts;
- database, rollback-journal, temporary-file, and total engine bytes;
- peak RSS/PSS or an explicit unavailable status;
- correctness result, reopen result, and source/build fingerprint.

Materialization B1–B3 and VFS end-to-end behavior are later-phase benchmarks.
Phase 4 must not claim native materialization throughput from these engine rows.

## 11. Phase 4 acceptance gate

Phase 4A is complete only when all rows below are true:

1. the engine reopens a validated store using the fixed SQLite profile;
2. object publication is immutable, authenticated, and idempotent;
3. exact range reads work without an application-sized full-object copy;
4. a capture publishes objects, delta, and root atomically;
5. failure injection proves no partial visible root;
6. typed SQLite errors and resource cleanup are directly tested;
7. P4-I1, P4-I2, P4-R1, and P4-R2 have repeatable APFS measurements;
8. the report separates application memory, SQLite cache, APFS cache, journal,
   and durable engine bytes;
9. the report names the bottleneck and records whether Phase 4B is triggered;
10. no deferred backend, pack carrier, VFS, or SDK work is hidden inside the
    Phase 4A implementation.

Phase 4B is a separate decision and acceptance gate. It is not required merely
because pack files may be useful in a future durable engine.

## 12. Explicitly deferred

- PostgreSQL or another storage provider;
- WAL and journal-mode tuning as a production change;
- append-only carrier/index unless the Phase 4A evidence triggers Phase 4B;
- carrier compaction, free-space recycling, repacking, and GC;
- FUSE, overlayfs, APFS clone/reflink, and native materialization;
- public SDK endpoint expansion;
- internal async workers or a general scheduler;
- filename substring search and full-text search.
