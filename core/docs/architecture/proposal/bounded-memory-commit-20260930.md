# Bounded-memory Commit and concurrent Workspace architecture

> **Status: Research; informative and not a product contract.**
> Proposed architecture for the next Core phase, 2026-09-30. Baseline inspected:
> `7edddbdb8e8512627aed0ed42533ef099d802384`. This document authorizes no
> implementation, benchmark, profile change, merge or release claim.

The owner requests architecture research using subagents in this thread, for
large model/database files and potentially millions of mutating chunks or
objects. Three subagents independently examined Workspace, Server/C1/C2, and
generation/publication correctness. Existing experimental branches remain
unaccepted and unmerged. No product source was changed for this study.

The [Workspace scenario catalog](../../../../scenarios.md) records the five
workload categories, source-pinned ordinary-shell cases, live/Commit pressure,
generation/failure/resource scenarios and future profile boundaries. Use its SC
IDs when selecting a design obligation or prospectively frozen proof.

The owner extends the research to
[#249 concurrent Exec / multiple Workspaces](https://github.com/Ephemeral-AI-Lab/layerfs/issues/249)
and [#219 Workspace count policy](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219).
Their ownership, resource and command-lifetime contracts are designed jointly
with generation streaming now. #249's implementation/qualification dependency
on #248 remains: design integration does not establish its memory/custody gates
or authorize concurrent delivery before that proof.

The later owner request selects a concrete replacement implementation
[specification packet](bounded-workspace-implementation-20260930/README.md),
including private v3 authority, namespace/file construction v2, metadata-only
Store v11 migration, typed C2/C5 admission, owned execution/projection and an
explicit [latest-#276/quadratic-work map](bounded-workspace-implementation-20260930/ISSUE276-AND-SCALING.md).
Use that packet for the selected end-state and implementation interfaces. This
earlier research remains its dated source analysis; no earlier result is promoted.

## Recommendation and priorities

Make Commit a pull pipeline over an immutable captured generation. Persist
operation-sized facts in quota-owned paged backing, admit every simultaneous
resident window, and install a known canonical result through a small local
selection change. Make the successor generation a separate delta over the exact
captured view, so installation does not rewrite all live extents and names.

1. **P0: whole-operation bounded memory and publication safety.** Design
   capture, replay, canonical construction/validation, local install and
   retirement together. Remove predictable proportional memory admission after
   the Branch head has advanced. Primary scope: #248/#256/#276 under #245.
2. **P1: explicit large-workload envelope and byte-based aggregate admission.**
   Review file/stream/count bounds, wide-directory interfaces, graph state,
   engine caches and active Save footprints. Sparse updates must preserve
   unchanged-subtree reuse. A streamed upload alone is an intermediate result.
3. **P2: concurrent Exec and multiple Workspaces enablement.** Design #249/#219
   ownership/resources as part of P0 now, then enable and qualify their delivery
   after #248 prerequisites. Compose operations, caches, command leases and engine
   connections. P2 is implementation/proof order, not delayed ownership design.

FUSE statfs/interruption, explicit dirty-discard/recovery and pinned-read framing
remain important adjacent correctness packets. Codec/storage tuning is lower
priority. The accepted history storage tolerance and prohibition on sacrificing
about 50% speed for about 5% storage benefit remain in force.

## What current main does and why upload streaming is insufficient

These are source observations, not newly executed failures or peak measurements.
F = dirty identities, E = surviving extents, K = changed bindings, A = active
pack/index pages, and T = pin-retained retired pages.

| Current mechanism | Scaling term or boundary | Required design change |
| --- | --- | --- |
| Workspace preparation | Dirty/saved/name/extent vectors and maps, complete prepared bytes, 24E descriptor bytes | Ordered cursors and paged saved facts; fixed framing windows |
| Future frontier admission | Estimated future encoded bytes reserved from the resident Budget | Separate resident scratch, physical backing, and declared shape |
| Local reconciliation | Prepared rows, deletion keys, 192E extent allowance, complete update map and ordered conversion | Paged patch runs or generation-native installation |
| Writer gate | Full reconciliation/index/compaction work | Only bounded capture/install ordering points |
| Live overwrite/shrink planning | Complete overlapping-extent list, update map/vector and created/replaced-page bookkeeping | Bounded range-splice/update construction before live root publication |
| One live WRITE | Current replacement/payload cap 8 MiB; FUSE delivery buffer 128 KiB | Explicit larger-operation profile or bounded application calls; no automatic cap widening |
| Active page and retirement registries | 128A and 256T source charges; payload registries grow with acquisitions | Paged exact ownership/cohort ledgers with bounded caches |
| Prepared input/RowSource | One directory is returned/decoded as a full Vec of changes | Directory header plus binding cursor; incremental parser/sink |
| Server namespace admission | Default ordering resource derives a 65,536 count allowance | Explicit shape policy independent of resident graph buffers |
| C1 reference effects | Service provides no OrderingBacking; default pending spill boundary 4,096 | Compose actual external ordering backing and cursor output |
| C1 filesystem update | Full content/addition/topology/touched/count collections and directory merges | Exact external reductions/joins and streamed final inode values |
| C1 edited file mapping | Charged unfinished draft state can reach roughly 8 MiB | Bounded mapping frontier or externally backed drafts/refcounts |
| C1 validation | Whole effective directories; alias maps and depth-dependent traversal state | Paged final graph, exact disk-backed colors/stack and bounded joins |
| C2 | Private save_id, bounded waves and short transactions already exist | Reuse this publication foundation; qualify aggregate engine memory |

One million surviving descriptor records require 24,000,000 bytes in the current
descriptor array alone (22.89 MiB). The current reconciliation formula would
reserve 192,000,000 bytes for one million extents (183.11 MiB). These are arithmetic
illustrations of source formulas, not admitted workloads, observed peaks or a
claim that one million writes leave one million extents.

The default 8 MiB Workspace-host accounting Budget is distinct from Server memory,
process RSS and file cache. C2 currently allocates a 16 MiB encoder arena per Save,
with up to 1 MiB decoder state when used, plus indexes and buffers. Default two
writers already imply 32 MiB of encoder arenas if both are live; a hypothetical 64
simultaneous Saves imply 1 GiB. These allocation calculations are not RSS evidence.

Two narrower source findings require prospective investigation: localized edit
load_node inserts into PageCache without its caller-required make_room_for step;
the prepared parser can reserve an entire binding vector before final totals
refusal. Neither finding was reproduced or fixed in this research.

## Proposed flow

```text
                            Workspace / daemon
Canonical R0 + live G1 delta in paged backing
                 |
                 | bounded capture: roots, revision, resolver, counters
                 v
       immutable captured G1             separate live G2 delta
                 |                       over exact captured G1 view
                 |                       writers keep their own root
                 v
 dirty / file extent / binding cursors
                 |
     fixed descriptor and replacement windows
                 |                saved-file facts -> paged result index
                 v
                       Bridge / Server / C1
 authenticated frames -> replayable ordered input runs
                 |
   file boundary frontier + directory merge frontier
                 |
   exact external reference / topology / inode-state reductions
                 v
                             C2 Store
 private save_id -> bounded waves / short transactions
                 |
 exact EOF + complete proofs + checked construction completion
                 v
 published candidate roots; staged filesystem result
                 |
 Workspace completion resources READY before final Branch operation
                 v
                   one conditional History Branch publication
                 |
 known canonical result -> fixed-size local selection installation
                 |
 live G2 delta over canonical R1; old pins keep original contexts
                 v
 bounded retirement / reconciliation / compaction cursors
 explicit charged maintenance or failure custody; checked refunds
```

Every producer is driven by downstream pull capacity. Releasing one byte window
permits the next; queues have byte limits as well as item limits. Keep one
construction producer per admitted construction operation, without internal
fanout. Independent Workspaces may overlap under existing Store writer capacity;
do not replace that contract with a daemon-wide producer mutex.
Counting/validation and replay can make a fixed number
of passes over the same immutable selection without retaining all records.

## Stream contracts and paged operation state

Replace APIs whose return type requires a complete collection:

| Existing interface/state | Proposed contract |
| --- | --- |
| scan_dirty / scan_extents / directory_rows -> Vec | Captured cursor, checked ordering, one path and bounded record batch |
| ActiveUpload.descriptors / ActiveStream.bytes | Encoder state machine and fixed descriptor/body window |
| saved BTreeMap / declaration collections | Paged generation/result facts with sorted joins |
| sink.directory(parent, Vec) | directory_begin(parent,count), binding(record), directory_end |
| RowSource::directory_for / next_row full payload | Directory header and independent replayable binding cursor |
| touched_serials / final inode rows / release prefetch vectors | Ordered reduction and join cursors |
| Index::prepare(slice of all updates) | Incremental sorted merge/builder with paged creation/replacement custody |
| Complete GC/compaction/cohort plans | Bounded source/owner cursors plus persisted continuation |

Parse declared width, ordering, byte counts and actual totals before each
allocation. A complete final batch still receives exact validation before
publication; per-frame validation cannot establish global alias or cycle facts.

Use quota-owned sorted runs or paged temporary indexes for saved roots, reference
effects, additions, graph colors/edges, unreachable state, touched identities,
draft/refcount/completion state and ownership. External merging has fixed fan-in;
do not accumulate one buffer/reader for every run. Account both live merge
inputs and outputs until checked removal. Replay objects carry source selection,
incarnation, generation/revision, schema, ordering and exact totals.

The existing checked paged owner ledger in backing/ownership.rs is a useful
foundation. Its 64-byte records and 62-record pages do not directly define the
new active-page/payload ledger: PageRef, birth/retirement, inverse references and
pin authority need their own grammar. RootOwner::build_ordered's 256-cell and
24-leaf-branch bounds also prevent reusing that builder unchanged at large scale.

## Generation-native views and small installation

The preferred private design separates a canonical parent, frozen G1, and live
G2. G2 owns only its later changes and tombstones. Its base view must resolve
names, inode attributes, links and content over the complete selected G1 view;
falling directly back to R0 would lose G1 creates, moves and removals.

```text
ViewBase = Canonical(root)
         | CapturedView(indexRoot, canonicalParent, immutableResolver)

ParentFileVersion = exact incarnation / generation / file selection / length
```

When a file first changes in G2, inherited spans select the frozen G1 file's
result coordinates. Preparation constructs and seals paged G1 saved-result and
exception tables before READY, keyed by exact selected file-version identity.
Known publication fills the pre-admitted outcome slot and selects a prebuilt
resolver context; it does not construct, scan or rewrite those tables. G2 edits
keep their coordinates and their own root; install replaces the live parent
context, rather than rewriting every extent.

A plain replacement of today's I.base is unsafe: R0 = abcdef; G1 inserts XX before c
and produces R1 = abXXcdef. R0 offset 2, length 2 selects cd; the same coordinates in
R1 select XX. Inferring translation afterward also has to handle deleted ranges,
holes, truncation and repeated/backward copies. Explicit parent provenance
avoids that inference for ordinary successor ranges.

Selected old views retain immutable resolver contexts and actual pins. They must
never follow a mutable live token table. A paged exception table preserves exact
open-unlinked/fresh orphan or otherwise selected versions absent from R1.
Stable serial identity remains separate from selected content version.

G1 facts are sealed. G2 can still create/unlink/mutate after READY, so its facts
remain independently admitted and paged. Install atomically selects the
then-current G2 root and changes its parent resolver/baseline identity only.
Orphan/identity resolution consults that selected G2 state as well as sealed G1
exceptions. An exhaustive prebuilt G2 exception table or detached G2 snapshot
would be stale and is not installed.

The proposed serialization scope is one pending Commit/Stage per Workspace,
subject to host-wide byte and pending-generation admission. Current main's
Submission::reserve uses a host-scoped frozen permit; independent admission is a
future scope change. The live default parent returns to a canonical parent after
success, while historical pins may retain detached roots on charged disk.

Every saved-result/exception entry resolves to a terminal canonical file root or
an exact owned private version with paged terminal ranges. It must not recursively
reference another generation's result/exception resolver. The only unresolved
live generation edge is its pending captured parent. Unknown saved results remain
owned private versions; their canonical roots cannot be guessed. Ordinary parent
tokens resolve through sealed saved roots without a whole live rewrite.

Any exception/dependency normalization is cursor-driven, with reserved scratch,
quota and backlog admission. Enforce terminal-source/depth invariants before a
dependent capture. If a capture must wait for maintenance, expose and account
for that admission delay; do not hide an aggregate rewrite at installation.

SaveFile v2 has one canonical file Base. Ordinary read/write copies become Local
bytes. A future optimized multi-origin copy needs either an explicit versioned
server-side authorized source-range grammar or pre-upload acquisition into
quota-owned backing with bounded residency. Do not perform recursive daemon
ReadFile while upload holds the native session mutex.

Canonical identity is a separate gate. Using R1 as predecessor can change
retained/replacement boundaries and C1's CDC/tree partition. Preserving grammar
and bytes alone is insufficient. Require exact old accepted root/partition
equivalence, or a separately approved identity/profile version decision.

## READY, publication and failure custody

Local READY means captured inputs and saved facts are sealed and every fixed
post-publication control/install/failure allocation is admitted. Reserve an
outcome slot for the final filesystem root if it is not yet known. Protect the
fixed physical completion credits from G2 admission. Predictable F/E/K-sized
allocation must not remain after the Branch operation is sent.

Server construction has its own completion boundary: exact EOF, topology,
references and object dependencies finish before C2 exposes its candidate save.
C2 object visibility and History Branch publication are distinct boundaries;
known saved objects are not proof that the Branch head changed.

This complete-graph guarantee applies to the filesystem candidate Save. Earlier
per-file Saves may already have published known content roots. Their saved-object
custody is distinct from a validated namespace or an advanced Branch; the diagram
does not imply one C2 transaction spanning all files and metadata.

READY covers every physical write and continuation record mandatory to install
the result and preserve promised selected reads. Either completion/retirement
backing is pre-admitted on disk, or the install ticket already owns the entire
unfinished generation/root and later cleanup uses fixed pre-admitted cursor and
ledger capacity. Detaching G1 enqueues a bounded root/cohort continuation. Failure
retains that root and position instead of accumulating an in-memory failed-owner
vector. Protected progress capacity can safely reuse existing owned records;
G2 cannot consume it. Actual I/O failure can still leave maintenance pending.

```text
LIVE -> CAPTURED -> PREPARING -> READY -> BRANCH_COMMAND_SENT
                                             |
             +-------------------------------+-----------------------+
             |                                                       |
    no authoritative result                                validated known result
             |                                                       |
    UNKNOWN_RETAINED                                    bounded local install
                                                                     |
                                                installed + cleanup complete
                                                installed + cleanup pending/error
```

The bounded installation tuple identifies submission, branch context, canonical
filesystem root, immutable parent/result/exception roots, baseline epoch,
installed revision and cleanup disposition. No full dirty scan, patch conversion
or compaction occurs under the installation gate. G2 root changes remain intact.

| Outcome | Required custody |
| --- | --- |
| Failure before Branch command | Retain known saved objects and exact preparation owner; no head assumption |
| Delivered command, unknown reply | Retain submission and dependent selections; no resend or guessed deletion |
| Known Branch result, local install failure | Save exact result; explicit same-selector completion does local work only |
| Installed result, retirement error | Keep installed revision and charged failed owner separately |
| Checked retirement complete | Refund only exact identity/allocated blocks after successful release/unlink |

READY removes proportional RAM surprises; it cannot guarantee storage or locks
never fail. Explicit completion and unknown custody remain necessary. Public SDK
resume/discard semantics need a separate API design from this memory proposal.
Each submission makes exactly one Branch command attempt. Same-selector completion
performs local work only; this is not an exactly-once distributed outcome claim.

## C1 construction and complete graph validation

For files, keep sparse subtree reuse and existing split/join semantics where
compatible. Prefer a bounded unresolved mapping frontier; externally backed
draft/refcount/completion tables are a compatibility-first alternative with
additional random I/O. Do not rebuild/read an entire large model for a small
overwrite merely to obtain a bounded heap.

Frontier emission requires proof that a finalized page cannot later change while
preserving exact partition and sparse reuse. External draft execution instead
needs paged draft, parent-reference and committed-identity tables with bounded
caches. Choose this design prospectively; neither is an error-triggered fallback.

For names, merge ordered changed bindings with paged immutable base bindings and
emit one canonical directory frontier. Initial counts and final inode values are
external reductions/joins. Compose OrderingBacking into the real Service route;
its existence elsewhere does not establish streaming here.

Global validation uses exact disk-backed effective edges, unique-parent/alias
facts, colors and traversal stack. Namespace depth is not the bounded canonical
B-tree height; a deeply nested namespace cannot keep its complete DFS stack in
RAM. This can preserve canonical formats while paying for all required proof
work. An authenticated parent/ancestry index can later reduce sparse move costs,
but it changes authenticated state and deserves a separate compatibility packet.

## Live backing and the five workload targets

Commit streaming completes only part of the workload path. Current live storage2
is an offset-keyed pooled I/N/D/E/P/R/L page index, not the older length-indexed
metadata_pieces sequence. I records hold inode facts; N bindings/tombstones; D
dirty identities; E(serial,start) final intervals; P/R/L locator and inverse
ownership facts. Up to four intervals can be inline; more use ordered E records.
Reads compose Base, Packed, Payload and Zero ranges from a selected view. Packed
and Payload become the Commit wire category Local.

For canonical abcdefghij, pwrite(XY,offset3) keeps Base[0,3), records private XY
at destination[3,5), and keeps Base[5,10). Result abcXYfghij is visible after the
selected index revision publishes; reads do not require whole-file assembly.
Append targets EOF. Write beyond EOF and truncate extension create Zero ranges;
shrink removes/cuts later ranges. Pins keep the prior selected authority.

| Target | Existing live foundation | Additional bounded-path work / limits |
| --- | --- | --- |
| Fast frequent mutations | Local revision publication, restricted tiny append/forward-overwrite hot paths; no canonical Commit per WRITE | Bound mutation planning/ownership and all-phase Commit ordering holds; IO/CPU/admission still govern throughput |
| Large files | Unchanged Base ranges plus private replacements; range reads | File cap 4 GiB; stream fragmented edits/Commit and page permanent populations before larger-profile review |
| Many tiny files | Pooled inode/name/dirty metadata; writes <=128 bytes can share a pack tail across files | Larger tiny payloads use owned backing; page identities/owners and complete namespace proof; per-file IO/reservation costs remain |
| Large edits | 128 KiB aligned acquisition windows and 1 MiB payload segments | One replacement <=8 MiB today; stream overlapping-range planning, sorted updates and retirement instead of whole affected lists |
| Many small edits | Final surviving intervals rather than a required historical WRITE replay | Disjoint edits grow E and ownership; stream their mutation/construction/Commit paths, normalize safely and reclaim unselected state |

ExtentPlan::replace scans the overlapping range in 128-entry batches but
accumulates every affected extent. MAX_AFFECTED128 is a batch, not a total limit.
publish_no_pack also materializes an ordered vector from its update map. Shrinking
a heavily fragmented file or overwriting a wide fragmented range can therefore
exhaust scratch before Commit starts. These plans, update construction, page
custody and deferred read spans need the same cursor/paged treatment. An atomic
live publication still requires a fully prepared, authorized candidate and exact
conflict/ordering rules; streaming must not expose partially constructed ranges.

Current payload publication caps a native WRITE at 8 MiB. An application's large
POSIX write can be delivered in bounded FUSE requests, but that does not promise
atomicity across all those callbacks or qualify a larger one-shot native operation.
TinyPack cutoff128 bytes is distinct from canonical C2 packing; do not claim all
small files are efficiently packed by the live layer.

Zero gaps require no private payload bytes in live backing. Current SaveFile v2
includes Local and Zero bytes in its replacement stream, however, so Commit can
transmit/process a large zero range. Implicit-zero wire transfer is a separate
version/compatibility decision, not an existing sparse-network guarantee.

The full architecture target covers bounded live read/edit plans, paged permanent
owner state, and bounded Commit/canonical completion. Commit streaming alone
does not establish all five workloads at arbitrary scale.

## Joint ownership and concurrent Exec (#249 / #219)

Workspace ID plus incarnation is the unit of mutation, capture, mount, submission
and failure ownership. Resolve/authenticate a registry entry under a short lock,
then retain a charged entry lease. Release the registry lock before shell
execution, Store calls, construction or teardown waits. Each Workspace owns its
mount, closing/lifecycle state, live/frozen generations, one retained Commit/Stage
slot, protected completion fund and active command leases. Retained submissions
prevent a second Commit on that Workspace, not a daemon-wide logical submission.

```text
One sandbox daemon                         Shared admission
  |                                        resident bytes / private disk
  +-- W-A ID + incarnation                 PID / FD / command memory envelope
  |     Exec A1 ----+                       existing Store writer permits
  |     Exec A2 ----+-> concurrent syscalls  protected control/completion capacity
  |     Commit A: G1 captured, G2 live
  |     same-W second Commit -> Busy
  |
  +-- W-B ID + incarnation
        independent Exec B1, mount and generation
        Commit B owns a submission and leased working windows
        same Branch as A -> explicit expected-head conflict if moved
```

#219 max_workspaces_per_sandbox is the single operator live-Workspace policy.
Attaching, mounted, failed-retained and closing entries consume a slot until exact
owners release. G1/G2 count once; closed sequential Workspaces consume no lifetime
count. The setting neither limits Exec calls nor supplies another Commit lock.

### Scoped gates and delivery

Current main has separate barriers: singleton selected/mount and one control
session; a long lifecycle/dispatch lock; host-wide frozen/remote admission;
shared metadata writer/windows; and Mutex<Transport> with a one-operation native
Client. Moving frozen alone leaves the other exclusions. Keep genuine short
global accounting/allocation locks and per-Workspace publication order. Replace
long cross-Workspace logical exclusion with resource-admitted operation leases.

The smallest proposed transport path is a pool of exclusive authenticated
channels, one in-flight operation per channel. It can preserve request framing
without wire multiplexing. Admit sockets, FDs, crypto/control state and windows;
do not wait for a channel while holding a publication or registry lock. Preserve
channel-local increasing IDs at dispatch: IDs allocated globally before leasing
may arrive out of order on a reused channel. Return channels after complete
input/terminal synchronization. Unknown/malformed exchanges close or quarantine
the channel, never resend a begun mutation.

Protect small status/cancel/liveness and metadata/read capacity from bulk data
windows. Channels alone do not guarantee create progress: current Service charges
ReserveInodes to the same Store writer permits as long Saves. Select/review
preadmitted serial ranges or a small catalog-control class that does not add a
C2 Save arena. Preserve Store writer capacity. Allocation identity and unknown
reservation outcomes need explicit compatibility/custody rules; do not hide true
shared capacity refusal as independent-Workspace progress.

Shared backing currently has four 128 KiB payload windows, 32 metadata roots,
11 arenas and a separate 128 KiB retained-memory Budget. These are not #219's
Workspace count. Replace lower magic owner/window caps with charged paged
authority and resource leases, or declare a supported negotiated envelope before
accepting a profile. An accepted operator count must not later discover an
undocumented arena ceiling. WorkspaceStatus.accounted_bytes is host-shared;
summing it across Workspaces double-counts. Attribution needs tagged ownership.

### Command lifetime and supervision

Admit an exact command lease before spawn, keyed to daemon instance, Workspace
incarnation and request/command identity. It owns execution-domain state, FDs,
bounded output and cancellation/terminal/cleanup custody. active_operations does
not cover a shell between syscalls. Retire finished leases directly; neither
retain nor scan every historical command, and impose no hidden numeric Exec cap.

Use one event-driven supervisor or a fixed shared reactor set for sockets, pipe
readiness, child exit and heartbeat timers. Keep 8 KiB stdout and 8 KiB stderr
capture, drain excess with truncation flags, and bound drain quanta and unsent
heartbeat state. Current control-session threads and Client::call_until's 2 MiB
upload thread even for zero-body Exec must not become a thread/stack per command.
Threadless empty control and incremental owned I/O state are required; replacing
only the process loop leaves native helper-thread growth. Borrowed Output and
mandatory Instant interfaces need an explicit new lifetime/liveness contract.

#249 selects no elapsed whole-command deadline. Exec runs until owned execution
exit or explicit cancel/disconnect/Workspace or daemon shutdown, without a huge
finite-duration substitute. Carry this through SDK, validation, transport and
daemon. Retain finite handshake, stalled-I/O, terminal-delivery and cleanup bounds,
FUSE callback deadlines, Commit deadlines and benchmark wall gates. Quiet commands
send authenticated bounded-rate liveness; heartbeats cannot exhaust a fixed
lifetime frame allowance. Keep frame/data byte bounds and nonce non-reuse;
liveness is not fabricated mutation progress.

Commands share their Workspace filesystem, not a whole-command transaction.
Accepted syscall publications can fall on either side of capture. Failed or
cancelled Exec does not rollback accepted writes or another command's changes.
Conflicting paths obey declared syscall ordering; shared-Branch Workspaces still
use expected-head CAS without automatic merge/retry.

Leader exit, descendant-domain completion, pipe EOF and terminal delivery are
different events. A child can close pipes or leave a process group and still
mutate the mount. Specify supported descendant containment/detached-child policy;
retain the command lease until the exact domain is stopped/reaped/drained or
retained cleanup is reported. Current process_group/killpg/direct-child wait is
not an all-descendant proof. Cancellation targets an exact command lease, not a
reused PID or the latest command on a Workspace.

Unmount/close marks only the selected entry closing and consults command, FUSE,
submission and pin owners before releasing its slot. Unrelated entries do not
take its long lock. Orderly shutdown custody is not crash recovery after daemon
death. Distinguish local retained failure from Store/allocator quarantine: shared
stops remain legitimate where accounting integrity is unknown.

### Per-Workspace load bearing and isolation

Global byte/disk ceilings compose tagged per-Workspace usage, working windows,
protected progress and explicit fairness. Account G1/G2, pins, replay/merge state,
idle state, channels, output and cleanup debt. Another bulk stream cannot spend
a submission's protected completion resources. Shared CPU, device and Store
contention still affects throughput; ownership is not dedicated physical hardware.

Logical/control identity and buffer leases are not strict child resource/security
containment. Exec currently selects a working directory; same-UID commands can
name another visible mount. Process groups do not enforce RSS/PID/CPU or prevent
setsid descendants. A profile promising strict sibling filesystem or child-resource
isolation requires supported runtime execution domains and filesystem/mount
confinement. Unsupported required capabilities fail explicitly. A trusted shared
sandbox declares its weaker scope. No such capability was implemented/probed here.

## Resource contract

Use per-Workspace operation ownership with a shared host byte pool. An admitted
Commit leases its own exclusive bounded working set: cursors, frame/body windows,
construction scratch and protected completion state. Concurrent admitted Commits
must not overwrite the same mutable buffer. The shared pool supplies capacity
and may recycle released storage; it does not turn the working set into one
globally locked transfer buffer.

Allocate full Commit working sets on admission, not permanently for every idle
Workspace. Idle/dirty Workspaces still own bounded base/control/reference state
and whatever progress/custody reservation their accepted state requires. Separate
that protected completion reservation from reusable transport/construction
windows. Return each lease when its actual owner releases it, including retained
failure rules; a moved or still-consumed buffer does not return its credits.

The target permits one Commit per Workspace and up to K admitted independent
Commits across the host, with K constrained by global byte/slot resources and
the separately designed transport/scheduler capabilities. Per-Workspace
ownership does not itself remove today's host frozen/remote-call serialization.
Use declared bounded admission or an upfront Capacity result when capacity is
unavailable; no unbounded queue, implicit retry or partial Branch publication.

For declared maximum tree height H, fixed merge fan-in k and admitted operation
slots q, bound the simultaneously live working set:

```text
M_daemon <= host fixed state + sum live Workspace base/control/reference state
          + bounded live caches
          + sum admitted(cursor paths + frame/data windows
                         + fixed install/failure state + GC windows)

M_server <= fixed service state + sum Store shared index/cache allowances
          + admitted engine connections / caches
          + sum admitted(input/replay windows + mapping/directory frontiers
                         + graph/sort windows + wave overlaps
                         + codec arenas + private indexes + DB statement/journal)

cursor/sort term <= c*(H+1)*page_bytes + k*window_bytes + fixed buffers
```

There must be no resident term proportional to total file length, E, F, directory
width, total encoded bytes, saved roots or retired pages. All mandatory retained
identity/owner populations are paged, with explicitly admitted handles/leases
and a bounded cache. An active request count is paired with a byte lease. The
daemon 8 MiB accounting target does not define an 8 MiB Server or process RSS target.

Application/model/database heap and mappings have their own deployment budget.
The target here bounds LayerFS working memory; a container/host peak must also
account for application owners, child output and file cache in its declared scope.

Wave admission explicitly includes the producer object before C1 allocates it,
the drained wave still being consumed, PendingBatch's next retained object,
encoded group/pack buffers, and permitted singleton objects larger than the usual
wave allowance. Moving a Vec transfers ownership without releasing its bytes.
At publication, private indexes, the old shared snapshot and the new clone can
coexist. Include actual capacity/node overhead and that overlap, not only source
accounting estimates. Shared Store indexes must themselves have bounded caches
or paged representations as object/pack populations grow.

SQLite cache_size controls page-cache allowance, not total engine heap.
Connection admission, bounded SQL/query/wave shapes, journal/temporary structures
and mmap need their own envelope. Evaluate a supported process-wide SQLite heap
guard and competition between connections; it is neither per-connection nor a
whole-Rust-process cap. Preserve definite/unknown outcomes on refusal. Retain
MEMORY journal/synchronousOFF and existing
transaction atomicity. [SQLite cache/heap controls](https://www.sqlite.org/pragma.html#pragma_cache_size),
[SQLite temporary structures](https://www.sqlite.org/tempfiles.html).

```text
D_owned = actual private changes + metadata/owner ledgers + replay/index runs
        + live sort inputs/outputs + unpublished objects
        + selected old versions + unfinished cleanup custody
```

Disk and total work can grow with changed data and required proofs. Minimum work
includes reading/transmitting actual replacement bytes and all distinct final
records. Bound RAM independently, and expose real disk/headroom/deadline refusal.

Buffered spool writes can grow OS file cache despite bounded user buffers. The
physical provider/deployment must establish a verified resident-window contract
for spool/index/Store I/O. Keep logical/canonical behavior host-independent and
use explicit per-platform capabilities; unsupported required enforcement fails
openly. Advice, lifetime cgroup peaks and source-only invalidation are not proofs
of the complete resident/cold domain. No APFS-specific architecture is proposed.

## Complexity targets and remaining time costs

These are proposed algorithmic bounds, not measured speed or implemented
guarantees. Use fixed/checked record and page widths; variable name/attribute
bytes are charged separately. S is payload bytes actually processed, including
necessary base-boundary reads. E is surviving extents examined across dirty
files, F dirty identities, N changed bindings, and M metadata/mapping page work.
L is operation-sized metadata records needing ordering/joins, G final graph
vertices/edges required by topology proof, and U ownership work required for
cleanup. G can cover the whole namespace. H is index/tree height, not namespace
nesting depth; p is page bytes, B fixed resident windows/caches/scratch, and Q
admitted simultaneous Commits.

| Operation | Proposed logical work | Resident memory | Additional backing |
| --- | --- | --- | --- |
| Capture / known-result install | O(1) selection changes; bounded slot I/O | Fixed control/progress state | Fixed root/continuation records |
| Ordered lowering / framing / replay | O(S+E+F+N), plus actual seeks/page work | O(B+H*p) | Replay/state facts O(S_staged+L) where required |
| Sparse canonical construction | O(S+M) for a bounded finalized frontier, plus any external draft/index costs | O(B+H*p) | New objects/drafts proportional to required construction |
| External ordering | Already ordered input admits linear merging; generic comparison sorting O(L log L) | Fixed merge fan-in windows within B | O(L) peak successful run storage with checked release |
| Complete topology proof | Graph walk plus external lookup/order costs; tree-indexed state can add O(G log G) record work | Fixed windows; depth stack is paged | O(G) exact external proof state |
| Retirement / compaction | Actual owner/relocation work U, plus lookup/index/I/O costs | Fixed cursor/cache/scratch | Quota-owned continuation and selected/failed owners |

Do not collapse this into an unconditional linear Commit promise. A useful
decomposition is:

```text
T_commit = ordered data/record/page work
         + T_order(L) + T_graph(G) + T_cleanup(U)

M_active_commit <= O(B + H*p)
M_daemon <= shared fixed/cache state + live Workspace state
          + sum admitted working sets + retained completion allowances

additional peak disk = actual new/staged payload and object bytes
                     + ordered/graph/draft/owner metadata
                     + unique pin-retained bytes + unfinished custody
```

With a declared H ceiling, working RAM is independent of S/E/F/N/G, subject to
the separate engine and physical-residency envelopes. Directory depth cannot be
hidden in H. Total host RAM still grows with admitted requests and Workspace
base state, bounded by admission. Data/metadata on disk and elapsed work still
grow with actual changes, required proof and retained selections.

For fixed fan-in external merging, larger L adds sequential read/write passes.
Generic ordering is not free, and tree-indexed graph/draft state can add random
I/O. Many captured inputs already have useful ordering, so use streaming joins
and sorts only for genuinely different keys. Do not spool/rebuild whole unchanged
payloads merely to bound metadata memory. A sparse file update should pay for
its replacement/boundary bytes and required mapping paths, not automatically
for total file length. A whole-file replacement must pay all of its bytes.

Lowering uses final surviving state rather than every previous WRITE, but U and
pin-retained ownership can still reflect earlier activity. Sparse namespace
changes can also require large G without an authenticated incremental parent/
reference proof mechanism. Such indexes are a distinct compatibility design;
bounded graph RAM alone does not make a move in a million-entry namespace cheap.

Streaming alone does not guarantee better latency or smaller persistent data.
Fewer whole-array allocations, amortized cursors and small install holds are
potential wins. Replay/sort/draft/ledger I/O and metadata are potential costs.
Their byte/time deltas need future declared evidence; existing storage tolerance,
command budgets and source-bound proof reuse remain unchanged.

## Maintenance and honest latency accounting

Retirement/compaction uses bounded cursors on the existing owner/producer path,
with admitted continuation state and exact pin reachability. No extra helper
worker is required. Old views and failed/unknown owners remain charged. Temporary
I/O, proof state and maintenance backlog have quota and admission limits.

Small atomic installation bounds the writer gate, not total Commit wall time.
Whether a future API exposes logical installation plus pending maintenance is
an explicit semantic decision. Existing benchmark scopes continue to include
their required cleanup; moving work to maintenance cannot earn a speed PASS.
No new latency, RSS, command-limit or arbitrary-scale PASS follows from this study.

## Alternatives and compatibility

| Alternative | Benefit | Limitation |
| --- | --- | --- |
| Cursor upload only | Removes extent/descriptor arrays | Leaves other RAM and post-publication work |
| Fully paged current reconciliation | Smaller format-compatible memory milestone | Long writer gate and older Base lineage can remain |
| Detached rebuild with catch-up | Moves construction outside live gate | Stale install risks accepted writes; continuous writers defeat unbounded catch-up/retry |
| Generation-native views plus small install | Coherent bounded-memory/write-progress target | Private format/resolver change, pin/orphan/provenance and identity proofs |

Recommend the last as the target. Cursor and external-state mechanisms are its
constituents. Reuse C2 private staging, existing paged custody and ordering runs;
avoid a second benchmark framework, codec campaign, dependency patch or new
durability service. Wire grammar may remain where streaming sink changes suffice;
private generation/ownership formats require versioned compatibility design.

## Constraints removed, re-scoped and retained

These are design targets, not changes already present in the baseline. The
redesign removes the requirement that an operation's logical population fit in
resident scratch. It does not remove admission, require unlimited disk, or turn
whole-operation work into a constant-time operation.

| Current architectural constraint | Proposed removal or replacement | Constraint that remains |
| --- | --- | --- |
| Capture, lowering, descriptors and saved facts accumulate complete mutation populations in memory | Paged exact facts and bounded cursors/windows decouple resident memory from final extent, identity and binding counts | Physical backing, declared shape, tree height and admitted working-set capacity |
| A fragmented live overwrite/shrink accumulates all overlapping extents and updates before publication | Stream bounded range-splice plans into private paged state before selecting the live result | Syscall ordering, exact source ownership, one-call limits and physical admission |
| Known canonical publication can still require proportional local reconciliation allocation and a long writer hold | Seal install/failure resources before READY; use generation-native selection and cursor retirement | Local I/O can fail; retain exact known-result custody and bounded native completion |
| One wide directory/full graph or unfinished mapping draft must fit a resident collection | Binding cursors, exact external graph proof/reductions and bounded/external drafts | Complete validation, canonical equivalence and potentially namespace-wide proof work |
| Singleton daemon mount/control state and host-wide pending submission exclude independent Workspaces | Registry entries, operation/channel leases and per-Workspace submission ownership | One pending Commit/Stage per Workspace; Store permits, global resources and expected-head conflict |
| A shell's elapsed lifetime is capped at 30 seconds and control helpers scale as threads/stacks per command | Exec runs until owned exit/cancellation with shared event supervision and bounded-rate liveness | Active command resources, bounded output, explicit descendant cleanup and finite stalled-I/O/cleanup bounds |
| Fixed metadata owner/arena/window ceilings can silently undercut the accepted Workspace profile | Paged ownership plus charged leases, or an explicit supported envelope checked before admission | Operator Workspace count and actual shared capacity; no unbounded waiting queue |

The default 8 MiB Workspace-host accounting Budget remains a resource target;
future encoded bytes and total operation facts must not be charged as if they
were simultaneously resident. Server memory has a separate envelope. Each
admitted Commit leases exclusive working windows from the shared global pool;
idle Workspaces do not permanently allocate those full windows. Resource sharing
does not imply one mutable transfer buffer or one logical Commit for the daemon.

The 4 GiB logical-file cap, 8 GiB SaveFile body, 256 MiB prepared body, default
65,536 declaration-count allowance, 8 MiB native replacement cap, active index
height, handle and view-lease caps are separate profile/structural policies.
The 65,536 allowance concerns declared update shapes, not all existing files.
Review those boundaries after the complete bounded path exists; streaming alone
neither widens them nor supplies larger-profile evidence. The 4,096 reference
reduction pending boundary instead needs actual external ordering backing;
lifting an in-memory buffer count is not the scalable replacement.

Retain one construction producer per admitted operation, Store writer capacity,
global/per-Workspace byte and disk admission, exact pins and refunds, authenticated
framing, definite/unknown outcome rules, same-Workspace Commit ordering and
shared-Branch CAS. Keep syscall/Commit/transport and benchmark bounds distinct
from the removed whole-Exec lifetime deadline. Shared Store/allocator quarantine
remains valid when shared accounting integrity is uncertain. Child memory and
strict sibling isolation require their own supported runtime capabilities.

## Workload envelope and named limits

| Workload | Required architecture property | Separate current limitation/qualification |
| --- | --- | --- |
| Multi-gigabyte model, few edits | Sparse range cursor plus canonical subtree reuse | Single file currently capped at 4 GiB; whole-file initial transfer still pays its bytes |
| Highly fragmented mutable DB/model file | Bounded mapping/draft state and parent-version provenance | SaveFile body capped at 8 GiB, logical file length capped at 4 GiB; larger-profile arithmetic audit needed |
| Millions of changed objects/names | Binding/inode cursors, external exact graph proof and saved facts | Current count admission 65,536; prepared stream 256 MiB; new shape profile needed |
| Millions of surviving final runs | Fixed descriptor windows and streamed mapping construction | Distinct from number of WRITE calls; index maximum level 7 and shape/count caps remain |
| Many simultaneous Workspaces | Global byte admission plus per-Workspace Commit serialization | #249/#219 interfaces and lifecycle ownership remain separate |

C1 canonical mapping lengths/counts use u64. The 4 GiB API cap is therefore not
proof that canonical file grammar must change, but lifting it requires a complete
checked arithmetic/resource audit. Do not widen limits and call the existing
architecture scalable. Model shards below the current limit and larger single
files are different profiles.

Database hosting additionally needs mmap/locking/fsync and application snapshot
semantics. Current FUSE fsync/fsyncdir return EOPNOTSUPP; this proposal retains the
current no-sync/no-WAL profile. SQLite's durability design relies on filesystem
locking and flush semantics, so supported DB snapshots and durable DB hosting
need distinct capability contracts. [SQLite atomic commit](https://www.sqlite.org/atomiccommit.html).
An S3-style application with millions of files exercises namespace scale; an S3
API adds object/version/multipart semantics through its own adapter.

## Proposed next design milestones and proof obligations

These are future review/evidence requirements, not commands or implementation
already launched:

1. Freeze the allocation/physical/shape ledger, READY promise and outcome state
   machine, including actual simultaneous wave/engine/cache residency.
2. Specify immutable full-generation views, file-version origins, pin contexts,
   orphan exceptions, bounded dependency depth and the small install grammar.
3. Specify stream contracts for prepared bindings, file edits, result/reference
   joins, tree construction and paged owner/retirement/compaction state.
4. Resolve exact canonical root/partition compatibility. Audit large offset/count
   boundaries separately from memory streaming and existing accepted profiles.
5. Prospectively prove scale axes independently: file bytes, final extents,
   dirty identities, one wide directory, namespace depth, selected old pages and
   aggregate requests. Record actual capacities and owned I/O, not final counters
   or lifetime peaks. Retain sparse-edit page/subtree work as a separate property.
6. Prospectively prove accepted writes at capture, lowering, body consumption,
   remote construction and installation; insertion/shrink/holes, repeated/backward
   sources, multi-origin copies, aliases, rename and open-unlinked pins.
7. Cover pre-publication refusal, unknown delivery, known-result local failure,
   installed-before-cleanup error, same-selector completion and exact final-pin
   refunds. Never resend or refund on a guessed outcome.
8. Compose the jointly designed #249/#219 registry, command/session/lifetime and
   byte contracts, qualifying physical/cache/execution-domain capabilities.
   Enable and prove concurrency after #248's memory/identity/custody gates, then
   promote explicit larger workload profiles.

No benchmark/test was run for this study. Existing finite family proofs retain
their exact source identities and scopes, especially Family 2. The 10240 selection
remains current-source UNKNOWN / OWNER-DEFERRED / SKIPPED. Existing numeric cache
verdicts remain INELIGIBLE. This research supplies an architecture target and
review obligations, not an implementation or performance qualification.

## Source map

All implementation links below resolve in the owned baseline checkout; the
baseline commit above is the authority, not an experimental branch.

- [Workspace preparation](../../../crates/layerfs-workspace/src/commit/active.rs)
  and [reconciliation](../../../crates/layerfs-workspace/src/commit/active_reconcile.rs).
- [Install/compaction lifetime](../../../crates/layerfs-workspace/src/backing/active/lifetime.rs),
  [page registry](../../../crates/layerfs-workspace/src/backing/active/pages.rs),
  [retirement](../../../crates/layerfs-workspace/src/backing/active/retirement.rs),
  [paged ownership](../../../crates/layerfs-workspace/src/backing/ownership.rs).
- [Live frontier admission](../../../crates/layerfs-workspace/src/runtime/state.rs),
  [selected namespace resolution](../../../crates/layerfs-workspace/src/filesystem/namespace_view.rs),
  [active namespace implementation](../../../crates/layerfs-workspace/src/filesystem/active_view.rs),
  [completion outcomes](../../../crates/layerfs-workspace/src/commit/completion.rs).
- [Live file publication](../../../crates/layerfs-workspace/src/filesystem/active_file.rs),
  [affected range planning](../../../crates/layerfs-workspace/src/backing/active/extents.rs),
  [live reads / payload publication](../../../crates/layerfs-workspace/src/backing/active/generation.rs),
  [hot frontier writes](../../../crates/layerfs-workspace/src/backing/active/hot_path.rs),
  [payload segments](../../../crates/layerfs-workspace/src/backing/segments.rs).
- [Prepared wire parser/sink](../../../crates/layerfs-bridge/src/contract/prepared_stream.rs),
  [wire bounds](../../../crates/layerfs-bridge/src/contract/request.rs).
- [Server prepared admission](../../../crates/layerfs-server/src/service/save/prepared.rs),
  [filesystem composition](../../../crates/layerfs-server/src/service/save/filesystem.rs),
  [publication ordering](../../../crates/layerfs-server/src/service/save/catalog.rs).
- [RowSource](../../../crates/layerfs-content/src/filesystem/rows/source.rs),
  [RowSpool](../../../crates/layerfs-content/src/filesystem/rows/spool.rs),
  [filesystem update](../../../crates/layerfs-content/src/filesystem/update.rs),
  [reference reduction](../../../crates/layerfs-content/src/filesystem/references/reduce.rs),
  [topology validation](../../../crates/layerfs-content/src/filesystem/validate.rs).
- [Edited mapping state](../../../crates/layerfs-content/src/file/edit/tree.rs),
  [mapping cache](../../../crates/layerfs-content/src/file/mapping/read.rs),
  [fresh streaming builder](../../../crates/layerfs-content/src/file/mapping/build.rs).
- [C2 private save lifecycle](../../../crates/layerfs-storage/src/cas/lifecycle.rs),
  [publication](../../../crates/layerfs-storage/src/sqlite/ownership.rs),
  [paged cleanup](../../../crates/layerfs-storage/src/sqlite/cleanup.rs),
  [codec arenas](../../../crates/layerfs-storage/src/encoding/codec.rs).
- [Daemon lifecycle](../../../crates/layerfs-daemon/src/lifecycle.rs),
  [control admission](../../../crates/layerfs-daemon/src/control.rs),
  [delivery](../../../crates/layerfs-daemon/src/run.rs),
  [execution](../../../crates/layerfs-daemon/src/execution.rs),
  [native client](../../../crates/layerfs-bridge/src/adapters/native/client.rs),
  [SDK Workspace calls](../../../crates/layerfs-api/sdk/src/workspace.rs),
  [submission admission](../../../crates/layerfs-workspace/src/overlay/snapshot.rs).
- [Current Workspace source description](../17-workspace-active-components.md),
  [earlier final-delta study](../../issues/245/FINAL_DELTA_COMMIT_COMPLEXITY.md),
  [seven-family checkpoint](../../issues/286/SEVEN-FAMILY-CHECKPOINT-20260930.md),
  [documentation policy](../../../../docs/general/documentation-policy.md).
