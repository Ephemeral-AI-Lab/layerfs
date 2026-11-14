# Experiment `sqlite-read`: SQLite read cost of the Store

> **Status:** Diagnostic research for issue #684, branch `exp/sqlite-read`
> from `4db7d765c6`. Not a product contract. One sample per arm; times are
> diagnostic, counts are exact. Disposable/WAL/synchronous=OFF throughout.

## Hypothesis

In round 1, SQLite was 27% of the cold read: locate 6%, pack reads 21%
without their SHA-256. Candidates: `mmap_size` and a larger `cache_size` on
read-only connections, a larger Store `page_size`, cheaper locate batching,
and one reusable buffer for blob reads.

## What the read path does today (source, `4db7d765c6`)

- **Locate** (`metadata/locations.rs`): one statement per demand page of up
  to 256 distinct ids (about 203 here):
  `SELECT … FROM object_location o JOIN pack p ON p.pack_id=o.pack_id WHERE
  o.object_id IN(?,…)`. Every id count is a new SQL text, so the 64-entry
  statement cache rarely hits. Rows are decoded into owned `Record`s (two
  `Vec<u8>` per row). A single id uses a cached typed statement.
- **Pack reads** (`objects/selection.rs`, Monolithic layout, the product
  default): one `SELECT pack_id,domain,digest,length FROM pack WHERE
  pack_id=?1`, then **incremental BLOB I/O** (`blob_open` on `pack.body`,
  `read_at_exact`): header, control prefix, then the rest into a fresh
  zero-filled `Vec`. Whole packs are then SHA-256'd. The metadata path
  (`objects/read.rs`) uses a `SELECT body` copy instead; the cold read does
  not use it.
- **Reader connection pragmas** (`session/profile.rs`): `cache_size=-2048`
  (2 MiB), `mmap_size=0`, `temp_store=2`, `page_size=4096` set at create
  and required at open. SQLite 3.53.2 is built with `DIRECT_OVERFLOW_READ`,
  so each overflow page is one `pread` of one page that bypasses the page
  cache: 500 MiB costs about 130,000 read syscalls at 4 KiB.

## Change

Commit `59274421db` (all off when `LAYERFS_PROBE_SQLITE` is unset):

- `page=<bytes>`: page size of a Store created by this process; an open then
  accepts that size.
- `mmap=<bytes>`, `cache=<KiB>`: read-only Store connections only.
- `locate=typed+bucket+split`: decode rows in place; pad the id list to a
  power of two (≥16) by repeating the last id, so a few prepared statements
  serve every demand; read `object_location` alone, then each distinct pack
  descriptor once per page instead of a join row per object.
- Stage timers `locate_sql` and `pack_descriptor`. The probe prints Store
  page size and bytes, reader SQL work (VM steps, statement phases, BLOB
  calls, bytes and ns), `/proc/self/io` read syscalls, and `RssFile`/`RssAnon`.

Binaries: `73db557fbc67fa7aeae15e3c8a54e11695858cc183f41f395b9c2694fd692a3f`
(batch 1: page/mmap/cache only) and
`b8e7c339ff7c9d6335daaee6fcc3c2723d0fd9b36aff65a32d34ff2e98379b6a`
(batches 2–5, = `59274421db`). Fixtures: one per page size, each prepared
by the first arm of its tag. Every arm uses `LAYERFS_PROBE_READ=async,noverify`
at 128 KiB / 1 in flight unless marked *sync*. Guest 1-min load 2.05–4.27.

## Store bytes per fixture

| Page size | Store bytes | Δ vs 4 KiB | Init s |
| --- | ---: | ---: | ---: |
| 4 KiB | 532,901,888 | — | 1.411 |
| 8 KiB | 534,102,016 | +1,200,128 (+0.23%) | 1.260 |
| 16 KiB | 537,722,880 | +4,820,992 (+0.90%) | 1.177 |
| 64 KiB | 570,818,560 | +37,916,672 (+7.12%) | 1.341 |

## Arms

Times in ms over the 500 MiB cold read. *Locate* is the stage (includes
waiting for the session lock); *SQL* is the locate statements alone. *Blob*
is SQLite BLOB read time. *Pack excl.* is the pack-read stage minus its
SHA-256. *Stmts / rows* are reader statements and returned rows. BLAKE3
was 13.2–13.8 µs per call in every arm.

| Batch | Arm | MB/s | Δ vs control | Locate / SQL | Pack excl. | Blob | Read syscalls | Stmts / rows | VM steps | `VmHWM` KB |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | control | 367.2 | — | 150 | 451 | 411 | 135,926 | 2,431 / 29,511 | 1,036,298 | 104,256 |
| 1 | mmap 1 GiB | 389.1 | +6.0% | 219 | 316 | 277 | 5,083 | 2,431 / 29,511 | 1,036,298 | **621,724** |
| 1 | cache 64 MiB | 373.5 | +1.7% | 151 | 435 | 397 | 135,460 | 2,431 / 29,511 | 1,036,298 | 109,920 |
| 2 | control | 370.0 | — | 155 / 145 | 452 | 415 | 135,926 | 2,431 / 29,511 | 1,036,298 | 104,260 |
| 2 | typed | 386.4 | +4.4% | 136 / 129 | 430 | 396 | 135,926 | 2,431 / 29,511 | 1,036,298 | 104,904 |
| 2 | typed+bucket | 391.9 | +5.9% | 131 / 124 | 410 | 374 | 135,926 | 2,431 / 29,511 | 1,053,794 | 102,600 |
| 2 | typed+bucket+split | 394.0 | +6.5% | 121 / 113 | 413 | 377 | 135,644 | 2,559 / 31,838 | 731,424 | 102,664 |
| 2 | 16 KiB | 373.2 | +0.9% | 181 / 171 | 383 | 344 | 43,387 | 2,429 / 29,509 | 1,036,274 | 105,172 |
| 2 | 64 KiB | 352.5 | −4.7% | 299 / 286 | 366 | 299 | 23,839 | 2,431 / 29,511 | 1,036,298 | 104,768 |
| 3 | control | 371.6 | — | 157 / 147 | 431 | 392 | 135,926 | 2,431 / 29,511 | 1,036,298 | 104,604 |
| 3 | 16 KiB, cache 64 MiB | 381.1 | +2.6% | 163 / 152 | 387 | 346 | 37,951 | 2,429 / 29,509 | 1,036,274 | 129,676 |
| 3 | 16 KiB, cache 64 MiB, split | **403.8** | **+8.7%** | 124 / 115 | 355 | 316 | 37,951 | 2,557 / 31,833 | 731,346 | 127,160 |
| 3 | 64 KiB, cache 64 MiB | 376.5 | +1.3% | 231 / 220 | 326 | 285 | 13,749 | 2,431 / 29,511 | 1,036,298 | 168,984 |
| 3 | 64 KiB, cache 64 MiB, split | 372.7 | +0.3% | 214 / 203 | 340 | 295 | 13,749 | 2,559 / 31,837 | 731,406 | 190,664 |
| 4 | control | 377.8 | — | 147 / 137 | 438 | 401 | 135,926 | 2,431 / 29,511 | 1,036,298 | 104,896 |
| 4 | 16 KiB, split | **407.2** | **+7.8%** | 131 / 123 | 364 | 329 | 39,321 | 2,557 / 31,833 | 731,346 | 102,876 |
| 4 | 16 KiB, cache 8 MiB, split | 393.5 | +4.2% | 136 / 120 | 373 | 330 | 37,954 | 2,557 / 31,833 | 731,346 | 191,712 |
| 4 | cache 8 MiB, split | 361.7 | −4.3% | 163 / 153 | 466 | 425 | 135,460 | 2,559 / 31,838 | 731,424 | 108,200 |
| 5 | control | 362.0 | — | 153 / 143 | 495 | 457 | 135,952 | 2,433 / 29,512 | 1,036,324 | 104,504 |
| 5 | 8 KiB | 375.4 | +3.7% | 149 / 140 | 436 | 398 | 71,334 | 2,428 / 29,508 | 1,036,262 | 105,088 |
| 5 | 8 KiB, split | 382.6 | +5.7% | 122 / 114 | 436 | 393 | 70,512 | 2,556 / 31,832 | 731,334 | 181,496 |
| 5 | 16 KiB, cache 16 MiB, split | 365.7 | +1.0% | 139 / 130 | 435 | 381 | 37,953 | 2,557 / 31,833 | 731,346 | 183,348 |
| 5 | *sync* control (`noverify`) | 292.8 | — | 132 / 122 | 423 | 387 | 135,906 | 2,427 / 29,511 | 1,036,242 | 114,556 |
| 5 | *sync* split | 292.1 | −0.2% | 117 / 108 | 422 | 379 | 135,640 | 2,551 / 31,829 | 729,142 | 114,324 |

Every arm exited 0 and read the 500 MiB back with the correct fold. The
Store was 0 bytes resident before every cold read except mmap (946,176
bytes: pages the reader had mapped during mount cannot be evicted).

## What worked

- **16 KiB pages cut blob read time by 60–75 ms** in each batch (415→344,
  392→346/316, 401→329, 457→381), and read syscalls from 135,926 to about
  38,000–43,000. 8 KiB gets most of that (457→398, 71,334 syscalls) for
  +0.23% storage. The remaining ~300–330 ms is close to copying 530 MB out
  of a cold page cache (`storage_read_bytes` is 532–567 MB in every arm).
- **Split + bucketed + typed locate** cuts VM steps 29% (1.04 M → 0.73 M),
  statement preparation 4.3 → 1.4 ms, row decoding 11.5 → 4.2 ms. Locate
  SQL fell 14–37 ms in four of five same-batch comparisons (b2, b3, b5 8 KiB,
  b5 sync; b4's 8 MiB-cache arm rose 16 ms). It matters most with larger
  pages, where the join re-reads big pack leaf pages through a 2 MiB cache
  (16 KiB: 171 → 115–130 ms). It costs 128 more statements and 2,327
  more rows (one descriptor row per pack per page).
- **Best combination:** 16 KiB pages + split locate at the default cache:
  +7.8% (b4) and +8.7% with a 64 MiB cache (b3), for +0.9% Store bytes.

## What did not

- **mmap (1 GiB):** blob reads −134 ms, but locate +69 ms (random faults on
  cold index and pack-leaf pages), `VmHWM` +517 MB of file-backed pages
  (`RssFile` 527,628 KB), and a mapped Store cannot be fully evicted before
  the cold read. Rejected: it trades a 6% gain for file-sized resident memory.
- **Larger `cache_size` at 4 KiB:** no effect on locate (150.9 vs 150.4 ms).
  Locate is CPU plus first-touch I/O, not re-reads. At 16 KiB a larger cache
  partly offsets the page count lost by the fixed 2 MiB, but adds 22–25 MB
  `VmHWM` per reader connection; split locate removes most of that need.
- **64 KiB pages:** fewest syscalls, but +7.1% storage, locate +77–144 ms
  (pack rows keep up to ~64 KiB locally, so leaves are big), and no speed
  gain. Rejected on both counts.
- **Reusable blob buffer:** not built. The pack-read stage minus SHA-256,
  BLOB reads and the descriptor statement is 26–40 ms in total (blob open,
  allocation, zero-fill). The body must stay an owned `Vec` in the pack
  cache, so a reusable buffer adds a copy back; the bound is about 2%.

## Risks and gaps

- **`VmHWM` jumps to 181–192 MB in 4 of 7 async split arms** (never in
  async non-split arms at ≤16 KiB, never in the sync split arm, and with
  the default 2 MiB cache in two of them). The extra is anonymous memory;
  the cause is undiagnosed. Round 2 saw the same jump once (512 KiB async).
  It looks like a timing interaction with asynchronous read-ahead, not
  memory the locate holds, but this is not shown.
- **Noise:** blob read time varies ±30 ms between batches on the same
  binary and fixture (host caching of the VM disk is not controlled). The
  4 KiB split arm with an 8 MiB cache (b4) was slower than its control, with
  both locate and blob time high; one sample cannot separate the cache
  setting from I/O noise. Counts (syscalls, VM steps, statements, rows) are
  exact.
- `page_size` is a Store format choice: product Init pins 4096 and every
  open checks it. Only a new Store can use another size; existing Stores
  stay 4 KiB. Commit/Save speed with larger pages was not measured (the
  probe's write phases do not Commit).
- Split locate turns a missing pack row for a located object into
  `Integrity` (the join silently dropped it). The foreign key makes this
  impossible in a valid Store.
- Clippy `NOT_RUN` (no clippy in the image). No unit tests were run; the
  probe's byte check is the only correctness check.

## Recommendation

1. **Adopt split + bucketed + typed locate** (no format change; fewer VM
   steps and prepares on every Store), after diagnosing the async `VmHWM`
   interaction above.
2. **Consider 8 or 16 KiB pages for new Stores** only as a modest gain:
   −60 to −75 ms of blob I/O per 500 MiB (≈4–5% of the cold read) for
   +0.23% / +0.9% storage. It needs an owner decision on the Store format
   and a Commit/Save measurement first.
3. **Do not use mmap or 64 KiB pages.** Keep `cache_size` at 2 MiB unless
   pages are larger than 4 KiB and split locate is not adopted.

SQLite reads are now about 35% of a Store demand, and their floor is the
cold copy of the bytes (~300 ms). The larger remaining levers are the pack
SHA-256 (≈218 ms) and fetching in parallel within one Workspace.
