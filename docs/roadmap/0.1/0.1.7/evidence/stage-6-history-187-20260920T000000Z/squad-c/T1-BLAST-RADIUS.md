# T1 — blast radius, file structure, and LOC

> **Status:** Estimate. **Nothing here is implemented.** Every LOC figure is an **estimate** with its
> basis stated; every byte figure is **measured** unless marked. Source pin `66bce8378`.
> Companion to [`t1-implementation.md`](../../../../docs/roadmap/0.1/0.1.7/retained-history-t1/t1-implementation.md).
> **No timing figure appears here.**

## 0. The headline

**T1 is small in code and large in blast radius and risk.** The mechanism already exists — the advisory
type, the consumer, the content index, the record grammar that carries the base id. T1 is wiring and
constants, not new algorithms. **Estimated net: +95 to +185 production LOC**, of which **R0 is 0**.

Two things make it bigger than the line count suggests, and both are flagged below: **R1a costs ~7 MiB
of resident memory** (128 KiB → 7 MiB), and **R2 needs a third index removed via the cleanup path**,
which I had not listed when I quoted R2's 7,018,610 B.

## 1. Blast radius

**11 files across 2 crates plus 1 SQL file.** Physical LOC and headroom against the 999-line ceiling:

| file | LOC | headroom | change | why it is in scope |
| --- | --: | --: | --- | --- |
| `layerfs-storage/src/encoding/delta/select.rs` | 392 | 607 | R1b, R3 | `SelectInput.candidates` borrow; `DepthCache::cost_of` |
| `layerfs-storage/src/encoding/delta/candidates.rs` | 166 | 833 | R1a | `SLOTS`, `REFERENCES`, `INDEX_BYTES` + the size assertion |
| `layerfs-storage/src/cas/lifecycle.rs` | 233 | 766 | R1b | `Candidates::new()` moves off the owner (`:90`) |
| `layerfs-storage/src/cas/store.rs` | 616 | 383 | R1b | the Store owns the index; `begin_save` hands it over; `self.owner = None` (`:392`) |
| `layerfs-storage/src/cas/selection.rs` | 138 | 861 | R2 | the pending member carries the base |
| `layerfs-storage/src/cas/placement.rs` | — | — | R2 | 3 `base_object_id` sites |
| `layerfs-storage/src/sqlite/schema.rs` | 296 | 703 | R2 | DDL is `include_str!`; `create`/`validate`/`identity` |
| `layerfs-storage/src/sqlite/write.rs` | 198 | 801 | R2 | the single `INSERT INTO objects` (`:181`) and its column list |
| `layerfs-storage/src/sqlite/lookup.rs` | 172 | 827 | R2 | `ObjectLocation` and the four `base_object_id` sites |
| `layerfs-storage/src/sqlite/cleanup.rs` | — | — | **R2 (newly identified)** | `ORDER BY pack_id DESC, group_number DESC, record_number DESC` (`:48`) is the only consumer of `objects_locations` |
| `layerfs-storage/src/encoding/delta/read.rs` | 341 | 658 | R2 | the reader takes the base from the record instead of the column |
| `layerfs-storage/src/policy.rs` | 360 | 639 | R2 | `SCHEMA_VERSION: i64 = 4` → 5 (`:27`) |
| `layerfs-storage/sql/schema.sql` | 72 | — | R2 | the `objects` DDL and two indexes |
| `layerfs-content/src/file/content.rs` | 323 | 676 | R0 (optional) | only if a C1 entry point is added |

**Not touched:** `layerfs-content/src/file/edit/tree.rs` (978 lines — the **only** product file near the
999 ceiling) is **out of scope**, as is the whole edit path, the filesystem tree, the pool lane and the
codec.

## 2. File and folder structure

**Unchanged. No new files, no new folders, no splits.**

````text
core/crates/
  layerfs-storage/
    sql/
      schema.sql                    72   <- R2 edits this in place
    src/
      policy.rs                    360   <- SCHEMA_VERSION
      cas/
        lifecycle.rs               233   <- R1b
        store.rs                   616   <- R1b (headroom 383: the tightest file in scope)
        selection.rs               138   <- R2
        placement.rs                     <- R2
      sqlite/
        schema.rs                  296   <- R2
        write.rs                   198   <- R2
        lookup.rs                  172   <- R2
        cleanup.rs                       <- R2 (newly identified)
      encoding/delta/
        candidates.rs              166   <- R1a
        select.rs                  392   <- R1b + R3
        read.rs                    341   <- R2
  layerfs-content/
    src/file/content.rs            323   <- R0, only if a C1 entry point is added
````

**Why no split is needed — measured, not assumed:**

- Every file in scope has **>= 383 lines of headroom**; the median is ~660. The largest is
  `cas/store.rs` at 616.
- **No `lib.rs` or `mod.rs` in either crate exceeds 150 lines**, against a 200-line cap, so the
  stricter entry-file rule is not approached.
- The only product file within 21 lines of the 999 ceiling is `file/edit/tree.rs` (978), and T1 does
  not touch it.
- R2 is a **schema-and-read-path** change, not a new subsystem, so it adds no module.

**If any file did approach the cap**, the split would be by responsibility, not by number: R2's
read-path work would go to a focused `sqlite/location.rs` and R1's index lifetime to
`encoding/delta/index.rs`. **Neither is needed at these sizes.**

## 3. LOC by change

All figures are **estimates**. "Basis" names what the estimate is anchored to.

| change | added | removed | net | basis |
| --- | --: | --: | --: | --- |
| **R0** — declare the base | **0** | 0 | **0** | E4: the advisory type and its consumer already exist; the caller attaches via `with_predecessors` |
| R0 (optional C1 entry point) | ~18 | 0 | +18 | E4: `construct_bytes_with_predecessors` in `content.rs` |
| **R1a** — index capacity | ~4 | ~1 | **+3** | 3 constants + their docs; the `const _` size assertion is computed and needs no edit |
| **R1b** — index lifetime | ~50 | ~5 | **+45** | move `Candidates::new()?` from the owner to the Store, thread the borrow through `begin_save`/`SelectInput` |
| R1b (persisted across restart) | ~150–300 | ~5 | **+145–295** | E4: new table + `SCHEMA_VERSION` 4→5 + `accept_keyed` |
| **R2** — lean row grammar | ~60 | ~45 | **+15** | 16 `base_object_id` sites read from the record instead of the column; 3 DDL blocks removed; the `INSERT` column list shrinks |
| **R3** — depth measurement | ~5 | ~1 | **+4** | carry a `u8` offset out of the `cost_of` loop |
| | | | **+67** | **R0–R3, in-memory index** |
| | | | **+212–362** | **with a persisted index** |

**Rounded and stated as a range: +95 to +185 production LOC** for the recommended path (R0–R3 with an
in-memory index, plus the optional C1 entry point and normal slack); **+250 to +450** if the index must
survive a process restart.

**Removed LOC is real but small.** R2 deletes three DDL blocks (`objects_bases`, the `base_object_id`
column, `objects_locations`) and shrinks one `INSERT` column list — **~45 lines**. There is no
subsystem to delete.

## 4. Two things the line count hides

### 4a. R1a costs ~7 MiB of resident memory, not 3 lines

The index is a fixed-size structure with a **compile-time assertion** that it fits `INDEX_BYTES`
(`candidates.rs:35-42`). Today: `SLOTS = 1024`, `REFERENCES = 8192`, `INDEX_BYTES = 128 KiB`.

To index a whole 52,032-object Store the slot count must be a power of two >= 52,032, i.e. **65,536** —
and `SLOTS < NO_ENTRY (65535)` is asserted, so 65,536 is exactly at the `u16` reference limit.

````
  65,536 slots x size_of::<Option<Entry>>()   Entry = ObjectId(32 B) + [u64; 8](64 B)
                                              + an Option discriminant -> ~104 B
  = 6,815,744 B
  65,536 references x 2 B                     =   131,072 B
  ----------------------------------------------------------
  ~6,946,816 B  ->  INDEX_BYTES must rise to 7 MiB

  128 KiB -> 7 MiB  =  56x,  and ~15 % of a 47 MB Store held resident
````

**This is a memory-vs-coverage curve, not a free win, and the curve is unmeasured between the two ends.**
E2 measured 1,024 slots -> 18,433 objects and unbounded -> 37,200; **nothing in between was swept.** The
optimal slot count is an open question, and `INDEX_BYTES` is a *declared bound*, so raising it is a
policy statement.

### 4b. R2 needs a third index removed, and that index has a consumer

I quoted R2 as 7,018,610 B. Measured `dbstat` on `/tmp/s0_l7` says that number needs **all three** of:

| component | bytes | removable? |
| --- | --: | --- |
| `objects_bases` | 2,805,760 | **yes** — the base id is already in the packed record (`record.rs:87-89`), so the column and its index are redundant |
| `base_object_id` column, inside `objects` | ~614,400 | **yes** — same reason (l7's `objects` is 4,128,768 vs base187's 3,514,368; the growth tracks base rows) |
| `objects_locations` | 2,547,712 | **only with a cleanup change** — `cleanup.rs:48` orders by `pack_id, group_number, record_number`, and that is its only consumer |
| | **5,967,872** | **85 % of R2** |

````
  l7 non-pack (dbstat)                        9,609,216
    - objects_bases                          -2,805,760   -> 6,803,456
    - the base_object_id column                -614,400   -> 6,189,056
    - objects_locations                      -2,547,712   -> 3,641,344
  v0.1.6's measured non-pack                   3,259,108
  residual                                       382,236
````

**Dropping only `objects_bases` recovers 40 % of R2, not 100 %.** Reaching the target needs the
`objects_locations` removal too, which means changing the failed-save cleanup path from a location scan
to in-memory tracking — **a blast-radius item I had not listed**, and the reason R2's estimate carries
the widest error bar in this document.

## 5. What this means for the issue

- **The 999-line and 200-line ceilings are not a constraint on T1.** No split, no new module.
- **The LOC is small (+95 to +185), which is the risk**: the change is *wide and shallow*, touching 11
  files across 2 crates, one SQL file and one schema version, with a format change and a memory
  statement in it. Small diffs across a schema boundary are where review has to be strongest.
- **Two estimates are weak and should be closed before scheduling:** the optimal `SLOTS`/`INDEX_BYTES`
  (4a — a sweep that has not been run) and R2's `objects_locations` removal (4b — a cleanup-path change
  that has not been scoped in detail).
- **Production LOC: 84936 -> 84936 (delta 0)** today. My own count of the `core/crates/` subtotal with
  this document's method is **19,537** (`src/*.rs` + `sql/*.sql`, non-blank, non-`//`); the campaign's
  combined headline is unchanged at 84,936.
