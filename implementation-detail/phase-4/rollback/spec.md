# Phase 4 Rollback to SQLite and Core/Engine Optimization Specification

- Status: controlling rollback direction; implementation pending
- Date: 2026-08-17
- Branch: `codex/empty-worktree`

## 1. Authority and meaning of “rollback”

This document is a Phase 4 addendum. It controls the active direction for:

- retiring the two rejected pack experiments;
- retaining SQLite as the authoritative durable engine;
- freezing the missing durable Phase 3 logical-object mapping;
- measuring and optimizing the shared CAS, CDC, COW, canonical-object, and
  storage-integration path; and
- deciding whether a third storage backend is justified later.

Where this document conflicts with an active Phase 4B implementation or
promotion instruction, this document controls. `../storage/sqlite/spec.md` continues to
control SQLite correctness, durability, and resource requirements where it
does not conflict with this addendum. Existing Phase 2 and Phase 4B reports
remain historical evidence; they are not active implementation authority.

“Rollback” here means removing rejected source-code experiments and returning
the active architecture to the last qualified SQLite direction. It does not
mean adding a user-visible rollback API, checkpoint API, storage rollback
feature, or migration system.

## 2. Decision

Phase 4 adopts the following direction:

1. Remove the Phase 4B append-only/carrier engine from active production code.
2. Remove the Phase 2 `PackedInMemoryCas` experiment from active core code.
3. Keep SQLite as the only authoritative durable engine.
4. Keep a bounded in-memory implementation as the semantic reference and
   performance ceiling, not as a durability substitute.
5. Freeze and implement the real Phase 3 durable logical-object mapping before
   making full-workload create/edit performance claims.
6. Benchmark the same logical workload through Memory and SQLite at 1, 10, and
   100 MiB, including new files, small edits, large edits, publication,
   reopen/authentication where applicable, and range verification.
7. Optimize shared core work first where measurements show it dominates, then
   optimize SQLite statement and transaction batching.
8. Add no third engine until measurements identify a backend-specific limit or
   an approved remote-storage requirement that SQLite cannot satisfy.

The earlier “Rust + SQLite” experiment is not a third database. It is the
current Rust implementation using SQLite. This specification therefore defines
two active benchmark lanes, not three invented implementations:

- Memory: semantic reference and upper-bound diagnostic;
- SQLite: durable production candidate and Phase 4 authority.

## 3. Evidence behind the decision

The decision is based on the following measured or structural evidence.

| Candidate | Evidence | Decision |
| --- | --- | --- |
| Phase 2 `PackedInMemoryCas` | Corrected 100 MiB comparisons were effectively parity: about 0.09% to 0.94% faster in selected pre-sized rows, below the 5% promotion threshold; a non-pre-sized row was about 4.77% slower. | Delete the implementation. Preserve the report. |
| Phase 4B append-only carrier | The first diagnostic used one locator per collision page, produced about 55,240 index-page reads for about 5,363 lookups, flushed after the 32-page cache filled, and had about 4.02x reported reopen read amplification. Its original empty-root benchmark was not promotion-valid. | Do not optimize or promote this layout. Delete the implementation. |
| Later same-source carrier proxy | Five measured rows per lane produced a median of 31.596630 MiB/s for append-only and 35.290276 MiB/s for a deliberately conservative SQLite control. Append-only was 11.69% slower in wall time. Both lanes explicitly lacked frozen Phase 3 semantic persistence and therefore were non-promotion diagnostics. | Treat as supporting no-go evidence, not as a final full-logical-workload result. |
| SQLite | It remains the only qualified durable implementation and already provides transactions, indexing, recovery, and one durable publication boundary without a custom carrier format. | Retain as Phase 4 authority and optimize only from fair measurements. |

The same-source proxy used these frozen observations:

- source bytes: 104,857,600;
- raw BLAKE3:
  `0855eedd9498bf31a1eafb5a2f00bf84f646db5153cc86632fcb0cc0e180fb36`;
- logical-v1 BLAKE3:
  `52ce153eab81e33a0243a25a47a8805a86ba9bec125a27bee3c50de647cdafbc`;
- expected historical SHA-256:
  `27f82e57f589b7ed79f28a8cef02acd2db82682fbccb35cdd6b48a136d98a7d6`;
- 4,801 CDC chunk occurrences and 263 unique chunks;
- 4,803 object submissions, 265 creations, and 4,538 reuses in the proxy graph.

Those rows do not authorize a 200 or 300 MiB/s claim because the current core
does not yet persist the complete Phase 3 logical model.

## 4. Goals

The immediate goal is an honest, reproducible integration boundary between:

```text
CAS + CDC + COW + canonical identity
                    |
                    v
              storage engine
```

The durable SQLite lane should reach, for the qualifying 100 MiB new-file
capture row:

- minimum target: 200 MiB/s, equivalent to at most 500 ms;
- stretch target: 300 MiB/s, equivalent to at most 333.333 ms.

The timed boundary must include all required logical work and exactly one
durable publication. A memory result may establish the shared-core ceiling but
cannot satisfy a durability target.

Small-edit rows are latency and work-amplification tests, not throughput claims.
They must demonstrate that work scales with the changed region and required COW
spine except where a frozen format or authentication rule requires more work.

## 5. Scope

This addendum includes:

- deletion of the two active pack experiments;
- preservation of historical evidence describing them;
- a frozen, versioned Phase 3-to-Phase 1 persistence mapping;
- the Memory and SQLite semantic engine lanes;
- a fair create/edit benchmark matrix;
- core-path measurement and optimization;
- SQLite batching and query-plan measurement;
- a storage-backend compatibility audit; and
- a final Phase 4 decision record.

## 6. Explicit non-goals

This work does not include:

- native materialization, projection, clone/reflink, FUSE, or VFS work;
- cloud database deployment or network synchronization;
- PostgreSQL, Redis, RocksDB, another KV database, or another custom engine;
- a generic provider registry, connection pool, factory, plugin system, or
  configuration framework;
- WAL mode, GC, compaction, repacking, carrier rotation, or checkpoint APIs;
- an async runtime, Rayon, hidden workers, retry storms, or hidden queues;
- changing the frozen CDC profile, Phase 1 canonical-object bytes, or hash
  domains merely to improve a benchmark; or
- rewriting historical documents to make rejected experiments disappear.

Native materialization remains owned by the later materialization/projection
phase. This Phase 4 work measures authenticated logical reconstruction and
exact range reads only.

## 7. Deletion contract

### 7.1 Phase 4B append-only engine

Remove active code and dependencies that exist only for the append-only
carrier, including:

- the append-only engine module and its public exports;
- carrier frames, marker/index/page-cache implementation, carrier-specific
  counters, fault hooks, and tests;
- append-only benchmark lanes and binaries;
- carrier-only dependencies when no remaining caller uses them; and
- active documentation links that present Phase 4B as a pending production
  implementation.

Do not add a reader, migration utility, cleanup tool, or compatibility layer
for the experimental carrier. It was never promoted as a production format.
Existing experimental carrier files are not SQLite data and are outside the
supported storage contract.

### 7.2 Phase 2 packed in-memory CAS

Remove active code that exists only for `PackedInMemoryCas`, including:

- the implementation and packed locator types;
- packed-only constructors, helpers, counters, tests, and benchmark modes; and
- packed-specific content/COW entry points when no non-packed caller uses them.

Retain `InMemoryCas` and the ordinary semantic paths required by current core
tests and the Memory benchmark lane.

### 7.3 Evidence preservation

Keep the Phase 2 packed reports, Phase 4B specifications, ledgers, benchmark
reports, and finding documents as historical records. Add a clear rejected or
superseded status where an active reader could otherwise mistake them for the
current direction. Do not alter frozen result values.

### 7.4 Deletion acceptance

After deletion:

- no production Rust target may compile or expose the append-only carrier;
- no production Rust target may compile or expose `PackedInMemoryCas`;
- no dependency may remain solely for either experiment;
- SQLite data and its on-disk compatibility remain unchanged; and
- default and all-feature builds must not contain a hidden feature that
  resurrects either candidate.

## 8. Preserved semantic and resource contracts

The following remain non-negotiable:

- Phase 1 canonical object encoding and object identity;
- Phase 2 CDC profile, boundary behavior, and fragmentation independence;
- Phase 3 COW, tree, root, and delta semantics;
- immutable authenticated CAS behavior;
- authentication before reuse;
- exact bounded range reads;
- typed first-cause and dominant-cause error behavior;
- atomic root/delta capture publication;
- one writer and synchronous caller-thread operation;
- streaming source processing with no source-sized staging buffer;
- bounded object/reference metadata and no unbounded ID map or object cache;
- one durability-equivalent commit per SQLite capture; and
- reopen verification against the published root and delta.

Two receipt classes are distinct. A bounded authenticated snapshot receipt may
prove complete closure validation for one exact store authority, integrity
epoch, mapping profile, generation, root, and transition; it may cover an
unchanged subtree, but never authenticates bytes fetched later or incumbent
equality. A future operation-local duplicate-read shortcut must instead bind
the exact store authority, epoch, profile, generation, root and transition,
object, locator or row identity, and byte range, with explicit count/byte
bounds and eviction. A cached key alone is not proof of authenticated payload
equality.

## 9. Durable Phase 3 logical-object mapping

### 9.1 Current blocker

The current core is not yet a frozen durable Phase 3 representation:

- `LogicalFile` is explicitly unencoded and is coupled to `InMemoryCas` for
  construction/authentication;
- `TreeNode` identity is explicitly provisional, not a stable object format;
- only `Object::Bytes` and `Object::Directory` have frozen Phase 1 encodings;
- a logical chunk reference needs the raw `ChunkId`, raw length, and canonical
  Bytes-object identity, which are not interchangeable identifiers;
- Phase 3 `Delta` embeds tree nodes and has no durable codec; and
- the engine's `DeltaRecord.payload` is currently opaque.

No benchmark may be called a full logical create/edit workload until this
mapping is frozen and round-tripped through both active lanes.

### 9.2 Required mapping properties

Before implementation, a subordinate mapping record must freeze exact bytes,
limits, and golden vectors for:

- format domain and version;
- file versus directory node kind;
- all persisted Phase 3 metadata;
- ordered file chunk references containing raw `ChunkId`, raw length, and the
  canonical `Object::Bytes` object ID;
- canonical directory entry names, ordering, and child references;
- root identity and parent relationship;
- delta operations and their before/after identities;
- strong-edge closure rules;
- maximum object, page, reference, depth, and decoded-allocation bounds;
- exact EOF, malformed, overflow, unknown-version, and identity errors; and
- deterministic encoding independent of Rust layout, allocator, platform, and
  iteration order.

Use the existing Phase 1 `Object::Bytes` and `Object::Directory` vocabulary
unless the mapping review proves that a new Phase 1 object kind is necessary.
Do not add a new object kind casually. If manifests require paging to preserve
bounded memory, the page format and root linkage must be frozen in the same
mapping record before code changes.

### 9.3 Mapping acceptance

The mapping qualifies only when:

- fixed golden bytes and object IDs cover empty, one-chunk, multi-chunk, nested
  tree, metadata, root, and delta cases;
- encode-decode-encode is byte-identical;
- Memory and SQLite produce identical object IDs, root IDs, delta bytes, and
  reconstructed source bytes;
- reconstruction and exact range reads do not require `InMemoryCas` authority;
- malformed, truncated, oversized, reordered, duplicate, and trailing bytes
  return exact typed errors; and
- a reopen can recover and authenticate the same logical root and delta without
  process-local state.

## 10. Active engine lanes

### 10.1 Memory lane

The Memory lane is a bounded semantic reference and performance ceiling. It
must execute the same CDC, canonical encoding, object identity, COW, root,
delta, closure, and range semantics as SQLite.

It must report durability and process-reopen work as `NotApplicable`, never as
zero-cost durable success. A fresh in-process reconstruction may be timed as a
separate semantic verification row.

### 10.2 SQLite lane

SQLite remains the durable production lane. Preserve its required durability
profile. `../storage/sqlite/visible-head.md` is the only authorized
schema exception: after WP4-P selects one mapping profile, WP7 may implement
its single version-2 complete-visible-head schema and its exact version-1
handling. No other migration or multi-profile production format is authorized.

Each capture must use:

- one writer;
- one transaction;
- bounded prepared-statement reuse;
- immutable insert-or-authenticate-reuse behavior;
- complete root/delta publication in that transaction;
- one durability-equivalent commit; and
- authenticated reopen and range verification.

“Batched” means reducing statement preparation, round trips through the SQLite
API, and transaction boundaries. It does not mean constructing an unbounded
source-sized SQL statement or skipping per-object identity/authentication.

### 10.3 No third engine yet

Do not create an engine trait, factory, or third implementation merely to
anticipate a future database. The smallest shared semantic port may be extracted
only when the Memory and SQLite implementations already need it.

A third engine becomes eligible only when one of these is true:

- the optimized SQLite row misses the target and counters attribute the miss to
  SQLite-specific work rather than shared core work; or
- a separately approved remote-storage milestone requires networked authority.

## 11. Benchmark contract

### 11.1 Dataset matrix

Use pre-generated deterministic single-file fixtures of:

- 1 MiB;
- 10 MiB; and
- 100 MiB.

Freeze each fixture's length, raw fingerprint, logical fingerprint, CDC chunk
sequence fingerprint, canonical object IDs, and expected root/delta identities
in a machine-readable manifest before measuring.

The minimum operation matrix is:

| Operation | 1 MiB | 10 MiB | 100 MiB | Primary measure |
| --- | ---: | ---: | ---: | --- |
| New file capture | required | required | required | full wall and MiB/s |
| Unchanged recapture | required | required | required | reuse/auth latency |
| One-byte middle replacement | required | required | required | latency and bytes revisited |
| 4 KiB middle replacement | required | required | required | latency and CDC/COW reuse |
| 1 MiB middle replacement | not applicable | required | required | latency and work amplification |
| Full replacement | required | required | required | full wall and MiB/s |

Retain the broader frozen Phase 2/3 edit-shape rows—prepend, append, truncate,
EOF edit, and scattered edits—as regression coverage. The 1/10/100 matrix is
the fast Phase 4 optimization loop; it does not erase the existing 16/100/512
MiB scaling rows.

### 11.2 Equal work

For a comparable Memory/SQLite row, both lanes must use the same:

- pre-generated source and fingerprint;
- source read and timer boundary;
- CDC output and chunk strategy;
- canonical bytes and object IDs;
- object creation/reuse decisions;
- full persisted logical file/tree/member graph;
- root and delta identities;
- closure traversal semantics;
- exact reconstruction bytes; and
- range probes, including at least one cross-chunk-boundary probe.

SQLite additionally includes its one transaction commit, close, genuine reopen,
and authenticated reopen verification. Those costs must remain visible, not be
subtracted to imitate Memory.

### 11.3 Timer boundary

Fixture generation, fixture fingerprint preflight, and empty-store preparation
occur outside the timer.

The SQLite headline timer starts immediately before reading the prepared source
for capture and ends only after:

1. CDC and canonical object creation/reuse;
2. complete logical COW/root/delta construction;
3. closure validation;
4. one transaction commit;
5. close and reopen;
6. authenticated root/delta/closure verification;
7. streamed source reconstruction; and
8. exact range verification.

The Memory headline uses the same start and semantic end, but labels omitted
durability/process-reopen work `NotApplicable`. It is a ceiling diagnostic, not
a fair durable winner.

Also record additive phase timers so optimization can distinguish shared core
work from engine work. Phase timers must nest within, not replace, the headline
wall timer.

### 11.4 Campaign rules

For each promotion row:

- run one untimed correctness/warmup iteration;
- run at least five unchanged measured iterations;
- report median, minimum, maximum, and spread;
- run and label warm-cache and controlled cold-cache campaigns separately;
- never infer cold APFS state from reopening a file;
- run no unrelated Cargo/compiler workload during the campaign; and
- preserve raw machine-readable rows.

### 11.5 Mandatory observations

Report values or explicit `Unavailable`/`NotApplicable` states for:

- wall time and MiB/s;
- source-read, CDC, canonical encoding, hashing, existing-value authentication,
  COW, closure, SQL, commit, reopen, reconstruction, and range time;
- source, encoded, hashed, authenticated, compared, read, and written bytes;
- chunk/object/node submissions, creations, reuses, and closure visits;
- SQL statements by operation, rows examined/changed, prepared-statement
  reuse, transactions, commits, syncs, busy/locked events, and query plans;
- logical memory high-water, allocator/RSS observation, and bounded-cache
  high-water;
- SQLite database, journal, temporary, logical, apparent, and allocated bytes;
- physical read/write observations when the host exposes them; and
- source/store cache conditioning and observed state.

Unavailable physical or cache observations must not be replaced with logical
bytes or zero.

## 12. Optimization order

Optimization follows measured cost, subject to this default order:

| Order | Area | Allowed direction | Required evidence |
| ---: | --- | --- | --- |
| 1 | Benchmark/mapping correctness | Freeze real persistence and equal work first. | Golden identities and full round trips pass. |
| 2 | Repeated authentication and closure | Reuse bounded exact receipts within the same immutable snapshot; fuse duplicate traversals where semantics permit. | Fewer authenticated bytes/visits with unchanged tamper detection. |
| 3 | SQLite batching | One transaction, prepared-statement reuse, bounded existence/read batches, bounded insert execution, one root/delta publication. | Fewer statements/API crossings with identical outcomes. |
| 4 | Canonical encode/hash/write path | Stream once where possible, reuse caller-owned bounded buffers, and avoid duplicate canonical vectors or hash passes. | Lower copied/encoded/hashed bytes and memory high-water. |
| 5 | COW edit locality | Rebuild only the required changed region and authenticated tree spine. | Small edits reduce created nodes and bytes revisited. |
| 6 | CDC mechanics | Optimize buffer and scan mechanics without changing boundaries or IDs. | CDC fingerprint remains identical and CDC time falls. |
| 7 | SQLite schema/index/query plan | Change only a proven slow query or index; preserve compatibility or specify migration. | Query-plan and statement timing identify the exact win. |

Do not optimize one micro-phase by moving equivalent work outside its timer or
into reopen. Do not weaken immutable reuse authentication, closure validation,
root/delta semantics, or durability.

## 13. Storage-backend compatibility audit

The audit is semantic, not an instruction to build adapters. For Memory and
SQLite, document how the following operations are satisfied:

- immutable conditional put and authenticated incumbent reuse;
- bounded batch existence/authentication/read operations;
- exact object and range reads;
- one-writer capture ownership;
- atomic root and delta publication;
- snapshot/generation identity across reopen;
- compare-and-publish conflict behavior;
- durability acknowledgment;
- typed missing, malformed, conflict, permission, short-I/O, capacity,
  cancellation, and ambiguous-durability errors; and
- bounded memory, request count, and retry behavior.

For a possible remote engine, the audit must additionally identify:

- RPC count and round-trip amplification per capture;
- bounded request and response batch sizes;
- idempotency keys and retry ownership;
- transaction or conditional-write semantics for root/delta publication;
- server-side versus client-side authentication responsibility;
- consistency and stale-read behavior;
- timeout/cancellation propagation; and
- local cache authority and invalidation rules.

Do not expose raw SQL as the shared LayerFS engine contract. Do not assume that
a local method call can become a remote call without batching and consistency
changes.

## 14. Acceptance gates

### Gate A: rejected code is gone

- append-only and packed in-memory production code are absent;
- no experimental dependency remains without a caller;
- historical evidence is retained and labeled;
- SQLite compatibility and existing core semantics pass unchanged.

### Gate B: persistence is real

- the frozen mapping and golden vectors are approved;
- Memory and SQLite produce/reconstruct identical full logical roots and
  deltas, and SQLite persists them across reopen;
- no provisional node identity or benchmark-only proxy is used in a result;
- range reads and reopen verification authenticate the persisted graph.

### Gate C: benchmark is qualified

- all 1/10/100 create/edit rows pass exact output gates;
- five measured iterations and cache conditioning are reported;
- timer boundaries, memory, storage, SQL, and work-amplification counters are
  complete or explicitly unavailable;
- source generation remains outside timing;
- no result omits closure, publication, or required reopen work.

### Gate D: optimization is justified

Each optimization must show a median improvement on an unchanged qualifying
row, retain correctness gates, and identify the decreased direct counter. A
change that only moves work, depends on an invalid cache state, or improves a
microbenchmark while regressing the full row is rejected.

### Gate E: final Phase 4 decision

The final decision must be one of:

1. SQLite reaches at least 200 MiB/s on the qualified 100 MiB durable new-file
   row; retain it and continue shared-core/SQLite optimization toward 300 MiB/s.
2. SQLite remains below 200 MiB/s, but the measured remaining cost is shared
   core work; retain it and optimize the shared path before considering a new
   engine.
3. SQLite remains below 200 MiB/s and a backend-specific cost is the proven
   dominant limit; authorize a separate specification for one named third
   backend.

Memory throughput alone cannot select outcome 1. A third backend is not
authorized by this specification.

## 15. Required deliverables

The phase produces:

- this controlling specification and its implementation plan;
- a deletion record for both rejected experiments;
- the exact Phase 3 persistence mapping record and golden vectors;
- the two-lane create/edit benchmark and frozen fixture manifest;
- raw benchmark JSONL and a summarized report;
- an optimization ledger tying each change to counters and full-row results;
- the storage-backend compatibility audit; and
- a final Phase 4 decision record stating whether 200 and 300 MiB/s were
  reached and what authority remains.
