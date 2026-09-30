# Implementation specification: bounded streaming and concurrent Workspaces

> **Status: Research; informative and not a product contract.**
> Proposed implementation specification, 2026-09-30. Inspected source:
> `7edddbdb8e8512627aed0ed42533ef099d802384`. All after-state diagrams,
> algorithms, interfaces and LOC forecasts describe future implementation.

The subsequent owner assignment starts implementation under #287. Its
[R0 interface freeze](../../../issues/287/R0-FROZEN-INTERFACES.md),
[current checklist](../../../issues/287/CHECKLIST.md) and
[implementation log](../../../issues/287/IMPLEMENTATION-LOG.md) supply the current
implementation authority. This packet retains its original research status and
historical source/evidence; support is recorded at each implementation checkpoint.

The owner requests one deep implementation design combining bounded-memory
live backing and Commit, targeted optimization/bug fixes, concurrent opaque
commands and multiple Workspaces. Three subagents research Workspace,
Server/C1/C2 and concurrency independently in this thread. This packet consolidates
their source analysis and cross-review. Production implementation, test runs,
benchmarks, issue publication and commits are outside this specification task.

The owner's subsequent direction makes this a replacement architecture, not a
patch queue around old failure paths. Reuse independently sound components and
algorithms; replace the representations and authority that permit the old
failures. The concurrent target has no old singleton/control-thread fallback.
Each excluded state below has an enforcing contract and an adversarial proof;
this is stronger than a checklist of repaired examples, without an unverifiable
claim that future software can contain no bugs.

## 1. Recommendation and packet map

Adopt immutable selected generations with per-file range roots, paged exact
operation/ownership facts, pull-based construction and protected small result
installation. Compose those semantics with an admitted Workspace registry,
exclusive authenticated channels and owned event-driven command supervision.
Use ordinary functions/concrete internal types and a few real I/O boundaries.
Keep C1 independent of Workspace, mounts, databases and runtime process policy.

| Packet | Owns | Implementation detail |
| --- | --- | --- |
| [LAYOUT.md](LAYOUT.md) | Consolidated target crate/folder/file paths | One module map, responsibility/dependency rules and replacement discipline |
| [ROLLOUT.md](ROLLOUT.md) | Implementation delivery order and enablement | R0-R7 dependencies, exit gates, API-independent FUSE capture, migration and retirement |
| [#287 implementation handoff](../../../issues/287/HANDOFF_IMPLEMENTATION.md) | Copyable implementation-only agent assignment | Milestone iterations, exact per-commit LOC, checkpoint publication and #287 updates; #288 campaign stays separate |
| [ISSUE-CLOSURE.md](ISSUE-CLOSURE.md) | Expected closure scope after implementation | Direct targets, parent/supporting candidates, exact missing gates and retained #276 ledger |
| [EVALUATION.md](EVALUATION.md) | Verification and performance selection | Seven-family run/reuse map, proposed command/Workspace concurrency families, resource/complexity gates and reports |
| [WORKSPACE.md](WORKSPACE.md) | Live edits, selected generations, capture/lowering, READY/install, private owners and retirement | Range/root layout, provenance, publication sequencing, modules and LOC forecast |
| [SERVER.md](SERVER.md) | Streamed prepared input, C1 construction/graph proof, C2/SQLite memory and allocator admission | Cursor contracts, ordered algorithms, engine envelope, compatibility and LOC forecast |
| [CONCURRENCY.md](CONCURRENCY.md) | Registry, mounts, command domains, channels, lifetime/cancel/close and hierarchy | Owned state machines, platform ports, wire changes, modules and LOC forecast |
| [ACCEPTANCE.md](ACCEPTANCE.md) | Cross-packet invariants, adjacent bugs, scenario proof and focused evidence | Failure matrix, source versus historical facts, unchanged proof reuse and observer correction |
| [ISSUE276-AND-SCALING.md](ISSUE276-AND-SCALING.md) | Latest eight issue comments and quadratic-work exclusion | Exact traceability, current versus historical status, accumulated-work laws and count diagnostics |
| [BASELINE-LOC.json](BASELINE-LOC.json) | Reproducible current product-size baseline | Per-file production/physical counts, exact source/tree and counter hash |

Read the [scenario catalog](../../../../../scenarios.md) and the earlier
[architecture research](../bounded-memory-commit-20260930.md) for source context.
The implementation packets make more concrete choices; they do not retroactively
change older evidence or establish a released contract.

Implementation amendments describe their own source and delivered scope:
[R1a typed Server admission](R1A-SERVER-ADMISSION.md),
[R1b cache ownership](R1B-CACHE-OWNERSHIP.md) and
[R1c indexed construction metadata](R1C-INDEXED-CONSTRUCTION.md) and
[R1d exact run-seek delivery](../../../issues/287/R1D-RUN-SEEK-DELIVERY.md). Exact passing,
partial and unsupported outcomes belong to the append-only
[implementation log](../../../issues/287/IMPLEMENTATION-LOG.md); these selective
deliveries do not turn the R0 research packet into a completed R1–R7 product.

## 2. Non-negotiable product route

WorkspaceApi::exec accepts opaque ordinary commands. A compiler, shell, Python,
package manager or application uses the selected mounted filesystem. No command
parser, workload classifier, executable allowlist shortcut, special large-file
writer or benchmark mutation route enters the implementation. An ordinary copy
or prepend pays the bytes it actually copies. A separately requested projected
splice capability is a generic feature with its own ABI/proof, not automatic
recognition of shell intent.

Filesystem mutation authority is the authorized mount/actor/issued handle,
independent of an Exec command lease. Externally launched authorized processes
use the same kernel/FUSE path. Their process supervision has separate ownership;
the [evaluation plan](EVALUATION.md) keeps that route distinct from SDK coverage.

```text
Public SDK lifecycle + WorkspaceApi::exec(opaque command)
                         |
                  ordinary program
                         |
               ordinary filesystem calls
                         |
                FUSE semantic projection
                         |
             incremental private Workspace
                         |
           explicit generation capture / Commit
                         |
              authenticated logical streams
                         |
              Server: C1 -> C2 and C5
```

Logical identities, canonical semantics and stream/resource contracts remain
host-independent. Linux mounting/execution and supported storage-residency
enforcement are explicit platform adapters. This packet introduces no
APFS-specific assumption or direct access to server SQLite from commands.

## 3. Before: distinct serialization and population bottlenecks

The baseline already has incremental live ranges and bounded C2 save waves.
Those foundations are retained. The problems are complete resident populations
and several separately scoped exclusions, not a claim that every existing
operation loads an entire file or holds one giant SQLite transaction.

```text
SDK control
    |
    v
one control session -> long Lifecycle lock -> one selected mount/Workspace
                                              |
ordinary Exec process -> FUSE -----------------+
                                              |
                         offset-keyed incremental private backing
                         affected extents/update plans grow in RAM
                                              |
                              host-wide frozen/remote admission
                                              |
                     capture / lower / descriptors / saved facts
                       complete populations and prepared bytes
                                              |
                          one outbound Mutex<Transport>
                                              |
Server: directory Vecs -> mapping drafts -> graph/maps/reference collections
                                              |
C2: private save_id -> bounded waves / short transactions -> visibility
                                              |
C5: exact Stage / Branch expected-head publication
                                              |
Workspace: proportional reconcile plans -> long writer hold -> cleanup
```

Removing only the upload descriptor array would leave live mutation plans,
namespace binding decode, canonical proof, installation and retained ownership
growth. Removing only the frozen flag would leave mount/session/lifecycle and
transport exclusions. The implementation therefore composes both changes.

## 4. After: bounded state with per-Workspace ownership

```text
                       SDK: ordinary opaque commands
                                  |
                 owned control states / bounded reactor
                                  |
                   short identity registry lookup
                     /                         \
                    v                           v
            Workspace A                    Workspace B
       mount + command leases         mount + command leases
       Exec A1 / A2 / ...             Exec B1 / B2 / ...
              |                              |
             FUSE                           FUSE
              |                              |
       private per-file roots         private per-file roots
       short revision publish         short revision publish
              |                              |
       frozen G1 + live G2            frozen G1 + live G2
       one submission owner           one submission owner
              |                              |
       exclusive work windows         exclusive work windows
              +---------------+--------------+
                              |
                admitted exclusive channel leases
                protected control/read/cleanup capacity
                              |
Server input -> checked record cursor -> ordered joins / paged exact proof
                              |
C1: bounded frontiers + authenticated parent index -> finalized objects
                              |
C2: existing codec + private waves / short transactions + paged indexes
                              |
C5: known conditional publication or exact definite/Unknown outcome
                              |
        sealed result context -> short CURRENT G2 installation
                              |
               bounded retirement / exact last-pin release

Shared admission: daemon bytes/private disk/PID/FD + separate Server envelope
                  unchanged C2 Save permits + short C5 catalog permits
                  protected control/completion ownership
```

One construction producer belongs to each admitted Commit/Stage construction
operation. Existing Store capacity limits how many independent operations
overlap. Per-Workspace ownership does not authorize adding workers within a
Commit or changing Store capacity. The repository's namespace-init worker
exception remains explicit; this redesign does not collapse Init to one worker.

## 5. Incremental live behavior and capture timing

Commands keep using private backing while Commit streams its immutable captured
view. Each file mutation prepares a paged candidate under that file's conflict
lease, then publishes bounded catalog changes into the current generation.
Unrelated files/readers can progress. Namespace mutations acquire an explicit
ordered conflict set and publish their final atomic binding/inode changes.

```text
time --------------------------------------------------------------->
commands    accepted writes | accepted writes / creates / renames ...
                           |
capture          short ----+ select G1; open G2
Commit                     | read immutable G1 / construct / publish
live state                 | G2 = selected G1 + subsequent changes
install                                              short selection
live state afterward                                CURRENT G2 retained
```

A whole command is not a transaction. Accepted syscalls can belong to either
side of capture. Installation must preserve the actual current G2, not an
earlier detached copy. Same-file conflicting mutations can wait; I/O/resource
admission can wait or refuse within declared bounds. No unconditional zero-wait
or dedicated-device throughput guarantee is made.

Per-file retained spans select terminal canonical/private sources or bounded
shared interval subtrees. Repeated writes/Commits must not build a recursive
chain through every preceding file version. Pins retain immutable resolver
contexts. Shifted offsets, holes and orphan selections remain exact; root bytes
and historical canonical partitions require the compatibility gate in SERVER.

At first touch in a successor generation, represent the complete captured file
as one opaque parent span. Split that span around the new change; lowering stops
at the inherited span and does not enumerate all predecessor intervals. A
candidate crossing capture normalizes only its bounded retained boundaries
against the exact frozen file. Without this rule, repeated small Commits could
revisit ever-growing historical fragmentation and become quadratic even with
perfectly bounded buffers. Private cleanup still pays actual unreachable owners.

## 6. Responsibility and dependency rules

| Owner | Single responsibility | Receives / returns | Forbidden coupling |
| --- | --- | --- | --- |
| API SDK | Public lifecycle and typed operation invocation | Opaque command, exact Workspace identity, typed outcomes | Recognizing workload or accessing private backing/Store |
| Bridge contract | Identity, declared stream/lifetime/framing semantics | Owned checked records and operation states | Mount/process policy or C1 construction algorithm |
| Bridge native adapter | Authenticated bounded I/O progression | Channel-owned frame/body states | Per-Exec thread stacks, begun mutation replay |
| Daemon assembly | Bind registry, runtime domains, mounts and channels | Concrete capabilities and admitted entry/command leases | Filesystem canonical algorithms or global long locks |
| FUSE adapter | Project ordinary syscalls and truthful quota/errno | Workspace semantic operations | Command recognition, daemon command lifetime or server SQL |
| Workspace | Private selected views, mutation/capture/install and custody | Authorized ranges/bindings, stream cursors, outcome selection | C1/C2 in-process fallback, child-domain implementation |
| Server assembly | Compose authorized logical streams, C1/C2/C5 and resources | Checked inputs, exact outcomes | Workspace mount layout or command semantics |
| C1 content | Canonical construction, reads, references and graph validity | Authenticated objects, finalized consumer and bounded scratch ports | Databases, filenames, mounts, Workspace/history dependence |
| C2 storage | Physical representation, exact membership/index, private save and publication | Canonical objects and storage resources | Commands, generations or Branch conflict policy |
| C5 history | Stage/Commit/Layer identity, allocator and expected-head transition | Exact saved root and typed publication context | Live extents, shell cancellation or payload codec |

SRP is enforced by named cohesive files, not arbitrary numbered splits. Public
component boundaries remain independently usable. Introduce a trait only at a
real supplied I/O/resource boundary; use concrete internal algorithms instead
of a factory/interface per class. Do not add a service locator, runtime plugin
registry, universal scratch service or dependency on root reference crates.

Each packet supplies its proposed folder/file tree and file responsibility.
Implementation files remain <=999 physical lines, lib.rs/mod.rs <=200 and
declaration/delegation only. Aim below the ceiling to leave review space; no
minification or implementation moved outside the production scope for LOC.

## 7. Concrete design decisions

| Decision | Selected approach | Reason / remaining proof |
| --- | --- | --- |
| Private authority | Versioned paged inode/version catalog, per-file interval roots and exact owner ledgers | Isolates large file plans and removes global population arrays; private compatibility/lifecycle gate required |
| Capture / install | Immutable G1, distinct live G2, sealed saved-result/orphan resolver and current-G2 selection | Avoid full reconciliation rewrite and predictable post-publication proportional admission |
| Small operation path | Same checked cursor grammar with bounded resident runs/pages and direct ordered consumption | Preserve ordinary-case speed; spilling chosen by declared byte policy, not an allocation-error fallback |
| Unordered state | Exact bounded run generation/merge and paged graph state | No probabilistic alias/cycle acceptance; sort only where keys differ |
| Command/control | Owned incremental states driven by shared reactor; exact command/domain leases | Avoid per-command helper thread growth and retain cancellation/drain custody |
| Transport concurrency | Exclusive authenticated channel pool, one operation per channel | Removes global session exclusion while preserving simple frame ownership |
| New inode progress | Separate short C5 catalog admission from C2 Save admission; preadmitted serial ranges amortize calls | Allocator/history-only operations do not hold a codec/Store Save permit; protected bytes/engine/control resources remain bounded |
| Isolation | Explicit runtime execution-domain capability; strict filesystem confinement separately selected | Process groups/cwd/output caps do not establish arbitrary descendant or child-memory isolation |
| Optimizations | Ordered traversal, bounded cached page reuse, safe work-triggered maintenance and batched exact reservations | Reduce real calls/page work without changing codec for small storage gains |
| Pure move locality | Authenticated parent index bound into target namespace profile v2 | Incremental exact parent-chain proof; initial build/migration is explicit, older profile identities retained |
| Store schema upgrade | Explicit quiesced metadata-only locator migration to schema v11 | New parent roles require widened role validation; payload packs remain unchanged; actual metadata high-water/freelist remains charged |

Required platform/dependency capabilities fail explicitly when unavailable. Use
already pinned dependencies where they supply the capability; no vendor/fork,
registry edit, patch or hidden backend fallback. New cfg-gated dependency
features require a locked, reviewed compilation input. Keep the ARMv8 AEAD flags.

### States the architecture must exclude

| Old enabling pattern | New enforcing boundary | Required adversarial proof |
| --- | --- | --- |
| Operation-sized Vec/Map or future wire bytes consume resident Budget | Cursor-only operation interfaces; fixed charged windows; paged population authority | Scale extents/identities/one-directory width independently without resident growth |
| A mutable latest-version map supplies old readers | Immutable selected version/resolver and terminal source grammar | Pin old bytes through overwrite, move, insert, unlink and successive Commits |
| Known Branch success enters a new proportional allocation phase | Only a sealed READY submission can publish; fixed pre-admitted installation/outcome ownership | Exhaust ordinary G2 resources before ack and complete exact known result |
| Install overwrites G2 with a stale detached view | Publisher applies parent selection to CURRENT live root under short gate | Mutate during every phase and verify exact post-install/next-Commit bytes |
| Per-file parent dependency recursively references all earlier revisions | Terminal/shared-subtree range authority plus checked bounded pending-generation edge | Repeated edits/Commits with stable resolution depth and exact orphan versions |
| A shell or remote call owns a global registry/lifecycle lock | Registry issues keyed entry leases; slow work owns only its operation/conflict scope | Quiet long Exec and bulk Commit while another Workspace performs status/create/edit |
| A channel can be shared concurrently or recycled with ambiguous protocol state | Exclusive authenticated lease, monotone dispatch sequence, terminal synchronization/quarantine | Interleave lease order, lose terminal and reject replay/reuse of uncertain channel |
| C5-only allocation waits for an entire C2 bulk Save | Typed C2SavePermit versus C5CatalogPermit, phase-scoped composite acquisition and protected catalog capacity | Consume initial ID range while two large file units stay active; refill via ordinary authorized catalog RPC |
| PID/group exit is mistaken for exact command-domain completion | Owned runtime domain with separate leader/domain/pipe/terminal states | setsid/closed-pipe descendants remain owned until exit/cancel/retained cleanup |
| Output/frame/global counters hide unbounded process/engine/cache memory | Separate admitted daemon/Server/child/physical-residency envelopes | Pressure each owner/domain independently; no heap-only or lifetime-peak substitution |
| Unmount/nonzero exit silently discards accepted writes | Explicit detach versus private-discard grammar and exact workflow selection | Exit7, cancel and disconnect preserve writes until authorized explicit discard |

SOLID applies to these actual boundaries. C1's supplied read/consumer/index
ports can be substituted only when authentication, ordering, byte bounds and
ownership lifetimes remain identical. Split read, append, indexed lookup and
command-domain capabilities so a consumer cannot invoke unrelated authority.
Policy extension uses checked version/capability input at assembly; it does not
create command handlers or expose every internal helper as an interface.

### Proof obligations, beyond example tests

The implementation must establish these laws through checked constructors,
typestate transitions, algorithm review and independent external oracles:

1. **Range law.** Selected intervals are ordered, non-overlapping and exactly
   cover the declared file length. Each source range is authorized against its
   immutable selected version and checked length; no serial-only interpretation.
2. **Publication law.** Candidate bytes/pages/edges are complete and owned before
   their root becomes selectable. A multi-key namespace transaction has one
   linearization point; preparation failure cannot expose a partial mutation.
3. **Generation law.** Each accepted mutation belongs to the generation current
   at its publication. Capture freezes published state; install preserves the
   actual later state. Neither a prepared candidate nor a whole Exec determines
   the boundary by its start time.
4. **Reachability law.** Selected root owners, immutable child edges and unfinished
   cleanup custody determine exact physical ownership. A last-owner refund occurs
   only after checked release; caches and byte equality are not reachability.
5. **Publication-resource law.** Only a sealed READY owner can start the final
   composite publication. Required fixed outcome/install resources cannot be
   spent by live G2, another command or another Workspace.
6. **Namespace law.** A parent index corresponds to actual forward bindings under
   a certified Base or complete build/import proof. Final-batch effects preserve
   reference/parent restrictions and acyclicity before successor certification.
   A successor certificate is fixed authority, not a recursive parent-proof
   Arc/DAG. Construction parent leases release when certification finishes;
   actual pins retain their own independent roots.
7. **Execution law.** Every user descendant begins inside the owned runtime
   domain, cannot migrate out, and retains command custody through exit, exact
   attribution, reaping and pipe cleanup. PID/leader/pipe observations alone
   cannot release the Workspace slot.

These are required implementation proofs, not completed formal verification or
a zero-defect claim. A regression in an enforcing boundary is visible as a
violated law with a focused oracle; it is not handled by an alternate old path.

## 8. Time, memory and disk comparison

S = actual processed payload/boundary bytes; E = surviving extents examined;
F = dirty identities; N = changed bindings; M = mapping/metadata page work;
L = records requiring joins/order; G = graph vertices/edges required by proof;
U = actual retirement work; H = declared index height; p = page bytes;
B = admitted fixed buffers/caches; Q = simultaneous admitted Commits;
A = active Execs; W = live Workspace entries. Namespace depth is separate from H.

| Mechanism | Before: source-backed mechanism | After: proposed bound / work |
| --- | --- | --- |
| Workspace final-delta preparation | Complete dirty/extent/name/saved populations and 24E descriptor bytes | O(B+Hp) resident windows; O(S+E+F+N) record work plus page I/O |
| Wide fragmented live edit | Accumulated affected-range/update populations | Bounded split/join frontier; affected/retired page work remains charged |
| Local install | Reconciliation charge includes 192E and complete patch populations | Fixed selected roots + O(H) catalog paths; retirement U is separate measured work |
| Directory/graph proof | Resident binding vectors, graph/topology collections and depth state | Bounded windows, paged depth/color state; complete O(G) proof plus lookup/order costs |
| Mapping drafts | Resident unfinished tree drafts up to declared byte ceiling | Bounded frontier/external drafts; required canonical page/object work M remains |
| Target namespace v2 move | Broad/repeated graph work can dominate a local binding edit | Verified parent-tree paths and distinct affected ancestor proof; initial full build/migration and actual deletion closure remain |
| Ordering | Finite pending buffers without composed backing on service path | Ordered inputs merge linearly; generic comparison sort O(L log L), O(L) disk |
| Command supervision | Per-session/helper stacks and whole Exec deadline | O(A) admitted owned states/output/FDs, shared reactor, no elapsed whole-Exec timer |
| Cross-Workspace progress | Singleton and host/session logical exclusions | Admitted independent operations; same-file ordering and shared resource contention remain |
| Store/index envelope | Bounded shared/private windows multiply with Save owners and clone overlap | Admit actual aggregate capacities/engine footprint; exact private population authority is paged |

```text
M_daemon <= fixed host/reactor/caches
          + W * admitted base/control/custody state
          + sum live command state/output/socket allowances
          + sum admitted Commit and mutation working sets
          + protected retained completion allowances

M_server <= fixed service/index caches + admitted SQLite engine envelope
          + sum input/graph/draft/codec/wave/index working sets

D_peak = actual new/private payload + unpublished canonical objects
       + replay/draft/order/graph/owner state
       + uniquely selected old bytes + unfinished cleanup custody

T_commit = required payload/record/page work
         + ordering + complete graph proof + actual cleanup
```

With checked H and admitted windows, active construction RAM has no term
proportional to S/E/F/N/G. Total RAM still includes W/A/Q and actual engine/file
cache domains. More concurrent admitted work may use more total RAM within the
pool. This is not an 8 MiB Server/RSS promise; the default Workspace-host
accounting Budget is a separate resource.

One million descriptor records would occupy 24,000,000 bytes under the current
width; the current reconcile formula would reserve 192,000,000 bytes. These are
source arithmetic, not observed peaks or accepted million-record workloads.
Default two active C2 encoder arenas can total 32 MiB before other engine/wave
state; existing codecs stay unchanged and must fit the separate Server envelope.

Temporary metadata can increase as resident populations become paged authority.
Existing replay/private storage is reused rather than adding duplicate full
payload spools. Sparse inherited edits preserve unchanged range/subtree reuse;
fresh/full replacement still pays all bytes. Physical buffered file cache needs
an independently supported resident-window capability; bounded Rust buffers do
not prove bounded total memory.

No speed multiplier is forecast. Expected wins are less array conversion,
ordered cursor work, shorter capture/install gate residence and lower
cross-Workspace waiting. Extra graph/draft/ledger I/O can slow small operations.
Measure those deltas under the same declared conditions. Preserve the accepted
storage tolerance and reject about 50% speed loss for about 5% storage saving.

## 9. Implementation sequence and compatibility gates

1. **Replacement contracts and authority.** Freeze stream/owner/READY/outcome
   schemas, typed command lifetime, read framing and admission envelopes.
   Specify old failure states as prohibited representations before splitting
   implementation ownership. No fix-first patch chain or unrelated benchmark
   campaign.
2. **Private bounded live authority.** Implement paged catalogs, per-file
   candidates, precise conflict leases and owner/retirement authority. Preserve
   private-version lifecycle and exact accepted syscall publication.
3. **Unified streamed Commit and Server composition.** Replace complete prepared
   populations; compose graph/order/draft ports and bounded C2 indexes/engine
   memory. Use one generic bounded file-set/namespace composition contract;
   file count changes grouping, not command routing. Seal generation
   results/exception/READY state and small install.
4. **Target identity and bounded core qualification.** Implement authenticated
   parent-root namespace v2 and immediate-captured-parent construction policy,
   with explicit v1 snapshot import/new-stack identity and verified-base
   authority. Qualify locality/no cumulative prior-delta replay and
   SC01–07 byte/root/pin/outcome/resource
   and actual all-phase writer progress. Known canonical results and cleanup
   use exact same-selector native completion. Unknown delivery never replays.
5. **Concurrent delivery.** Enable registry, count policy, pooled channels,
   owned reactor/commands and runtime domains as one composed path. Design them
   in steps1–3, but honor #249's #248 prerequisite before enablement/proof.
6. **Larger workload profiles.** Qualify explicit file/body/count profiles and
   supported child/runtime envelopes. Do not automatically activate deferred
   10240 or every tier.

Canonical object grammar/roots, private generation layout, Exec wire lifetime,
read framing and runtime capabilities each have distinct gates. A private
layout change is not permission to change canonical identity. The selected
end-state uses explicit versioned namespace/construction policy where needed;
v1 compatibility retains its exact accepted partitions and expected roots.
New profile roots are independently qualified, never relabeled equal to v1
because logical bytes match. C5's fixed stack profile/scope requires explicit
new-stack import rather than an in-place silent conversion.

C2 schema10 constrains object roles1–13. New parent roles therefore require
the selected schema11/capability boundary. Under proven deployment quiescence,
stage/copy exact locator metadata in bounded indexed batches, compare ordered
rows, swap table names in a bounded schema transaction, retire old rows in
bounded batches, drop only an empty retired table and finish the version
transition. Preserve pack BLOBs, save identities and ordinals. No open-time
automatic migration, full-table MEMORY-journal transaction or mandatory payload
copy is used. If all configured Store users cannot be proven closed, migration
refuses before effects. Metadata high-water can remain allocated as SQLite
freelist after logical cleanup; count it honestly. See SERVER's exact state,
Unknown custody and maintenance-exclusive contract.

Binding a parent-index root into a filesystem descriptor proves identity of
the pair, not its semantic correspondence. Build/import proves all indexed
parents against actual directory bindings. Incremental use requires verified
base validity at the C1/Server boundary, with exact published provenance or a
full validation of untrusted imported roots. C2 authenticates bytes and remains
agnostic about namespace semantics.

Current file4GiB/body8GiB/prepared256MiB/count65,536/nativeWRITE8MiB and
handle/view/index policies are audited separately. Use checked u64 logical
offset/count contracts and explicit byte admission, while initial compatibility
proof retains current profiles. A larger configuration is accepted only after
its complete path and any lower internal ceilings are supported.

## 10. Source-size and verification status

The exact baseline production totals are reference65,417, Core70,279 and
combined135,696. The counter/version/per-file report is retained in
[BASELINE-LOC.json](BASELINE-LOC.json). Existing production was verified unchanged
before counting. Documentation and ASCII diagrams do not contribute production
LOC. Current implemented production delta for this specification task is0.

Forecasts in the three packets are implementation estimates with explicit
scope/removal assumptions, not measured after totals. The final integration
forecast reconciles shared files and packet ownership; runtime SQL remains
production source. Every future commit must recompute exact first-parent and
staged/committed totals after code exists, separately reporting reference/Core.

### Integrated implementation LOC forecast

The following is an **ESTIMATED planning envelope**, not a measured after tree.
Each scope is exclusive: Server owns prepared/file-set/profile grammar, C1/C2
construction and metadata migration; concurrency owns native I/O/lifetime/view
framing and SDK/Sandbox creation/import routing; Workspace owns private
generation/owner/Commit semantics. Projection and observation are separate rows.
C5's existing opaque profile/scope/genesis initialization contract is reused;
no C5 schema change or payload/history retirement is counted as simplification.

| Exclusive scope | Estimated additions | Estimated deletions/replacements | Estimated signed net |
| --- | ---: | ---: | ---: |
| Workspace private v3, live/generation/custody/Commit | 7,000–11,000 | 5,000–8,000 | -1,000 to +6,000 |
| Server/C1/C2/stream profiles and metadata migration | 7,800–13,050 | 2,600–4,800 | +3,000 to +10,450 |
| Daemon/Bridge owned I/O/SDK/Sandbox concurrency | 4,050–6,850 | 1,700–2,350 | +1,700 to +5,150 |
| FUSE exact actor/quota/cancellable kernel-session adapter | 2,500–4,500 | 500–1,000 | +1,500 to +4,000 |
| Bounded complete operation observation/attribution | 500–1,000 | 100–200 | +300 to +900 |
| **Combined independent-bound envelope** | **21,850–36,400** | **9,900–16,350** | **+5,500 to +26,500** |

Mathematical projected Core range is **75,779–96,779**, reference remains 65,417,
combined **141,196–162,196**. A rough working planning midpoint is about +16,000
production lines; uncertainty is substantial until the implementation and
retirement scopes are staged. These figures neither impose a LOC growth target
nor justify unused scaffolding. Reusing/splitting existing algorithms is already
assumed. Safety checks, lifecycle authority and actual supported features are
never deleted/minified solely to make the number smaller.

Optional strict sibling filesystem/user-namespace confinement is excluded
(concurrency's separate estimated900–1,800 lines), as are DB durability/mmap/S3
adapters and unrelated root-reference retirement. Source counting is exact at
the baseline; future estimates must be replaced by exact commit comparisons.

No product tests, builds, benchmarks or host capability probes run in this
research task. Local link/anchor/whitespace validation and source-size counting
are document checks. [ACCEPTANCE.md](ACCEPTANCE.md) defines the future focused
checks, unchanged Family2 reuse and every remaining qualification obligation.
