# Pre-registration — #219 round 18: drop the locator's second B-tree

Written **before** the first edit and before the first run of this arm.

**Owner ruling carried in: the database page size stays 4 KiB.** Nothing here sets it; the change
below does not touch `PRAGMA page_size`, the journal mode, `synchronous`, `temp_store`,
`foreign_keys`, `cache_size`, `cache_spill` or `mmap_size`.

## Control

`benchmark-results/issue219/ns19-Q2-repin-20260921T115200Z/pipeline-namespace-10000` — **PASS, 13/13
gates, 14/14 pinned counters**, one sample, `--verify full`:

| | |
| --- | ---: |
| `pipeline.operation_work_ns` | 1,218,880,166 ns |
| `pipeline.diag_insert_objects_ns` | 143,438,848 ns |
| `pipeline.diag_commit_total_ns` | 458,259,288 ns |
| `pipeline.inserted` | 25,245 |
| `pipeline.commits` | 95 |
| `digest:filesystem_root` | `1d6fba29aba105aefbb668c34b026f8b89d20773fc36d548b991034dc3857847` |

## The one difference

**`CREATE INDEX objects_save ON objects(save_id, object_id);` is removed**, together with the two
declarations that require it. Nothing else moves: no column, no constraint, no pack framing, no
pragma, no statement text, no transaction cadence, no worker count.

| file | change | why it is forced by the one difference |
| --- | --- | --- |
| `core/crates/layerfs-storage/sql/schema.sql:77` | the `CREATE INDEX` is deleted | the difference itself |
| `core/crates/layerfs-storage/src/sqlite/schema.rs:95` | `REQUIRED_INDEXES` 3 → 2, `objects_save` dropped from the list | `validate` refuses a Store whose required index is missing (`schema.rs:129-139`), so a Store created without the index could not be opened |
| `core/crates/layerfs-storage/src/policy.rs:51` | `SCHEMA_VERSION` 9 → 10 | `sql/schema.sql:12` states the rule the Store already follows — *"Older schemas are rejected, never migrated"* — and a schema-9 Store carries an index a schema-10 build no longer requires. `identity()` (`schema.rs:176-186`) refuses it by version, before any table is read |

The doc comments beside `SCHEMA_VERSION` and `REQUIRED_INDEXES` are updated because both enumerate
their reasons; the stale half of the `REQUIRED_INDEXES` comment (it describes the requirement as
emptied by ruling C while the constant lists three names) is corrected as a consequence of touching
the line, not as a separate change.

## Why this is a product change and not a diagnostic on a copy

Squad C pre-registered this lever as arm **C** — *"drop `objects_save`, `signatures_save`,
`packs_save` on a copy for this arm only"*, expected *"≥ 60 ms of page-write work"*, refuted below
30 ms — and marked it *"diagnostic evidence only"*
(`evidence/issue219-squadC-cadence-20260921T044258Z/pre-registration.md`, the separating-arms table).
It was never run, and it **cannot** be run as written: `validate` refuses a Store whose required
index is missing (`schema.rs:129-139`), so a copy with the index dropped does not open. The choice is
therefore between a replica that bypasses the product (round 17's, already filed) and the product
change itself. This round takes the second, on the commission's own terms —
`issue219-ns19-algorithm-gap-handoff.md` §2: *"this is a **contract**, not an accident, and it is the
only structural difference found so far that sits directly on the 143.4 ms insert term … the mandate
permits changing [it] with an argument"* — and on owner ruling C's argument, which removed
`objects_locations` for serving *"one bounded cleanup page query"* (`schema.rs:87-94`); round 17
established that `objects_save`'s only consumer is that same query on the definite-failure path
(`sqlite/cleanup.rs:42`, `cleanup::abandon`, called only from `cas/lifecycle.rs:65` and `:307`), and
that the hot read path (`sqlite/lookup.rs:79`) is driven by `object_id` and uses the primary key.

## Prediction, in the row's own units

Round 17 measured, on a count-driven replica of this row's row shape (25,245 rows, one transaction,
release build): the index is **265 pages**, **43.93 %** of the pages written, and takes the insert
from **1.966 to 3.019 µs/row**. Transferred by the two admissible methods:

| quantity | prediction | derivation |
| --- | ---: | --- |
| `pipeline.diag_insert_objects_ns` | **93.4 – 116.9 ms** | 143.44 − 26.58 (absolute: 1,053 ns/row × 25,245) to 143.44 − 50.03 (ratio: 34.88 %) |
| `pipeline.operation_work_ns` | **1168.9 – 1192.3 ms** | the same two subtractions from 1,218.88 |
| `pipeline.diag_commit_total_ns` | **moves ≤ 2 ms** | L74 priced this term byte-bound at 659 MB/s over 302 MB; the index's 265 pages are 1.09 MB, 0.36 % of 302 MB ≈ 1.6 ms |
| `pipeline.commits`, `pipeline.inserted`, `pipeline.content_objects`, `pipeline.content_bytes` | **unchanged** | the index affects no count |
| `digest:filesystem_root` | **unchanged** | the index stores no canonical byte |

## What would refute it

1. **`operation_work_ns` falls by less than 15 ms.** Then the replica's mechanism does not transfer to
   the product, the round-17 transfer is wrong, the change is **withdrawn and reverted**, and the
   direction is closed with that result.
2. **Any pinned counter moves, or the root digest changes.** Then the change is not the work-neutral
   schema edit declared above; the round stops and the movement is diagnosed before anything is kept.
3. **`diag_commit_total_ns` moves by more than 15 ms.** Then the index's pages were being paid in the
   commit term rather than the insert term, round 17's attribution is wrong, and the change is not
   kept on this prediction.
4. The row is not **PASS 13/13** with **14/14** pinned counters, or `--verify` is not `full`, or the
   complete command exceeds its budget, or the sample is not one sample per case per arm.
5. `sqlite/cleanup.rs`'s `abandon` fails any test that covers it. Its query becomes a full scan of
   `objects` — declared, expected, and `NOT_MEASURED` in time — but it must still be *correct*.

## What is not claimed

No v0.1.6 pairing, no release claim, no gate raised or lowered, and no claim about the `abandon` path's
cost. This is one arm against one control on one case; the row stays single-worker under
`AGENTS.md` §3.8. `LAYERFS_CONSTRUCTION_WORKERS=1` is exported by the harness and is not raised.
