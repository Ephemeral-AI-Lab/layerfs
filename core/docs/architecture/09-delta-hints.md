# Positional delta hints — a proposal

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
>
> **DRAFT — PARTLY BUILT.** §14.4's cursor now exists in `core/`
> (`file/mapping/predecessor.rs`, reached through
> `construct_bytes_with_predecessor`) and has been measured on one retained-history
> lane; §14.6's four-case plan has **not** been run, so §14.7 and §14.8 are still
> hypothesis rather than finding. See [§14.1](#141-status-and-authority) and
> [§14.9](#149-what-would-finalize-this-paper).

Part of the [replacement-core architecture](README.md) set. Source pin
`1884e3eca`; scope, method, measurement status and upkeep are stated in the
[index](README.md).

Chapter numbers are global to the set: this paper holds **chapter 14**.
[`08-representations.md`](08-representations.md) holds chapters 12–13 and is the
descriptive counterpart this proposal builds on.

---

## 14. Positional delta hints

### 14.1 Status and authority

| Part | Kind | Authority |
| --- | --- | --- |
| §14.2 the gap, §14.3 the mechanism | **Descriptive** — read from source at the pin, both trees | Same as the set |
| §14.4 the design | **Built** in `core/` for the complete-construction route; still proposed for the `apply_edits` routes | Source |
| §14.5 bounds | **Built** as named constants; the four-slot reduction is still proposed | Source for the constants, draft for the reduction |
| §14.6 the measurement plan | **Proposed** — one lane measured, the four cases not run | Draft |
| §14.7 expectations | **Hypothesis**, explicitly not a finding | Do not cite |

This paper is the second in the set to contain guidance rather than description.
The first is [`07-importing.md`](07-importing.md); the same convention applies —
the status differs per section and the reader is told which is which.

### 14.2 The gap

Core offers one delta base per chunk, and it is the **wrong chunk**:

```text
   CORE — file/edit/apply.rs, replace_chunked

   ┌──────────────────────────────────────────────────────────────┐
   │  // continue the retained payload immediately before the      │
   │  // insert position, which is a physical hint only.           │
   │  let predecessor = rightmost_payload(&mut objects, left)?;    │
   │                                                               │
   │  FastCdc::new().scan(source, |chunk| {                        │
   │      builder.push_chunk(chunk, predecessor, &mut sink)        │
   │  })                        ▲                                  │
   │                            └── ONE id, shared by EVERY chunk   │
   │                                of the replacement              │
   └──────────────────────────────────────────────────────────────┘
                              │
                              ▼
   AdvisoryPredecessors ──► save.rs collects predecessors().ids()
        (4 slots)                    │
                                     ▼
                          select(): Chunk arm reads advisory.first()
                                     │
                          ✓ slot 0 is used
                          ✗ slots 1–3 are NEVER filled by any producer
                          ✗ slot 0 holds the chunk BEFORE the edit,
                            not the chunk AT this position
```

For an **in-place overwrite**, the useful base is *the previous version of this
exact region*. Core offers the region's left neighbour instead — which in a binary
or a text file is unrelated content, so the trial usually loses and the chunk is
stored FULL.

**This diagram is the `apply_edits` route, and it is still true.** The
complete-construction route (`construct_bytes` → `construct_chunked` →
`build_streaming`) had a *different* gap at the pin — it offered `None` to every
chunk — and that one is now closed: the route consults the cursor in §14.4 when the
caller offers a previous version's stored root. `replace_chunked` and
`stream_combined` are unchanged and still offer the left neighbour and nothing,
respectively.

The reference tree does not have this gap:

```text
   REFERENCE — crates/layerfs-content/src/file/rope/read.rs

   PredecessorCursor::hints(store, start, len) -> [Option<ObjectId>; 4]
        │  walks the BASE file's extent tree in logical order
        │  (last_end enforces monotonic span order; a regression is
        │   InvalidRecord("predecessor span order"))
        │  returns up to 4 base extents OVERLAPPING [start, start+len)
        │  deduplicated; exhausted after 4,096 descriptors
        ▼
   objects.rs:  object.1.prior_ids = cursor.hints(&CoreReader(reader), start, len, …)
```

The design documentation for this release says to keep it:

> In the current CHUNK path, a cursor supplies up to four previous-file overlap
> hints, but admission tries only the first. […] **Preserve bounded
> predecessor/range information through the new boundary**; adding another search
> strategy is a separate measured change.

Core preserved the *carrier* — `AdvisoryPredecessors` crosses the C1/C2 boundary
with four slots and provenance tags — and dropped the *producer*. Against that,
`PredecessorProvenance::ReusedRange` is never constructed by anything in `core/`,
which is the shape of an unfinished port rather than a decision.

### 14.3 What a positional hint is worth

The value is **hint quality, not hint count**. Both implementations try exactly one
candidate:

```text
   reference   object.prior_ids().iter().flatten().next()   ← first non-None
   core        advisory.first()                             ← first only
```

So the four slots are a fallback structure in *both* trees, and the difference is
which single base is offered:

| | Base offered for a new chunk at logical offset `X`, length `L` |
| --- | --- |
| Reference | the base extent **overlapping `[X, X+L)`** — the old content at that position |
| Core | `rightmost_payload(left)` — the extent immediately **before the edit point** |

The two coincide only for an edit at the very end of the file. Everywhere else they
differ, and for the case delta is best at — an in-place modification of
incompressible content — the reference offers the right answer and core does not.

### 14.4 The design — built for complete construction

```text
   BUILT — a cursor over the retained base mapping

   ┌────────────────────────────────────────────────────────────────────┐
   │  struct PredecessorCursor {                                        │
   │      stack:     Vec<(Summary, u64)>,   // right spine, logical order│
   │      position:  u64,                   // next unread base offset  │
   │      last_end:  u64,                   // monotonic-order guard    │
   │      descriptors: usize,               // work bound               │
   │      exhausted: bool,                                              │
   │  }                                                                 │
   │                                                                    │
   │  fn hint_for(&mut self, start: u64, len: u32) -> Option<ObjectId>  │
   │      // Walk forward only. Return the first base extent whose      │
   │      // span overlaps [start, start+len). Never rewind.            │
   └────────────────────────────────────────────────────────────────────┘
                                   │
        consulted per chunk inside ExtentBuilder::push_chunk, where the
        logical offset is already known (build.logical_len)
                                   │
                                   ▼
   push_chunk(chunk, self.hint_for(offset, chunk.len()), consumer)
                                   │
                                   ▼
        IDENTICAL consumer path — no storage change, no new record form
```

**What was actually built, and where it differs from the sketch above.** The file
is `core/crates/layerfs-content/src/file/mapping/predecessor.rs`; the entry point is
`construct_bytes_with_predecessor`, and the cursor is threaded into
`build_streaming_with_predecessor`. Three differences from the sketch:

| Sketch | Built |
| --- | --- |
| consulted inside `ExtentBuilder::push_chunk` | consulted in `build_streaming_with_predecessor`, which owns the running offset (`builder.logical_len()`) — `push_chunk` keeps its signature |
| `hint_for(start, len) -> Option<ObjectId>` | `hint(start, len) -> ContentResult<Option<ObjectId>>`; the fallible part is the mapping-page read |
| base supplied as a root | base supplied as `PredecessorBase` (a provider and a root); opening it costs one read and is what declines a base that is not chunked |

The descriptor ceiling is `PREDECESSOR_DESCRIPTOR_LIMIT = 4_096`, exhaustion is not
an error, and span order is enforced exactly as §14.5 proposes.

**Why this is small.** The whole change is one producer:

| Element | Change needed |
| --- | --- |
| `ExtentBuilder::push_chunk(raw, Option<ObjectId>, consumer)` | **none** — already takes an optional base |
| `AdvisoryPredecessors` (4 slots, provenance) | **none** — already crosses the boundary |
| `FinalizedObject::with_predecessors` | **none** |
| `save.rs` `predecessors().ids()` collection | **none** |
| `select()` Chunk arm `advisory.first()` | **none** — a better value in slot 0 is the fix |
| The cursor, and consulting it per chunk | **new** |

`ExtentBuilder` already tracks the logical offset (`build.logical_len`), so the
cursor can be consulted without changing `FastCdc::scan`'s callback signature,
which yields only chunk bytes.

### 14.5 Proposed bounds

Carried over in shape, because an unbounded walker on the write path is worse than
a poor hint:

| Bound | Proposed | Rationale |
| --- | --- | --- |
| Span order | `start >= last_end`, else `InvalidRecord` | A rewind would make the cursor a search; forward-only keeps it linear and cheap |
| Descriptors per operation | a declared ceiling (reference: 4,096) | Bounds walk work on a pathological tree |
| Exhaustion | on ceiling, stop hinting — **not** an error | A hint is advisory; exhausting hints must never fail a save |
| Hints per object | **one** | see below |
| Depth eligibility | unchanged — `depth < chunk_delta_max_depth` | The cursor changes *which* base, never *whether* a base is allowed |

**Why one slot and not four.** The four overlapping base extents of a ≤ 32 KiB
chunk are near-simultaneous versions of one region: if the first is
depth-ineligible, its neighbours almost certainly are too, because they were
written in the same version sequence. Fallback would rarely fire. That argues for
filling slot 0 correctly and **reducing `AdvisoryPredecessors` to a single slot**
rather than the opposite — which is a simplification the current type does not earn.

### 14.6 The measurement plan

The proposal is only worth building if it wins on a stated workload. Two arms,
identical inputs, one sample per case per arm:

```text
   ARM A   core as it stands  (one hint: the extent before the edit)
   ARM B   core + positional cursor

   CASE 1  incompressible large file, repeatedly edited in place
           (binaries, images, database pages) — where hints pay
   CASE 2  compressible large file, repeatedly edited in place
           (source, text, JSON) — the CONTROL, where hints are expected to lose
   CASE 3  append-only growth — no overlap, hints should be inert
   CASE 4  rewrite-in-place with no similarity — hints should be inert
```

Reported per arm, from counters that already exist (see
[`10-counters.md`](10-counters.md)):

| Counter | What it answers |
| --- | --- |
| `DeltaCounters.prefix_selected` vs `no_candidate` | did the hint actually win? |
| `DeltaCounters.full_losses` | how often a trial was paid and lost |
| `DeltaCounters.trials` | total trial work |
| `ChainCounters.edges`, `max_depth`, `objects` | read work the chains added |
| retained bytes and pack count | storage recovered |
| `EditCounters.payloads_created`, `payload_bytes` | new payloads, independent of encoding |

Two cautions the design documentation itself raises, and which any receipt must
respect:

> **Record savings do not alone prove Store savings or speed.**
> **An early base choice can force a later FULL reset.** Compare complete retained
> history and exact readback, not a sum of individually best pairs.
> **Low delta selection can be healthy** when exact reuse already avoids new
> records.

So the receipt must account for the retained graph chronologically — every unique
retained record including dependency-only bases, counted once — not sum per-pair
savings.

### 14.7 The trade, stated honestly

Delta is a **storage-for-speed trade**, and the speed side is negative. Removing
hints is *not* a regression in both directions:

| | With positional hints | Without (core today) |
| --- | --- | --- |
| Write time per changed chunk | + base acquire + one prefix trial | trial against the wrong base, usually lost |
| Read time per **chained** chunk | `O(d+1)` object reads | `O(1)` object read |
| Read time per **unchained** chunk | `O(1)` | `O(1)` |
| Storage per changed chunk | `O(δ)`, the diff | `O(c)`, the whole chunk |

Asymptotically: hints move storage per changed chunk from **O(c)** to **O(δ)** and
read cost from **O(1)** to **O(d+1)** for those chunks — bounded, because
`chunk_delta_max_depth = 4` caps `d` at 4.

The read side is smaller than it first looks. Amplification applies only to
*chained* chunks, and chains are sparse after light editing. For a 1 GiB file of
65,000 chunks with 3 chained at depth 4, a full read costs 65,012 object reads
instead of 65,000 — negligible. Amplification only bites when most chunks have been
modified at some point, which is the same workload where storage savings are
largest.

### 14.8 Hypothesis, not finding

Stated so it can be falsified rather than cited:

- On **incompressible** content, a correctly-chosen positional base should win the
  `37 + prefix_frame < 5 + full_frame` comparison by a wide margin, because zstd
  cannot compress the FULL alternative at all.
- On **compressible** content, zstd FULL is already small, so PREFIX should win
  rarely and the trial cost should be a net loss.
- Therefore the eventual design is probably **conditional** — take the positional
  hint only when a cheap compressibility signal says it will pay — rather than
  universal.

None of that is measured. It is a hypothesis with a stated falsification test,
not a finding.

### 14.9 What would finalize this paper

1. Implement the cursor behind the existing 4-slot type, with no storage-format
   change. — **done** for the complete-construction route (working tree at
   `66bce8378` + the L4 change). It remains **not done** for `replace_chunked` and
   `stream_combined`.
2. Run §14.6's four cases, two arms, and record the whole retained graph. — **not
   run.** One retained-history lane was measured instead; that reading is in
   [the L4 receipt](../../../../docs/roadmap/0.1/0.1.7/evidence/stage-6-history-188c-20260920T000000Z/L4/README.md)
   and is **not** the four-case plan this step asks for.
3. Either promote this paper to a description of shipped behaviour, or delete it
   and record the rejection in the set's index.

Until step 3 this paper stays a draft, and any number taken from it is an estimate.
A number measured from the built cursor is a measurement of that lane, not of this
paper's plan.
