# Complete cluster-two implementation plan

> **Status:** Current planning checklist; no release candidate exists.
> Prepared 2026-10-07, Asia/Singapore, after the completed Claude Code session
> **Codex progress and performance vs cluster one**. Reviewed source:
> local `main` `f0e9bcfd119118d8dcbbededd7c74f394a8d34c7`.

This is the execution map for finishing cluster two iteratively in the primary
checkout `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, with product code under `core/`.
It covers remaining S0 prerequisites and S7–S13. Creating this plan performs
documentation work; it does not execute those milestones or retire source.
When assigned for implementation, continue through useful independent packages
until the assigned scope is complete or an exact required external gate remains.

The [303 primary contracts](../303/README.md) own semantics and milestone exits.
This plan supersedes older next-work ordering and batch-only routing for a future
whole-plan assignment, while retaining their source-qualified evidence. The
[S7/S9 plan](IMPLEMENTATION-PLAN-S7-S9-20261006.md) retains detailed A/R/E contracts.
The [performance acceptance companion](PERFORMANCE-ACCEPTANCE-S7-S12-20261007.md)
owns this plan's criteria, comparison boundaries and registration obligations.
The [organization recommendation](SOURCE-ORGANIZATION-S7-S13.md) extends
[SOURCE-ORGANIZATION](SOURCE-ORGANIZATION.md); it selects organization, not algorithms.

## 1. Reconciled starting point

| Item | Verified state at planning |
| --- | --- |
| HEAD / tree | `f0e9bcfd119118d8dcbbededd7c74f394a8d34c7` / `71100d82ab71b665757be1875297f0116ad86abb` |
| Product subtree | `core/crates` = `e67f06e72239363ffb8614ed5cecc08d1e906474`, exactly equal to `4a207cea1:core/crates` |
| Restoration | `2fced797d` restored the selected incumbent; `6af972dc0` and `f0e9bcfd1` add documents/receipts |
| S5 / S6 | Completed S5 `a0dc7da9b`; S6 `983c2ee6d36a4417d8fff2d14db6b141f5386c8c`, tree `be2744223a450eaa01b9f31c4e3c850bbd141d72`, receipt/handoff `4ecea41983b673d62db90880b777a94565eac985` |
| Active membership | Eleven crates in [core/Cargo.toml](../../../Cargo.toml); FUSE, API-core and Sandbox remain excluded integration source |
| Production LOC baseline | Core 95,056; root reference 65,417; combined 160,473. Recompute exact snapshots for each new commit |
| Native dependency | Owner-authorized crates.io fuser 0.18.0 timestamp correction; Docker verification accepted. Retained Linux endpoint failure remains a platform limitation |
| Milestones | S1–S6 complete; S0/S7/S8/S9/S10/S11/S12/S13 incomplete |

The completed Claude session ended on 2026-10-07 local time with the
[restoration results](INCUMBENT-RESTORATION-RESULTS-20261007.md). All six history
pairs pass their approved speed/allocation gates. That driver bypasses native
Init and acquisition tables; it does not qualify acquisition, runtime or Commit.
All eight restored Init cases pass their scoped correctness/cache/cleanup checks
and fail both the 1.10× speed gate and strict allocation gate. The historical
9.749 s and later 10.693 s observations use the same executable; two observations
do not establish a variability distribution or justify choosing the smaller one.

Keep the restoration selected. Do not resume withdrawn payload-segment or
streaming-Init experiments, introduce another acquisition algorithm, or reopen
an optimization campaign merely because this planning document has a new commit.
Necessary source-driven corrections remain possible under the normal iteration
rules. Unresolved Init gates stay visible through final qualification.

Preserve the completed S5/S6 chat and [stopping record](HANDOFF-S7-S13.md), both
source-organization/Workspace-analysis side documents, the two separately owned
untracked resume/speed-plan documents, four unrelated containers, foreign
worktrees, reference source and all prior receipts. Reconcile again before edits.

### Current capability and gap ledger

| Scope | Implemented foundation to reuse | Remaining acceptance |
| --- | --- | --- |
| S7 | Original-job SQL/VM/binding/copy/queue observations; startup and wire accounting; allocation/high-water/freelist observations | E1–E4 complete resource/service gates and later native attribution |
| S9 acquisition | A1–A4 contracts/provider/Project port/custody corrections; symlinks, hard-link identity, bounded acquisition rows, removed 4 GiB refusal | Full payload/huge-root/dense >4 GiB proof, resource evidence and retained performance failures |
| S9 runtime | Authenticated handlers, scoped Saves/history, fair service, native logical framing, input/output fences and consumer ports | R1–R4 actual supervision/assembly, contextual admission and disconnect/restart custody; Q1 integration |
| S8 | S1–S6 filesystem/owner interfaces and accepted fuser correction | Actual replacement mount/dispatch/kernel lifetimes/cache coherence/control/Bash/teardown |
| S10 | Stable capture/read/operation custody, prepared install, cluster-one public constructors/Save/history | Backed construction prerequisites and complete captured-state pipeline |

Older handbook/303/audit paragraphs retain dated references to native symlink
refusal, a 4 GiB Init cap, input-sized scan collections or incomplete S6. Use
current source and appended completion records for status. Do not reinterpret
removal of the Init cap as a completed dense >4 GiB proof or as resolution of
the independent downstream Commit constraints.

### S0 prerequisite disposition

| Prerequisite | Current disposition / next owner |
| --- | --- |
| P1 semantic/context authority | Local grammar/reference/binding checks exist; R2 completes contextual child/topology/provenance acceptance |
| P2 Linux capability | Portable builds/proofs exist; qualify each newly assembled native package in F0/R4; global provider remains macOS-owned |
| P3 backed deferred editing | Open; K1 must replace growing Content edit state without merely raising its refusal |
| P4 sparse integration | Fresh `construct_runs` proved in S5; localized sparse edit and actual Commit remain K1/K3 |
| P5 cheap lengths | Existing owning facts, SDK delivery, Base/stat and remote port; prove actual mounted route in R4/F1 |
| P6 directory change streams | Open; K2 corrects per-directory resident Vec, not just outer row paging |
| P7 parent membership | Open; K2 supplies indexed backed construction membership |
| P8 engine algorithms | S1–S6 implemented; E2–E4/F3–F6/K5 finish integrated resource/native/lifetime evidence |
| P9 qualification | E1 registers routes/observers/numerical gates; Docker and family policies remain scoped |
| P10 unknown resolver | Terminal-unknown custody/fences required; any additional resolver policy remains an explicit unresolved contract gate |
| P11 accessor docs | `.0` correction complete; no new API work |
| P12 complete acquisition | A1–A4 delivered; Q1 full payload/huge/>4 GiB/resource/application proof and retained performance failures remain |
| P13 backed validation/release | Open; K2 fixes demanded/touched/zero/release collections and memory-derived total refusals |
| P14 incremental topology | Open; K2 supplies checked incremental evidence without whole-base validation at Commit or hidden at bind |

R2 may consume the same owning Content validation corrections as K2. Resolve
that dependency explicitly if contextual admission needs them; do not manufacture
S9 completion through a parallel SDK validator or claim those prerequisites are
solved by A3. The independent runtime/supervision work remains ready.

## 2. Binding contracts and implementation rules

Read [root AGENTS](../../../../AGENTS.md), [core AGENTS](../../../AGENTS.md), both
root handbooks ([cluster one](../../../../cluster_one_handbook.md),
[canonical mechanisms](../../../../cas_cdc_deltaencoding_handbook.md)), the
[seven primary contracts](../303/README.md#primary-design-documents),
[runtime integration](../303/06-cluster-one-integration.md),
[milestone validation](../303/07-implementation-validation.md) and
[optimization policy](../../../../docs/general/optimization-guide.md).

- One initialized host Store, embedded library owners and authenticated adapters;
  no revived Server. One separate disposable overlay SQLite database per daemon.
  Initial acquisition rows are provider-owned in the global Store; live Workspace
  and Commit scratch belong in the daemon overlay, with operation namespaces.
- Full roots include ignored files, dependencies, caches, output, symlinks and
  `.git/index`. No per-mount scan/copy/materialization/database creation or hidden
  dependency reinstall. Native input needs explicit acquisition once.
- Ordinary Bash has no automatic runtime/output-total limit, implicit Commit or
  unmount. Short and long commands, per-call and persistent Workspaces use the
  same supported filesystem. Process, pipe, descriptor and mapping lifetimes differ.
- Preserve permissions, stable inode identity, exact kernel request/open/lookup/
  reply custody, coherent caches and kernel writeback off. No acknowledgement
  before the owning local publication; lost replies do not erase published work.
- No whole Exec/Save/Commit holds a SQL owner or registry lock. Short atomic jobs,
  bounded service/admission and streamed backpressure must provide fair progress.
- Keep canonical formats and public APIs. Existing public constructors/readers
  own immutable trees; SQLite owns growing mutable indexes/membership/scratch.
  Add narrowly needed owning capabilities instead of custom parallel engines.
- One attempt; no retry/busy handler/reprepare/refresh/resend/re-stage or guessed
  success/deletion. Definite refusal, conflict, unknown and known publication with
  failed installation retain distinct state and original custody.
- No third-party modifications except the recorded fuser timestamp patch. Run
  the focused provenance check before native fuser builds. Do not require QEMU.
- No sync on disposable backing. Global Durable/Disposable guarantees stay scoped.
  Exact allocation, retained state and eligible debt are separate quantities.
- Keep `lib.rs`/`mod.rs` thin and at most 200 lines; other production files at
  most 999. Product tests/harnesses remain outside `src/`; runtime SQL counts as
  product. Add real modules only; no empty scaffold or alternate product path.

## 3. Iteration protocol and dependency order

Each package below is an implementation checkpoint, not a reason to end the
assigned batch. An agent should leave a working, reviewable source state and move
to the next ready package. Use subagents for independently owned research/review
or implementation when authorized; give file ownership, preserve others' edits,
and serialize shared-file changes and same-worktree builds/measurements.

1. Reconcile HEAD, working tree, ownership, active packages, tracker and receipts.
2. State the ordinary public route, input/output/custody, exact failure boundary,
   operation and cumulative cost model, affected tests and reusable evidence.
3. Implement one coherent behavior through real owners. Add adversarial public
   tests with bounded waits; do not add benchmark-only algorithms or test hooks.
4. Build locked with `--no-run` first. Every test invocation has an explicit wall
   ceiling of at most 120 s. Timeout is FAILED; diagnose source/output before any
   changed rerun. Never background or repeat-loop a test to evade the ceiling.
5. Run changed-scope checks and count diagnostics. SQL changes need both EXPLAIN
   and correlated execution profiles. Correct the demonstrated cause, not timing
   noise. Preserve failures and unavailable observations.
6. At a coherent final identity, run required covering proof once and register/run
   only the affected admissible performance selection. Reuse qualifying unchanged
   receipts. No new docs-only identity is permission to replay measurements.
7. Update source/API docs, separate milestone audits and an append-only receipt.
   Record exact first-parent/final-staged production LOC, commit locally, verify
   the committed tree against that receipt, then update tracker #307.

No CI or aggregate pre-push wrapper. Pushes, releases and deployments are outside
this plan's execution. Before stopping, report delivered packages, all failed/
unrun gates, exact source/tree/check identities and the next concrete ready work.
Only an actually hard required external/owner-contract gate blocks its dependent
work; unfinished coding, registration or instrumentation is not an external gate.

```text
T0 current-state reconciliation
  +-- E1 -> E2/E3/E4 independent engine evidence -----------+
  +-- R1 -> R2/R3 -> R4 -> Q1 real host/consumer readiness --+--> S8 native slices
  +-- K0 prerequisite designs -> K1/K2 component fixes -----+        |
                                                             S10 complete Commit
                                                                  |
                                                          S11 integration cleanup
                                                                  |
                                                          S12 final qualification
                                                                  |
                                                          S13 reference retirement
```

S7's native request/open/lookup/reply dimension needs S8. Close independent
engine evidence first and retain that exact residual gate; do not demand a
completed S8 merely to perform E1–E4, or omit native work to check S7 early.
Native component tests may start during S8; full-root product measurements need
the relevant S9 owners and fixture qualification. S10 component corrections can
precede the mounted pipeline without claiming an integrated Commit.

## 4. Next batch: finish S7 and S9 foundations

The existing A1–A4 acquisition work is retained. **T0, R1 and E1 are next ready.**
Update stale tracker/progress summaries with the latest restoration evidence,
then start actual supervision and measurement registration rather than another
general Init performance experiment.

| Package | Implementation and owners | Acceptance before dependent enablement |
| --- | --- | --- |
| T0 | Reconcile this plan, source and known regressions; keep existing milestone IDs | Exact baseline/active membership, preserved state and no stale completed-slice task restarted |
| R1 | SDK runtime/service supervisor plus client attachment; Bridge native input/output owner composition | Real sockets and initialized owners; blocked input/output independent; provider locks exclude I/O; original results/partial buffers stay charged until exact fence/release |
| R2 | Runtime contextual admission and consumer root binding | Peer/Workspace/Branch/root/Store/profile/scope/Save identity exact; role-derived references and child meanings checked; stale/cross-authority/wrong-root input refused before prohibited effects |
| R3 | Connection, consumer-process and host-runtime restart fences/custody | Queued/unattempted versus attempted/known/unknown states distinguished; old epochs/capabilities rejected; original work cannot execute after its claimed completion fence; no inferred crash recovery |
| R4 | API-core, Sandbox Docker owner and daemon upstream composition | Real macOS Store ↔ authenticated Linux consumer public path; existing Store reused; no SDK→Daemon dependency cycle, legacy fallback or whole-Save checkout |
| E1 | Existing core harness registry/families/shared observers; explicit case manifest | Every selected row has route, source/build/fixture/cache identity, timer/ack/cleanup scope, independent oracle, numeric gates, resource envelope and budget before sampling |
| E2 | Overlay/Daemon/SDK/Bridge/Telemetry observations on ordinary paths | Entire successful or failed operation accounted; worst/amortized/cumulative bounds match source, SQL plans and actual executions |
| E3 | External host/Docker resource observers plus supported provider data | Baseline/peak/final physical allocation, I/O and phase residency; aggregate owner domains included; unsupported exact counters remain explicit gaps or permitted conservative bounds |
| E4 | Registered concurrent owner/service/reclamation workloads | Every runnable class/Workspace progresses within its frozen service bound; capacity pressure is explicit; eligible debt drains automatically during live and idle phases |
| Q1 | Public Project/runtime/consumer proof on both Store profiles | Faithful complete-root oracle after native source removal; aliases/symlinks/raw targets; large dense and sparse files; huge namespace; two interleaved Saves, same-Save reads, history conflicts, exact disconnect outcomes |

R1 should compose the existing `Runtime`/`Sessions`/`Service`, native pools and
`Calls`/remote object-length-serial ports. `Calls` is one bounded serialized
exchange, not a pipelined multi-request channel. Separate connections must not
allow a slow peer to occupy unrelated service. Retain protected demand/control/
completion capacity and same-Save ordering while other Workspaces make progress.

R3 must distinguish process/session knowledge from durable Store contents. An
in-memory owner disappearing is not proof that an attempted history operation
did not publish. P10 supplies no automatic resolver. Implement terminal-unknown
custody and exact fences; if a required exit needs a new resolver policy, retain
that exact contract gate instead of inventing replay or reopening the Branch.

The unresolved Init performance criteria remain in the companion. Address a
specific cause when it blocks a required exit or has a justified corrective slice;
do not silently waive failures or require speculative Init redesign before R1/E1.

## 5. S8: native FUSE and ordinary lifecycle, tested as implemented

| Package | Concrete implementation | Required mounted proof |
| --- | --- | --- |
| F0 capability/build | Activate replacement FUSE and executable wiring with real dependencies; verify patched fuser provenance and exact native API reachability | Locked Linux build; actual Docker attach/read/detach; inventory required callbacks/cancellation types and unsupported capabilities before designing around them |
| F1 read path | Root binding, lookup/stat/readlink, bounded directory resume, open/read/release, cheap length facts and shared immutable client | Real canonical root through S9; stable serial/attributes; permissions; no full scan/copy/schema work at mount; complete ignored/dependency/index visibility |
| F2 mutations | Route create/mkdir/link/symlink/unlink/rmdir/rename/chmod/time, write/append/truncate through current Workspace operations | Atomic metadata/payload, overlapping appends, hard-link aliases, rename replacement, sparse/regrow zeros and independent native expected bytes |
| F3 ownership/dispatch | Exact native request/reply/open/lookup tokens, deferred readiness, interruption/cancellation and targeted FORGET/RELEASE | Two guarded-inode waiters do not block unrelated work; no contention EBUSY; exact once-use replies; lost reply preserves mutation; stale release/cancellation/unmount retains correct owners |
| F4 coherent caching | Selected TTL/KEEP_CACHE profile, stable identity and required invalidations; kernel-origin mmap handling | Attribute/data/name races, cached tails after truncate, aliases, dirty mmap/msync/retained mappings; no blanket invalidate on view-preserving capture/install |
| F5 daemon/API lifecycle | Registry, ready mount, status, ordinary Bash streams, concurrent calls, explicit cancel and terminal unmount | No lock across command/pipe/network lifetime; no automatic timeout/Commit/unmount; descendant/pipe/handle lifetimes distinguished; detach/join/logical close with automatic reclaim |
| F6 native resource/scale | Aggregate receive/reply/cache/lookup/thread/FD accounting; fresh mounts and persistent multi-call workload | Actual session buffers and copies counted, cheap individual disposal, full-tree and multiple-Workspace proof, saturated lifecycle progress; append native S7 evidence |

S8 implements Commit control ownership/readiness surfaces without a fake success
path. Real capture→Save→history→install is K3/K4 below. Test providers over real
content-built roots are allowed only in component tests, never as the measured
product arm or as a substitute for S9 readiness.

The selected starting profile is 60 s entry/attribute TTL, KEEP_CACHE, 128 KiB
requests, two receivers, background/congestion 1/1, `default_permissions` under
the configured non-root command identity, and kernel writeback off. Daemon
fsync/fsyncdir are compatibility no-ops with no backing sync or durability claim;
ordinary kernel write/mmap synchronization still needs its native proof.

Capability gaps are explicit F0 findings. The sole third-party exception is the
timestamp patch; do not extend it for a hidden INTERRUPT/batch-forget/receive API.
Resolve any mandatory unsupported capability through an allowed owning interface
or corrected dependency and its qualification under the dependency contract.
Continue independent FUSE/runtime work while retaining the precise unresolved
gate. Docker acceptance for timestamps does not assert every fuser API exists.
The researched pinned API supplies reply-send attempts, not kernel delivery
receipts; it does not expose an implemented public INTERRUPT callback, and the
batch-forget argument type is not publicly nameable. Use the supported individual
FORGET route with exact counts; distinguish explicit daemon lifecycle cancellation
from kernel INTERRUPT support. F0 must reconcile required interruption exits with
actual APIs before promising them. A new dependency/patch exception would need its
own owner decision; this plan does not grant one.

## 6. S10: canonical prerequisites and complete incremental Commit

| Package | Required correction / integration | Acceptance |
| --- | --- | --- |
| K0 inputs and failure protocol | Specify final-state normalization from captured readers, operation scratch, provenance, same-Save reads and phase ownership | No chronological FUSE log used as edits; every phase's refusal/conflict/uncertainty/known-install-failure disposition and release fence explicit |
| K1 file editing, P3/P4 | Back deferred draft/reference/detached/committed state; wire sparse runs into localized edit/Commit using existing content algorithms | No 8 MiB-derived total-edit refusal; no file-sized resident draft state; dense >4 GiB, fragmented and sparse changes preserve canonical compatibility and exact bytes without O(hole length) zero processing |
| K2 namespace, P6/P7/P13/P14 | Stream per-directory changes; back new-parent membership, validation/touched/zero/release state; incremental checked topology/reverse-binding evidence | Wide/deep/alias/cycle inputs preserve checks; tiny rename/Commit avoids whole-base walk, including hidden bind/setup walk; output-sized indexed cumulative work |
| K3 construction and Save | Captured file/namespace cursors, one constructor, runtime Save consumer and same-Save object reads | Children before parents, bounded in-flight bytes, exact base/provenance, original storage failure surfaced, SaveFinish once before Stage |
| K4 history and installation | Exact StageChanges/CommitStaged token protocol; prepared base and paired known local install | Committed/UpToDate/conflict distinct; installed root equals capture; later mutations and retained old-root reads survive; known publication plus failed local install reported honestly |
| K5 failure/concurrency/lifetime | Phase fences, definitive local resolution, terminal unknown, concurrent Workspaces and repeated same-mount Commit | No replay/rebase/guessed discard; one pending/unresolved Commit per Workspace; long writer and orphan log progress, bounded failure depth and actual cleanup debt |
| K6 full product oracle | Product mount→Bash→Commit→terminal unmount→fresh root/mount and historical readback | Full affected state survives without source directory access; same-Branch race has exact winner/conflict; no observer or proof repairs product state |

Paging only the caller's `PreparedRows` does not repair Content's internal
resident vectors/maps. K1/K2 must correct the owning implementations, retain
public paths and canonical compatibility, and extend neutral backing ports only
where needed. SQL storage alone does not make a whole-base validation incremental.
Daemon Commit scratch uses the existing local Overlay owner; do not route it
through Project's global acquisition schema or create a new database per Commit.

## 7. Apply optimization throughout implementation

An optimization is complete only when selected behavior, correctness, work counts
and resources are demonstrated on the ordinary product path. A lower isolated
statement count, smaller BLOB or diagnostic wall time is insufficient. The
[FUSE investigation](../303/fuse-optimization-investigation.md) and
[305 assessment](../303/05-fuse-assessment.md) provide hypotheses, not blanket
permission to enable every experimental flag.

| Mechanism | Disposition and owning work |
| --- | --- |
| One initialized DB/runtime; root bind without materialization | Required R4/F1; initialization and 256 MiB reservation still charged separately and amortized honestly |
| Shared immutable ObjectId cache and cheap attributes/length | Required R1/R2/F1; select/read back aggregate allowance, preserve authority, count copies and misses; no per-Workspace duplicate authoritative cache |
| Long TTL, KEEP_CACHE, stable stat identity and `.git/index` | Selected F4; permissions and coherence proof first; preserve inode serials through known install/remount |
| Bounded cells, no base-payload copy-up, sparse runs, cutoff shrink | Preserve S5/S6 mechanisms; K1 completes sparse edit integration; count partial edges, masks, journal/pages and deferred debt |
| Final-state capture and localized construction | Required K0–K5; no bulk snapshot copy, whole-prefix rewrite, historical replay or whole-base validation moved to setup |
| Indexed SQL, keyset/generation cursors, statement reuse | Required every changed SQL path; EXPLAIN + executions/visited rows/VM, including trigger and range-allocation overhead; no retry/reprepare fallback |
| Deferred requests and short fair SQL/runtime turns | Required R1/F3/F5/E4; provider waits and socket backpressure release shared service; protect completion/lifecycle capacity |
| Bounded directory resume and targeted FORGET/RELEASE | Required F1/F3/F6; avoid per-entry resident maps and repeated whole-owner collection |
| Automatic live/idle reclamation and independent orphans | Preserve S6, qualify E4/F6/K5; include held state, eligible debt, high-water allocation and last-owner gates |
| Negative entries, adaptive READDIRPLUS, cached directory/symlink replies | Conditional after first correct slice; register isolated permission-preserving comparison and invalidate/lookup-reference/teardown proof before selection |
| FLUSH elision, atomic truncate-open, larger requests/in-flight tuning | Conditional F3/F4/F6; qualify actual supported library/kernel route, per-open ordering and memory. Do not copy flags from a passthrough experiment |
| Handle-free open, copy_file_range/reflink or copy reduction | Conditional only with exact orphan/alias/snapshot/reply ownership and authentic API; no zero-copy or cheap-clone claim without actual byte/allocation evidence |
| Kernel writeback, permission removal, mutable passthrough bypassing capture, fixed CPU pinning | Excluded product treatments; historical diagnostics retain their evidence scope |
| Extra construction producers, canonical/Store format changes, another acquisition engine | Not selected as performance remedies by this plan |

The conditional review must also preserve these distinctions from the newer
kernel/request research:

- `FLUSH -> ENOSYS` with OPEN retained and `FOPEN_NOFLUSH` need different proofs;
  the latter bypasses kernel flush checks and cannot inherit the former's result.
- Directory caching applies to the same mount; fresh mounts do not inherit its
  kernel pages. Cached symlink targets need replacement/serial correctness.
- `ATOMIC_O_TRUNC` needs permission-before-effect and failure/size/cache ordering;
  negotiated request size must match both adapter and Workspace processing limits.
- Extra receivers, `clone_fd`, parallel-directory or background depth changes need
  deployment capability, fair service and aggregate buffer/stack proofs. The initial
  selected profile remains two receivers and background/congestion 1/1.
- A receiver's researched allocation is 16 MiB + 4 KiB independently of 128 KiB
  negotiation; session handshake buffers/stacks and concurrent mounts add costs.
  Inventory the actual build; this allocation is not a measured RSS claim.
- Whole immutable-file `copy_file_range` reuse is a separate conditional candidate
  from mutable/partial copies. Require independent destination inode identity,
  stable authorized source, overlap/append/holes and later mutation/Commit proofs.
  Do not replace a copy with a hard link or claim partial edits are zero-I/O.
- Gate disabled tracing before formatting and remove source-demonstrated redundant
  copies, while preserving exact credited owners and ordinary product paths.
- Splice/io_uring/DAX/native reflink and receive-session alternatives have no
  selected supported route merely because a flag or generic ioctl exists.
  Pending multi-request group commit also changes the selected atomicity boundary;
  it remains research and cannot acknowledge successful mutation publication at
  an inner savepoint before the outer COMMIT, including on disposable backing.

For conditional opportunities, record a justified adoption or deferral with source,
counts, correctness costs and affected workload. They are not all mandatory
features. Mandatory performance gates cannot be waived by deferring the fix that
would satisfy them. Finish selected correctness/resource work before tuning flags.

## 8. Issue 305 readiness and final qualification

| Checkpoint | What can run | What it cannot claim |
| --- | --- | --- |
| During F0–F4 | Focused real Docker kernel tests as each behavior exists | Whole-root product readiness or real Commit from a fixture provider |
| F1–F6 plus relevant R/Q1 | E01–E17 command paths through actual product; walks/Git/build/replay/churn; unchanged-root repeated mounts | Original B/C post-LayerFS-Commit proof before K4/K6 |
| K4/K6 with S8/S9 | Mutation→real LayerFS Commit→fresh-mount proof; repeated/concurrent Commit scenarios | Final S12 qualification from development diagnostics |
| S11 then S12 | Final registered product matrix, independently verified state/resources and required comparisons | Automatic PASS from completed reports with FAIL/NOT_RUN rows |

`git commit` inside E10 is ordinary Bash and can run during S8. The additional
LayerFS Commit/remount survival obligation requires S10. E18's cross-mount
`.git/index` state must come from legitimate earlier product work; persisting its
changed state uses real Commit. Do not copy/prime that state outside the product
and call it a cold product result. A separately declared unchanged-root identity
test has a different scope.

Retain the full #305 fixture identity and membership: 130,045 entries,
3,475,776,149 regular-file bytes and 10,070 symlinks; replay has 95,021 entries
and 2,126,509,110 bytes. See [Commit workload contract](../303/workspace-api/commit.md#10-workloads-and-cost-expectations).
Verify the prepared fixture's continued availability, platform suitability and
sealed manifest; do not silently reinstall Linux-native dependencies or shrink it.
Any necessary fixture change is a separate prospective identity, not the old case.

Reuse existing harness/family boundaries and eligible closed inputs. Register a
new real-product arm, not a revival of `core/experiment/` as the product. Native
and cached passthrough controls are explanatory comparisons where their semantics
match; neither can replace a real Commit/storage/resource baseline. Historical
#305 timings lacked residency proof and are not qualifying speed controls.
Preserve B/E10 FAILED, Stage C NOT_RUN and E09's nondeterministic-output gap.
Specify a deterministic independent E09 oracle before a new selection without
weakening content/exit/state checks. Retain full post-Commit proof scope.

## 9. S11–S13 and stopping boundary

S11 removes excluded predecessors and duplicate Init/host-construction/Server
routes only after actual replacement coverage. Update public exports, manifests,
architecture and tests together. Root `crates/` stays available for S12 explicitly
selected comparisons; no alias/source include/error-driven fallback is allowed.

S12 maps all seven families in [validation](../303/07-implementation-validation.md)
to active routes, reused exact receipts or prospectively registered successors.
Run final covering checks once at the selected source. Required correctness,
resource and numerical gates must actually pass; a complete report with failures
is a report completion, not product acceptance. Retained Init failures require a
source-qualified correction or an explicit owner disposition; this plan grants
no waiver. Do not infer broad speedup from history-only evidence.

S13 removes the legacy root source and obsolete manifest/build/test wiring after
S12 acceptance and the authorized retirement boundary. Preserve shared ARM64
build flags and immutable historical evidence/Git identities. Verify Core without
root dependencies. Record removal as legacy retirement, not measured algorithmic
simplification. No release/push/deployment is implied by local completion.

At each milestone append its separate audit and tracker receipt. At the final
boundary give exact commit/tree/product/build identities, per-commit LOC, full
gate disposition, preserved artifacts, support limits and any next-ready work.
Do not check S8 from the timestamp fix, S9 from authentication alone, S10 from
component construction, or S12 from a list of completed benchmark invocations.

## 10. Expected final source organization

This is the destination structure, not a claim that every module already exists.
The 11 active packages remain; FUSE, API-core and Sandbox become active only with
real implementation. Tests/examples/benches are outside each crate's `src/`.
Small adjacent responsibilities can share a file; retain stable public reexports.

```text
core/
├── Cargo.toml, Cargo.lock
├── crates/
│   ├── layerfs-content/src/
│   │   ├── contract/  object/
│   │   ├── file/{construction,cdc,mapping,edit}/
│   │   └── filesystem/{attributes,directory,inode,references,rows,sorted,validate}/
│   ├── layerfs-storage/src/{store,encoding,pack,port,read,save}/
│   │   └── port/acquisition/             neutral acquisition contract
│   ├── layerfs-history/src/contract/
│   ├── layerfs-persistence/
│   │   ├── src/{store,storage,backend,history,metadata,objects}/
│   │   │   ├── storage/acquisition/
│   │   │   └── backend/sqlite/acquisition/
│   │   └── sql/sqlite/                  global Store/history/acquisition SQL
│   ├── layerfs-project/src/import/       one native acquisition orchestrator
│   ├── layerfs-overlay/
│   │   ├── src/{contract,database,namespace,payload,lifetime,maintenance,diagnostics}/
│   │   └── sql/                        authoritative local runtime SQL
│   ├── layerfs-workspace/src/
│   │   ├── base/  workspace/  mutation/  ports/
│   │   ├── operations/{namespace,file}/
│   │   └── commit/                     capture/files/namespace/scratch/Save/history/install/outcome
│   ├── layerfs-fuse/src/
│   │   └── {mount,requests,dispatch,ownership,coherence,diagnostics}/
│   ├── layerfs-daemon/src/
│   │   ├── bin/layerfs-daemon.rs        thin executable
│   │   └── {service,overlay,registry,lifecycle,execution,control,upstream}/
│   ├── layerfs-bridge/src/{contract,codec,native}/
│   ├── layerfs-api/
│   │   ├── core/src/contract/
│   │   └── sdk/src/
│   │       ├── client/                 calls, attachment and consumer ownership
│   │       └── runtime/                owner, sessions, binding, handlers, service/supervision
│   ├── layerfs-sandbox/src/{owner,backend/docker,session,lifecycle}/
│   └── layerfs-telemetry/src/{timer,output,runtime,platform}/
├── benchmark/fs-bench-pro/{families,registry,shared,diagnostics}/
├── tools/                              scoped development/provenance guards
├── docs/architecture/                  actual source/API boundaries
├── docs/issues/307/                    milestone audits, plans and append-only checks
└── vendor/fuser-0.18.0/                 sole authorized third-party exception

root .cargo/config.toml                  retained shared ARM64 inputs
root crates/                            retained through S12; retired only in S13
```

Preserve crate boundaries: FUSE converts native requests, Workspace decides
filesystem/Commit semantics, Overlay owns local SQL, Daemon owns process/service
lifecycle, Bridge owns authenticated wire, SDK owns host/consumer adapters,
Persistence owns the global SQLite engine, and Project owns initial traversal.
Content remains canonical algorithms; Storage owns encoding/Save; History owns
publication semantics; Sandbox owns Docker lifecycle; API-core owns public types.
No second mutable inode tree, global mirror, generic coordinator or per-operation
database is added to make the folder tree look complete.

## 11. Research and plan verification

The main author read the completed Claude session and reconciled source, both
handbooks, operation contracts, audits and the restoration reports. Three
independent subagent reviews cover (1) FUSE/#305 and optimization disposition,
(2) runtime/Commit/source organization, and (3) performance/resource criteria.
Their findings are integrated into this plan and its companion; primary sources
remain authoritative. Planning changes receive link/anchor, whitespace, source
identity and preserved-state checks. Rust/runtime tests and new measurements
are inapplicable to a documents-only change.

Every subsequent commit uses unchanged `tools/production_loc.py`, SHA-256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`, on exact
first-parent and staged trees, including shipped SQL and application adapters.
Verify the committed tree against the receipt. Report separate Core/reference/
combined before/after/signed deltas even for documentation-only commits.

Planning checkpoint comparison: **Production LOC: 160473 -> 160473 (delta +0)**;
Core 95056 -> 95056 (+0), reference 65417 -> 65417 (+0). Exact parent and staged
archives use the counter/method above. The [research review](checks/completion-plan-20261007/research-review.json)
and [LOC receipt](checks/completion-plan-20261007/production-loc.json) preserve
the baseline, protected-document hashes and review corrections. Final staged and
committed-tree verification is retained separately as described by the receipt.
