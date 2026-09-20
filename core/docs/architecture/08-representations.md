# Representations: CDC, CAS and delta

> **Status:** Research; source-backed description of the current tree. Informative,
> not a product contract.

The pending issue #192 schema 7 changes are described in
[save ownership and publication](15-multi-writer-storage.md), based on
`0819f3f39833d477d9ed6d878a50691c3c046a83`. That description supersedes the
older exclusive-save, prefix-publication, shared-private-cache and cleanup rules
below, and adds the returned-read byte bound and bounded ordinal window. The
older source-pinned sections remain historical descriptions; they do not qualify
the pending implementation or a performance change.

Part of the [replacement-core architecture](README.md) set. Source pin
`1884e3eca`; scope, method, measurement status and upkeep are stated in the
[index](README.md).

Chapter numbers are global to the set: this paper holds **chapters 12 and 13**.
Chapter 14 (the delta-hint proposal) is
[`09-delta-hints.md`](09-delta-hints.md).

---

## 12. The four mechanisms

### Notation used throughout

```text
   n    file logical length, bytes
   T    construction cutoff (small_file_threshold_bytes), default 131,072
   c    chunk payload size, 8 KiB .. 32 KiB (frozen profile; target 16 KiB)
   k    chunks in a chunked file, k = ceil(n / c)
   F    mapping-page fanout, 64 .. 128 entries (MAX_MAPPING_ENTRIES = 128)
   L    mapping-tree height, L = ceil(log_F k), bounded by MAX_LEVEL = 31
   e    edits in one stream, bounded by MAXIMUM_EDITS_PER_OPERATION = 4,096
   r    total replacement bytes in one edit stream
   m    chunks newly emitted by one edit (the changed region only)
   d    dependency edges below one object, bounded by the role's depth cap
   N    rows in the `objects` table
   B    objects in one preparation wave, bounded by BATCH_OBJECT_LIMIT = 512
```

### 12.1 Four mechanisms, three questions

The mechanisms are **not alternatives**. They answer different questions at
different layers, and the confusion is common enough to state as a table:

```text
   LAYER            MECHANISM          GRANULARITY       ANSWERS
   ─────            ─────────          ───────────       ───────
   C1  logical      representation      the whole file    "one object, or a tree?"
   C1  logical      CDC                 a region of a     "which regions changed?"
                                        file
   C2  physical     CAS                 identity          "have I stored these exact
                                                          bytes before?"
   C2  physical     DELTA               one new object    "how do I store this
                    (FULL / PREFIX)     vs one base       compactly?"
```

Only **CDC** affects canonical identity. Delta changes the physical record and
nothing else: two files with identical bytes have identical roots however they
are encoded. CAS is what makes identity *useful* — it turns "same identity" into
"no new bytes".

### 12.2 The representation split

`ConstructionPolicy::representation` ([§3.1](03-files.md)) is a single dispatch:

```text
   n == 0        ──► Empty       defined empty file-state form
   n <  T        ──► WholeFile   1 object, LFS5SML value, +23 B canonical overhead
   n >= T        ──► Chunked     FastCdc partitions → FileState over extent tree
```

**Why the line sits at 128 KiB.** CDC cannot define a boundary in a payload
smaller than its own minimum: `MINIMUM_CHUNK_BYTES = 8 KiB`. A 200-byte file would
yield exactly one chunk, and the chunked form would then pay for a chunk object, an
extent slice, a leaf page and a `FileState` to describe what **one** whole-file
object stores directly. Below `T` the whole-file form is strictly smaller and
strictly simpler.

Above `T` the trade reverses: re-storing an entire large file per version makes
every write O(n), and reconstructing a version from one large delta base makes
reads expensive. CDC replaces both with region reuse.

`T` is configurable — any power of two in `[131,072, 1,048,576]` — and **it is a
compatibility boundary, not a tuning knob**: a different `T` produces different
canonical roots for the same bytes. A non-power-of-two `T` is refused at the entry
point, "because it would make the transition probe ambiguous" — the streaming
constructor probes exactly `T` bytes to choose its branch.

### 12.3 CDC — the frozen GEAR profile

```text
   MINIMUM_CHUNK_BYTES =  8,192      NORMALIZATION_SHIFT = 2
   TARGET_CHUNK_BYTES  = 16,384      PROFILE_SEED        = 0
   MAXIMUM_CHUNK_BYTES = 32,768

   profile_id() = BLAKE3( label ‖ sizes ‖ shift ‖ seed ‖ 4 masks ‖ all 256 GEAR values )
```

`profile_id()` is pinned inside every `FileState`, so drift is a
`ProfileMismatch` on decode — **never silent re-chunking**. A file chunked under
one profile cannot be read as though it were chunked under another.

The scanner holds exactly one buffer of `MAXIMUM_CHUNK_BYTES` and never retains
the input. Chunk boundaries are content-defined, so an insertion or deletion
perturbs boundaries only locally and the partition **resynchronizes** a short
distance later — which is what lets unchanged chunks be reused by identity.

### 12.4 CAS — exact reuse, byte-verified

```text
   flush_batch(objects)                     one paged SQL lookup per wave
        │
        ├── seen earlier in THIS wave?   ──► in-memory byte compare
        ├── row exists in `objects`?     ──► reuse_or_collide  ──► REUSE
        ├── pending in this unsealed save? ► seal, then reuse_or_collide
        └── genuinely absent             ──► offer(...) ──► FULL vs PREFIX
```

`cas/membership.rs` states the contract:

> Membership says an identity is present; **it never proves equality**. Every
> occurrence that finds an existing row **compares the stored canonical bytes**
> with the offered bytes and reuses the row only when they are identical. A
> mismatch is a collision and **fails the save; it is not repaired, replaced or
> retried**.

So a BLAKE3 collision is *detected* (`StorageError::Collision`), never silently
accepted. The cost of that guarantee is that reuse **reads the stored object back**
and re-authenticates it, plus whatever chain reconstructs it.

Two ordering facts follow, and both matter:

- **Reuse is decided before delta.** An object that already exists never reaches
  representation selection. You cannot delta your way around a duplicate.
- **Duplicates inside one wave are cheap; across waves they cost a read.** The
  `prepared` map short-circuits a repeat within the same 512-object wave to a
  memory compare. `save.rs` records why this is load-bearing: *"a store that fails
  on repeated content fails on exactly the workload it exists for."*

### 12.5 DELTA — one trial, complete-cost comparison

```text
   prepare a compressed FULL alternative        ← ALWAYS, first
        │
        ▼
   candidate acquisition
        │
        ├── role == Chunk      ──► advisory.first() ONLY  (no cache fallback)
        └── otherwise          ──► advisory in order, then the session
                                    candidate cache by content signature
        │
        ▼
   eligible?   same role  ∧  depth < role cap  ∧  chain within budget
        │
        ├── no candidate at all      ──► FULL   (POLICY)
        ├── candidate absent         ──► FULL   (POLICY)
        ├── candidate ineligible     ──► FULL   (POLICY)
        ├── chain over budget        ──► FULL   (POLICY)
        └── eligible ──► ONE prefix trial, compare complete framed cost
                              │
                              ├── prefix cheaper ──► PREFIX
                              └── else           ──► FULL
        │
        └── codec / allocation / read failure ──► ERROR, never a representation
```

The last line is the no-fallback rule in one place: **four policy outcomes and one
failure outcome**, and `DeltaCounters` counts them separately so a receipt can say
*why* FULL was chosen.

**Only three roles may take a delta base.** `select()` returns early for the other
ten:

```rust
ObjectRole::ExtentLeaf | ExtentBranch | FileState
| DirectoryLeaf | DirectoryBranch | InodeBranch
| FilesystemRoot | AttributeLeaf | AttributeBranch | Symlink
    => return encode_full(...);   // "never choose a payload delta base"

ObjectRole::InodeLeaf => Err(...)  // pooled lane, handled by select_pooled
```

This is easy to assume the other way. Tree pages carry an `UnchangedPrefix`
predecessor from `sorted/page.rs`, but that predecessor is **not** consumed:
`placement.rs` never reads predecessors, and the only consumer in C2 hands them to
`select()`, which discards them for tree roles. See [§13.6](#136-what-is-not-established).

### 12.6 The funnel

For one region of a 1 GiB file, edited in place:

```text
   1 GiB file, ~4 KiB changed in the middle
        │
        ▼
   ┌─ C1 · CDC ───────────────────────────────────────────────┐
   │  only the changed region is re-chunked                    │
   │  unchanged ranges stay as extent slices referencing        │
   │  EXISTING payload ObjectIds — never re-emitted             │
   │  ~65,000 chunks in  ──►  ~2 new chunks out                 │
   └──────────────────────────┬────────────────────────────────┘
                              ▼
   ┌─ C2 · CAS ───────────────────────────────────────────────┐
   │  is a new chunk byte-identical to a stored one?            │
   │  → reuse the row; emit nothing                            │
   │  ~2 in  ──►  0–2 out                                       │
   └──────────────────────────┬────────────────────────────────┘
                              ▼
   ┌─ C2 · DELTA ─────────────────────────────────────────────┐
   │  one eligible base, PREFIX strictly cheaper than FULL      │
   │  → store the framed diff                                  │
   └───────────────────────────────────────────────────────────┘
```

Each stage only ever sees what survived the previous one. That is the whole
architecture of storage cost in this product: **CDC bounds how many objects a
change produces; CAS removes the ones that already exist; delta shrinks what is
left.**

---

## 13. Transitions and storage economics

### 13.1 The transition matrix

`apply_edits` dispatches on the representation of the **result**, never the base:

```text
                      result: WholeFile          result: Chunked
                    ┌──────────────────────┬──────────────────────────────┐
   base: WholeFile  │ assemble_final       │ stream_combined              │
                    │ read retained ranges │ RE-CHUNK THE WHOLE RESULT    │
                    │ → one object         │ through FastCdc              │
                    │ + predecessor = base │ no predecessor (None)        │
                    ├──────────────────────┼──────────────────────────────┤
   base: Chunked    │ assemble_final       │ replace_chunked              │
                    │ read retained ranges │ splice: split at start,      │
                    │ from the extent tree │ split tail, SLICE boundary   │
                    │ → one object         │ extents, chunk only the      │
                    │ + predecessor = base │ replacement                  │
                    │   (role mismatch ⇒   │ + predecessor = retained     │
                    │    ineligible)       │   neighbour (one, shared)    │
                    └──────────────────────┴──────────────────────────────┘
```

### 13.2 What a transition costs

Both directions are bounded by the **result**, never the base — which is better
than it first appears:

| Transition | Time | Peak memory | Storage added |
| --- | --- | --- | --- |
| small → large (`stream_combined`) | **O(n_result)** | O(T) resident base + 32 KiB chunk buffer | O(k_result + k_result/F) |
| large → small (`assemble_into`) | **O(n_result) ≤ O(T)** | one buffer of `n_result + 23` — the object itself | O(n_result) |

`assemble_into` reads only *retained* ranges and allocates only the object:

```rust
out.try_reserve_exact(final_len as usize)?;
// A chunked base assembles through ONE ordered cursor, so a mapping page two
// retained runs share is demanded once for the whole assembly.
let mut cursor = view.file_state()?.map(|state| RangeCursor::new(reader, state, scope));
for segment in &segments {
    match segment {
        Segment::Retain { base } => cursor.read_segment(base.0..base.1, &mut out)?,
        Segment::Replace { index, len, .. } => {
            // The replaced base range is deliberately not read
            append_replacement(source, index, len, &mut out)?;
        }
    }
}
```

`begin_whole_file_object(capacities, final_len)` sizes that one buffer — the
canonical envelope, the value header and `final_len` payload bytes reserved
exactly, so no append reallocates — and writes the framing; the assembly appends
into it and `FinalizedObject::new` **moves** it. There is no separate payload, no
separate value and no canonical copy of either, so the whole-file route's peak is
`final_len + 23` rather than the `4n + 184` a measured probe showed while the
payload, the value and the canonical object were three live allocations at once.
`encode_whole_file` remains the complete-construction encoder and the reference
the edit's bytes are checked against.

So shrinking a 1 GiB file to 128 KiB reads **128 KiB**, not 1 GiB. The discarded
range is never touched. Because a whole-file result is definitionally below `T`,
**large → small is bounded by the cutoff regardless of how large the base was.**

The cursor changes the *provider demands* of that read, never its bytes: R retained
runs pay `O(R·h)` mapping-page demands without it — one root-down traversal each,
even for the root they all share — and `O(h + shared)` with it. Payload demands are
unchanged: a chunk that straddles two retained runs is still read once per run,
because a run is served exactly and independently. Emission order and the emitted
object are untouched.

### 13.2.1 The operation's shared page memo

One `apply_edits` builds one `PageCache` and hands it to both passes. The
comparison pass navigates the base through a `RangeCursor` over it — one descent
for all of a replacement's windows instead of one per 64 KiB window — and the
construction pass consults the same memo in `EditObjects::load_node` **before** the
`nodes_read` charge and before the reader demand, so a mapping page the comparison
already acquired is neither read nor charged twice. A memo hit still decodes under
the caller's root context: only canonical bytes are shared, never decoded nodes,
because the two contexts validate different partition rules.

That is where the chunked route's `nodes_read` saving comes from: on the D27 shape
the mapping root and one leaf are each demanded once by the comparison and again by
the split descent, and the memo turns the second pair into hits — `nodes_read`
9 → 7. A pure deletion is unaffected: a length-changing edit is a difference by
construction, so the comparison returns before it reads any base byte. The
comparison verdict, its emission behaviour, and the `edit.compare` timing child are
all unchanged — the memo only answers demands that would otherwise re-read a page,
so **an error raised by a memo hit is raised at the point of the later demand
rather than at a re-read**, which no test pins today.

Growing across the boundary is the expensive direction: `stream_combined` must
chunk the entire result. That is inherent — the base was a whole file, so there
were no chunks to reuse — and it streams, so memory stays flat no matter how large
the result becomes.

### 13.3 No reclamation — the economics that follow

Verified by exhaustion: the **only** `DELETE` statements in `layerfs-storage` are
in `sqlite/cleanup.rs`, and both serve `abandon()` — cleanup of a *definitely
failed, unpublished* save. There is no GC, no reclamation, no vacuum, no
compaction and no reachability sweep anywhere in `core/`.

```text
   A representation transition ADDS a representation.
   It never converts one. The old one is not reclaimed.

   small → large                         large → small
   ─────────────                         ─────────────
   + chunks (ALL FULL, O(n) bytes)       + 1 whole-file object, O(n) bytes
   + mapping pages                       + nothing else
   + 1 FileState
   old whole-file object REMAINS         old chunks + mapping + FileState REMAIN
```

Three consequences:

1. **A transition roughly doubles that file's storage.** Large → small on a 1 GiB
   file adds a 128 KiB object while **1 GiB of now-unreferenced chunks stays on
   disk**.
2. **Oscillation across the cutoff accumulates without bound.** A file edited back
   and forth across `T` N times leaves N representations, and nothing will ever
   reclaim them.
3. **This is partly intentional.** A superseded representation is *history*, not
   garbage — history is the product. But a transition is not a storage-neutral
   reformat, and a workload that bounces across `T` is a storage leak in practice.

### 13.4 Transitions take no delta base — in either direction

```text
   small → large    push_chunk(chunk, None, consumer)     ← hardcoded None
                    ⇒ no predecessor ⇒ no_candidate ⇒ ALL FULL

   large → small    predecessor = view.root()   — the FileState
                    eligible() rejects: location.role != role
                    ⇒ ineligible ⇒ FULL
```

Neither direction *loses* delta encoding — **neither ever had it**. And nothing is
destroyed either: the old objects keep their chains intact in storage, and are
simply no longer referenced by the new root. There is no chain *breakage*, only
chain *abandonment*.

### 13.5 Complexity summary

Time, memory and storage for every operation in this paper. `O(1)` below means
*independent of file size*; the constants are declared, not incidental.

| Operation | Time | Peak memory | Storage |
| --- | --- | --- | --- |
| Construct whole-file | O(n) | **O(n)** — ≈2n peak | O(n) |
| Construct chunked | O(n) | **O(c + L·192)** — flat | O(k + k/F) |
| Read whole-file | O(n) | O(n) | — |
| Read chunked | O(n + d·m) | **O(1 MiB + 4 MiB)** — flat | — |
| Edit, whole-file, differs | O(n) | O(n) | O(n) |
| Edit, whole-file, identical | O(min(n, window)) | O(64 KiB) | **0** |
| Edit, chunked | **O(r + L)** | O(L · page) | O(r/c + L) |
| Transition small → large | O(n_result) | O(T + c) | O(k + k/F) |
| Transition large → small | **O(n_result) ≤ O(T)** | O(n_result) | O(n_result) |
| CAS lookup, per wave | O(B log N) | O(B) | — |
| CAS verify, per reuse | O(size + d) | O(size) | — |
| Delta selection, per object | O(size + d) | O(size) | — |

The two rows worth staring at are the flat ones. **Chunked construction and chunked
reads are both O(1) in file size**, because the mapping builder retains only
boundary pages (`stream_flush_entries = 192`, ≤ 31 levels) and the reader demands
payloads in waves of 32 objects (≤ 1 MiB) with a 4 MiB dependency-pack cache.
Those are the same ceilings §9 lists; here they are as asymptotics.

The `d·m` term in chunked reads is the chain cost: `m` chunks are chained (those
that changed at some point) and each costs `d + 1` object reads. For a full-file
read of a 1 GiB file with 65,000 chunks of which 3 are chained at depth 4, that is
65,012 reads instead of 65,000 — **negligible**. Amplification only bites when
chains are *dense*, that is when most chunks have been modified at some point.

### 13.6 What is not established

Recorded as `unknown` rather than guessed:

- **The `UnchangedPrefix` predecessor on tree pages is inert.** `sorted/page.rs`
  sets it; nothing consumes it (`select()` discards it for tree roles, and
  `placement.rs` never reads predecessors). Either it is reserved for a placement
  policy not yet written, or it is dead metadata. Bounded, deduplicated and
  harmless either way — but not acted on.
- **Whether delta is worth its read cost.** [§12.5](#125-delta--one-trial-complete-cost-comparison)
  describes the mechanism; the storage it recovers and the read work it adds are
  measurement questions. [`09-delta-hints.md`](09-delta-hints.md) states the test.
- **The magnitude of transition storage growth** is a structural consequence of
  "no `DELETE` exists", not a measured figure.

### 13.7 How the reference tree compares (v0.1.6)

`docs/roadmap/0.1/0.1.7/architecture-overview.md` describes the **reference** tree
under root `crates/`. On this paper's subject the two are largely the same design:

| Aspect | Reference (`crates/`) | Core (`core/`) | |
| --- | --- | --- | --- |
| GC / reclamation | none — measures `unreachable_bytes`, never deletes | none | shared |
| Chain budgets | 512 KiB canonical / 256 KiB encoded | identical | shared |
| Whole-file depth | `CHAIN_EDGES = 8` | 8, named + configurable | shared |
| Chunk depth | `depth >= 4` as a bare literal | 4, named + configurable | shared |
| Predecessor slots | `prior_ids: [Option<ObjectId>; 4]` | `MAXIMUM_ADVISORY_PREDECESSORS = 4` | shared |
| Fresh chunking hints | none | none | shared |
| **Per-chunk positional hints** | **`PredecessorCursor`** | **`PredecessorCursor`** (`file/mapping/predecessor.rs`) | shared in kind, narrower in reach |

The reference has exactly three product `DELETE`s and none reclaims superseded
data; its Monitor computes `unreachable_objects` / `unreachable_bytes` and nothing
acts on the number. **The storage economics in §13.3 are inherited, not
introduced.** Core is a faithful port of them.

The one substantive difference was the missing cursor, which is the subject of
[`09-delta-hints.md`](09-delta-hints.md). The cursor now exists in `core/`, but it
is consulted by the **complete-construction** route only: the reference attaches it
to every emitted object that carries a span, while `replace_chunked` and
`stream_combined` still pass no cursor. The reach of the two trees therefore still
differs even though the mechanism no longer does.
