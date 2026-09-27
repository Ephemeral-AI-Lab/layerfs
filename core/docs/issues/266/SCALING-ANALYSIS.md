# Issue 266: separated-write time and space analysis

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Three read-only subagents independently audited (1) the extent B+ tree and
ownership ledger, (2) payload/space and Commit, and (3) FUSE/Exec timing.
Their source findings were reconciled with the retained
[fixed 512 receipt](evidence/treatment-512/receipt.json), the
[failed 4,097 receipt](evidence/gate-4097-fail/receipt.json), and one separately
[predeclared 1,024 count diagnostic](../../../../docs/roadmap/0.1/0.1.7/issue266-1024-count-diagnostic-spec.md).
The latter used the same public mount → one Exec → one Commit route, generic
one-fd writer, old-head master and independent oracle; its product seal
`8d93f981a486ea33b2618c37b439db76e7abd5bd25e23ba674ac7f23fc45e10e`
matches the fixed 512 row. It changed only harness selection and optional
snapshot interval. One attempt at source `a8f3514c4` produced 1,024 actual
FUSE WRITE callbacks, exact old/new heads and bytes, 1,024 changed runs,
2,048 lower pieces, verifier PASS and positive daemon close. Exec was
6.497280 s, Commit 0.154572 s and complete command 8.172595 s. These
times remain cache-`INELIGIBLE`; they are diagnostic observations, not a
qualified 512→1,024 speed comparison. The [receipt](evidence/scaling-1024/receipt.json)
pins image, release binary, master, clone, source and harness identities.

## Measured work by count

The table combines cumulative *counter* checkpoints from the fixed 512
selection (128-write spacing) and the distinct 1,024 selection (256-write
spacing). Both use the same product seal and old fixture. `backing_status()`
reads the ledger counters; it does not issue ledger-page I/O. Its optional
status snapshot does scan retained payload records in memory, so elapsed
times across different snapshot intervals are not matched performance arms.

| Accepted WRITEs | Source row | Ledger 4 KiB reads | Ledger 4 KiB writes | Metadata-page reads | Live metadata pages | Live ledger pages |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 128 | 512 | 4,198 | 2,301 | 952 | 8 | 3 |
| 256 | 512 and 1,024 | 9,601 | 5,402 | 2,104 | 10 | 5 |
| 384 | 512 | 15,540 | 9,039 | 3,256 | 12 | 7 |
| 512 | 512 and 1,024 | 22,015 | 13,212 | 4,408 | 14 | 9 |
| 768 | 1,024 | 36,573 | 23,166 | 6,712 | 18 | 13 |
| 1,024 | 1,024 | 53,275 | 35,264 | 9,016 | 22 | 17 |

At 512, successive 128-write blocks made **4,198, 5,403, 5,939, 6,475**
ledger reads and **2,301, 3,101, 3,637, 4,173** writes. After the first
branch transition, each later block added exactly **536 reads and 536 writes**
over its predecessor. At 1,024, the 256-write blocks made **9,601, 12,414,
14,558, 16,702** reads and **5,402, 7,810, 9,954, 12,098** writes; later
blocks increased by **2,144 reads and 2,144 writes** each. Metadata-page reads
after startup stayed at **9 per WRITE**, rather than rising with the file's
extent count. At observed multiples of 128, with `k = accepted_writes / 128`,
the ledger counters exactly fit `reads = 268k² + 4599k − 669` and
`writes = 268k² + 2297k − 264`. This is an **empirical identity at six
observed checkpoints**, not a proved formula for every count or a projection
of 4,097 work. The 512 ledger API moved 137.61 MiB and the 1,024 row moved
345.86 MiB for 512 and 1,024 supplied bytes respectively. These are 4 KiB
ledger API transfers, **not measured physical-device bytes**.

## Source-backed time model

Let `N` be accepted distinct one-byte writes, `E ≈ 2N` final extents, `L`
the occupied extent leaves and `H` tree height. A leaf holds at most 124
records; a branch holds at most 248 child references. The
[splice](../../../crates/layerfs-workspace/src/backing/binary_plus_tree/extent/splice.rs)
descends to the touched leaf and shares untouched subtrees, but rebuilding
its root branch walks its child references. New page publication
[enumerates and advances every child ownership edge](../../../crates/layerfs-workspace/src/backing/ownership.rs),
and routine [old-root cleanup](../../../crates/layerfs-workspace/src/backing/metadata_reclaim.rs)
decrements those edges. The authenticated
[ledger batch](../../../crates/layerfs-workspace/src/backing/ownership/ledger_batch.rs)
reads and writes direct 4 KiB pages for contiguous owner-reference runs.

While one root branch grows with roughly `L ≈ 2N/124` children, that edge
work grows with `N` per WRITE and can accumulate like `N²` over this finite
range. This is a specific explanation for the exact convex ledger counts.
It is **not** a global `O(N²)` claim: node fanout is capped at 248 and height
eventually grows, giving a bounded-page `O(H·F)` edge envelope per update
with `F ≤ 248`. The ideal packing lower bound at 4,097 is 67 leaves, which
could fit one 248-child branch; actual occupancy and height were not
exported, so a height transition or fragmentation at that failed count is
not ruled out. The historical whole-registry-per-write reclaim scan is
absent; the 1,024 row counted zero registry lookup scans.

The fixed 512 writer's four 128-write intervals took **573, 652, 672,
748 ms**. The separate 1,024 writer's four 256-write intervals took
**1,281, 1,470, 1,729, 2,004 ms**. Within the 1,024 Exec, recorded payload
acquisition was 2.895586 s and Workspace publication 3.547489 s, together
6.443074 s of the 6.497280 s public Exec. The remaining 54.206 ms includes
admission, kernel/FUSE transport, writer and timing gaps; it is not a direct
measurement of reply-wait cost or a bound at 4,097. The analogous recorded
share at 512 was 2.625175 of 2.659247 s. The FUSE reply-wait check itself
has no count-dependent scan. Both acquisition and publication interval costs
rise, so neither is safely treated as a constant per WRITE. Optional status
snapshots add `O(N)` in-memory record scans at only four checkpoints; their
cost and uncontrolled cache prevent a qualified latency exponent.

## Space and Commit

Each accepted one-byte WRITE creates one private segment with a 4 KiB header
and a 4 KiB padded data page: one file, two aligned writes and **8,192 live
payload bytes** while these separated changes remain referenced. At 512,
512 retained payloads account for 4,194,304 B, plus 94,208 B of metadata
(14 live metadata pages and 9 ledger pages), totaling **4,288,512 B**.
At 1,024, 1,024 payloads account for 8,388,608 B, plus 159,744 B metadata
(22 live metadata pages and 17 ledger pages), totaling **8,548,352 B**.
Live private space is therefore growing approximately linearly in these
snapshots, even as cumulative ledger I/O grows convexly. This is allocated
backing, not process heap, cgroup page cache or a phase-local memory peak.
If all 4,097 writes succeeded and remained distinct, the private payloads
alone would require at least 33,562,624 B and 4,097 files; no such final
allocation was observed on the timed-out gate.

Commit traverses the frozen final extent/replacement view and reads its
owned payloads; its cursors do not replay 512 or 1,024 mutation calls. The
512 and 1,024 rows lowered 1,024/2,048 pieces and 512/1,024 changed runs,
with 512/1,024 replacement bytes. Commit took 0.088/0.155 s respectively;
the Store file ended at 897,024 B in both attempts from the same 618,496 B
master. C1 exported zero small-file nodes read/created in these rows; that
counter does not price all Store SQL work. The 4,097 attempt never reached
Commit, so C1 or Store Commit work cannot explain its observed pre-Commit
timeout.

## Meaning of the 4,097 failure

A literal linear multiplication of the raw 512 Exec time by `4097/512`
already gives about **21.28 s** before complete-command overhead, above a
20-second expectation. It is only arithmetic: the 512 row had post-reply
snapshots and uncontrolled cache, while the 4,097 gate image omitted those
snapshots. The actual gate stopped its host driver at **25.006537 s**, and the
orphaned daemon later logged a **29.108651 s failed WorkspaceExec**. Its
driver emitted no receipt, callback count, progress, Commit or verifier
result; shutdown retained a dirty Workspace. It proves neither a precise
scaling exponent nor that ownership I/O alone caused the timeout. It also
does not show another EBUSY refusal. The public Exec's 30-second product
deadline belongs to #249, and the #265 child owns payload, extent-index and
C1 optimizations. The measured ledger curve makes growing ownership work a
strong, source-supported target for #265, without crossing its source
ownership boundary here. The 4,097 FAIL remains unchanged and unrerun.
