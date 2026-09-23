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

## Round R10: actual seven-commit pack and pager counts

One new [count-driven diagnostic](seven-commit-pack-count-diagnostic.md) on
the isolated 64-MiB-wave algorithm recorded **7 file-Save COMMITs / 5 waves**,
**1,259 pack creations and 1,024 appends**, and 1,320 queue drains (1,190
capacity, 124 lane switch, six boundary). Pack-write calls totaled
**95.690 ms**, the disjoint SQL bucket **148.078 ms**, and COMMIT
**271.961 ms**. The actual Save connection reported **zero cache spills**
over five successful wave readings, with 82,483 SQLite page-write events
and a largest boundary cache-used reading of 8,767,488 B. These are pager
events and boundary samples, not device bytes or an exact memory peak.

The one H3 run had **0/27,503 resident source payload pages** at preflight
and immediate recheck; it remains a `DIAGNOSTIC` with metadata cache
unqualified and verifier `SKIPPED`. Its **1.429113 s** caller is not a
replacement sample for the earlier seven-commit treatment. Temporary pager
FFI was archived and removed. The exact 1,024 appends motivate a distinct
prospective pack-fit experiment, but no avoidable fraction or speed gain has
yet been measured.

## Round R11: indexed same-Save identity lookup

An isolated [matched pair](identity-index-result.md) replaced bounded linear
pending-member and current-wave sealed-row searches with hash lookups. The
count-driven control inspected **2,173,717 pending IDs** and **3,798,693
sealed IDs**; the candidate eliminated those **5,972,410 linear ID
inspections**. Its raw public caller fell **1.349030916 → 1.330246417 s**
(−18.784499 ms), but its Store gained **520,192 apparent bytes**, two packs
and **516,333 B** of pack slack. That misses the preregistered physical-space
condition, so the source remains isolated and **rejected**. The control's
telemetry ingestion was `INCOMPLETE`; both source payload checks found
0/27,503 resident pages, metadata was unqualified, and both separate full
reopened readbacks passed the same root/object IDs, 10,101 paths and 300 MB.
No arm was repeated and no physical-space threshold was rewritten.

## Round R12: exact v0.1.6 versus Core on one source

The [common-source comparison](v016-v017-common-source-results.md) reran the
unmodified v0.1.6 release product and current integrated Core once each on
independent writable copies of the **same 10k/300-MB fixture**. Both final
pre-call checks found **0/27,503 resident payload pages**; directory/inode
metadata remained unqualified. Both kept pack BLOBs inside 4-KiB-page
SQLite databases. v0.1.6's public Init took **0.750625833 s (399.667
MB/s)**; Core's took **1.380218125 s (217.357 MB/s)**, a raw **0.629592292-s**
gap. The old row is `DIAGNOSTIC`; Core is `INCOMPLETE` because daemon
telemetry lost an event. Each separate full reopened readback passed all
10,000 files and 300 MB; the first old verifier setup failed on an ID-text
newline before content access and was retained.

The source-aware micro comparison found **75 traced old COMMITs** versus
**105 exact Core C2 + two source-derived Core C5 COMMITs**. Measured COMMIT
wall was **223.015 ms** for 74 old diagnostic cohort commits versus
**226.570 ms** across all Core C2 Saves; scopes differ slightly, but neither
is near the 630-ms caller gap. Old admission sent **1,203 bounded slabs**;
Core received **34,562 object/completion messages**. The old pipeline took
**709.704 ms** and Core's file loop **1,169.934 ms**; their event/wait scopes
are not identical, so the difference is a batching hypothesis rather than
an attributed saving. Old object-row INSERT calls numbered **639**, Core
**1,414**. Read-only [dual-schema EXPLAIN](v016-core-sqlite-head2head.md)
found primary-key seeks in both, no missing hot content index, and Core's
extra Save-visibility work. The old single DB was **304,553,984 B** apparent;
Core content Store plus History was **333,737,984 B**. The next prospective
Core-only direction is bounded producer slabs while leaving payload in
SQLite and both 4-KiB pages and the 128-KiB cutoff fixed. No public arm was
repeated or promoted to fully cold admission.

## Round R13: external segment direction canceled

Before the owner's instruction that packs remain in SQLite, an isolated
schema-11 segment prototype ran one BLOB control and one segment candidate
on fresh 10k Stores. The raw callers were **1.773005209 → 1.168286875 s**,
but candidate telemetry was `INCOMPLETE`, metadata cache residency was
unqualified, and independent full readback was `NOT_RUN`. The owner then
stopped the external-pack direction. The [retained cancellation record](segment-direction-stopped.md)
keeps small receipts and sidecars; no segment source is adopted or compared
as an eligible speed arm. All further work keeps packs inside SQLite.

## Round R14: exact pack-fit buffering inside SQLite

An isolated [one-pair exact pack-fit treatment](exact-pack-fit-experiment.md)
kept the seven-COMMIT, 64-MiB-wave policy and SQLite BLOB packs. It reduced
file-Save pack appends **1,054 → 21 (−98.01%)** with seven COMMITs in each
arm, but increased the candidate Store **528,384 B apparent**, added two
packs and increased sampled Service RSS **16,891,904 B**. Both independent
full 10k/300-MB readbacks passed and source payload checks were 0/27,503
resident pages. The raw caller fell **1.517590917 → 1.482207834 s**;
however, the control had genuine concurrent LayerFS processes from another
worktree and incomplete telemetry, while candidate metadata cache remained
unqualified. No causal speed gain is claimed. The preregistered no-worse
space/RSS conditions failed, so the product source remains isolated and
unadopted. Three old visibility tests also retained a stale early-commit
premise under the 64-MiB wave; they were not repaired during this experiment.

## Round R15: Core bounded producer slabs

The [Core-only slab pair](slab-handoff-experiment.md) kept four existing Init
constructors, **one C2/SQLite owner**, 4-KiB DB pages, 128-KiB cutoff and
SQLite BLOB packs. Ordered object/file-completion events moved through a
four-slot bounded slab channel; every object still called the same C2
`SaveHandoff::accept`. The control/candidate handoff count fell **34,562 →
1,202 (−96.52%)**; raw public 10k Init fell **1.400623250 → 1.166250708 s**
(−234.372542 ms, −16.73%). The Store file and pack capacity were smaller,
exact root/object IDs matched, both full reopened 10k/300-MB readbacks passed,
and both final source checks found 0/27,503 resident payload pages.

This remains exploratory: control telemetry was `INCOMPLETE`, its
preregistered manual-build binary hash differed from the exact H3 runner
binary, and metadata residency was unqualified. The timed pair did **not**
log per-Save COMMIT/SQL counts. One separate, preregistered count diagnostic
on the slab algorithm measured **115 C2 COMMITs / 268.705 ms** (plus two
source-derived catalog writes), but its own daemon telemetry was `INCOMPLETE`
and its counts are **not** attached to either timed arm. The raw slab
candidate still took **415.625 ms** longer than the same-source v0.1.6
diagnostic. Subsequent owner direction keeps similarity-signature calculation
on the single C2 owner; producer-side offload was stopped without a sample.
At the measured slab source, Core boundary/tool tests, formatting and full
workspace tests passed; warning-denying Clippy **failed** one
`collapsible_if` style warning in `import_native.rs:200`. It was not fixed
after the timed source identity, and no release-admission claim follows.

## Round R16: Service layout and ImportBatch integration

The [Service layout report](service-layout-and-import-batch.md) records the
source move from a flat `operation/` directory into `service.rs`, `server/`,
`read/`, `save/`, and `save/import/batch/`. The import path now uses an ordered,
bounded `ImportBatch` on the existing four file constructors and one C2 owner.
Experiment-only per-file counters and `LFS237` stderr logging were removed from
the product implementation. Pack BLOBs remain in SQLite, with 4,096-byte pages
and the 128-KiB whole-file cutoff.

The prior one-shot prototype measured **1,400.623 → 1,166.251 ms** and
**34,562 → 1,202** channel receives. Its control telemetry and binary
preregistration were incomplete, and metadata cache state was unqualified.
Those figures remain pinned to the prior candidate identity; they are not
assigned to this reorganized source. Its locked Core workspace tests/doctests and
warning-denying Clippy passed, as did formatting, the product boundary guard
and six tool tests. The issue remains exploratory: channel batching did not
establish a 90% reduction in C2/SQLite transactions.

One subsequent [integrated-source diagnostic](service-layout-and-import-batch.md#integrated-source-one-10k-diagnostic)
used clean product commit `bc944fe63` and a fresh independent source copy.
The public Init was **1,110.332 ms / 270.189 MB/s**, with **0/27,503** resident
payload pages at the final check, 1.735 ms before timing. Separate reopened
readback passed for 10,000 files and 300 MB. Both SQLite files used 4,096-B
pages and the Store's small-file cutoff was 131,072 B. The public row is
**INCOMPLETE** from Service and daemon telemetry loss; metadata residency
remains unqualified and no new matched control was sampled. The raw receipt,
cold sidecars, build seal and readback are archived under
[`evidence/import-batch-integrated/`](evidence/import-batch-integrated/).

## Round R17: side-by-side post-batch gap and isolated mechanisms

The [post-batch v0.1.6 table](post-batch-v016-gap.md) records a 359.706-ms raw
public gap between exact release v0.1.6 and integrated Core ImportBatch on the
same 10k/300-MB source. The closest broad file-loop/pipeline spans differ by
225.420 ms; their boundaries are not identical, and arithmetic outside those
spans is not causal attribution. All retained 10k arms below use fresh
independent source copies and had 0/27,503 resident payload pages at final
preflight; metadata residency is unqualified.

- [Signature count diagnostic](signature-owner-diagnostic.md): 9,399 serial
  calls / 33.747 MB / 96.164 ms, zero delta trials or duplicate FULL-loss
  scans. Two exact signature variants failed to improve a pure-function
  one-pass microbenchmark. No product change selected.
- [Connection lifetime](connection-slot-experiment.md): isolated one-slot
  Store candidate raw 1,153.709→1,069.652 ms, while file Save connection
  release fell 66.443 ms→0.000458 ms. Readbacks and IDs matched. Apparent
  Store grew 507,904 B and sampled RSS 1,507,328 B; both public rows were
  telemetry `INCOMPLETE`. Candidate remains isolated/unselected.
- [Single-scope locator CTE](sql-bulk-admission-result.md): pair raw
  1,320.471→1,119.887 ms, but object-INSERT region improved only 5.519 ms
  (7.96%, below the prospective 15% gate); 149.704 ms of the raw wall
  difference tracked connection-close variation. Store grew 528,384 B.
  Treatment rejected; no root product change.
- [Paged collision lookup](collision-lookup-batching.md): pair raw
  1,092.106→1,194.798 ms and Store +778,240 B. Both readbacks passed, but
  speed and geometry gates failed; timed collision wall was not exported.
  Treatment rejected; no root product change.

No sample above is re-run or pooled to manufacture a speed median.

## Round R18: streamed transaction and pack-path feasibility

The [streamed 4-MiB-wave pair](streamed-transaction-result.md) kept canonical
pending batches bounded while one SQLite transaction spanned multiple waves.
Its file-Save COMMITs fell **91→10** (89.01%), and raw public Init fell
1,193.328→1,113.243 ms. Both separate full readbacks passed, with 0/27,503
resident source payload pages at final preflight. It missed frozen gates:
≤9 COMMITs, ≤200-ms longest lock (observed 261.890 ms), and ≤16-MiB sampled
RSS growth (observed +61.609 MiB). Store apparent grew 262,144 B. The
candidate is **not adopted**; its long lock and cross-process zero-busy
conflict remain product risks. Control telemetry was `INCOMPLETE`, candidate
telemetry PASS but metadata cache unqualified; do not treat raw wall as a
release-admitted speedup.

The [pack BLOB write review](pack-blob-write-feasibility.md) found no narrow
current-format mechanism with a credible large enough gain to justify a new
10k sample. Packs remained inside SQLite BLOBs throughout. The exact-release
v0.1.6 microstep count diagnostic follows below.

## Round R19: exact-release v0.1.6 microsteps

One [count-only release diagnostic](v016-microstep-count-diagnostic.md) on the
same 10k/300-MB manifest returned `DIAGNOSTIC`, with **0/27,503** resident
source payload pages immediately before its call. Its instrumented 768.649-ms
public wall is not a second release speed arm; the original unmodified
750.626-ms observation remains the speed reference. Temporary timers were
archived and removed, leaving the release `crates` tree byte-identical.

Old pipeline727.788ms comprised consumer callback661.613ms and blocking
receiver wait65.862ms. Separate Core ImportBatch count diagnostic file loop
951.323ms comprised C2 `accept`752.757ms and wait197.262ms. Both used about
1,200 bounded messages; their timer boundaries and identities differ. Old
four producers computed 9,399 signatures /33.747MB in96.907ms summed wall;
Core measured96.164ms for the same bytes on its single C2 owner. The
arithmetic `752.757−96.164=656.593ms` is close to old consumer661.613ms,
but is not a measured public speed saving. The old maximum producer wall
726.482ms versus Core951.226ms tracks their broad pipeline endpoints;
receiver wait/producer pacing remains unresolved. No producer-side signature
treatment was run under the earlier owner direction.

## Round R20: owner accepts the refactored 1.110-s checkpoint

On 2026-09-23 the owner selected the earlier reorganized `ImportBatch`
research source (`bc944fe63`, documented at `970854f2c`) and its one-shot
**1,110.332-ms / 270.189-MB/s** 10k observation as the stopping point.
The root `core/crates/` Git tree still has exact hash
`0d2aa55282b7bf968ccbd64fb71cc332caff5eb1`, the same as checkpoint
`970854f2c`; later commits added **documentation and evidence only**. The
accepted public row remains telemetry `INCOMPLETE`, source payload0/27,503
resident pages at final preflight, metadata cache unqualified, and separate
full reopened 10k/300-MB readback PASS. This is not release admission.

The isolated [known-length C1 candidate](known-length-native-init-stopped.md)
was built and passed focused tests but its public control/candidate pair was
**NOT_RUN**. Review found two public-API edge cases; the interrupted correction
diff and committed candidate diff were archived before restoring its
uncommitted files. Signature placement, connection reuse, CTE locator,
collision pages, streamed transactions, and pack-write ideas remain historical
research records. No further product optimization from those branches is
adopted into this worktree.
