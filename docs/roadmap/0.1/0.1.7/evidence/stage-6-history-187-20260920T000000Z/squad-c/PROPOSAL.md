# The proposed change — diagram, mechanism, and why it cuts

**Basis: apparent bytes (st_size), stride10, 17 states.** All figures measured unless marked
**[computed]**. Nothing here is admission evidence.

---

## 0. Where 47,048,435 B sits

`history-stride10`, the 17-state selection `{1, 11, ..., 151, 157}`.

````
128,864,256   the registered lane today (apparent; = page_count x page_size, exact)
 -65,126,400  CAUSE 1  the driver declares no cross-commit base      [MEASURED]
 -----------  -> 63,737,856   the faithful model, depth-capped
  -9,670,811  LEVER 1  cross-path-first base selection (R4)          [computed from B2]
 -----------  -> 54,067,045
  -7,018,610  LEVER 2  lean SQLite row grammar                       [computed, both sides measured]
 -----------  -> 47,048,435   PROPOSED
  49,315,840  v0.1.6 stride-10, retained artifact sha256 80c2b10a...
 -----------  margin 2,267,405 B = 0.954x
````

---

## 1. Today — the advisory list is empty, so the only base is a same-save near-duplicate

````
  C1  construct_bytes(bytes)                 C2  cas/save.rs:100
+-----------------------------+            +--------------------------------------+
| FinalizedObject::new(       |            | advisory = predecessors().ids()      |
|     WholeFile, canonical)   | predecessors|          = []                        |
| .predecessors = EMPTY       | ---- [] -->|   ^ EMPTY ON THIS PATH. See note.    |
+-----------------------------+            +------------------+-------------------+
                                                              |
                                                              v
                            select.rs:342  acquisition(role, advisory=[], cap)
                                           for id in [] { probe(id) }   ->  None
                                                              |
                                            fallback: the PER-SAVE candidate cache
                                            - WholeFile lane only (all 4 insert sites
                                              guarded by  lane == WholeFile)
                                            - dropped at the end of every save
                                              (cas/lifecycle.rs:90, cas/store.rs:392)
                                            - 32-byte min-hash signature,
                                              SLOTS=1024, REFERENCES=8192
                                                              |
                                      +-----------------------+-----------------------+
                                      v                                               v
                                found: 18,344                                   none: 26,847
                                PREFIX record 4.770x                            FULL record 2.842x
                                (ALL same-save; 0 cross-save)                   (forced: nothing offered)
````

**The measured consequence.** Whole-file lane, 44,148 objects / 348,460,709 B canonical:

| | stored | ratio |
|---|--:|--:|
| 25,804 objects with no base | 93,744,892 | **2.842x** |
| 18,344 objects with a same-save base | 17,196,162 | 4.770x |
| **total** | **110,941,054** | **3.141x** |

The 2.842x bucket is 266,427,536 B of canonical content compressed **as if it had never existed**.

---

## 2. Proposed — C1 declares 4 candidates from the whole retained history

````
  C1  declares, in B2's measured preference order      C2  cas/save.rs:100
+--------------------------------------+              +-------------------------------+
| AdvisoryPredecessors   (max 4)       |              | advisory = [c1, c2, c3, c4]   |
|   1. cross-path best     <-|         |              +---------------+---------------+ 
|   2. cross-path 2nd        | 4 slots |  ---------->                 |
|   3. same-path PREVIOUS    |         |                              v
|   4. same-path earlier   <-|         |   acquisition(): probe IN ORDER, first eligible wins
+--------------------------------------+                              |
                                                                      v
                                        encode_prefix ->  tag(1) + base_oid(32) + zstd_frame
                                                          ^^^^^^^^^^^^^^^^^^^^^
                                                          the base content is the RAW PREFIX
                                                          DICTIONARY for the frame
````

**Order matters, and it was measured, not assumed** (B2, with the product's own byte-exact codec):

| order | whole-file lane stored |
|---|--:|
| same-path previous **first** | 45,926,982 |
| **cross-path best, cross-path 2nd, same-path prev, same-path earlier** | **35,937,886** |
| ...plus the measured cache elsewhere | **34,839,722** |

Putting `previous` first costs **297,925 B**. And "try all four and keep the smallest frame" is
**worse by 433,443 B** — it deepens chains and pushes later objects past the depth cap.

---

## 3. Why it is better than the current one

**Correction (Squad E1, measured).** An earlier draft of this document said "the advisory list is
empty — never populated". That is true **only of the `construct_bytes` path the history driver uses for
file content**. There are exactly **three** product producers of `with_predecessors` in
`core/crates/*/src`, and one of them is **live**:

| producer | provenance | base it names | role |
| --- | --- | --- | --- |
| `file/edit/apply.rs:117-122` | `OriginalBase` | `view.root()` (the declared edit base) | WholeFile |
| `file/mapping/build.rs:125-130` | `UnchangedPrefix` | the chunk payload a run continues (only `apply.rs:309-325` passes `Some`; `build_streaming` passes `None` at :326) | Chunk |
| **`filesystem/sorted/page.rs:380-390`** | `UnchangedPrefix` | **`page.origin`** — the stored node a changed page was materialised from, i.e. a node of the **previous** state's root | DirectoryLeaf, DirectoryBranch, **InodeLeaf**, InodeBranch |

That third producer is the **sole source of cross-save bases in the unmodified lane**: **917 of 1,738
InodeLeaf rows carry a base (52.76 %)**, and the InodeLeaf route (`select_pooled` → `pool_base`) can
obtain a base **only** from the advisory slice — it never consults the candidate cache. And three of
the four roles it covers are then **silently dropped** by the ten-role short circuit at
`select.rs:204-222`.

So the precise statement is: **on the file-content path the driver uses, the advisory list is empty;
elsewhere in the product it is live and is already buying 917 cross-save bases.** The correction makes
the diagnosis *sharper*, not weaker — the mechanism is proven to work end-to-end in this very Store,
for a different role.

The whole-file rule is still not a rule: the only bases the whole-file lane gets come from the per-save
candidate cache, which can only ever see *near-duplicates inside the same state*, and is thrown away at
every save boundary.

The proposal draws on the **whole retained history**, and the corpus has far more to offer there:

| source of a base | objects covered | whole-file lane stored |
|---|--:|--:|
| today (same-save cache only) | 18,344 | 110,941,054 |
| same-path **previous** version | 28,560 | 45,926,982 |
| same-path **any** earlier version | +1.5 % only — a negative result | 45,230,840 |
| **cross-path, 4 slots, cross-first** | **35,407** | **35,937,886** |

**Why cross-path wins:** **33,210 of 43,887 non-first-state objects (75.7 %)** have an already-stored
near-identical object at a **different** path — vendored copies, duplicated files, shared generated
artifacts. Same-path *any-earlier* is worth almost nothing (+696,142 B, 1.5 %), so the win is not
"look further back", it is "look sideways".

---

## 4. Why it is expected to cut — the mechanism, in one measurement

A whole-file record is compressed **alone** unless it is given a base. B3 measured both regimes on
29,222 real stride10 modified records (265,065,265 B canonical):

| regime | stored | ratio |
|---|--:|--:|
| compressed **alone** | 89,063,696 | 2.976x |
| compressed with the **previous version as a raw prefix dictionary** | **11,319,962** | **23.416x** |

**The dictionary is worth 7.8678x.** My own full-population sweep (29,302 pairs, no sampling,
285,191,546 B) agrees: 23.823x at the product's own level 3, 28.584x at level 19 — within 1.7 % of
B3's figure, from a separate implementation.

So the cut is not a codec trick. It is **moving 266 MB of content out of the "alone" regime and into
the "against a stored neighbour" regime**. B2 measured that move end-to-end with the product's own
encoder: whole-file lane **44,510,533 -> 34,839,722**, i.e. **-9,670,811 B**.

**[computed]** The 47,048,435 figure applies that delta to my measured `l7` Store.

---

## 5. Lever 2 — why the schema decides the gate

| | today | v0.1.6 |
|---|--:|--:|
| non-pack SQLite bytes | **10,277,718** | **3,259,108** |
| per object (52,032 / 51,722) | **197.5 B** | **63.0 B** |
| share of the Store | **16.1 %** | 6.6 % |

B4's `dbstat` split of the current Store: `objects` 67.5 B/row + `objects_locations` 49.0 B/row +
`objects_bases` 86.0 B/base-row. v0.1.6 carries the base identity **inside the record**
(`tag u8 [+ 32-byte base oid] + frame`), so it needs neither the bases table nor a separate
locations index.

**Why this is the deciding term, and why it gets worse, not better:** B3 measured that the non-pack
bytes are **NOT reduced by grouping** — it is a *fixed* cost per object. As the pack bodies shrink,
it grows as a share. That is exactly why B4, working blind, named the schema as sensitivity #1:

````
B4 modelled floor, real grammar + REAL schema   56,162,647   ->  NOT below 49,344,512 (+13.9%)
B4 modelled floor, real grammar + LEAN schema   47,648,422   ->  below the gate
````

B4 and my Step 0 landed on the same deciding term from opposite directions.

---

## 6. The two things that must land first, and are NOT in the 47,048,435 figure

1. **The depth-measurement defect.** The faithful model is *rejected* by the product:
   `Integrity("dependency chain depth")`. **The bounds themselves are consistent** — E4 measured
   the product's own `delta_chains` suite (9 passed, including a cap-2 policy storing *and reading
   back* a 2-edge chain). The defect is that `DepthCache::cost_of` records every level of a
   cache-hit walk **one edge short**, so `eligible` admits a base whose true depth is the cap.
   Proved by the writer's own output: `/tmp/s0_l7` holds **4 objects at 9 edges whose bases sit
   at true depth 8**. Fix: **~4 lines, one function, no format change — and it buys legality, not
   bytes (~2,248 B)**, because every base at depth >= 8 is refused under a cap of 8. **It does not
   unlock lever 1's bytes**; lever 1 comes from base *selection*, which is a different change.
2. **A producer.** A benchmark driver is not a layer. The layer that owns "this path's previous
   version" is **Stage 7 / Workspace**, and the Stage 7 specification does not exist (A3).
   **E4's finding: the declaration itself needs ZERO lines of core** — `AdvisoryPredecessors` already
   rides the C1/C2 boundary and is already consumed. What is missing is only the *caller that holds
   the map*, and the harness has proved a `BTreeMap<path, ObjectId>` is sufficient.

Both are `core/crates/` changes. **Neither is made here; both need an owner ruling.**

---

## 7. Honest statement of the margin

**2,267,405 B, 0.954x of v0.1.6.** Three independent blind results say that is thin:

- **B4:** the corpus admits nothing below 49,344,512 B under the real schema — the lean schema is
  what crosses the line, and that is one assumption wide (-8,514,225 B on its model).
- **B1:** a store rebuilt at its own P2 floor lands at **0.995x-1.051x** of the gate.
- **B3:** grouping is worth 1.2637x more, but it makes the 18,344 already-based records **2.95 %
  worse** and costs **59.8x** single-record read amplification.

**Where real margin would come from:** B1's P1 — **27,184,431 B (13.682x)** at 254 group frames,
<= 11.26 MB decoded per read. That needs a grouping change and an access-cost trade.

**No timing was taken anywhere in this campaign** — eight analysis agents shared the machine — so
every access cost above is stated in **bytes per read**, never seconds.
