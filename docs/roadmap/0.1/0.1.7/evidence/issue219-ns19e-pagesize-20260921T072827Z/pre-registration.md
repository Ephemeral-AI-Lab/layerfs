# Pre-registration — #219 round 6: the Store's page size

Written **before** the first measured run of this arm, and before any product edit.
Parent: `565f95366` (the handoff), tree clean apart from this evidence directory.

## The one difference

**`PRAGMA page_size = 65536` applied when the Store is created, and nothing else.**

The product never set `page_size`; a Store was created at SQLite's 4096-byte default
(`PRAGMA page_size` on the row's own store, `ns19-D4b-formula-20260921T071758Z/pipeline-namespace-10000/sample.sqlite`:
4096, `page_count` 82,129, `freelist_count` 0, `cache_size` -2000). `page_size` only
takes effect before the first table exists, so this is a Store-creation decision. One
pragma, one value, one call site; no other pragma, no capacity, no cadence, no policy
constant, no schema text, no table shape moves with it.

## The identity this arm is compared against

| field | value |
| --- | --- |
| row | `benchmark-results/issue219/ns19-D4b-formula-20260921T071758Z` (`e11984c16`, dirty) |
| `phases.admission.operation_ns` | 1,864,922,917 ns |
| `pipeline.operation_work_ns` | 1,820,957,583 ns |
| `pipeline.diag_commit_total_ns` | 402,724,715 ns |
| `pipeline.diag_write_pack_total_ns` | 231,034,537 ns |
| `pipeline.pack_bytes_written` | 302,406,480 |
| `pipeline.commits` | 800 |
| store pages (row artifact) | 82,129 |

## What the probe says the page size is worth — the count-driven instrument

`page-probe.py` (this directory, run once; output `page-probe.out`) reproduces the row's
write shape with no product code: 800 transactions, 16,800 blob appends of 18,000 bytes
into 256 KiB packs through `blobopen`, 24,800 `WITHOUT ROWID` row inserts, the product's
own pragma profile. Same bytes, four page sizes:

| page_size | pages | commit (800) | µs/page | blob-open+write | close |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 4096 (today) | 77,420 | 482.48 ms | 6.23 | 50.26 ms | 292.21 ms |
| 16384 | 19,470 | 167.31 ms | 8.59 | 35.55 ms | 269.78 ms |
| **65536** | **5,008** | **122.05 ms** | 24.37 | 34.69 ms | 379.66 ms |

`commit` is the `COMMIT` statement alone, summed over the 800 transactions — the same
region `pipeline.diag_commit_total_ns` charges. The cost is **per dirty page, not per
byte**: 15.5x fewer pages is 3.95x less time at 64 KiB, and 3.95x fewer pages (16 KiB) is
2.88x less time. The remaining 24.37 µs/page at 64 KiB is 64 KiB of `pwrite` inside the
OS page cache — memcpy speed, which is the floor.

**Predicted movement, in the instrument's own units**, applying the probe's ratio to the
row's own counts:

| instrument | today | predicted | derivation |
| --- | ---: | ---: | --- |
| `pipeline.diag_commit_total_ns` | 402.7 ms | **102 ms** (accept 90–150) | 402.7 x 122.05/482.48 |
| `pipeline.diag_write_pack_total_ns` | 231.0 ms | **~200 ms** | 231.0 x 34.69/50.26 on the append share |
| store pages | 82,129 | **~5,200** | 82,129 x 5008/77420 |

`operation_work_ns` is predicted at **1450–1580 ms**. That movement is larger than the
~250 ms this machine drifts in 13 minutes, but **the claim is carried by
`diag_commit_total_ns` and the store's page count** — a duration whose cause is a count —
and not by the row total. One sample per arm; no confirmation run.

## What would refute it

1. `pipeline.diag_commit_total_ns` >= 300 ms (the per-page mechanism is not what the
   probe says it is, or the product's dirty set is not the pack pages the probe models).
2. `pipeline.commits` != 800, or `pipeline.pack_bytes_written` != 302,406,480, or
   `pipeline.packs_created` != 1268 — the change is not work-neutral.
3. `digest:filesystem_root` != `1d6fba29aba105aefbb668c34b026f8b89d20773fc36d548b991034dc3857847`,
   or any of the 13 gates fails.
4. The store's `page_count` does not fall to ~5,200.
5. Any read of an existing 4096-byte Store fails, or `cas_reuse` / `delta_payload` /
   `pack_watermark` / `multi_writer` / `visibility` / `persistence_failure` / `pack_locator`
   is not green.

## Both directions of the read path

`page_size` is a property of the **file header**, not of the schema or the pack framing:
`page_count = file_size / page_size` and every page number in the file is read against it.
The value is **not** validated, refused or asserted anywhere in the product — `Pragma::PageSize`
is documented "read for evidence only" (`sqlite/connection.rs`) and `schema::validate` never
looks at it — so this change is version-compatible in both directions and `SCHEMA_VERSION`
stays 9:

- a **4096-byte Store** created by the previous build still opens, validates and reads: its
  page size is read from its own header, and no code path compares it to a constant;
- a **65536-byte Store** created by this build is refused by nobody: the schema identity,
  the six table shapes, the four indexes and the persisted policy are unchanged, and the
  pack framing is untouched.

A version bump would be a label, not a refusal. The version number exists to make an
*unreadable* Store fail at open; nothing becomes unreadable here. One test does pin the
default (`tests/connection_profile.rs:112` asserts the store's page size is 4096); it is
updated to the declared constant, and that is a test change, not a format change.

## Recorded, not claimed

- The probe's own `close` column (292 -> 380 ms at 64 KiB) is **outside** the row's formula
  and is not claimed as a movement. It is reported because it is the same page mechanism
  and a reader should see that it does not follow the commit's ratio.
- `cache_size` is **not** part of this treatment. The probe's two `cache_size` rows
  (`-8000`, `-32768` at 64 KiB) move `commit` by 6.9 ms and 5.8 ms, inside the probe's own
  resolution, so the campaign's earlier `cache_size` refutation stands at this page size
  and there is nothing here for a later round to register on that instrument.
- `pipeline.commits` is pinned at 800. This change does not touch the cadence.
