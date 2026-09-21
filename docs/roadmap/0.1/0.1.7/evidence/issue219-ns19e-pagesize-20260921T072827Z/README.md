# #219 round 6 — the Store's page size: a count that moved exactly as predicted, and a row that did not

Pre-registration: `pre-registration.md`. Probes: `page-probe.py` (`page-probe.out`), then, after
the prediction was missed, `statement-probe.py` (`statement-probe.out`) and `spill-probe.py`
(`spill-probe.out`) as **labelled diagnostics**. Row: `benchmark-results/issue219/ns19-E1-pagesize-20260921T073509Z`,
**PASS, 13/13 gates**, landed as `41f4b7b8f` and **reverted** by the commit after it.

## 1. What was registered, and what happened

One difference: `PRAGMA page_size = 65536` at Store creation (`STORE_PAGE_SIZE_BYTES`), where the
product previously set nothing and a Store was created at SQLite's 4096-byte default.

| pre-registered | prediction | E1 | verdict |
| --- | --- | ---: | --- |
| store pages | ~5,200 | **5,319** (was 82,129) | **hit** |
| `diag_commit_total_ns` | 102 ms (accept 90–150) | **263.3 ms** (was 402.7) | **MISSED** by 161 ms; refutation bound (≥ 300 ms) not reached |
| `diag_write_pack_total_ns` | ~200 ms | 264.7 ms (was 231.0) | missed |
| `operation_work_ns` | 1450–1580 ms | **1957.6 ms** (was 1821.0) | **missed, wrong direction** |

The refutation clause that fired is the row itself: the treatment did not move toward 1 s, it moved
away. **A duration whose cause is a count is not enough when the count's saving is spent elsewhere**,
and the round is recorded as a refutation rather than a movement.

## 2. It is the page size and not the machine window

Three regions no page can touch were flat across the two rows, so the window is comparable:

| control region | D4b | E1 |
| --- | ---: | ---: |
| `profile_full_ns` (zstd/frame encode, no SQL) | 245.013 ms | 245.388 ms (+0.2 %) |
| `profile_delta_ns` | 0.093 ms | 0.092 ms |
| `span_build_ns` (C1 tree build) | 358.736 ms | 368.175 ms (+2.6 %) |

And the work counters are identical, so nothing about *what* was written moved:

`commits` 800, `inserted` 25245, `statements` 16595, `pack_bytes_written` 302406480,
`packs_created` 1268, `pack_appends` 15534, `full_records` 25241, `prefix_records` 4,
`content_bytes` 301171810, `pipeline.bindings` 10100 — **all unchanged**.

The extra work is real and it is CPU, not wall: **CPU user+system 1835.1 -> 1974.3 ms (+139.2)**,
matching the row's +136.6 ms of work. Peak RSS rose 524.4 -> 683.2 MB.

## 3. Where it went

| counter | D4b | E1 | delta |
| --- | ---: | ---: | ---: |
| `diag_commit_total_ns` | 402.7 ms | 263.3 ms | **−139.4** |
| `diag_insert_objects_ns` | 151.8 ms | 294.9 ms | **+143.1** (16,595 statements both rows: 9.15 -> 17.77 us/statement) |
| `diag_write_pack_total_ns` | 231.0 ms | 264.7 ms | +33.6 |
| `diag_wave_ns` | 112.7 ms | 97.2 ms | −15.5 |
| `diag_validate_ns` | 52.1 ms | 65.0 ms | +12.9 |
| `diag_finish_drop_ns` (the connection close) | 39.4 ms | 294.3 ms | +254.8 |
| `operation_work_ns` | 1821.0 ms | 1957.6 ms | +136.6 |
| complete command | 3130.7 ms | 3534.9 ms | +404.2 |

## 4. The diagnostics, and what they did and did not explain

`page-probe.py` priced the lever before the run and is what the missed 102 ms came from: it
reproduces the row's write shape and measures the summed `COMMIT` at four page sizes
(77,420 pages / 482.48 ms at 4096; 5,008 / 122.05 at 65536). It models the commit and nothing else.

`statement-probe.py` was written after the miss and **did not reproduce the insert regression**: it
inserted ascending ids, so every statement touched the last leaf and the wider page made the inserts
*cheaper* (4.20 -> 3.18 us/row). A diagnostic that cannot reproduce the anomaly it is sent after
refutes nothing.

`spill-probe.py` added random 24-byte keys and the product's full table set, and **does** reproduce
the direction for the first time: 4.20 -> 6.63 us/row, signature writes 16.62 -> 66.38 ms. It also
showed the page cache removing 88 % of it (6.63 -> 4.48 us/row at 8 MiB or 32 MiB), which is what
round 7 was registered on.

**The probe's model of *this* row was still wrong, and the refutation stands on the row, not on the
probe.** Round 7 removed only 15.2 ms of the 143.1 ms, and section 3's arithmetic says why: at
4096 bytes a wave's ~31 scattered row inserts land on ~31 of 343 leaf pages, and at 65536 bytes they
land on nearly all 23 — so each wave journals and copies ~1.2 MB of page images instead of ~130 KB,
about 1 GB against 104 MB over 800 waves, and the cache cannot remove a copy that the pager makes
per dirty page however resident the page is.

## 5. What is refuted, and what is not

**Refuted, on this write shape:** a 65536-byte page for a Store whose rows are inserted with
scattered keys. It buys 139 ms of commit and spends 143 ms of journal/pager image copies on the row
inserts, and it inflates the connection close and the complete command.

**Not measured, and deliberately not claimed:** any other page size, on this row or any other. The
row's own arithmetic above predicts that a *narrower* width (16384, 8192) would keep a
proportionally smaller share of the image-copy cost while still cutting the commit, but that is a
different treatment and it was **not** registered, **not** run and is **`NOT_MEASURED`**.

Checks as run for the landed change: `cargo test -p layerfs-storage` 34 binaries, 0 failed; the
whole core workspace `--no-fail-fast` 109 `test result: ok`, 0 failed; `clippy --all-targets` clean;
`fmt --check` clean; `core/tools/check_product_boundary.py` PASS. Production LOC 31376 -> 31391
(delta +15), `python3 tools/production_loc.py --root <tree>`.
