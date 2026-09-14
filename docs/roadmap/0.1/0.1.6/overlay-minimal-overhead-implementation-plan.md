# #130: compact Workspace implementation plan and two-second objective

Status: promoted implementation plan, 2026-09-14. Architecture/feasibility review,
not implemented or performance-qualified. No new benchmark was run for this plan.

Owner updates supersede the former after-closure scheduling of
[#130](https://github.com/Ephemeral-AI-Lab/layerfs/issues/130): promote this work
now; pursue **a complete warm, fresh 25,000-file Workspace workflow in at most
2,000,000,000 ns**; reject quadratic scaling, accept necessary linear work, and
prefer logarithmic point operations and constant-sized lifecycle bookkeeping.

The target belongs to #130. Existing #124 correctness/capacity and #125 benchmark
contracts, including stronger applicable tiny-churn criteria, remain unchanged.
The [overlay rules](overlay-snapshot-rule.md) and
[specification](overlay-snapshot-spec.md) still govern semantics. This plan
supersedes the earlier #130 implementation order and makes the remaining larger
representation proposals conditional; historical analysis and failures remain.

## 1. Feasibility review and decision

Two independent subagent reviews traced tiny-churn fixtures/receipts and the
current write/index/transport path. The coordinating review checked ownership,
snapshot and complexity requirements. Source checkpoint: main
`695c482e7aa8471710ad7e869334084de0bf2fa0`, plus the separately identified,
unverified interrupted Step 5 changes in lifecycle/projection/reconcile/registry.
Those local changes are not delivered or passing product evidence.

**Verdict: promotion is justified; two-second feasibility is not yet proved.**
The current representation has demonstrable amplification. The earlier narrow
clean/lazy/correspondence/range-leaf proposal is insufficient for this dirty-file
objective. The architecture must address foreground index work, common-file
storage and transport; faster inner Commit alone cannot satisfy it.

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

Two seconds permits only 80 microseconds per created file before fixed costs.
Current `HostClient::call_owned` serializes a complete request/reply; the current
host path needs at least 50,000 create/write exchanges, before other operations.
That leaves less than 40 microseconds per exchange before Begin/Commit/End.
This is a necessary-condition calculation, not a measured RTT or impossibility
proof. TCP_NODELAY already exists. A sequential write cannot be pipelined before
its dependent create returns; the existing legacy batch transport is not a
ready-made host-wire Create/Write batching mechanism.

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

The steps below are #130 work packages, not a renumbering of #124's seven phases.
Continue dependency-ready work across packages; a review/revision is not an
implementation completion or a reason to stop an authorized execution loop.

### P130.1 — Freeze the public workload and diagnose the actual bottleneck

- Preserve the interrupted patch and all existing results before choosing source.
  Correct its post-publication metrics error propagation and unbounded
  reconciliation scans before accepting those respective changes under #124.
- Wire/verify macOS host authority to Docker daemon/FUSE with SDK parity. Receipts
  must identify new dispatch; never measure legacy Docker dispatch as the new path.
- Register the opt-in milestone in the existing family/workload/runner/oracle,
  with the exact contract in section 4. The existing create schedule has only
  500 ranked entries: use an explicit `0..25_000` schedule and exact completion
  assertions instead of merely changing its tier.
- Obtain compatible release baseline/current diagnostics under the measurement
  lock. Count transport requests/queue time, host execution, Index edits/path
  copies/catalog I/O, range/payload allocations and complete lifecycle phases.
  Expensive tracing is a diagnostic, outside accepted timing distributions.
- Test the serial transport budget early. If it exceeds the available time,
  investigate an actual compliant reduction; do not optimize only disk layout,
  silently change worker count, defer host acknowledgment ownership, or claim
  that independent-request multiplexing removes a sequential dependency.

Exit: exact fixtures/timers/custody and route evidence; a source-backed cost
breakdown and selected next change. Unknown RTT and the two-second miss remain
visible. A missing V1 mechanism does not block independent ordinary-input cost
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

### P130.3 — Remove common-file storage floors

Select and version one coordinated private encoding before implementation:

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

Exit: encoded byte/record inventory and actual physical measurements reject the
former dedicated-range-plus-rounded-payload floor. Report parent/index/catalog,
retained versions, scratch and slack too; no guessed MiB allowance is acceptance.

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

### P130.5 — Meet milestone one, then complete migration qualification

Run the full section-4 workflow from the selected release source. Require the
two-second result, exact reopened contents/metadata/count, correct publication,
resource/custody and cleanup. Report foreground latency during natural Commit
construction separately from held-builder correctness tests. Passing this one-byte
case establishes its own scope; it does not establish mmap or realistic-payload
performance.

Complete the affected inherited tiny-churn family and the relevant small-C2,
large-file CDC/extent, hardlink/rename/open-unlinked, replay/publication and fsync
checks. Finish V1-V4 and the actual production/non-Commit migration under #124.
Complete the required million-changed-file proof with one final Commit and fresh
reopen on the selected representation; carry valid prior evidence only when its
relevant dependencies/custody permit. The 25k milestone is not a substitute.

Seal the candidate only after required correctness/capacity is complete. #125
then runs the complete existing included suite, including required extended cases,
with the 36 #122 exclusions reconciled against the registry. Neither prior #124
closure nor a separate second full benchmark campaign is required to begin that
handoff. Required failures must be fixed; the new target does not replace existing
criteria. Close issues only after their respective terminal evidence is valid.

## 4. Milestone one: exact public workload and acceptance

Proposed new opt-in case identity: `tiny-create-25000-onebyte-lifecycle-v1`, in
`tiny_file_churn`. It is a separate #130 extension, not a new default family tier,
a renamed #122 case or a replacement for the 20 inherited cases. Registration,
actual operation counts, fixture, oracle and selection self-checks must agree.

| Field | Frozen requirement |
|---|---|
| Initial state | Empty target namespace in an independently owned sample Store; setup/Init outside the lifecycle timer and separately reported |
| Services / Workspace | Warm already-bound services; one fresh Workspace per sample; no mutable Workspace reset/reuse |
| Files | Exactly 25,000 regular files `file-00000` through `file-24999` in the root |
| Bytes | Exactly `b"x"` written once per file: 25,000 supplied bytes; intentionally dedup-friendly, not a general throughput profile |
| Workload | One managed process, sequential ordinary open/create-exclusive, one-byte write, close; no SDK bulk-create substitution or 25,000 Docker execs |
| Metadata / sync | Reuse declared tiny-churn normalization: mode 0640 files, 0750 directories, mtime 1700000000000000000 ns; include normalization and the existing single root-directory fsync in Exec; no per-file fsync |
| Primary time | Contiguous monotonic elapsed time immediately before public Begin through completed End, including managed Exec, Commit and any intervening public visibility operation |
| Objective | Primary elapsed time <= 2,000,000,000 ns under the frozen applicable sampling rule; a miss remains TARGET_MISS, not PASS from inner Commit time |
| Other times | Begin, Exec/create-write/finalization, Commit including acquisition/build/stage/publish, visibility, End, existing pure-call sum, full runner/setup/proof/cleanup wall |
| Placement | macOS Store/SDK/coordinator/construction/spool; Linux Docker daemon/FUSE/workload under existing 2-CPU/2-GiB/no-swap/256-PID profile |
| Qualification | Exact public dispatch, release binaries/image/fixture/harness identities, complete expected operation counts, independent reopen verification, ownership/resource and cleanup success |

The mode above differs from the internal diagnostic's 0600 creation mode; this
is an explicitly new public fixture, not a byte-for-byte identity claim for the
old test. Metadata normalization is real work. Existing wrapped syscall counts
omit its chmod/utimensat calls, so report normalization and actual transport
counts separately rather than claiming that counter counts every filesystem call.

Successful source-level counts, absent retries: 25,000 file opens, 25,000 one-byte
writes, 25,001 closes including the root descriptor, one root open, one root
fsyncdir, zero per-file fsync, and 25,001 metadata normalizations. The existing
wrapper therefore counts 75,003 calls; normalization adds 50,002 chmod/utimensat
calls. Freeze and assert these alongside exact file/byte counts; report attempted
and retried calls separately. Observe actual FUSE/host RPC counts rather than
equating them to POSIX counts.

Reuse existing complete-sample/seed/repetition and applicable paired-comparison
rules. Freeze exact applicability in the new case before collection; do not pick
statistics, repetitions or a tolerance after seeing results. Keep the inherited
`pure_call_sum_ns` definition unchanged. Benchmark-only expensive census/formatting
belongs outside the new contiguous timer; required engine work stays inside.
Moving required cleanup out of End is not an optimization. If existing semantics
permit deferred physical reclamation, report its bounded bytes and time through
settlement separately, and require successful cleanup before accepting evidence.

Independent verification enumerates all 25,000 paths from reopened committed
state, checks every byte/length/type/mode/mtime and exact count, root-directory
mode/mtime, branch outcome, and final cleanup. No full verification inside performance timing. Workload,
proof and cleanup retain separate deadlines and their existing failure rules.

First obtain a matched v0.1.5 result with the same workload, public operation,
oracle, topology, cache and enclosing timer. If compatible execution cannot be
established, label that comparison unavailable; retain old tiny-churn rows only
as historical context. A v0.1.5 miss does not waive #130's two-second objective.

## 5. Scaling and full-suite evidence

Reuse inherited tiny-create 100/500 and bulk-create 1,000/5,000 cases for affected
regression coverage; their payload/background differences make them unsuitable
as a single scaling curve. When diagnosis requires scaling evidence, use a
separately identified diagnostic mode of the new one-byte recipe, with cheap
cumulative counters at fixed prefixes such as 5k/10k/20k/25k and unchanged
directory/content/concurrency settings. Do not add a new matrix or repetition
schedule, run a full census at those points, or mix diagnostic timings into the
performance distribution. Keep only 25,000 as the new timed objective; smaller
prefixes do not replace final qualification.

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
applicable strict <1-second criterion is not replaced by this two-second target.
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
Do not manufacture a two-second PASS or relax an oracle to end the loop.
