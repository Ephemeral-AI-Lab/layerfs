# Handoff: finish R1 infrastructure and measure its speed

> **Status: Current planning checklist; no release candidate exists.**
> Owner-requested plan, 2026-10-01. Preparation does not resume implementation,
> run benchmarks or claim a completed gate.
> Planning parent: `a0a4066e877189f45009e9d6dc766488caf4fde6`.
> Current product: `7a3a9b34c7c976ee8e63ba0db07833ffeda02a10`.
> Pre-R1 product control: `7edddbdb8e8512627aed0ed42533ef099d802384`.

## Copyable agent assignment

You own the remaining **R1 infrastructure implementation and a focused end-R1
speed checkpoint** for [#287](https://github.com/Ephemeral-AI-Lab/layerfs/issues/287).
Implement the named deliveries below through real current C1/C2/Server callers,
freeze coherent source, run owning proofs, and publish each checkpoint with exact
production LOC and a substantive #287 update. Continue autonomously within this
assignment after a checkpoint; it is not a request for permission to continue.

The latest owner direction adds a narrow measurement exception to the original
[implementation handoff](HANDOFF_IMPLEMENTATION.md): this agent may prepare and
collect the six existing cases in section 6, with a matched pre-R1 control and
finished-R1 candidate, and make the minimal existing-harness identity/setup work
needed to measure them honestly. This does **not** authorize the seven-family
campaign, Family8/9, a new runner/family, routine Family2 runs, the deferred 10,240
case, a codec campaign, or edits to #288. Read #288 for dependencies and leave its
remaining qualification delegated/unrun. Record this focused selection in #287.

The old pause remains in force while this plan is merely being prepared. When
the owner assigns this handoff to an implementation agent, it authorizes the R1
work and focused measurements described here. Stop at the final R1/speed report;
do not start R2-R7 product work under this assignment. Do not create new Codex
sessions or message unrelated tasks. Current-thread subagents may have explicit
nonoverlapping ownership; the root agent retains integration, acceptance,
publication and the single coordinated Bridge contract ownership.

Language: English. Preserve existing Phase4.5 functionality and canonical v1
compatibility. A passing correctness test is separate from resource, progress
and latency qualification. No new dependency where existing capabilities suffice;
no third-party patch/fork/vendor, retry/fallback, WAL, fsync, durability expansion,
extra construction producer, or hidden quota/worker/deadline increase.

## 1. Establish current source and ownership

Inspect pwd, status/branch, actual HEAD and ancestry, remotes, current #287/#288
and managed artifacts before editing. Reuse the suitable owned worktree only
after confirming no other agent is actively writing it:

```text
/Users/yifanxu/.codex/worktrees/issue287-implementation/layerfs
codex/issue287-implementation
origin https://github.com/Ephemeral-AI-Lab/layerfs.git
```

Inspect later published changes rather than resetting to any pin. Preserve the
primary checkout, foreign worktrees/targets, unmerged candidates, output posters,
old receipts, prepared inputs and archived binaries. A control worktree, if
needed, must be newly owned and isolated from the intentionally selected published
control. No build borrows another worktree's Cargo target.

Read root/Core AGENTS and scenarios, then:

1. [Current checklist](CHECKLIST.md), [append-only log](IMPLEMENTATION-LOG.md),
   [graph delivery](R1D-EFFECTIVE-GRAPH-DELIVERY.md) and
   [speed/source review](SCRATCH-SPEED-REVIEW-DRAFT.md).
2. [R0 interfaces](R0-FROZEN-INTERFACES.md),
   [resource audit](R0-RESOURCE-AUDIT.md),
   [graph freeze](R1D-EFFECTIVE-GRAPH-FREEZE.md), the other delivered R1 freezes,
   and [bootstrap limitation](R1E-BOOTSTRAP-DELIVERY.md).
3. The packet's [ROLLOUT](../../architecture/proposal/bounded-workspace-implementation-20260930/ROLLOUT.md),
   [SERVER](../../architecture/proposal/bounded-workspace-implementation-20260930/SERVER.md),
   [ACCEPTANCE](../../architecture/proposal/bounded-workspace-implementation-20260930/ACCEPTANCE.md)
   and [EVALUATION](../../architecture/proposal/bounded-workspace-implementation-20260930/EVALUATION.md).
4. [Benchmark rules](../../../../docs/general/benchmark_rules.md), root and
   [Core benchmark AGENTS](../../../benchmark/fs-bench-pro/AGENTS.md),
   [root benchmark AGENTS](../../../../benchmark/AGENTS.md), applicable quickstarts,
   documentation/release rules and the selected family specifications.

Historical packet observations are not current-source facts. Reuse scalar
bindings/sparse16 points, Sites, prefetch64, DirectoryRoots, forward/point run
seeking, bounded update FinalRows, and selective effective SCC. Do not revive the
older broad graph proposal or reject an unrelated historical nonseed cycle.

### What remains and what belongs later

| Responsibility | This assignment | Later boundary |
| --- | --- | --- |
| Current C1 navigation, drafts, base facts, alias traversal, reference/count/release state | Replace the actual remaining growing owners with admitted cursor/paged authority | This is not live Workspace v3 |
| C2 working-byte/index/cache/consumer overlap, Store authority, engine shapes and protected dispatch | Implement actual admission and prove supported-provider behavior | Missing physical/provider observations cannot become PASS |
| Scratch setup/reset/reuse and verified empty construction | Implement the same production algorithms with truthful ownership and bounded lifetimes | No workload recognizer or error-driven small route |
| Early release of legacy composite content admission | Provide the resource/transfer foundations; keep the current R1a gate visible | R3a's versioned saved-result/candidate custody is still needed, especially for import's multiple accepted roots |
| FileSet/results, canonical v2/certified parents, schema11 | Excluded | R3 |
| Live split/join catalogs, capture/install, command lifetime and concurrent Workspaces | Excluded | R2/R4/R5/R6 |

Report completed R1 infrastructure precisely. Do not mark aggregate R1 complete
while an original R1-required gate is unproved. The R3-dependent release row stays
unchecked and source-pinned; do not force it into an unsafe v1 refactor or silently
erase it from the milestone. Unsupported strict-provider scope also remains
CAPABILITY-LIMITED. This is a closure matrix, not a percentage estimate.

## 2. Source findings to preserve in the implementation freeze

- [mapping/read.rs](../../../crates/layerfs-content/src/file/mapping/read.rs)
  still retains complete level/next frontiers despite 32-page acquisition waves.
- [edit/tree.rs](../../../crates/layerfs-content/src/file/edit/tree.rs) retains
  drafts, parent references, detached/committed maps, a full settle collection and
  publication sets. A draft ceiling does not charge every associated owner.
- [facts.rs](../../../crates/layerfs-content/src/filesystem/validate/facts.rs)
  stores full positive/absence populations; [site_aliases.rs](../../../crates/layerfs-content/src/filesystem/validate/site_aliases.rs)
  retains pending/seen traversal state while Sites is live.
- [update.rs](../../../crates/layerfs-content/src/filesystem/update.rs),
  [reduce.rs](../../../crates/layerfs-content/src/filesystem/references/reduce.rs)
  and [release.rs](../../../crates/layerfs-content/src/filesystem/references/release.rs)
  retain fresh counts/unreachable/final rows, declared/touched populations, zero
  seeds, release queues/stacks and a seed-wide prefetch map. Bounded listing pages
  alone do not bound these owners. Existing pending run windows and update
  FinalRows lookahead are already bounded; preserve them.
- [C2 acquisition/publication](../../../crates/layerfs-storage/src/cas/lifecycle.rs)
  has Save effects before private index/codec allocation and publication clones
  after SQL publication. Admission must precede effects and include real clone
  overlap and consumer-held data, not only writer counts.
- [Service admission](../../../crates/layerfs-server/src/service/admission.rs)
  and [construction owners](../../../crates/layerfs-server/src/service/handler.rs)
  are per StoreAccess entry. Same-Store aliases can create duplicate scratch
  domains before persisted writer capacity refuses. C5 already deduplicates the
  actual catalog authority. Freeze actual Store authority association, not path
  or numeric-ID guesses.
- [Native acceptor](../../../crates/layerfs-server/src/host/acceptor.rs) can
  saturate before authenticated purpose classification. A spare generic socket
  is not protected catalog/control capacity.
- Current [Workspace Commit](../../../crates/layerfs-workspace/src/commit/operation.rs)
  sends composite Commit even for an empty change set; [Server Commit/Stage](../../../crates/layerfs-server/src/service/save/catalog.rs)
  creates construction state unconditionally. Therefore a clean final Commit
  currently pays the empty native Sites/Graph/Roots lifecycle. UpToDate proves no
  head advance, not absence of Stage or scratch work.
- Logical roots completion does not delete root rows/ordinal entries or close
  the connection. Returning a successful operation to a pool requires new checked
  retirement/reset, not renaming existing release as return-to-pool.
- GraphAttempt's six fixed 128-slot arrays enlarge even old-profile inline
  Resource layouts. Encoded record widths are not allocated/retained RAM. New
  large-value variants must not multiply unused worst-case inline storage.

## 3. Ownership and common contracts

Freeze concrete records, phase transitions, source/selection binding, epochs,
count/byte/window limits, capacity transfer, failure capsules and independent
vectors before implementations diverge. Allocate actual table/profile/opcode
values from the current registry; this plan invents no future tags or proof IDs.
Use focused existing-crate files and genuine callers, not empty scaffolding or a
universal state manager.

| Owner | Exclusive responsibility | Coordination |
| --- | --- | --- |
| C1 navigation/draft owner | file/mapping and file/edit state plus external owning tests | Frozen C1 draft/cursor/provider interfaces; preserve v1 split/join partitions |
| C1 namespace owner | filesystem validation/reference/update state and tests | Exact fact/alias/count/release interfaces and phase overlap |
| C2 state/resource owner | construction_state, shipped SQL, working owners/index/caches and provider tests | Root supplies interfaces and resource arithmetic; assign shared native files once |
| Engine/C5 owner, if split from C2 | engine and explicitly assigned connection-profile files | Avoid overlapping SQLite/native helpers; actual guard and scheduling witness |
| Root / one Bridge contract owner | Server composition, Store authority, native purpose/dispatch, Bridge negotiation, integration and independent expected vectors | Producer/consumer grammar reviewed together; protected capability bits stay false until proof |
| Root / measurement owner | Existing harness seals/setup, frozen selection, receipts, report, commit/LOC/#287 | No product mutation algorithm in the harness |

Tell every worker they are not alone, must preserve others' edits, and must not
run competing Cargo builds/measurements. Root serializes owning builds in the
worktree, freezes interfaces first and retains the combined acceptance decision.

The user-selected S is captured per native owner. Default/minimum remains 16MiB;
existing graph arithmetic R=S/256 is not a budget for every new fact/draft class.
Derive actual record/index/journal/window overlap for each new phase and the
combined live populations. Sites plus alias/fact state coexist; roots plus
reference/release/ordering may coexist. An 8KiB draft is not a 60-byte graph node.
No independent full S per table, uncharged second file, or automatic growth.
Request/work/file/cache/worker/deadline limits stay unchanged. Larger configured
budgets have only the actual tested support; the formal maximum is not qualified.

## 4. Named implementation sequence and exit gates

### R1-finish-freeze: closure inventory, provider gate and speed specification

Record actual parent, affected SC IDs, active authority being replaced, owned
files, expected independent result and smallest complete path for every row below.
Reconcile actual lifetimes against R0 resource arithmetic and publish the closure
matrix. Preserve already passed proofs and explicitly separate R3 dependencies.

Probe the selected required provider early through existing isolated proofs:
exclusive bootstrap, exact 32MiB hard-limit readback, tracking/free, actual SQL
NOMEM custody and native ready/EOF/reaped behavior. Current Apple SQLite3.51.0
reads back zero; native main correctly refuses before Store/history/workers.
Keep that scope Unsupported. No guard increase, bundled/forked provider,
bootstrap retry/reset, or zero/reuse-selected bypass. An eligible existing Linux
provider can supply separately labelled owning proofs; it does not qualify the
macOS benchmark Server. An explicit Linux component-test topology must never be
relabeled as the host benchmark topology.

Direct Darwin library proofs remain their explicit logical compatibility profile.
They can establish state/reset correctness and scoped SDK timings, not strict
native memory/protection. Continue independent authorized work after a capability
limitation; keep dependent enablement false and report the exact unsupported scope.

Before speed optimization or collection, commit the six-case spec and complete
source custody described in section 6. Repair minimal existing identity handling
with external tool tests: runner product sealing currently omits shipped SQL, and
its Python harness seal alone omits JSON registries/C workloads. Include shipped
SQL/other implementation, registries, helpers, fixtures, dependencies, flags,
compiled driver/verifier binaries and image inputs. A SQL-only change must never
reuse an old executable because the Rust-only seal matches.

Acquire qualified fixtures once and collect the untouched control only after
this prospective spec/setup/seal is published. Commit control-derived per-case
targets before subsequent speed-oriented product changes or candidate collection.
If the common harness changes later, re-evaluate pair validity before sampling.

### R1b-working-owners: actual Store association and allocation admission

Share Save/scratch/byte scheduling for aliases of the same unforgeable Store
authority. Preserve persisted writer-slot authority across independently opened
handles/processes. Prove a shared association or refuse the strict configuration;
do not infer equivalence from a path or StoreAccess ID.

Introduce real last-owner byte leases before C2 acquire/open/clone/codec effects.
Cover old shared indexes, two active private copies, publication copy, actual
B-tree allocation, codec arenas, read/pack/group/dependency caches, drained waves
plus the next retained object, decoded/encoded alternatives and slow consumers.
Use actual capacities/layouts, not collection len or a nominal per-entry estimate.
Add an owning compiled-size/allocation proof for full GraphAttempt/Resource and
concurrent acknowledgements. The six arrays derive41,472B from current compiled
field sizes alone; codes/scalars/solver/cursors/seals/padding/caller pages remain
additional. This is not a physical/process-memory bound.
Do not claim the independent 32MiB returned-canonical compatibility reader fits
the proposed 8MiB strict read owner; preserve its explicit larger profile.

R0's 72MiB Save/176MiB first-party arithmetic is the starting contract, not measured
fit. Derive singleton versus standard composition before effects. Include idle and
retained scratch resources and allocator/container margins. A global heap cap is
not reserved per-connection catalog headroom.

Exit: real overlapping owners, pre-effect exhaustion, alias capacity, slow held
results, publication crossing, last-owner refund and retained failure tests under
actual allocator/native observations. No whole-operation lock that blocks pure
catalog/control progress replaces byte ownership.

### R1d-known-clean-reset and R1d-scratch-reuse

First deliver bounded retirement of successful sealed root rows and their ordinal
projection. Preserve exact count/bytes/last-key progress; at most128 records and
header-inclusive64KiB per transaction. Retain partial and COMMIT-Unknown progress.
Verify all live tables/indexes empty and reset fixed owner rows in a small known
transaction. No population-sized DELETE, DROP/CREATE, VACUUM or provider repair.

Then add a fixed per-actual-Store idle file/connection/schema pool within existing
owner slots. First reuse eligibility follows the independently known successful
**scratch-owner terminal state**, with all dependent metadata consumers ended:
matching terminal root seal, completed Sites/Graph retirement, checked root reset,
autocommit, no live statement/BLOB/cursor/read lease, no failure/quarantine or
release attempt. Known failed operations retain their existing explicit cleanup
path; Unknown/reset/rebind failure never returns idle or triggers a cold fallback.
Keep release() as close/unlink/refund; introduce explicit return-to-idle ownership.
Later independent Store/C5 uncertainty retains its own capsule and cannot authorize
scratch-Unknown cleanup/refund/adoption. Do not conflate the owners or infer whole
operation success merely from known scratch completion.

Separate stable continuously owned native path/FD identity from the fresh
operation token. Each checkout burns a new identity, takes the fresh SourceId,
recomputes full native binding/header/subject, and rejects old tokens/scopes/seals
before SQL. A filename containing the first token cannot be guessed from a later
token. Exact provider/profile/schema/budget classes cannot adopt one another.
Old private profiles keep their explicit semantics; changing a private format
requires a frozen grammar and independent vectors, not Store schema11 migration.

Active/resetting/idle/retained owners all remain charged. Return-to-pool is not
physical refund. Sum mixed16/48MiB captured budgets; do not count*current-config,
resize an idle owner or add per-Workspace/lifetime registries. Empty slots may
deliberately initialize new owned files; failed reuse is not a retry opportunity.
Explicit shutdown drains known idle owners once without SQLite global shutdown
or destructive cleanup of Unknown ownership.

Exit: same-file repeated real Service construction, independent roots/bytes,
maximum and partial root retirement, old-reader/stale-token exclusion, foreign
contexts, reset/rebind COMMIT Unknown, allocation/identity mismatch, mixed budgets,
occupied-pool refusal and explicit drain failure. Count create/reserve/open/schema,
reset/rebind/close/unlink events. Measure reset cost as well as avoided setup;
reuse is not presumed faster for large populations.

### R1d-verified-empty-state: preserve the cheap empty/small path

Use the same checked construction coordinator with a genuine typed empty-state
authority for an exact eligible Update shape. Start with all-zero change sets;
extend to verified ordinary file-only updates only when every skipped population
is provably empty or fits an independently admitted fixed window. D=0/B=0 alone
is not proof that every other future state table is empty. A fresh build still
has a root node and is not automatically an empty graph.

Freeze an explicit nonnative authority/profile if current selection types require
a native association. Never fabricate native bindings, files, cleanup or allocated
bytes. An impossible nonempty append must refuse. Global counts/EOF/replay seals,
base/root/kind/identity/allocator checks, Save/C5 custody and deadlines remain.
Do not recognize commands, workloads or filenames, catch Capacity to switch paths,
or select a small historical graph algorithm from B<=128. Guard eligibility is
unchanged. No-op and one-edit public SDK paths must actually select the intended
plan; an UpToDate head observation alone does not prove that.

Exit: ordinary clean Commit and a real file update, exact known heads/parents,
independent bytes/roots, lying body/count/EOF/kind/context refusals, truthful native
scratch zero, retained failure custody and no hidden resource/profile enlargement.

### R1b-navigation-cursor and R1d-draft-authority

Navigation replaces level-width vectors with a range-pruned cursor/continuation
bounded by the existing mapping height31 and admitted page waves. Visit needed
pages/extents once; do not repeatedly descend from the root to manufacture each
window. Preserve demand order, absolute-offset rebasing, grouped reads and
partial-output/error custody through the real read_range caller.

Draft state replaces draft/parent/detached/committed/emitted populations and
whole-set settle/publication collection. Use exact checked link counts, advancing
detached jobs and children-before-parent completion. Preserve split/join locality,
shared immutable subtrees, advisory predecessor semantics and v1 partitions.
Do not suppress decode/cleanup failures or use unchecked parent increments.
Payload bytes stay outside metadata scratch. Integrate real apply_edits and
Server SaveFile; a standalone port with no producer is not delivery.

Exit: independent v1 edit roots/partitions, real-provider wide/deep range reads,
overwrite/append/shrink/shared-draft cases, exact pre-effect refusal and acceptance/
cleanup failures. Demonstrate fixed frontier/draft windows and linear completion/
retirement counts. Preserve the delivered64-page cache ownership and run seeking.

### R1d-base-facts and R1d-alias-discovery

Bind positive/absent/unknown facts to the immutable selected inode table and
context. Keep a fixed admitted hot window plus exact paged facts where persistence
is necessary. Avoid repeated inode-prefix walks; preserve logical demand/site
counts across eviction and recorded physical prefixes on provider errors.

Replace alias pending/seen authority with discovered/pending/expanded records and
an indexed frontier while Sites remains live. Expand reachable directories once;
process each old edge/changed stored site once. Preserve active_stored==0 early
return, restated-edge skip, follow-once, legal-old-alias OR, unreachable exclusions
and alias-before-cycle verdict priority. An uncertified v1 Base may require a real
reachability walk; do not invent R3 parent certification or pretend D/B bounds V/E.

Exit: independent positive/absence facts and alias events, small memo/star/
permutation/deep/wide/move cases, reachable old cycles, removed/retained aliases,
exact context rejection, simultaneous Sites+fact+frontier capacity, and actual
provider failure/Unknown. Preserve current point16/Sites/selective-SCC proofs.

### R1d-reference-counts and R1d-release-finalization

Replace fresh initial-count/unreachable/final-row and declared/touched populations
with exact inclusion/count authority and advancing cursors. Reuse sealed new-serial
membership rather than copy it. Ordered scans carry current values forward;
do not buy another point read for each emitted row. Preserve all effects before
values, root protection, new-orphan rules and addition-before-release.
New-parent/unreachable authority is needed before Sites/Graph validation: it also
feeds active stored sites, alias exclusions and fresh graph leaves. Freeze that
upstream phase and its simultaneous lifetime before implementing the cursor join.

Stream proven-zero seeds into typed jobs with effective kind/root and persisted
continuations. Do not copy all seeds into a queue or prefetch all bases. Carry a
page's already-read child fact; expand each proven-zero directory and decrement
each released edge once. Namespace depth is not mapping-tree height31: arbitrary
supported namespace depth cannot grow an in-memory release stack. Final inode
rows/EOF and known required cleanup precede filesystem-root emission.

Exit: an independent event interpreter for final counts/values/removals/roots;
cross-boundary aliases, broad removal, deep namespace chains, retained moves,
mixed fresh/existing state, late consumer/provider failure and Unknown, exact
final EOF and no uncharged touched/zero/final Vec. Preserve bounded pending runs,
fixed-stride backward seeking and current update FinalRows lookahead.

### R1d-graph-work-quanta: reduce demonstrated per-step overhead

Use the published selective SCC coordinator and its exact adjacency/proof
semantics. The reviewed raw196608-record fixture entails524288 separate CAS/pop
transactions, with repeated fixed metadata verification and fixed-window capsule
initialization. These are source-derived counts, not measured latency or an
ordinary complete C1 workload. Identify the actual repeated queries/preparation/
allocation and choose the smallest safe reduction before changing the port.

A closed bounded compound transition or independent-target batch must be frozen
with exact old/proposed rows, solver state, discovery/order, source/seals, record
and byte sums, acknowledgement and known/Unknown/rejection custody. The current
port rejects repeated targets: do not concatenate dependent DFS steps into an
existing batch or silently weaken CAS/owner validation. Similarly, statement or
metadata reuse needs continuously owned identity, admitted cache lifetime and
proof that corruption/foreign-context refusal is preserved.

Keep bounded progress/cancel quanta and one producer. Do not materialize the graph,
run a second historical cycle algorithm, cache an unbounded query population, or
trade fewer checks for guessed ownership. Normalize only a proven equivalent
sequence; no reseeding or per-SCC prefix scan.

Exit: independent graph verdicts and canonical roots, exact traversal/work counts,
real SQL query/transaction/allocation reductions for the changed mechanism,
closed batch refusal, real COMMIT Unknown and seeded-cycle acknowledgement,
retirement and original failure custody. A safe reduction unsupported by evidence
is not invented to meet a speed target; retain the concrete unresolved cost and
TARGET_MISS rather than a fake performance PASS.

### R1e-engine-shapes and R1e-protected-native

Pass the supported guard to participating Store/catalog/scratch factories before
their first effects; validate at relevant boundaries. Enforce/read back each actual
connection's cache/mmap/TEMP/persistence/busy/limit profile. Qualify statement,
BLOB, dirty-page and MEMORY-journal shapes and retained allocations under real
simultaneous owners. Keep R0's32MiB guard and30MiB scheduled shape arithmetic;
prove catalog headroom/progress, not merely global containment or a PRAGMA value.

One Bridge owner coordinates the frozen authenticated HELLO v2 purposes
(General/Catalog/Control), bounded preauthentication and protected dispatch,
FD/frame/terminal owners. Check actual registry availability before changes;
update grammar, native client/server and capabilities together. Current default
capacity is four persistent sessions plus one accept/refusal owner. Freeze the
full General/Catalog/Control/preauthentication/refusal/closing/terminal count, FD,
executor and byte vector within the approved resource envelope before enablement.
Four occupied General sessions plus a renamed fifth socket do not establish
protection. Do not raise ordinary workers/Save slots, silently lower the read
ceiling, or add an uncharged generic socket; refuse a strict topology that cannot
satisfy its actual vector.
V1 remains explicitly unprotected. Capability bits stay false until the real
native proof passes; final FileSet/v2/schema11 features remain unavailable.

Exit: two persisted Save owners plus saturated ordinary read/data sessions and
active scratch coexist with a completed authorized catalog refill response before
either held source resumes. Prove hostile/slow handshakes/output, protected terminal
ownership, count/byte exhaustion, scope/conflict, NOMEM and Unknown custody through
actual kernel/provider barriers. Direct Service catalog progress is reused but
does not stand in for this native saturation proof. Physical-domain observation
is separate; heap-only or lifetime peaks cannot satisfy it.

### R1-v1-integration: coherent infrastructure handoff

Freeze the whole supported R1 source and reconcile every remaining owner with the
same real route. Inspect SourceIds, selected roots, table phases, object acceptance,
failure/Unknown boundaries, idle/retained resources and accumulated work. Maintain
one current next action; never start dependent enablement after an incomplete gate.

Run required owning locked Core tests/examples/fmt/Clippy and product boundary/
self-tests for the changed packages and relevant public composition. Reuse
unaffected passed bodies; a demonstrated correction runs its covering commands.
Record exact exits and platform skips. Complete final full-Core requirements at
their mandated handoff; no CI or retired aggregate preflight. Repository-root
ARMv8 AEAD flags and worktree-owned targets apply to every build.

Produce the R1 closure matrix: implemented/tested infrastructure, original
composition-dependent rows, provider/physical limitations and unchanged R2-R7
boundaries. Full R1 is not COMPLETE if a required gate is missing. Then collect
the focused final candidate once as specified below.

## 5. Resource and correctness rules common to every delivery

- Simultaneous encoded/decoded/clone/page/result/consumer/capsule owners are charged
  before dependent growth/effects. A returned result does not refund a live owner.
- For mutable external iteration, freeze exact before/after records and progress;
  successful continuations advance. Known COMMIT and observation acknowledge work;
  Unknown retains proposed rows/continuation/credit and permits no resend/adoption.
- Keep one producer per construction operation. Idle Workspaces do not reserve
  full Commit buffers. Idle pooled native resources remain explicitly charged.
- Preserve source scopes, v1 roots/partitions and canonical-byte acceptance.
  Independent expected results are pinned before candidate output, not generated
  by it. New private-format vectors have their own independent framing oracle.
- Keep tests/fixtures/real coordination outside product src. No fake allocator,
  clock, fault branch or test-only product hook. A PID, sleep or submitted RPC
  alone proves no overlap. Real held FDs/transactions/accepted I/O establish it.
- Production files stay<=999 physical lines; lib.rs/mod.rs<=200, declarations and
  delegation only. New runtime SQL is production source. Respect crate boundaries.
- Preserve accepted live writes and known accepted/published roots. Abort only
  existing known-unfinished ownership under its contract; never roll back Unknown
  or refund a retained resource. No stricter durability or stronger confinement.

## 6. Focused end-R1 speed specification

### Selection and exact public route

Use the existing public SDK on the macOS host with host Server/Store/history/
SQLite/canonical publication. Linux Docker owns only daemon, real FUSE and the
ordinary workload. No direct helper substitutes for SDK latency. Component work
counts/timers are attribution, not another performance family or SDK comparator.

Freeze cohort/spec identity `issue287-end-r1-speed-v1` with six ordered existing IDs, seed1,
one sample per case per arm, the actual fixtures/workloads/oracles and all gates:

| Label | Existing case ID | Main question | Complete bound |
| --- | --- | --- | --- |
| clean | workspace-commit-clean-retained-writes-4097-v3 | Empty construction/retained-generation fixed cost | 15s |
| one-edit | workspace-commit-one-edit-retained-writes-4097-v3 | Small nonempty Commit after4097-write prelude | 15s |
| overwrite4k | workspace-shell-package-overwrite-4k-sdk-v2 | Ordinary4KiB overwrite at5MiB in10MiB file; small Exec/Commit | 15s |
| namespace67 | workspace-namespace-move-replace-descendants-67-sdk-v1 | Real inherited binding/site/alias/graph work | 15s |
| components270 | workspace-namespace-components-270-sdk-v1 | Existing deep directory-heavy case, not a larger capability claim | 25s |
| many128 | workspace-shell-package-many-128-sdk-v2 | Moderate count, row/wave/root boundaries | 15s |

Clean and one-edit keep their entire4097-write prelude, pin, first Commit and final
operation inside the complete command; do not move that work into setup. Clean
currently enters Stage with empty populations. The five dirty cases enter ordinary
prepared construction. Record actual route and state-plan counts at final source;
do not infer them from a case label or advanced=false.

Keep each family's emitted receipt profile/schema unchanged unless a prospectively
versioned existing-harness extension explicitly records the cohort. Never relabel
historical receipts with the new cohort.

The six-case selection is not whole-family admission. Keep other registry cases
visible as out-of-selection/unrun, not missing/failing or silently PASS. Any new
setup/cache method needs a prospectively versioned profile/case as applicable;
do not relabel the existing historical receipts.

### Mandatory fixture and identity gates

Audit all master provenance, not just hashes. F5's current sdk_master restores
wide/small Stores produced by workspace_commit_native inside Docker and adds a
host marker. The permanent hosting rule prohibits restoring those Docker-owned
prepared Stores for a new run. **Do not execute that setup path.** Provide an
equivalent master through existing public host SDK Init/benchmark_shell setup,
using exact independent source bytes/namespace manifests, before either arm.
Reuse the same closed validated host master via independent writable byte copies.
If exact host setup cannot be produced through the supported existing route,
keep both F5 selections NOT_RUN with concrete evidence; do not run a native family,
hand-construct a measured Store or shrink270/67 to make it work.

F7 already acquires its master through host ProjectApi::init. F4's referenced
master/artifact provenance must also be checked. Acquire each qualified master
once; never reuse a mutated sample. Root quickstart flags are not the Core parser:
Core clone reuse is internal and recorded; it accepts no --setup clone or
--reuse-pass flag for these invocations.

Product and compilation seals must include shipped SQL. Harness custody must
include Python, JSON registry, literal command/writer sources, fixtures, manifests,
lock/dependency flags, root .cargo/config.toml, driver/verifier/daemon binaries and
immutable image IDs. Old compact reports/seals alone do not prove these inputs.
A common harness overlay on a control worktree must itself be sealed/published
with unchanged pre-R1 product trees; every such commit has exact LOC. Do not run
an unreported dirty control or transplant candidate product into its control.

### Control, targets and measurement order

Control product is the published7ed pin; verify the selected public API and v1
grammar are available. Future candidate is the actual final R1 published commit,
not the current partial7a3 checkpoint. Both use the same sealed measurement
mechanics, locked release build, dependencies/ARMv8 flags, one construction worker,
provider topology, fixture/workload/oracle and declared cache treatment.

1. Publish the prospective spec and any narrow source-seal/host-setup changes.
2. Prepare once and take each untouched control observation once. Preserve raw
   performance receipts and separate proof before optimization. This is not reuse
   of an old Phase4.5 timing as a fresh control.
3. Publish per-row SDK Exec, SDK Commit and complete-command targets before further
   speed-oriented optimization or candidate collection. The design objective is
   observed nonregression against that matched control, not an invented percentage
   allowance. Historical few-ms clean/light Exec and15–35ms small Commit values
   are context only. The history10% storage tolerance is not a latency tolerance.
4. Finish implementation and owning proofs, freeze/publish final candidate and
   validate the same pair inputs. Collect each candidate row once, with separate
   independent proof. Record interference and unavailable attribution honestly.
5. A target miss remains TARGET_MISS and gets source/count diagnosis. A single
   observation is not statistical proof of regression. Do not resample an unchanged
   passing or inconvenient arm. If a demonstrated product correction requires a
   new source, freeze a new identity/selection and apply the actual matched-arm
   reuse/invalidation rules before any further collection; retain every attempt.

No n3/median/best-of sampling is selected. The #118 ordinary-screen carve-out is
not this profile. Do not enlarge complete/proof bounds, add workers, reduce input,
warm paths or hide initialization/reset/cleanup in setup to meet a target.

### Executable command grammar

Run each case separately from its owned arm worktree root; replace ARM and PIN
with frozen literal labels/actual identities and use a fresh nonexisting output.
These are command templates, not commands executed by this planning task:

```sh
python3 core/benchmark/fs-bench-pro/runner.py run --case workspace-commit-clean-retained-writes-4097-v3 --out benchmark-results/fs-bench-pro/issue287-end-r1-ARM-PIN-clean
python3 core/benchmark/fs-bench-pro/runner.py run --case workspace-commit-one-edit-retained-writes-4097-v3 --out benchmark-results/fs-bench-pro/issue287-end-r1-ARM-PIN-one-edit
python3 core/benchmark/fs-bench-pro/runner.py run --case workspace-shell-package-overwrite-4k-sdk-v2 --out benchmark-results/fs-bench-pro/issue287-end-r1-ARM-PIN-overwrite4k
python3 core/benchmark/fs-bench-pro/runner.py run --case workspace-namespace-move-replace-descendants-67-sdk-v1 --out benchmark-results/fs-bench-pro/issue287-end-r1-ARM-PIN-namespace67
python3 core/benchmark/fs-bench-pro/runner.py run --case workspace-namespace-components-270-sdk-v1 --out benchmark-results/fs-bench-pro/issue287-end-r1-ARM-PIN-components270
python3 core/benchmark/fs-bench-pro/runner.py run --case workspace-shell-package-many-128-sdk-v2 --out benchmark-results/fs-bench-pro/issue287-end-r1-ARM-PIN-many128
```

Do not use F5/F7 family defaults or invent comma-separated --case lists. After a
completed performance run, use its retained exact identity for separate proof:

```sh
python3 core/benchmark/fs-bench-pro/runner.py prove --run benchmark-results/fs-bench-pro/issue287-end-r1-ARM-PIN-LABEL --out benchmark-results/fs-bench-pro/issue287-end-r1-ARM-PIN-LABEL-proof
```

Existing prove bound is9s and performance is not replayed. Existing verify checks
evidence custody on these schemas; it is not the independent canonical-byte proof.
Record exact commands and all build/setup/performance/proof/cleanup walls, source
seals, method versions, fixtures, CPUs/memory/provider/image and clone reuse.

### What the measurements may claim

Current F4/F5/F7 profiles explicitly record performance_claim=false and numeric
latency INELIGIBLE because the full source/dirty-byte/OS/Store/daemon Commit cache
domain is unproved. Preserve that assessment even if candidate beats a target.
The focused collection is useful diagnostic evidence and command-budget evidence;
it does not by itself establish eligible latency nonregression or release admission.

No turning INELIGIBLE into PASS by changing an evaluator constant. Eligible
comparison requires an actually proven, prospectively versioned cache method and
complete required observations; absent that, report the qualification delegated
under #288. This assignment does not silently expand into a new cache/physical
benchmark campaign. Unsupported setup/provider rows stay NOT_RUN/INCOMPLETE.

Report final SDK Exec and Commit separately from prelude and complete command;
do not sum overlapping/nested spans. SDK/Server share host process resources;
missing exclusive Server attribution is unavailable. No sampled/lifetime cgroup
peak or heap-only observation substitutes for phase-local physical memory.

Count/timer diagnostics should identify first authority use, file create/reserve/
open/schema, reset/rebind/drain, source decoding, fact/site/graph construction,
CAS/pop transactions, index seeks, publication/cleanup and retained resources.
Use existing public telemetry/counters or narrowly justified production telemetry,
never benchmark-only hooks or another speed sample disguised as a diagnostic.
Pool initialization stays in first-use metrics, reset stays in the owning operation
or declared lifecycle, and idle ownership stays charged. No phase receives hidden
warm source bytes or expected results from setup.

## 7. Checkpoints, LOC and final handoff

For each named delivery: record parent/source/SC IDs, owned files, replaced state,
expected result and exact exit gates; implement the smallest complete path;
review resource/work/failure custody; freeze; run meaningful owning checks once;
diagnose a red result from output/source and run its covering correction only.
Update architecture/contracts, checklist and append-only log with every gap.

Stage only owned files. Count first-parent and final staged/committed snapshots
with the same approved production counter, scope and exclusions, including shipped
SQL/imports/declarations/delegation. Report reference/Core/combined and signed
delta in every commit message and handoff. Test/docs/tool commits have unchanged
product totals, not a zero-sized product. Recount after amendments/rebases/source
changes and verify committed tree matches the counted staged source. Normal push
only; confirm remote identity. Publication failure retains the local commit and
gets its actual local-only status. No force-push, remote main merge or issue close.

Read latest #287 before edits; preserve original requirements, old verdicts and
comments. Add one substantive checkpoint with actual commit URLs, exact checks,
LOC/method, provider/platform/measurement scope, remaining gates and next action.
Keep #288 untouched. Do not claim a hypothetical future self-referential SHA in
its own document; source seals and parent live locally, resulting URLs externally.

Final deliverables:

1. Committed R1 implementation with actual growing owners retired, preserved v1
   semantics, explicit supported resource/provider classes and owning proofs.
2. Current closure matrix separating completed infrastructure, R3-dependent
   composition, capability-limited original gates and unstarted R2-R7.
3. Six-row control/candidate table with exact source/artifacts/routes, Exec/Commit/
   complete/proof walls, resource/work/cleanup observations, actual target outcome,
   and independent numeric eligibility. Every FAIL/INELIGIBLE/INCOMPLETE/NOT_RUN
   remains visible; historical receipts are not promoted.
4. A concrete speed decision: which fixed/scaling work was removed, what still
   dominates, which targets were met/missed and which claims remain unqualified.
   Correctness/count PASS alone never becomes a speed PASS.
5. Per-commit LOC/publication/#287 record and precise remaining #288/R2-R7 handoff.

Complete all authorized implementable work and the focused collection; do not
stop after publishing a plan or first small slice. A real capability/setup blocker
gets concrete evidence and precise unsupported scope, while independent work
continues. Full missing gates remain incomplete. Stop after this assignment's
final R1/speed report; further rollout requires its own owner assignment.
