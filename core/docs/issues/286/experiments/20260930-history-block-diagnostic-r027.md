# #286 r027: complete per-state block diagnostic

> **Status: complete count-driven diagnostic, no candidate sample.** The
> official stride1 gate remains r021's **FAIL**: C2+C5 allocated 84,086,784 B
> against strict `<83,947,520 B` (excess 139,264 B). No separate verifier or
> numeric timing admission was claimed for this diagnostic.

Source commit `5a8676624`, binary SHA-256
`8f52b440238a548d117716759adc28794b580404b7421172ba85ceaf1bf1b661`
from the benchmark manifest's own `target/release/`, fixed v3 corpus/root pins,
157 states, one construction worker, original 170 s command limit. The labelled
`LAYERFS_HISTORY_BLOCK_DIAGNOSTIC=1` run exited zero in **165.179 s** and emitted
all **628** expected C2/C5 × allocated/apparent counters (157 complete states).
It did not run the separate verifier and its time is cache-uncontrolled.

| State | C2 apparent B | C2 allocated B | allocated−apparent B | C5 allocated B |
| ---: | ---: | ---: | ---: | ---: |
| 145 | 66,973,696 | 67,112,960 | 139,264 | 196,608 |
| 146 | 67,190,784 | 83,890,176 | 16,699,392 | 196,608 |
| 157 | 75,712,512 | 83,890,176 | 8,177,664 | 196,608 |

Between states 145 and 146 C2 apparent size grew **217,088 B**, but allocated
blocks rose by exactly **16,777,216 B**. C2 allocated blocks then stayed fixed
through state 157 while apparent size grew another 8,521,728 B. This is direct
evidence of a 16-MiB physical-allocation step, not proof of which kernel or VFS
heuristic caused it. The closed C2 and C5 bytes have SHA-256
`ef1fba79a76add4c781c9bbcf1c5219a9548d3cb03b4483a8d84ded9059ca781`
and `8dcf12b01322afcaf9e1a758b05abbe68e1d3216c0a37eda9033090f9af93833`:
**both are byte-identical to the original r021 files**, with identical final
allocated/apparent readings. That calibrates the diagnostic on the actual
level9 population and explains why the r025 smaller encoded Store could use
more APFS blocks.

SQLite documents that a VFS may preallocate physical space when given a size
hint, and Apple documents that preallocation can reserve unwritten space
([SQLite file-control opcodes](https://www.sqlite.org/c3ref/c_fcntl_busyhandler.html),
[Apple preallocation](https://developer.apple.com/documentation/fskit/fsvolume/preallocateoperations)).
This trace alone does **not** establish that SQLite supplied such a hint or
that APFS chose a particular policy. A separate same-volume synthetic probe
showed `F_PUNCHHOLE` beyond logical EOF did not reduce a deliberately
preallocated file's blocks, while a one-byte grow/shrink did; neither probe
is a qualifying Store treatment. Direct raw truncation behind an open SQLite
connection is not selected as a product fix. The next source investigation is
SQLite's bounded incremental-vacuum path: determine whether a page can be
reclaimed as part of an ordinary committed save, with no file-size spool and
no private raw-file mutation, then make one prospective product change only
if the ownership and failure semantics remain sound.

[The SHA-indexed raw declaration, complete trace, original Store/History and
logs](20260930-history-block-diagnostic-r027.json) remain append-only in the
local worktree. R026's wrong-binary INCOMPLETE diagnostic remains separate.
Families 3–7 are NOT_RUN; the nine family-3 IDs have been registered without
sampling. No threshold, timeout, cache method or prior result changed.
