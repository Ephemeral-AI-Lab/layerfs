# Cluster-two performance and resource acceptance

> **Status:** Current planning checklist; no release candidate exists.
> Prepared 2026-10-07 against local `main` `f0e9bcfd1`.
> Companion to the [complete implementation plan](IMPLEMENTATION-PLAN-S7-S13-20261007.md).

These criteria govern E1–E4 and their S8/S10/S12 extensions. They distinguish
existing numerical gates, binding structural/correctness requirements, and
prospective numbers that must be registered before sampling. This document runs
no measurement and changes no existing verdict or budget.

## 1. Comparison identities and retained outcomes

| Comparison | Correct control / scope | Current disposition |
| --- | --- | --- |
| Restored native Project Init | Actual cluster-one-end public Project `197d2fb7d0a141d7a9350852022febeec3255bf2`, same Durable/Disposable profile, same fixture and acknowledgement | Eight speed FAILs and eight strict allocation FAILs; scoped proof/source-cold/cleanup/budget checks PASS |
| Current history strides | `7edddbdb8` reference against `6af972dc0` candidate, exact registered reference topology and candidate profile disclosed | Six pairs PASS approved speed/allocation; twelve arms PASS scoped proof/cache/cleanup |
| Old Phase 4.5 Service Init | `7edddbdb8`, split Store/history MEMORY/OFF, no Project crate | Separate historical competitive context; never label as cluster-one-end Project or matched Durable |
| #305 Native/A1/A2/B and follow-ups | Historical experiment, passthrough/prototype, no residency-qualified speed control | Diagnostic timing only; B/E10 post-Commit proof FAILED; CP5/CP6 incomplete; C scenarios NOT_RUN |
| New mounted product | Actual S8/S9/S10 source, supported profiles, ordinary SDK/Bash/FUSE path | Not yet available; new prospective registration and qualifying control evidence required |

Source/result identities and all eight/six rows remain in the
[restored results](INCUMBENT-RESTORATION-RESULTS-20261007.md) and linked ledger.
History creates a Store without acquisition tables and bypasses Project Init.
It measures shared Storage/Persistence and Content/Save/history work, not native
acquisition, authenticated delivery, mounted execution or Workspace Commit.
Original ineligible cache attempts, broad harness-test failures, withdrawn
treatments and unrun selections remain part of the record.

Use the selected incumbent as the starting product. A fresh commit/date alone
does not justify resampling unchanged behavior. Reuse exact qualifying source,
compilation, binary, harness, fixture and observer evidence. A later correction
needs a changed mechanism or another explicitly authorized selection; no best-of.

## 2. Existing numerical gates: preserve their exact scope

| Registered family | Speed criterion | Storage criterion | Existing performance / independent proof limits |
| --- | --- | --- | --- |
| Incumbent-restored Init, both profiles, 100/1,000/10,000/100,000 files | `10 * candidate_ns <= 11 * same_profile_control_ns` | Final allocated database + WAL + SHM bytes `<= same_profile_control_bytes` | 30 s / 19 s |
| History stride 10/3/1 | `10 * candidate_ns <= 11 * matched_reference_ns` | `<= 54,278,964 / 70,427,034 / 92,342,273 B` respectively | Durable 120/340/600 s and proofs 24/24/60 s; Disposable 60/170/300 s and proofs 12/12/30 s; existing family-only authorization |
| Retained nine-cell Workspace write matrix | Frozen append/dispersed/repeated criteria, authentic complete-command route | Owning family accounting and cleanup gate | 100/512-write command 15 s; 4,097-write command 25 s; proof 9 s. A replacement route requires its own registered successor |

History's approved allocation ceilings are distinct from its stricter original
missed storage targets. Do not relabel those older targets PASS. Init's strict
allocation gate has no implicit tolerance. No new universal 10% allowance for
all cluster-two operations follows from these two component families.

The latest Init regression is +15.432% to +38.424% by row. Durable100,000 is
10,692,601,500 ns against 7,724,523,333 ns; its permitted integer ceiling is
8,496,975,666 ns. These failures are outstanding criteria, not a new baseline
that erases the control. No new source/layout/profile selection is made here.

The #305 **100 ms fixed-cost, 2× native and 15 s absolute lines are reporting
lines**, not existing product PASS gates. Carry them as explicitly labeled
comparative targets when meaningful; report actual overages. Promoting any into
a new binding acceptance gate requires a prospectively specified operation,
profile, acknowledgement, complete cost boundary and admissible budget.

## 3. Required structural and correctness performance criteria

These are mandatory even when a timing is faster. Measurements account actual
logarithmic index depth, bytes and necessary topology rather than assuming every
bounded request is constant-time.

| ID | Authentic operation | Acceptance criterion | Required observation |
| --- | --- | --- | --- |
| G01 | Repeated Workspace bind | Zero new overlay DB/schema initialization, zero whole-root scan/copy/materialization, zero dependency restoration | Initialization, traversed entries, provider demand and physical I/O; startup reservation reported separately |
| G02 | Local ordinary write | Zero inherited base-payload copy-up and zero global Save/history operations; only bounded affected cells/metadata | Base demands separated into metadata/payload; full SQL/publication/reply-attempt trace |
| G03 | Payload locality | For W>0 request bytes, at most `ceil((W+4095)/4096)` intersecting cells; prior byte-fragmentation does not add cells; partial edges and staircase seeks included | 65,536 prior one-byte writes versus equivalent unfragmented window; aligned/unaligned and stale-edge shapes |
| G04 | Metadata/cursors | Indexed point `O(log N)`; covering keyset `O(log N+K)` with noncovering fetches charged; zero growing OFFSET or unrelated-population scan | Exact EXPLAIN, binds, returned/visited rows, VM, index/fullscan/sort work at declared populations |
| G05 | Capture/install | Zero payload/changed-row-sized snapshot copy; stable captured EOF independent of arriving active keys; no mandatory whole-base install scan | All source/capture/install jobs, pending frontier wait, bytes and subsequent replay passes |
| G06 | Failure/orphan history | Live namespace layers <=2; orphan <=3 while migrating then1; no current read-depth growth solely with prior Commit count | Repeated known success/failure, held snapshots and open-unlinked descriptor; exact data/owner checks |
| G07 | Truncate/reclaim | Foreground work follows boundary/staircase, not discarded length; cumulative cleanup follows eligible output, never whole-state scan per target | Discarded-size and target-population scaling; stale epoch checks, actual cleanup bytes/rows/debt |
| G08 | Sparse construction/Commit | No processing proportional to unwritten logical zero length; account real data, boundaries, mapping height and object emission | Fresh runs plus localized sparse Commit; independent boundary/interior readback |
| G09 | Namespace Commit | No whole-directory resident changes or total operation cap from ordering memory; tiny rename does not require whole-base topology traversal | Wide directories/new parents and growing unchanged base with alias/cycle validation intact |
| G10 | Fairness | Runnable unrelated work progresses with two guarded-inode waiters and with two Saves plus continuous reads; lifecycle/release/Finish not starved | Frozen per-class service-turn/wall limits, worker occupancy, offered/admitted/refused/attempted/completed counts |
| G11 | Native ownership | Credits precede owned request/reply copies; exact lookup decrement/open release; zero whole-state collection per FORGET | Kernel requests, native buffers, retained replies, original send attempts and disposal SQL |
| G12 | Automatic maintenance | Eligible work progresses live and idle without status/mutation/manual cleanup calls; held custody not called reclaimable | Debt production/service/drain and headroom; last-owner release and no later product call |
| G13 | One-attempt outcomes | Zero prohibited retry/refresh/resend/re-stage/fallback; refusal/conflict/unknown/known-published-install-failure distinct | Attempt receipts and failure/rollback/cancellation/restart fences; no guessed zero for unavailable counters |
| G14 | Root/cache/identity | Exact canonical/authority-qualified reuse; unchanged stable stat identity; permissions and alias/coherence intact | Real two-call/remount and same-mount tests, `.git/index`, kernel/base cache misses/hits and invalidations |
| G15 | Storage/resource honesty | Entire 268,435,456 B daemon reservation and allocation high-water counted; aggregate buffers, pager/journal/kernel/file cache and held/eligible state accounted | Physical allocated B, logical pages, committed freelist, every trigger/freelist/identity/range-call cost; requested range volume is not newly consumed disk |

S5/S6 count proofs are useful source-qualified anchors: equal later overwrite
work after 65,536 prior writes; equal shrink work for 2 and 2,048 discarded cells;
equal last-release work for 128/1,024/4,096 targets. Their old VM/statement totals
are not universal ceilings after accounting or native integration changes. Reuse
unchanged mechanism evidence and qualify new complete-operation costs honestly.

Initial proposed real-consumer cases may additionally require exactly one
1,024-serial reservation for 1,000 sequential successful creates from a known
empty allocation range, and zero extra provider reads for later exact immutable
hits retained inside the same charged workload. Freeze the initial state and
exceptions before sampling; these are not promises for concurrent/refused traces.
Never prime a cache in setup and claim its subsequent hits as cold acquisition.

## 4. E1 must turn criteria into executable numerical gates

There is no source-supported universal millisecond, RSS or MB/s limit covering
every cluster-two deployment. A complete plan therefore makes choosing and
registering these numbers a required implementation deliverable; it does not
fabricate them or leave PASS to visual judgement after results arrive.

For each selected case, freeze the following fields in an owning specification
and executable registration **before the candidate sample**:

| Field group | Required numerical values / checks |
| --- | --- |
| Route/workload | Entry counts, byte sizes, edit positions, number of callers/Workspaces/Commits, duration/arrival trace, read/write/Save/cleanup mix and exact fixture hash |
| Latency | End-to-end deadline; each separately gated operation/queue/service/resource wait bound; acknowledgement/drain boundaries; matched relative formula where applicable |
| Service | Offered operations/bytes per second, per-class minimum completed service, maximum runnable wait, admission/refusal envelope, observation horizon and final drain deadline |
| Resident capacity | Separate host and Docker aggregate byte ceilings, baseline/peak/final accounting, allowed cross-size spread; all simultaneously live owners included |
| Physical capacity | Initial/full reservation, peak/final allocation, required protected headroom, maximum eligible debt and source-derived write/read/copy amplification envelope |
| Mechanism counts | Allowed requests/statements/visited rows/bytes/copies as a function of declared workload; forbidden scans/setup/retry counts; source-qualified constant/index-depth assumptions |
| Observer precision | Sampling interval, maximum observation gap, missing-boundary policy, clock error, inclusive versus exclusive spans and measured observer overhead |
| Verification | Independent state oracle, exact supported metadata, alias/link/byte coverage, old/new root checks, custody/cleanup predicates and separate wall limit |
| Controls | Exact source/build/profile/topology/cache/workload match and immutable eligible receipt reuse, or a prospectively registered control selection |

Derive deterministic limits from the actual mechanism/resource contract where
possible. If a deployment-dependent numeric constant needs a baseline, collect
or reuse an eligible control under a predeclared calibration contract before
candidate treatment; then freeze its formula. Do not set limits from a favorable
candidate observation or add a new control run to excuse a failed candidate.
Missing required values/observers leave admission INCOMPLETE, with the exact
registration task identified. No universal bound follows from `cache_size` or a
queue capacity alone.

For comparisons, keep separate questions: nonregression of equivalent product
operations; overhead against a Native/A2 filesystem command; and total product
cost including Commit/history/cleanup. A2 has no real canonical host acquisition
or Commit. A claim of beating A2 requires eligible `candidate_ns < A2_ns` for the
same named command boundary and supported semantics, but it is not the universal
definition of product acceptance.

The two untracked side documents [resume](HANDOFF-S7-S9-RESUME-20261006.md) and
[speed draft](S7-S9-SPEED-TEST-PLAN.md) are preserved. Reconcile the draft's 27
E/R/Q/P proposals into existing `core/benchmark/fs-bench-pro` registry/families and
shared observers. They are proposals, not implemented or sampled registrations.
Do not create another generic campaign runner or benchmark-selected product path.

## 5. Complete accounting and resource observation

E2 follows caller input, admission, provider and SQL work, original completion,
reply attempt, caller-held result, release and later cleanup. Record original
attempts/failures and rollback work. Include count triggers, repeated SQL inside
loops, BLOB binds/returns, decoded/encoded/clone/zero buffers, transport grants/
fragments/authentication, native socket attempts and deferred bytes.

E3 records phase baseline/peak/final across the host Store/runtime and Docker
daemon/kernel separately. Include SQLite pager, rollback journal/dirty state,
OS backing cache, native receive/reply buffers, thread stacks, kernel inode/dentry/
file pages, constructed objects, Save/decoder windows and blocked producers.
Pinned fuser receive-buffer allocation is independent of negotiated 128 KiB
requests; inventory actual per-session buffers and concurrent mounts. Allocation
is not RSS, a sampled peak is not a proven maximum, and a lifetime high-water is
not a phase peak. The 256 MiB reservation is backing allocation, not resident RAM.

Use safe driver APIs and supported external OS/device observations. No third-party
instrumentation patch or invented zero. Where an exact required dimension is
unavailable, retain INCOMPLETE unless its owning contract permits an explicitly
derived conservative bound. That limitation does not stop independent dimensions.

E4 separates offered, admitted, refused, attempted, completed, delivered,
cancelled and retained work. Count every ready class and Workspace. Distinguish
queue wait from device/provider wait and from an intentionally held reader.
Publish eligible-debt growth/service and final automatic drain, plus held/live
state. Prove finite workload completion without deleting retained owners or
invoking a benchmark-only maintenance pump. Shared-writer fairness does not imply
unlimited throughput; overload must produce the declared backpressure/refusal.

## 6. Staged workload matrix and optimization comparisons

| Family / group | Required coverage and earliest complete route |
| --- | --- |
| F1 native Init | 100/1,000/10,000/100,000, both profiles; keep all current misses; Q1 adds complete-byte/huge/>4 GiB/source-removal/runtime acceptance |
| F2 history | Strides 10/3/1, both profiles; reuse latest six pairs if exact qualification scope unchanged; acquisition remains outside their route |
| F3 Workspace writes | Nine append/dispersed/repeated cases; prospectively register reopened 10,240-write successor and required 100,000-write functional/structural coverage; S8 write/ownership proofs, complete nine-cell lifecycle at S10 |
| F4 Commit | Clean/no-op, small/localized, fragmented, large/sparse, retained reads, conflict/failure/uncertain/later-writer cases; S10 |
| F5 namespace | Lookup/list/rename, wide/deep, aliases/cycles, captured EOF and incremental topology; S8 ordinary mutations then S10 Commit |
| F6 mutations | Mixed base/captured/active bytes, append, truncate/regrow, links, metadata, permission/cache races and release; S8/S10 |
| F7 shell/package | Ordinary Bash and Linux-compatible tools, short/long commands, output backpressure, descendant custody, explicit teardown; S8/S9 then S10 survival |
| #305 E01–E17/E18 | Full registered product command routes and stable identity; no filtered tree; Git commit command available at S8, LayerFS survival/remount proof at S10 |
| #305 C01–C08 | Per-call and retained mounts, 100 small Commits/change-size, writer during Commit, two Workspaces/four calls, resource limits/refusal/teardown; full original survival at S10 |
| Sustained pressure | Repeated known success/failure, orphan log, cold misses, slow sockets, saturation/device-full, live/idle cleanup and lifetime scaling; E4/F6/K5 then S12 |

Original experiment concurrency values are fixture settings, not total product
Workspace/Exec caps. Full affected state, ignored data, `.git/index`, dependencies,
caches/output, symlinks and hard-link aliases must survive through real product
history. Independent expected state cannot be derived from candidate output.
Retained direct Project Init and history receipts qualify their original component
routes. A newly assembled SDK facade or integrated lifecycle needs its own
registered route proof; reuse unchanged component evidence without promoting it.

For each conditional optimization from the master plan, isolate its mechanism
prospectively on the permission-preserving product. Record removed versus added
requests, attributes, lookup references, copies, allocation and cleanup; compare
the same complete route. A READDIRPLUS win that increases teardown or a storage
win with severe latency loss is not an automatic improvement. Reject about 50%
speed loss for about 5% storage benefit; disclose both raw deltas. Do not promote
no-permission, pinning, handle-free or passthrough diagnostics into product gates.

## 7. Execution, cache and reporting rules

The [measurement policy](../../../../docs/general/agent-measurement-policy.md),
[benchmark rules](../../../../docs/general/benchmark_rules.md),
[report template](../../../../benchmark_agent_report.md) and
[core harness guide](../../../benchmark/fs-bench-pro/AGENTS.md) remain binding.

- Build first, locked and with repository ARM64 inputs. Export
  `LAYERFS_CONSTRUCTION_WORKERS=1`; only native Init retains its separate supported
  constructor profile. No extra Commit producer to pass a gate.
- Every test has explicit wall timeout <=120 s; timeout is FAILED and requires
  source/output diagnosis. No background/repeat test loop. New performance default
  <=15 s, declared exceptions <=25 s, independent proof <10 s; existing owner-
  approved larger family limits retain only their exact family/profile scope.
- The old #305 60/600 s amendments do not transfer automatically. If a full
  workload cannot fit the admissible budget, retain NOT_RUN and its exact budget
  conflict or obtain a prospective owning exception before selection. Do not
  shrink the workload or use a warm repeat. Product Bash itself has no automatic
  lifetime timeout.
- Reuse closed prepared inputs/sealed builds and eligible proof receipts. Use
  `--setup clone` where supported for post-initialization cases; fresh for Init.
  A clone is neither a cold attestation nor permission to omit timed work.
- Attest each named cache domain at each measured phase boundary. Setup/earlier
  phases/own writes cannot supply uncharged warmth. Persistent-workspace reuse
  is a separately declared workload that pays its initial acquisition; it does
  not relabel later warm phases cold. Unknown/mismatched cache is INELIGIBLE.
- Fresh append-only outputs; one sample per case/arm, no best-of, no cosmetic
  identity replay. Separate diagnostic instrumentation from final measurements.
  Inspect retained source/count causes before another treatment.
- Worktree-local targets/locks, no same-worktree overlap; declare host/Docker
  interference and preserve unrelated containers. Setup and observer costs cannot
  be moved out of the timer when they are product work.
- Record complete command, mount, execution/output drain, capture, construction,
  Save, history, install, native teardown, logical close and eventual cleanup.
  Do not add overlapping spans as an exclusive wall decomposition.
- Every row retains source/tree/product/compilation/dependency/binary/image/
  harness/workload/cache identities, raw output, exact arithmetic, command,
  budget and PASS/FAIL/INELIGIBLE/INCOMPLETE/NOT_RUN reason. Unsupported observations
  are visible. Reuse receipts by exact unchanged scope rather than broad labels.

## 8. Completion decision

An iteration may finish a component checkpoint with retained failures and proceed
to independent work. S7/S9/S8/S10 completion requires their own exits, including
named dependent dimensions when available. S12 requires every mandatory numerical,
functional, authority, custody, resource and cleanup gate to pass, or an explicit
owner disposition that leaves the historical failure intact and states its
scope/remaining risk. This plan supplies no waiver or permission to shrink scope.

In particular, current Init failures cannot disappear because history passes,
native FUSE looks faster, or the report is complete. Complete evidence coverage
and successful product acceptance are different results. S13 retirement follows
actual S12 acceptance; release/deployment approval remains outside this plan.
