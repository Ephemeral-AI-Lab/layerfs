# #219 round 17 — the algorithm-gap exploration: the premise holds, H2 is settled by reading, H1 is priced

Status: **Exploration.** Commissioned by
[`issue219-ns19-algorithm-gap-handoff.md`](../../issue219-ns19-algorithm-gap-handoff.md). **No product
line changed, no arm registered, no gate claimed, no performance claim made.** One labelled
diagnostic ran, once, and its whole stdout is filed at `raw/locator_btree_shape.txt`. Everything else
in this report is read from receipts and source that already exist.

Pre-registration, written before the diagnostic ran:
[`pre-registration.md`](pre-registration.md).

**Owner ruling carried into this round: the database page size stays 4 KiB.** It was read and
asserted (4096 on all three arms), never set, and nothing here proposes changing it.

---

## Step 1 — the commission's §1a is re-derived, and it holds

**The boundary used.** One boundary, applied to both sides, is: *fixture read or byte generation +
object construction + Store open + the C1 tree build + admission*. On this row that is
`operation_work_ns` (the product's own work counters, which exclude construction under C2's
supplied-object rule and include the C1 build) **plus** the two charges the row publishes beside it
and inside neither, `pipeline.construct_ns` and `pipeline.construct_noise_ns`. On the reference's
side it is `layerstack_init_ns`, whose own timer already contains reading its fixture and building
the objects it admits, with `initialization_user_cpu_ns + initialization_system_cpu_ns` for CPU.

**This row**, from `benchmark-results/issue219/ns19-Q2-repin-20260921T115200Z/pipeline-namespace-10000/receipt.json`
— PASS, 13/13 gates, 14/14 pinned counters, 95 commits, 25,245 locator rows, 302,231,057 canonical
bytes:

| quantity | ns | source |
| --- | ---: | --- |
| `pipeline.operation_work_ns` | 1,218,880,166 | `counters` |
| `+ pipeline.construct_ns` | 449,315,829 | `counters` |
| `+ pipeline.construct_noise_ns` | 113,830,615 | `counters` |
| **boundary-matched work** | **1,782,026,610** | **1782.027 ms** |
| `phases.cpu_user_ns` + `phases.cpu_system_ns` | 694,879,000 + 533,453,000 = 1,228,332,000 | `phases` |
| **boundary-matched CPU** | **1,791,478,444** | **1791.478 ms** |

**The six reference rows.** All six are `family: init_namespace`, `case: namespace-10000`,
`timer: layerstack_init_ns`, `status: PASS`, `setup_policy: fresh-output`, 73
`initialize_admission_transactions` each. Their raw `perf.jsonl` files are **not present in this
worktree**; the readings below are taken from
`benchmark-results/issue219/20260921-s0/inventory-raw.json`, which embeds each row's whole `sample`,
`header` and `summary` object and was produced by `inventory.py` reading those raw files.

| # | `layerstack_init_ns` | user_ns | sys_ns | CPU ns | implied cores | objects | canonical B | Store file B | file/canonical | disk read B |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 402,720,750 | 657,427,250 | 1,125,838,334 | 1,783,265,584 | 4.43 | 54,465 | 305,039,320 | 362,872,832 | 1.1896 | 16,384 |
| 2 | 407,598,375 | 666,539,375 | 982,668,166 | 1,649,207,541 | 4.05 | 54,458 | 305,038,564 | 363,331,584 | 1.1911 | 16,384 |
| 3 | 578,245,125 | 670,660,750 | 1,020,098,125 | 1,690,758,875 | 2.92 | 54,463 | 305,039,104 | 363,724,800 | 1.1924 | 0 |
| **4** | **928,022,417** | 1,055,100,042 | 734,412,250 | **1,789,512,292** | 1.93 | **25,158** | 302,182,831 | 304,955,392 | **1.0092** | **0** |
| 5 | 1,020,422,292 | 1,090,911,084 | 1,025,751,791 | 2,116,662,875 | 2.07 | 25,158 | 302,182,831 | 304,627,712 | 1.0081 | 337,367,040 |
| 6 | 1,100,711,333 | 1,056,112,042 | 1,139,218,959 | 2,195,331,001 | 1.99 | 25,158 | 302,182,831 | 304,705,536 | 1.0083 | 322,199,552 |

Paths, in the same order: `benchmark-results/worktree-archives/issue49-live-integration-ba7f7fbe1/host-store/campaigns/issue38-main-refresh-1788696142088555000-corrected/performance/init_namespace/namespace-10000/perf.jsonl`;
`benchmark-results/nine-family-fast-baseline/72f408b467840389/performance/init_namespace/namespace-10000/perf.jsonl`;
`benchmark-results/worktree-archives/issue49-live-integration-ba7f7fbe1/host-store/campaigns/issue38-main-refresh-1788696142088555000/performance/init_namespace/namespace-10000/perf.jsonl`;
`benchmark-results/host-store/issue118/20260912/namespace-qualification/candidate-small10000-corrected-input/perf.jsonl`;
`benchmark-results/host-store/issue120/performance/init_namespace/namespace-10000/perf.jsonl`;
`benchmark-results/issue152/g1/namespace-10000-r4/perf.jsonl`.

**Verdict: the commission's §1a is confirmed, to the printed precision.** Rows 4–6 are the
same-shape rows (25,158 locators against this row's 25,245) and their CPU is **1,789.5 / 2,116.7 /
2,195.3 ms**, exactly as L69 and the commission state. Against the fastest of them the row is
**−0.42 % on work** (1782.027 vs 1789.512) and **+0.11 % on CPU** (1791.478 vs 1789.512). The
handoff's 1782.0 / 1791.5 are reproduced from `receipt.json` to four significant figures. The
premise of the v0.1.6 question — *v0.1.7 is behind* — **does not survive the matched boundary.**

**One refinement the commission's table does not carry, and it matters.** The three same-shape rows
are not one condition. Row 4 read **0** bytes from storage inside its window; rows 5 and 6 read
**337.4 MB** and **322.2 MB**. So the 1,789.5 → 2,195.3 ms band is *not* an algorithm band: it spans
cache state, and only **row 4** is like-for-like with this row, which generates its 302 MB in process
and charges that generation as `construct_noise_ns` (113.8 ms) rather than paying a storage read. The
row is level with row 4 and *ahead* of rows 5–6 on the work that excludes a storage read. No claim is
made about rows 5–6 beyond that: their receipts carry no separate read timer, so the read cannot be
subtracted, and `initialization_disk_read_bytes` is not a phase reading — it is the process's own
`/proc/self/io` `read_bytes:` differenced across the initialization window
(`benchmark/fs-bench-pro/src/main.rs:390-393` and `:458`, consumed as a `resource_delta` at `:2103`),
so it bounds the read without pricing it. **The three
rows' spread is `NOT_MEASURED` as an algorithm quantity.** All six are `cache_contract: null` and
`verification_status: NOT_RUN`, so no pairing is claimed with this row either.

---

## Step 2 — H2 is settled by reading: the reference's payload reaches the pager

The commission's fork was: *if the reference's payload bytes go to a spill file rather than into
SQLite, then its commit is cheap by construction and the 458.3 ms is the price of a design it never
paid.* **It does not. The reference pays for its own storage.**

**The insert site.** `crates/layerfs-layerstack-store/src/objects/admission.rs:1547`:

```rust
let sql = format!(
    "INSERT INTO object_packs(pack_id,data) VALUES {}",
    vec!["(?,?)"; page.len()].join(",")
);
```

and the bytes bound are `packs[end].1` — the assembled pack, bodies plus directory plus header
(`admission.rs:1509-1545`, bounded at 1 MiB of BLOBs per statement). The schema it lands in is
`crates/layerfs-layerstack-store/sql/schema/v7.sql:3-6`: `object_packs(pack_id INTEGER PRIMARY KEY,
data BLOB NOT NULL) STRICT` — the payload *is* the BLOB.

**The path that reaches it.** `crates/layerfs-layerstack-store/src/layerstack.rs:353` —
`CheckedOutputAdmission::new_for_initialization(db)` inside `initialize_layerstack`, the exact entry
point that produced all six rows; the admission set publishes through `AdmissionBatch::publish`
(`admission.rs:1247-1251`).

**`spill.rs` is not a payload home.** `crates/layerfs-layerstack-store/src/objects/spill.rs` owns
`DeferredObjects::Spill(SpillObjects)` — a *candidate* staging structure that bounds resident memory
before admission publishes (`objects.rs:1316`, `objects.rs:2673`, `CANDIDATE_SPILL_BUFFER_BYTES`).
Its `put` (`spill.rs:450-497`) writes `id(32) + length(8) + hints(144) + canonical` into a private
temporary file, and its other users are the seen/location pages and the oversized RAW singleton
(`admission.rs:1212-1213`, `temporary_file("prepared-pack")`). Nothing in it is the published Store.

**And the profile is the same family, so the file length is a fair count.** The reference's write
profile is `crates/layerfs-layerstack-store/src/schema.rs:514-527`: `foreign_keys = ON`,
`journal_mode = MEMORY` (read back and verified at 516), `synchronous = OFF`, `temp_store = MEMORY`,
`cache_size = -SQLITE_PAGE_CACHE_KIB`, `cache_spill = OFF`, `mmap_size = 0`,
`locking_mode = EXCLUSIVE`. This Store's is `core/crates/layerfs-storage/src/sqlite/connection.rs:33-47`:
`journal_mode = MEMORY` (verified), `synchronous = OFF`, `temp_store = MEMORY`, `foreign_keys = ON`,
`busy_timeout = 0`. **Neither is WAL**, so in both cases the main database file's length is the pages
the pager was given, with no `-wal` sidecar holding an undercount.

**The bytes handed to the pager per canonical byte**, same shape, one definition
(`store_database_bytes = std::fs::metadata(&store_path)?.len()`, `benchmark/fs-bench-pro/src/main.rs:2226`;
this row's `resources.space.after.apparent_bytes` is `st_size` of the same file,
`shared/space.py:6,103`):

| | database B | canonical B | per canonical byte |
| --- | ---: | ---: | ---: |
| reference, row 4 | 304,955,392 | 302,182,831 | **1.0092** |
| reference, row 5 | 304,627,712 | 302,182,831 | **1.0081** |
| reference, row 6 | 304,705,536 | 302,182,831 | **1.0083** |
| **this row** | **336,920,576** | **302,231,057** | **1.1148** |

**Consequence for the commission's ranking: direction #1 is closed, and it closes *for* the
comparison rather than against it.** The reference's payload bytes are in the database, its commit is
the same kind of commit, and the matched boundary of step 1 is the right frame. There is no third
column to add and no re-framing to do.

**But the 10.6 % between the last two rows is a real, count-driven difference, and it has a named
mechanism.** This row's `object_packs.data` sums to **332,922,880 bytes** — and
**332,922,880 = 1,270 × 262,144 = `pipeline.packs_created` × `PACK_LIMIT`**, exactly.
`core/crates/layerfs-storage/src/sqlite/write.rs:88-95` creates every pack row **zero-filled at the
pack's capacity**:

```rust
let capacity = i64::try_from(write.capacity)...;
connection.execute(
    "INSERT INTO object_packs (pack_id, data, save_id) VALUES (?1, zeroblob(?2), (SELECT save_id FROM temp.layerfs_read_scope))",
    rusqlite::params![pack_id, capacity],
)?;
```

with `capacity` = `PACK_LIMIT` = 256 KiB for the ordinary, native, whole-file and pooled lanes
(`core/crates/layerfs-storage/src/pack/layout.rs:173-174`, `policy.rs:103`). The row then wrote
**301,865,004 bytes** of pack content into them (`pipeline.pack_bytes_written`, defined at
`cas/placement.rs:249-250` as `bodies + directory + HEADER_LEN` — the bytes actually handed to
SQLite). The difference, **31,057,876 B (10.29 %)**, is declared capacity that no content ever
reached. `SUM(length(data))` is what the receipt's `pack_bodies_bytes` is
(`shared/space.py:159-169`), and the file is that plus `nonpack_bytes` 3,997,696 — to the byte.

**Whether that 31 MB costs *time* is `NOT_MEASURED`, and the honest reading is that it probably does
not.** `write.rs:80-86` states the design intent — *"the pages the write does not touch are never
dirtied"* — and L74's commit arithmetic is already fully explained by the 302 MB actually written
(302 MB at 659 MB/s = 458 ms of the 458.3 ms term), which leaves no room in that term for another
31 MB. So this is filed as a **space** finding with a **time** question attached, not as a lever.
It is also shape-dependent on the reference's side: its *older* 54,46x-object rows carry 1.19 database
bytes per canonical byte, so a 1.01 figure is not a property of the reference's format either.

---

## Step 3 — H1 is priced on a count-driven instrument

`core/benchmark/fs-bench-pro-storage-content/tests/locator_btree_shape.rs`, a labelled diagnostic in
the harness workspace. Three arms, one difference each, 25,245 locator rows, the same ids, the same
column values, the same 1,270 `object_packs` rows seeded before the first reading, the same
connection profile, the same insertion order, the product's own multi-row `INSERT` shape
(`sqlite/write.rs:217-269`), one transaction per arm, release build — the mode the harness measures
in (`runner.py:81,257-261`). One sample per arm; the whole stdout is
`raw/locator_btree_shape.txt`.

| arm | `objects` PK | index | statements | `pages_dirtied` | `objects` pages | `objects_save` pages | µs/row |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| `reference` | `object_id` (32 B) | none | 198 | **320** | 321 | 0 | **1.470** |
| `store-unindexed` | `(object_id, save_id)` (40 B) | none | 198 | **337** | 338 | 0 | **1.966** |
| `store-indexed` | `(object_id, save_id)` (40 B) | `objects_save` | 198 | **601** | 338 | **265** | **3.019** |

`freelist_count` is 0 on all three arms after the inserts, and `PRAGMA page_size` is 4096 on all
three, so `page_count` growth is a live-page count and the owner ruling was honoured. `dbstat` is
available in the linked engine, so the per-B-tree split is measured, not inferred.

**Against the pre-registration, clause by clause:**

| prediction | measured | outcome |
| --- | --- | --- |
| `pages_dirtied` R 250–340 | 320 | **held** |
| `pages_dirtied` C− 280–380 | 337 | **held** |
| `pages_dirtied` C 530–720 | 601 | **held** |
| index share `(C − C−)/C` 30–50 % | **34.88 %** | **held** |
| wider-key share `(C− − R)/C−` 0–15 % | **25.23 %** | **refuted — the prediction was too low** |
| µs/row R 3.0–5.0 | 1.470 | **refuted — the engine is faster than predicted** |
| µs/row C− 3.2–5.5 | 1.966 | **refuted** |
| µs/row C 5.0–8.5 | 3.019 | **refuted** |
| refutation 1: share < 15 % | 34.88 % | did not fire |
| refutation 2: index pages < 200 | **264** | did not fire |
| refutation 3: arms within 10 % | 1.470 / 1.966 / 3.019 | did not fire |
| refutation 4: page size != 4096 | 4096 everywhere | did not fire |
| refutation 5: `freelist_count` != 0 | 0 everywhere | did not fire |
| refutation 6: `dbstat` unavailable | available | did not fire |

**H1's mechanism claim is confirmed at the product's own row shape.** A locator insert into this
Store's `objects` maintains a **second B-tree**: 265 pages of `objects_save` for 25,245 rows, taking
`pages_dirtied` from 337 to 601 (**+78 %**, index share **43.93 %** of all pages written) and the
insert from 1.966 to 3.019 µs/row. The reference's shape maintains one.

**What it is worth, on the row's own insert term.** Two applications, and the truth is between them:

| method | arithmetic | ms |
| --- | --- | ---: |
| absolute — the per-row delta at the row's own row count | (3,019 − 1,966) ns × 25,245 | **26.58** |
| ratio — the index share of the row's own `diag_insert_objects_ns` 143.44 ms | 34.88 % × 143.44 | **50.03** |

The absolute figure is the right one if the row's extra 2.66 µs/row over the replica's 3.02 (5.68 µs
measured against the replica) is work that is *not* B-tree maintenance — statement text that varies
with every chunk width (L73's sign problem) and the per-row scalar subselect; the ratio figure is the
right one if that extra work scales with the B-tree work. **The lever is 26.6–50.0 ms of
`diag_insert_objects_ns`.** The wider-key and extra-columns share — 496 ns/row, **25.23 %**, worth
12.5–36.2 ms on the same two methods — is **not** part of it: those columns are the contract.

### Is it takeable? Yes, and the argument is a re-application of an owner ruling

`objects_save` has exactly **one** consumer in the product. Every SQL site that touches `objects`:

| site | statement | which B-tree serves it |
| --- | --- | --- |
| `core/crates/layerfs-storage/src/sqlite/cleanup.rs:42` | `DELETE FROM objects WHERE save_id=?1 AND object_id IN (SELECT object_id FROM objects WHERE save_id=?1 ORDER BY object_id LIMIT ?2)` | **`objects_save`** |
| `core/crates/layerfs-storage/src/sqlite/lookup.rs:79` | `... FROM objects o JOIN saves s USING(save_id),temp.layerfs_read_scope r WHERE o.object_id IN (...) AND o.pack_id <= ?` | the **PK prefix** `object_id` |
| `core/crates/layerfs-storage/src/sqlite/write.rs:252` | the multi-row `INSERT` | maintains both |
| `core/crates/layerfs-storage/src/sqlite/schema.rs:354` | `SELECT COUNT(*) FROM objects` | neither |

`lookup.rs:79` is driven by `object_id IN (...)`, so it uses the primary key and **cannot** use
`objects_save(save_id, object_id)` — which is also why the primary key cannot simply be reordered to
`(save_id, object_id)`: that would turn the hot read path into a full scan. And `cleanup.rs:42` is
`sqlite/cleanup.rs::abandon`, documented as *"Removes only one definitely failed private save"*, called
only from `cas/lifecycle.rs:65` and `cas/lifecycle.rs:307` — the **definite-failure** path
(`CleanupFailed`, `cleanup_attempted`, `quarantined`). No measured row enters it.

**Owner ruling C has already ruled on this exact class of index.** `core/crates/layerfs-storage/src/sqlite/schema.rs:87-94`:

> **Empty by owner ruling C (R2, T1 #188).** The Store carried two secondary indexes and neither
> earned its bytes: `objects_bases` had no query consumer at all, and **`objects_locations` served one
> bounded cleanup page query**.

`objects_locations` was `UNIQUE INDEX ... ON objects(pack_id, group_number, record_number)`
(`core/crates/layerfs-storage/tests/cas_roundtrip.rs:413`) and it was removed on exactly that
argument. `objects_save` is a different index — it arrived with the multi-writer model in `eb319aaa9`
(#216) — but its position today is the same one ruling C rejected: **one bounded cleanup page query,
on a path no measurement enters.**

*(The discrepancy the commission asked to be noted, resolved: the doc comment is coherent and the
constant is not. The comment describes the requirement as emptied by ruling C; the constant on the
next line lists three names, `objects_save` among them. One of the two is stale. This round did not
change either, and it did not need to: ruling C's argument is what the direction below rests on, and
that argument is in the comment.)*

---

## The direction, chosen from the result

**Direction: drop `objects_save` — the locator's second B-tree — and let `abandon`'s bounded cleanup
page query scan the primary key.**

| | |
| --- | --- |
| **expected** | **26.6–50.0 ms** off `diag_insert_objects_ns` (143.44 ms), on a mechanism measured at 265 index pages and 34.88 % of the replica's insert time |
| **price** | a schema change (the index is in `sql/schema.sql:77`, in `REQUIRED_INDEXES` at `schema.rs:95`, and in every existing Store), and a slower `abandon` on the definite-failure path — `NOT_MEASURED`, and bounded by `CLEANUP_PAGE_ROWS` pages over a full scan |
| **why this one** | it is the only *algorithm-level* difference the commission found that is now both real and measured; its read consumer is a single query on a failure path; and the owner has already ruled on this class of index once |
| **what it is not** | it is not the 143.4 ms term. It is 27–50 ms of it, and the rest of that term — L73's varying statement text and the per-row scalar subselect — is direction #5 of the commission, which this round did not measure |

**Ranked behind it, unchanged or newly visible:**

1. **The build span's uncharted ~70–100 ms** — still the largest block with no attribution at all, and
   rounds 10–13 turned two cheap instruments into a −207.5 ms treatment. Larger than this round's
   finding and still unmeasured.
2. **The native lane's 2.03 records per group** (~40–60 ms, commission direction #3) — a policy
   constant, and the pack-capacity finding above is a second reason to look at the
   `GROUP_LIMIT` / `PACK_LIMIT` pair together: at ~50 KiB groups in a 256 KiB pack each pack carries
   an average 24,455 B of unreached tail, which is 31.06 MB of the file across the row.
3. **The insert statement shape** (commission direction #5) — now with a second, independent reason:
   the replica's per-row cost at this Store's shape is 3.02 µs against the row's 5.68 µs, and the
   difference is work the schema does not explain.

**Closed by this round:** the commission's direction #1 (H2). The reference's payload bytes reach the
pager; the matched-boundary comparison stands; there is no spill to find.

---

## What is not claimed

No arm, no gate, no product change, no release claim. The diagnostic measures a **replica** of the
row's own row shape, not the product's Store, and its µs/row column is a **single sample** — the page,
`dbstat` and statement columns are counts and do not depend on that. Arm `reference` is measured in
an insertion order arbitrary with respect to `object_id`, which is what this Store does
(`cas/placement.rs:166-182` builds rows in placement order and never sorts) and **not** what the
reference does (it sorts its locators by `object_id` before inserting,
`crates/layerfs-layerstack-store/src/objects/admission.rs`), so arm R is measured in an order *less*
favourable than its own product uses and **the measured index share is a lower bound**. Arm R also
binds five parameters per row against the Store arms' six, which favours R for the same reason. No
v0.1.6 pairing is claimed: the six reference rows are `cache_contract: null`,
`verification_status: NOT_RUN`, on another workspace with no matchable seal, and their own spread
spans parallelism (1.93–4.43 implied cores) and cache state, not one algorithm.

## Checks as run

- The diagnostic: `cargo +1.85.1 test --release --manifest-path
  core/benchmark/fs-bench-pro-storage-content/Cargo.toml --test locator_btree_shape -- --nocapture
  --test-threads=1` — **1 passed, 0 failed**, 0.23 s. Its whole stdout is filed.
- Lock parity, after adding `rusqlite = "=0.40.2"` (the product's own pin and feature set) to the
  harness manifest: `python3 core/benchmark/fs-bench-pro-storage-content/shared/test_lock_parity.py`
  — **PASS, 46 shared package entries compared, 0 mismatches.** The harness's `Cargo.lock` already
  carried `rusqlite` 0.40.2 and `libsqlite3-sys` 0.38.2 at the product's versions and checksums, so
  nothing re-resolved and `--locked` still holds.
- **Not run:** the product's suites (`cargo +1.85.1 test/clippy/fmt --manifest-path core/Cargo.toml
  --locked`) and `core/tools/check_product_boundary.py`. No product source or product manifest was
  touched by this round, and the harness is not product source — its own `Cargo.toml` says so and
  `check_product_boundary.py` scans only `core/crates/*/src` and `core/crates/*/sql`. The harness's
  own wider suite was not re-run either: this round added one test file and one manifest line, and
  ran exactly the test it added plus the parity check.
