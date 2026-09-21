# Pre-registration — #219 round 7: the page cache, declared in pages

Written **before** the first measured run of this arm and before any product edit.
Parent: round 6's row `ns19-E1-pagesize-20260921T073509Z` (landed as `41f4b7b8f`).

## The one difference

**The connection profile declares the page cache as a page count (`STORE_CACHE_PAGES = 512`)
read against the Store's own `page_size`, where it previously declared nothing and left
SQLite's engine default (`cache_size = -2000` KiB, 2 MiB) in force.**

One number, one pragma site, derived from a value the file already carries: a 4096-byte
Store keeps 2 MiB (`512 x 4096`), a 65536-byte Store gets 32 MiB (`512 x 65536`). Nothing
else moves — not the page size, not the schema, not a capacity, not the cadence.

512 is not a tuned value: it is SQLite's own default expressed in the unit the engine
charges in (`-2000` KiB at the 4096-byte page the engine assumes is 512 pages), and it is
the page count round 6's Store silently lost when the page grew 16x.

## Why round 6 needs it, from round 6's own receipts

Round 6 (page_size 4096 -> 65536) did exactly what the count said it would: the row's store
went from **82,129 pages to 5,319** (`sample.sqlite`, read back with an independent
connection). `diag_commit_total_ns` fell 402.7 -> 263.3 ms. But
`diag_insert_objects_ns` rose **151.8 -> 294.9 ms** on an unchanged 16,595 statements
(9.15 -> 17.77 us per statement), `diag_write_pack_total_ns` rose 231.0 -> 264.7, CPU rose
1835.1 -> 1974.3 ms and the row's work figure rose 1821.0 -> 1957.6 ms. Control regions that
no page can touch were flat (`profile_full_ns` 245.01 -> 245.39, +0.2 %), so this is not a
machine window.

The cause is a count, and `spill-probe.py` (round 6's directory; random 24-byte keys, the
product's full table set, 800 transactions, 24,800 rows) reproduces it and prices it:

| page_size / cache | commit | blob | insert (us/row) | signature |
| --- | ---: | ---: | ---: | ---: |
| 4096 / 2 MiB (the engine default) | 514.37 ms | 51.40 ms | 104.12 (4.20) | 16.62 ms |
| 65536 / 2 MiB (round 6 as landed) | 149.57 ms | 38.67 ms | **164.51 (6.63)** | **66.38 ms** |
| 65536 / 8 MiB | 194.16 ms | 36.33 ms | **111.14 (4.48)** | 45.54 ms |
| 65536 / 32 MiB | 192.51 ms | 36.15 ms | 112.65 (4.54) | 45.68 ms |
| 16384 / 2 MiB | 247.21 ms | 38.98 ms | 111.74 (4.51) | 24.71 ms |

A 2 MiB cache is 32 pages of 64 KiB. The row's statements seek a random leaf of `objects`,
`objects_save` and the `content_signatures` ring; when the page is not resident the engine
reads, journals and modifies a whole 64 KiB page instead of 4 KiB, and past 32 dirty pages
it spills and re-dirties. Raising the cache removes the penalty in the probe —
insert recovers 88 % of it — and `8 MiB` and `32 MiB` are indistinguishable there
(111.14 vs 112.65 us; 194.16 vs 192.51), which is why the declared value is the
principled 512 pages rather than the smaller of two probe rows.

## Prediction, in the instruments' own units

| instrument | round 6 (E1) | predicted | derivation |
| --- | ---: | ---: | --- |
| `diag_insert_objects_ns` | 294.9 ms | **165–205 ms** (17.8 -> 10.0–12.4 us/statement) | 88 % of the +143.1 ms penalty removed |
| `diag_write_pack_total_ns` | 264.7 ms | 235–255 ms | probe 38.67 -> 36.33, product's own +33.6 explained by the same page |
| `diag_commit_total_ns` | 263.3 ms | **290–345 ms** | the cache stops spilling, so more of the same bytes is written at commit (probe 149.57 -> 194.16, +30 %) |
| `operation_work_ns` | 1957.6 ms | **1750–1860 ms** | the sum above |
| CPU user+system | 1974.3 ms | 1850–1960 ms | the page copies are CPU; wall-fixed costs are not |

The predicted row movement is at the edge of the ~250 ms this machine drifts, so **the claim
is carried by `diag_insert_objects_ns` over its own 16,595 statements** — a duration whose
cause is a per-page count — and not by the row total. One sample per arm; no confirmation run.

## What would refute it

1. `diag_insert_objects_ns` >= 260 ms (the cache does not remove the page penalty).
2. `diag_commit_total_ns` >= 400 ms, or `operation_work_ns` >= 1957.6 ms (the round does not
   recover what round 6 spent).
3. `pipeline.commits` != 800, `pipeline.inserted` != 25245, `pipeline.statements` != 16595,
   `pipeline.pack_bytes_written` != 302406480 — the change is not work-neutral.
4. `digest:filesystem_root` != `1d6fba29aba105aefbb668c34b026f8b89d20773fc36d548b991034dc3857847`,
   or any of the 13 gates fails.
5. A Store at another page size stops opening or reading, or `cas_reuse` / `delta_payload` /
   `pack_watermark` / `multi_writer` / `visibility` / `persistence_failure` / `pack_locator`
   is not green.

## Both directions of the read path

A page cache is **not stored in the file and not a format property**: it is a per-connection
engine setting, so a Store created by any build opens under any cache, and this change alters
no byte of any Store. It is declared in the profile and re-verified on acquisition like the
journal mode and the synchronous setting, and it is derived from the page size the *file*
reports, so a Store at another page size gets the same 512 pages and never a starved cache.
