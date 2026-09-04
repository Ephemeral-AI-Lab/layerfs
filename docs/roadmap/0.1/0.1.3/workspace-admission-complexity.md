# Workspace admission: complexity and reuse targets

See the [concrete file layout and ownership plan](phase-2-shared-code-layout.md)
and [mechanism-adoption audit](phase-2.1-mechanism-adoption-audit.md) for actual
callers, existing implementation status and the two #38 tracks.

The [API/algorithm simplification audit](api-algorithm-simplification-audit.md)
separately records reduction counts. Consolidating three admission accumulators
into one shared implementation is not deleting three public methods, changing
publication ownership, or reducing the indexed insertion complexity. Initial
SDK cleanup removes zero production methods; compatibility-gated API/wire
deletions are separate from performance and complexity claims.

Analysis date: 2026-09-05. Source inspected at
`810bb3a589ac58d103483df34bb58ecfe0f0ddf4` in
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-p21-integration`.
This is a code-based cost model, not a benchmark result. No implementation or
measurement was started; execution remains after #45's infrastructure handoff.

## Scope and variables

The current post-#40 Workspace path builds a selected candidate before calling
`admit_checked_objects`. Admission is not the whole Commit. Full Commit also
includes content/tree construction, candidate reachability, staging, conditional
publication, continuing-view installation and required cleanup.

| Symbol | Meaning |
| --- | --- |
| N | Distinct selected candidate objects presented to admission |
| B | Sum of their canonical byte lengths, including objects already in Store |
| D | Canonical bytes of the selected objects that conflict with existing Store rows; 0 <= D <= B |
| M | Objects present in Store before admission |
| G / P | All retained candidate objects / bytes generated before final selection, including unreachable intermediate objects |
| K | Maximum objects per admission batch: 8,191 |
| L | Maximum canonical payload bytes per batch: 4 MiB minus one byte |
| q | Actual nonempty admission transactions |

Object IDs have fixed length. The database term below assumes the conventional
logarithmic indexed lookup/insertion model, with data-page writes, journaling,
cache effects and filesystem synchronization accounted for separately. It is not
a hard bound on elapsed time under contention or slow storage.

## Current admission time

`objects.rs:2985` implements `admit_checked_objects`:

1. Visit selected candidate objects.
2. Authenticate each selected object's identity: O(B) hashing.
3. Copy its bytes into a carried batch: O(B) additional copying.
4. Attempt one indexed INSERT per selected object.
5. For skipped INSERTs, fetch, authenticate and compare the existing bytes:
   O(D) byte work plus indexed lookups.
6. Commit each bounded batch; release the writer between batches.

Under the indexed-operation model, the admission cost can be expressed as:

```text
T_admit = T_visit + O(B + D + N log(M + N + 1))
          + T_transaction_boundaries + T_physical_IO
```

The separate terms make the limit of the big-O statement explicit: B-tree page
splits, payload/journal I/O, cache misses and commit synchronization can dominate
wall time. A byte/object-linear application loop is not a constant-time database
operation, and increasing the batch size does not remove per-object INSERTs.

Candidate visitation also has a real cost:

- Memory-backed candidate: `visit_prevalidated_order` performs one BTreeMap
  lookup per selected ID, O(N log(G + 1)), excluding byte consumption.
- Spilled candidate: `SpillObjects::visit_ordered` scans record headers in order
  and reads selected object bytes, O(G + B) logical work on that ordered path.
  It can skip unreachable record payloads; seek/storage costs still belong in
  physical I/O. It is not one random index lookup for every selected object.

For N > 0, feasible object sizes and successful admission:

```text
max(ceil(N / K), ceil(B / L)) <= q <= N
```

The lower bound is not an exact packing formula: variable object sizes can force
partially filled batches. q is zero for an empty candidate. #40 raised K from
127 to 8,191 without raising L, so bytes can remain the dominant batch limit.
This reduces transaction overhead in applicable inputs but does not change the
fundamental asymptotic per-object/database work.

## Current admission space

Admission's additional application memory is:

```text
O(L + K + Smax)
```

Here Smax is the largest selected object buffer needed by the visitor. The batch
holds <=L canonical bytes plus up to K object records. Outcome flags, skipped-row
references and other bookkeeping are O(K). A spill reader and SQLite may hold
additional object/page buffers. Include the configured database cache separately.

Because K, L and maximum object size are fixed by policy, this is bounded extra
admission memory with respect to candidate size. It is **not** a claim that the
whole process uses <4 MiB or that the full Commit has constant space.

The upstream candidate remains alive during admission:

- `DeferredObjectStore` starts in memory with an approximately 8 MiB accounting
  threshold; this is not the entire Workspace RSS limit.
- Candidate index, cached references, reachability set and ID-order structures
  have separate accounting; several use individual 64 MiB thresholds.
- Spilled candidate bytes and indexing/order metadata occupy temporary storage
  scaling with generated output, approximately O(P + G) for these fixed-size
  identifiers/records, even if fewer bytes B are finally selected.
- Workspace loaded nodes, file pieces/spools, tree scratch, traversal stacks and
  SQLite caches are additional. A complete memory bound must include them and
  simultaneous ownership, not add only the two headline byte thresholds.

New durable object data grows with newly inserted selected bytes/objects, not B
when most candidates already exist. Physical SQLite file growth also includes
indexes and page allocation; logical deduplication is not an equal disk saving.

## Candidate construction is a separate complexity question

`ObjectBuffer::finish -> reachable_from` runs before admission. It traverses
candidate references, detects cycles and forms a selected object order. Its
cost depends on generated objects, references, cached metadata, spill lookups
and graph shape, in addition to content construction and tree edits.

There is a concrete worst-case caveat: `SpillableObjectSet::contains` switches
from a BTreeSet to scanning an ID file once its memory accounting overflows.
Repeated membership checks can become quadratic for growing sets. The code
explicitly documents this at `objects.rs:1708`. This is upstream reachability,
not the ordinary INSERT loop, and is not evidence that any selected family
actually reaches that regime. Check ID counts/spill activation before adding
another index or treating it as an optimization target.

Publication modifies a fixed number of stage/Commit/Branch records, but indexed
database access is not O(1) in total Store/history size. Required validation and
continuation must also remain explicit. `rebase_committed` still traverses loaded
nodes; faster admission cannot be credited with removing that traversal.

## Existing methods to reuse or refactor

Paths in this table are relative to the inspected checkout. These are candidates
selected by measured costs, not permission to rewrite every listed method.

| Method / type | Role and proposed treatment | Expected complexity/cost effect |
| --- | --- | --- |
| `objects.rs::admit_checked_objects` | Keep checked carried batching; consider consuming selected owned objects instead of borrowing every payload | Remove an extra copy where ownership can move; same broad time complexity |
| `DeferredObjectStore::consume_prevalidated_pages` | Existing test-only owned-page consumer; adapt for production checked admission if memory-resident copies matter | Move in-memory Vec ownership rather than copy O(B) bytes; spill path still copies; no streaming claim |
| `insert_checked_object_batch` | Already shared; retain exact insertion/conflict authentication | Reuse, do not duplicate or weaken; per-object indexed work remains |
| `InitializationObjectSlab`, `InitializationSlabWriter`, `InitializationSegmentAdmission` | Reuse bounded buffering/handoff and checked insertion through a narrow nonempty-Store adapter if candidate replay is material | Could remove full candidate payload write/readback and overlap construction/admission on eligible routes; final selection/closure still required |
| `Workspace::build_candidate`, `build_frontier_candidate`, `build_localized_candidate` | Supply stable selected output to the existing construction/admission primitives; preserve sparse edits and existing capture | Potentially avoid intermediate retained work; quantify generated vs selected bytes and references |
| `directory_apply_sorted_with_budget`, `inode_table_apply_sorted_with_budget` | Already used by Workspace; reuse for eligible native initial construction and improve bounded grouping only where justified | Reduce repeated affected-page processing; no universal O(number-of-edits) promise across all fallbacks |
| `build_initial_directory`, `build_initial_inode_table_from_pairs` | Current native callers still use older initial-tree builders; evaluate transfer to the sorted helpers separately | Possible structural work reduction, subject to same-seed canonical parity and measured comparison |
| `direct_initialize_root_directories_inner` | Reuse existing task queue with better eligible-file distribution if counters justify it | Can improve overlap/wall time; does not inherently reduce total CPU or change asymptotic byte work |
| `PortableMetadataCache::get_or_build`, `rope::build`, incremental file mutation | Already in use; preserve and feed correctly rather than reimplement | Avoid redundant metadata work; preserve unchanged extents instead of full-file rebuilds |
| `Workspace::rebase_committed` | Separate continuation target only if its measured cost remains material | Potentially avoid repeated loaded-node resolution; must retain alias/identity and exact-snapshot semantics |

`InitializationSegmentAdmission::new` currently requires an empty Store. Its
whole implementation cannot simply be substituted for ordinary Workspace
admission. Nonempty Store reuse must preserve historical roots, collision checks,
candidate read-your-writes, selected-root closure, staging and error outcomes.
There is no generic construction pool already available to turn on.

## How to verify the model after #45

Use the migrated Docker-only runner with a matched baseline and candidate.
Record N/B/new/reused bytes, generated candidate bytes, copies, spill/readback,
batch count and occupancy, SQL/transaction time, tree/reference visits, total CPU,
peak memory and temporary/durable storage separately. Keep these in compact logs.

Test one mechanism at a time. An owned-consumption patch succeeds only if its
predicted copy counter falls without changed identities or outcomes. A direct
delivery transfer must demonstrate removed replay traffic and bounded combined
ownership; a scheduling change must report total CPU as well as wall time.
Do not infer an asymptotic improvement merely from one faster timing or from
larger transaction batches. Full original family qualification follows only
after the changed route is stable.
