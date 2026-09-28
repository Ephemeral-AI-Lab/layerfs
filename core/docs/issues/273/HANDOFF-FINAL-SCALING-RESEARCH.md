# #273 final scaling research and architecture proposal handoff

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
>
> **Scope:** Research-first continuation of #273 / draft PR #274. This document
> hands off an experiment programme, not a predetermined minimal patch or a
> claim that a globally quadratic public WRITE has already been proved.

## 1. Resume the actual source and preserve its evidence

- Assigned branch/worktree: `codex/issue273-active-head`,
  `/Users/yifanxu/.codex/worktrees/issue273-active-head/layerfs`. Last
  product/observer/harness commit: `52878a58429a61fa40a17fc615b21f847aaacb3c`,
  tree `ce59ca331084e3b3a2c183f13bc5bc42b81200e9`. The documentation
  checkpoint preceding this proposal is `12be4b4eea10e5b19c437ed642f5692d1e130c3c`.
  Check the **actual HEAD, clean status, PR draft state, owned Docker resources
  and Cargo processes** again on resumption. Other worktrees have owners.
- Frozen #271 control product `48b51e874a41b3e1e6c6661e145316df8b408f07`
  is not to be edited. The archived first #273 campaign candidate, later
  diagnostic identities and current candidate are **three distinct sources**.
  Never promote an older receipt to the newest identity or replay an unchanged
  performance arm.
- Read [current iteration ledger](CHECKPOINT5-OPTIMIZATION-LOG.md), especially
  009–012 and the phase-local WRITE operands; [raw latest RESULT summary](../../../../benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-012/RESULTS.json)
  and its [checksum manifest](../../../../benchmark-results/fs-bench-pro/issue273/checkpoint5-optimization/iter-012/SHA256SUMS);
  [original campaign](CHECKPOINT5-LOG.md), [root-cause qualifications](CHECKPOINT5-ROOT-CAUSE-RESEARCH.md),
  [implementation authority](PHASE4.5-IMPLEMENTATION-SPEC.md),
  [evaluation/format contract](ACTIVE-FORMAT-AND-EVALUATION-v1.md), and the
  [preregistered workloads](../../../../docs/roadmap/0.1/0.1.7/issue273-checkpoint5-execution-spec.md).
  Also read the repository/Core/benchmark rules and release/documentation
  policies linked in [the preceding optimization handoff](HANDOFF-CHECKPOINT5-OPTIMIZATION.md).
- Source entry points: [index candidate/admission](../../../crates/layerfs-workspace/src/backing/active/index.rs),
  [generic splice and normalization](../../../crates/layerfs-workspace/src/backing/active/splice.rs),
  [validated hot cursor](../../../crates/layerfs-workspace/src/backing/active/hot_cursor.rs),
  [hot WRITE boundary](../../../crates/layerfs-workspace/src/backing/active/hot_path.rs),
  [page custody](../../../crates/layerfs-workspace/src/backing/active/pages.rs),
  [segments](../../../crates/layerfs-workspace/src/backing/segments.rs),
  [pack reader](../../../crates/layerfs-workspace/src/backing/active/reader.rs),
  [bounded Commit source](../../../crates/layerfs-workspace/src/commit/active_source.rs),
  [upload/extent scan](../../../crates/layerfs-workspace/src/commit/active.rs),
  and [runner](../../../benchmark/fs-bench-pro/checkpoint5_273.py). Trace
  every caller and the relevant selected-G1 and successor-G2 lifecycle.

## 2. Strong progress, with its exact qualification

| Case | Historical frozen control complete wall | First #273 candidate | Latest candidate |
| --- | ---: | ---: | ---: |
| Append 4,097 | **timeout at 25 s**, no completed phase durations | 7.033 s | **5.696 s**: Exec 4.671, Commit 0.129 |
| Dispersed 4,097 | **timeout at 25 s**, no completed phase durations | 13.283 s | **10.666 s**: Exec 9.308, Commit 0.504 |
| Repeated 4,097 | 14.518 s (INELIGIBLE) | 6.885 s | **6.013 s**: Exec 5.188, Commit 0.022 |

The named *separate* #248 4,096-WRITE physical-backing checkpoint went
from 18,751,488 B (17.88 MiB, historical #271 fixture) to a verified
1,814,528 B (1.73 MiB, phase-4.5 public stage fixture): **90.3% less,
10.33x smaller**, below the unchanged 3 MiB target. This is a scoped
physical-space result, not a matched latency ratio and not a universal space
bound. The latest matrix shows 9/9 independent byte-oracle, full callback,
cleanup and v4 four-checkpoint observer PASS. The record retains every
uncorrected failure. The repeated-100 complete wall is 1.612 s, **worse**
than its historical 1.228 s control wall, although its Exec is shorter;
the difference outside Exec/Commit is not attributed to cache or noise.

**No release or numeric matched speed claim:** latest nine rows are
`INELIGIBLE` (control not rerun under a common frozen harness; private
cache/VM/backend/device/host and phase-local cgroup proof missing).
The #248 gate remains `INCOMPLETE` for *missing explicit C1-zero provenance*,
even with valid bytes and verifiers. The clean and one-edit public SDK
retained-journal controls are `NOT_RUN`: a Store detached/reopened after
Commit is not a live/pinned private journal in one Workspace. The exact
three-sequential-Exec and complete mutations selections are also unrun.
The original baseline's 25-second timeout is a **censored wall**, never
an Exec/Commit speedup denominator. The latest raw receipt's
`charged_backing_bytes` combines a backing field and its *nested* metadata
status; do not double-count those as independent physical allocations.
Use actual backing allocation and independently checked `st_blocks * 512`
for physical-space claims, plus the separate Budget, quota and process
memory domains.

## 3. What is and is not quadratic

The shared inverse-reference patch lookup already changed from
`O(P*M)` to `O(P log M + R)` for P touched pack/payload pages, M patch
entries and R relevant references. Exact-reserving full-file extent
vectors every selected 128-entry page and directory rows on each binding
was replaced by geometric allocation with old+new capacity charged.
Do not reimplement these fixes or credit them to WRITE Exec.

The still-open **Commit locality intermediate regime** is real:
for dispersed 100/512/4,097 writes the latest grouped pack loads are
**2/7/209** (at 4,097: 5 windows, 52 chronological packs, 4,097 final
required references, 16,468 decoded records, 627 locator-index reads).
Eight times more WRITEs from 512 to 4,097 caused nearly 30 times more
loads. With a fixed 1,024-reference and 32 KiB byte window, if pack
cardinality P grows with W, cross-window visits can behave locally like
`Theta((N/K)*P)` until a bound saturates. The formal global upper bound
is `sum_i distinct_packs(window_i) <= N`; **do not** call this a proven
unbounded `Theta(W^2)` algorithm, nor call 209 loads a completed locality
optimization. In this registered large case, measured source fill was
about 72 ms of a 0.504 s Commit and a 10.666 s complete command; fixing
only that source-read span cannot halve total wall. A pack-once claim for
*arbitrary* output orders needs a valid bounded scatter representation or
an explicit impossibility/lower-bound argument under ordered SaveFile.

For WRITE, 100/512/4,096 accepted callbacks show dispersed index seeks
**1,443/7,623/61,389** (14.43/14.89/14.99 per accepted WRITE), not a
rising *per-WRITE* whole-journal scan. Repeated overwrites have about
16 seeks/WRITE. Dispersed index page writes grow from 3.73 to 6.44 per
WRITE and retirement inspections from 4.61 to 7.36 per WRITE; separate
selected height, carries, hot working-set eviction, pins and owner closure
before assigning an exponent. At WRITE 4,096, dispersed admits 3,936 hot
cursors and performs 4,330 normalizations with **only five** actual hot
WRITEs. Find out if the generic route is needlessly turning over an old
same-inode cursor or if full authentication and future eligibility require
that cost. The index candidate clones a bounded cursor set and `admit`
traces selected paths; a nested loop over at most eight cursors is not
by itself an asymptotic W-squared defect.

The 4,096-WRITE physical creation operands are append **4,096 pack +
16,672 index** pages; dispersed **4,096 pack + 26,392 index + 4,095 hot
 directory** pages; repeated **4,096 pack + 4,096 index** pages.
Append direct writes/readbacks cost about **1.750/0.964 s** and dispersed
about **2.844/1.413 s**, both inside measured Exec. Required immutable
page versions, authenticated readback, actual changed edges and canonical
construction are legitimate O(W), O(affected records) or O(required bytes)
floors. Additional index page versions or lifecycle duplication may still
be avoidable; expensive is **not** synonymous with quadratic. The current
charged per-file Commit extents and 24-byte descriptors remain `O(E_f)`
memory; this is a separate RAM-scaling limitation, not an established
`O(E_f^2)` time defect. Nine finite schedules cannot prove all permutations.

## 4. Research first, then compete on architecture—not the smallest patch

This checkout is an **experiment environment** for exploring the best
source-bound design. Read code, instrument production or externally, run
small deterministic public-route/count experiments and discard bad design
hypotheses **before** choosing a product patch. Do not rush to a wider LRU,
a workload-specific window, a cosmetic micro-optimization or a claim of
`O(log W)` total WRITE. Meaningful architecture redesign is welcome where
its representation, accounting, format compatibility, performance and
failure semantics can be proved. This research-first priority supersedes a
local *minimal-patch preference*, not custody, budget, workload or benchmark
rules. Experiments remain diagnostic, labelled `admission_eligible=false`,
and do not modify/prove frozen original receipts.

**Required first deliverable:** a prospective analysis/decision record, not
another benchmark-only report. Define W accepted WRITEs, N required final
replacement references, P distinct physical/logical packs, E_f selected
extents, D affected identities, H selected index height, A allocated pages,
G retained pins, K charged window references, B charged window bytes and
`Q_fetch` actual reads. For *each* public operation, decompose work into
mandatory input/output, changed closure, logarithmic ordered lookups,
amortized carries, hot admission/eviction, pinned owner release, page
creation/readback, C1/C2 canonical construction and observer overhead.
Derive both count and RAM/quota bounds before asserting Big-O:

- WRITE: give a recurrence or potential function for height changes, carries,
  hot-slot epochs, cursor replacement, selected/pinned page copies and
  retirement. Bound *unrelated* index/old-owner visits by logarithmic
  queries and affected closure, not by total historical W. Retain one
  immutable, atomic, physically checked acknowledgement per accepted WRITE;
  O(W) required callbacks and page I/O do not become O(log W).
- Commit: derive `sum_i distinct_packs(window_i)` **and** decoded records,
  locator seeks/index reads, required ordered replacement bytes, `O(E_f)`
  descriptor/extent capacity (including transient overlap) and the
  selected-G1 versus live-G2 identities. Prove or disprove a better
  pack-local layout or bounded multi-pass/stream algorithm without an
  unbounded out-of-order spool or secret setup phase.
- Compare at least three genuinely different designs: (A) changed-closure
  hot/generic admission and per-inode continuation, (B) physically safe
  page/owner publication and release, (C) alternative pack/index or
  SaveFile-source representation. A design may combine these. Record
  predicted **loads, seeks, page versions, bytes, RAM/charge peaks, locks,
  cleanup**, code/format complexity and failure cases against the nine
  existing cells *and* adversarial permutations around K, carry/height
  transitions, many-file sharing and retained G1/G2 pins. Design choice
  should follow the dominant measured Exec pool; do not optimize a ~72 ms
  source span and promise a 2x whole command.

For any storage-format change, write a **prospective v2/v3 reader,
migration/legacy, identity and custody specification first**, then prove
old readers/old pinned bytes or declare the compatibility boundary. A
free-slot or preallocated-page design needs final-owner release, fresh epoch,
checked allocation/refund and unknown-outcome quarantine. Never overwrite
a selected incarnation; do not change canonical identity or C1/C2
construction just to improve a timer. `<=8` resident pack pages, `<=64`
index nodes, `<=8` hot cursors, 64 hot slots, 1 MiB hot bytes, default
8 MiB Budget, single construction worker, and the registered 15/25 s
complete-command limits stay fixed unless the owner prospectively revises
a contract. No warm-page credit, implicit copy-up, after-ack work, uncharged
spool, `fsync`, deadline-free command, extra worker, process suspension or
new daemon/mount lease.

## 5. Verification and safe handoff discipline

1. Diagnose the shared cause across callers using the retained per-phase
   counters; add *real* production telemetry only when a counter is missing.
   Prespecify an oracle with terms per accepted WRITE, final reference,
   affected identity, carry/height/eviction/pin and page bytes for
   append/dispersed/repeated × 100/512/4,097. Treat a malformed/missing C1
   or interleaved sample as INCOMPLETE, never as zero. Count extended
   adversarial schedules only when justified; label them diagnostics, not
   replacements for registered workloads.
2. Commit each meaningful algorithm or format/bound change with its
   architecture document, exact first-parent/staged/committed Core,
   reference and combined production LOC from
   `python3 tools/production_loc.py --json --root <snapshot>`, external
   public Workspace/FUSE regressions, Budget/physical quota refusal,
   G1/G2/pinned bytes/epoch reuse and clean-close refund. Do not put tests
   or benchmark branches under product `src/`; keep file ceilings and
   platform gates. One Cargo owner and target directory per worktree.
3. Run affected host release tests, all-target warning-denying Clippy/fmt,
   product boundary and tools tests; build the exact aarch64 release Linux
   test and execute on an **owned ext4** volume with `TMPDIR=/work`,
   `LAYERFS_ACTIVE_TEST_ROOT=/work`, `--test-threads=1`. Reissue affected
   `stage_route.py` cases with the closed functional fixture and release
   daemon/test artifacts, preserving `performance_claim=false` and
   `cache_claim=null`. Retain every failure, raw stdout/stderr and checksums
   in a fresh `iter-NNN` folder under the existing optimization evidence.
4. Run **one prospective labelled count diagnostic** per changed case and
   identity, never another performance sample to select a better number;
   instrumented wall includes its overhead. At a frozen source, require
   all nine independent public byte oracles, source/WRITE/resource counts,
   and stage custody proofs. Compare against the *archived operand*, not an
   invented 25-second completed baseline phase. Current runner has
   `run --diagnostic` and an independently verified `run`; it has **no**
   generic `--setup clone` or `--perf-fast`. Reuse protected masters, sealed
   immutable binaries/images and independent writable byte copies, not
   mutated inputs or cached pages.
5. If seeking **numeric** control/candidate comparison, create a separate
   owned checkout for the unchanged frozen control, port only genuinely
   common harness/driver/observer changes, prove its product-source seal,
   then collect the 12 selections row-major control→candidate with common
   cache/verifier/artifact identities. The SDK retained pin and explicit
   C1-zero provenance still block two quick controls and the #248 gate;
   obtain a concrete owner ruling/capability rather than fabricating their
   receipts. Missing private cache/phase cgroup capability also remains
   INELIGIBLE, not a product speedup. Keep PR #274 draft; do not merge or
   close #273 without owner authorization.

**Stopping condition:** no known *avoidable* scaling factor remains in the
covered operations: source-derived amortized bounds and phase-local counts
explain every 3×3 trend by required work, explicit carries, selected height,
working-set eviction, pins and changed identities, with exact quota/Budget
custody and running-process continuity. If an incompatible API or external
cache capability blocks admission, retain its precise `NOT_RUN`/
`INCOMPLETE`/`INELIGIBLE` status and report the accomplished optimization
without inventing a general 2x, 10x, constant-RAM or release claim.
