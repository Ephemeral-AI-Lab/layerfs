# #237 — native Init scaling research

> **Status:** Research; informative and not a product contract.

This work is isolated in
`/Users/yifanxu/.codex/worktrees/2776/layerfs` on
`codex/issue237-init-research`, based on `main` at `7df25f979`. Product edits
on this branch are **unmerged research prototypes**. They do not close #237 or
the four-tier #231 gate. The only fixed database page size used here is 4 KiB.
The direct C1 build and bounded C2 group admission now live in this research
tree; neither has been merged to `main` or release-qualified. The
[iteration record](iteration-record.md) lists their separate and integrated
results, failed receipts and open gates.

The methods and prospective differences are in the
[preregistration](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/preregistration.md).
Each invoked case/source identity has one public-operation sample at most, a
fresh output directory, and retained failures. The research
[cold driver](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py)
uses the existing Darwin invalidate/mincore backend. The original Core runner
still marks these rows `source-cache-uncontrolled-v1` and
`admission_eligible=false`; research sidecars do not relabel them as gate PASS.
The sidecars prove zero resident **payload** pages just before the call. Their
file opens and metadata checks can leave directory-entry and inode metadata
resident; that cache is not independently evicted or qualified. Accordingly,
these are source-payload-cold diagnostics, not a fully cold namespace claim.
Verification is separate from every performance number. D9 and subsequent
algorithm exploration use the performance-only fast lane by default.

## What was measured

All times below are **one raw observation**, not a median. The 10k source is
10,000 files, 100 data directories and **300,000,000 total logical bytes**,
including its one 100 MB anchor. D1–D7 used debug binaries. Command walls
for D1–D6 include the research cold preflight, so they
are not comparable to the frozen complete-command budget. D7 moved that
preflight outside the command. D9's command includes an extra 0.373 s
nonfaulting residency recheck, which makes its command number conservative.

| Attempt | Change / source | Public Init | Result and proof |
| --- | --- | ---: | --- |
| D0 | Initial research driver import error | no sample | Failed before fixture or product process; [ledger](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/preregistration.md#attempt-ledger). |
| [D1 control](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d1-control/daemon-host/init_namespace/namespace-10000/receipt.json) | `e6e528c3`, original Core import | 14.021 s | No root; verifier NOT_RUN; FAIL. |
| [D1 metadata](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d1-metadata/daemon-host/init_namespace/namespace-10000/receipt.json) | `561aaf940`, one-entry portable-metadata memo | 13.004 s | No root; verifier NOT_RUN; FAIL. |
| [D3 counts](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d3-phasecounts/daemon-host/init_namespace/namespace-10000/receipt.json) | Instrumented metadata prototype, dirty diagnostic | 13.011 s | No root; source milestones retained in stderr. |
| [D4 reopen](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d4-native-reopen/daemon-host/init_namespace/namespace-10000/receipt.json) | `d8282da2`, native-only duplicate `FileView` removal | 12.111 s | No root; source tree recorded dirty from a concurrent research note. |
| [D5 reducer counts](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d5-reducer-counts/daemon-host/init_namespace/namespace-10000/receipt.json) | Count-driven C1 diagnostic, dirty source | 11.048 s | No root; 13,869,215 run-row reads by 5,120 remaining values. |
| [D6 gap](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d6-gap/daemon-host/init_namespace/namespace-10000/receipt.json) | `0615c5f4`, per-tier proven-absence interval | **7.362 s** | Confirmed C5 root, 9.318 s command including 1.301 s cold preflight, cleanup/telemetry PASS; full verifier **TIMEOUT** at 5 s, row INCOMPLETE. |
| [D7 oracle](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d7-oracle/daemon-host/init_namespace/namespace-10000/receipt.json) | `858624cbc`, verifier metadata memo + stronger cold preflight | 7.456 s | Root and cleanup/telemetry PASS; 7.511 s command; full verifier still **TIMEOUT**, row INCOMPLETE. |
| [D8 release build](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d8-release-stale/daemon-host/init_namespace/namespace-10000/receipt.json) | `59a85bea`, `--release` build | no sample | Build PASS in 17.544 s; cold preflight became stale during startup. Retained NOT_RUN. |
| [D9 release fast lane](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d9-release-fast/daemon-host/init_namespace/namespace-10000/receipt.json) | `2a66f84d`, release binaries, immediate whole-source mincore recheck | **1.591 s** | Confirmed C5 root, 2.018 s command, telemetry/cleanup PASS; verifier **SKIPPED**, diagnostic only. |
| [D10 100k research](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d10-release-100k/daemon-host/init_namespace/namespace-100000/receipt.json) | `a6d1d563`, one-shot release driver around the official hard skip | 10.011 s | Daemon returned `Unknown` with no confirmed root; Service later reported 11.021 s success and one LayerStack. Command 15.395 s, telemetry INCOMPLETE, cleanup FAIL; verifier NOT_RUN. No further 100k work is planned in this 10k-focused round. |
| [D11 ingest counts](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d11-ingest-counts/daemon-host/init_namespace/namespace-10000/receipt.json) | Dirty, temporary release instrumentation, then restored | 1.493 s | Root and telemetry/cleanup PASS, verifier SKIPPED; count-driven diagnostic only, no time comparison. |
| [D12 collision detail](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d12-collision-detail/daemon-host/init_namespace/namespace-10000/receipt.json) | Dirty, reporting-only release instrumentation, then restored | 1.875 s | Root returned, source payload 0/27,503 resident pages, verifier SKIPPED; daemon dropped one telemetry event, row **INCOMPLETE**. |
| [C1 fixed-identity proof](c1-fixed-identity.md) | Direct fresh-build C1, one control/candidate pair | 1.616 → **1.381 s** | Same root/object IDs/Store capacity; both full verifiers PASS. Control telemetry INCOMPLETE; cache admission unqualified. |
| [D13 live pager](pager-10k.md) | Reporting-only actual Save connection diagnostic | 1.530 s | 0 cache spills; proposed 32 MiB policy pair rejected. Prior setup attempt retained NOT_RUN. |
| [C2 bounded admission](c2-admission-experiment.md) | One fixed-identity control/candidate pair | 1.597 → **1.468 s** | Same root/readback; candidate Store smaller. Control telemetry INCOMPLETE; cache admission unqualified. |
| [Integrated C1+C2](combined-c1-c2.md) | One C1-only control, one combined candidate | 1.311 → **1.252 s** | Best raw rate **239.554 MB/s**; same root/readback, both telemetry INCOMPLETE, metadata cache unqualified; pack used +160 B despite smaller Store. |
| [8-MiB wave policy](wave8-experiment.md) | One isolated 4→8 MiB C2 wave pair | 1.281 → **1.234 s** | Commits 80→55, but metadata cache unqualified; Store capacity and sampled RSS increased. No validated cold gain or adoption. |
| [Sparse C2 guard](sparse-c2-guard.md) | Separate fresh history control/candidate | no completed history sample | Both stopped on `Integrity("dependency encoded work")` before a state root. Equal partial Store geometry and authenticated objects do not close #229. |
| [Bounded transaction waves](bounded-wave-experiment.md) | One instrumented 10k control/candidate pair | 1.364 → **1.440 s** | File-Save commits **80→7** (−91.25%), but candidate was slower with higher sampled RSS and a larger Store; both full reopened readbacks passed. Control telemetry INCOMPLETE and metadata cache unqualified. |
| [External pack segments](segment-direction-stopped.md) | Isolated format feasibility; owner stopped it | no eligible result | An isolated pair ran before the instruction to keep packs in SQLite; candidate telemetry was INCOMPLETE and full readback NOT_RUN. No segment source was adopted. |
| [Integrated hot-path profile](integrated-hot-profile.md) | One current C1+C2 count diagnostic | 1.352 s | 80 file-Save commits; 232 ms COMMIT, 140 ms SQL, 725 ms owner accept and 442 ms receiver wait. Queue drains were mostly byte-bound. Metadata cache unqualified; verifier SKIPPED. |
| [Per-lane queue feasibility](per-lane-pack-experiment.md) | Untimed source/count review and synthetic check | no public pair | Adjacent 4-MiB diagnostic found only 118 lane-switch drains among 1,347; 64-MiB matched append and speed gates remain NOT_RUN. [Exact pack-fit follow-up](wave-wide-pack-proposal.md) is unbuilt. |
| [Seven-commit pack/pager counts](seven-commit-pack-count-diagnostic.md) | One 64-MiB-wave count diagnostic | 1.429 s diagnostic | 1,024 pack appends, 1,190 capacity-driven drains and **zero actual-owner cache spills**; 0 resident payload pages, metadata unqualified, verifier SKIPPED. Not a speed arm. |
| [Exact pack-fit buffering](exact-pack-fit-experiment.md) | One isolated SQLite BLOB control/candidate pair | 1.518 → 1.482 s raw | Pack appends 1,054→21 with 7 COMMITs both; Store +528,384 B, sampled RSS +16.9 MB, control externally interfered/telemetry INCOMPLETE. Both full readbacks PASS; source not adopted. |
| [Same-Save identity index](identity-index-result.md) | One matched integrated C1+C2 control/candidate pair | 1.349 → **1.330 s** | Removed 5.97M linear ID inspections, but Store +520,192 B and two packs; rejected by preregistered space gate. Both full readbacks PASS; control telemetry INCOMPLETE and metadata cache unqualified. |
| [v0.1.6 versus Core, same source](v016-v017-common-source-results.md) | One exact-release reference and one integrated Core 10k diagnostic | **0.751 vs 1.380 s** | Same 300 MB bytes, zero resident payload pages, both full readbacks PASS. Core telemetry INCOMPLETE; metadata cache unqualified. [SQLite plans and counts](v016-core-sqlite-head2head.md) show nearly equal measured COMMIT wall but 1,203 old slabs versus 34,562 Core messages. |
| [Core bounded producer slabs](slab-handoff-experiment.md) | One Core control/candidate pair | **1.401 → 1.166 s** raw | Receive events 34,562→1,202 with one C2/SQLite owner and SQLite BLOB packs. Exact root/IDs and full readbacks PASS, Store smaller; control telemetry and binary prereg incomplete, metadata cache unqualified. Separate slab count diagnostic: 115 C2 commits, not timed-arm counts. |

A separately preregistered [C1 direct-build prototype](c1-direct-prototype.md)
used one release 10k control/candidate pair in its own worktree: **1.564 →
1.318 s** public Init (**191.9 → 227.7 MB/s**, 15.7% less time). Each arm's
source payload had zero resident pages at preflight and immediate recheck. The
runner randomized stack/scope identities, so the preregistered exact-root and
whole-Store equality gate **failed**; the candidate also left four existing
ordering-resource assertions failing. Its source remains unmerged. Neither
arm qualifies as a fully cold or independently verified PASS.

D9 reused exactly sealed release binaries from D8. Its source was rehashed and
invalidated outside the command; both the first check and the immediately
preceding nonfaulting check found **0 resident pages out of 27,503** across all
10,000 files/300 MB. The recheck finished 1.061 ms before the caller timer.
The release file-construction/Store handoff span was 1.250 s of the 1.591 s
operation. The Store remained at SQLite `page_size=4096` and occupied
334,184,448 B on disk; its 1,263 pack rows reserved 331,350,016 B and
declared 305,977,888 B used. These are [raw Store geometry](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d9-release-fast/store_geometry.json),
not a matched compactness PASS.

The historical **518.8 decimal MB/s** comparison is **0.578245 s** for this
300-MB numerator. The stretch target is **700 decimal MB/s**:
300,000,000 B / 700,000,000 B/s = **0.428571 s**. D9 reached 188.58 MB/s.
Its file construction/Store span alone was 1.249614 s, and everything else in
the caller consumed 0.341234 s. Keeping that other work fixed would require
the file span to fall to 0.087338 s, a 93.0% cut. A file-only change cannot
credibly clear the target without also reducing C1/other work. This is a
bound from one observed row, not a prediction that the target is impossible.

D11 split a separate 1.147 s file span into **0.786 s** of single-owner C2
`accept` calls and **0.357 s** of receiver wait. The four producers accumulated
**2.880 s of channel send time** and **4.319 s of file-construction wall**;
these are overlapping per-thread totals and must not be added to the receiver
wall. The completed save inserted 24,364 objects, reused 198, made 1,257
packs/6,439 appends, issued 7,750 object-row statements and 79 commits.
Its disjoint profile charged 0.192 s to SQL and 0.227 s to transaction cadence.
The 24,364 file-save object IDs exactly match D9's, although physical pack
placement varies slightly with producer scheduling. This localizes serial C2
work; it does not establish a treatment speedup. The temporary
[instrumentation diff](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/d11-instrumentation.diff.gz)
is retained and absent from the current product tree.

## Improvements and failed ideas

The shared C1 reducer's `RunStore::find` repeatedly restarted a sparse newer
run while looking for ascending serials held by an older run. D5 measured
9,011,202 run-row reads just after directory effects and 13,869,215 by the
5,120-value milestone. A source-derived model reproduces those exact cumulative
counts after merge reads and predicts 18,484,823 lookup row returns for the
full 10k sequence, versus 33,331 with one proven-empty interval per tier.
The same model predicts 1,375,401,339 versus 353,971 at 100k; **the 100k
counts are not runtime measurements**. The bounded C1 prototype holds one
interval per live tier, changes no Store format, and its external sparse-tier
regression passes. D6 is the first 10k public operation here to return a root
inside the unchanged request deadline. See [C1 analysis](c1-scaling.md).

The adjacent metadata memo and native-only duplicate file-root read removal
each reduced redundant work in source, but their attempts **did not return a
root**. They are retained as failed treatments rather than selected speedups.
The proposed one-pack C2 group queue stopped at a source-level feasibility
check: delaying placement also delays locators required by same-save exact
reuse, delta selection and reads. No prototype or timed arm was run for that
idea; see [the failed C2 approach](c2-grouping-prototype.md).
The proposed smaller per-file prefix reserve also failed its standalone
[10k allocation diagnostic](prefix-probe-experiment.md): a 4 KiB start took
7.006 ms versus 6.560 ms for the existing 128 KiB reserve, added 7,506
reallocations, and grew capacity to 256 KiB at the default cutoff. No public
Init arm or product edit followed that result.
The [D12 C2 detail](c2-detail-diagnostic.md) charged 43.306 ms to collision
queries across the file Save, below the preregistered 50 ms threshold for a
batched-query treatment. That candidate was rejected without a product edit.
The same incomplete row charged 350.786 ms to SQLite connection release;
D11's entire finish call was 54.223 ms, so this is variability to diagnose,
not an established saving.
The verifier's one-entry metadata memo kept the full path/metadata/content
oracle but **did not** get the debug build under its fixed 5 s watchdog.
Moving cold preflight before startup initially made its one-second freshness
window expire: D8 took zero samples. The immediate nonfaulting recheck in D9
resolved that acquisition problem, without warming source bytes. The debug to
release time change is a **build-profile difference**, not an algorithm speedup
factor; the original runner's debug build contradicted the frozen #231 release
build specification. See [file ingest](file-ingest.md) and
[verifier analysis](verifier.md).

## Open qualification work

- The 10k row still lacks a passing full independent readback at its final
  performance identity. D9's `SKIPPED` proof is explicit; D6/D7's 5 s timeouts
  remain failures. Explore performance separately from verifier repair, as the
  updated `AGENTS.md` guidance says.
- The Core first-pass runner deliberately leaves `namespace-100000` `NOT_RUN`.
  Its exact 100,000-file/500 MB source, cold contract, complete command, resource
  scopes and full oracle need a prospective runner version and one fresh sample.
  The [100k route handoff](100k-route.md) identifies every hard skip. D10's
  one-shot research driver did not change that registry or prove readback. The
  2.7 s historical cold target remains a target, not a waiver or PASS.
- Process CPU, sampled RSS, source-page residency, scratch/spool and Store bytes
  are separate domains. The retained RSS samples have no complete phase
  coverage, and no cgroup anonymous/file split exists for this host route.
  Neither a lifetime high-water mark nor a compact heap proves a memory gate.
- A dense Init Store cannot prove the [#229 sparse-pack lane](sparse-pack.md).
  The available archived/current stride1 Stores differ in advisory depth and
  physical representation; the alleged matched baseline cannot be reconstructed
  from those named files. A new same-policy sparse-history control and treatment
  need full reopened readback and physical pack utilization checks before any
  compactness claim.
- The [v0.1.6 comparison](v016-comparison.md) corrects a repeated numerator
  error: its 100 MB anchor is inside 300 MB. The 578.245 ms reference row is
  518.8 MB/s, not 691.8 MB/s. Faster historical rows were dirty and cache
  served. Legacy, #219 pipeline and Core native Init have different routes,
  content bytes, timers and cache identities; none is a matched speed pair.

## Three-squad 518.8 MB/s investigation

- The [v0.1.6/Core architecture comparison](architecture-v016-v017.md)
  separates the old dirty, zero-disk-read 578 ms row from the cold-source Core
  route. Bounded producer/admission overlap and direct initial namespace
  construction are transferable ideas; its time is not a cold baseline.
- The [complexity map](complexity-10k.md) records the removed C1 sparse-run
  blowup, the remaining 30,302 reducer entries and modeled 83,551 ordering
  writes, and the necessary `Ω(bytes + files + objects)` work. Batching can
  reduce call count and latency, not make a full import constant time.
- The [version-matched SQLite EXPLAIN audit](sqlite-explain.md) finds primary-key
  seeks on the hot object and pack queries, with no missing-index scan. Core's
  observed fresh-connection default is about 8 MiB page cache versus the
  reference's explicit 32 MiB, but the timed Save's spill counters were not
  captured. A larger same-operation cache is a hypothesis to test after an
  actual-owner pager diagnostic, not an established speedup.

The [518.8 MB/s budget](target-518.md) predated the integrated C1+C2 pair.
From its best raw **1.252325 s** observation, the remaining gap is
**0.674080 s**. D11/D12 charged about **0.2275 s** to file-Save transaction
cadence at older source identities, so even removing that entire bucket would
not recover the historical time. The [bounded-wave experiment](bounded-wave-experiment.md)
cut file-Save commits by **91.25%** but made the matched public caller
**75.087 ms slower** and raised sampled RSS and Store space. This treatment is
not adopted. The 4 KiB database page, 128 KiB whole-file cutoff, fresh timed
work and no-warm-source rule remain fixed.
The C1/C2 source changes remain confined to this research branch; the complete
#229 sparse-history space/readback gate remains open after both attempted arms
stopped at the same product error.
