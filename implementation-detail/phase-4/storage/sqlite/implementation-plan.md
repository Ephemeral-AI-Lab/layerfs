# Phase 4 Implementation Plan — SQLite Durable Engine

Status: planned
Controlling specification: [spec.md](spec.md)
Required target: Phase 4A SQLite BLOB baseline
Conditional target: Phase 4B append-only carrier/index only after measurement;
see [../append-only/spec.md](../append-only/spec.md)

The plan is deliberately staged so that persistence and performance risks are
visible before VFS or SDK work begins. Each stage has two sections:

1. what to implement;
2. what to test.

Do not begin Phase 4B while Phase 4A is moving. Phase 4A is the reference
implementation; Phase 4B is a candidate carrier selected by evidence.

## 0. Working rules and stop conditions

### What to implement

- Work only in the restart repository at
  /Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty.
- Preserve the existing Phase 1 canonical bytes, BLAKE3 IDs, Phase 2 CDC
  profiles, and Phase 3 COW/delta behavior.
- Keep the implementation synchronous and caller-thread based.
- Keep SQLite private to layerfs-engine.
- Add one direct SQLite binding only; no ORM, provider registry, or generic
  storage trait with unused implementations.
- Keep Phase 4A and Phase 4B separate in the ledger and benchmarks.
- Stop and resolve any conflict between a current phase specification and an
  older historical note before writing code.

### What to test

- Inspect the clean/dirty worktree before every implementation checkpoint.
- Verify the workspace manifest and current crate boundaries.
- Run cargo metadata before adding a dependency.
- Confirm that no existing core caller expects a raw database connection.
- Record the source fingerprint and toolchain for the Phase 3 reference
  baseline.

## 1. Persistence-readiness boundary

### What to implement

Define the smallest engine-facing records and conversions:

~~~text
ObjectRecord  -> authenticated canonical object
RootRecord    -> root ID plus authenticated root object reference
DeltaRecord   -> delta ID plus authenticated parent/child/root transition
~~~

Implement or document the conversion from Phase 3 in-memory values to these
records without serializing Rust memory layout. Reuse the existing canonical
object encoder and BLAKE3 identity owner. If delta bytes need a private
persistence encoding, define it once at this boundary and keep it separate
from object canonical bytes.

Do not add path search, generic metadata, object caches, or a second tree
format. If a required Phase 3 value cannot yet be represented durably, fix
that narrow gap before adding SQLite tables.

### What to test

- Round-trip each record through its persistence representation.
- Recompute every object ID from its canonical bytes.
- Verify that root and delta references point only to authenticated objects.
- Verify deterministic bytes and IDs across two processes or two fresh store
  instances.
- Reject malformed, truncated, oversized, and mismatched records.
- Confirm that the existing Phase 1 and Phase 3 tests still run with a
  nonzero test count.

Exit condition: the engine can receive durable semantic records without
depending on Rust debug layout or a raw Phase 3 memory pointer.

## 2. Engine skeleton and SQLite profile

### What to implement

Create the minimum engine package:

~~~text
crates/layerfs-engine/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── error.rs
│   ├── transaction.rs
│   └── sqlite.rs
└── tests/
    └── sqlite_engine.rs
~~~

Split sqlite.rs into sqlite/connection.rs, schema.rs, objects.rs, roots.rs,
deltas.rs, or capture.rs only when the file becomes difficult to review.
Avoid creating empty modules “for later”.

Implement:

- open/create;
- schema creation and format marker;
- fixed profile:
  journal_mode=DELETE, synchronous=FULL, temp_store=FILE, mmap_size=0;
- effective-profile validation on reopen;
- bounded busy handling;
- typed engine errors;
- transaction wrapper with explicit commit/rollback behavior.

Use prepared parameterized statements. Do not expose the connection outside
the engine package.

### What to test

- create a new store on APFS;
- reopen the same store;
- reject an incompatible schema/profile;
- verify WAL is not active;
- verify the database and rollback journal are in the expected location;
- prove a failed transaction does not expose uncommitted rows;
- prove busy/locked SQLite errors map to the intended typed error;
- run diff check, formatting, and package compilation with the chosen feature
  set.

Exit condition: an empty engine can open, validate, transact, close, and
reopen without leaking a connection or transaction.

## 3. Immutable object store

### What to implement

Add Phase 4A object operations:

- put an authenticated canonical object if absent;
- reuse an authenticated equal occupant;
- reject unequal occupants;
- reject malformed or tampered stored bytes;
- read an exact object range;
- expose direct counters for object puts, reuses, validations, bytes read, and
  bytes written.

Use SQLite BLOBs as the Phase 4A carrier. Batch multiple object publications
inside the capture transaction, but do not retain all canonical bytes or all
decoded objects for the whole capture.

Keep the object table keyed by the canonical object ID. Do not use SQLite row
IDs as identities. Do not overwrite an incumbent in place.

### What to test

- empty-object and boundary-size objects;
- canonical object sizes around 8 KiB, 16 KiB, 32 KiB, 1 MiB, and the current
  object limit;
- equal duplicate reuse;
- unequal duplicate rejection;
- tampered BLOB rejection after close and reopen;
- missing, malformed, permission, short-read, and constraint failures;
- exact range reads at offset zero, middle, end, empty range, and out of bounds;
- no application-sized full-object read on the range path;
- object and byte counters return to their baseline after each test.

Exit condition: immutable object persistence and bounded reads are correct
independently of roots, deltas, VFS, and SDK.

## 4. Root, delta, and atomic capture

### What to implement

Add:

- root lookup and authentication;
- delta persistence and lookup;
- current/visible-root publication;
- one capture transaction that publishes objects, delta, child root, and
  visible-root advancement in order;
- parent-root validation;
- explicit rollback on any pre-commit failure.

The commit order is:

~~~text
validate parent
  -> publish/reuse objects
  -> write delta
  -> write child root
  -> advance visible root
  -> commit
~~~

No native filesystem operation, callback, network call, or background worker
belongs in this transaction.

### What to test

- clean capture from an empty store;
- capture with reused and newly-created objects;
- root and delta reopen round-trip;
- parent mismatch rejection;
- injected failure after object publication;
- injected failure after delta publication;
- injected failure before visible-root advancement;
- injected failure during commit/close handling where the SQLite binding
  exposes that boundary;
- after every failure, the previous root is still visible and no partial
  child root is reachable;
- after a successful commit, every referenced object is present and
  authenticated;
- repeat the same capture and prove idempotent object reuse.

Exit condition: the storage engine has a load-bearing atomic publication path,
not merely independent insert/read tests.

## 5. Reopen, concurrency, and resource discipline

### What to implement

Add the smallest correct lifecycle and observation layer:

- close/reopen behavior;
- one-writer policy;
- bounded reader behavior;
- busy timeout or equivalent bounded policy;
- direct transaction, statement, object, byte, lock, and temp-file counters;
- explicit cleanup of statements, transactions, and temporary resources.

Do not add worker threads, Rayon, an async runtime, hidden retries, or a
general connection pool in this phase.

### What to test

- committed data survives close/reopen;
- uncommitted data does not become visible;
- one writer and bounded readers behave according to the documented policy;
- concurrent readers do not corrupt or change returned bytes;
- a writer conflict returns a bounded typed result;
- all resource counters return to baseline after successful and failing
  operations;
- no statement/transaction remains open after a panic-free failure path;
- cancellation/deadline behavior is not silently converted into a retry.

Exit condition: concurrency behavior is explicit and observable rather than
an accidental property of the SQLite binding.

## 6. Phase 4A benchmark harness and baseline

### What to implement

Extend the existing evaluation tooling with engine-only rows. Reuse the
deterministic datasets and APFS placement from
[the evaluation plan](../../../evaluation.md). Add the minimum
instrumentation needed to report:

- wall and CPU time;
- throughput and repeated latency;
- CDC bytes, chunk count, object count, and reuse count;
- SQLite transaction and statement counts;
- database, rollback-journal, temporary-file, and total engine bytes;
- peak RSS/PSS or explicit unavailable;
- correctness and reopen result;
- source/build fingerprint.

Required rows:

| Row | Workload |
|---|---|
| P4-I1 | 100 MiB full ingest into SQLite BLOB engine |
| P4-I2 | repeated 100 MiB ingest with authenticated reuse |
| P4-R1 | full read after close/reopen |
| P4-R2 | random bounded range reads after close/reopen |
| P4-E1 | one-byte edits at 16, 100, and 512 MiB logical sizes |
| P4-E2 | beginning, middle, and end edit positions |
| P4-C1 | one writer with bounded readers |
| P4-C2 | bounded concurrent readers |

Keep materialization B1–B3 and VFS end-to-end rows out of this harness. They
belong to the layer that owns them.

### What to test

- correctness before timing;
- at least three repetitions for timing rows, with median and spread;
- cold/open-after-close and warm/repeated-read labels;
- equal dataset, CDC profile, SQLite profile, release build, and APFS volume
  for every comparison;
- clean source fingerprint for the final evidence;
- nonzero benchmark/test counts;
- no claim that concurrent overlap is speedup without a throughput result;
- no claim that logical memory equals RSS/PSS.

Exit condition: the baseline can identify whether time is dominated by CDC,
canonical encoding/hash, SQLite BLOB writes, rollback journal, or APFS I/O.

## 7. Phase 4A decision gate

### What to implement

Before writing append-only carrier/index code, produce a short decision
record:

~~~text
Phase 4A baseline
  -> dominant measured cost
  -> target workload affected
  -> expected carrier/index benefit
  -> correctness/recovery/memory risks
  -> decision: defer the candidate or open Phase 4B
~~~

If SQLite BLOB/journal overhead is not the dominant cost, defer the candidate
and keep the BLOB implementation as the reference. If it is dominant, open the
smallest append-only carrier/index A/B experiment defined by
[../append-only/spec.md](../append-only/spec.md).

### What to test

- compare Phase 4A against the previous core-only baseline without changing
  the dataset or CDC profile;
- attribute the 100 MiB ingest total to CDC, CAS/object publication, SQLite
  statements/transactions, journal bytes, and commit;
- compare repeated ingest and one-byte edit behavior;
- verify that any proposed optimization targets the measured bottleneck;
- record a no-candidate decision when evidence does not justify another
  carrier/index.

Exit condition: Phase 4A is either accepted as the baseline or Phase 4B is
explicitly authorized by evidence. Do not carry two unmeasured designs.

## 8. Optional Phase 4B append-only carrier candidate

### What to implement

Only if Stage 7 triggers it, and only according to
[../append-only/spec.md](../append-only/spec.md):

- append canonical object frames, immutable disk-index pages, roots, deltas,
  and one commit marker to one aligned APFS store log;
- keep the complete object index out of memory and use only bounded page/cache
  state;
- publish the marker only after all referenced frames are authenticated;
- perform one measured durability sync per capture;
- preserve the exact Phase 4A engine operation surface;
- add crash/reopen and bounded range-read behavior.

Do not add pack compaction, free-space recycling, or GC until the append-only
candidate wins the A/B gate.

### What to test

- differential equality against Phase 4A for IDs, bytes, roots, deltas, and
  errors;
- duplicate reuse and unequal occupant rejection;
- exact range reads across pack offsets and boundaries;
- failure after pack append but before catalog commit;
- reopen after committed and uncommitted operations;
- APFS engine bytes, RSS/PSS, ingest, repeat ingest, range read, and edit
  benchmarks;
- the append-only candidate must win on the declared target workload without
  regressing correctness or violating memory/resource bounds.

Exit condition: choose one carrier for the next milestone and delete or keep
the candidate explicitly. Do not leave both as indistinguishable production
paths.

## 9. Verification checklist

Run at stable checkpoints, not after every edit:

~~~sh
cargo metadata --no-deps --format-version 1
cargo test -p layerfs-core --offline
cargo test -p layerfs-engine --offline --all-features
cargo check --workspace --all-features
cargo fmt --all -- --check
git diff --check
~~~

For benchmark evidence, record:

- exact command;
- nonzero test/benchmark count;
- source fingerprint;
- toolchain and release profile;
- APFS volume and dataset profile;
- SQLite effective settings;
- whether the tree was clean.

Do not mark Phase 4 complete from compilation alone, a zero-test command, a
single timing sample, or counters without wall-time evidence.

## 10. Expected Phase 4 folder shape

The minimum implementation shape is:

~~~text
layerfs-empty/
├── spec.md
├── implementation-plan.md
├── ../../../evaluation.md
├── architecture.md
├── crates/
│   ├── layerfs-core/
│   └── layerfs-engine/
│       ├── Cargo.toml
│       ├── src/
│       │   ├── lib.rs
│       │   ├── error.rs
│       │   ├── transaction.rs
│       │   └── sqlite.rs
│       └── tests/
│           └── sqlite_engine.rs
└── tools/
    └── layerfs-eval/
~~~

If sqlite.rs becomes too large, split only the owners that already have real
behavior:

~~~text
src/
├── lib.rs
├── error.rs
├── transaction.rs
└── sqlite/
    ├── mod.rs
    ├── connection.rs
    ├── schema.rs
    ├── objects.rs
    ├── roots.rs
    ├── deltas.rs
    └── capture.rs
~~~

Do not create pack, postgres, custom-engine, VFS, OS, SDK, worker, or search
folders in Phase 4 merely to reserve names.
