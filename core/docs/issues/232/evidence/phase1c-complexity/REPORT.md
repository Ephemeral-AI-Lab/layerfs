# #232 Phase 1C count screen: nine retained attempts

**Disposition:** one attempt for each of the nine frozen diagnostic cases.
Eight public SDK Exec→mounted ioctl→Commit routes completed with independent
verifier and cleanup PASS; each remains **INELIGIBLE** for cold latency. The
128-edit case is **FAIL**: its caller's Exec returned `Unknown` before Commit,
Unmount returned `Io`, and the verifier was `NOT_RUN`. The raw attempt and
all 128 logged piece splices remain retained. No Phase 1C product
algorithm change is justified by these receipts, and no failed or unchanged
arm was resampled.

The [pre-run manifest](PRE_RUN.json) froze the
[nine-row registry](../../../../../benchmark/fs-bench-pro/registry/workspace-exec-complexity-v1.json)
at SHA-256 `670973c95d747828fe2db02c82323a4df4f585bc3d9ed011e2c378159a731cad`,
execution source `66c702a08fd916f08a1839bbd1bc8256a4e42903`, product seal
`a71196edd7797f19f90beafa0c81201b7593465c644859ec06337a7b13327ea0`,
harness seal `bce60d62bc9fd528d152157707d2113d8d4ea3b601af98c68876b4c9eae83c24`
and image `sha256:571e34b0b2e439eddd5a1931d17fbcf5e8891485ec5f0fdcac1c1737459fc298`.
That image sets `LAYERFS_COMPLEXITY_DIAGNOSTIC=1` in the Linux daemon;
the host Service had the same diagnostic flag and
`LAYERFS_CONSTRUCTION_WORKERS=1`. Binary, payload and six prepared-master
hashes, exact commands, clone outputs and cutoff policy are in `PRE_RUN.json`.
Every case used an independent writable byte copy of its validated, closed
master. Setup and verification were outside the LFT1 operation timer.

## Retained status and timing

These are **diagnostic** LFT1 numbers, not eligible cold performance results.
The complete command includes Sandbox and container lifecycle plus cleanup;
the verifier is separate. Times were not repeated for stability or selection.

| Case | LFT1 Exec→Commit | LFT1 Exec | LFT1 Commit | Complete command | Verifier | Status |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| repeated-1 | 40.538 ms | 22.038 ms | 18.494 ms | 2.623 s | 0.598 s PASS | INELIGIBLE |
| repeated-32 | 796.946 ms | 733.326 ms | 63.612 ms | 1.742 s | 0.022 s PASS | INELIGIBLE |
| repeated-128 | 5,033.572 ms failed root | 5,033.565 ms | absent | 10.950 s | NOT_RUN | **FAIL** |
| locality-1m | 42.277 ms | 22.268 ms | 20.004 ms | 0.903 s | 0.022 s PASS | INELIGIBLE |
| locality-100m | 59.843 ms | 29.800 ms | 30.037 ms | 0.903 s | 0.620 s PASS | INELIGIBLE |
| locality-capped500m | 84.555 ms | 26.729 ms | 57.822 ms | 0.909 s | 3.073 s PASS | INELIGIBLE |
| cutoff-below | 43.054 ms | 22.995 ms | 20.050 ms | 0.916 s | 0.015 s PASS | INELIGIBLE |
| cutoff-at | 41.236 ms | 21.388 ms | 19.842 ms | 0.929 s | 0.018 s PASS | INELIGIBLE |
| cutoff-above | 48.004 ms | 22.426 ms | 25.573 ms | 0.909 s | 0.016 s PASS | INELIGIBLE |

All complete commands were under 15 s. All eight executed independent
verifiers were under 10 s and checked full result digest, head/root shape,
mode/mtime, and retained old Commit and reopened Branch; the three cutoff
verifiers additionally checked the old full-file digest against the sealed
fixture recipe. The canonical root is **observed**, not compared to a
predeclared exact result root for these new diagnostics; content digest,
representation and historical identity are independent checks. Each completed
row had the exact expected STATE/EDIT callback counts, 4,096 accepted bytes,
zero shifted suffix bytes, Unmount PASS and public Sandbox Delete PASS.
All nine Store copies passed the macOS pre-sample residency check. The Linux
FUSE backing cache remained uncheckable and could credit Commit from Edit's
resident writes, hence no latency PASS or comparison to v3/v4 gates.

The [derivation script](derive.py) verifies each copied file's `SHA256SUMS`,
the frozen identity, LFT1 caller root, registered callback/piece cardinality,
verifier, cleanup and cache status. [derived.json](derived.json) retains every
piece count, per-edit visit/page count, C1 count, sampled resource window and
post-run Store observation. The nine [attempt folders](attempt-01/) contain
raw caller/daemon/Service LFT1 and count lines, driver/verifier outputs and
receipts. Store/history SQLite bytes and the private cursor key stay at the
ignored local run paths named in each `POST_RUN.json`; their hashes are
retained. Post-run page counts are Store-wide after verification, not phase
read counts.

## Repeated-piece result

The frozen 1/32/128 sequences all offered **4,096 total** replacement bytes.
Each successful ioctl inserted a distinct slice with a checked stamp, one
revision and bounded readback before the next request. The daemon emitted
`LFS_PIECE_COUNT` and `LFS_PIECE_PAGES` once per requested edit, including
128 lines in the failed attempt. Only the two completed cases have a public
post-Commit status and verifier proof.

| Count | Final logged pieces | Sum of splice loop visits | Piece-index pages written | Commit lowering visits | Outcome |
| ---: | ---: | ---: | ---: | ---: | --- |
| 1 | 3 | 7 | 1 | 3 | verified |
| 32 | 65 | 3,200 | 46 | 65 | verified |
| 128 | 257 | 49,664 | 487 | absent | no Commit/verification |

Visits and piece-index writes grow superlinearly with edit count, as the
source's two old-piece passes and full piece-index rebuild predict. The
128-edit row reached its 128th logged splice, then the caller returned
`Unknown` at about 5.033 s with no typed Exec result. The daemon's LFT1
`WorkspaceExec` continued to 7.116 s; no final tool acknowledgement was
retained. The native bridge's fixed `IO_PROGRESS_MS=5_000` requires progress
on the control response even though the public SDK Exec budget is 30 s.
The caller made no Commit, public Status was unavailable, Unmount returned
`Io`, and Sandbox Delete succeeded. This is a real route failure for this
registered count diagnostic. No deadline was inflated and the case was not
retried.

The LFT1 Exec child includes process launch, 2R STATE requests, R EDIT
requests, bounded readbacks and daemon/Service work. There is no LFT1 child
isolating the piece-vector scan within it. Thus the superlinear visit count
does **not** establish that piece scanning has a material wall share or that
a tree would resolve the fixed bridge progress failure. The piece-index
replacement remains **open with this measured reason**. A future distinct
diagnostic would need to isolate its share before a product algorithm change;
the current 56-case #232 registry contains one semantic edit per case.

## C1 locality and cutoff

Each locality row was one 4 KiB middle overwrite, with zero FUSE write
callbacks and only bounded readback callbacks, independent of suffix length.
The successful C1 `FastCdc::scan` checked exactly
4,096 input bytes in each file. Its existing `EditCounters` reported:

| Pristine size | C1 nodes read | Mapping nodes created | Payload objects created | Final extents | Piece visits/pages |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 MiB | 6 | 1 | 1 | 56 | 7 / 1 |
| 100 MiB | 25 | 5 | 1 | 5,396 | 7 / 1 |
| capped 500 MiB | 27 | 7 | 1 | 26,996 | 7 / 1 |

The touched-node counts grow with the larger extent-tree paths and boundaries,
while CDC input and emitted payload count stay fixed. None of these counts is
proportional to untouched file bytes or suffix length. C1 locality is
**bounded and accepted for these three shapes**; no C1 or CDC rewrite is
justified. These counts do not resolve Phase 1B's separate C2 batch-drain
growth.

The default construction cutoff is exclusive 131,072 bytes. The below/at/above
rows preserved exact result bytes, observed canonical roots, portable
mode/mtime, historical old bytes and Branch; the verifier reported WholeFile
at 131,071 bytes and Chunked at 131,072 and 131,073. Source inspection of
the WholeFile branch shows a bounded 131,071-byte canonical reconstruction
for `cutoff-below`; its `EditCounters` are zero because that branch does not
use the Chunked extent editor. The at/above rows scanned 4,096 CDC bytes,
read six C1 nodes and created one mapping node and one payload object each.
Caller LFT1 sampled maximum RSS was 28,786,688 / 31,440,896 / 32,342,016
bytes across the three rows. First and last resource samples did not enclose
the operation boundaries, so these are **sampled process windows, not exact
phase peaks**. The cutoff transition is **bounded and accepted** on this
diagnostic; no representation change is justified.

## Phase 1C exit

Repeated pieces: **open** because 128 edits failed before Commit and scan
wall share is not isolated. C1 locality: **bounded and accepted** in the
observed 1/100/capped-500 MiB single edits. WholeFile/cutoff: **bounded and
accepted** at the three exact edges. Unmodified suffix-moving editors remain
a separate POSIX workflow; these cases used the opt-in ioctl carrier. C2
batch drain remains open under the [Phase 1B result](../../../241/evidence/phase1b-finish-diagnostic/REPORT.md).
No C3 algorithm commit or C4 changed-source proof is triggered. The #232
Phase 2 rollout must retain this failed diagnostic and independently qualify
each of its 56 registered single-edit cases; these count rows are not
performance admission evidence.
