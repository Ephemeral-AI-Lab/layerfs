# #237: 10k Init iteration record

> **Status:** Research; informative and not a product contract. Append new
> attempts without replacing old outcomes. One observation is never a median.

## Contract for this round

The measured operation is the public native `namespace-10000` Init of
**10,000 files / 300,000,000 total logical bytes**, including its 100 MB
anchor. SQLite database pages stay **4,096 B**. A prepared source workspace may
be reused for setup, while every timed arm gets a fresh Store and source data
pages invalidated and checked at zero residency immediately before its call.
No source bytes from a previous run may serve the timer. The current sidecars
do **not** independently establish cold directory/inode metadata; rows retain
the runner's uncontrolled-cache admission label. Performance exploration
skips the separate verifier, preserves failures, and never counts verification
wall in throughput. No public sample is repeated merely to pick a better time.

## Evidence carried into this round

| Attempt | Evidence and result | What it established / did not establish |
| --- | --- | --- |
| D1–D5 | [Raw attempts and preregistration](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/preregistration.md) | Debug control and small Service changes did not return a root. D5 counted the C1 sparse-run rewind; those failed rows remain visible. |
| D6–D7 | [Issue summary](README.md#what-was-measured) | The per-tier C1 gap fix first returned a 10k root, but both full verifier attempts timed out at 5 s. Debug numbers are not compared as release algorithm gains. |
| D8 | [Release build attempt](README.md#what-was-measured) | Build passed; cold preflight was stale by launch, so there was no public sample. |
| D9 | [Receipt](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d9-release-fast/daemon-host/init_namespace/namespace-10000/receipt.json) | Release public Init **1.590847 s**, root/cleanup/telemetry complete, source payload **0/27,503 resident pages** at immediate recheck, verifier SKIPPED. Admission remains diagnostic. |
| C1 direct prototype | [Pair, diff and failed gates](c1-direct-prototype.md) | One isolated pair **1.563611 → 1.317539 s** (15.7% lower); root and whole-Store equality gate failed because the runner randomized scope/stack and file-save packing varied. Four ordering-resource tests failed under the new fresh-build path. This source is not adopted. |
| D11 | [File-ingest count diagnostic](README.md#what-was-measured) | 24,562 owner accepts totaled **0.785607 s** and receiver wait **0.357453 s** inside a **1.146653 s** file child. Thread totals overlap; no algorithmic speed gain follows. |
| D12 | [Incomplete collision detail](c2-detail-diagnostic.md) | Collision queries totaled **43.306 ms**, below the preregistered 50 ms threshold for a batch-query treatment. Connection release was **350.786 ms** on this one row but not stable against D11. Daemon telemetry dropped one event; row INCOMPLETE. |
| Smaller prefix reserve | [Synthetic diagnostic](prefix-probe-experiment.md) | A 4 KiB initial reserve was **0.446 ms slower** than the existing 128 KiB reserve over the 10k size census and breached its capacity bound. No public treatment followed. |
| Simple C2 group queue | [Feasibility rejection](c2-grouping-prototype.md) | Delayed placement would hide locators needed by same-save reuse, delta bases and reads. No unsafe prototype or timed arm followed. |
| SQLite plan audit | [Version-matched EXPLAIN](sqlite-explain.md) | Hot lookups use primary-key seeks; there is no missing-index scan. Fresh-connection cache profile differs from v0.1.6, but timed-owner spill counters are missing. |

## Active experiments

| Track | Prospective difference and proof needed | Current status |
| --- | --- | --- |
| C1 direct | Fix ordering-test coverage, freeze identical stack/scope in both public arms, check exact root/object set, full reopened readback and 4 KiB Store geometry. Preserve the old failed timing pair. | In progress in isolated `issue237-c1-direct-prototype` worktree; no new result recorded here yet. |
| C2 bounded admission | Design dependency-aware group publication with bounded encoded bytes and locator rows; test same-save reuse, delta bases, reads, abort/commit and #229 space before interpreting any speed. | In progress in isolated `issue237-c2-admission` worktree; no candidate result recorded here yet. |
| Timed Save pager | Measure cache use, misses, writes and spills on the actual file-Save connection. Only if pressure is observed, compare a 32 MiB/spill policy to the default in a separate cold-source pair, reporting RSS and Store space. | In progress in isolated `issue237-pager` worktree; no pager result recorded here yet. |

The [v0.1.6/Core comparison](architecture-v016-v017.md) and
[518.8 MB/s budget](target-518.md) explain why the old 578 ms row is not a
cold-source baseline. The fastest separate C1 candidate still needs
**0.739294 s** less public time to reach that number. Current work does not
establish that the target is attainable; any new receipt, failure or rejected
hypothesis will be appended with its exact identity and causal evidence.

## Round R2: C1 parity and timed Save pager

- **C1 H1, fixed identities:** one control at `a6d1d563` returned in
  **1.615643334 s** and one direct-build candidate at `dc654bb06` in
  **1.381173500 s** (raw difference −0.234469834 s). Both public commands
  used the same frozen stack and scope, fresh Stores, and independently found
  **0/27,503 resident source payload pages** immediately before their timers.
  Both existing full verifiers passed 10,101 paths and SHA-256 of 300 MB after
  reopening. Exact root and all 24,683 object IDs matched; whole-Store
  apparent bytes and pack capacity matched, with 4,096-B database pages.
  The control's daemon lost one telemetry event, making its receipt
  **INCOMPLETE**; the candidate remains `INELIGIBLE` under the official
  uncontrolled-cache label. The result proves the stated C1 parity/readback
  on this fixture, not a fully cold #231 admission or #229 sparse-history
  compactness. The repaired full `layerfs-content` suite passed. Source and
  tests were adopted into this research worktree in `738157ede`; see
  [full H1 evidence](c1-fixed-identity.md).
- **Pager D13 attempt A:** copied fixture directory mtimes changed during
  setup, so the harness stopped with `NOT_RUN`, **zero timed samples**. It was
  retained. After repairing only copied setup metadata, attempt B made one
  fixed-identity public call in **1.530461375 s** with a fresh 4-KiB-page
  Store and **0/27,503 resident source payload pages** on both checks.
  Verification was SKIPPED; telemetry/cleanup passed. The *actual timed file
  Save connection* used 2,000 cache pages and recorded **718,990 hits,
  3,838 misses, 94,513 page-write events and zero spills** across 79 commits.
  Per its preregistered rule, the proposed 32-MiB/spill-OFF policy pair was
  **rejected without a treatment sample**. Source instrumentation was restored;
  the one D13 time is not compared as an algorithm change. See
  [pager report and both attempts](pager-10k.md).

C1 adoption remains limited to this research branch. The separate C2
dependency-aware admission candidate has one control/candidate timing pair
under analysis; its physical Store/readback outcome will be appended before
any adoption decision. Source metadata cache remains unqualified for every
reported run, and no prior receipt is promoted or rewritten.

## Round R3: bounded C2 admission and integrated pair

- **Isolated C2 pair:** one control at `d98cca6fe` returned in
  **1.597006084 s** and one bounded-group candidate at product source
  `60ced47d1` in **1.468407708 s** (raw −0.128598376 s, −8.05%). Both
  used the same fixed stack/scope and fixture manifest, fresh 4-KiB-page
  Stores and independently found **0/27,503 resident source payload pages**
  immediately before timing. Exact roots and the complete object-ID digest
  matched; both full reopened verifiers passed 10,101 paths and 300 MB.
  The candidate Store file and pack capacity fell by **249,856 B** and
  **262,144 B**, with allocated bytes unchanged. Its closed Store had nine
  more physical groups but one fewer pack; actual pack-write and SQL INSERT
  call counts were **NOT_MEASURED**. The control's daemon lost one telemetry
  event (`INCOMPLETE`); the candidate retained the runner's uncontrolled-cache
  `INELIGIBLE` label. No #229 sparse-history proof followed from this dense
  row. Source/tests/architecture were adopted into this research branch at
  `a5f484b14`; see [C2 report](c2-admission-experiment.md).
- **Integrated C1 versus C1+C2:** preregistered [pair](combined-c1-c2.md)
  used this worktree's C1-only control source `181973312` and combined
  candidate source `a5f484b14`, each once with fixed public IDs and fresh
  Stores. Caller times were **1.310979458 → 1.252324750 s**, a raw
  **0.058654708 s / 4.474%** reduction. Both source rechecks were
  **0/27,503 resident payload pages**, exact root and all 24,683 object IDs
  matched, full reopened verifiers passed, and SQLite pages were 4,096 B.
  Both daemon telemetry ingests were **INCOMPLETE**, so there is no admission
  PASS or selected replacement sample. Candidate apparent Store bytes were
  **16,384 B lower** and pack capacity equal, but declared pack used bytes
  were **160 B higher**; the preregistered strict no-worse condition on each
  space field therefore missed despite a smaller Store file. #229 remains
  unrun at this combined identity. The best raw public observation is now
  **1.252324750 s (239.554 MB/s)**, still **0.674080 s** above the
  historical 518.8-MB/s time; its metadata cache state is unqualified.

The next prospective product-policy experiment tests an 8-MiB rather than
4-MiB C2 wave/transaction byte cap on a separate branch, retaining the
512-object cap and 4-KiB SQLite pages. Its outcome is not recorded yet. A
separate fresh sparse-history control/candidate guard is also in preparation
because neither dense 10k Store establishes #229 compactness.

## Round R4: policy tradeoff and cache preconditioning limit

- **8-MiB wave policy:** one isolated control/candidate pair used identical
  count-only Service instrumentation. File-Save commits fell **80 → 55**;
  pack appends **1,170 → 1,102**; object INSERT statements **1,464 →
  1,435**. The raw public call fell **1.281196166 → 1.234139583 s**,
  while Store apparent bytes rose **782,336 B**, pack capacity **786,432 B**
  and sampled Service RSS **11,927,552 B**. Both payload rechecks found
  **0/27,503 resident pages** and page size stayed 4 KiB. Candidate source
  path reused the earlier prepared workspace, so directory/inode metadata
  could have remained warm from the control; the 47.057-ms raw difference is
  **not a validated cold causal gain**. Control telemetry was INCOMPLETE,
  verifier SKIPPED, full readback and #229 NOT_RUN. The policy remains
  isolated, with the diagnostic hook restored; see [wave8 report](wave8-experiment.md).
- **Cutoff proposal cancelled:** a proposed 128→512 KiB whole-file cutoff
  change was stopped at the owner's explicit direction to keep **128 KiB**.
  No product edit or timed arm of that policy was kept. Work pivoted to a
  more aggressive C2 transaction design, under a separate preregistration.
- **Nonportable OS purge proposal retired:** `/usr/sbin/purge` failed with
  `Operation not permitted`; noninteractive sudo required a password. The
  owner then required OS-host-agnostic product changes. The purge flag was
  removed from the research driver; no public sample was involved. The
  driver can make an independent writable byte copy per arm before full
  payload rehash/eviction,
  preventing source-file cache reuse from a previous arm. It cannot clear or
  independently qualify metadata warmed by that arm's own setup on this
  host. Future such timings remain exploratory and the official cache label
  stays uncontrolled. See the [H2 failure and portable H3 correction](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/preregistration.md#h3-portable-independent-source-copy-h2-purge-retired).
- **Sparse-history guard:** one fresh control and one C2 candidate both
  stopped `INCOMPLETE` on `Integrity("dependency encoded work")` before a
  state root. Each partial 4-KiB-page Store had 1,556 objects, 25 packs,
  6,742,016 B apparent/allocated and 4,875,831 B pack slack; all published
  objects reopened/authenticated with equal canonical digest. This is only
  partial-store parity; the complete #229 sparse-history space/readback gate
  remains **OPEN**. See [retained failure report](sparse-c2-guard.md).

## Round R5: primary transaction-count hypothesis

The owner's new priority is reducing the roughly **80 file-Save SQLite
commits by at least 90%**, to **8 or fewer**, while keeping the 128-KiB file
cutoff and 4-KiB SQLite pages fixed. Source review rejected the first
Save-wide-transaction sketch: it would release Store arbitration while an
SQLite write transaction remained open, making other writers fail at the
zero busy timeout. No code or timed arm of that sketch was committed.

The prospective experiment instead enlarges one **bounded preparation
unit** to roughly 50–64 MiB and at least 4,096 objects, then seals the
remaining pack lanes and publishes the result in one final bounded
transaction. Every transaction commits before arbitration is released; the
final transaction must flush queued groups and validate collision candidates
before publication. The C2 queued-group buffer stays separately bounded at
512 locator rows.
This may trade much higher peak memory and longer writer lock holds for
fewer commits; rollback and multiwriter behavior must be tested and any
failure retained. The product change must be OS-host agnostic. The pair
will use fresh independent source byte copies and zero-resident payload
checks, but remains exploratory because metadata warmed by setup is not
qualified. No timed transaction treatment or result has been taken yet;
do not infer a wall-time gain from the 227-ms historical COMMIT bucket.

## Round R6: seven commits, slower Init

The [bounded-wave pair](bounded-wave-experiment.md) used one instrumented
control `c5d9e8af3` and one candidate `343e4e029`, with the same H3 driver,
fixed operation IDs, fresh independent writable source copies and fresh
4-KiB-page Stores. Both immediate checks found **0/27,503 resident source
payload pages**; metadata warmed by setup was unqualified. The control's
daemon telemetry lost an event (`INCOMPLETE`); the candidate is `DIAGNOSTIC`.
Both public runs skipped verification and were sampled once.

- The candidate enlarged bounded preparation to **8,192 objects / 64 MiB**
  and coalesced final seals/publication. File-Save commits fell **80 → 7**
  (**−91.25%**), with preparation waves **75 → 5**. This met the owner's
  transaction-count target without changing the 128-KiB file cutoff or
  SQLite page size.
- Public Init became **1.364419666 → 1.439506625 s** (**+75.086959 ms**,
  slower). File-Save COMMIT time rose **228.068 → 276.719 ms**; the largest
  accounted transaction rose **8.51 → 134.19 MB**, longest arbitration hold
  **22.28 → 267.89 ms**, and sampled Service RSS **59.97 → 247.10 MB**.
  The candidate Store file grew **1,069,056 B** and pack slack **1,032,165 B**.
  Current-owner accept/SQL breakdown and actual-owner pager spill counters
  were `NOT_MEASURED`, so the cause of the extra COMMIT and finish time is not
  established beyond its correlation with larger transactions.
- Separate reopened full-manifest verification passed both Stores: identical
  root/object-ID digest, 10,101 paths and all 300 MB. Its wall time did not
  enter the public comparison. The candidate stayed isolated and was **not
  adopted** as a speed optimization. The first Save-wide-transaction sketch
  was rejected before build/sample because it would release Store arbitration
  with a live SQLite write transaction; its [record](single-transaction-experiment.md)
  and diff are retained.

The candidate's **208.4 MB/s** raw rate remains far below the historical
**518.8 MB/s / 0.578245 s** row, which itself lacked a cold-cache contract.
Reducing transaction count by 90% was insufficient on this 10k route.

## Round R7: external pack segment feasibility

A [schema-11 bundle design](segment-store-feasibility.md) examined moving
immutable pack bytes outside SQLite while retaining 4-KiB database pages and
the 128-KiB file cutoff. The proposed motivation is to keep payload writes
out of the large SQLite MEMORY-journal transactions, **not** a measured speed
result. A naive per-Save append file fails runtime rollback: the current open
pack tail can cross committed waves, so a later failed append could overwrite
pack header/directory bytes already referenced by an earlier committed row.
Safe external storage needs per-transaction immutable segments, new pack
locators/read paths, abort and unknown-COMMIT handling, and bundle-aware
copy/backup/space verification. This is an explicit Store format migration.
No product edit, build or timed sample was made; a public pair is **NOT_RUN**.

## Round R8: current integrated source hot-path diagnostic

One [count-driven diagnostic](integrated-hot-profile.md) at the integrated
4-MiB-wave C1+C2 source recorded a **1.352215375 s** public operation,
**0/27,503 resident payload pages** just before it, and telemetry/cleanup
PASS. It is `DIAGNOSTIC`: metadata residency was unqualified and full
verification was `SKIPPED`. File Save made **80 commits**; aggregate COMMIT
time was **232.460 ms**, SQL **139.616 ms**, and FULL encoding **69.896 ms**.
The receiver spent **725.295 ms** accepting 24,562 objects and **441.699 ms**
waiting for producers. Four workers had **1.800 s aggregate non-send
construction** and **2.701 s aggregate blocked/object-send** time; sums
overlap and are not caller contributions to add.

The bounded queue drained **1,347** times: **1,153 byte-cap**, 118 lane
switch, 75 wave end and one row cap. Its queued encoded bytes imply a
**1,147-drain** minimum at 256 KiB, so removing lane-switch flushes alone
has limited scope. The seven-commit 64-MiB candidate is a *different*
source identity; these queue-cause counts are not its matched baseline.
About 260 ms of owner work remains outside the seven disjoint Save profile
buckets, and connection release cost **62.64 ms** inside the public call.
Those are localization targets, not attributed savings. The diagnostic's
temporary instrumentation was archived and removed from product source.

## Round R9: per-lane queue feasibility stopped before timing

An isolated [per-lane queue proposal](per-lane-pack-experiment.md) attempted to
remove incidental lane-switch flushes while preserving the seven-commit
64-MiB wave policy. It was preregistered but **no public control/candidate
pair ran**. Its synthetic first check failed an overstrong zero-append
assertion: two final open lane tails still append after the preparation wave.
The corrected focused check passed 5/5; other focused targets and 10k readback
were `NOT_RUN`. The adjacent R8 **4-MiB** profile's 118 lane-switch drains
among 1,347 suggested low leverage, but is not a measured ceiling on the
**64-MiB** source or a failure of the planned 50% matched append gate. The
prototype diff and first failure are retained; active product in the isolated
worktree was restored to its measured seven-commit baseline. A distinct
[exact pack-fit proposal](wave-wide-pack-proposal.md) remains unbuilt and
untimed; it needs current seven-commit append/flush counts before selection.
