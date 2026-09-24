# #237: releasing native entries before the C1 tree build

> **Status: Research; informative and not a product contract.** One
> prospectively frozen control/candidate release memory diagnostic on the
> exact 100k/500-MB SDK Init source. The candidate failed its whole-call
> memory decision rule and is being reverted. No speed or release admission
> claim follows from these rows.

## Treatment and custody

The candidate transferred the existing `Vec<PreparedEntry>` into
`build_namespace` and dropped it after prerequisite, inode and directory
inputs were complete, before C1 validation/build. Both public Service
callers transferred ownership. Worker count, source checks, serial order,
C1/C2/C5 format and Save boundaries were unchanged. The
[implementation spec](architecture/bounded-import-implementation-spec.md)
set an **8-MiB whole-call peak RSS reduction** as the local adoption rule.

The first [freeze](evidence/c3-entry-memory-20260924/freeze.json) named a
retained compact-Job control. Before either sample, its harness-seal mismatch
was found; the append-only [matched-pair freeze](evidence/c3-entry-memory-20260924/freeze-v2.json)
replaced that comparison. It fixed one control and one candidate under the
same harness seal `96e74ab69821c13c33649c5873003151c72d0502a7657980860686047c1614a6`.
Both used the exact seed-1 SHAKE manifest SHA-256
`23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`,
independent byte copies, fresh Store/History, locked release binaries and
0/126,206 resident source payload pages at launch. Directory/inode metadata
cache state remained unqualified. The control reused its exact archived
binary; the candidate rebuilt under its distinct product seal. No arm was
resampled. Raw closed Stores, source copies and receipts remain under
`benchmark-results/fs-bench-pro/issue237-entry-ctrlnn-20260924-01/` and
`benchmark-results/fs-bench-pro/issue237-entry-memory-20260924-01/` in
this worktree. The [curated evidence](evidence/c3-entry-memory-20260924/)
includes both receipts, phase traces, separate full oracle results,
instrument patches, arithmetic and SHA-256 manifest.

## One-shot result

| Measure | Matched control | Entry-drop candidate | Candidate minus control |
| --- | ---: | ---: | ---: |
| Scan-end process peak RSS | 76,185,600 B | 76,267,520 B | +81,920 B |
| File-loop-end process peak RSS | 106,708,992 B | 106,889,216 B | +180,224 B |
| Tree-input process peak RSS | 120,143,872 B | 120,799,232 B | +655,360 B |
| Tree-build-end process peak RSS | 142,229,504 B | 142,934,016 B | +704,512 B |
| **Whole-call peak RSS** | **145,686,528 B** | **146,440,192 B** | **+753,664 B** |
| Current RSS at return | 136,904,704 B | 137,658,368 B | +753,664 B |
| Raw public-call time, diagnostic only | 5.406411000 s | 5.387219583 s | −0.019191417 s |
| Separate reopened oracle | PASS | PASS | — |

Both release drivers made one `Client::init_project` call and reported one
new process-lifetime high-water at call return; the separate oracles passed
all 101,001 paths, portable metadata values, file sizes and SHA-256 of
500,000,000 bytes. The candidate's tree-input trace places the
14,680,064-B entry vector and 705,000 B of name lengths immediately before
its explicit `drop(entries)`. The new `entries_released` marker is followed
by C1 validation/build. The earlier entry allocation is structurally gone
from C1's live ownership, but its freed pages did not lower observed process
RSS. The phase high-waters remained close and the namespace Save still set
the maximum. These values locate intervals, not disjoint allocations.

The observed whole-call reduction was **−753,664 B**, below the frozen
**+8,388,608-B** requirement. The treatment is rejected for this memory
goal and should not be kept merely because its source diff is small. The
raw public time difference is not a speedup claim: metadata cache state is
unqualified, the benchmark selection is an unregistered release diagnostic,
and one observation is not a spread estimate. The registered #236 SDK Init
selection remains debug-only and its 100k case `NOT_RUN`.

The conditional file-job frontier does not trigger: candidate file-loop
peak was 106,889,216 B, while namespace Save reached 146,440,192 B.
Removing job state that has already drained cannot be credited with fixing
that later high-water. A full bounded fresh builder would first need the
record-custody, validation, serial-order, high-fanout and page-cache design
gates in the [architecture proposal](architecture/bounded-compact-fresh-namespace.md).

All arithmetic and source/binary identities are in the
[comparison](evidence/c3-entry-memory-20260924/comparison.json),
[control receipt](evidence/c3-entry-memory-20260924/control/receipt.json) and
[candidate receipt](evidence/c3-entry-memory-20260924/candidate/receipt.json).
