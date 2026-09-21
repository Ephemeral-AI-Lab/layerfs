# #219 round 7 — the page cache in pages: it recovered a third of round 6, and round 6 still loses

Pre-registration: `pre-registration.md`. Careful: the pre-registration file was written under the
directory timestamp `20260921T073908Z` and its own path name says `ns19f-cache`; this README is the
record. Row: `benchmark-results/issue219/ns19-F1-cache-20260921T074257Z`, **PASS, 13/13 gates**,
landed as `3e1814a91` and **reverted** with round 6 by the commit after it.

## 1. What was registered, and what happened

One difference against round 6: the connection profile declares the page cache as a **page count**
(`STORE_CACHE_PAGES = 512`, derived from the Store's own `page_size`) instead of leaving SQLite's
`-2000` KiB byte default in force.

| pre-registered | prediction | F1 | verdict |
| --- | --- | ---: | --- |
| `diag_insert_objects_ns` | 165–205 ms | **279.7 ms** | **MISSED**; the refutation bound (≥ 260 ms) **fired** |
| `diag_write_pack_total_ns` | 235–255 ms | 231.3 ms | inside, at the low edge |
| `diag_commit_total_ns` | 290–345 ms | 230.7 ms | missed in the favourable direction |
| `operation_work_ns` | 1750–1860 ms | 1862.0 ms | missed by 2 ms at the top of the range |
| CPU user+system | 1850–1960 ms | 1896.7 ms | inside |

**The round is a refutation on its own registered instrument.** The treatment was registered to
remove a per-statement page penalty of 143.1 ms and removed 15.2 ms of it.

## 2. What it did do — measured, and not enough

| counter | D4b (4096) | E1 (65536, 2 MiB) | F1 (65536, 512 pages) |
| --- | ---: | ---: | ---: |
| `diag_commit_total_ns` | 402.7 ms | 263.3 ms | **230.7 ms** |
| `diag_insert_objects_ns` | 151.8 ms | 294.9 ms | 279.7 ms |
| `diag_write_pack_total_ns` | 231.0 ms | 264.7 ms | **231.3 ms** |
| `diag_offer_total_ns` | 868.8 ms | 1055.7 ms | 1002.8 ms |
| `diag_flush_batch_ns` | 1062.7 ms | 1344.3 ms | 1286.4 ms |
| `profile_total_ns` | 1063.2 ms | 1100.6 ms | 1015.3 ms |
| `operation_work_ns` | **1821.0 ms** | 1957.6 ms | 1862.0 ms |
| CPU user+system | **1835.1 ms** | 1975.3 ms | 1896.7 ms |
| `phases.operation_ns` | **1864.9 ms** | 2257.0 ms | 2009.3 ms |
| complete command | **3130.7 ms** | 3534.9 ms | 3323.0 ms |
| peak RSS | **524.4 MB** | 683.2 MB | 560.6 MB |
| store pages | 82,129 | 5,319 | 5,319 |
| `diag_finish_drop_ns` (the close) | **39.4 ms** | 294.3 ms | 141.7 ms |

Work counters again identical in all three rows: `commits` 800, `inserted` 25245,
`statements` 16595, `pack_bytes_written` 302406480. Round 7 recovered 95.5 ms of round 6's 136.6 ms
and round 6 still stands 41 ms of work and 62 ms of CPU above the 4096-byte baseline, with a
complete command 192 ms worse and 36 MB more resident.

**The declared cache is verifiably in force** — the save's own connection reports
`cache_size = -32768` at `page_size = 65536` (a 4096-byte Store reports `-2048`, exactly SQLite's
own 2 MiB default), asserted on `SaveConnectionProfile` through
`tests/page_size.rs::the_page_cache_is_a_page_count_and_follows_the_stores_own_page_size`.
Reading `PRAGMA cache_size` on the row's `sample.sqlite` afterwards reports `-2000`: that is a
*fresh* engine connection's default, not the save's, and is not evidence about this round.

## 3. Why the cache could not fix it

Section 4 of round 6's README states the arithmetic that round 7 confirms: the penalty is not a
*read* of an uncached page, it is the pager's **image copy per modified page**. At 65536 bytes a
wave's ~31 scattered row inserts land on nearly every leaf of `objects` (23 pages), and at 4096 bytes
on ~31 of 343; the journal therefore copies ~1.2 MB per wave against ~130 KB, about 1 GB against
104 MB over 800 waves. A cache changes whether a page must be fetched; it cannot change what the
pager does to a page it is about to modify. The `spill-probe.py` rows that predicted an 88 % recovery
were measured on a shape whose leaf count per transaction could be kept resident, and they were
wrong about this row.

## 4. Disposition

Both landings are **reverted** in the commit that follows (`41f4b7b8f` and `3e1814a91`), because
neither produced a movement toward the target and both made the row slower on `operation_work_ns`,
CPU, the complete command and RSS. The diagnostics, the pre-registrations and the rows stay.

Checks as run for the landed change: `cargo test -p layerfs-storage` 34 binaries, 0 failed; the seven
write-path invariants green; the whole core workspace `--no-fail-fast` 109 `test result: ok`,
0 failed; `clippy --all-targets` clean; `fmt --check` clean;
`core/tools/check_product_boundary.py` PASS. Production LOC 31391 -> 31414 (delta +23).
