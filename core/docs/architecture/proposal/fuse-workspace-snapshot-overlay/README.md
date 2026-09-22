# FUSE, Workspace, snapshot overlay and Commit integration

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> Implementation planning baseline finalized 2026-09-21. The design is informed by reviewed main
> `152b9c3a2e8ec2536a1d63601b681e1f7ef34455`; the v0.1.6 comparison uses release
> `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`. The R1 read-only Linux implementation and functional mounted proof are recorded
> in [08](08-readable-implementation.md). That R1 checkpoint does not qualify writable behavior;
> later scoped functional proofs are linked below. Performance and durability remain unqualified.

**Current owner-directed completion plan:** [51 — bounded Linux implementation](51-implementation-completion-spec.md)
supersedes the DSH-first continuation and large-workload gates for the current
phase. Finish the missing required operations end to end, keep existing numeric
limits, and verify a small real-daemon workflow with two explicit Commits.
DSH, scalability expansion, stress and speed/R6 qualification are deferred.
The historical results below remain unchanged; use51 for the next agent's task.

**Current namespace implementation state:** [58 — closure report](58-issue179-closure-report.md)
records the bounded Linux implementation as complete per
[51 section 7](51-implementation-completion-spec.md#7-definition-of-implementation-phase-completion):
every required operation works through native Workspace and its applicable
actual FUSE path, the real-daemon small-project scenario passes two explicit
Commits, and the final whole-core checks are recorded (host 702 passed /
0 failed / 3 ignored; Linux 700 passed / 0 failed; clean clippy, build, fmt,
boundary guard and guard self-tests; the Linux clippy component gap remains an
owner item). It lists the accepted bounds, optional unsupported features,
deferred load work and historical open failures separately. The push, the
merge to the main line and the issue closure remain owner acts; no release
claim is made.

**Previous namespace implementation state:** the three tasks [55](55-issue179-realdaemon-handoff.md)
section 5 left are complete. Task 1 (`a9657b85f`) finished the 51 section 6
real-daemon scenario: two explicit Commits, public saved-state reads, root A
readable after B, remount, acknowledged read and checked Unmount + CloseClean
(`core/target/pair1-evidence/round56/t1-small-project-0j`), with all ten native
namespace selections re-passing on the fixed source. Task 2 (`f802cc124`) added
the mounted FUSE syscall evidence for the six namespace operations and fixed the
live link-count presentation defect the route exposed
(`core/target/pair1-evidence/round57/final-ns-mounted-{kernel,durability}-01`,
plus `round57/final3-ns-<case>-01` and `round57/final3-t1-small-project-01`).
Task 3 refreshed the operation matrix and the rule documents and gave the exact
reproduction commands a permanent home
([01 section 7.4](01-workspace-fuse-contract.md#74-phase-evidence-and-exact-reproduction-commands));
[57](57-issue179-architecture-matrix-handoff.md) records the invariants that
update states, including the live namespace link-count presentation with its
baseline-gated guards. The committed-directory move refusal, rename
exchange/whiteout and the re-resolved base-identity link-count corner are
declared limitations, not oversights. The final whole-core checks and the #179
closure report remain for the final round; the merge and issue closure are
owner acts.

**Earlier namespace implementation state:** [54 — third-round closure handoff](54-issue179-namespace-closure-handoff.md)
records commits `5e4a89d0a` and `a7356d665`, which raise the registered selections
that pass from eight of sixteen to sixteen of sixteen: all ten `namespace_route.py`
cases and all six `mkdir_route.py` cases, with the stage route's ten selections
re-run as a regression check and the whole-core checks clean. It records the four
owner rulings that round used, the admission and re-anchor rules it changed, and
what is still open, including the section 6 real-daemon integration. The earlier
handoff [53](53-issue179-namespace-completion-handoff.md) remains the record of
the round that took the count to eight.

**Historical namespace implementation state:** [53 — second-round handoff](53-issue179-namespace-completion-handoff.md)
records commit `578ef7012`, which raises the registered selections that pass from
four of sixteen to eight of sixteen: `namespace/setattr`, `mknod`, `link` and
`rmdir`, and `mkdir/semantics`, `refusals`, `reserve_denied` and
`reserve_unknown`. It names the exact remaining cause of each failure still open
and carries the task prompt for the next implementation round. The earlier
handoff [52](52-issue179-handoff.md) remains the historical record of the round
that added the operations.

**Fresh streaming, first live attempts:** [Round49](49-fresh-file-streaming.md)
implements the fresh-file streaming prerequisite, and its two live selections have
now run once each. The largest already-preinstalled DSH file18,259,144 bytes
streamed through one mounted writable Workspace in140 caller buffers and completed
one ConstructFile/Commit plus one4-byte EditFile/Commit: **PASS** in40.81s. The
captured-G replay selection **FAILED** at its post-rebase `refuse_extra`
assertion and stays open: the +1 write returned `Capacity` with no observed state
change but delivered one native `Inspect` first, which the caller forbids. Four
registered regressions against the same frozen product passed. The budget-stop
handoff [50](50-budget-stop-handoff.md) remains the historical continuation
record; the full prepared tree, its one complete upload Commit, incremental
Commits and matched R6 are still unqualified.

**Fresh-file prerequisite:** [portable metadata construction](41-construct-portable-metadata.md)
is implemented and functionally verified through the existing Service save owner.
[Prepared fresh regular files](42-prepared-files.md) now verifies the existing
direct and C5 save routes with zero/nonzero content and derived references.
[Native file creation](43-native-create.md) is implemented, with13 registered
functional selections passing and the capacity gate still failed/open. Its
labelled diagnostic PASS does not replace that failure. [Mounted CREATE](44-mounted-create.md)
now verifies kernel/SDK creation, handle ownership, entry coherence and G/D1
reconciliation. [Shared symlink-content construction](45-construct-symlink.md)
now verifies exact target saves through the existing C1/C2 path. [Prepared fresh
symlinks](46-prepared-symlinks.md) verifies shared direct/C5 declarations with
canonical reference counts. [Native symlink creation](47-native-symlink.md) now
verifies target ownership, local/canonical Readlink, G/D1 and typed failures.
[Mounted SYMLINK](48-mounted-symlink.md) now verifies kernel/SDK creation,
checked notification custody and exact target/readlink boundaries. Complete workload
admission remains open.

**Mounted namespace continuation:** [mounted mkdir](40-mounted-mkdir.md) now verifies
kernel creation, coherent SDK creation, stable directory handles and checked entry
notification failure/recovery. It uses [native mkdir](39-native-mkdir.md)'s maintained
directory state, exact saveability admission and G/D1 reconciliation. Full prepared upload and R6 remain open.

**Shared directory prerequisite:** [prepared directory construction and metadata](38-prepared-directories.md)
is implemented and functionally verified under the existing Service save path.
Native and mounted directory creation now use this primitive. Complete prepared
upload remains open.

**Current continuation:** [authenticated Commit](37-control-commit.md) and its writable
startup/dirty-shutdown prerequisites are verified for their declared scope. [Authenticated Attach](36-control-attach.md) is verified for its declared scope,
building on [failed native Attach ownership](35-failed-attachment-ownership.md). [Authenticated Mount](32-control-mount.md)
and [cancelled pipe I/O](33-cancelled-pipe.md) retain their verified scopes. The owner-selected
[preinstalled DSH workload](34-preinstalled-dsh-workload.md) is prepared and pinned;
actual mounted upload/Commit/R6 remain open.

**Continuation task handoff:** [30](30-continuation-handoff.md) records the exact
current checkpoint, remaining operation sequence and loose coupling boundaries.
[31](31-source-map-and-loc.md) gives actual per-file/folder/crate production LOC
and physical lines; its counts replace no historical planning or evidence pins.

**Mount failure prerequisite:** [retained native ownership](29-mount-failure-ownership.md)
corrects partial startup failure and deadline handling before remote Mount is
exposed. The native failure/daemon startup routes and selected regressions pass;
the original admission-oracle failure remains recorded. Remote Mount is still a
separate operation.

**Next daemon lifecycle operation:** [authenticated CloseClean](28-control-close-clean.md)
adds separate authority for existing native clean closure and preserves inspection
of the exact closed target. Five new actual daemon/Linux cases and eight affected Unmount regressions pass.
Wider control/writable work remains open.

**Daemon lifecycle control:** [authenticated Unmount](27-control-unmount.md)
adds operation-specific authority and exact target/incarnation matching through
the existing native protocol. Its checked entered-attempt result distinguishes
Unmounted from Retained; transport loss remains Unknown. Eight actual new daemon/Linux cases and the existing Status regression pass.
Wider read/lifecycle/writable controls remain separate operations.

**Next size operation:** [size SETATTR and truncating OPEN](26-mounted-resize.md)
is implemented with eight new actual mounted proofs and one native API subset.
It preserves the separate Linux post-reply completion order and failed-open handle
cleanup. Authenticated lifecycle/writable controls and retained round25 limits
remain open.

**Mounted WRITE round:** [existing-file kernel writes](25-mounted-write.md) add
an explicit direct-I/O writable projection and a bounded origin/reply permit,
using the existing private backing and incremental Commit path. Size SETATTR,
truncating open and daemon writable controls remain separate dependencies.
The record explicitly retains unqualified RWF append variants and concurrent
SDK-size mapping/splice routes; this is not full R4 or Pair 1 qualification.

**Write prerequisite:** [routine healthy-owner reclamation](21-routine-reclamation.md)
adds synchronous consumer-wide retirement before input, mutation and submission
admission. Failed/partial owners remain charged until explicit cleanup. The next
operation that follows is [native handle write](22-handle-write.md); actual
writable FUSE and R6 remain open.

**Shared read prerequisite:** the pending write frontier exposed
[logical-fragment/native-frame coupling](23-native-stream-fragmentation.md).
The focused correction coalesces small local input/output fragments without
raising wire limits. Its original failure and diagnostic remain source-pinned;
the handle-write operation is verified separately afterward.

**Native write update:** [handle write, append and Zero gaps](22-handle-write.md)
share the bounded mutation and Commit pipeline. Twelve actual native cases pass,
including the complete 256-edit frontier after the separate transport correction
and live writes during actual service save. Mounted SDK coherence and kernel
write/append/truncate callbacks remain required projection work.

**Mounted SDK-coherence update:** [the bounded reply/invalidation binding](24-mounted-sdk-coherence.md)
now permits SDK mutations with checked visibility through a real RO Linux kernel
projection. Old replies exclude publication, one pending notification excludes
the next mutation, and known publication failures retain their exact receipt and
any READY truncating-open handle. Six real mounted cases and two API completion
subsets pass. Kernel writes/append positioning/SETATTR, remote SDK controls, npm
and R6 remain open.

This is the current detailed Pair 1 document packet. It consolidates the earlier
Workspace/FUSE discussions, platform ruling, file plan, POSIX decisions and
merged Pair 2 integration. Earlier dated research and measurements remain
historical evidence; they are not rewritten or promoted by this consolidation.
The implementation sequence, component ownership, public boundary and acceptance
requirements are now the planning baseline. The concrete control/backing/shared
input decisions listed in 04's R0 are explicit first-round outputs, not silently
assumed completed implementations or permission to skip their review.

**Main synchronization, 2026-09-21:** integration combines local checkpoint
`81ace2778201036e9b1ca3040c63949e595f5971` with upstream
`b0260df3a2ffc371773cd062feafd4b5e435bf1e`. The local checkout now contains
bridge, daemon, service and history alongside C1/C2/telemetry. The packet's
reviewed source basis remains `152b9c3a2`; it is not relabelled as a review of
the later source. In particular, [the concurrency controls](../../../../../docs/roadmap/0.1/0.1.7/concurrency-controls.md)
include C2 schema 8 and configured writer admission. Reconcile later shared API,
resource and namespace changes before implementing a source-pinned prerequisite.
Start a new implementation task from synchronized `main`; record the actual
starting commit and use the current proposal packet.

**Crate layout (owner update, 2026-09-21):** `layerfs-fuse` and
`layerfs-workspace` are separate libraries under `core/crates/`.
`layerfs-daemon` assembles them in one execution-side process. Workspace groups
its implementation into `runtime`, `filesystem`, `overlay`, `backing` and
`commit`; [04's file plan](04-implementation-and-verification.md#3-proposed-production-layout)
owns the paths, public boundaries and revised size allowances. This supersedes
the earlier layout with both implementations inside daemon modules.

**Implementation update:** R0-R shared reads and the declared R1 Linux read-only
mount are implemented; [08](08-readable-implementation.md) records exact proof,
limits and retained failures. Next add the separate R1-C daemon-control route before
host-SDK/container-Workspace claims. Required shared read metadata is a
prerequisite to its advertised callbacks. Writable backing and shared-input
decisions close before their dependent rounds. The ready-to-use
[implementation handoff](07-implementation-handoff.md) carries the full
sequence and persistent owner decisions into the next task.

**R1-C update:** the first independent control operation, authenticated daemon
[Status](09-daemon-status.md), is implemented and verified against the actual
container-mounted Workspace. Remaining lifecycle/edit/Commit controls are still
open; Status does not qualify those routes.

**Readable admission correction:** [the registry now grows on demand](11-registry-admission.md), with retained capacity and failed cleanup still charged. It no longer preallocates MAX_COUNT.

**R2 prerequisite:** [the attribute hierarchy correction](10-attribute-hierarchy.md) fixes shared wide-tree construction and validates bounded descent before the portable metadata operation. Existing historical roots and receipts keep their original identities.

**R2 update:** [portable metadata save](13-portable-metadata.md) is implemented and verified through the real authenticated daemon/service route. It preserves content and generic attributes; mounted mutation, snapshots and Commit still depend on R3/R4. The full-core check also prompted the focused [early-refusal correction](12-early-refusal.md).

**R3a update:** committed in `4629b8d62de1e0df8a7bd9808b59a86d7c6669f3`, [immutable owned payload input](14-owned-payload.md) now uses bounded private direct I/O and explicit retention/reclamation. Its real Linux functional proof includes short writes and ENOSPC. The next operation is R3b local RangeEdit with a maintained disk index; visible edits and Commit are not implemented by the input primitive.

**R3b update:** [local RangeEdit and the maintained metadata index](15-local-range-edit.md) are implemented and verified through the real native service/Linux backing route, including all 104 available regular inodes and actual failure ownership. Writable mounting, capture and Commit remain open; the next public operation is stage with its required private capture/lowering.

**Stage update:** [private capture, live successor and actual StageChanges](16-stage-capture.md)
are implemented with bounded disk completion associations. Ten real native
selections, including actual C2-save progress and failure retention, passed;
readable-mount/Status and existing local operations also passed regression.
The next public operation is CommitStaged with exact known-result reconciliation.
Full writable mounting, repeated Commit and npm remain open.

**CommitStaged update:** [exact completion and live-successor reconciliation](17-commit-staged.md)
now advance the local canonical base and Branch context after known C5 success.
Twelve native selections pass, including repeated incremental commits, occupied
quota, lost replies and local failure after remote success; one corrected test
oracle failure remains recorded. Ordinary composite Commit, failure disposition,
writable mounting, npm and R6 are still open. This is an SDK route subset.

**Composite Commit update:** [ordinary Workspace Commit](18-composite-commit.md)
now reuses the service's single composite command and the same lowering/completion
owners. Fourteen actual native selections pass, including clean UpToDate before
and after a changed Commit, late D1, real save progress/loss and exact failures.
Composite replies carry no token; local result types now represent that absence.
Writable mounting, failure disposition, full npm and R6 remain open.

**Resize prerequisite:** [existing-inode set_len and logical zeros](19-resize-zero-ranges.md)
now support shrink, zero extension and same-length timestamp updates through the
shared mutation/Commit path. Nine native selections pass, including exact G/D1
truncate/zero lowering and actual-save progress; a corrected test-oracle failure
remains recorded. Writable open/handle semantics and kernel coherence are next
prerequisites before enabling mount writes.

**Portable open prerequisite:** [open rights and atomic truncation](20-portable-open.md)
now reserve a pending slot/node pin before truncate preparation and publish the
handle with the new file state. Eight native cases pass, including actual nonroot
DAC, full/contended handle admission, forget/deadline and local failure; two test
setup/lifecycle failures remain recorded. Handle write/append positioning and
writable kernel coherence/binding remain open.

**Current writable target:** the owner includes `npm install` with large and
tiny files and wants to keep memory low. Pending data uses explicit local disk
backing with bounded RAM buffers, resident indexes and snapshot state. The
earlier RAM-only candidate is superseded for this target. The
[workload and backing contract](01-workspace-fuse-contract.md#134-npm-install-and-low-memory-backing)
records the metadata-scaling/shared-operation prerequisites. The payload-input implementation is recorded in 14. Disk metadata/indexes,
visible mutations, snapshot/Commit and the full installation remain separate;
no RAM-budget increase or supported-install claim is made.

[Linux and Docker placement examples](01-workspace-fuse-contract.md#321-one-configurable-root-linux-example)
show one configurable root with `workspace/` and `private-backing/` children,
separate disk quota and shared snapshot ranges without full directory copies.
[Proposed Pair 1 variables](01-workspace-fuse-contract.md#323-proposed-pair-1-startup-variables-and-attach-inputs)
name the common root, two resource budgets and finite Workspace-count admission,
distinguish per-Workspace attach inputs, and reuse the existing Pair 3 connection
settings.

## 1. Reading order and ownership

| Document | Owns | Concrete outputs |
| --- | --- | --- |
| [01 — Workspace/FUSE contract](01-workspace-fuse-contract.md) | Application-visible filesystem and projection boundary | Architecture/deployment diagrams, storage/configuration, proposed local methods, mount lifecycle, lazy reads, complete POSIX operation table, errors and scope limits |
| [02 — Overlay and snapshot](02-overlay-snapshot.md) | Live mutable state and consistent capture | B/G/D1 data model, immutable version ownership, piece/tombstone examples, local Commit serialization, capture/completion algorithms, bounds and concurrency invariants |
| [03 — Commit integration](03-commit-integration.md) | Consumption of existing service/history operations | PreparedChanges mapping, repeated incremental Commit scenarios, exact stages, Commit/Layer distinctions, failure observations, continuing authority and current upstream bounds |
| [04 — Implementation and verification](04-implementation-and-verification.md) | Delivery sequence and proof | Proposed production files/LOC ranges, prerequisite graph, per-operation milestones, external test matrix, qualification and measurement discipline |
| [05 — Exact v0.1.6 source comparison](05-v016-source-comparison.md) | Reference FUSE/transport/memory mechanisms and fair migration comparison | Pinned call paths, cache/scheduler/transport/backing costs, preserved behavior, improvement targets and compatibility risks |
| [06 — Benchmark qualification map](06-benchmark-qualification-map.md) | Existing harness and release evidence applicability | Complete family mapping, qualified historical values/statuses, exact timing/resource scopes and prerequisites for matched v0.1.7 qualification |
| [07 — Implementation handoff](07-implementation-handoff.md) | Continuing task prompt | Start with R0-R/R1, carry source/crate/resource decisions, execute one operation per round and report actual proof boundaries |

These documents have separate responsibilities. Callback semantics are defined
in 01; capture mechanics in 02; shared history integration in 03; implementation
and proof obligations in 04; reference/source comparison in 05; retained benchmark
evidence and coverage mapping in 06. A change crossing a boundary updates both owners
instead of duplicating a second definition elsewhere.
Document 07 summarizes execution instructions and links these owners; it does
not introduce an alternative API, backing design or measurement contract.

## 2. Architecture at a glance

```text
 EXECUTION MACHINE / CALLER'S KERNEL              SERVICE MACHINE
 (same host, Docker executor, or remote node)      (local or remote)

 +--------------------------+
 | shell / editor / program  |
 +------------+-------------+
              | ordinary filesystem syscalls
              v
 +--------------------------+
 | local Linux kernel       |
 | VFS + FUSE mount          |
 +------------+-------------+
              | kernel callbacks
              v
 +-------------------------------------------------------+
 | layerfs-daemon                                        |
 |                                                       |
 | layerfs-fuse library                                  |
 |   handles kernel types, replies, errno and mount life   |
 |             |                                         |
 |             v                                         |
 | layerfs-workspace library                             |
 |   WorkspaceHost owns Workspace lifecycle              |
 |   semantic methods shared by FUSE and SDK edit binding |
 |   namespace + inode state + semantic handles           |
 |   pieces + extent references + consumer RAM budget     |
 |   frozen G + live G+1 + exact completion association    |
 |             +--> private disk backing                 |
 |             |    payload extents + indexed metadata   |
 |             |    bounded buffers/indexes/open files   |
 |             |                                         |
 |             `--> existing bridge client               |
 +-------------+-----------------------------------------+
               |
               | authorized logical operations + bounded bytes/results
               | profile 1: Inspect / ReadFile / file saves
               | profile 2: HistoryQuery / HistoryCommand
               v
                                      +---------------------------------+
                                      | layerfs-service                 |
                                      | authorization + admission       |
                                      | common operation handlers       |
                                      |       |                 |       |
                                      |       v                 v       |
                                      | local C1 + C2      C5 history   |
                                      | construction/      stages,      |
                                      | authentication/    Branches,    |
                                      | storage            Commits,     |
                                      |       |            Layers       |
                                      |       v                 v       |
                                      | content Store     catalog       |
                                      +---------------------------------+
```

The two databases remain service-owned and separately acknowledged. FUSE is not
part of the service. The bridge carries logical operations, not kernel callbacks,
canonical-object requests, SQLite handles, SQL or physical pack locators.
The daemon depends on both libraries; FUSE depends on Workspace; Workspace
depends on neither FUSE nor the daemon. Library separation creates no extra
process boundary or round trip. Backing remains local to the execution process.

SDK edits enter the same Workspace through their explicit semantic binding;
they are not simulated FUSE writes. WorkspaceHost creates/owns finite handles
and performs lifecycle admission, without holding its registry lock across an
operation. An external host/cluster controller needs the separately reviewed
daemon-control binding; current headless service forwarding does not provide it.
The [control and cluster diagram](01-workspace-fuse-contract.md#21-public-control-sdk-edits-and-cluster-integration)
shows those entry paths and their owners.

## 3. Decision set

| Topic | Direction | Classification |
| --- | --- | --- |
| Platform | Linux FUSE first; macFUSE and Windows filesystem adapters later | Owner direction; later adapters unimplemented |
| Mount roots | `/workspace-id` is a managed immutable mount identity; descendant names may change under supported operations | Owner direction |
| Mount layout | One mount per Workspace, with distinct kernel sessions | Proposed; kernel-session isolation does not establish backend fairness |
| Crate ownership | `layerfs-fuse` and grouped `layerfs-workspace` libraries, assembled by `layerfs-daemon` | Owner direction; replacement source under `core/crates/`, reference crates are not reused as dependencies |
| Mutable state | Public local WorkspaceHost/Workspace API in `layerfs-workspace`; FUSE and SDK bindings use the same semantic owner | Proposed integration boundary; remote/container management binding required before that route is claimed |
| Service ownership | Existing C1/C2 and C5 handlers; no pending payload overlay or FUSE handles in C5 | Source foundation plus integration requirement |
| Snapshot target | Frozen G with live G+1; applications continue during save | Full Pair 1 target; requires implementation/proof |
| Capture point | Coherent local frontier rotation, no payload copy, canonical construction or network call in capture | Proposed algorithm, not a measured latency claim |
| Pending byte backing | Explicit local disk backing with bounded RAM state; file-range COW, no whole-file copy-up or automatic spill | Current writable target; disk/metadata design and qualification remain required |
| Workspace allocations | 8 MiB total per consumer across Workspaces, generations, readers and owned state/buffers | Proposal requiring decision/qualification; not total RSS |
| Workspace count | Explicit positive per-daemon MAX_COUNT, counting attach reservations and retained/closing state; no chosen numeric default | Proposed admission input; machine-wide aggregation belongs to the launcher/orchestrator |
| Saving | Explicit lifecycle action; one logical submission slot per Workspace, initially one retained frozen submission per consumer; no Commit per write/close | Proposed runtime policy; retained state is distinct from an active network permit |
| Construction | One construction producer per ordinary operation | Standing owner rule; namespace init retains its exception |
| Concurrency limits | Preserve current service-wide two active operations, including reads, and C2's two private saves per Store | Source facts; changing/configuring limits is discussion only |
| Extra content caching | No new userspace payload cache/prefetch in the initial profile; exact v0.1.6 had a shared 32 MiB immutable range/name/page cache | Explicit resource/performance trade; 8 KiB is only the reference acquisition-prefetch cutoff, not proof of a cache miss |
| Unknown outcomes | Retain exact state and report; no automatic replay, token refresh or guessed discard | Required failure boundary |
| Durability | MEMORY journal, synchronous OFF, no added sync/WAL or crash-atomic claim | Existing contract |

The 8 MiB allocation envelope, callback deadline and scheduling choices are
proposals, not owner-approved performance/resource results. A larger service
operation bound does not enlarge those policies implicitly.

Supported filesystem operations continue during the actual save; only capture
and coherent publication use the short Workspace state lock. The
[concurrent-operation rules](02-overlay-snapshot.md#34-multiple-operations-continue-during-commit)
define the cut and remaining capacity/remote limits. The
[ownership and no-copy-up rules](02-overlay-snapshot.md#102-bounded-ram-without-whole-file-copy-up)
require charged retained allocations/backing and streaming
capture/save paths. Safe ownership alone is not a memory-consumption bound.
The [exact v0.1.6 comparison](01-workspace-fuse-contract.md#131-what-the-reference-actually-stores-on-disk)
shows that piece-based no-copy-up already exists there. Its default 1 GiB
per-Workspace spool-byte policy was not equivalent to the earlier 8 MiB total
consumer RAM-only proposal. The current target keeps separate RAM/backing
accounts; numeric disk limits and actual resource behavior still need design
and qualification rather than inheriting the reference's limits or claims.

The full source review corrected the earlier blanket statement that files above
8 KiB were uncached in the release. It also found repeated full-frontier record
exports, host-side frozen-input materialization and tighter new consumer admission
than the reference's per-Workspace captures. Read 05 before classifying a change
as an improvement; read 06 before quoting mount/Commit/transfer or memory numbers.

## 4. The three distinct kinds of state

```text
 local live state           local frozen input           service history stage
 ----------------          ------------------           ---------------------
 G+1 edits and handles     G's immutable versions        exact Workspace/token
 current namespace        captured Branch context      saved candidate root
 current byte pieces      stable byte references       captured expectations
          |                         |                           |
          | continued reads/writes  | file saves +              |
          |                         +-- PreparedChanges -------->|
          |                                                     |
          |                    known Commit result              |
          +<----------------------------------------------------+
            reconcile G only; preserve every later edit
```

A local snapshot is not a C5 stage. Stage success is not Branch Commit. Branch
Commit is not Layer publication. Querying an observable stage is not a generic
exactly-once recovery protocol. Document 03 owns those distinctions.

### 4.1 Incremental pipeline and where work belongs

```text
 supported write/namespace mutation
             |
 reserve RAM/backing + prepare from current version
             |
 install new bytes/metadata before coherent publication
             |
 live view = unchanged base references + local changed extents/records
             |
 explicit Commit: short capture cut
             |
             +----------------------------+
             |                            |
 frozen G: immutable frontier          live D1: later operations
             |                            |
 stream changed file inputs               | reads/writes continue within bounds
             |                            |
 C1 file construction + C2 save            |
             |                            |
 Commit(PreparedChanges)                   |
   C1 filesystem + C2 save                 |
   C5 exact stage/conditional Commit       |
             |                            |
             +-- known own completion ----+
                       |
       next base = acknowledged R1/head K1; keep D1
       next Commit captures only its remaining changes
```

| Phase | Required work placement / reuse | What is not established by that property |
| --- | --- | --- |
| Mutation | Write accepted new bytes once to backing; update changed namespace/inode/piece state; preserve base-range references | Zero-copy callbacks, zero local I/O or unlimited pending writes |
| Capture | Publish already-maintained immutable roots/frontier with reserved descriptors | A measured latency, literal lock-free execution, or permission to scan/clone the entire dirty set under the cut |
| File save | Lower G's final edits and stream their bytes; unchanged file roots need no redundant save | Every edit reading only changed bytes; C1 boundary, comparison and representation work still applies |
| Filesystem/history save | Use the existing shared staging/Commit handler once for G; preserve unchanged structure where C1 does | O(changes) total work; topology validation, storage checks and current request bounds remain |
| Completion | Associate only the exact result with G, advance known own baseline and preserve D1 | Automatic rebase, replay, crash recovery or immediate release of every old extent |
| Reclamation | Release backing only after all live/frozen/read owners are gone; account dead space and temporary copies | Free disk capacity merely because a name or logical slice disappeared |

These are requirements for avoiding unnecessary work and preserving semantics.
Actual speed is a later matched mounted result. Document 03 contains the precise
service work/exchange ledger; 04 defines the observations and tests needed to
verify each property and account for costs outside the capture cut.

## 5. Status vocabulary

| Label | Meaning |
| --- | --- |
| Source-backed at the pin | Read from the reviewed source, within its actual support envelope |
| Proposed | Design to implement and qualify; it is not a shipped capability |
| Open prerequisite | Required before exposing a dependent callback or lifecycle operation |
| Deferred | Explicitly outside the initial profile; costs and compatibility limits are stated |
| NOT_RUN / UNVERIFIED | No qualifying execution evidence for the stated operation/schedule |

This packet provides implementation inputs; it creates no empty production
modules, provider registry, alternative transport, benchmark runner or recovery
engine. Source-file LOC ranges are working guidance, not quotas. Correctness,
authentication, resource bounds and required validation are never removed to
meet a line target.

## 6. Source basis and existing owners

The requested destination checkout may lag the reviewed main pin and contains
other work. The document move does not update that checkout's product code.
Source-backed links therefore identify immutable reviewed GitHub blobs where a
local path could resolve to an older or missing implementation. Implementation
must select its actual source tree and review any changes from this pin.

| Existing owner | Authoritative reference |
| --- | --- |
| Pair 2 history API | [bridge history contract at 152b9c3a2][history-contract] |
| Pair 2 service composition | [history operation handler][service-history] |
| Pair 2 semantic architecture | [16-history.md][history-architecture] and [repaired boundary][repaired-boundary] |
| Pair 3 common request/bounds | [request contract][request-contract] |
| Pair 3 Workspace/FUSE forward boundary | [integration proposal at reviewed pin][future-fuse] |
| C1 read/input semantics | [filesystem input][filesystem-input] and [file range reads][file-read] |
| Exact legacy reference | [v0.1.6 FUSE source][legacy-fuse] |
| Coding and documentation rules | [repository AGENTS](../../../../../AGENTS.md), [core AGENTS](../../../../AGENTS.md), [documentation policy](../../../../../docs/general/documentation-policy.md) |

These sources are reused, not redefined. In particular the packet does not
duplicate C5 schema/Commit identity, Pair 3 framing/crypto, or C1/C2 canonical and
physical algorithms. The predecessor research remains useful for history, but
its old 64 MiB/A=1/10-second protocol-limit observations are superseded by the
current source table in 03.

## 7. Qualification and issue state

Pair 2 implementation and remediation are merged. Its carried verification does
not establish a core mount or finish the outstanding history schedules. The
[qualification owner #210][issue210] retains H04 real overlap/reverse failure,
H06 independent-stack upload overlap, H08 actual boundary/identical-root failure
schedules and H14 full history-consumer substitution.

The earlier #179 closure at 2026-09-20 23:33:35 UTC was an unintended automatic
effect of PR #212's closing-keyword wording in a negated sentence. The wording
was corrected and #179 reopened at 23:56:59 UTC. This is issue history, not
implementation or qualification evidence. This documentation task changes no
issue state and makes no release/closure claim for #179/#180/#181/#190/#205/#207/#210.

## 8. Entry points preserved during consolidation

[The original Pair 1 overview](../01-projection-and-runtime.md) becomes a short
navigation entry to this folder. This README and the six numbered documents are
the maintained packet in the requested checkout. Earlier dated checkpoints and
receipts remain intact as source material; moving the active design is not
permission to rewrite historical facts, measured rows or retained failures.

## 9. Audit disposition: incremental COW and the Commit pipeline

The 2026-09-21 parallel audit covered local COW/snapshot ownership, shared
C1/C2/history integration, all reference FUSE modules and relevant transport
paths, and the complete main benchmark-family/release evidence map. The table records
documentation resolutions and remaining implementation work. It is not a product
audit PASS, a performance result or approval to change a shared bound.

| Finding | Resolution in this packet | Remaining proof / owner |
| --- | --- | --- |
| Main diagrams still described RAM-only payload while npm required larger backing | Current architecture, storage contract, overlay algorithm and delivery plan now consistently use explicit local disk backing with bounded resident state | Actual backing implementation; 01/02 behavior and 04 rounds |
| COW could be confused with whole-file clone or filesystem reflink | Pieces retain unchanged canonical ranges and immutable local extents; initialize bytes before publication; no payload/namespace copy-up | Small edit to large base, write failure, old reader/G/D1 overlap; 02/04 |
| A short capture could hide a dirty-set scan, metadata-page enumeration or recursive drop | Maintain frontier/view roots during actual mutations; the cut retains roots/descriptors and does no bulk work | Count/cause observations of capture, subsequent metadata traversal and retirement; 02/04. No latency claim follows from the design |
| Repeated Commit could resubmit unchanged files or lose newer edits | Per-captured-inode/version submission, exact G results, next own R1/K1 baseline, G-relative successor coordinates and one filesystem save path | Same mount across several Commits, hard links, metadata-only changes and concurrent D1; 03/04 |
| The word incremental could conceal broader C1/C2 work | Explicit ledger includes comparisons, mapping/child-page reads, sibling work, alias validation, physical reuse checks and UpToDate work | Real counters/telemetry from the selected operation and matched mounted measurements; 03/04 |
| Disk payload alone could leave unbounded metadata, descriptors, dead extents or page cache | Separate RAM/backing accounts; bound resident metadata/index/FD state, reserve before visibility, retain physical ownership through G/read/unknown outcomes | Metadata/index algorithm, disk quota, segment/dead-space limits, reclamation and kernel residency policy; 02/04 |
| Streamed file bytes could be mistaken for scalable complete history input | Existing Source supports bounded file delivery; current PreparedChanges remains a bounded complete record set. Paged wire delivery alone does not solve receiver accumulation | Shared live metadata/new-inode/symlink operations and whole-generation input beyond current record/byte limits; 03/04 |
| Non-pausing Commit could be claimed from a delayed terminal only | Require D1 write/read progress while G's real service save remains incomplete; retain admission, capacity and remote-I/O caveats | S-11 and the expanded snapshot/resource schedules in 04 |
| An 8 KiB threshold was mistaken for an uncached release band | Exact-tag audit found the separate shared 32 MiB immutable range/name/page cache and kernel KEEP_CACHE | Actual hit/miss/eviction, callbacks and logical requests in read/exec comparisons; 05/06 |
| Small transport pages were mistaken for bounded full-generation work | Exact release repeatedly collects all frontier IDs per page; host input and current prepared requests also have aggregate materialization limits | True bounded cursor/index and complete-generation input contract, with no unchanged-path scan or hidden partial Commit; 02/03/05 |
| Single build admission was mistaken for single internal construction worker | Source-default worker selection, recorded sample settings and the current one-producer rule are now separate | Enforce selected default wiring and exact matched sample worker identity; 04/05/06 |
| Low create-path spool residency was generalized to dense rewrites | Historical rewrite/cache-amplification failure and limited SDK verifier scope remain visible | Full operation-specific kernel/cache and independent verification evidence; 06 |
| Faster/smaller could be asserted before a comparable mount exists | Structural work requirements are separated from numeric performance/resource qualification; full installation and all refused/unrun cells remain visible | Existing #207/#210 evidence ownership and unchanged measurement protocol |

Before writable implementation exposes the full selected workload, choose the
backing quota scope/value, segment/extent limits, allowed dead slack, resident
metadata/index/FD limits, fragmentation/reclamation policy and kernel-residency
treatment. R0 also explicitly resolves the initial frozen-admission proposal
against required multi-Workspace schedules; one retained G per consumer is
tighter than the reference and cannot establish those schedules by assumption.
The 8 MiB consumer allocation target remains unqualified; none of
these choices may silently expand it. Complete the required shared operation
contracts in their own rounds. A missing decision is reported as a design gap,
not replaced with inherited constants, hidden intermediate Commits, automatic
replay, an extra construction worker or a reduced workload.

### 9.1 Improvement list and its evidence status

| v0.1.7 direction relative to the reference | Status |
| --- | --- |
| Reuse separate C1/C2/C5 and the authenticated logical service/bridge surface | Shared foundation exists; Pair 1 consumption remains to implement |
| Keep filesystem algorithms local to Workspace and service algorithms local to their owners; replace the old syscall-proxy duplication | Planned boundary/code simplification; required ordering and errors must survive |
| Replace repeated full-frontier vector construction and scans with maintained immutable indexes and bounded cursors | Source-grounded optimization target; new implementation and measurements required |
| Bound resident namespace, piece, saved-root, descriptor and cleanup state as well as payload buffers | Planned scalability/ownership improvement; index format and resource qualification still required |
| Submit each changed captured inode payload once and use one filesystem construction path per logical Commit | Planned integration efficiency; explicit counters and complete output proof required |
| Use exact local publication/remote outcome/reconciliation reporting, with bounded failure retention | Explicit contract improvement; no automatic retry/recovery guarantee |
| Separate kernel/transport/Workspace interfaces, configurable common root and finite Workspace admission | Concrete implementation-plan/API decisions, not an installed runtime |
| Faster FUSE, SDK edit, Commit, repeated exec or lower total memory | **Not established.** Full matched measurements are required; source advantages, fewer features, a smaller cache or stricter admission do not prove them |

Range COW, private disk segments, lazy base reads and live mutation during save
are preserved reference strengths. The zero-extra-payload-cache choice, one
frozen generation per consumer and initially unsupported operations are explicit
tradeoffs/compatibility risks. [05](05-v016-source-comparison.md#9-fair-comparison-keep-simplify-reduce-then-measure)
provides the detailed comparison and [06](06-benchmark-qualification-map.md)
maps the evidence needed to support any eventual measured improvement.

[history-contract]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/contract/history.rs
[service-history]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-service/src/operation/history.rs
[history-architecture]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/docs/architecture/16-history.md
[repaired-boundary]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/docs/architecture/proposal/commit-history/remediation-contract-20260921.md
[request-contract]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-bridge/src/contract/request.rs
[future-fuse]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/docs/architecture/proposal/service-daemon-transport/06-future-fuse-and-cloud.md
[filesystem-input]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/filesystem/input.rs
[file-read]: https://github.com/Ephemeral-AI-Lab/layerfs/blob/152b9c3a2e8ec2536a1d63601b681e1f7ef34455/core/crates/layerfs-content/src/file/read.rs
[legacy-fuse]: https://github.com/Ephemeral-AI-Lab/layerfs/tree/44cf748486863ab7c21ca47e731bd88e2b9a7b4a/crates/layerfs-fuse/src
[issue210]: https://github.com/Ephemeral-AI-Lab/layerfs/issues/210
