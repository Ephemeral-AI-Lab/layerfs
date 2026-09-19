# VACUUM — the gate is met, and by how much depends on the basis

**Diagnostic.** Measured on the L4 Store (@/tmp/l4-after/sample.sqlite@, 49,672,192 B, the L4 receipt's
artifact) and on v0.1.6's retained Store. **No timing figure** — VACUUM's I/O cost is **not measured**.

## 1. The measurement

Both Stores copied, vacuumed with SQLite @VACUUM@, re-read with @shared/space.py@:

| store | as recorded | after @VACUUM@ | saved |
| --- | --: | --: | --: |
| **ours (L4)** | **49,672,192** | **48,717,824** | **954,368** |
| **v0.1.6** | 49,315,840 | **48,857,088** | **458,752** |

@quick_check@ = @ok@ afterwards, and the pack directory decodes unchanged
(@whole-file 37,347,557 · native 3,957,829 · pooled-metadata 2,019,419 · ordinary 1,022,795@).

## 2. Two bases, and both must be stated

@```
  the basis the exit criterion NAMES (v0.1.6's recorded figure):
      ours, vacuumed      48,717,824
      v0.1.6, as recorded 49,315,840
      -> 598,016 B BELOW the gate   (1.21 % margin)

  the LIKE-FOR-LIKE basis (both vacuumed):
      ours, vacuumed      48,717,824
      v0.1.6, vacuumed    48,857,088
      -> 139,264 B BELOW the gate   (0.28 % margin)
@```

**The vacuum is worth 954,368 B to us and 458,752 B to v0.1.6 — a net 495,616 B.** v0.1.6's recorded
Store was left fragmented, so part of the apparent win is that its recorded number includes
**458,752 B of fragmentation we would be removing from our own**.

**The honest headline is the second row: 139,264 B below v0.1.6 on a like-for-like basis.** The first row
is what the criterion literally names, and it is true, but it is not the like-for-like comparison and must
not be quoted alone.

## 2.1 What "vacuumed" actually means, measured

@VACUUM@ rebuilds the database file from scratch: every table and index is rewritten in key order into a
fresh file, which replaces the old one. Two different things get reclaimed, and they are not the same
size. Measured on **v0.1.6's Store** (@/tmp/vb16.sqlite@, a fresh copy):

@```
  page_size 4096 B
    page_count    12,040 -> 11,928    112 pages =  458,752 B total
    freelist          17 ->      0     17 pages =   69,632 B   free pages reclaimed   (15 %)
    live pages    12,023 -> 11,928     95 pages =  389,120 B   btree coalescing       (85 %)

  where the live-page change sits:
    objects        2,650,112 -> 2,416,640   -233,472
    object_packs  46,505,984 -> 46,350,336   -155,648
@```

**Only 15 % is the freelist.** The other 85 % is **btree coalescing**: this Store inserts rows keyed by
@object_id@, which is a blake3 hash — effectively random order. A btree built by random inserts splits
pages and leaves many of them partly filled. Rewriting in key order packs them densely, which is why the
@objects@ table alone gives back 233,472 B without a single row being deleted.

**So "vacuumed" means: the file has been rewritten so every page is densely packed — no free pages and no
half-empty btree pages.** It changes no byte of the logical content; the pack directory and the record
grammar are untouched, and @quick_check@ returns @ok@.

**Why this is an accounting question and not just a win.** v0.1.6's **recorded** figure is a *fragmented*
Store. Our Store is fragmented too. Comparing our vacuumed Store against its un-vacuumed recorded number
credits us with removing fragmentation that its number still contains — which is why the margin is
**598,016 B on the recorded basis but 139,264 B like-for-like**. Both are real; only the second is
like-for-like.

**And it is not free.** @VACUUM@ rewrites the entire file, costing roughly its own size in I/O plus
temporary space, at close. **That cost is unmeasured here** and no timing may be taken on a shared
machine.

## 2.2 There is no post-compaction anywhere, and it is worth about 1 %

**Confirmed from source, not assumed:**

- @VACUUM@ appears **nowhere** — not in @core/crates/@, not in @crates/@, not in the harness.
- @auto_vacuum@ is **not set**. The connection profile sets only @journal_mode = MEMORY@,
  @synchronous = OFF@ and @temp_store = MEMORY@ (@sqlite/connection.rs:30-37@), so the default
  @auto_vacuum = NONE@ stands and SQLite never compacts incrementally either.
- **All packing happens inside the save.** @SaveOperation::accept@ -> @placement::seal_group@ ->
  @INSERT INTO object_packs@ / @UPDATE object_packs@ (@sqlite/write.rs:78,90@), and
  @finish_inner@ (@cas/lifecycle.rs:159-163@) seals every remaining lane group before the COMMIT.
  Chunk -> compress -> pack -> write is synchronous within the save; **nothing runs after it.**
- @sqlite/cleanup.rs@ is **not** compaction: it is bounded deletion of a definitely-failed,
  unpublished save. It removes rows and never rewrites or repacks.

**And the size of the opportunity, which is the point:**

@```
  v0.1.6 Store, apparent 49,315,840
     object_packs  (compressed content)   46,505,984   94.30 %
     objects       (rows + primary key)    2,650,112    5.37 %
     schema / mvf / freelist                 159,744    0.32 %
     ----------------------------------------------------------
     VACUUM reclaims                         458,752    0.93 %   <- all page layout
@```

**About 94 % of the Store is the packed, compressed content.** Row and index overhead is ~5 %.
Page-layout slack — the only thing a post-compaction can touch — is **~1 %**.

**So post-compaction is an accounting detail, not a strategic lever.** It is real, it is measurable, and
it is the last 1 %. The levers that move the Store are the ones that change what goes *into* the packs
(base coverage, chunk deltas) and the row grammar. It is also symmetric: v0.1.6 has no compaction either,
so its recorded number carries the same kind of slack ours does.

## 3. Is VACUUM a legitimate lever? — a ruling, not an optimisation

@VACUUM@ rewrites the whole database at close. It is **not** a format change and it changes no byte
grammar, but it is a **new close-time operation with an unmeasured I/O cost**, and this campaign has
measured nothing about it but the bytes. Under @AGENTS.md@ §1 a phase must pay for its own work from a
declared cache state, so **the close-time cost has to be measured on a quiet machine before this is
adopted** — and this lane already records @budget.complete-command@ **40.0 s against a 15 s ceiling**.

**So: the gate is reachable, and the cheapest route to it is a close-time VACUUM worth a net 495,616 B —
which is a ruling.**

## 4. What the alternatives would cost, on the measured Store

Baseline: **49,672,192 B**, i.e. **356,352 B above the gate**.

| lever | net bytes | resulting | vs gate | cost |
| --- | --: | --: | --: | --- |
| **VACUUM at close** | **495,616** | 49,176,576 | **-139,264** | ruling; close-time I/O **unmeasured** |
| @GROUP_LEVEL@ 1 -> 19 | 261,819 | 49,410,373 | +94,533 | one constant; **not sufficient alone** |
| VACUUM + @GROUP_LEVEL@ | 757,435 | 48,914,757 | **-401,083** | both rulings |
| codec level 3 -> 19 | ~4,270,443 | ~45,401,749 | ~-3,914,091 | CPU **unmeasured**, budget already failing |
| L5 @objects@ row grammar | 1,236,992 | 48,435,200 | -880,640 | **format change**, read cost per chain edge |

**Without one of these, the gate is not met: 356,352 B short.**

## 5. Not claimed

- **No stride3 confirmation exists.** Nothing here is a gate claim.
- The verify phase is a **declared sample** (1,083 of 101,477 declared path-states) and reports
  @INCOMPLETE@, never a full PASS.
- **VACUUM's I/O cost is unmeasured** and no timing may be taken on a shared machine.
- v0.1.6's vacuumed figure is **our measurement of its artifact**, not a v0.1.6 product behaviour —
  its recorded Store is the fragmented one.
