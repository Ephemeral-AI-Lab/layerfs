# Current cluster-two rollout ledger R0–R9

> **Status:** Current planning checklist; no release candidate exists.
> Fresh owner dispatch 2026-10-08; input `1a6bb53ef14e1860d8f222df11394e5a654bb34d`.
> Owner stop boundary: finish verified R1 and its FUSE handoff, then pause.
> Full R0–R9 objective remains uncompleted; source/proof identity governs claims.

Later dispatch2026-10-08 resumes full R2–R5 in the primary checkout. The earlier
R1 stop boundary above is historical. The owner subsequently authorized the
[scoped fuser lifecycle extension](FUSER-LIFECYCLE-DECISION-20261008.md); no
dependency approval remains pending for that scope.

[Rollout contract](CLUSTER-TWO-LOC-AND-ROLLOUT-20261008.md#4-combined-implementation-rollout)
and [complete proposed layout](FINAL-CLUSTER-TWO-FILE-LAYOUT-20261008.md) govern
implementation. This ledger records actual state, not promised acceptance.

| Checkpoint | State | Evidence/remaining work |
| --- | --- | --- |
| R0 contract/proof/guide/layout/dispatch reconciliation | COMPLETE — documentation scope only | [Deepest-file plan](checks/r0-owner-reconciliation-20261008/01-deepest-file-plan.md), [owner/proof disposition](checks/r0-owner-reconciliation-20261008/03-owner-and-proof-ledger.md); [independent review](checks/r0-owner-reconciliation-20261008/06-independent-review-disposition.md) and [scoped document verification](checks/r0-owner-reconciliation-20261008/07-verification-scope.md); exact staged LOC/identity recorded in checkpoint receipts; no native product claim |
| R1 real SDK and ordinary Sandbox lifecycle/execution/access | COMPLETE — owner-selected R1 scope | [Deepest-file plan](checks/r1-sdk-sandbox-20261008/01-deepest-file-plan.md); real Project/Workspace facade and [daemon application checkpoint](checks/r1-daemon-composition-20261008/65-application-results.md) verified; [ordinary runtime foundation](checks/r1-sandbox-runtime-20261008/81-foundation-results.md) now verified; [owned lifecycle/SDK startup/access](checks/r1-sandbox-lifecycle-20261008/94-final-native-lifecycle-proof.txt) now verified; [final no-admin verification and R2 handoff](R1-COMPLETE-FUSE-HANDOFF-20261008.md) closes only the rollout R1 requirements. Optional administrative cancellation/provenance is owner-deferred; native Ready belongs to R2. Old excluded Sandbox cannot activate unchanged |
| R2 real native Ready/read/permissions/indexed custody/normal drain | IMPLEMENTED — functionally verified on real mounts at the declared scope; owner acceptance pending. FP-9, native FP-8 and fusectl agreement unrun; FP-17/21/31 partial; no resource or timing claim | [Completion record](R2-COMPLETION-20261008.md) and [checks](checks/r2-completion-20261008/00-results.md), including two failed 9 s topology attempts; [Initial source inspection and plan](R2-NATIVE-PREREQUISITE-20261008.md); [Pending Future component](checks/r2-completion-future-20261008/20-results.md); [admission and worker-exit component](checks/r2-admission-future-20261008/36-results.md); [resumable mutations](checks/r2-mutation-plan-20261008/18-results.md); [Store reader service](checks/r2-store-read-service-20261008/45-results.md); [fuser lifecycle checkpoint](R2-FUSER-LIFECYCLE-20261008.md). Bound is not Ready; Ready is the separate Attach acknowledgement. Archived native draft remains unqualified and unapplied |
| R3 ordinary mutation/kernel coherence | IMPLEMENTED — functionally verified on real mounts at the declared scope; owner acceptance pending. FP-16 fixture half unrun; FP-15 and FP-29 partial; three host test binaries fail on an owner-credit timing race in untouched code; no resource or timing claim | [Completion record](R3-COMPLETION-20261008.md) and [checks](checks/r3-mutation-20261008/00-results.md); architecture [77](../../architecture/77-native-mutation-coherence.md). Five decisions await owner review, including one reversal of an R2 engine rule |
| R4 captured namespace and incremental topology | IMPLEMENTED — functionally verified at the declared scope on host and Linux; owner acceptance pending. R4-6 partial (sorted-merge sibling reads grow with the base); R4-3 row 6 partial (no real dirty mapped pages); R4-2 alias and cycle refusals proven at producer scope, not through the driver; no product caller; no resource or timing claim | [Completion record](R4-COMPLETION-20261008.md) and [checks](checks/r4-captured-namespace-20261008/01-deepest-file-plan.md); architecture [78](../../architecture/78-captured-namespace-construction.md); earlier [page port component](checks/r4-captured-namespace-port-20261008/19-results.md). Fifteen decisions await owner review. The producer is reached by the unchanged Commit driver only in a daemon test |
| R5 actual mounted live Commit/known install/survival | IN_PROGRESS — track S (product constructor, +226 production lines) and track M (mounted rig; R5-1, R5-2 mounted part, R5-3, R5-8, R5-9 pass on Linux) committed; track K (R5-2 overwrite, R5-8 on one open Store, R5-10, R5-11 pass on host and Linux) and fix S2 committed: K found that a settled refusal could not release its operation owner because held release completions kept both Lifecycle credits; S2 drops a done release's completion, and every Commit-thread job and captured port call now waits for admission (+34 production lines); tracks N (R5-4/5) and F (R5-6/7) running; next: review their receipts, commit, fresh-context review, final suites, completion record | [Deepest-file plan](checks/r5-mounted-commit-20261009/01-deepest-file-plan.md) with Amendment 1; architecture [79](../../architecture/79-product-commit.md); assignment in the [R5–R9 handoff](HANDOFF-R5-R9-20261009.md). Carried to R6 from [the audit](checks/r6-concurrency-teardown-20261009/00-audit-findings.md): install can deadlock on owner slots held by fenced source jobs under 16 or more concurrent requests (H-A), and install has no bound behind a retained request (H-C). H-B (Commit refused instead of waiting) is fixed by S2 |
| R6 several Workspaces/processes, sustained cleanup, forced FS drain | NOT_STARTED | Component finite-arrival/cleanup proofs retain their exact scope; native integrated route remains |
| R7 covered excluded core predecessor/integration/Server retirement | NOT_STARTED | Root reference retained; no new source retirement in R0 |
| R8 frozen integrated registered qualification/acceptance | NOT_STARTED | Existing failures/ineligible/unrun/owner-deferred items preserved; no new numerical gate invented |
| R9 conditional root-reference/wiring retirement and independence | NOT_STARTED | Authorized only after R8 acceptance and replacement/dependency coverage; no early retirement |

Original reusable [pre-S8 evidence](PRE-S8-COMPLETION-20261007.md) and
[resource-growth scope](PRE-S8-RESOURCE-GROWTH-RESULTS-20261007.md) retain their
identities. E04 is closed and never reopened. Historical WAL speed/storage
failures, incomplete phase attribution, fractional signed-minimum platform FAIL,
>4GiB execution waiver, original unknown custody and owner-deferred numerical
acceptance remain explicit. P-1–P-7/overwrite/refill/profile rulings govern exact
scope; no component proof establishes a native mount/live namespace Commit.

R0 input source: core/crates tree `611c3a5387cffc73816ef0358d8b8b285c8b52cb`,
root reference tree `498dd1917812ae90efb8841f57e22bfc284e96fb`. Exact baseline
production LOC: combined165813; core100396; active57522; excluded predecessors
36325; excluded integration6549; reference65417. Use the pinned root counter over
exact first-parent/final staged/committed trees for every commit. Checkpoint
commit/LOC receipts are appended here as they are completed; no estimates enter
commit accounting.

## Local checkpoint commits

| Commit | Checkpoint scope | Exact production LOC |
| --- | --- | --- |
| `eb9b06c4c8633053e9b1cbf5f545d141b88915fc` | R0 documentation reconciliation only, no native product claim | 165813→165813 (delta+0); core100396/active57522/reference65417/predecessor36325/integration6549 unchanged |
| `932abe80894e070d4767eba2c213dc5d38a64d8f` | R1a real SDK Project/Workspace component facades | 165813→166022 (delta+209); core100605/active57731; reference/predecessor/integration unchanged |

R0 [commit confirmation](checks/r0-owner-reconciliation-20261008/10-commit-confirmation.json)
confirms exact first parent, final staged/committed tree and counted product trees.

R1a real Project/Workspace component facade subcheckpoint is verified; R1 remains
IN_PROGRESS until actual Sandbox/runtime/daemon-ready/access scope. Final
[results](checks/r1-sdk-sandbox-20261008/19-component-results.md) and
[exact LOC](checks/r1-sdk-sandbox-20261008/21-exact-production-loc.json) retain
all original and final receipts. Production LOC: 165813→166022 (delta +209); active 57522→57731; core 100396→100605; reference/excluded subtotals unchanged.

R1b implements and verifies the protected-config daemon application and direct
Store startup/control route. [Results](checks/r1-daemon-composition-20261008/65-application-results.md),
[independent review disposition](checks/r1-daemon-composition-20261008/64-review-disposition.md)
and [exact source-size comparison](checks/r1-daemon-composition-20261008/67-exact-production-loc.json)
retain failed and final source identities. Production LOC:166022→167277
(delta+1255); core100605→101860, active57731→58986; reference65417,
excluded predecessors36325 and excluded integration6549 unchanged. Growth is
actual startup/config/wire/custody/control assembly, not relocation or retirement.
The final Linux binary proof uses an actual named VM volume and no host Store
data server, but its controller Init is Linux; macOS-to-Linux qualification,
ordinary Sandbox commands/access, FUSE Ready/live Commit and graceful application
drain remain open. R1 remains IN_PROGRESS; R2–R9 are not completed by this proof.

R1c foundation activates the real ordinary Engine command/stream/status adapter
and retains its exact kernel UID/capability/NNP and2MiB duplex proof. Full R1
remains open. [Results](checks/r1-sandbox-runtime-20261008/81-foundation-results.md),
[review disposition](checks/r1-sandbox-runtime-20261008/80-source-review-disposition.md)
and [LOC comparison](checks/r1-sandbox-runtime-20261008/84-exact-production-loc.json)
retain all failed/invalid/unqualified selections. Production LOC:167277→169065
(delta+1788); core101860→103648, active58986→60774, reference65417 unchanged.
The intact1106-line excluded Sandbox relocation reclassifies excluded integration
6549→5443 and excluded predecessors36325→37431; this is no retirement or
algorithmic simplification. Actual owned lifecycle/private-config/daemon-ready/
access and external-runtime process cancellation are still required. Published
standard2.2.4 client capability is identified, but access/CNI/actual signal proof
remains unqualified; it is not an owner waiver or a reason to close the Goal.


R1d now implements actual owned Engine lifecycle/private deployment/absolute
listener wait/exact topology/auth/SDK Hello and same-Control installation. The
cross-platform [native proof](checks/r1-sandbox-lifecycle-20261008/94-final-native-lifecycle-proof.txt)
passes with macOS host Init/seal, two Linux direct-Store daemons sharing one named
VM volume, ordinary identity/access and a third reopen after product deletions.
The [original failed close proof](checks/r1-sandbox-lifecycle-20261008/59-native-lifecycle-proof.txt)
is retained; original EndSession send/EOF/fence ordering is repaired without retry
or ENOTCONN suppression. [Architecture](../../architecture/72-owned-sandbox-lifecycle.md)
records exact ownership/failure/limits. R1 remains IN_PROGRESS; native FUSE Ready,
per-command cancellation qualification, S10 mounted Commit, application forced
close/join, full acceptance and retirement remain open. Prepared official unchanged
ctr2.2.4 bytes are checksum verified; they establish no runtime signal qualification.

R1d exact source-size comparison: Production LOC: 169065 -> 170673 (delta +1608). Core103648→105256; active60774→62382; reference65417, excluded predecessors37431 and excluded integration5443 unchanged. No relocation or retirement. [Exact accounting](checks/r1-sandbox-lifecycle-20261008/106-exact-production-loc.json) uses the same pinned counter over first-parent/staged product snapshots.

## Owner scope correction and stop boundary, 2026-10-08

The earlier R1c/R1d paragraphs and raw receipts preserve their original scope and
verdicts. Their statements requiring native Ready before R1 closure were broader
than the owning rollout R1 row: complete native Ready is R2. The owner now
explicitly defers the optional privileged ctr/admin cancellation and command-client
provenance subsystem. Ordinary runtime command streams/status, identity/backing
protection, real daemon/control startup and exact lifecycle/failure semantics remain
required. All filesystem contracts and R2–R9 qualification remain in force.

The owner requests a pause only after verified R1 completion and an actionable
FUSE handoff. Admin removal alone does not close R1. The final verification and
exact source/commit/LOC references are in the [R1 handoff](R1-COMPLETE-FUSE-HANDOFF-20261008.md).
No native filesystem acceptance or early reference retirement is inferred.

R1 final closure: restored no-admin product equals e5e95e76 exactly. Final host
and Linux locked build/Clippy checks,16 host tests,26 Linux tests,47 tooling
tests, actual cross-platform lifecycle/access/reopen and2MiB ordinary Bash duplex
proof all pass at [the recorded identities](checks/r1-completion-20261008/26-build-runtime-pins.json).
Required R2 native Ready/permissions/custody/drain remain unimplemented. The owner
stop boundary applies after this verified R1 completion and its actionable handoff.

R1 final closure accounting: Production LOC:170673 ->170673 (delta +0);
core105256/active62382/reference65417/excluded predecessors37431/integration5443
unchanged. [Exact parent/staged count](checks/r1-completion-20261008/28-exact-production-loc.json)
classifies archived uncommitted admin/R2 drafts as evidence, with no production
retirement credit. The handoff and post-commit confirmation identify the closure.

## Reviewed source organization, after R1

At input5be93f6d7, [the ownership review](R2-R5-SOURCE-OWNERSHIP-REVIEW-20261008.md)
selects Fuse as owner of session/dispatch/kernel operations/replies/coherence.
Daemon supplies application/filesystem assembly and narrow service/filesystem_port
implementations over the existing shared fair SQL owner/direct Store; it keeps
registry, overall Ready/normal terminal conjunction and existing Commit driver.
Planned daemon native/ and request/steps/ homes are superseded, not moved code.
Current active count is12; replacement FUSE will make13. Product/LOC and all R1
proof identities are unchanged. No R2–R5 implementation or new agent is dispatched.
Normal drain remains R2, coherence R3, captured namespace R4, mounted Commit R5;
R4 component work may overlap R2/R3 and R5 depends on all three. Forced teardown/
sustained concurrency/frozen acceptance and conditional retirement remain later.

## Dispatched R2 component work

The Pending Future checkpoint after4236225ee adds race-safe completion events
while retaining the original job/result/credit and synchronous caller route.
[Results](checks/r2-completion-future-20261008/20-results.md) record18 public tests
on each of macOS and Linux, scoped locked builds/Clippy, fmt,759-file boundary
scan and47 tooling tests. Native Ready/dispatch/mount remains unfinished.
Production LOC:170673 ->170710 (delta +37); core105256 ->105293,
active62382 ->62419, reference65417/predecessors37431/integration5443 unchanged.
The [fuser lifecycle authorization](FUSER-LIFECYCLE-DECISION-20261008.md) resolves
the inspected dependency policy prerequisite; its code/proof remains next work.

The subsequent [fuser lifecycle checkpoint](R2-FUSER-LIFECYCLE-20261008.md)
implements that authorized dependency API. Eight selected Linux cases pass,
including real pids-limit partial spawn and native mount/Ready/detach/join with
zero ordinary callbacks before serving. Its original timestamp provenance is
unchanged, and lifecycle provenance is separately pinned. This is dependency
scope only: full R2 native service/permissions/custody/drain remains in progress.

Dependency checkpoint accounting: Production LOC170710 ->170710 (delta +0),
core105293/active62419/reference65417/predecessors37431/integration5443 unchanged.
Affected dependency Rust files are separately1198 ->1505 (delta +307); no
first-party reduction or retirement is credited. Exact first-parent/staged and
post-commit confirmations are retained with the checkpoint receipts.

The [R4 captured namespace page checkpoint](checks/r4-captured-namespace-port-20261008/19-results.md)
exposes provider-neutral retained-reader pages and the actual Owner adapter,
reusing existing typed jobs/SQL. Five selected public tests pass on each platform,
with scoped locked builds/Clippy, formatting,761-file guard and48 tooling tests.
Production LOC170710 ->170795 (delta +85); core105293 ->105378 and
active62419 ->62504; other subtotals unchanged. R4's complete producer and
bounded/incremental Content validation remain unfinished.

R2 admission/worker-exit component: [results](checks/r2-admission-future-20261008/36-results.md) and [exact LOC](checks/r2-admission-future-20261008/37-exact-production-loc.json). Production LOC: 170795 -> 170986 (delta +191); core105378→105569, active62504→62695; reference65417/predecessors37431/integration5443 unchanged. Fixed credited notification slots and terminal original-job custody are verified; full native R2–R5 continues.

R2 resumable Workspace mutation component: [results](checks/r2-mutation-plan-20261008/18-results.md), [exact LOC](checks/r2-mutation-plan-20261008/19-exact-production-loc.json). Production LOC: 170986 -> 171080 (delta +94); core105569→105663, active62695→62789; reference65417/predecessors37431/integration5443 unchanged. The synchronous driver uses the same semantic plan. Native dispatcher/reader admission and full R2–R5 remain unfinished.

R2 Store reader component: [results](checks/r2-store-read-service-20261008/45-results.md), [exact LOC](checks/r2-store-read-service-20261008/46-exact-production-loc.json). Production LOC: 171080 -> 171783 (delta +703); core105663→106366, active62789→63492; reference65417/predecessors37431/integration5443 unchanged. Idle-reader Future/lease admission, fair Workspace rotation, quarantine and read-only bind snapshots are verified on host/Linux. Full R2 remains an ACTIVE user-requested Goal; native integration/ownership/Ready/drain and owning proofs remain required.

R2 native read/lookup custody: [results](checks/r2-native-lookup-20261008/35-results.md), [exact LOC](checks/r2-native-lookup-20261008/44-final-production-loc.json). Production LOC:171783 ->172718 (delta+935); core106366→107301, active63492→64427; reference65417/predecessors37431/integration5443 unchanged. Atomic answer/count/read acquisition, source protection across FORGET/unlink and bounded indexed retirement pass68host/69Linux tests. Full R2 remains ACTIVE; native handles, dispatcher, Ready, permissions and complete drain are still required.

R2 native regular-file ownership: [results](checks/r2-native-open-20261008/25-results.md), [exact LOC](checks/r2-native-open-20261008/26-exact-production-loc.json). Production LOC:172718 ->172929 (delta+211); core107301→107512, active64427→64638; reference65417/predecessors37431/integration5443 unchanged. Atomic open decisions, exact encoded-handle validation, independent file processing after FORGET/unlink/release and indexed mapping cleanup pass71host/72Linux tests. Full R2 remains ACTIVE; directories, native dispatcher, Ready, permissions and complete drain remain required.

R2 native directory ownership/cookies: [results](checks/r2-native-directory-20261008/62-results.md), [exact LOC](checks/r2-native-directory-20261008/63-exact-production-loc.json). Production LOC:172929 ->173827 (delta+898); core107512→108410, active64638→65536; reference65417/predecessors37431/integration5443 unchanged. Exact directory/source custody, accepted-entry cookies, current retained parents, empty-whiteout continuation and bounded live cleanup have91host/92Linux covering tests with scoped evidence reuse. Full R2 remains ACTIVE; real Fuse activation/dispatch, mount Ready, permissions and normal drain/native proofs remain required.

R2 replacement Fuse request service: [results](checks/r2-fuse-service-20261008/71-results.md),
[exact LOC](checks/r2-fuse-service-20261008/68-exact-production-loc.json).
Production LOC:173827 ->176088 (delta+2261); core108410→110671, active65536→67797,
reference65417 unchanged. Intact1447-line predecessor relocation reclassifies
excluded predecessors37431→38878 and excluded integration5443→3996, with no
retirement. Shared fixed workers, real deferred Owner/admitted Store ports,
original request/reply custody and initial Linux read callbacks have10host/12Linux
component tests plus49tooling tests. The original wake-ownership cycle failure and
all compile failures remain in the receipts. Full R2 remains ACTIVE: directory
callbacks, complete kernel accounting, mount/Ready, permissions and normal native
drain/proofs are still required. Application ControlReady is not native Ready.

R2 native consumers and directory callbacks: [results](checks/r2-native-consumers-20261008/36-results.md),
[exact LOC](checks/r2-native-consumers-20261008/34-exact-production-loc.json).
Production LOC:176088 ->176905 (delta+817); core110671→111488, active67797→68614;
reference65417/predecessors38878/integration3996 unchanged, with no relocation.
An original full16-slot metadata/data credit stall is retained and fixed by actual
consumer disposal, without increasing capacity. READDIR/RELEASEDIR, exact accepted
cookies and file/directory GETATTR handle association are wired and covered by
4host/4Linux consumer cases, including16 simultaneous offered directory batches.
Native callback output is bounded to128KiB; actual kernel buffers, mount/Ready,
permissions and complete normal drain remain unqualified. The owner requests a
verified checkpoint/handoff and pause here; full R2 is not complete.

The production checkpoint is `edeb8b35024823f905252efd8333089ac0610168`.
[The R2 handoff](HANDOFF-R2-NATIVE-CONSUMERS-20261008.md) records exact identities,
unresolved native integration/proof work and retained resources. Its subsequent
documentation-only closure keeps production LOC176905→176905 (delta0), with
core111488/active68614/reference65417/predecessors38878/integration3996 unchanged.
The current agent stops and pauses its Goal after this closure on the owner's
explicit request. R2 must not be marked complete from these component receipts.

R3 native mutation and coherence: [completion record](R3-COMPLETION-20261008.md), [results](checks/r3-mutation-20261008/00-results.md). Production LOC per commit is in the completion record; core active 71141 at the R2 identity.
