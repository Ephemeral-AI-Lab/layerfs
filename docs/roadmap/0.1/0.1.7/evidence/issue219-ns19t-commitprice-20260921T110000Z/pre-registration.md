# Pre-registration — #219 round 19: the commit term's page price, and whether pack capacity is flushed

Written **before** the first edit and before the first run. This round is an **instrument**, not a
treatment: **no product line changes**, no arm is registered as a shippable change, and no gate is
claimed. Owner ruling carried in: **the database page size stays 4 KiB** — read and asserted, never
set.

## Why this round exists

Round 18 (`issue219-ns19s-indexdrop-20260921T103500Z`) took **27,022,997 ns** out of
`diag_commit_total_ns` by removing **1,101,824 B** of index — **24.5 ns per byte, 41 MB/s**. The same
term prices the row's 301,865,004 B of pack payload at **1.43 ns per byte, 700 MB/s**. **One term, two
page prices, 17× apart.** L74 closed that term as "byte-bound, closed as a lever" on the 659 MB/s
argument; round 18 does not refute the arithmetic, it shows the arithmetic is a *coincidence of the
pack's shape* rather than the term's law. At **431.24 ms it is 38.3 % of the row**, larger than the
entire 126.73 ms gap to the 1 s target, and the receipt publishes **no** `cache_size`, `cache_spill` or
`mmap_size` — the profile the closure quotes has never been read on the row it is quoted for.

Squad C registered this instrument two campaigns ago and it has never been built
(`issue219-squadC-cadence-20260921T044258Z/pre-registration.md`, arm D: *"read
`sqlite3_db_status(CACHE_WRITE / CACHE_SPILL / CACHE_USED)`, `sqlite3_status(PAGECACHE_*)` and
`page_count`/`freelist_count` on the product's own connection, per save"*).

## The one instrument

`core/benchmark/fs-bench-pro-storage-content/tests/commit_page_price.rs`, a labelled diagnostic in the
harness workspace (not product source — its own `Cargo.toml` says so and
`core/tools/check_product_boundary.py` scans only `core/crates/*/src` and `core/crates/*/sql`). It
drives the engine directly and reads, before and after each region:

| reading | API | what it counts |
| --- | --- | --- |
| `CACHE_WRITE` | `sqlite3_db_status(SQLITE_DBSTATUS_CACHE_WRITE)` = 9 | **dirty cache entries written to the database file** — the commit's flush |
| `CACHE_SPILL` | `SQLITE_DBSTATUS_CACHE_SPILL` = 12 | dirty entries spilled because the cache overflowed |
| `CACHE_USED` | `SQLITE_DBSTATUS_CACHE_USED` = 1 | page-cache heap bytes for this connection |
| `PAGECACHE_USED` / `PAGECACHE_OVERFLOW` | `sqlite3_status` 1 / 2 | page-cache pages, and pages held outside it |
| `page_count`, `freelist_count` | pragmas | pages allocated, and free pages |
| `st_size`, `st_blocks` | `std::fs::metadata` | file length and blocks actually allocated |

`Connection::handle()` (`rusqlite-0.40.2/src/lib.rs:952`) gives the raw handle;
`libsqlite3-sys = "=0.38.2"` is added to the harness manifest at the version and checksum the lock
already carries through `rusqlite`, so lock parity is preserved. The connection profile is the
product's (`sqlite/connection.rs:33-47`): `journal_mode = MEMORY` (read back and asserted),
`synchronous = OFF`, `temp_store = MEMORY`, `foreign_keys = ON`, `busy_timeout = 0`.

### Question 1 — is the commit priced per page-write or per byte?

The three round-17 shapes, unchanged, **with the COMMIT timed separately from the insert loop** and
`CACHE_WRITE` read across each: `reference` (32-byte key, no index), `store-unindexed` (40-byte key),
`store-indexed` (40-byte key plus `objects_save`). Same 25,245 ids, same column values, same 1,270
seeded packs, same insertion order, same statement shape, one transaction per arm.

**Hypothesis.** The commit's price is **per page-write**, at a price that is a property of the engine
and the profile, not of the table. The pack payload looks byte-bound only because an append-only pack
writes each page exactly once: 301,865,004 B ÷ 4096 = **73,698 pages**, and 431.24 ms ÷ 73,698 =
**5.85 µs per page-write**. On that price the index's 27.02 ms is **4,619 page-writes** — its 265 pages
dirtied and rewritten ~17 times across the row's 95 transactions.

| arm | predicted `CACHE_WRITE` pages | predicted commit |
| --- | ---: | ---: |
| `reference` | 300–360 | 1.8–2.4 ms |
| `store-unindexed` | 320–390 | 2.0–2.6 ms |
| `store-indexed` | 560–680 | 3.4–4.5 ms |

and the price **`commit_ns ÷ CACHE_WRITE`** predicted in **4–7 µs per page-write on all three arms**.

### Question 2 — is the row's declared pack capacity flushed?

Round 17 measured that this row's `object_packs.data` sums to **332,922,880 B = 1,270 × 262,144 =
`packs_created` × `PACK_LIMIT`**, because `sqlite/write.rs:88-95` creates every pack zero-filled at
capacity with `zeroblob(?2)`, while the content written into them is 301,865,004 B. The difference,
**31,057,876 B = 7,582 pages**, is capacity no content reached. Whether the commit flushes it was
recorded `NOT_MEASURED`. Three arms on `object_packs` alone, 1,270 rows each, one transaction each:

| arm | per pack | total payload |
| --- | --- | ---: |
| `packs-zeroblob` | `INSERT … VALUES(?1, zeroblob(262144))`, nothing written into it | 0 B written |
| `packs-partial` | the same, then **237,689 B** written in place through a BLOB handle — the row's own average (`301,865,004 ÷ 1,270`) | 301,865,004 B |
| `packs-full` | `INSERT … VALUES(?1, ?2)` binding a full 262,144-byte blob | 332,922,880 B |

**Prediction, if the capacity is not flushed:** `packs-zeroblob` writes only the BLOB headers and
pointer pages — `CACHE_WRITE` **1,000–3,000** pages, commit **5–20 ms** — and `packs-partial` writes
about **58 pages per pack** rather than 64, i.e. `CACHE_WRITE` **≈ 73,700** against `packs-full`'s
**≈ 81,300**, a gap of about **7,582 pages**, which is exactly the 31 MB.

## What would refute it

1. **Q1:** the price `commit_ns ÷ CACHE_WRITE` differs by more than **2×** across the three shapes.
   Then the commit's price is not one per-page price, the 5.85 µs figure is an artifact of the pack
   layout, and this round reports the three prices without claiming a law.
2. **Q1:** `CACHE_WRITE` does not track `page_count` growth on any arm (a gap above 15 %), meaning the
   counter is not measuring the flush this round takes it for.
3. **Q2:** `packs-zeroblob` reports `CACHE_WRITE` **≥ 70,000** pages. Then the declared capacity **is**
   flushed, round 17's 31 MB is worth roughly **44 ms** at the byte price, and it becomes a direction
   rather than a space note.
4. **Q2:** `packs-partial` and `packs-full` agree within 5 %. Then the unwritten tail is flushed after
   all and the answer to Q2 is the opposite of the prediction.
5. `PRAGMA page_size != 4096`, or `sqlite3_db_status` returns non-zero, or `journal_mode` is not
   `MEMORY` on any arm — the run is **void** for that arm.
6. Any of the readings is unavailable in the linked engine — it is reported `NOT_MEASURED` rather than
   inferred.

## What is not claimed

No product change, no arm, no gate, no release claim, and no claim that the commit term can be reduced.
The diagnostic measures the engine on a replica of the row's shapes; it does not measure the product.
One sample per arm, no re-run, no best-of. `LAYERFS_CONSTRUCTION_WORKERS=1` is untouched. Nothing here
re-opens a closed lever other than by measurement: codec level, stored frames, page size,
cache-in-pages and journal modes are all left as they stand.
