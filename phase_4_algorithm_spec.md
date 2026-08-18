# Phase 4 CAS + CDC + COW algorithm specification

- Status: candidate algorithm contract; profile selection and implementation
  pending
- Date: 2026-08-17
- Branch: `codex/empty-worktree`
- Applies to: WP4-M through WP14 of the Phase 4 rollback plan

## 1. Purpose and authority

This specification defines the algorithms that connect LayerFS Phase 1
canonical objects, Phase 2 content-defined chunking, Phase 3 copy-on-write
semantics, and the Phase 4 Memory and SQLite storage lanes.

It exists to prevent three kinds of drift:

1. optimizing a benchmark by omitting required logical or durability work;
2. freezing a page or fan-out constant before physical-I/O evidence selects
   it; and
3. creating a storage abstraction that moves canonical identity or COW
   semantics out of `layerfs-core` and into a database implementation.

Requirements apply in this order:

1. `PHASE_4_ROLLING_BACK_TO_PREVIOUS_OPTIMIZATION_SPEC.md` controls the active
   Phase 4 direction, correctness gates, and performance targets.
2. `PHASE_4_SQLITE_VISIBLE_HEAD_MIGRATION_SPEC.md` is the sole authority for
   the SQLite schema revision and version-1 handling required by this mapping.
3. `PHASE_4_LOGICAL_PERSISTENCE_MAPPING.md` controls exact candidate bytes,
   object roles, identities, strong edges, bounds, and profile-promotion
   procedure.
4. This document controls the algorithmic division of work, required data
   flow, asymptotic behavior, and implementation boundaries.
5. `PHASE_4_ALGORITHM_COMPLEXITY_ANALYSIS.md` supplies derived equations and
   explanatory analysis. It does not independently grant compatibility
   authority.
6. `PHASE_4_ROLLING_BACK_TO_PREVIOUS_OPTIMIZATION_IMPLEMENTATION_PLAN.md`
   controls work-package order and verification.

If this document conflicts with exact canonical bytes or identity rules in the
mapping specification, the mapping specification controls. If a candidate
profile loses the required WP4-M A/B, WP4-P replaces the candidate constants,
deletes the losing alternatives, and updates this document before production
format promotion.

The source fingerprints at creation of this specification are:

| Artifact | SHA-256 |
|---|---|
| rollback specification | `d8f59b476f40511564c3dedfc6f2646d149e4c7c141bfbd5538cc148a35eebd4` |
| rollback implementation plan | `f5097601cb8dd8ec24b3fa608d019de6b38c470cd1c41ae7bf4078b87a0e91dc` |
| WP4-C logical mapping | `3e94b054e6bf0eb198f6b04287d8a6cb209fb2925450b6c6bc6a69c84ab63e06` |
| SQLite visible-head migration authority | `cfddcc291cfff40ffcfd19e8e93ba2a4e51b3b16c412d137ece5463acc7625df` |
| complexity analysis | `33879535b0a2ddaf8a4f77a61c47844be9b1ae39d3b5486b51890882f58f2ee2` |

## 2. Decision summary

Phase 4 uses the following architecture:

```text
prepared source
    |
    v
streaming CDC (frozen 8/16/32 KiB profile)
    |
    +--> raw ChunkId
    |
    v
canonical Phase 1 Bytes object --> canonical ObjectId
    |
    v
immutable CAS creation or authenticated reuse
    |
    v
bounded file reference leaves --> radix branches --> file root
    |
    v
bounded directory pages/index/wrapper --> durable root
    |
    v
ordered durable delta
    |
    v
closure qualification --> one atomic visible-head publication
    |
    v
reopen/authentication --> reconstruction and exact range verification
```

The durable engine is SQLite. The Memory lane is a semantic reference and
shared-core performance ceiling. The deleted append-only carrier and packed
CAS are not candidates. No third database or remote provider is implemented in
this phase.

The selected data-structure family is:

- a small file root;
- bounded fixed-width reference leaves;
- bounded radix branches added only as required by the checked `u64`
  reference count;
- bounded directory pages plus a small authenticated index and wrapper;
- bounded delta pages plus an authenticated delta index; and
- immutable canonical objects addressed only by Phase 1 `ObjectId`.

The exact file leaf capacity `K`, branch fan-out `F`, and directory page
ceiling remain measurement candidates until WP4-P. No candidate is called
optimal or compatibility-bearing before that promotion.

## 3. Goals

The algorithms must provide:

- deterministic canonical identities independent of fragmentation, platform,
  allocator, Rust layout, database, and iteration order;
- streaming source processing with resident memory independent of source file
  size;
- an initial capture whose necessary work is linear in input bytes;
- logarithmic mapping navigation for authenticated range reads;
- page-local plus ancestor-spine work for same-count local edits;
- right-edge locality for append and truncate;
- authenticated immutable object reuse;
- a complete source-referencing file/tree/root/delta closure;
- one atomic visible-head transition per capture;
- a fast unchanged-reopen path only when an exact authenticated receipt has
  valid authority;
- an independent full-scrub path that reauthenticates the complete closure;
- exact typed failures with bounded first- and dominant-cause provenance;
- mathematical representation of a 100-GiB file without requiring a 100-GiB
  local qualification run; and
- fair separation of shared-core cost from SQLite-specific cost.

The post-promotion 100-MiB durable new-file target is:

- minimum: at least 200 MiB/s, or at most 500.000 ms;
- stretch: at least 300 MiB/s, or at most 333.333 ms.

These are full-workload targets, not CDC-only, hashing-only, SQL-only, or
reconstruction-only targets.

## 4. Non-goals

This specification does not authorize:

- append-only carriers, packs, packed indexes, carrier migration, or carrier
  cleanup code;
- a third local database, RocksDB, PostgreSQL, Redis, or a remote database;
- a generic engine factory, provider registry, connection pool, async runtime,
  Rayon, hidden worker, hidden queue, or retry storm;
- WAL mode, GC, compaction, repacking, or user-visible checkpoints;
- native materialization, projection, FUSE, VFS, clone/reflink, or SDK work;
- source-sized staging buffers, unbounded object-ID maps, unbounded visited
  sets, or unbounded object-byte caches;
- changing the Phase 1 object format or Phase 2 CDC boundaries for speed; or
- treating a profile-selection row as a product throughput result.

## 5. Preserved contracts

### 5.1 Canonical objects

Phase 1 remains the only physical identity domain:

```text
ObjectId(canonical_object) =
    BLAKE3("layerfs/object\0" || complete_canonical_object_bytes)
```

Only `Object::Bytes` and `Object::Directory` are used. Mapping records are
typed inner values carried by those existing canonical objects. A backend may
not invent a second content identity, trust a row key without authenticating
the canonical bytes, or reinterpret strong edges.

### 5.2 CDC

The CDC profile remains:

| Parameter | Value |
|---|---:|
| minimum chunk | 8,192 bytes |
| target chunk | 16,384 bytes |
| maximum chunk | 32,768 bytes |
| normalization | 2 |
| seed | 0 |

Fragmented and contiguous delivery of identical source bytes must produce the
same ordered chunk boundaries and identities.

### 5.3 COW, roots, and deltas

Phase 3 logical file order, directory order, metadata, mutations, root meaning,
and sequential delta application remain unchanged. Durable identities replace
provisional in-memory identities only at the persistence boundary.

The durable root identity is the canonical `ObjectId` of the root directory
wrapper. Parentage belongs to the publication transition and delta, not to the
content identity of the root.

### 5.4 Storage and execution

The production path is:

- disk-backed for SQLite;
- synchronous on caller threads;
- one writer per engine;
- one SQLite transaction and one durability-equivalent commit per capture;
- bounded in memory;
- free of hidden retries and workers; and
- fail-closed on ambiguous authority or durability.

The current schema-version-1 `visible_root` column cannot represent the frozen
complete visible head, and its root row conflates content identity with
parentage. WP4-M may use only isolated candidate databases. WP7 may change the
production schema only under
`PHASE_4_SQLITE_VISIBLE_HEAD_MIGRATION_SPEC.md` after WP4-P promotes one
profile.

## 6. Notation

| Symbol | Meaning |
|---|---|
| `S` | raw source bytes in one logical file |
| `N` | ordered chunk-reference occurrences |
| `U` | unique canonical chunk objects |
| `K` | references per file leaf candidate |
| `F` | children per file branch/root candidate |
| `P = ceil(N/K)` | file leaf count |
| `H` | branch levels between file root and leaf |
| `X_b` | bytes in the edit/resynchronization region |
| `X_c` | changed/new reference occurrences after CDC rejoin |
| `Z` | suffix references repacked by a count-changing fixed-ordinal edit |
| `E` | entries in a logical directory |
| `B_d` | canonical directory page ceiling candidate |
| `T` | ordered delta-entry count |
| `A` | canonical bytes authenticated by an operation |
| `V` | strong-edge occurrences visited by an operation |
| `B_v` | radix branch occurrences visited by one range |
| `L_v` | file-leaf occurrences visited by one range |
| `C_v` | chunk-reference occurrences visited by one range |
| `Q` | peak live mapping-owned allocation |
| `W` | cumulative work/input/authentication bytes |
| `D` | cumulative decoded, streamed, or spooled output bytes |
| `B_sql` | bounded SQLite batch size |

All persistent lengths, offsets, counts, cumulative ends, and observations use
checked `u64` arithmetic. Per-object encoded fields may use narrower widths
only where the mapping specification freezes them and validates conversion.

`Q` is a live-allocation high-water mark. `W` and `D` are cumulative
counters. A 100-GiB streamed operation may have 100-GiB-scale `W` or `D`
without allocating 100 GiB. No file is rejected merely because cumulative
streamed output exceeds a resident-memory budget.

## 7. Module ownership

The final implementation should add the smallest code in existing semantic
owners:

```text
crates/layerfs-core/src/
  object/{model.rs,codec.rs}       existing Phase 1 authority
  cdc/{mod.rs,gear.rs}             existing CDC authority
  cas/mod.rs                       narrow canonical-object read port
  content/mod.rs                   existing logical file/edit semantics
  content/persistence.rs           file leaf/branch/root algorithms
  cow/{tree.rs,mutate.rs}          existing tree/COW semantics
  cow/persistence.rs               directory/root persistence algorithms
  delta/mod.rs                     existing delta semantics
  delta/codec.rs                   durable delta algorithm
  limits.rs                        one promoted profile after WP4-P
  error.rs                         typed failures

crates/layerfs-engine/src/
  lib.rs                           existing SQLite engine and integration
  memory.rs                        Memory semantic lane
  bin/phase4_create_edit_benchmark.rs

crates/layerfs-engine/tests/
  phase4_engine_parity.rs
```

SQLite remains in the current engine module unless a later independently
justified refactor proves that moving it reduces real complexity. There is no
public engine trait or factory solely to dispatch between Memory and SQLite.

WP4-M may contain a private benchmark/test selector for candidate profiles.
Each candidate database and receipt must use the exact private
domain-separated profile ID in the mapping specification. WP4-P must delete
that selector, every candidate ID, and every losing constant, branch, and
fixture before final goldens or production promotion.

## 8. Shared capture algorithm

### 8.1 Preconditions

Before starting a capture:

1. acquire the single-writer capture authority;
2. authenticate the declared parent visible head when the operation has one;
3. validate the operation's bounded resident-resource plan;
4. prepare reusable bounded chunk, canonical-object, leaf, branch, page, and
   spool windows;
5. begin one SQLite transaction for the durable lane, or one staged in-memory
   publication for the Memory lane; and
6. initialize checked observations without publishing a new visible head.

Source generation and fixture fingerprint preflight are benchmark preparation,
not capture work. Reading the prepared source for the actual capture is inside
the headline timer.

### 8.2 Streaming pipeline

For each source fragment:

1. feed the fragment into the frozen CDC scanner;
2. when CDC emits a chunk, compute its raw `ChunkId` from raw bytes;
3. canonically encode `Object::Bytes(raw_chunk)`;
4. compute the canonical `ObjectId` over the complete canonical object;
5. submit the canonical object to immutable CAS publication/reuse;
6. append `(raw_chunk_id, raw_length, canonical_object_id)` to the current
   file-reference leaf builder;
7. when the leaf reaches `K`, finalize, hash, and stage it, then propagate its
   cumulative end through the radix builder; and
8. retain no source-sized chunk list or canonical-object map.

The implementation target is:

```text
one source traversal
  -> CDC
  -> both required identity domains
  -> one canonical construction
  -> one validated storage handoff
```

The raw and canonical hashes are semantically distinct. They may be fed from
the same bounded chunk window but may not substitute for each other.

### 8.3 Finalization

After source EOF:

1. finalize the last CDC chunk if any;
2. finalize the last partial file leaf;
3. finalize partial radix branches bottom-up;
4. produce the file root;
5. incorporate the file `NodeId` into the changed directory page and rebuild
   required directory ancestors;
6. produce the durable root and ordered durable delta;
7. qualify all newly created and reused strong edges required by the capture
   using the full or incremental rule below;
8. create the publication transition and validated receipt only after its
   claimed closure work succeeds;
9. stage the complete `VisibleHead { generation, child, transition,
   validation_receipt }`; and
10. use exactly one SQLite COMMIT as the atomic publication and durability
    boundary.

Immutable objects written before a failed final transition may remain as
unreachable authenticated residue. A failure proven before COMMIT dispatch
must not change the visible head and must report residue/custody honestly; it
must not claim that no bytes were written. A failure after dispatch follows
section 16.3 and may be successful, definitely absent, conflicting, or
ambiguous after reconciliation.

## 9. CDC algorithms

### 9.1 New file and full replacement

CDC examines every source byte:

```text
time   = Theta(S)
memory = O(32 KiB + fixed rolling state)
```

No algorithm can safely make a first scan sublinear. Optimization is limited
to fewer copies, bounded buffer reuse, contiguous scanning, and reduced call
overhead while preserving the exact boundary fingerprint.

### 9.2 Small edit and exact rejoin

Given an authenticated base file, a bounded edit may:

1. retain the unchanged prefix reference sequence;
2. begin rescanning at the frozen safe predecessor boundary;
3. scan the replacement and required old suffix bytes;
4. require the frozen number of exact CDC rejoin confirmations;
5. retain the authenticated unchanged suffix only after rejoin succeeds; and
6. fall back honestly to a larger scan or full replacement if no safe rejoin
   is established.

The intended successful cost is:

```text
time   = O(X_b)
memory = bounded CDC/edit/rejoin windows
```

Rejoin optimization may not skip base authentication or accept a merely
probable boundary match.

## 10. Canonical object creation and reuse

### 10.1 New canonical object

For a canonical object of `b` bytes:

1. produce exact canonical bytes once;
2. feed those bytes to the identity hasher;
3. validate role-specific grammar and bounds;
4. transfer the same verified bytes to the backend without an avoidable clone;
5. perform immutable no-replace insertion; and
6. return `Created` only if those exact bytes became the occupant.

The necessary time is `Theta(b)`. A second full hash or decode of the same
caller-owned verified buffer is removable amplification unless a trust
boundary requires it.

### 10.2 Existing object

A key, locator, row, or uniqueness conflict is not proof of immutable equality.
Without a future exact operation-local verified-work receipt, reuse requires:

1. load the complete incumbent canonical object;
2. recompute and compare `ObjectId`;
3. validate outer Phase 1 grammar;
4. validate the expected inner mapping role when applicable; and
5. compare the authenticated canonical bytes or equivalent authenticated
   semantic value required by the no-replace contract.

Only then may the result be `Reused`.

A future WP10 bounded operation-local verified-work receipt may avoid duplicate
backend reads only when
it binds the exact store identity, validation authority, integrity epoch,
mapping profile, generation, authenticated root/transition, object ID, locator
or row identity, and byte range. It must have an explicit count/byte bound and
deterministic eviction behavior. It is not an unbounded object cache and is not
`ValidatedSnapshotReceiptV1`.

## 11. File radix algorithms

### 11.1 Candidate family

Each file occurrence preserves exactly:

```text
raw ChunkId [32]
raw length  u32
canonical chunk ObjectId [32]
```

The candidates are:

| Candidate | Leaf capacity `K` | Branch fan-out `F` | Purpose |
|---|---:|---:|---|
| K64/F64 | 64 | 64 | retained locality starting candidate |
| K59/F101 | 59 | 101 | near-complete-4-KiB canonical objects |
| K256/F256 | 256 | 256 | lower object/SQL-call count candidate |

These are temporary WP4-M choices. The algorithm must not assume that K64 is
optimal merely because retained in-memory evidence favored it.

### 11.2 Streaming radix construction

Maintain:

- one partial leaf with at most `K` references;
- one partial branch at each active level with at most `F` child descriptors;
- checked cumulative raw length and reference count; and
- a bounded canonical-object output window.

When a leaf fills, finalize it and push its `(cumulative_end, ObjectId)` into
level 1. When a branch fills, finalize it and push its descriptor into the next
level. At EOF, flush partial nodes bottom-up and emit the smallest-height root
that represents the sequence. Redundant top levels and nonfinal partial groups
are malformed.

For fixed `K` and `F`:

```text
construction time   = Theta(N)
resident metadata   = O(K + F*H)
mapping space        = Theta(N)
height               = O(log_F(N/K))
```

### 11.3 Exact range routing

With a valid snapshot receipt, a range `start..end`:

1. authenticates the file root;
2. validates `start <= end <= total_raw_length`;
3. for an empty range, returns after root validation;
4. at each root/branch, computes each child's file-global interval using the
   inherited base plus the prior cumulative end;
5. descends only into nonempty children intersecting the request;
6. authenticates selected leaves completely;
7. authenticates every selected complete chunk object;
8. verifies raw chunk length and raw `ChunkId`; and
9. emits only the requested raw byte slices through a bounded output window.

Zero-length references remain in the semantic sequence but never contribute
payload bytes to a nonempty range.

For arbitrary ranges, the honest fast-path cost is:

```text
time   = O(F*B_v + K*L_v + C_v + returned bytes)
memory = O(H + K + one chunk + output window)
```

For a range contained in one leaf, `B_v=O(H)`, `L_v=1`, and this reduces to
`O(F*H + K + C_v + returned bytes)`. Branch and leaf search is linear in the
bounded candidate fan-out unless the promoted codec freezes a different local
search structure.

Without a valid receipt, skipped cumulative summaries lack authority. The
operation must first perform a full scrub or return the exact validation-
authority error; it may not silently trust the index.

### 11.4 Full reconstruction

Traverse leaves and references in canonical ordinal order. For every
occurrence:

1. authenticate the referenced canonical Bytes object;
2. validate its raw length and raw `ChunkId`;
3. emit its payload to the caller-provided streaming sink; and
4. update checked output/fingerprint counters.

No eager source-sized output `Vec` is required by the scalable API.

```text
time   = Theta(S + N + authenticated mapping bytes)
memory = bounded object, traversal, spool, and output windows
```

An eager convenience API may return `AllocationBudgetExceeded` after
preflight. That does not make the durable file unrepresentable.

### 11.5 Same-count local edit

When CDC rejoin changes references without changing total ordinal count:

1. retain authenticated unchanged leaves by `ObjectId`;
2. rebuild only leaves containing changed references;
3. rebuild only their ancestor branches and the file root;
4. rebuild the containing directory page/index/wrapper and namespace ancestor
   spine; and
5. retain every unaffected canonical object by identity.

For one changed leaf:

```text
file mapping work      = O(K + F*H)
created file objects   = O(H)
durable namespace work = page plus index/wrapper at each changed ancestor
```

The retained K64/F64 equations predict roughly 7.1 KiB and three file-mapping
objects at 100 MiB, but measured object, SQL, hash, BLOB, and physical-byte
costs decide whether that constant is acceptable. The current in-memory
`BTreeMap` mutation can still clone/hash `O(E)` directory state; the durable
mapping does not erase that separate WP12 cost.

### 11.6 Append and truncate

Append rebuilds the last partial leaf, any new leaves, and the rightmost radix
spine. Truncate locates the cut, rewrites the boundary leaf and spine, and
omits complete suffix subtrees from the new root. Neither operation physically
deletes old CAS objects.

With authenticated base authority:

```text
append   = O(appended/rescanned bytes + new refs + K + F*H)
truncate = O(H + K) mapping work after cut location
```

### 11.7 Count-changing middle edits

Fixed ordinal grouping has an honest worst case: inserting or deleting a
reference may shift the entire later page partition.

```text
time and new mapping history = O(Z)
worst case Z = Theta(N)
```

WP4-P must not hide this weakness. The forced `+1` reference row at 100 MiB
and 512 MiB is a format gate, not an optional regression. The 5% local ratio is
only an alarm because it can compare two linear quantities. Promotion also
requires the measured 100-to-512-MiB suffix slope, analytical rewritten-object/
byte projection at 100 GiB, and an explicitly approved absolute 100-GiB
middle-insert budget. That budget is not a file-size admission limit. If fixed
ordinal grouping fails it, the only authorized next candidate is a
deterministic, history-independent content-defined/prolly boundary algorithm
over the ordered reference stream. No such structure is added speculatively.

## 12. Directory algorithms

### 12.1 Representation

A durable directory consists of:

- one typed metadata Bytes object;
- zero or more bounded Phase 1 Directory entry pages;
- one authenticated typed index over ordered page boundaries and IDs; and
- one two-entry Phase 1 Directory wrapper referencing metadata and index.

User names appear only in entry pages and remain in strict canonical order.
They cannot collide with wrapper schema names.

### 12.2 Candidate page ceilings

WP4-M compares complete canonical page ceilings of:

- 64 KiB;
- 256 KiB; and
- 1 MiB.

Pages are greedily packed without splitting an entry. A page ceiling is a
physical/locality candidate, not a maximum logical directory size. The
existing direct-reference limit continues to apply per canonical Directory
object.

Greedy-to-16-MiB pages are rejected as the default because one child-ID change
could rewrite and authenticate approximately 16 MiB of metadata. The smaller
candidates trade page count against point-update and authentication bytes.

### 12.3 Construction and lookup

Directory construction streams canonical ordered entries into one page at a
time and uses a bounded index builder or file-backed spool when index metadata
exceeds the resident plan.

```text
create time = Theta(total encoded entry bytes)
resident memory = O(B_d + bounded index/spool window)
```

Point lookup authenticates the complete bounded index, binary-searches its
page boundary records, authenticates one selected page, and binary-searches or
ordered-searches entries within that page.

### 12.4 Directory COW

A same-size child-ID replacement rewrites:

1. the containing entry page;
2. the index containing the page's new ID; and
3. the directory wrapper containing the new index ID.

The containing namespace ancestors repeat that bounded three-object pattern.
The current in-memory `BTreeMap` clone/rehash cost in `cow/tree.rs` remains a
separate measured core optimization; the durable page format alone does not
make that CPU path logarithmic.

Greedy count-changing insertion/removal can repack the later page suffix and
remain `O(E)` in the worst case. The wide-directory A/B must measure both
same-size replacement and leading insertion before selecting `B_d`.

## 13. Delta algorithm

Delta entries preserve their admitted Vec order. Repeated paths remain
semantic. Encoding may not sort, deduplicate, combine, or parallel-reorder
entries.

The durable algorithm:

1. translates each provisional embedded `TreeNode` to its durable `NodeId`;
2. encodes each delta operation with its exact path, before/after IDs, and
   metadata required by the mapping specification;
3. streams whole entries into bounded typed Bytes pages without splitting an
   entry;
4. emits an authenticated ordered page index carrying parent and child root
   IDs; and
5. defines the delta ID as the Phase 1 `ObjectId` of that index object.

Replay authenticates and decodes pages in order and applies each semantic
operation sequentially. Its time is linear in encoded delta bytes plus the COW
mutation costs invoked by those entries.

## 14. Closure, cycles, and bounded traversal

### 14.1 Full closure qualification

A full closure operation visits every strong-edge occurrence in the frozen
order. For each object it:

1. loads complete canonical bytes;
2. verifies `ObjectId`;
3. validates Phase 1 grammar and exact EOF;
4. validates the expected mapping role/version;
5. emits strong edges in encoded order; and
6. checks role-specific counts, lengths, partitions, and cumulative ends.

```text
time = Theta(A + V)
```

Genesis, a missing/unreceipted prior head, an invalid prior receipt, and the
named full-scrub operation always use this complete traversal.

An incremental capture may avoid replaying unchanged siblings only when its
prior `ValidatedSnapshotReceiptV1` remains valid for the exact store, epoch,
generation, profile, root, and transition. It authenticates the prior and
replacement node on each changed spine and compares their ordered strong-edge
IDs. An equal child ID is covered by the prior closure; every new or different
child is fully traversed. Every fetched object is still completely hashed and
role-validated. Missing or corrupt changed-path bytes fail before publication.
This induction is the only receipt-backed closure shortcut; it uses no global
visited map.

### 14.2 Cycle detection

Cycle rejection requires only the active ancestry. The traversal maintains a
bounded stack of `(ObjectId, role, next_edge)` frames. Encountering the same
`(ObjectId, role)` on the active path is a cycle. A completed shared DAG node
may be visited again without an unbounded global black/visited set.

Wide pending work uses a bounded resident edge-spool window and a file-backed
spool with exact cursor offsets when required. Resuming a parent may not refetch
the parent once per child merely to avoid storing cursor state.

### 14.3 Depth and resources

The durable logical namespace depth is bounded by the existing 256-component
path contract. Physical mapping depth is derived from logical depth plus the
exact wrapper/index/radix grammar; it is not the Phase 1 parser nesting limit.

Structural malformed limits, durable admission, resident allocation, and
cumulative operation work are distinct:

- malformed structure returns the exact format error;
- a logically constructible value beyond durable namespace depth returns the
  exact durable-admission error;
- live allocation beyond the admitted `Q` plan returns
  `AllocationBudgetExceeded`;
- checked counter overflow returns `LengthOverflow` or the more exact frozen
  counter error; and
- large cumulative streamed `W` or `D` is not itself a resident-memory
  failure.

## 15. Receipts, reopen, and authentication modes

### 15.1 Snapshot and operation-local receipts

`ValidatedSnapshotReceiptV1` is exactly the 216-byte backend-private snapshot
attestation frozen by mapping section 9.5. It binds the receipt magic/version/
kind, store instance, validation authority, integrity epoch, head generation,
child root, transition, mapping profile, and authenticator. It is not a
user-visible checkpoint.

The receipt never:

- replaces canonical object hashing for bytes actually fetched;
- proves that an object still exists;
- authorizes a different store, epoch, generation, profile, root, or
  transition;
- turns a stale or rolled-back store into a valid snapshot; or
- permits trusting an index key without authenticating the returned object.

It contains no locator transcript and never proves incumbent equality. A
future WP10 operation-local verified-work receipt is a separate concept. It
may bind an exact immutable store identity, validation authority, integrity
epoch, mapping profile, generation, authenticated root/transition, object ID,
locator or row identity, and byte range only if counters justify it and its
count/byte bound and deterministic eviction are explicit. It cannot replace
the snapshot receipt or authorize cross-reopen trust.

### 15.2 Fast unchanged reopen

When snapshot receipt and monotonic store authority are valid:

1. authenticate the authoritative visible head;
2. authenticate and validate the bound receipt;
3. authenticate the durable root wrapper;
4. open the snapshot without replaying every sibling object; and
5. authenticate each later fetched path object completely.

The initial work is bounded independently of file size. Subsequent range or
edit work pays for selected paths.

For the current SQLite trust model this shortcut is same-open only by default.
Cross-reopen reuse is reportable only under the mapping specification's
explicit non-adversarial database/key/file trust assumption. Otherwise reopen
performs a full scrub or returns `ValidationAuthorityUnavailable`; it must not
claim adversarial receipt equivalence.

### 15.3 Fresh scrub reopen

When receipt authority is missing, invalid, stale, or deliberately bypassed by
an audit, reopen performs full closure qualification. This remains a separate
named benchmark row. Fast reopen must not be reported as a fresh corruption
scrub.

## 16. Atomic publication and failure semantics

### 16.1 SQLite capture

The SQLite lane must:

1. stage all canonical object insertions/reuse classifications in one capture
   transaction;
2. validate required closure and transition preconditions;
3. stage durable root, transition, receipt, and the complete visible-head tuple
   exactly once;
4. dispatch exactly one COMMIT using the preserved durability profile; and
5. expose the new generation at that COMMIT boundary, subject to exact
   post-dispatch reconciliation when acknowledgement is lost.

No root or delta publication occurs in a separate transaction merely to make
batching easier.

### 16.2 Memory capture

The Memory lane stages the same canonical object graph and atomically switches
its in-process visible head after the same semantic gates. It reports
durability and process reopen as `NotApplicable`, never as zero-cost durable
success.

### 16.3 Failure precedence

Every operation retains bounded provenance for:

- the earliest exact typed failure; and
- a later dominant cleanup, invalidation, or ambiguous-durability failure only
  where the lifecycle contract authorizes dominance.

Cleanup may not erase the first cause. The bounded record contains `first`,
`cleanup_first`, `reconciliation`, and `dominant`. Counter or elapsed-time
accounting may not mask an earlier I/O failure.

Before publication dispatch, a failure guarantees the prior complete head is
still authoritative. After COMMIT or compare-and-publish dispatch, reconcile
the authoritative complete tuple:

| Observed authority | Operation outcome |
|---|---|
| exact requested head and receipt, with the retained request key reproduced by the frozen derivation | success; first/cleanup remain diagnostic |
| exact prior head | return the original exact failure; publication proven absent |
| a different complete head | `dominant=PublicationConflict` |
| requested/prior/different cannot be established | `dominant=AmbiguousDurability`; visibility unknown |

Only the identical idempotency key may retry an ambiguous operation. New
immutable objects may remain unreachable residue in every failed case and must
be counted or reported with typed unavailable/custody state.

## 17. SQLite integration and batching

SQLite is a storage implementation, not the owner of canonical semantics.
Core supplies exact canonical objects, IDs, edges, and root/delta values;
SQLite stores them and performs the atomic publication.

The baseline must first count:

- transaction and commit count;
- statement preparations and executions by operation;
- existence probes;
- insert attempts, creations, and conflicts;
- incumbent object reads/authentication;
- BLOB opens and bytes;
- root/delta/head reads and writes;
- rows examined or changed;
- query plans and index use; and
- busy/locked events.

Allowed optimization order is:

1. reuse one prepared statement per repeated operation in the capture;
2. remove a redundant existence probe when insert/conflict classification
   supplies the same decision and incumbent authentication is retained;
3. use bounded ID batches for existence or incumbent reads only when measured
   API crossings dominate;
4. execute bounded insert groups inside the existing one transaction;
5. write root, delta, receipt, and head once after qualification; and
6. modify an index only after `EXPLAIN QUERY PLAN` and direct timing prove the
   access-path problem.

For operations that SQLite can execute as a real bounded multi-row request,
batching can reduce API crossings from approximately `O(rows)` toward
`O(ceil(rows/B_sql))`. Prepared-statement reuse reduces preparations, not
executions, and row/authentication work remains linear. A batch may not become
a source-sized SQL statement or source-sized result set.

## 18. Reconstruction versus materialization

This Phase 4 algorithm includes authenticated logical reconstruction and exact
range reads. It does not implement native materialization.

Full reconstruction:

```text
CAS objects -> authenticated mapping traversal -> streamed raw bytes
```

Future native materialization additionally requires:

```text
streamed raw bytes
  + destination directories/files
  + metadata application
  + destination publication/custody
  + destination durability
  + explicitly defined verification
```

That work belongs to the later `layerfs-os` materialization phase. No Phase 4
result may call reconstruction or a range read native materialization.

## 19. Complexity requirements

| Operation | Required algorithmic behavior | Known caveat |
|---|---|---|
| new capture | `Theta(S)` byte work plus `O(N)` object/row work | constants decide 200/300 MiB/s |
| unchanged recapture | authenticated reuse; no new live payload bytes | unaided reuse still hashes incumbent bytes |
| same-count small edit | `O(X_b + X_c + K + F*H)` | namespace ancestor work remains |
| EOF append | appended/rescanned bytes plus rightmost spine | durability can dominate tiny appends |
| EOF truncate | boundary path plus leaf/spine | old objects become residue; no GC |
| middle count change | bounded resident memory, worst `O(Z)` | mandatory format rejection gate |
| arbitrary range | `O(F*B_v + K*L_v + C_v + returned bytes)` with receipt | full scrub required without authority |
| full scrub | `Theta(A + V)` | intentionally complete |
| fast reopen | fixed head/receipt/root work then lazy paths | not equivalent to fresh scrub |
| full reconstruction | `Theta(S + N + mapping auth)` | small-object/SQL constants matter |
| directory replace | one page + index + wrapper per ancestor | current in-memory map clone may be `O(E)` |
| directory leading insert | worst `O(E)` | candidate page split weakness |
| delta replay | linear encoded order plus invoked COW work | no sorting/dedup shortcut |

The implementation may improve constants but may not claim a better Big-O
class than the promoted format actually supplies.

## 20. Memory and large-file requirements

### 20.1 Resident memory

Normal streaming capture, scrub, range, and reconstruction retain only:

- one maximum canonical-object window;
- one CDC/chunk window;
- one file leaf and partial branch per active level;
- one directory or delta page;
- bounded traversal frames;
- a bounded spool window;
- a bounded output window; and
- bounded receipt/counter state.

The frozen maximum `Q` is 1 GiB, a pathological admission guard rather than an
allocation target. The ordinary fixed windows total exactly 33,604,696 bytes
before live semantic results and backend/runtime overhead. `W` and `D` are
checked cumulative `u64` telemetry with no lower fixed ceiling. The exact
logical Q high-water, W/D totals, external RSS, and unavailable allocator/page-
cache components must be reported separately.

### 20.2 100-GiB validity

The representation must support a 100-GiB logical file mathematically using
checked `u64` lengths/counts and additional radix levels only as required.

For the retained-density K64/F64 model:

- source bytes: 100 GiB;
- reference occurrences: approximately 5,410,816;
- leaves: 84,544;
- branch levels: two;
- mapping objects: approximately 85,887;
- mapping plus per-chunk canonical framing: approximately 444,117,735 bytes;
- mapping overhead: approximately 0.4136%; and
- root-to-chunk path: root, two branches, leaf, chunk.

These values are analytical projections, not a fabricated benchmark. The
qualification campaign measures 100 MiB and 512 MiB and compares observed
per-byte/per-object slopes with the equations. It must report projection
uncertainty.

No arbitrary 100,000-reference, 2-GiB, 3-GiB, graph-work, or cumulative-output
ceiling may be introduced. Exact structural object limits, logical depth,
backend capacity, live allocation, and checked arithmetic overflow remain
valid independent limits.

## 21. Candidate profile selection

### 21.1 File profile

WP4-M must compare K64/F64, K59/F101, and K256/F256 on identical 100-MiB and
512-MiB fixtures. Every row is labeled:

```text
qualification=false
purpose=profile_selection
```

The comparison includes full capture, full scrub/reopen, reconstruction,
prefix/middle/EOF/cross-boundary ranges, same-count middle edit, and forced
`+1` early/middle edit.

K64/F64 is the default. The predeclared primary metric is the complete
100-MiB full-cycle SQLite median, guarded by 512-MiB scaling and range,
same-count-edit, forced-`+1`, CPU, allocated-store-delta, logical-Q, and
external-RSS observations. A challenger replaces it only if its overall
primary median improves by at least 5%, it is faster in at least four of five
paired matched blocks, and no protected outcome/resource median regresses by
more than 5% at either size. If a required observation is unavailable,
rankings reverse between sizes, or the win is dominated by a removable per-row
SQL crossing, the result is inconclusive and retains K64/F64 pending the
smallest counter-driven SQL-sensitivity probe.

Fixed ordinal grouping is rejected if it violates the measured suffix-rewrite
model or the approved 100-GiB middle-insert analytical work budget over
rewritten reference occurrences, leaves/branches/objects, canonical mapping
bytes, and optional rewrite-to-capture amplification. The local 5% ratio,
measured 100/512-MiB slope, and analytical 100-GiB work projection are all
mandatory evidence; none alone is sufficient. Projected 100-GiB latency is a
nonbinding estimate with stated uncertainty, not a fabricated benchmark or an
admission ceiling.

### 21.2 Directory profile

WP4-M compares 64-KiB, 256-KiB, and 1-MiB canonical page ceilings on one
identical wide-directory corpus. It measures create, reopen/full validation,
point lookup, same-size replacement, and leading insertion.

The 256-KiB ceiling is the default. The predeclared primary metric is complete
same-size middle-child `edit_verification_wall`. Protected outcome/resource
metrics are create/full-validation wall and CPU, point-lookup latency,
same-size replacement `edit_publish_wall` and `edit_verification_wall`,
leading-insert publication/verification latency, allocated-store delta,
logical Q, and external RSS. A challenger replaces the default only if its
overall primary median improves by at least 5%, it is faster in at least four
of five paired matched blocks, and no protected median regresses by more than
5%. An unavailable or split result retains 256 KiB.

The winner must balance:

- canonical and physical page bytes;
- number of mapping objects, rows, statements, and BLOB opens;
- point-lookup authentication bytes;
- same-size replacement rewrite bytes;
- count-changing suffix rewrite bytes;
- CPU and wall time; and
- peak logical memory/RSS.

The listed object/page/SQL/auth/rewrite quantities are mandatory diagnostics,
not uniform 5%-nonregression guards. No candidate is selected from
object-count arithmetic alone. If removable SQL crossings could reverse the
ranking, the same private bounded sensitivity probe and defer rule as section
21.1 applies.

### 21.3 Promotion

WP4-P must:

1. select exactly one file K/F and one directory page ceiling;
2. delete all losing constants, code branches, selectors, and candidate
   fixtures;
3. regenerate independent final golden bytes and IDs;
4. fingerprint the promoted specification and vectors;
5. pass independent read-only correctness and performance audits; and
6. expose only the promoted profile to WP5+ production integration.

The 500.000-ms minimum and 333.333-ms stretch values are reported during
WP4-M as credibility diagnostics, not pre-optimization promotion blockers.
The binding 200/300-MiB/s decision is made only after WP10-WP12 on stable
source in WP14.

## 22. Measurement boundaries

### 22.1 SQLite headline capture row

The timer begins immediately before reading the prepared source and ends only
after:

1. source read;
2. CDC;
3. raw and canonical identity work;
4. object creation/reuse;
5. complete file/tree/root/delta construction;
6. required closure qualification;
7. one durable commit;
8. drop every engine handle, SQLite connection, and process-local receipt/cache
   value, then construct a fresh engine instance from the durable path;
9. authenticated visible root, delta, and required closure verification;
10. full streamed reconstruction and fingerprint verification; and
11. exact range verification.

Fixture generation, preflight fingerprinting, and empty-store preparation are
outside the timer.

A separate child-process integration test is required if an acceptance row
claims a new-OS-process reopen. A fresh instance in the benchmark process must
not inherit authority from dropped process-local state.

### 22.2 Memory headline row

Memory uses the same source read and semantic work. It performs an independent
fresh in-process read/reconstruction after publication but labels disk
durability and process reopen `NotApplicable`. It is a shared-core ceiling,
not a durable competitor.

### 22.3 Separate diagnostic rows

The benchmark must also keep these distinct:

- capture-only phase totals nested inside the headline;
- fast receipt-backed unchanged reopen;
- fresh full-scrub reopen;
- same-count edit;
- forced count-changing edit;
- exact range latency/path work;
- full streamed reconstruction; and
- one repeat-heavy 100-MiB dedup diagnostic after profile promotion; and
- later native materialization, which is outside Phase 4.

No diagnostic row may be relabeled as the headline full workload.

## 23. Mandatory observations

Every performance row reports a value or explicit `Unavailable` or
`NotApplicable` for:

- wall time, CPU time, throughput or latency, median/min/max/spread;
- source-read, CDC, encode, raw-hash, canonical-hash, CAS, COW, closure, SQL,
  commit, reopen, reconstruction, and range time;
- bytes read, encoded, copied, hashed, authenticated, compared, written, and
  emitted;
- chunk/reference/object/node/edge submissions, creations, reuses, visits, and
  unreachable residue;
- file height, leaves/branches, selected range path, rebuilt pages, and
  ancestor nodes;
- SQL preparations/executions, rows, BLOB opens, transactions, commits, syncs,
  query plans, busy events, and locked events;
- logical `Q/W/D`, bounded cache/spool high-water, and external RSS;
- SQLite logical/apparent/allocated database, journal, and temporary bytes;
- host physical reads/writes when directly observable; and
- exact source/store cache conditioning.

Unavailable physical or cache state is never replaced by logical bytes or
zero.

## 24. Correctness acceptance

No performance row is accepted unless it proves:

- exact source, CDC sequence, canonical object, root, and delta identities;
- expected object creation/reuse outcomes;
- complete required closure membership and order;
- immutable incumbent tamper detection;
- exactly one SQLite transaction/commit/publication for SQLite capture, edit,
  or complete-cycle rows; zero write transactions for SQLite read-only rows;
- zero SQLite actions and `NotApplicable` durability/process reopen for Memory;
- prior visible head after every prepublication failure;
- exact reopened root, delta, and generation;
- full reconstructed-byte and fingerprint equality;
- exact range bytes, including empty, cross-chunk, cross-leaf, and
  cross-branch probes;
- bounded live allocation and traversal/spool state;
- exact typed malformed, overflow, I/O, conflict, and authority errors;
- preserved first and dominant causes;
- honest immutable residue/custody accounting; and
- no hidden retry, worker, queue, or additional durability boundary.

## 25. Implementation sequence

The algorithm must be implemented in this order:

1. **WP4-M:** minimum private candidate codec/SQLite measurement path.
2. **WP8/WP9 selection campaign:** non-qualifying file and directory A/B.
3. **WP4-P:** select one profile, delete alternatives, regenerate goldens.
4. **WP5:** finalize shared core mapping and bounded object-read reconstruction.
5. **WP6:** add the Memory semantic lane.
6. **WP7:** integrate the promoted mapping with SQLite.
7. **WP8/WP9 baseline:** establish the unoptimized fair Memory/SQLite rows.
8. **WP10:** optimize duplicate authentication/closure only when counters
   prove it dominates.
9. **WP11:** optimize SQLite statements/batches only when counters prove it
   dominates.
10. **WP12:** optimize remaining measured core encode/hash/COW/CDC work.
11. **WP13:** audit compatibility requirements for a future backend without
   implementing one.
12. **WP14:** run the stable-source final campaign and select the Phase 4
   outcome.

No production compatibility-bearing codec begins from an unmeasured profile.
No optimization is retained solely because a microbenchmark improved.
WP7 uses only `PHASE_4_SQLITE_VISIBLE_HEAD_MIGRATION_SPEC.md`; it does not add a
general migration subsystem.

## 26. Final decision outcomes

The final Phase 4 result must be exactly one of:

1. retain SQLite after reaching at least 200 MiB/s on the qualifying durable
   100-MiB new-file row;
2. retain SQLite and continue shared-core optimization because Memory and
   SQLite evidence shows the remaining limit is engine-agnostic; or
3. authorize a separate specification for one named third backend because
   optimized SQLite still misses the target and measured SQLite-specific work
   is dominant.

Memory alone cannot satisfy the durable target. The profile-selection A/B
cannot satisfy it. A fresh reconstruction cannot be called materialization.

## 27. Pending benchmark discussion

This specification freezes what each accepted row must prove, but these
campaign-policy choices should be agreed before benchmark implementation:

1. the exact deterministic 1/10/100-MiB fixture generator and whether the
   retained 512-MiB fixture is reused byte-for-byte or regenerated under a new
   manifest version;
2. the exact edit offsets and replacement byte sequences for one-byte, 4-KiB,
   1-MiB, same-count, and forced `+1` reference cases;
3. the practical cold-APFS conditioning procedure, or whether cold state is
   reported `Unavailable` and only controlled warm/unknown campaigns are
   promotion-bearing;
4. whether full reconstruction is streamed to a hashing sink or a temporary
   destination file in Phase 4; the default recommendation is a bounded
   hashing sink because native destination I/O belongs to materialization;
5. the bounded SQLite batch candidates used only after the unoptimized
   baseline identifies statement/API cost;
6. the exact external RSS/physical-I/O observation commands and their timer
   boundaries; and
7. the acceptable analytical work budget for a forced +1 middle insert at
   100 GiB, expressed in rewritten reference occurrences,
   leaves/branches/objects, canonical mapping bytes, and optional
   rewrite-to-capture amplification. Projected latency remains nonbinding
   unless a later specification freezes a calibrated model and safety margin;
   this edit-policy gate never limits file admission.

Until these are settled, benchmark code may implement correctness fixtures and
counter plumbing, but it must not publish a promotion or 200/300-MiB/s claim.
