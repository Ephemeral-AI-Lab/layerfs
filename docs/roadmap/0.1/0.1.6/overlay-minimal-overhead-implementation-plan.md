# #130: compact Workspace implementation plan and tiny-churn evaluation

Status: promoted implementation plan, 2026-09-14. Architecture/feasibility review,
not implemented or performance-qualified. No new benchmark was run for this plan.

Latest owner direction supersedes both the former after-closure scheduling and
subsequent two-second milestone: **promote #130 now and evaluate closeness to the
existing tiny-* benchmarks. Defer the 25,000-file and million-file cases.**
The quick iteration set is the five **tier-500** create/stat/unlink/bulk-create/
bulk-delete cases; the full 20-case family is the later stable qualification.
Reject quadratic scaling, accept necessary linear work, and prefer logarithmic
point operations and constant-sized lifecycle bookkeeping.

Use the existing benchmark workloads, timing definitions and applicable criteria.
The 25k/two-second proposal is retained as deferred history, not a current exit
criterion or registration task. Million-file qualification is deferred and still
open wherever a broader migration contract requires it; small-case success does
not satisfy or waive it. Do not launch either deferred case now.

The [overlay rules](overlay-snapshot-rule.md) and
[specification](overlay-snapshot-spec.md) still govern semantics. #124 correctness
and #125 final benchmark obligations remain. This plan selects the next measured
improvement instead of requiring every previously proposed storage mechanism.

## 1. Feasibility review and decision

Two independent subagent reviews traced tiny-churn fixtures/receipts and the
current write/index/transport path. The coordinating review checked ownership,
snapshot and complexity requirements. Source checkpoint: main
`695c482e7aa8471710ad7e869334084de0bf2fa0`, plus the separately identified,
unverified interrupted Step 5 changes in lifecycle/projection/reconcile/registry.
Those local changes are not delivered or passing product evidence.

**Verdict: promotion is justified; comparable tiny-churn performance still needs
measurement.** The current representation has demonstrable amplification.
Repair the quadratic reclamation mechanism and remove repeated preparation first;
then select the next measured cost. Foreground write/index/transport, complete
Commit and cleanup must all be visible. Clean-Commit improvements alone do not
establish performance for dirty tiny-file cases.

| Retained evidence | What it establishes | What it does not establish |
|---|---|---|
| [25k diagnostic source/log](https://github.com/Ephemeral-AI-Lab/layerfs/blob/1ffd63688eaabca7b8a4b756a342cde4514011d2/docs/roadmap/0.1/0.1.6/evidence/step1-capacity/run-25000.log) | 25,000 one-byte files read back; no resident-owner exhaustion; payload index 233,033,728 B, arena 117,489,664 B; metadata 105,515 live pages | Its 1,099.52 s debug wall includes repeated full censuses and verification. No public Begin/Commit/End, production throughput, full V1 or million-file proof |
| [v0.1.5 tiny-create-500-mixed-v4](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v0.1.5/release-notes/0.1.5/benchmark-closeout.md#L268-L292) | 500 created files, 824,450 B, 5,000-file/500-MiB background; 242.659707 ms public-call sum | Not 25k one-byte files; sum is not contiguous wall; old snapshot semantics are not new supported-surface proof |
| v0.1.5 tiny-bulk-create-500-mixed-v3 | 5,000 new files / 500 MiB; 5,732.993668 ms public-call sum | Includes three 100-MiB files, so not a uniformly tiny-file workload |
| [Host integration #131](https://github.com/Ephemeral-AI-Lab/layerfs/pull/131) | Linux host-FUSE authority and focused Commit/live-operation tests | Docker placement still uses the legacy route; a Linux-local mounted test is not macOS-host/Docker integration |

The v0.1.5 tiny-create-500 raw phases are Begin 10.733458 ms, Exec 166.714666 ms,
Commit 59.709208 ms, visibility 0.107333 ms, End 5.395042 ms. Its retained raw
receipt is `benchmark-results/host-store/issue120/performance/tiny_file_churn/`
`tiny-create-500-mixed-v4/perf.jsonl`, SHA-256
`bb0fc751f116f94e68888f12cd17939ebdb24754572192ff0d5b948209626ee5`.
The producing binary is `c55daf13e372331a5ab6dbd465ece351a55923831c45864325ac46b1508fa295`
on source `1ff1f2ddd`. Historical statuses and original source identities stand.

Current `HostClient::call_owned` serializes a complete request/reply. Measure
queue/transport/host execution on the existing cases; do not assume page packing
alone restores their latency. TCP_NODELAY already exists. A sequential write
cannot be pipelined before its dependent create returns, and legacy batch
transport is not a ready-made host-wire Create/Write batching mechanism. The
older two-second/50,000-exchange arithmetic is historical context for a deferred
case, not a current feasibility gate.

The review also found a **production quadratic counterexample**, separate from
the diagnostic censuses. In `overlay_payload.rs`, releasing coverage sets
`state.sweep`; maintenance switches arenas and evacuates the entire previous
arena, including unrelated live blocks. A small deletion can move O(N) live
blocks; repeating deletions with completed arena maintenance between them can move
N + (N-1) + ... blocks, or Theta(N^2). The 64-KiB per-step move bound does not
bound cumulative work. Repeated hot-file replacement among N unrelated blocks
can similarly cost Theta(updates * N). Repair this before accepting scalability.
See [payload reclamation source](../../../../crates/layerfs-workspace/src/overlay_payload.rs),
especially `reclaim_step` and the coverage-release trigger.

Other inspected paths must not be mislabeled quadratic: metadata Index updates
follow tree paths; live readdir resumes by cookie; captured-change deduplication
uses an index; candidate construction makes a fixed number of passes. They have
indexed/logarithmic factors and large constants. Fixed-count backing-file stat
calls are expensive constants, not whole-namespace scans.

## 2. Selected architecture and scaling contract

Keep one host authority, one existing Index/ownership backend and one canonical
construction pipeline. Keep published metadata immutable. Share the immutable
base; store only actual upper state and retained ownership. A snapshot retains
the existing root; publication changes exact coverage/comparison context, never
live contents, handles or mount identity.

```text
Linux FUSE / public SDK
         |
         | bounded authenticated operations; host ownership before acknowledgment
         v
Host current root -----------------------> immutable canonical base
         |
         +--> dense existing metadata Index
         |      inode / binding / cookie / alias / current-change records
         |      compact empty/single range; indexed fragmented ranges
         |
         +--> ordinary payload tokens -> shared host arena allocation
         |      small sources share allocation units; independent ownership
         |
         +--> O(1) owned snapshot root -> existing canonical builder
                                        -> exact stage -> conditional publication

Later writes install a new root; old owners retain only their required state.
No new per-Commit index copy, private tree copy, operation history or full reset.
```

Symbols: N is namespace size, D relevant changed files, K relevant changed names,
P pieces in an affected file, T pieces actually visited/changed, U bytes processed,
G unowned resources visited by reclamation, R legitimately relocated live blocks,
and M payload-catalog records. Count names/aliases and output bytes explicitly
when those are the operation's required output.

| Operation | Required work shape | Rejected pattern |
|---|---|---|
| Begin over an existing base | Constant-sized Workspace setup plus actual lease/mount work | Enumerate/promote all N base files |
| Point lookup / small write | Metadata/range index paths plus actual affected records/bytes; include payload-catalog lookups, normally O(log M) per affected interval, and indexed updates for visited pieces | Whole-directory/tree/piece-list rewrite per point update |
| Snapshot acquisition | O(1) root/ownership bookkeeping independent of N, D, P, U | Full scan, dirty-map clone, cache drain, deferred whole-copy on next write |
| Fixed-view directory/change scan | Linear in records necessarily visited/yielded, plus bounded seeks on resumed batches and required indexed lookups | Restart from the beginning for each page, full N scan for each changed inode |
| Build all D genuinely new files | At least linear input/output work; preserve bounded sorted construction and actual codec work | Rebuild all prior files separately for every new file |
| Small C2 after large C1 | Relevant D/K and affected index/range work | Rescan all changes since Begin or reconstruct C1 because live provenance is older |
| Reclamation | Bounded foreground assist; O(G + R) resource visits plus actual indexed lookup/update work, normally O((G + R) log M), and copied bytes; amortized R follows freed/affected blocks | Scan all accumulated allocation/history on every allocation; retain every intermediate edit |
| Verification of all files | One necessary streaming pass plus actual canonical lookup/hash work | Put a full verifier/census after each timed mutation |

**No quadratic scaling is accepted.** Separate per-operation from cumulative
cost: D indexed operations may total O(D log N), and a bounded external sort may
take O(D log D). Name these honestly; do not call them linear or constant.
Prefer ordered cursors/batched existing construction to remove redundant sorting
where possible. Fixed page size does not make a walk over all pages O(1).
Moving a scan into maintenance, End, a background thread or the first post-capture
write does not remove its cost or make quadratic behavior acceptable.

Prove these bounds from loops/cursors and ownership algorithms, then check
visited-record/page counts, bytes and physical allocations over increasing sizes.
Two timing points cannot prove an asymptotic bound. Diagnostic full censuses every
1,000 insertions have cumulative quadratic work when the index grows with N;
retain the old diagnostic, but exclude those censuses from performance sampling.

## 3. Implementation sequence and exit evidence

### Issue ownership: #130 implements overhead reduction

| Issue | Implementation/evidence responsibility |
|---|---|
| **#130** | Reclamation and metadata-update cost; measured compact storage, construction, lifecycle and transport improvements; their affected correctness checks and five/20-case tiny-churn comparisons |
| **#124** | Finish production host/Docker FUSE and SDK wiring, supported snapshot capture/V1, Commit integration, non-Commit consumers and obsolete-coupling removal; owns complete migration correctness |
| **#125** | Later complete existing benchmark campaign excluding #122, final matrix/custody/report and its own terminal requirements |

Promoting #130 means implementing its overhead changes **now alongside #124**.
It does not rename unfinished #124 integration as #130 work or require finishing
all of #124 before the first optimization. A verified new production route is a
dependency for authentic public measurements; independently testable allocator,
Index, range and correspondence work can start against owned inputs immediately.
Full supported-surface correctness and later issue closure keep their own gates.

These are the concrete #130 code deliverables and their bounded checks:

| #130 change | Primary source | Focused verification |
|---|---|---|
| Replace whole-arena sweep with local reclamation | `overlay_payload.rs` | Repeated small deletion/replacement moves only justified blocks; retained-reader/partial-I/O/quota/cleanup checks; no quadratic cumulative relocation |
| Prepare final metadata/range records without redundant intermediate roots | `overlay_index.rs`, `overlay_ranges.rs`, `overlay.rs` | Fewer page/catalog writes; stale-source/disjoint-write/retained-snapshot/rollback checks |
| Reduce measured metadata and common-range allocation | Same Index/range modules, when selected | Byte-aware bounds; Empty/Single/Indexed transitions; maximum names/values; exact independent token/base ownership |
| Reduce measured tiny-payload rounding | `overlay_payload.rs`, when selected | Packed arena allocation; append/subrange/relocation/neighbor retention and exact physical charging |
| Reduce measured Commit correspondence/setup cost | `correspondence.rs`, `snapshot_candidate.rs`, `host_runtime.rs`, when selected | Same canonical output, equal-byte/new-Origin UpToDate and localized C2, receipts and retained cleanup |
| Reduce measured request/cache overhead | Existing host client/operations/transport paths, when selected | Same ordering/replay/acknowledgment/ownership with lower observed wait/I/O cost |

The first implementation slice is the #130 reclamation counterexample and repair,
with metadata preparation next. P130.1's comparator/instrumentation work supports
those changes; it is not a replacement for implementing them. #124's unfinished
four-file patch is separately owned and must not be bundled into an overhead PR.

The packages below are not a renumbering of #124's seven phases.
Continue dependency-ready work across packages; a review/revision is not an
implementation completion or a reason to stop an authorized execution loop.

### P130.1 — Pin #130 comparisons and cost instrumentation

- Preserve the interrupted #124 patch and all existing results before choosing
  source. Its publication-error/reconciliation repairs and production wiring
  remain #124 tasks; do not make them the first #130 implementation package.
- Consume #124's verified macOS-host/Docker route for public measurements.
  Receipts must identify new dispatch. If that integration is unavailable, mark
  only dependent public measurements blocked and continue #130 component work;
  never substitute legacy dispatch or hold all overhead work behind the route.
- Select the five tier-500 tiny-create/stat/unlink/bulk-create/bulk-delete cases using
  section 4. Preserve their fixtures, seeds, worker counts, normalization, sync,
  timing and independent proof. Do not register the deferred 25k case or expand
  the 500-entry schedule for it.
- Obtain compatible release baseline/current diagnostics under the measurement
  lock. Count transport requests/queue time, host execution, Index edits/path
  copies/catalog I/O, range/payload allocations and complete lifecycle phases.
  Expensive tracing is a diagnostic, outside accepted timing distributions.
- Start with the slow/affected existing case and its smallest correctness check.
  If network waiting dominates, investigate its actual dependency; do not change
  worker count or defer host acknowledgment ownership to make the case faster.

Exit: exact fixtures/timers/custody and route evidence; a source-backed cost
breakdown and selected next change. Unmeasured performance remains unqualified. A missing V1 mechanism does not block independent ordinary-input cost
diagnostics, but those do not qualify the complete snapshot surface.

### P130.2 — Remove quadratic reclamation and repeated Index construction

Replace release-triggered whole-arena evacuation with indexed free-space reuse,
tail truncation and bounded evacuation of an eligible tail block into known free
space. Update only that block's bounded occupants through stable locations.
Retained readers keep old physical ownership until safe release; failures retain
charges. The existing Index's dense-slot/tail-page reclamation is an algorithmic
reference to reuse, not a reason to add a generic allocator framework.

Require an amortized work argument across the complete cleanup sequence. A tiny
deletion/replacement must not move every unrelated live block. If a tail cannot
move because it is pinned, retain and charge it instead of scanning/repacking the
whole arena repeatedly. Pair this change with single-deletion, shrinking-delete
and fixed-hot-set replacement checks over a large unrelated live set.

Use the existing prepared mutation, leased source and atomic installation.
Prepare the final affected key/value edits together, updating shared paths once
instead of publishing and retiring intermediate Index roots per field. Bound
batch records/bytes and reuse existing page/ownership transactions, including
rollback and release admission. Do not add a second mutable map or transaction
framework. Combine the current three-record small-range leaf preparation.

The reviewed ordinary create/first write performs about 27 direct Index edits
before extra mounted-handle, lookup, refcount and reclamation work. Treat that as
a source inventory, not a syscall count or promised speedup. Remove redundant
full physical-stat queries from hot paths only where admitted logical counters
already provide the same required decision; retain real physical reporting.

Exit: reclamation work follows freed/relocated blocks rather than all live payload
per deletion; smaller edit/path-copy/write counts, with stale-source installation,
disjoint writes, retained snapshots, quota/partial-I/O and rollback checks passing.
No published-page in-place mutation is introduced.

### P130.3 — Select the next measured storage cost

The complete compact-storage bundle is not mandatory for the first useful
result. Select the smallest change justified by current tiny-churn phase/I/O/
allocation evidence; version its coordinated private encoding before coding:

| Candidate | Selection evidence |
|---|---|
| Denser existing Index | Page slack, path depth or COW/catalog I/O remains material |
| Compact common ranges | Dedicated range pages and their lookup/reference work remain material |
| Packed tiny arena allocation | Rounding, payload writes or reclamation remains material |
| Singleton correspondence | Commit builds/retains disproportionate per-file description trees |

The concrete design directions, when selected, are:

1. **Dense pages in the existing Index.** Split/merge by encoded bytes and bounded
   decoded size; keep maximum-name/value and overflow validation. Raising the
   seven-key constant alone is unsafe. Retain indexed ownership and bounded
   split/reclaim work; do not replace the database/backend.
2. **Compact empty/single/indexed file ranges.** Inline the bounded common
   description in its existing owner record; use the current indexed form for
   fragmentation through one range reader/planner API. Coordinate the inode and
   value layout: the current 128-byte inode already meets the inline-value
   threshold, so simply appending a Piece would create another overflow page.
3. **Small ordinary payload allocation in shared backing.** Add a packed
   allocation class inside the existing arenas with stable independent tokens,
   indexed free slots and bounded per-block reverse occupants. Preserve the
   selected maximum 4-KiB indivisible retention unit and reuse P130.2's local
   reclamation. This is not merely lowering `BLOCK`: source coverage currently
   rounds to 4 KiB and relocation assumes aligned contiguous locations. Version
   source length, allocation bounds, lookup and relocation before coding.
   A one-byte source must not
   inherently require its own physical 4-KiB allocation. Do not create a separate
   RAM-first payload engine or relabel ordinary writes as SDK Inline.
4. **Metadata-only singleton canonical correspondence.** Put the bounded
   singleton description in its parent value; preserve indexed fragmented
   descriptions, zero anchors, Origins and the existing C2 planner.

Packed neighbors need independent logical ownership: an A-only file reader must
not retain B's payload token through a shared metadata page. Acquire the target
file's bounded ownership under a temporary parent lease and then release the
parent. Physical block slack, including unrelated bytes within one legitimately
retained allocation unit, stays charged and bounded; it is not logical ownership
of B. Fragmented readers use bounded file-scoped cursors, not full Piece vectors.

Prove append/join/subrange promotion, Origin continuity, old-reader/new-writer
behavior, fsync/error handling, partial allocation failure, spills, reclamation
and truthful physical accounting. Do not raise quotas to hide page amplification.
Do not pack fragmented range trees across unrelated files in this first change;
the common Single form does not require that extra retention mechanism. Denser
metadata leaves can increase external-token retain/release callbacks per COW
page: batch correct ownership work and measure it. Byte savings alone do not
prove a faster write path.

Exit: encoded byte/record inventory and physical measurements demonstrate the
selected change's improvement, with its ownership/failure checks passing. Quantify
any remaining dedicated-range or rounded-payload floor without claiming an
unselected change was implemented. Report parent/index/catalog, retained versions,
scratch and slack too; no guessed MiB allowance is acceptance.

### P130.4 — Remove the remaining measured latency

Choose only what the new cost breakdown requires:

- Reuse warm physical services and defer genuinely unused backing/journals; every
  sample still has a fresh Workspace identity and ownership scope.
- An ordinary bounded immutable-page cache may reduce reads while preserving
  the existing disk ownership model. It does not remove page allocation or COW
  writes. A resident-only/write-back tier needs explicit bootstrap, location
  transfer, eviction/rollback, fsync-before-eviction and recovery contracts first.
- Transport changes require bounded request IDs/results, exact replay/acknowledged
  prefixes, ordering, disconnect/uncertainty handling and aggregate admission.
  Reuse applicable framing/authentication; legacy batch calls are not proof of
  host-wire batching. Preserve each operation's acknowledgment contract.
- A narrow unchanged-sequence shortcut may skip construction only after valid
  capture, with exact predecessor ownership and the existing stage/V4 receipt
  path. Equal-byte new-Origin writes still update correspondence. This shortcut
  is secondary to the dirty create/write objective.

Do not bundle a full pager, virtual inode IDs, alias promotion and replacement
change/cookie indexes merely because they appeared in the old proposal.

Exit: the previously failing cost/latency check and the genuinely affected
regressions pass. Retain failed attempts and record why any previous pass became
invalid. Do not repeat unchanged runs to select a favorable number.

### P130.5 — Qualify the existing tiny-churn evaluation and hand off

First complete the five tier-500 quick-iteration cases, retaining their valid
passes across unrelated fixes. Once the implementation is stable, complete all
20 unchanged tiny-churn cases under section 4's prospective
comparison rule and each existing stronger criterion. Require independent exact
verification, source/custody/resource and cleanup success. Publish actual case
numbers, phase differences and physical storage; no family average may hide an
individual mandatory failure. Retain every failed attempt and valid reused pass.

Complete the genuinely affected small-C2, large-file CDC/extent,
hardlink/rename/open-unlinked, replay/publication and fsync checks. Report
foreground latency during natural Commit construction separately from held-builder
correctness. Continue V1-V4 and actual production/non-Commit migration under #124;
small-case performance cannot claim complete supported-surface correctness.

This package verifies #130's delivered changes and regressions. Completing #124's
remaining V1/integration/consumer implementation or executing #125's full matrix
is not relabeled as #130 implementation. Keep cross-issue dependencies explicit
and link their evidence when a public acceptance claim needs it.

The 25k/two-second milestone and million-changed-file qualification are deferred,
not required to complete this current tiny-churn evaluation. Do not run them as
an implicit prerequisite. Where #124/#125 terminal contracts still require the
million-file proof, retain that obligation as DEFERRED/OPEN; do not close those
issues on small-case evidence or silently change their closure criteria.

The later #125 full included campaign still needs a correct sealed candidate
and its applicable capacity prerequisites. Retain its complete scope and 36 #122
exclusions. Existing development tiny-case results are not automatically final
campaign evidence; reuse them only where exact source/custody rules permit.

## 4. Current evaluation: existing tiny-* cases and closeness

The active source registry defines exactly **20 tiny-* cases**, all in one
family, `tiny_file_churn`: five operations at four tiers. Keep every identity,
fixture and default membership unchanged. None is in the 36 #122 exclusions.

**Quick iteration selects exactly these five existing cases:**

| Case | Affected files / payload |
|---|---|
| `tiny-create-500-mixed-v4` | 500 created tiny files; registered mixed background |
| `tiny-stat-500-mixed-v4` | 500 stat targets; registered mixed background |
| `tiny-unlink-500-mixed-v4` | 500 removed tiny files; registered mixed background |
| `tiny-bulk-create-500-mixed-v3` | 5,000 created files / 500 MiB plus registered witness |
| `tiny-bulk-delete-500-mixed-v3` | 5,000 removed files / 500 MiB plus registered witness |

The `500` tier does not mean 500 files in bulk cases. Do not substitute smaller
tiers or one-byte fixtures. Establish source-bound results for this set, then
rerun only a failing case and genuinely affected regressions after each fix.
A changed shared Index/ownership path can invalidate several passes; an unrelated
change does not. Do not rerun all five or all 20 routinely after every edit.
The five-case screen is not full-family or final #125 campaign completion.

The later stable full-family membership remains:

| Operation | Tiers / affected files | Registered suffix |
|---|---|---|
| tiny-create, tiny-stat, tiny-unlink | tiers 1/10/100/500 affect 1/10/100/500 files | tiers 1/10: compact-v2; tiers 100/500: mixed-v4 |
| tiny-bulk-create, tiny-bulk-delete | tiers 1/10 affect 50/500 files; tiers 100/500 affect 1,000/5,000 files | tiers 1/10: compact-v2; tiers 100/500: mixed-v3 |

Case IDs are `<operation>-<tier>-<suffix>`. Bulk tiers supply 1/10/100/500 MiB
respectively, with the existing witness profiles. Mixed tiers include varied and
large files, and individual-operation tiers have their registered untouched
background. Do not flatten those fixtures or use file counts as interchangeable
workload identities. Keep normalized metadata and the registered root sync in
managed Exec; no added per-file fsync or removal of existing synchronization.

### Prospective comparison rule

Compare each case against a compatible v0.1.5 control using the existing
[#118 regression screen](../../../general/benchmark_rules.md), also documented
in the benchmark QUICKSTART. Adopt it prospectively for #130 before collecting
new comparisons; do not invent a uniform two-second or per-file gate:

- Existing n3 fresh alternating pairs, with the existing arm/seed/custody rules.
- A material wall regression requires median paired slowdown greater than
  `max(15% of control median, 3 ms)` **and** at least two of three pairs slower.
- The analogous CPU rule uses `max(15% of control median, 1 ms)`.
- Preserve each stronger applicable unwaived criterion, including the existing
  strict <1-second tier-100 bulk create/delete target. A paired screen does not
  replace the recorded tiny-create-100 <1-second criterion where applicable, nor
  override a correctness, resource, cleanup, deadline or stronger timing failure.

For this evaluation, “close” means no material regression under that existing
screen, all applicable mandatory criteria satisfied, and resource/storage costs
reported without hidden deferred work. Preserve permitted minor differences and
reporting-only statuses under their original contracts; do not silently convert
reporting-only targets into hard gates. Faster results remain welcome, but no
blanket speedup ratio is promised.

Apply the comparison to the registered metric of the **whole selected workflow**.
The inherited receipt's `operations=1` is not 500 or 5,000 independent workflows;
never divide a slowdown by its file count to evade the millisecond floor.
Report file/byte-normalized mechanism counters separately for diagnosis.

### Public route, timer and evidence

- Use exact existing family/workload/oracle and seeds. Baseline and candidate
  have matched fixture, cache, topology, operations and timer, with only the
  declared product change differing. A legacy-route candidate run cannot qualify
  the new host-authority path. If the historical product cannot run compatibly,
  report comparison BLOCKED/unavailable and preserve its published rows as
  historical context; do not substitute a slower control silently.
- macOS owns Store/SDK/coordinator/construction/spool; Linux Docker owns real
  daemon/FUSE/workload under the existing 2-CPU/2-GiB/no-swap/256-PID profile.
  Reuse warm services, matching builds and pristine prepared inputs; each sample
  has a fresh Workspace. No mutable-Workspace reset/reuse.
- Preserve `pure_call_sum_ns` and the case's existing boundaries. Report Begin,
  managed Exec, Commit, visibility, End and full orchestration wall separately.
  A public-call sum is not contiguous wall or inner Commit time. If an enclosing
  interval is added for attribution, name it separately and do not replace the
  historical comparison metric.
- Keep normalization and sync in their existing timed scope. Wrapped syscall
  counters omit chmod/utimensat normalization; report those separately and observe
  actual FUSE/host requests. Do not equate POSIX and transport counts.
- Keep setup, verification and cleanup domains separate, while including each
  required product action in its real boundary. Report any legitimately deferred
  physical reclamation through settlement; moving cost out of End cannot create
  an apparent improvement. Independent proof checks the registered full expected
  state, including unchanged witnesses, metadata, branch results and cleanup.
- Retain exact source/binary/image/harness/fixture identities, paired raw results,
  sample counts, all failures and applicable statuses. Once a pass remains valid,
  reuse it; rerun only after concrete dependency/fixture/environment/custody
  invalidation. Never retry unchanged cases to improve a median.

### Deferred work

The [earlier 25k/two-second proposal](https://github.com/Ephemeral-AI-Lab/layerfs/blob/d36058a31f734ca12995c3b249c5ff80331b5a21/docs/roadmap/0.1/0.1.6/overlay-minimal-overhead-implementation-plan.md)
is retained history. Do not add its case registration, baseline run, counter
prefixes or two-second acceptance to the current task. The million-changed-file
capacity proof is also deferred; its original one-Workspace/final-Commit/reopen
contract remains open wherever required. Neither deferral is a PASS or waiver.

## 5. Scaling and full-suite evidence

Use the existing tiers for realistic workload/regression comparisons, but their
background and payload distributions differ, so they are not a one-variable
scaling curve. Inspect algorithmic loops and cheap operation/relocation counters
on the affected existing case; a small focused ownership/reclamation test can
exercise the quadratic counterexample without a new scale campaign. Do not add
25k prefix diagnostics or million-file work while those cases are deferred.

Report totals and per-file/record values for metadata/payload/index bytes, page
occupancy, Index edits, copied/visited pages, ownership/reclaim work, content tasks,
transport calls, CPU, RSS/cgroup domains and latency. Counters must expose deferred
work through settlement. Hot-set updates must not grow with unowned operation
history; large-base/small-delta tests must not scale as full-base reconstruction.

Retain #130's existing storage-accounting examples: an unchanged 100k-file base
with D=0/1/10 and a workload with 100k genuinely changed files. These distinguish
base sharing from actual upper-state cost; they add no timing gate and do not
replace the million-changed-file qualification. Bind names, ranges, bytes, pins,
snapshots and Commit phase to each accounting receipt.

All 20 inherited tiny-churn cases are outside the recorded 36 exclusions. At
review time `cases.json` SHA-256 was
`6a89e9ac1c5df25eb0a4a495cf013d0eb8ab66f55b10281de1f5d72b0a0fcdc8`, matching
the exclusion manifest. Recheck before execution. Tier-100 bulk create/delete's
applicable strict <1-second criterion remains unchanged.
Keep all original WARN/FAIL/waiver/reuse labels and applicable existing deadlines.

Use [benchmark rules](../../../general/benchmark_rules.md), `benchmark/AGENTS.md`
and the existing QUICKSTART/runner and measurement lock. Do not run
`shared/v016_matrix.py` or any #122-owned scenario. No release/tag, website deploy
or closure of #123 is part of this plan.

## 6. Issue updates and iterative execution

Post a substantive #130 comment when each P130 package actually completes:
changes, exact source, tests/case IDs, raw evidence, retained failures/fixes,
reused passes with validity, and next concrete task. Mirror production/correctness
changes to #124 and benchmark-facing changes to #125. The original seven-phase
completion comments remain required when their complete exits are met; a #130
package comment is not a substitute or a premature phase-completion claim.

During implementation, use bounded subagents with non-overlapping ownership for
independent work/review. Iterate against concrete failing checks, integrate and
review their results, then immediately execute the next ready task. Do not stop
at a design revision, passing component or agent completion while authorized work
can advance. Preserve unaffected passes and every failed attempt in the existing
ledger; no new test framework or parallel performance campaign.

Keep current source, active processes, evidence/issue links, blockers and next
action in the existing progress note. An external block must name the failed
action, evidence and intervention needed, and only blocks actual dependants.
V1 remains open until a generic supported mechanism is proved; no host-only cut,
implicit user fsync, removed mmap, third-party patch or freeze/drain is authorized.
Do not manufacture a comparison PASS or relax an oracle to end the loop.
