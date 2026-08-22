# 2026-08-21 Phase 4 full grind roadmap

Status: **G4 STAGE TERMINAL PASS under the user-approved 1-ms absolute-regression materiality rule; v12 remains SEALED TERMINAL REVISE under its frozen relative-only contract**

V12 is sealed and must not be reanalyzed or rerun. It passed source/static
closure (166 passed, 1 ignored, 0 failed), exact work, direct <=1-MiB buffer
evidence, resources, native durability, residue, custody, and independent
ledger agreement. The unchanged <=5% adjacent gate failed only at seq17
(100-MiB clone no-op, +8.535%), seq20 (1-MiB count change, +6.800%), and seq26
(1-MiB before-publication fault, +14.360%). The old gate did not pass. Their
absolute mean deltas are only +0.226229 ms, +0.285522 ms, and +0.099604 ms,
below the controlling 1.000-ms absolute-regression threshold, while all hard
absolute and mandatory semantic/resource/evidence gates pass. Three fresh
independent read-only audit lanes reconciled to PASS with no source/evidence
P0/P1. G4 has a separate stage-level terminal PASS. Phase 4 remains incomplete
and no production or platform integration is accepted.

This task stops before and authorizes no G5 implementation or measurement.
Concurrent
premature `research/phase-4/g5-round-0` planning is foreign to and excluded
from G4 custody; its presence does not make it an accepted roadmap start.

This is the current execution roadmap for the remainder of Phase 4. It starts
from the retained G1 checkpoint and supersedes older ordering in research
notes that still identifies CP-0009, Canonical-v2, or FastCDC work as next.
Historical reports remain evidence, not the current control.

The roadmap organizes work; it does not itself promote an experiment, change
a profile or format, authorize 500-MiB work, start WP5, or declare Phase 4
complete.

## 1. Current position

```text
G0  FastCDC-v2 checkpoint              COMPLETE  286eb7a
G1  SQLite writer-memory policy        COMPLETE  d79f0e0
G2  materialization decomposition      COMPLETE
G3  incremental prototype              PASS      sealed v13
G4  materialization acceptance         PASS      stage terminal; v12 remains REVISE
G5  remaining core lanes               PENDING   no implementation/measurement in this task
G6  Phase-4 closure                     PENDING
```

The accepted runtime is Canonical-v2 plus the exact-boundary FastCDC
contiguous-region kernel and `PRAGMA cache_spill=2000`.

| Current 100-MiB operation | Retained result | Qualification |
|---|---:|---|
| Durable fresh create | **279.463 ms / 357.829 MiB/s** | accepted G4-v12 evidence |
| Writer maximum RSS | **12.48 MiB** | G1; 86.005% below its control |
| SQLite cache snapshot maximum | **8.35 MiB** | G1; 89.944% below its control |
| Same-open same-count edit | **8.043 ms** | accepted G4-v12 evidence |
| Same-open `+1` early / middle | **5.108 / 4.576 ms** | latest Canonical-v2 lifecycle evidence |
| One-byte early / middle / late | **6.410 / 6.415 / 6.725 ms** | latest Canonical-v2 guards |
| Warm authenticated reconstruction | **237.214 ms / 421.560 MiB/s** | accepted G4-v12 evidence |
| Fresh-process reconstruction | **237.381 ms / 421.263 MiB/s** | accepted G4-v12 evidence; OS cache warm-or-unknown |
| Reopen / visible head | **3.583 ms** | accepted G4-v12 evidence |
| First edit after reopen | **154.019 ms** | full authority work remains |
| Authenticated returned 1-MiB range | **2.046 ms / 488.823 MiB/s** | accepted G4-v12 evidence |
| First/full native materialization, warm source | **307.652 ms / 325.042 MiB/s** | accepted G4-v12 durability evidence |
| Same-open protected-seed full read | **10.058 ms / 9,942.582 MiB/s** | accepted byte-delivery evidence; digest separate |
| 100-MiB one-byte incremental materialization | **4.104 ms** | accepted G4-v12 evidence |

The rows explicitly qualified above are accepted G4-v12 observations under the
separate stage-level terminal decision. Cells not measured by G4 remain the
latest retained Canonical-v2 authority. V12 evidence remains sealed REVISE;
the separate G4 stage decision promotes the qualified benchmark-private
baseline without relabeling the v12 old-gate result. Historical CP-0008
500-MiB results remain scale evidence; no new 500-MiB execution is authorized.

Controlling documents:

- [current benchmark scoreboard](baseline/current-benchmark-scoreboard.md)
- [G3 incremental materialization report](experiments/g3-incremental-materialization/G3-REPORT.md)
- [G3 campaign baseline](baseline/g3-incremental-materialization-baseline-v1.md)
- [G1 writer-memory baseline](baseline/sqlite-writer-memory-cache-spill-2000-baseline-v1.md)
- [Canonical-v2 baseline](baseline/canonical-v2-baseline-v1.md)
- [FastCDC-v2 baseline](baseline/fastcdc-contiguous-region-kernel-v2-baseline-v1.md)
- [CP-0008 count-change scale diagnostic](test-checkpoint-report/cp-0008-dirty-4f1c97f81f7c-count-change-scale.md)
- [optimization decision map](../../research/phase-4/decision-map.md)
- [invariant matrix](../../research/phase-4/foundations/invariant-matrix.md)
- [benchmark method](../../research/phase-4/foundations/benchmark-and-evidence.md)

## 2. Execution order

```text
G1 retained control
  -> G2 decompose materialization and select exactly one mechanism
  -> G3 implement and kill-screen the smallest incremental mechanism
  -> G4 qualify materialization across the compact 1/10/100-MiB matrix
  -> G5 close reopen, edit locality, concurrency, and residual SQLite lanes
  -> G6 freeze the final scoreboard, evidence, limitations, and WP5 handoff
```

| Stage | Scope | Fast-iteration budget | Exit condition |
|---|---|---:|---|
| **G2 — materialization research** | Decompose SQLite read, authentication/hash, output, filesystem, receipt, and mutation-authority work | `<20 s` diagnostic plus static analysis | exactly one candidate selected, or `INSUFFICIENT_EVIDENCE` |
| **G3 — incremental prototype** | Receipt-valid no-op, same-size one-byte update, 1-MiB replacement, invalid-receipt/fault fallback | `<20 s` mechanism screen | retain or revert one same-size mechanism |
| **G4 — materialization acceptance** | Compact 1/10/100 matrix for authenticated reconstruction, native/cold qualification, trusted hot, incremental, and fallbacks | `<=120 s` measured campaign total | materialization baseline frozen |
| **G5 — remaining core lanes** | Reopen authority, count-change locality, concurrency/endurance, optional physical-profile/create work | separate one-variable screens | every lane accepted, retained, or explicitly deferred |
| **G6 — closure** | Final matrix, tests, manifests, limitations, decision ledger, and WP5 handoff | no new candidate | Phase 4 PASS or exact blocker list |

Research may proceed in parallel. Builds, source changes, and measured
campaigns advance serially from the latest accepted control. A stage groups
decisions; it does not authorize stacking them into one implementation.

## 3. G2 — materialization decomposition and authority

### Objective

Separate the currently conflated operation:

```text
open/head
  + SQLite/CAS reads
  + canonical authentication and hashing
  + mapping traversal
  + reconstructed-output copies
  + native destination writes and metadata
  + destination authority/publication
  = materialization lifecycle
```

The historical pre-G4 338.776/366.357-ms rows reconstructed every byte through
an authenticated logical sink. They were neither incremental materialization
nor proof of a cold native checkout; the current accepted G4 rows are
237.214/237.381 ms.

### Required distinctions

- empty destination versus authenticated existing destination;
- first-ever versus repeated materialization;
- logical reconstruction versus native output;
- cold, warm, reopened, and warm-or-unknown cache states;
- authenticated SQLite/CAS bytes read versus destination bytes written;
- receipt validation versus byte-level destination authority;
- no-op, changed-range, and full-fallback work;
- logical, apparent, and allocated storage observations.

Unsupported physical I/O, cache, sync, or CPU attribution remains
`Unavailable` with a reason. RSS, Q, logical length, allocation, and wall time
must not substitute for physical I/O.

### Candidate-selection rule

G2 launches no broad implementation. It ranks concrete mechanisms by measured
removable wall, correctness authority, memory/storage/concurrency effects, and
the fastest falsifying screen, then selects exactly one:

1. destination-authority-gated no-op and changed-range materialization;
2. one-pass authenticated streaming if duplicate reads are directly measured;
3. a verified same-volume native seed/clone path with exact fallback;
4. a read-side SQLite/page-profile mechanism only if decomposition attributes
   enough wall to it.

Foreground compression, Git-style delta packing, and a second durable carrier
remain rejected defaults. Their prior measured cost/storage evidence does not
justify reopening them.

### G2 closure — 2026-08-22

G2 closed `PASS / INSUFFICIENT_EVIDENCE FOR A CONSTANT-FACTOR CANDIDATE`.
The accepted decomposition retained the existing implementation and selected
destination-authority-gated incremental materialization for G3; it did not
promote a read-side micro-optimization.

| Decomposed 100-MiB materialization family | Median wall | Directly removable under current authority? |
|---|---:|:---:|
| Canonical authentication | **94.817 ms** | No |
| Closure commitment | **88.483 ms** | No |
| Source/output fingerprint | **87.890 ms** | No |
| SQLite BLOB acquisition | **59.404 ms** | No |
| Secondary byte decode | **0.141 ms** | Yes, but immaterial |

The instrumented decomposition added **1.067%** balanced overhead
(328.897 ms control versus 332.405 ms instrumented). The fresh complementary
BA semantic pair matched exactly on root, transition, database, work, storage,
one transaction/COMMIT, and terminal Q=0. It was not used for a timing claim.
The sealed v5 closure has 33/33 manifested payload entries, zero mismatches,
and completed within its 59-second protocol ceiling. Historical v1/v3 remain
`REVISE`; v4 remains rejected before execution.

Post-G2 static closure passed:

- `cargo test --workspace --offline --all-targets`: **142 passed, 1 ignored, 0 failed**;
- `cargo clippy --workspace --offline --all-targets -- -D warnings`: **PASS**;
- `cargo fmt --all -- --check`: **PASS**;
- `git diff --check`: **PASS**.

G3 is therefore eligible. The G3 experiment must target avoided full-file work
for a receipt-valid no-op and same-size changed ranges while preserving the
complete authenticated fallback.

Evidence: [G2 post-PASS static closure](experiments/g2-materialization-decomposition/G2-POST-PASS-STATIC-CLOSURE-20260822.md).

## 4. G3 — same-size incremental prototype

G3 completed its mechanism screen with **v13 TERMINAL PASS**. Attempt A was a
static NO-GO because ordinary destination metadata, receipts, and event hints
cannot prove current user-editable bytes without full authentication. The
retained Attempt B is a benchmark-private, same-open protected native seed:
clone the bound seed, authenticate and patch only proven changed ranges, then
sync and atomically publish. Invalid authority, destination mutation, count
change, missing qualification, or clone failure uses complete authenticated
fallback with zero permit consumption. Symlink/wrong-kind has typed preflight
precedence; publication ambiguity is reconciled to exactly target/new or
prior/old.

| v13 row | Retained observation | Operation ns | Result |
|---|---|---:|---|
| 10-MiB receipt-valid no-op | one clone; zero payload/canonical-auth/reconstruction/patch/fallback | 993791 | exact new output |
| 100-MiB one-byte update | 22551 canonical-authenticated B; one-byte patch; no full reconstruction | 3414166 | exact new output |
| 10-MiB one-MiB update | 1086013 canonical-authenticated B; 1048576-byte patch; no full reconstruction | 2926167 | exact new output |
| 1-MiB invalid authority | 0/1 authority success/failure; 1048576-byte complete fallback | 3684250 | exact new output |
| 1-MiB external mutation | 1048576-byte complete fallback | 4360042 | exact new output |
| 1-MiB symlink substitution | typed preflight; zero authority/seed-authority, permit, SQL/BLOB/canonical-auth/reconstruction, clone/copy/patch/fallback, temp/sync/rename/reconciliation counters; verification 1048576 B | 7666 | exact old output |
| 1-MiB count change | 1048577-byte complete fallback | 3837542 | exact new output |
| 1-MiB before-publication fault | one-byte patch attempt; no rename; temp removed | 602166 | exact old output |
| 1-MiB lost acknowledgement | target reconciliation compared 1048576 destination/source B; Q 56849/0 charges the fixed comparison buffer | 3123083 | exact new output |

Campaign wall was **17,722,050,000 ns** and the operation sum was
**22,948,873 ns**. All rows were byte/mode exact with terminal Q zero and zero
temp/seed residue. Maximum isolated allocation was **440,541,184 bytes** under
512 MiB; **552,169,472 bytes** is the descriptive sum across independently
retired row roots, not simultaneous usage. Primary and independent analysis
agreed on normalized ledger
`19a3fd5ab1d5fb4dc00ffe396de1d118bfc38706d85c4009a974033d0a4010a1`.

Versions v1, v2, v4, v5, v6, v7, and v9 were zero-row pre-execution revisions.
v3 retained six passing rows but revised its cumulative-storage protocol. v8
retained nine passing rows and cleanup but revised copied-analyzer repository
derivation. v10 retained nine passing rows and both analyzer results, then
failed workspace static closure because Cargo auto-discovered the G3 module as
a standalone binary; it was not sealed. v11 repaired that manifest and sealed,
but independent post-seal review classified it historical REVISE because its
Q accounting, cleanup ownership, first-error precedence, and canonical
changed-range proof were incomplete. v12 repaired those four product defects
but was frozen as a zero-row PREEXEC REVISE for five evidence-protocol gaps.
v13 retained the repaired source, closed only those five protocol gaps, and
reran all nine rows without reuse.

G3 remains benchmark-private, non-production, macOS/APFS-native, and limited to
operation-local same-open/process-lifetime custody with no persistent replayable
destination receipt or malicious same-UID guarantee. Physical I/O, OS cache
residency, and device stable-media completion remain unavailable. Static
closure passed 157 workspace tests with 1 ignored and 0 failed; 15 focused G3
tests, clippy, rustfmt, diff check, and custody review also passed. The 67-entry
manifest and terminal verification seal v13 as G3 PASS.

Sealed evidence: [campaign](../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/CAMPAIGN-v13.json),
[static closure](../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/STATIC-CLOSURE-v13.json),
[payload manifest](../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/PAYLOAD-MANIFEST-v13.tsv),
[terminal](../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/TERMINAL-v13.json), and
[terminal verification](../../target/phase4-g3-incremental-materialization-20260822-v13/results-v13/TERMINAL-VERIFICATION-v13.txt).

Evidence: [G3 report](experiments/g3-incremental-materialization/G3-REPORT.md)
and [G3 campaign baseline](baseline/g3-incremental-materialization-baseline-v1.md).

## 5. G4 — materialization acceptance

G4 v12 is **SEALED / TERMINAL REVISE** under the original relative-only
contract. Its complete <=120-second campaign
retained 30 records, 50 logical arms, and 76 measured child observations. It
passed source/static/resource/direct-buffer/durability/exact-work/residue/
custody gates and both analyzers produced the same normalized ledger. It failed
only the original adjacent <=5% equation at seq17 (+8.535%), seq20 (+6.800%),
and seq26 (+14.360%). V12 must not be reanalyzed or rerun, and the original
gate is not reported as passing. The later
controlling [G4 stage terminal](experiments/g4-materialization-acceptance/G4-STAGE-TERMINAL-v1.json)
classifies the three +0.226229/+0.285522/+0.099604-ms regressions as
non-material because product materiality requires both a >5% ratio and at
least 1.000 ms absolute regression. G4 is closed with a stage-level terminal
PASS, while the v12 old gate remains failed.

| Operation | 1 MiB | 10 MiB | 100 MiB | Primary evidence |
|---|:---:|:---:|:---:|---|
| Authenticated logical reconstruction | smoke | smoke | primary | SQL/BLOB/hash/output wall |
| Fresh-process reconstruction | smoke | smoke | primary | reopen plus reconstruction |
| Proven cold native materialization | smoke | smoke | primary | native read/write/publication wall |
| Trusted hot full read | smoke | smoke | primary | authority and avoided bytes/cache budget |
| Receipt-valid no-op | smoke | smoke | primary | zero payload writes |
| One-byte same-size incremental update | smoke | smoke | primary | changed ranges and bytes |
| Same-size 1-MiB replacement | smoke | smoke | primary | changed-byte scaling |
| Count-changing update | smoke | smoke | primary | honest suffix/allocation or fallback |
| Invalid receipt/external mutation | focused | focused | primary | exact complete fallback |
| Destination faults | focused | focused | primary | atomicity, reconciliation, cleanup |

Phase 4 qualifies the engine boundary, authority, and benchmark behavior.
The current operation-local macOS/APFS implementation is benchmark-private;
v12 does not authorize broad projection, FUSE/VFS, SDK, OS/application, or
product integration.

## 6. G5 — remaining core lanes

This task stops before and authorizes no G5 implementation or measurement.
The lane descriptions below remain roadmap material only.
Concurrent premature `research/phase-4/g5-round-0` planning is foreign to and
excluded from G4 custody; it must not be treated as an accepted G5 start.

### G5-A — reopen authority

Current open/head lookup is already 3.583 ms. The unresolved cost is the first
edit after reopen: 154.019 ms versus approximately 6.4 ms same-open. Full
closure authority, not head lookup or edit construction, dominates.

Choose one terminal disposition:

1. retain the secure `Theta(stored closure)` scrub after an untrusted reopen;
   or
2. accept a fast path only with a non-replayable store generation, mutation
   mediation, writer fencing, rollback/downgrade protection, and exact crash/
   ambiguous-outcome reconciliation.

Do not use a replayable receipt or ordinary file metadata as proof of fresh
cross-process authority. If no adequate external trust primitive exists,
retaining the scrub is a valid Phase-4 closure decision.

### G5-B — count-changing edit locality

Count-changing edits insert or remove CDC references. Current fixed-radix
mapping remains correct and fast in absolute terms, but rewrites the suffix:

| Evidence | Early `+1` | Middle `+1` |
|---|---:|---:|
| Current 100-MiB Canonical-v2 guard | **5.108 ms** | **4.576 ms** |
| Historical 500-MiB CP-0008 | **27.141 ms** | **15.102 ms** |

CP-0008 directly observed approximately 5x suffix/mapping work from 100 to
500 MiB. The lane therefore remains algorithmically open even though the
retained `<50 ms through 500 MiB` policy passed.

Before a durable mapping change, decide the product SLA. Run an H09/prolly
simulator only if the requirement is near-size-independent count-changing
latency at multi-GiB scale. Advance beyond simulation only if it demonstrates
deterministic, history-independent roots, bounded node sizes, direct offset
lookup, approximately flat affected work, and no material create/same-count
regression. A mapping change must not be justified by asymptotics alone.

### G5-C — concurrency and endurance

G1 reduced per-writer RSS from approximately 89 MiB to 12.48 MiB, but earlier
cache spilling may acquire SQLite's exclusive lock sooner. Single-operation
memory and latency do not prove concurrent behavior.

| Workload | Required observation |
|---|---|
| Reader during one writer | reader blocking/tail latency and writer wall |
| Multiple readers | aggregate throughput, tail latency, bounded memory |
| Two same-store writers | deterministic serialization and bounded waiting |
| Independent stores | CPU scaling, aggregate RSS/Q, no global bottleneck |
| 10/100/1,000 revisions | memory, storage, mapping depth, and latency plateau |
| Failure/cancellation under load | rollback, cleanup, one COMMIT per accepted mutation |

Any future pipeline keeps SQLite on one ordered writer connection, uses hard
bounds, preserves deterministic evidence/error order, and proves cancellation
and rollback. Do not add workers merely to chase a small full-create win.

### G5-D — residual SQLite/create optimization

The retained create result already exceeds the original 300-MiB/s goal. A
new create candidate requires a material objective and a directly measurable
budget.

Evaluate one variable at a time:

1. one intermediate spill threshold if it can recover mapping wall while
   retaining a prospectively bounded low RSS;
2. a fresh 16-KiB page profile if it reduces page/spill events and protects
   edit, materialization, range, storage, and migration behavior;
3. bounded ordered CDC/hash/SQLite overlap only when a materially sub-300-ms
   target justifies a new execution profile;
4. retain the current `279.463 ms / 357.829 MiB/s` control when no candidate
   has sufficient cross-operation upside.

Never restore the old approximately 89-MiB writer peak, duplicate the full
payload, add an unbounded cache, weaken `FULL + DELETE`, add a second durable
publication boundary, or call pager bytes physical I/O.

### G5-E — operation-shape qualification

The accepted final control must cover the important shapes that are not
necessary in every speculative performance screen:

- empty, tiny, CDC minimum/target/maximum, and EOF boundaries;
- append, truncate, insertion, deletion, replacement, and scattered edits;
- identical rewrite, existing-chunk CAS reuse, and cross-file deduplication;
- damaged object, mapping, receipt, authority, and visible head;
- before-COMMIT and ambiguous/lost-ack outcomes;
- repeated history and same-store contention;
- explicit cold, warm, reopened, and warm-or-unknown evidence labels.

Cheap correctness tests run early. Expensive performance/endurance cells run
only after a candidate demonstrates its primary mechanism.

## 7. G6 — final Phase-4 closure

G6 introduces no new optimization. It freezes the surviving implementation
and closes every lane with `ACCEPT`, `RETAIN_CURRENT`, or an explicit blocker.

Required deliverables:

1. refresh the simple 1/10/100-MiB scoreboard from one exact accepted control;
2. run focused boundary, corruption, fault, resource, and concurrency tests;
3. run the full workspace tests, clippy with warnings denied, and rustfmt;
4. freeze source, diff, executable, profile, fixture, command, environment,
   raw-row, analysis, and manifest hashes;
5. independently recompute the controlling statistics and timer equations;
6. report exact Q/RSS/memory bounds and logical/apparent/allocated storage;
7. label unavailable physical I/O, sync, cache, cold-state, and CPU evidence;
8. record every accepted, rejected, retained, and deferred research direction;
9. produce a clean checkpoint and the WP5 handoff without starting WP5.

Phase 4 closes only when every lane has a terminal evidence-backed decision:

| Lane | Required terminal disposition |
|---|---|
| Durable full create | accepted optimized control or retain-current decision |
| Same-open edits | size-scaling goal met or suffix-linear limit explicitly accepted |
| Reopen authority | trusted fast path accepted or secure full scrub retained |
| Materialization/read | cold/hot/first/repeated behavior measured; candidate accepted or fallback retained |
| Concurrency/resources | lock, aggregate memory, storage, history, and failure behavior qualified |
| SQLite durability | physical profile accepted or current profile retained |
| Global correctness | identities, errors, authority, Q, durability, storage, reconciliation, and custody pass |

## 8. Permanent execution rules

Every retained candidate preserves the applicable contracts:

- exact CAS, CDC, COW, canonical object, root, transition, delta, and receipt
  identities unless a separately authorized versioned profile changes them;
- exact errors and failure precedence;
- authenticated incumbents and no authority laundering;
- bounded owned memory, exact Q accounting, and terminal `Q=0`;
- one SQLite writer transaction, one publication COMMIT, and atomic visible
  head publication;
- rollback-journal `DELETE`, `synchronous=FULL`, `temp_store=FILE`, and
  `mmap_size=0` for the current profile;
- fresh independent reconciliation for ambiguous COMMIT outcomes;
- low extra storage with no duplicate full payload or unbounded history/cache;
- explicit concurrent-load and aggregate-memory qualification;
- no inference of physical I/O, sync, cold-cache state, or phase-local CPU
  from wall time, RSS, allocation, Q, or logical byte counts.

Every candidate uses the same cadence:

```text
measured removable budget and authority analysis
  -> prospective one-variable preregistration
  -> <20-second mechanism/parity screen
  -> immediate REVERT or retain for short A/B
  -> <=120-second total adjacent balanced campaign
  -> focused then full static closure only after a measured signal
  -> independent recomputation and read-only audit
  -> checkpoint accepted control or terminal negative evidence
```

No measured row may be selectively removed or rerun. Research and simulation
may run in parallel, but timed campaigns require a quiet host and advance
serially. No 500-MiB execution occurs without new explicit authorization.

## 9. After Phase 4

WP5 starts only after G6 freezes every disposition and the handoff. Phase 4
owns the optimized and qualified CAS + CDC + COW + canonical identity + SQLite
core. WP5 and later application/OS phases integrate that core into higher-level
APIs, workflows, projection/materialization surfaces, and product behavior.

This roadmap does not start WP5 or claim that application-level integration is
already complete.
