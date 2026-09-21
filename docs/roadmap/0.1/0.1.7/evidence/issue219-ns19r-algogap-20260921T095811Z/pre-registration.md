# Pre-registration — #219 algorithm-gap exploration: the locator table's B-tree shape (H1)

Written **before** the first run of the diagnostic and before any product edit. This round is an
**exploration**, not a treatment. No product line changes, no arm is registered, no gate is claimed,
and no performance number is asserted. The instrument is a labelled **diagnostic** in the sense of
`docs/general/benchmark_rules.md` §3.1 and it is reported as one, beside the receipts it reads.

**Owner ruling carried into this round: the database page size stays 4 KiB.** It is not a lever here.
The diagnostic *reads* `PRAGMA page_size` and fails if it is not 4096; it never sets it, and no
direction in this round proposes changing it.

Commission: `docs/roadmap/0.1/0.1.7/issue219-ns19-algorithm-gap-handoff.md` §3 steps 1–3.

---

## Step 1 — re-derive §1a from the receipts, and state the boundary

Deliverable: one row per receipt, `NOT_MEASURED` for any cell without evidence. The boundary used is
stated in the report; if the re-derivation disagrees with the commission, the report says so with the
file and line. **This step runs no measurement**: it reads receipts that already exist.

## Step 2 — settle H2's spill question by reading, then by counting

Deliverable: a quoted `file:line` for where the reference's payload bytes go, and the bytes the
reference hands to the pager per canonical byte, against this Store's. **No new run.** The readings
are the reference's own `INSERT INTO object_packs` site and its write profile, plus the
`store_database_bytes` / `store_canonical_bytes` pair the six reference receipts already publish and
the row's own `resources.space` block.

## Step 3 — price H1 on a count-driven instrument

The one difference under test, and nothing else:

| arm | `objects` primary key | key width | secondary index on `objects` | columns |
| --- | --- | ---: | --- | ---: |
| **R** — the reference's shape, `crates/layerfs-layerstack-store/sql/schema/v7.sql:9-16` | `object_id` | 32 B | **none** (`grep -c "CREATE INDEX" v7.sql` = 0) | 5 |
| **C** — this Store's shape, `core/crates/layerfs-storage/sql/schema.sql:66-77` | `(object_id, save_id)` | 40 B | `objects_save(save_id, object_id)` | 8 |
| **C−** — this Store's shape with the secondary index dropped | `(object_id, save_id)` | 40 B | none | 8 |

`C−` exists so that the two effects are separable rather than pooled: **R → C−** is the wider key and
the three extra columns, **C− → C** is the second B-tree alone. Every other thing is identical across
the three arms: the same 25,245 object ids, the same `save_id`, the same `pack_id` /
`group_number` / `record_number` values, the same `object_packs` and `saves` rows, the same connection
profile, the same page size, the same insertion order, the same statement shape, and one transaction
per arm.

The instrument is
`core/benchmark/fs-bench-pro-storage-content/tests/locator_btree_shape.rs`, a diagnostic test in the
harness workspace (the harness is **not** product source — its own `Cargo.toml` says so, and
`core/tools/check_product_boundary.py` scans only `core/crates/*/src` and `core/crates/*/sql`). It
uses `rusqlite` at the version the product locks (`=0.40.2`, `libsqlite3-sys` 0.38.2 — the harness's
own `Cargo.lock` already carries both at the product's versions and checksums, so
`shared/test_lock_parity.py`'s one-directional rule is preserved).

Counts reported per arm: `PRAGMA page_count` before and after the locator inserts, `PRAGMA
freelist_count` after, **`dbstat` pages per B-tree** (`objects` and, in `C`, `objects_save`), the
number of statements issued, and the timed `elapsed_ns` of the locator inserts only — so `µs per row`
and `pages dirtied per row` are both read off the engine rather than inferred.

### Fidelity, stated as the limits it is

- **Insertion order is arbitrary with respect to `object_id`**, because that is what this Store does:
  `cas/placement.rs:166-182` builds the rows in placement order and inserts them unsorted. The
  reference *sorts* its locators by `object_id` before inserting
  (`crates/layerfs-layerstack-store/src/objects/admission.rs`, `locators.sort_unstable_by_key(...)`).
  Holding one order across all three arms is what makes the arms comparable; it also means arm **R**
  is measured in an order *less* favourable than the one its own product uses, so **R's cost here is
  an upper bound and the measured index share is a lower bound.**
- **The statement shape is the product's**: multi-row `INSERT ... VALUES (?,?,?,?,?,?,(SELECT save_id
  FROM temp.layerfs_read_scope)),...` (`sqlite/write.rs:217-260`), chunked by the engine's variable
  limit and capped. One transaction per arm, because the mechanism under test is B-tree maintenance,
  not commit cadence — L74 already priced a COMMIT at 0.21 ms and this round does not re-open it.
- **The connection profile is the product's** (`sqlite/connection.rs:33-47`): `journal_mode = MEMORY`,
  `synchronous = OFF`, `temp_store = MEMORY`, `foreign_keys = ON`, `busy_timeout = 0`. It is the
  reference's family too (`crates/layerfs-layerstack-store/src/schema.rs:514-527`), which is what
  makes the two stores' file lengths comparable in step 2.
- The rows are a **replica** of the row's own row shape, not the product's Store. This measures a
  mechanism; it does not measure the product.

### Prediction, in the instrument's own units

25,245 rows of ~40 B (arm R) and ~45 B (arms C, C−) in 4096-byte pages, keys in arbitrary order:

| arm | predicted `objects` B-tree pages | predicted index pages | predicted `pages_dirtied` | predicted µs/row |
| --- | ---: | ---: | ---: | ---: |
| **R** | 250–340 | 0 | 250–340 | 3.0–5.0 |
| **C−** | 280–380 | 0 | 280–380 | 3.2–5.5 |
| **C** | 280–380 | 250–340 | **530–720** | 5.0–8.5 |

and the number the round exists to produce — the second B-tree's share of the locator insert term:

| quantity | prediction | derivation |
| --- | --- | --- |
| index share of the arm's own insert time, `(C − C−)/C` | **30–50 %** | two B-trees per row against one |
| wider-key share, `(C− − R)/C−` | **0–15 %** | +8 B of key, +3 columns |
| applied to `diag_insert_objects_ns` **143.44 ms** (`ns19-Q2-repin-20260921T115200Z`) | **43–72 ms** | the index share × 143.44, as an upper bound on what dropping it buys |

### What would refute it

1. `(C − C−)/C` **< 15 %** — then the secondary index is not a lever, direction #2 of the commission
   is refuted, and the 143.4 ms insert term is not where the schema difference lives.
2. `pages_dirtied(C) − pages_dirtied(C−)` **< 200 pages** — then the index is not maintaining a second
   B-tree of the predicted size and **H1's mechanism claim is wrong as stated**, not merely small.
3. The three arms' timed inserts land **within 10 % of each other** — then the µs/row instrument is
   not resolving the mechanism and the round reports `INCONCLUSIVE` rather than a number.
4. `PRAGMA page_size != 4096` on any arm — the run is **void** (owner ruling).
5. `freelist_count != 0` on any arm after the inserts — then `page_count` growth is not a live-page
   count and every page figure is reported as an upper bound, labelled.
6. `dbstat` is unavailable in the linked engine — the per-B-tree split is reported `NOT_MEASURED` and
   only `page_count` growth is claimed.

### What is not claimed

No arm, no gate, no product change, no release claim, and no claim that the reference's speed comes
from this difference. The reference's own advantage, if it exists, is not established by measuring a
replica of its schema. Nothing here re-opens a closed lever: codec level, stored frames, page size,
cache-in-pages, journal modes, transaction cadence and worker counts are all left as they stand.
