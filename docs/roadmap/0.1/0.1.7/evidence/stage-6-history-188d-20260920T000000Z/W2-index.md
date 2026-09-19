# W2 — the persisted cross-save content-signature index

**Diagnostic. Every number below is labelled `measured` / `computed` / `est` / `hypothesis`.**
Source pin: `66bce8378` + the working tree. Nothing here is a gate claim.

## 0. Headline

The product can now do what only the harness could: find a delta base by **content** across save
boundaries and across a close/reopen. Measured on the frozen harness binary, one sample per arm, one
construction worker, quiet machine before and after:

```
  PRODUCT index (harness similarity switch OFF)   45,432,832 B   0.9213x v0.1.6
  HARNESS index (LAYERFS_HISTORY_SIMILARITY=1)    44,175,360 B   0.8958x v0.1.6
  ------------------------------------------------------------------
  the product form is 1,257,472 B behind the harness form (2.85 %)
  and is 2.49 s FASTER and 7.19 MB lower peak RSS

  v0.1.6 (the gate)                               49,315,840 B
  the T1 target                                   47,048,435 B
  the previous default before this round          56,049,664 B   (campaign record, pre-W3 tree)
```

**The honest framing, stated first because it changes the arithmetic:** the 45,432,832 B is the
**tree's** number, not this squad's. W3's concurrent `objects.base_object_id` removal is in the same
binary. The only **isolated, like-for-like** measurement this squad owns is the pair above: same
binary, same tree, one variable — *which index supplies the cross-path candidate*.

---

## 1. What changed

### 1.1 The product form

Before: the content index was a field of `MutationOwner`, constructed in `acquire` and dropped with the
owner at the end of every save (`cas/lifecycle.rs`, `cas/store.rs`). A cross-save match was impossible
at **any** index size, which is exactly why the harness had to model the capability itself.

After:

| element | where | what |
| --- | --- | --- |
| `content_signatures` table | `sql/schema.sql` (W2 section, appended) | one row per ring slot: `slot`, `stamp`, `object_id`, `signature` |
| `SCHEMA_VERSION` 5 -> 6 | `policy.rs`, `sql/schema.sql` | older Stores rejected, not migrated |
| `Candidates::load` | `candidates.rs` | bounded read of the table at `Store::open` / `create` |
| `Candidates::flush` | `candidates.rs` | writes only the entries admitted since the last flush |
| `Candidates::invalidate` / `reload` | `candidates.rs` | failed save drops the slots; next save reads the table back |
| Store-owned `Arc<Mutex<Candidates>>` | `cas/store.rs`, `cas/owner.rs`, `cas/selection.rs` | the index outlives the operation |
| flush in the publishing transaction | `cas/lifecycle.rs` | the index and the objects it names become visible together |

**Why a separate table and not a column on `objects`.** A signature column would put 64 bytes of cold
payload on every row of the hot membership btree, which every `lookup::location` seek reads, and would
pay it for every role rather than for the whole-file lane alone. The separate btree leaves the `objects`
page shape unchanged — **0 B delta on the read path**.

**Why the key is the slot.** A row *is* a ring slot, so `INSERT OR REPLACE` maintains the ring and the
declared bound is enforced by the primary key. **No eviction statement ever runs.** `stamp` is the
one-based insertion number, kept so a reopened Store rebuilds the ring position `(stamp - 1) % SLOTS`,
the insertion cursor and the flush cursor without storing an order anywhere else.

### 1.2 The size bound — the sweep E2 never ran

The census measured exactly two points: ring 1024 -> 18,433 objects and unbounded -> 37,200.
**Nothing in between had ever been swept, and `SLOTS` is a declared bound, so its value had no evidence
behind it.** `w2_index_sweep.py` (this directory) reuses E2's own vectorised signature — self-checked
against a literal transcription of the Rust loop, `--selfcheck PASS` — and E2's own index model, and
sweeps the ring at the product's own reference width. **[M] `w2_index_sweep.json`:**

```
  ring  1024  refs 65536   20,995 objects  (46.2 %)     <- the census's own capacity
  ring  2048  refs 65536   28,661          (63.0 %)
  ring  4096  refs 65536   34,494          (75.9 %)
  ring  8192  refs 65536   36,544          (80.4 %)     <- KNEE
  ring 16384  refs 65536   36,544          (80.4 %)
  ring 32768  refs 65536   36,544          (80.4 %)     <- the previous SLOTS
  ring 65536  refs 65536   36,544          (80.4 %)
  ring unbounded, exact    37,200          (81.8 %)     <- the census's S3 row
```

The census rows reproduce **exactly** (S1 = 20,632 at refs 8192, S3 = 37,200 unbounded/exact), which is
the calibration for the new points. Percentages are of the 45,470 whole-file *occurrences*; of the
44,148 **distinct** objects the census used, 36,544 is 82.8 %.

**The mechanism — and this is the finding, not the observation.** An object is offered to the index only
when the selector found no candidate for it: `select.rs` increments `delta.no_candidate` and **then**
calls `candidates.insert`; an object stored as PREFIX (`delta.prefix_selected`) is **never** inserted.
The index therefore holds the objects that *failed* to get a base, not the objects the Store holds, and
its population is self-limiting: a better index produces fewer `no_candidate` objects, which keeps the
index small. **[M]** on the real lane the ring holds **6,594 of 8,192 slots (80.5 %)** and
`MIN(stamp), MAX(stamp) = (1, 6594)` with 6,594 distinct slots — **the ring never wrapped**, so the
bound was never reached, let alone binding. The census model predicted saturation at 7,614; the measured
6,594 is below it, which is the safe direction (the model does not simulate the prefix-trial losses, the
chain-budget refusals, or the broader candidate rule of 1.4, all of which admit fewer objects).

**`SLOTS = 32,768` therefore held 24,576 slots that buy exactly zero coverage.** [C] at 68 B per slot
that is **1,671,168 B of resident memory for nothing**; at the pre-fold 96 B entry it is 2,359,296 B.

The reference table was swept the same way, because it is the *other* capacity knob:

```
  refs  8192 (13 bits)   29,706      refs 65536 (16 bits)   36,544   <- KNEE
  refs 16384 (14 bits)   34,369      refs 131072 (17 bits)  36,728
  refs 32768 (15 bits)   36,052      refs exact             37,200
```
The last doubling that pays is 15 -> 16 bits (+492 objects for +64 KiB); the next costs +128 KiB for
+184. **65,536 is the knee and is the value the index already carried.**

### 1.3 The 32-bit fold — measured at zero coverage cost

The eight retained hashes are the eight **smallest** of a file's rolling hashes, so their high bits are
mostly zero and only the low half carries entropy. Folding each hash to its low 32 bits:

- **[M]** coverage is **byte-identical** to the 64-bit form on all 44,148 objects (36,544 either way,
  every ring size, sweep B of `w2_index_sweep.json`);
- **[C]** it takes the in-memory entry from 96 B to 68 B and the persisted row by 32 B;
- a collision is a false *proposal*, never a wrong answer: the selector reads the proposed base and
  keeps whichever representation is smaller, so a bad candidate loses the comparison and stores FULL by
  policy. `[C]` the false-positive probability is ~1.5e-8 per candidate pair; over the lane's ~353k
  comparisons the expectation is ~0.005 and the **observed count is 0**.

### 1.4 A rule difference this change exposed — pre-existing, not introduced here

**The harness consults its similarity index only when NOTHING was declared; the product consults its
index when nothing was ACQUIRED.** Confirmed against the current sources:

- harness, `core/benchmark/.../src/ops/history.rs`: `match previous { Some(previous) => vec![previous],
  None => cross_path_predecessors(...) }` — the index is reached only on `previous.is_none()`.
- product, `select.rs:392-404` — `acquisition()` returns `Ok(None)` for an empty advisory **and** for a
  non-empty one where every `probe` refused; `select.rs:295-307` then consults
  `input.candidates.find(...)` on **any** `None`.

So the product finds **more** candidates than the harness arm: declared-but-ineligible falls through
to the index. This is not a regression — the declared base was *refused*, so nothing good is being
replaced, and the evidence already measured that an inferior base beats no base by ~3.5x. It is also
cheaper: **[M]** `delta.ineligible_candidates` is 683 for the product against 2,675 for the harness, and
the product arm is 2.49 s faster. It is **not** changed here: `select.rs` is not this squad's file, and
its own doc comment already documents the current rule.

This surfaced as one test failure, `delta_payload.rs::an_absent_or_ineligible_candidate_selects_full`,
which a concurrent squad adapted while this report was being written. It now passes. **Not this squad's
edit and not re-verified by this squad beyond the suite result.**
---

## 2. LOC delta

Method: `core/crates/**/src/*.rs` + `core/crates/**/sql/*.sql`, non-blank, non-comment lines
(`w2_loc.py`). Before is `HEAD` (`66bce8378`); the working tree also carries every other squad's
uncommitted work, so the tree headline is not this squad's.

| file | before | after | delta | owner |
| --- | --: | --: | --: | --- |
| `layerfs-storage/src/encoding/delta/candidates.rs` | 126 | 263 | **+137** | W2 |
| `layerfs-storage/src/cas/lifecycle.rs` | 175 | 199 | **+24** | W2 |
| `layerfs-storage/src/cas/store.rs` | 409 | 431 | **+22** | W2 |
| `layerfs-storage/src/cas/selection.rs` | 108 | 115 | **+7** | W2 (lock only) |
| `layerfs-storage/src/sqlite/schema.rs` | 253 | 257 | **+4** | shared with W3 |
| `layerfs-storage/src/cas/owner.rs` | 97 | 97 | 0 | W2 (type + comment only) |
| `layerfs-storage/src/policy.rs` | 192 | 192 | 0 | shared with W3 (comment + one integer) |
| `layerfs-storage/sql/schema.sql` | 48 | 44 | **-4** | shared with W3 (net of their deletion) |
| **subtotal, this squad's files** | **1408** | **1598** | **+190** | |

`core/crates` production total: **19,519 (HEAD) -> 20,109 (now), +590** — that delta is the whole
working tree, not this squad. This squad's own contribution is **+190**. The new external test file
`core/crates/layerfs-storage/tests/content_index.rs` (5 cases) is test code and does not count.

### 2.1 Cross-ownership touches (all flagged to the parent)

| file | in the W2 ownership list? | edit |
| --- | --- | --- |
| `src/sqlite/schema.rs` | no — W3 owns it | `REQUIRED_TABLES` `; 4]` -> `; 5]` plus the 5th entry; `content_signatures` added to the unexpected-table `NOT IN` list. **Made before the parent's do-not-edit instruction arrived; the parent confirmed on 2026-09-20 that both stay.** |
| `src/cas/owner.rs` | no | `candidates: Candidates` -> `Arc<Mutex<Candidates>>`. Type only. |
| `src/cas/selection.rs` | no | one lock around the `SelectInput` construction; `candidate_index_bytes` reads through it. |
| `src/policy.rs` | no | `SCHEMA_VERSION` 5 -> 6. |

---

## 3. The measured lane result

**Frozen binary** `sha256 66aaa0946e6b82746b92b95709148b581cdd36092f2ea4a644dbe311975ffff3`,
3,546,000 B, copied out of the shared target directory before either arm so that a concurrent rebuild
could not move the pin. Verified at freeze time and again after: **no product or harness source is newer
than the frozen binary**.

**Timing-rule compliance.** `LAYERFS_CONSTRUCTION_WORKERS=1`; one sample per arm, no best-of; the
machine was checked with `uptime` and `ps aux | grep -E '[c]argo|[r]ustc|[f]s-bench'` immediately
before and after each arm and was **empty both times for both arms**. Cache state declared in 3.3.
The lane lock was held (`mkdir /tmp/lane.lock`) throughout.

### 3.1 The pair

| | PRODUCT index, harness switch OFF | HARNESS index ON | delta |
| --- | --: | --: | --: |
| **apparent bytes** | **45,432,832** | **44,175,360** | **+1,257,472** |
| whole-file lane | 34,761,166 | 33,554,278 | +1,206,888 |
| pooled-metadata lane | 1,904,445 | 1,904,445 | 0 |
| ordinary lane | 875,547 | 875,547 | 0 |
| native lane | 3,676,262 | 3,676,262 | 0 |
| pack bodies | 41,428,320 | 40,221,432 | +1,206,888 |
| `objects` rows | 52,032 | 52,032 | 0 |
| `content_signatures` rows | 6,594 | 6,011 | +583 |
| `content_signatures` on disk (`dbstat`) | 516,096 B / 126 pages | 471,040 B / 115 pages | +45,056 |
| `PRAGMA user_version` | 6 | 6 | 0 |
| `PRAGMA quick_check` | ok | ok | — |

The pooled, ordinary and native lanes are **identical to the byte** between the arms, which is the
control: the index moves the whole-file lane and nothing else.

### 3.2 Measured CPU and memory

| | PRODUCT | HARNESS | delta |
| --- | --: | --: | --: |
| `operation_ns` (the product's own children) | 37.86 s | 40.35 s | **-2.49 s (-6.2 %)** |
| `invocation_ns` (complete command) | 44.53 s | 47.03 s | -2.50 s |
| `cpu_user_ns` | 35.78 s | 38.03 s | -2.25 s |
| `cpu_system_ns` | 8.51 s | 8.77 s | -0.26 s |
| `process_peak_rss_bytes` | 247,857,152 | 255,049,728 | **-7,192,576** |

**The product form is the cheaper one.** It reaches 583 fewer objects but spends 1,992 fewer ineligible
probes (`delta.ineligible_candidates` 683 vs 2,675), and every ineligible probe is a depth-chain walk
before refusal. That is where the 2.49 s is. `[M]`

**CPU cost of the index itself, isolated.** `[C]` the flush walks at most `SLOTS` insertion numbers per
save (16 x 8,192 = 131,072 iterations over the whole lane) and writes at most the entries that save
admitted. The lock is taken once per offered object and never held across a codec call or a query.
The measured whole-lane CPU is *lower* than the harness arm, so the index's own CPU is below the noise
of the arm it replaces. A tighter attribution needs a no-index control arm, which the product cannot
express (5.2).

**Budget.** `budget.complete-command` is <= 15 s; measured **44.53 s**. This is a **FAIL** and it was
already one (the T1 squad recorded 40.0 s and reported `NOT_RUN`). It is **not** made to fit by
shrinking the workload.

### 3.3 Declared cache state — not a cold claim

- `store_state = CreatedInSample`, `cache_state = CreatedInSample` for both arms (each arm creates its
  own Store; `space.py` reports `sidecars: []`, `incomplete: []`).
- The **corpus is partially resident** from earlier runs in this session. Both arms read the same
  corpus, so the *comparison* is fair, but the absolute numbers are **warm-corpus** and must not be
  quoted as cold.
- `construction_workers = 1` on both arms.
- One sample per arm; no arm was re-run for a better number.

### 3.4 What the 45,432,832 B does and does not include

**It includes W3's concurrent work.** The binary carries W3's R2/T1 ruling D (`objects.base_object_id`
removed, their own measured **1,335,296 B** on this lane) and whatever else landed before 03:41:08.
This squad cannot separate those without a control binary, and did not have one.

Against the campaign's recorded arms, with the residual stated:

```
  the previous default (campaign record, pre-W3 tree)   56,049,664
  LESS W3's ruling D, their measurement                -1,335,296
  = a derived previous-default on this tree             54,714,368   [C]
  PRODUCT index, measured                               45,432,832   [M]
  ------------------------------------------------------------------
  raw difference                                        -6,725,632   [C]
  attributable to the persisted index                   -5,390,336   [C] = 84.5 % of 6,377,472
  residual (interaction between the two changes)          -1,335,296   [C] NOT MEASURED
```

The residual is not small and it is not explained here. **The like-for-like pair in 3.1 is the only
number this squad stands behind.**

### 3.5 The target

| | bytes | |
| --- | --: | --- |
| the harness arm's target | 49,672,192 | measured pre-W3, **not like-for-like** |
| PRODUCT, measured | **45,432,832** | 4,239,360 B under the target |
| HARNESS index, same binary and tree | 44,175,360 | the honest same-tree comparator |

**The target is met, and the honest comparison is the third row.** The product reproduces **98.2 %** of
what the harness's index buys on the identical tree; the 1.8 % it does not is priced in 4.
---

## 4. The honest arithmetic

### 4.1 The 6,377,472 B is NOT an addition

It is the **product form of a capability the harness already supplies**, and it must be booked that way.

| | bytes | status |
| --- | --: | --- |
| capability the harness models (campaign record) | 6,377,472 | **relocated**, not newly bought |
| what the product now reaches, same tree, one variable | 1,257,472 short of the harness | **[M]** |
| what is **newly bought** | nothing on disk | the capability now exists **in the product**, across a close and reopen, with no harness switch |

**Newly bought is a capability, not a byte count.** Before this change the product could not reach a
cross-save predecessor at any index size; the harness had to supply one, and the arm that did so was
switched OFF by owner direction precisely because the product could not do it. The arm is now
reproducible **with the harness switch OFF**. That is the deliverable.

### 4.2 Bytes spent vs bytes saved

```
  bytes SPENT (measured, dbstat, PRODUCT arm)           516,096   [M]  126 pages of 4,096 B
    = 6,594 rows x 78.1 B average
  declared in-memory bound (measured by the test)       688,128   [M]  asserted in content_index.rs
  declared index bound INDEX_BYTES                      720,896   [C]  704 KiB, const-asserted

  bytes SAVED by the product index (derived, 3.4)     5,390,336   [C]
  ------------------------------------------------------------------
  net                                                 4,874,240   [C]
```

Per-entry arithmetic, so it closes:

```
  slot array   8,192 x 68 B  =   557,056 B      (32 B identity + 32 B folded signature + 4 B Option)
  references  65,536 x  2 B  =   131,072 B      (u16, NO_ENTRY = 65,535 > SLOTS = 8,192)
  handle                      =        56 B
  ------------------------------------------------------------------
  measured live_bytes         =   688,128 B     [M] asserted by the external test
  declared INDEX_BYTES        =   720,896 B     [C] = 704 KiB, above the structure

  at the previous SLOTS = 32,768:  32,768 x 68 + 131,072 = 2,359,296 B  [C] (+1,671,168 B for 0 coverage)
  at the pre-fold 96 B entry:      32,768 x 100 + 131,072 = 3,407,872 B [C]
```

### 4.3 Access cost in bytes per read

| path | bytes | status |
| --- | --: | --- |
| `Store::open` — one bounded load | **<= 516,096 B per open** (6,594 rows; the query reads 3 of the 4 columns, so ~488 KB) | `[M]` table size, `[C]` read width |
| per save — the flush | only the entries that save admitted; **516,096 B over the whole 16-save lane** | `[C]` from the measured row count |
| **the read path** (`read_batch`, `contains`) | **0 B** | `[M]` the table is never queried by a read |
| an index-proposed candidate that is refused | 1 `objects` btree seek + a depth-chain walk; 683 of them, against 2,675 for the harness | `[M]` `delta.ineligible_candidates` |

The declared bound on the open read is `SLOTS` rows: **at most 8,192 rows, and a table that holds more
is refused with `Integrity("content index over slot bound")` rather than truncated.**

---

## 5. Negative results and things NOT run

### 5.1 Retained negative results

1. **Make the cache persist is not the fix — confirmed, and now superseded.** The census's S1 (persist
   the old 1024-slot ring) buys +2,199 objects. Persisting the *product's current* ring buys coverage
   only up to 8,192 slots; above that the ring is not the binding constraint at all.
2. **`SLOTS = 32,768` (R1a, T1 #188) is 4x larger than anything the lane can use.** 24,576 slots,
   1,671,168 B of resident memory, zero coverage. The sweep that would have caught it had never been run.
3. **Rebuild-at-open is rejected.** The shipped path loads the table (<= 516 KB). The alternative —
   reconstruct signatures by reading and decompressing every stored whole-file object — would cost the
   whole store's pack bytes at every open. It is **NOT MEASURED** and is not implemented, because a
   dead alternate path in product source is forbidden by `core/AGENTS.md`; it is ruled out
   architecturally, not by a number.
4. **`UNIQUE (object_id)` is rejected.** ~40 B/row for a second btree, and it cannot fire: an object
   reaches the index only when the Store did not already hold it.
5. **`objects.base_object_id` as a signature carrier is rejected.** It would raise the bytes per read of
   every location lookup.
6. **A periodic test payload gives a false negative.** `patterned()` has only 256 distinct rolling
   windows, so a fresh kilobyte rewrites which eight are smallest and two near-identical files share
   **1** hash, not 8. The first version of the persistence test asserted the wrong thing because of it.
   Recorded because it is a trap for anyone else writing a similarity fixture.
7. **The reference table is NOT worth widening past 65,536.** +184 objects for +128 KiB, then +62 for
   +256 KiB.

### 5.2 Checks NOT run, with the reason

| check | state | reason |
| --- | --- | --- |
| full `cargo test` on the final tree | **NOT RUN clean** | the tree was being edited by W3 throughout. Last full run: **483 passed, 3 failed**; 2 failures (`group_decodes.rs`, both) are W3's read-path change, and 1 (`delta_payload.rs`) was W3's in-flight adaptation and now passes in isolation. `content_index.rs` **5/5 PASS** on the final tree. |
| `--lane full` (220) / `--lane smoke` (20) | **NOT RUN** | untouched by this change by construction; not re-run this round. |
| `history-stride1` | **NOT RUN** | forbidden. |
| cold-cache lane arm | **NOT RUN** | no cold contract applies to this lane; cache state declared in 3.3. |
| verify phase | **NOT RUN** | the lane's verify is a declared sample; 3.1 is a perf arm only. |
| rebuild-at-open cost | **NOT MEASURED** | 5.1 item 3. |
| no-index control arm | **NOT MEASURED** | the product has no switch to disable its own index, and adding one is a test-only product hook, which `core/AGENTS.md` forbids. |
| clippy on the final tree | **RUN** | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets` — no error or warning from this squad's files. |
| `cargo fmt --check` | **RUN, clean** | after formatting this squad's two files. |
| `core/tools/check_product_boundary.py` | **RUN, PASS** | 121 production Rust/SQL files. |
| `core/tools` self-tests | **RUN, OK** | 6 tests. |

### 5.3 Measurement-integrity incidents, recorded

1. **The shared harness binary was rebuilt under this squad mid-campaign** (mtime 03:41:08, between the
   first and second arm). The first pair of arms is therefore **confounded and discarded**; both
   receipts stay on disk (`LANE/w2-product`, `LANE/w2-harness-index`). The reported pair was taken from
   a frozen copy (3).
2. **A concurrent squad ran a lane while this squad held `/tmp/lane.lock`** (`/tmp/w1/armB`, observed in
   `ps` at 03:41). The affected receipts are the discarded ones. The two reported arms were checked
   quiet before and after and are clean.
3. **W3 edited `sql/schema.sql` and `sqlite/schema.rs` while this squad was editing them.** No content
   was lost: the W2 table was **appended** with `cat >>`, never written by a whole-file replace, and the
   version was moved to 6 rather than colliding with W3's 5. Reported to the parent in two messages.

---

## 6. Verification of the capability itself

`core/crates/layerfs-storage/tests/content_index.rs` — **5 cases, all PASS** on the final tree:

| case | what it pins |
| --- | --- |
| `an_admitted_full_enters_the_index_and_is_written_in_the_saving_transaction` | admission, the row, and `content_index_bytes() == 688,128` |
| `the_index_outlives_the_handle_that_filled_it` | save, drop the handle, `Store::open`, then a **new** object gets a PREFIX base from the reopened index |
| `the_index_is_cross_save_within_one_handle_as_well` | the same across two saves on one handle |
| `an_abandoned_save_leaves_no_entry_behind` | `abort()` leaves 0 slots and 0 rows; the next save stores FULL by policy |
| `a_stored_candidate_still_reads_back_through_the_reopened_index` | the index-proposed base is a real, readable dependency edge after reopen |

The decisive one is the second: it is the capability the harness had and the product did not.

---

## 7. Reproduction

```sh
# the sweep (model, no lane lock needed)
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-188d-20260920T000000Z/w2_index_sweep.py

# the lane (take the lock first; one construction worker; one sample per arm)
mkdir /tmp/lane.lock
cargo +1.85.1 build --release --locked \
  --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml
# freeze the binary BEFORE either arm, or a concurrent rebuild moves the pin
cp core/benchmark/fs-bench-pro-storage-content/target/release/fs-bench-storage-content /tmp/frozen-bin
LAYERFS_CONSTRUCTION_WORKERS=1 /tmp/frozen-bin --case history-stride10 \
  --out <out-product> --corpus /Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data
LAYERFS_CONSTRUCTION_WORKERS=1 LAYERFS_HISTORY_SIMILARITY_CANDIDATES=1 /tmp/frozen-bin \
  --case history-stride10 --out <out-harness> --corpus /Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data
rmdir /tmp/lane.lock

# the numbers
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-188d-20260920T000000Z/w2_final.py \
  <out-product> <out-harness>
```

## 8. Artifacts in this directory

| file | what |
| --- | --- |
| `w2_index_sweep.py` / `.json` | the ring x reference x fold sweep, E2's instrument |
| `w2_refbits.json` | the reference-table width sweep at the ring knee |
| `w2_loc.py` | the production-LOC counter |
| `w2_final.py` | the arm extractor (footprint + counters + table size) |
| `w2_arms.py` | the same for the discarded confounded pair |
| `LANE/frozen-bin` | the frozen harness binary, `sha256 66aaa094...ffff3` |
| `LANE/w2f-product/`, `LANE/w2f-harness/` | **the reported receipts** |
| `LANE/w2-product/`, `LANE/w2-harness-index/`, `LANE/w2-product-clean/` | discarded attempts, kept on disk |
