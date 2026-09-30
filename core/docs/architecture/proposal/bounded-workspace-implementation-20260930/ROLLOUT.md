# Implementation rollout: bounded streaming and concurrent Workspaces

> **Status: Research; informative and not a product contract.**
> Proposed rollout, 2026-09-30. Source baseline:
> `7edddbdb8e8512627aed0ed42533ef099d802384`. No product implementation,
> runtime capability probe, test, benchmark or migration is performed by this plan.

Read the [master specification](README.md), [target layout](LAYOUT.md),
[Workspace packet](WORKSPACE.md), [Server packet](SERVER.md),
[concurrency packet](CONCURRENCY.md), [acceptance gates](ACCEPTANCE.md),
[issue/scaling map](ISSUE276-AND-SCALING.md) and
[workload scenarios](../../../../../scenarios.md). R0-R7 below are new rollout
milestones, distinct from historical component stages and benchmark families.
The detailed packets define semantics; this file consolidates dependencies,
delivery order, enablement and retirement. It is a plan, not authorization to
implement or publish changes during the current research task.

The [issue closure map](ISSUE-CLOSURE.md) ties these milestones to the actual
ticket requirements. In particular, #256 requires resource-admitted live handles
and directory cursors/cookies beyond the old fixed counts, plus the next u16
and wide-rebind boundaries; paged dirty rows alone do not close it. R2/R5/R6 must
implement and prove that live-reference scope. Bounded listing/frame batches do
not cap the total directory or replace admission by actual owner resources.

## 1. Delivery strategy and dependency graph

Build three cohesive implementation streams against one frozen contract:
Workspace/private generations, Server/C1/C2 construction, and runtime/transport/
FUSE. Integrate their smallest complete supported path early, then widen evidence.
Concurrency is designed from the beginning. Public concurrent enablement waits
for the #248 memory, identity, generation and failure-custody gates.

```text
R0  contracts, independent oracles, platform/migration capability gates
 |
 +------ R1 resource/admission + paged construction state ------+
 |                                                             |
 +------ R2 Workspace private v3 -------------------------------+--> R4
 |                                                             |    complete
 +------ R3 Server streaming + canonical v2 --------------------+    Commit
 |                                                                  |
 +------ R5 runtime/FUSE/lifetime implementation can start early -----+
                                                                    |
                          R4 qualification + R5 qualification -------+
                                                                    |
                                                               R6 concurrency
                                                                    |
                                                               R7 cutover,
                                                                  retirement,
                                                                  larger profiles
```

R1 supplies required ports/admission to R2/R3; the graph permits independent
cohesive work after its corresponding contract is ready, not use of incomplete
resource enforcement. R5 can be implemented early, but target runtime enablement
uses the qualified R4/R5 composition. Each delivery is a reviewable owning change
with architecture updates and exact production LOC. Do not create empty module
scaffolding, ship a parallel mutable implementation as an error fallback, or make
an unrelated benchmark campaign the integration prerequisite.

## 2. Milestones and exit gates

| Milestone | Concrete delivery | Evidence required before dependent enablement |
| --- | --- | --- |
| R0: freeze boundaries and proof sources | Stream/owner/generation/READY/outcome records, FileSet/result framing, canonical v2 grammar/profile, private v3 layout, admission windows, command lifetime/cancel contract, capability/opcode allocation, execution/mount ownership and explicit migration phases | Every growing population has a charged owner; independent v1/v2 oracle method and expected-root vectors are reproducible; actor/domain/FUSE and phase-memory observer capability obligations are explicit; Store quiescence can be established before migration effects |
| R1: bounded state and resource foundation | Paged cursor/draft/run/index ports in existing crates, byte admission and cache insertion ownership, typed C2 Save versus C5 catalog admission, SQLite engine accounting and protected completion/control capacity | Actual encoded/decoded/clone/window overlap fits the selected envelope; refusal precedes dependent effects; short C5 inode refill can progress while both C2 Save slots are occupied; cursor progress and run merging avoid repeated prefix work |
| R2: private Workspace v3 | Per-file interval roots, paged current inode/name/owner/location facts, exact per-inode conflict leases, short current-catalog publication, immutable source contexts and bounded retirement | Ordinary supported filesystem mutations preserve exact bytes/names/handles; unrelated inode operations progress during wide preparation; successor first touch keeps one opaque predecessor span; no resident full-population registries remain |
| R3: streamed Server and versioned canonical state | Unified FileSet/binding cursors, fresh direct input and one required inherited replay source, paged C1 drafts/graph/effects, framed saved results, immediate-parent file policy v2, certified namespace parent tree, schema11 roles and bounded explicit migration/import implementation | Exact roots/partitions against independent ledgers; v1 compatibility separately retained; cycles/aliases/moves and forged/stale proof authority fail correctly; result populations exceed32KiB safely; lost terminal retains Unknown; new schema refuses incompatible access and migration refuses unproved quiescence |
| R4: one complete Commit composition | Fixed G1 capture/new G2, ordered saved facts, protected READY before the single composite namespace/HistoryCommit request, known-result installation, Unknown custody and bounded included cleanup | All affected SC01-07 byte/root/pin/outcome cases; writes accepted during real Commit phases survive install; no proportional allocation after publication; same-selector local completion and cleanup failures are distinguished; all owned resident domains satisfy the qualified #248 profile |
| R5: native filesystem and owned command lifetime | First-party cancellable Linux FUSE session, exact actor/mount admission, quota projection, owned reactor/I/O and command domains, negotiated UntilOwnedExit, start/wait/cancel ownership, mount-death/close and explicit dirty-discard workflow | Healthy quiet command exceeds30s; finite liveness/callback bounds remain; authorized external process uses the mount without Exec; real interrupt/publication race, descendants, pipe drain/reaping, disconnect, sibling denial and quota semantics are proved; accepted exit7 writes persist until explicit discard |
| R6: concurrent enablement | Keyed registry/incarnations, resource-admitted channel pool, independent mounts, same-W overlapping commands, per-W submission leases and independent admitted Commits | #248 prerequisite remains satisfied under composition; SC08 same-W and cross-W progress, one pending Commit perW, explicit shared-Branch conflict, retained-owner counts, incarnation-safe teardown and bounded active command/PID/timer state |
| R7: explicit cutover and retirement | Fresh qualified v3/v2 attachments, quiesced existing Store upgrade, selected v1 snapshot import into a new v2 Stack/scope, old-owner drain/retirement, versioned capability/support documentation | Exact migration fields and unchanged pack payloads; independently certified imported namespace and serial remap; selected known/unknown migration custody; no old mutable path reachable from target operations; exact net LOC and targeted final qualification recorded |

Milestone numbering expresses dependency, not a claim that each row is one commit
or that R5 must wait to be developed. Independent narrow review units can land
earlier; a dependent capability stays unavailable until its required composition
is proved. No date/duration or measured speed forecast is inferred from LOC.

## 3. First deliverable and early risk retirement

R0 produces one executable implementation checklist with assigned responsibility
for each real boundary, exact wire/schema versions and independent expected roots.
Choose exact ExecHandle/cancel grammar and allocate real opcodes from the existing
registry; do not leave guessed tags in implementation. Freeze allocation arithmetic
for overlapping parse/encode/SQLite/result/install windows, not only individual
buffer sizes. Keep the selected standard Server envelope a proposed profile until
its real engine/cache/runtime composition is qualified.

Before broad feature enablement, narrow real-provider proofs must establish:

- Exact runtime actor access to a real mount, denied sibling access, and issued
  handle/kernel cleanup authority. The proposed kernel-session adapter must honor
  FUSE INTERRUPT without modifying a third-party package.
- Trusted bootstrap/domain membership before user code, non-escapable domain
  controls, attribution/reaping of descendants and bounded terminal cleanup.
- Supported physical residency enforcement and phase peak observation, including
  the same-open-FD memory.peak hypothesis described in ACCEPTANCE. Old numeric
  receipts remain INELIGIBLE; a working observer alone is not memory qualification.
- Deployment-wide Store quiescence for schema migration, including old users that
  cannot participate in new process-local arbitration. Refuse before effects if
  this ownership cannot be proved.
- Independent golden identities for namespace/file-policy v2. Candidate output
  cannot generate its own expected-root ledger. Reuse independent v1 references
  only for their exact retained v1 semantics.

These are architectural proof obligations. During implementation, an unsupported
required platform capability remains Unsupported with concrete evidence; it does
not activate a legacy path, relax a guard or trigger another backend.

The first integrated slice after R0/R1 is one supported Workspace, an ordinary
file mutation, one immutable capture, one FileSet unit, certified namespace
construction, one Branch publication, small install and exact cleanup. Reuse
existing sound codecs/transport primitives while replacing the selected authority
and composition. Then exercise fresh/inherited files, many members and overlapping
G2 mutations through that same path. This gives reviewable end-to-end behavior
before building out every large workload tier.

## 4. Ownership and parallel work boundaries

| Stream | Owns | Must coordinate through |
| --- | --- | --- |
| Workspace | backing/active nodes/files/catalog/generations/payloads/custody; commit prepare/results/READY/install/completion; private quota/state semantics | CapturedToken, source cursors, saved facts, InstallCapsule and exact outcome/cleanup ownership |
| Server/storage | C1 file/namespace construction and validity authority; C2 packs/indexes/SQLite/scratch/migration; Server input/results/admission/composite publication | Versioned FileSet/binding/result contracts, finalized consumers, C2/C5 admission and protected publication result capacity |
| Runtime/transport | Daemon registry/commands/control/platform, native owned I/O/channel pools, SDK execution handles, sandbox bootstrap/domains, FUSE session/projection | Exact W/incarnation, typed lifetime and cancellation, RuntimeActor/mount/RequestLease authority, bounded observations |

Bridge contract changes have one coordinated owner and reviewed producer/consumer
updates. History retains its C5 responsibilities and existing opaque scope/profile
initialization where sufficient; do not invent a new History migration framework.
Follow LAYOUT's grouped module paths and source line ceilings. Existing reference
crates stay outside Core dependencies. Unmerged research candidates are source
evidence; cherry-picking or advancing them is a separately justified implementation
choice, not this plan's default.

## 5. API-independent filesystem capture

The filesystem mutation boundary does not require an Exec command lease. An
authorized process with mount visibility and the correct runtime actor can use
supported ordinary syscalls whether launched by WorkspaceApi::exec or an external
supervisor. FUSE authorizes mount/actor/issued-handle requests; Workspace records
accepted mutations through the same semantics and generation publisher.

```text
SDK Exec ---------+
                  +--> ordinary syscalls --> kernel/FUSE --> private Workspace
external launch --+                                           |
                                                     explicit capture/Commit
                                                              |
                                                       Server/C2/C5
```

Add an external-launch proof alongside ordinary SDK proofs: same actor and mounted
path, ordinary create/write/truncate/rename/unlink, independent byte/name oracle,
then Commit through the public lifecycle. It has its own route label and does not
replace SDK end-to-end coverage. External launch is not automatically a LayerFS
owned command: descendant cleanup, application memory and output/cancellation
require explicit supervisor/domain integration. Neither path captures writes
outside the selected mount. Supported syscall coverage and filesystem mutation
capture do not promise a complete syscall journal or writable mmap/DB semantics.

R5 removes the whole-command30s cap using negotiated UntilOwnedExit across SDK,
Bridge and daemon. Connection/admission/stalled-I/O/callback/cleanup operations
retain finite bounds. A >30s correctness proof is separately bounded and is not a
performance selection whose wall budget has been enlarged. Keep constant bounded
unsent liveness/output state; do not substitute a very large finite timer.

## 6. Migration, enablement and failure containment

Activate supported capabilities explicitly; do not choose an algorithm after a
failure. New private v3 owners and new canonical v2 Stacks have distinct identities
and capability checks. Existing private v2 owners remain quiesced, charged custody
until exact resolution/drain; target operations cannot mutate through them.

Fresh Stores can start with schema11 when its contract is implemented. Existing
schema10 Stores upgrade only under proven quiescence, bounded indexed locator
copy/equality checks, the selected small schema swap and bounded retired-row
cleanup. Packs/payloads are not recopied by this metadata migration. Account real
temporary metadata and physical freelist/high-water costs. The new v2 Stack/scope
import is a separate explicit step after Store role capability is established;
it does not silently change the old Stack's profile or canonical identities.

Before new canonical publication, a definitely failed, side-effect-free activation
can leave the old selected profile in service. Once v2 canonical data is published,
old binaries/profile identity cannot simply be restored as a semantic downgrade.
Use supported readers and exact selected operation/migration state. Unknown
outcomes retain custody: no guessed resend, refund, deletion, backup restoration
or reset. This workflow adds no WAL, sync or crash-recovery promise.

Delete each displaced mutable planner, resident registry, reconciliation patch,
command timer/helper path or namespace scan after its replacement's owning gates
pass and callers have moved. Keep required v1 canonical compatibility readers/
contracts; their retention differs from an executable mutation fallback. Report
temporary duplication and final deletion separately. No early deletion can strand
live views, handles, commands, saved results or failed owners.

## 7. Qualification and performance discipline

Use the [evaluation plan](EVALUATION.md) for the seven-family run/reuse map,
proposed same-Workspace/multi-Workspace witnesses, independent streaming/resource
proofs and reporting boundaries. Proposed Family8/9 IDs need their own committed
roadmap specifications before benchmark implementation or collection.

For each coherent milestone, freeze the affected source and run its meaningful
covering proof. Diagnose failures from the existing output/source and narrow
cause diagnostics. Complete required Core locked tests/examples/fmt/Clippy,
boundary guard and guard self-tests at the appropriate frozen handoff; report
exact commands/gaps. No aggregate preflight or CI wrapper is introduced.

Select performance mechanisms prospectively with one case/arm sample and separate
verification, under the existing cache/worker/identity rules. Reuse prepared
fixtures and unaffected qualified evidence. Especially reuse Family2 history
proofs while their measured canonical/codec/C2/C5 mechanisms are unchanged. FileSet
save cadence, C2 metadata/index changes or canonical v2 profiles may require a
specific affected storage checkpoint; broad source seals alone do not justify
rerunning the whole history group. V2 roots need new v2 identity proofs.

Use the [benchmark report template](../../../../../benchmark_agent_report.md).
Report ordinary end-to-end/Exec/Commit, capture/install blocking, Server work and
required cleanup; resident domains and temp/final allocated bytes remain distinct.
No hidden maintenance or cached setup work leaves the timer. Preserve every
registered FAIL/INCOMPLETE/INELIGIBLE/NOT_RUN and historical verdict.

Count-driven diagnostics must show the architecture removed repeated-prefix and
historical-population work: predecessor extents not re-enumerated each generation,
no deep namespace triangular visits, no backward-prefix scratch joins, no lifetime
command/PID/timer scan. Sorting, actual changed bytes, tree height, real ancestor/
release work and initial namespace certification retain their honest costs.

The accepted existing history allocation gap is up to10%; it is not an allowance
for unbounded temporary disk. A roughly5% storage saving cannot justify roughly50%
speed loss. A5%-for-5% tradeoff is the owner's acceptable example, not permission
to weaken memory, identity or custody requirements. Do not start another codec
optimization campaign during architecture rollout without an affected measured
mechanism justifying it.

Every commit reports exact first-parent -> final staged/committed production LOC,
with reference/Core/combined subtotals and the reproducible counter. The existing
+5,500 to+26,500 net forecast is broad planning uncertainty, not a budget to spend.
File relocation adds zero net LOC; prefer existing sound primitives and retire
displaced authority in the same delivery when safe. Update architecture/source
pins with code, not afterward.

## 8. Completion and explicitly separate capabilities

The rollout is complete at a declared profile when the ordinary supported SDK and
external-mounted-path routes preserve accepted state; live mutation and full Commit
use bounded admitted memory; canonical v2 locality and independent identities pass;
G2/pins/known/unknown outcomes and cleanup retain exact custody; healthy commands
outlive30s; and same-W/cross-W concurrency obeys actual aggregate resource limits.
Publish the exact profile and remaining unsupported capabilities with evidence.

Larger logical/file/write/body/count profiles follow separate prospective
qualification after R7, without weakening old quotas to pass a row. The deferred
10240 case remains deferred until explicitly selected with its new profile.
Writable mmap, complete DB locking/crash durability, an S3 adapter and stronger
sibling filesystem confinement are separate capabilities with their own contracts
and proofs. A generic mounted route alone does not qualify all database/model/S3
workloads or every SC tier.
