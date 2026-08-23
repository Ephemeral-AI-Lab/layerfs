# G5-D cold, SQLite, and locality attribution

Disposition: **`RETAIN_CURRENT_SQLITE_PROFILE`**.

## What is known

The retained profile remains SQLite `FULL`, `DELETE`, `temp_store=FILE`, `mmap=0`, and the existing spill control. H11 sets `cache_size=1500` for history construction and again immediately after reopen for head/range/reconstruction/edit/materialization; the open itself uses the retained default before that caller-side setting. H11 directly observes SQL calls/rows, row-BLOB operations, canonical bytes authenticated, logical/apparent/allocated store bytes, child CPU/RSS, and logical Q within the scoped instrumentation. Post-open current-root SQL/BLOB work stayed exact across N=1/10/100/1,000 except the intentionally separated genesis versus non-genesis first-edit authority class.

H11 does not observe VFS `xRead/xWrite`, byte-level main/journal I/O, sync-call wall, stable-media state, continuous allocation peak, SQLite run/page locality, or OS cache residency. Its timed reopen interval starts before `Store::open_measured`; preflight/open/schema/profile queries are not all charged to `Metrics`, and `cache_size=1500` is set only after the open returns. Thus the reported reopen 3-query/8-BLOB counters are incomplete for that wall and the interval is not an exact 1500-page profile row. Reopen, a fresh process, `F_NOCACHE`, a slower wall, RSS/Q, pager counts, and allocated bytes cannot substitute for cold/physical facts. `physical_io_bytes`, `continuous_storage_peak`, and `controlled_cold` therefore remain `Unavailable`.

## Transferable Xet locality accounting

Xet's defragmentation hysteresis suggests counters, not carriers: mapping/chunk objects and BLOB rows per returned range; distinct SQLite pages/runs touched; contiguous references per leaf/run; history age of referenced objects; allocated bytes per live canonical byte; rolling chunks/bytes per physical run; and high/low fragmentation thresholds. Such heuristics must remain noncanonical and must never change roots or semantic results.

## Candidate order

1. Retain the profile and add read-only accounting or a benchmark-private SQLite VFS that directly records main/journal read/write calls and bytes, syncs, page numbers, and run transitions.
2. Only after direct attribution, design a controlled-cold protocol with an independently demonstrated cache-state mechanism or a disposable environment whose storage/cache state is externally controlled.
3. If fragmentation is material, test a noncanonical co-location/compaction heuristic with identical logical roots and crash-safe data-first/index-removal-before-delete ordering.
4. Consider page/cache/BLOB layout changes only if direct counters identify them as the bottleneck and all authority/durability/resource controls remain exact.

Xorbs, shards, global dedup indexes/caches, compression, network concurrency, and remote defragmentation machinery are rejected: they solve carrier/network problems not established in the local SQLite core.
