# Memory/SQLite semantic boundary after CP-0009

Status: research conclusion; no implementation authority beyond the exact WP5
handoff in this report

Date: 2026-08-21

Repository custody: `codex/empty-worktree` at committed HEAD
`febc20f046bba84ccdce1256363d77799eabf2db`, with the accepted CP-0007/8/9
dirty checkpoint package and user research preserved byte-for-byte.

## Executive disposition

The smallest sufficient design is **two concrete engine lanes sharing core
semantics, not a generic backend interface**:

```text
WP5: core-owned promoted codecs + authenticated traversal/reconstruction/range
     functions with one call-scoped complete-object loader callback
  -> WP6: concrete MemoryEngine with in-process atomic publication
  -> WP7: concrete SQLite Engine v2 with one durable COMMIT
  -> WP8: one benchmark binary with two direct lane branches
  -> WP9: one qualified parity/baseline campaign
```

The only shared call seam needed by WP5 is a call-scoped loader supplied to
core traversal code, conceptually:

```rust
load_complete: &mut impl FnMut(ObjectId) -> Result<CanonicalBytes, MappingError>
```

It is not a public trait, engine object, factory, registry, provider API, or
remote-ready abstraction. The two concrete lanes separately implement
immutable put/reuse, direct complete/range reads, capture ownership, and head
publication. They call the same core-owned codecs, closure qualification,
reconstruction, and logical-range algorithms.

H05 does not block this boundary. It is a private construction-witness
substitution that preserves the promoted v1 mapping, root, transition,
receipt, head, durability, and verification semantics. Its performance is
`Hypothesis(test needed)` and must not alter this semantic handoff.

## Evidence rules and authority

This report uses only these labels:

- `Observed(source/evidence)`: directly present in a cited source or accepted
  evidence artifact;
- `Derived(equation)`: follows from stated observed operands;
- `Hypothesis(test needed)`: requires a prospective test or campaign; and
- `Unavailable(reason/source)`: the named source cannot establish the fact.

`NotApplicable(...)` below is a lane result classification, not an evidence
label and never means measured zero.

| Authority | Fact used here | Evidence label |
|---|---|---|
| [CP-0009 report](../../../../implementation-detail/phase-4/test-checkpoint-report/cp-0009-dirty-b073a7e04c7a-current-product-baseline.md) and [analysis](../../../../implementation-detail/phase-4/test-checkpoint-report/cp-0009-dirty-b073a7e04c7a-current-product-baseline-analysis.json) | Current product control is 42/42 PASS under K64/F64 + DIR256K. The 100-MiB durable submit is 640.109209 ms; construction/proof/COMMIT medians are 504.215417/0.038542/135.855250 ms; same-count edit is 9.737250 ms; returned 1-MiB range is 3.285167 ms including routing and 3.171209 ms range-only; reopen/head is 3.007750 ms; every row returns Q to zero. | `Observed(evidence)` |
| [Current baseline manifest](../../../../implementation-detail/phase-4/baseline/current-baseline-v1-manifest.tsv) | The accepted binary, source, runner, raw rows, analysis, profile, and starting HEAD are fingerprinted; CP-0009 is a baseline, not a candidate comparison or promotion. | `Observed(evidence)` |
| [CP-0008 scale report](../../../../implementation-detail/phase-4/test-checkpoint-report/cp-0008-dirty-4f1c97f81f7c-count-change-scale.md) and [complexity sections 30-32](../../../../implementation-detail/phase-4/algorithm/complexity-analysis.md) | K64/F64 remains the current product profile under the accepted suffix-linear policy; same-open 500-MiB `+1` publication is 27.140916/15.102042 ms early/middle, while first-after-reopen authority is separately 1.262772/1.228564 s. WP4-P remains closed. | `Observed(evidence)` |
| [Rollback specification](../../../../implementation-detail/phase-4/rollback/spec.md), [implementation plan](../../../../implementation-detail/phase-4/rollback/implementation-plan.md), [algorithm specification](../../../../implementation-detail/phase-4/algorithm/spec.md), and [tests/benchmarks specification](../../../../implementation-detail/phase-4/algorithm/tests-and-benchmarks.md) | There are exactly two active lanes: Memory as semantic reference/core ceiling and SQLite as durable production authority. WP5-WP9 follow WP4-P. Factories, registries, pools, async, hidden workers, speculative third backends, and a public generic engine framework are forbidden. | `Observed(source)` |
| [Logical persistence mapping](../../../../implementation-detail/phase-4/mapping/logical-persistence.md) and [SQLite visible-head authority](../../../../implementation-detail/phase-4/storage/sqlite/visible-head.md) | The promoted mapping, exact receipt, complete visible-head tuple, schema-v2 shape, version-1 handling, errors, traversal order, resource rules, and publication reconciliation are frozen. | `Observed(source)` |
| [Decision map](../../decision-map.md) and [hypothesis ledger](../../foundations/hypothesis-ledger.md) | H05 is the next routed experiment, not evidence that a speedup exists and not authority to change the persistence boundary. | `Observed(source as routing context only)` |
| [Invariant matrix](../../foundations/invariant-matrix.md) and [benchmark/evidence method](../../foundations/benchmark-and-evidence.md) | Canonical identity, closure, one-COMMIT durability, error provenance, Q, and evidence classifications are protected; unsupported zeros are prohibited. | `Observed(source)` |

No Phase-4 H05, Cargo, Criterion, hyperfine, or other performance process was
observed by the guarded local checks before the report write. Therefore the
safety pause condition did not trigger. This statement does not claim that
H05 has run or passed.

## Current implementation reality

The present repository contains five materially different things. Conflating
them would make the future Memory lane either weaker than SQLite or falsely
durable.

### 1. Production `layerfs-core` semantics

The committed core already owns the required identity and logical vocabulary:

- Phase-1 canonical object encoding/decoding and complete-byte `ObjectId`
  authentication;
- frozen FastCDC and raw `ChunkId` behavior;
- Phase-3 `LogicalFile`, COW tree, root, mutation, and sequential delta
  semantics;
- selected K64/F64 file codecs, DIR256K directory codecs, canonical transition
  codecs, and the sole production mapping profile ID; and
- the exact 216-byte `ValidatedSnapshotReceiptV1` codec.

These are visible in [core exports](../../../../crates/layerfs-core/src/lib.rs),
[file persistence](../../../../crates/layerfs-core/src/content/persistence.rs),
[directory persistence](../../../../crates/layerfs-core/src/cow/persistence.rs),
[delta codec](../../../../crates/layerfs-core/src/delta/codec.rs),
[selected limits](../../../../crates/layerfs-core/src/limits.rs), and
[receipt validation](../../../../crates/layerfs-core/src/validation.rs).
`Observed(source)`.

The promoted constants are already singular, not runtime-selectable:

```text
FILE_LEAF_CAPACITY       = 64
FILE_BRANCH_CAPACITY     = 64
DIRECTORY_PAGE_CEILING   = 262,144
MAX_DELTA_PAGE_BYTES     = 8,388,608
mapping_profile_id       = b0ebb845409ef995a5fa454bb23d10a80c6ecf44deb7832ca2ce1213eb0f4ba1
```

`Observed(source/evidence)`.

The committed core is not yet the complete WP5 boundary. It has codec
primitives but no shared complete-object loader seam, no production full
closure/reconstruction/logical-range implementation over durable mapping
objects, and no core-owned Q lifecycle for those operations. The exact
ID-bearing missing-object and backend/resource error set frozen by the mapping
also exceeds the current `CoreError` surface in
[error.rs](../../../../crates/layerfs-core/src/error.rs).
`Observed(source)`.

### 2. Existing `InMemoryCas` and `LogicalFile` utilities

[InMemoryCas](../../../../crates/layerfs-core/src/cas/mod.rs) is a Phase-2 raw
chunk utility:

- it keys a `BTreeMap` by raw `ChunkId`;
- it authenticates `chunk_id(raw_bytes)`;
- it rejects values above the 32-KiB CDC maximum; and
- it has no root, delta, receipt, generation, visible head, capture guard, or
  atomic publication.

[LogicalFile](../../../../crates/layerfs-core/src/content/mod.rs) is explicitly
unencoded and currently:

- stores an eager `Vec<ChunkReference>`;
- records raw `ChunkId` and length but not the canonical Bytes-object ID;
- is constructed and range-read through a concrete `InMemoryCas`; and
- applies the current per-value 100,000-reference eager limit.

`Observed(source)`.

Therefore neither type is the future Memory semantic engine. They remain
useful Phase-2 content/CDC/COW utilities. Treating their existing range result
as Memory/SQLite semantic parity would omit the promoted file radix,
directories, root, transition, receipt, generation, closure, and publication
work. The current test named
`memory_and_sqlite_execute_the_same_authenticated_range` in
[phase4_engine_parity.rs](../../../../crates/layerfs-engine/tests/phase4_engine_parity.rs)
compares a raw `LogicalFile`/`InMemoryCas` slice with a SQLite canonical-object
slice; it is useful coverage but is not the WP6/WP7 full-graph parity gate.
`Observed(source)`.

### 3. Production SQLite `Engine`

The committed [SQLite Engine](../../../../crates/layerfs-engine/src/lib.rs) is
real production Phase-4A code and already supplies reusable concrete behavior:

- `DELETE` journal, `synchronous=FULL`, `temp_store=FILE`, `mmap_size=0`;
- synchronous access through one owned connection and one `BEGIN IMMEDIATE`
  capture;
- immutable canonical-object insert or complete incumbent authentication and
  equality comparison;
- complete object loads and authenticated canonical byte ranges;
- rollback-on-drop before successful COMMIT; and
- typed SQLite/storage counters and observations.

`Observed(source)`.

It is not yet the promoted WP7 engine. It still opens schema version 1, stores
only `visible_root`, stores parentage in `RootRecord` keyed by content root,
uses an ad-hoc opaque `DeltaRecord.payload` and `delta_identity`, and has no
generation/transition/216-byte-receipt complete head. Its capture compares
only an optional parent root. These are the exact gaps the schema-v2 and WP7
authorities require replacing. `Observed(source)`.

### 4. Benchmark-private CP-0009 `Store`

The dirty accepted
[phase4_create_edit_benchmark.rs](../../../../crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs)
contains a private `Store` and private `wp4m_*` schema. It implements much of
the target model for measurement:

- selected-profile canonical objects;
- store instance, validation authority, integrity epoch, and authority file;
- the complete `(generation, child, transition, receipt)` head;
- same-open carried authority and construction proofs;
- full and changed-spine qualification;
- root-first scrub, reconstruction, exact logical ranges, Q/W/D accounting;
- one SQLite transaction/COMMIT; and
- requested/prior/different/ambiguous post-dispatch reconciliation.

`Observed(source/evidence)`.

That `Store` is evidence and an extraction guide, not a production backend. It
is private to a 17k-line benchmark binary, uses benchmark-specific schema and
metrics, contains campaign and fault machinery, and is the accepted CP-0009
control surface. WP5 must not move it wholesale into core, and WP7 must not
rename it into production. Only its already-authorized semantics and the
smallest reusable algorithms belong in production owners.

### 5. Future responsibility split

| Package | Sole responsibility | Must not absorb | Evidence label |
|---|---|---|---|
| WP5 | Finish promoted core mapping; authenticate/decode/qualify/reconstruct/range through a call-scoped complete-object loader; exact mapping errors and Q/W/D. | Memory store, SQLite schema, engine trait, factory, benchmark/H05 logic. | `Derived(equation)` from the WP5 exit condition and current gaps |
| WP6 | Add one concrete in-process `MemoryEngine` and capture guard using the WP5 core path. | Durability claims, process reopen, SQLite-like API framework, third backend hooks. | `Observed(source)` from the implementation plan |
| WP7 | Upgrade the existing concrete SQLite `Engine` to the sole authorized v2 head/schema and promoted mapping. | General migration system, raw SQL contract, benchmark-private schema, cross-reopen shortcut without authority proof. | `Observed(source)` |
| WP8 | Call the two concrete lanes directly from one benchmark binary and emit the common semantic results plus lane-specific observations. | Backend registry, plugin API, async/pool, profile selector. | `Observed(source)` |
| WP9 | Establish qualified, isolated, alternating Memory/SQLite rows after WP5-WP8. | Relabeling CP-0009 or H05 rows as the two-lane baseline. | `Observed(source/evidence)` |

## Minimum shared semantic boundary

### Boundary shape: values and functions, not an engine trait

The shared boundary has three pieces only:

1. **Core-owned canonical values and algorithms.** Existing `ObjectId`,
   mapping codecs, role validators, root/delta semantics, receipt codec,
   closure traversal, reconstruction, range routing, Q/W/D, and mapping errors.
2. **One call-scoped complete-object loader callback.** Core traversal calls it
   by `ObjectId`; the concrete lane returns one bounded complete canonical
   object or an exact typed failure. Core then authenticates and role-validates
   those bytes before use.
3. **A few engine-private shared value structs.** `VisibleHead`, semantic put
   outcome, failure provenance, and row observations can be defined once in
   `layerfs-engine` because both concrete lanes live there. They need not be a
   public provider API.

The callback is sufficient because strong-edge closure, reconstruction, and
logical range routing all require complete canonical-object authentication.
A whole-object BLAKE3 ID cannot authenticate an unauthenticated backend byte
range. `Derived(equation)` from the identity preimage and mapping validation
order.

No custom `Engine` trait is required. WP6 calls core functions with a closure
over its in-memory object map; WP7 calls the same functions with a closure over
the existing SQLite concrete engine. WP8 uses an ordinary explicit CLI match
to call `run_memory(...)` or `run_sqlite(...)`. This is the first rung that
holds for two known lanes.

### Exact immutable object semantics

Both concrete lanes must implement the following behavior directly:

| Operation | Required semantic result | Memory concrete action | SQLite concrete action |
|---|---|---|---|
| put new canonical object | Verify complete canonical bytes, Phase-1 grammar, role when known, and `ObjectId`; insert without replacement; return `Created`. | Insert exact bytes in the bounded in-process object map owned by the active capture/store. | Insert exact kind/length/BLOB in the existing one-writer transaction. |
| put existing canonical object | A key hit is insufficient. Load complete incumbent; authenticate its ID; validate kind/length/expected role; byte-compare; return `Reused` only on equality. | Authenticate and compare the map occupant. | Authenticate and compare the incumbent BLOB after conflict. |
| complete get | Return one bounded complete canonical object only after ID authentication; preserve ID-bearing missing-object failure. | Map lookup plus complete authentication. | BLOB lookup/read plus complete authentication. |
| canonical byte-range get | First authenticate the complete canonical object, validate `start <= end <= canonical_length`, then return exactly the requested slice. | Authenticate the map value, then slice. | Authenticate the complete BLOB, then perform the bounded BLOB slice read; a partial read alone is not authority. |

`Observed(source)` for the immutable rules; `Derived(equation)` for the common
two-lane operation contract.

The core logical-range algorithm does not delegate tree routing to either
lane. It loads and authenticates every selected complete root/branch/leaf/chunk
object through the complete loader, checks raw length and raw `ChunkId`, and
emits only the requested raw slice. The direct canonical range operation is an
engine/storage API; the logical file range remains a core algorithm.

### Exact root, transition, receipt, generation, and head semantics

The shared semantic values are:

```text
RootId       = ObjectId(canonical directory wrapper)
DeltaId      = ObjectId(canonical transition index)

Transition   = Genesis { child }
             | Change { parent, child, ordered delta pages/entries }

VisibleHead  = {
  generation: u64,
  child: RootId,
  transition: DeltaId,
  validation_receipt: [u8; 216],
}
```

The following rules are identical in both lanes:

1. Root identity is content-only. Parentage is never stored in or hashed into
   the root.
2. Genesis has no Phase-3 parent/delta application. Change authenticates its
   exact parent, child, ordered entries, and replay.
3. Generation is checked `prior + 1`; the first published generation is one.
4. A receipt is issued only after the required complete or authorized
   incremental closure qualification succeeds. It binds store instance,
   validation authority, integrity epoch, generation, root, transition, and
   the sole promoted profile.
5. Capture compares the expected complete prior head, not only a root ID.
6. Root, transition, receipt, and generation become visible as one complete
   tuple or not at all.
7. The publication idempotency key is derived from the complete prior/request
   tuples, retained for exact ambiguous retry, and is not stored as a fifth
   head field.

`Observed(source)` from the mapping and visible-head specifications.

Memory uses a process-local random store instance and validation key. Its
receipt can authorize same-store/same-open incremental work, but it does not
turn the lane into persistent storage. SQLite persists the store identity,
validation-authority identity, and integrity epoch under the v2 schema while
keeping the protected key engine-private. Adversarial cross-reopen receipt
reuse remains unauthorized; default reopen performs a full scrub or returns
`ValidationAuthorityUnavailable`. `Observed(source)`.

### Capture, rollback, and exact error outcomes

The two concrete capture state machines share this semantic order:

```text
acquire one writer
  -> compare/authenticate expected complete prior head
  -> stage immutable puts/reuses
  -> build canonical file/directory/root/transition
  -> qualify required closure
  -> build exact receipt and requested head
  -> compare-and-publish exactly once
  -> release writer and all Q charges
```

The publication mechanics differ intentionally:

| Boundary | Memory | SQLite |
|---|---|---|
| Staging | Private in-process capture state. | One `BEGIN IMMEDIATE`-equivalent transaction. |
| Publication | One atomic in-process complete-head swap after semantic gates. | Stage complete head and issue exactly one `COMMIT` under DELETE/FULL. |
| Pre-dispatch failure | Prior head remains authoritative; discard staging or retain authenticated unreachable residue and report it honestly. | Prior head remains authoritative; rollback transaction; authenticated unreachable residue may remain where already acknowledged. |
| Post-dispatch uncertainty | Not a durability state: an in-process atomic swap either occurred or did not. A compare conflict remains typed. | Fresh read-only reconciliation classifies exact requested head, exact prior head, different complete head, or unresolved state. |
| Retry | Re-run only after a definite failure/conflict policy decision. | Only the byte-identical idempotency key may retry `AmbiguousDurability`. |

`Derived(equation)` from the common publication invariants and each lane's
mechanism.

Every failed public operation preserves the fixed-size provenance:

```text
FailureProvenance {
  first,
  cleanup_first,
  reconciliation,
  dominant,
}
```

The validation order, event order, and cleanup order determine `first`.
Cleanup never erases it. Before publication dispatch, `dominant=first`.
After SQLite COMMIT dispatch: requested head means success with the transport
cause retained only as diagnostic; prior head returns the original exact
cause; a different head yields `PublicationConflict`; unresolved authority
yields `AmbiguousDurability`. `Observed(source)`.

WP5 must close the current error-surface gaps without inventing a backend
framework. At minimum the durable mapping path must preserve an ID-bearing
`MissingObject(ObjectId)` and the frozen mapping/resource causes including
wrong role, identity/length/chunk mismatch, EOF/trailing bytes, noncanonical
partition/order, version/tag/discriminator, path/depth/cycle, Q/allocation,
capacity/permission/short-I/O, cancellation/deadline/I/O, receipt/authority,
publication conflict, and ambiguous durability. Lane-specific SQLite details
may wrap those causes, but generic `Io` may not replace a more precise cause.

### Closure, reconstruction, and ranges

These algorithms are shared core semantics and must not be reimplemented by
each engine:

| Operation | Exact shared behavior | Bound |
|---|---|---|
| Full closure/scrub | Iterative root-first DFS in frozen strong-edge order; authenticate complete bytes before role use; active-ancestry cycle detection; completed DAG occurrences may repeat; bounded edge spool, no unbounded visited set. | `Theta(authenticated canonical bytes + strong-edge occurrences)` |
| Receipt-backed incremental qualification | Authenticate prior and replacement changed spines; equal authenticated child IDs are prior-covered; fully traverse every new/different child; receipt must match exact store/epoch/profile/generation/root/transition. | Changed spines plus all new/different subtrees |
| Reconstruction | Traverse canonical file order; authenticate each selected mapping/chunk occurrence; check raw length and raw `ChunkId`; stream raw payload into a caller sink. | `Theta(output bytes + references + authenticated mapping bytes)` with bounded windows |
| Logical range | Authenticate file root; route with file-global inherited base and nonempty interval predicates; authenticate selected complete mapping/chunk objects; skip zero-length intervals; emit exact requested slices. | `O(F*B_v + K*L_v + C_v + returned bytes)` with K=F=64 |

`Observed(source/evidence)` for the algorithms and bounds.

The loader callback is called for complete canonical objects only. If SQLite
later uses a bounded multi-row read internally, that changes API crossings but
not callback semantics, authentication, ordering, or Q. A batch API is not
part of WP5 and is not required for WP6/WP7 correctness.

### Counters and evidence states

Use one concrete common semantic observation struct passed by mutable
reference or returned from core operations. Do not create an observer trait,
event bus, or pluggable metrics system. The minimum common fields are:

- object submissions, creations, reuses, complete authentications, canonical
  bytes authenticated/compared/written;
- raw source, CDC, raw hash, canonical encode/hash, COW/mapping, closure,
  reconstruction, range, and output bytes/work;
- mapping leaves/branches/pages, reference/edge occurrences, selected range
  path, rebuilt pages/spines, and unreachable residue/custody;
- visible-head reads, expected/requested generation, semantic publication
  count, receipt checks, full-scrub and incremental-qualification work;
- `Q` current/high-water/terminal, cumulative `W`, cumulative `D`, and bounded
  spool/output high-water; and
- exact `first`, `cleanup_first`, `reconciliation`, and `dominant` outcomes.

SQLite adds its existing concrete statement/query/row/BLOB, transaction,
COMMIT, sync, busy/locked, profile, database/journal/temp, and filesystem
observations. Memory adds in-process object count/stored canonical bytes and
atomic-swap count. RSS and host/filesystem cache/physical-I/O observations stay
separate from logical Q and bytes.

Fields must carry one of the evidence labels above or a semantic
`NotApplicable(...)` classification. A mechanism that exists but cannot be
observed is `Unavailable(reason/source)`, not zero. A mechanism absent from
the Memory lane is `NotApplicable(reason)`, not a measured fast path.

### Q, W, and D

Q is the peak simultaneously live mapping-owned or mapping-requested
allocation. W and D are checked cumulative work/output counters. The promoted
contract retains:

```text
MAX_DURABLE_LIVE_ALLOCATION = 1,073,741,824 bytes
ordinary fixed windows       = 33,604,696 bytes
terminal Q                   = 0 on every success and failure exit
```

`Observed(source/evidence)`.

The complete canonical input buffer, parser/decoded results, DFS frames,
spool window, streaming output, eager result when requested, receipts, and
backend buffers requested on mapping's behalf are charged exactly once.
Retained Memory store contents and SQLite page cache are storage/runtime state,
not transient mapping Q; report them separately. A large streamed W or D does
not consume resident Q. An eager result exceeding the remaining budget fails
before allocation with `AllocationBudgetExceeded`.

### Durability classification

| Observation | Memory lane | SQLite lane |
|---|---|---|
| Semantic capture/publication | Applicable: one writer and one atomic in-process complete-head swap. | Applicable: one writer transaction and one complete-head COMMIT. |
| Durability | `NotApplicable(in-process semantic reference; no persistent durability boundary)` | Applicable: DELETE journal plus `synchronous=FULL`, one durability-equivalent COMMIT. |
| Process reopen | `NotApplicable(nonpersistent store)` | Applicable: close every handle and construct a fresh engine from the durable path. |
| Fresh verification | Applicable: discard operation-local receipts/caches and use an independent reader/view over the authoritative in-process store; perform full scrub when required. | Applicable: authenticate v2 authority/head and perform the required full scrub or authorized path. |
| SQLite transaction/COMMIT/sync/journal/database bytes | `NotApplicable(no SQLite mechanism in this lane)` | Observed values or `Unavailable(reason/source)`; never inferred. |
| Crash/lost-ack reconciliation | `NotApplicable(no durable dispatch)` | Applicable: requested/prior/different/ambiguous complete-head classification. |

Memory may report a measured zero for a real semantic count such as objects
created in an unchanged capture. It may not report zero milliseconds, zero
commits, or zero durable bytes as if those were equivalent durable work.

## What is deliberately not shared

The following remain concrete implementation details:

- the Memory map and atomic-swap mechanism;
- SQLite connection, SQL, statements, BLOB handles, row IDs, schema, journal,
  transaction guard, busy policy, and filesystem observations;
- store/key custody and authority-file mechanics;
- benchmark fixture preparation, CLI parsing, row serialization, and campaign
  fault hooks; and
- any future network consistency, request batching, retries, or idempotency
  transport.

Sharing any of these now would create the forbidden generic framework without
reducing the semantic implementation required by two local lanes.

Explicitly rejected:

- public `Engine`/`Backend`/`Provider` traits for Memory and SQLite;
- factory, registry, plugin, configuration, profile-selection, or dependency-
  injection surfaces;
- connection pools, async functions/runtimes, Rayon, workers, queues, or
  automatic retry loops;
- a speculative third backend or remote-shaped batch/RPC API;
- raw SQL in core or as the shared LayerFS contract;
- copying the benchmark-private `Store` wholesale into production;
- treating `InMemoryCas` or the current raw `LogicalFile` path as the Memory
  semantic engine; and
- treating Memory publication as zero-cost durability.

## Exact WP5 handoff

WP5 begins from completed WP4-P and must perform only the following production
work.

### Files and ownership

Prefer the existing semantic owners already present under
[layerfs-core](../../../../crates/layerfs-core/src/lib.rs):

1. `content/persistence.rs`: finish selected K64/F64 construction,
   authenticated decode, summary validation, streamed reconstruction, and
   logical range routing over canonical `FileReference { raw_id, raw_length,
   object_id }`.
2. `cow/persistence.rs`: finish DIR256K metadata/page/index/wrapper
   construction and authenticated traversal; durable root ID is the wrapper
   `ObjectId`.
3. `delta/codec.rs`: finish ordered Genesis/Change page/index construction,
   authentication, and Phase-3 translation/replay using durable IDs only at
   the persistence boundary.
4. `validation.rs`: retain the exact 216-byte receipt codec unchanged; expose
   only the core validation needed by later concrete lanes.
5. `error.rs` or one narrowly owned mapping-error module: add the frozen exact
   durable mapping/resource errors that current `CoreError` cannot represent,
   especially `MissingObject(ObjectId)`, without a generic backend error type.
6. One existing persistence owner, not a provider module: add the call-scoped
   `impl FnMut(ObjectId) -> Result<CanonicalBytes, MappingError>` loader
   parameter used by closure, reconstruction, delta replay, and logical range.
7. One shared concrete Q/W/D accounting value for those core operations, with
   checked admission, exact ownership, high-water, and terminal-zero behavior.

Add a cross-domain codec module only if these existing owners cannot express
one exact function without a dependency cycle. Do not add it merely to create
an architectural layer.

### WP5 semantic entry and exit

WP5 input is exact canonical bytes and authenticated object IDs under the sole
profile. It must not accept a profile selector, SQLite row/connection, Memory
map, receipt cache, locator, or backend object.

WP5 exits only when all of these are true:

- every promoted success and malformed golden remains byte/ID exact;
- core can build, authenticate, decode, and traverse a complete file,
  directory root, Genesis/Change transition, and ordered delta using the
  call-scoped loader;
- reconstruction streams exact source bytes and exact cross-chunk/leaf/branch
  ranges without accepting an `InMemoryCas` parameter;
- every fetched complete object is ID-authenticated before role/summary use;
- root is content-only and transition owns parentage;
- full closure uses bounded active ancestry and spool state without an
  unbounded visited set;
- Q charges every live core-owned/requested capacity exactly once and returns
  to zero on all success, malformed, missing, overflow, allocation, sink, and
  cancellation exits;
- exact errors retain ID and precedence rather than collapsing to unit
  `MissingObject` or generic `Io`;
- no losing profile, selector, public engine trait, factory, registry, async,
  pool, Memory engine, SQLite v2 schema, or benchmark/H05 logic appears in the
  WP5 diff; and
- the existing Phase-2 `InMemoryCas`/`LogicalFile` utilities continue to
  preserve their current CDC/edit callers until WP6 deliberately adapts the
  full promoted mapping.

The smallest direct WP5 test seam is a test-local closure over a bounded map of
canonical objects. That closure is not `MemoryEngine`; it only proves that
core no longer depends on concrete `InMemoryCas` or SQLite authority.

### Work explicitly left to WP6-WP9

WP6 adds the concrete Memory object store, complete head, single-writer capture
guard, process-local receipt authority, atomic swap, and direct parity rows. It
may reuse `InMemoryCas` internals only after preserving the raw-chunk 32-KiB
contract; the unchanged type cannot hold all promoted canonical mapping
objects, whose admitted sizes exceed the CDC maximum.

WP7 modifies the concrete SQLite `Engine`: retain its object table and
immutable BLOB behavior; replace schema-v1 root/delta/head semantics with the
single authorized schema v2; enforce exact empty-v1 upgrade and nonempty-v1
`SchemaMigrationRequired`; publish the complete head in one COMMIT; and use
full scrub/default authority rules after genuine reopen. No second migration
path or alternate production profile is admitted.

WP8 contains two explicit lane calls, not a framework. WP9 freezes a new
post-WP5-WP8 two-lane manifest and runs the required isolated rows. CP-0009
remains the exact current-product control and protected-boundary evidence; it
is not relabeled as the future Memory/SQLite fair baseline.

## Readiness audit

| Required question | Resolution | Evidence label |
|---|---|---|
| Immutable put and authenticated reuse | One exact semantic contract; two concrete implementations; incumbent fully authenticated and compared. | `Observed(source)` |
| Complete and range get | Complete loader is the sole WP5 seam; direct range authenticates the whole object first; logical routing stays in core. | `Derived(equation)` |
| Root/delta/receipt/generation/head | Exact promoted identities and complete four-field head are frozen; parentage is transition-only. | `Observed(source)` |
| Rollback/errors | Pre-dispatch prior-head guarantee, immutable residue custody, bounded provenance, and SQLite four-way reconciliation are exact. | `Observed(source)` |
| Closure/reconstruction/ranges | Shared core algorithms and bounds are frozen and demonstrated in the CP benchmark; production extraction remains WP5. | `Observed(source/evidence)` |
| Counters | Common semantic counters plus concrete lane observations; unsupported zero rejected. | `Derived(equation)` |
| Q | Exact ceiling, ordinary-window total, ownership rules, and terminal zero are frozen; WP5 must move the lifecycle into core. | `Observed(source/evidence)` |
| Memory durability | Exactly `NotApplicable`, never zero. | `Observed(source)` |
| H05 dependency | No semantic or format dependency; performance remains unmeasured until its own screen. | `Hypothesis(test needed)` |
| Third backend/framework | Not required and explicitly unauthorized. | `Observed(source)` |

The semantic boundary is complete enough to start WP5 without waiting for an
H05 result. The known gaps are the work WP5-WP7 are expressly ordered to
close, not unanswered choices about the boundary.

## Artifact integrity

The exact SHA-256 is computed after the final byte is written and reported in
the task delivery. Embedding that digest here would change the file being
hashed. No detached hash file is created because this task authorizes one
report only.

READY_FOR_TWO_CONCRETE_LANES
