# workspace_api.commit — captured state to Commit history

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Owner requirements revised 2026-10-05. Product APIs were read at
> `f96d97651be5299f153ccde2bc8d921dd58807ad`; the inspected design checkpoint
> is `334fc743751b9a181e670d0601a24fb3169208f9`. Their product `core/crates`
> trees are identical (`05c00c5d62889ae316bec9ea09dba16e93ba888e`). This is
> a proposed operation contract, with no implementation or new measurement.

This document owns the public Commit workflow, stable input and result semantics,
concurrency, workload costs and required proofs. SQL/payload ownership is specified
in [daemon-sqlite](../daemon-sqlite.md); kernel acknowledgement/cache behavior is
specified in [FUSE](../fuse.md). Existing cluster-one Rust APIs and adapter work
are mapped in [06](../06-cluster-one-integration.md). The
[cluster-one handbook](../../../../../cluster_one_handbook.md) and
[CAS/CDC handbook](../../../../../cas_cdc_deltaencoding_handbook.md) govern their
implemented contracts.

Current owner supersession2026-10-07: Branch Commit is overwrite-only, as
recorded in [K34/O-25](../08-decisions-provenance.md) and the
[implementation/proof decision](../../307/BRANCH-OVERWRITE-DECISION-20261007.md).
The candidate keeps its captured parent; last database publication effect wins.
Busy and unknown custody stay unchanged. Older separate-stage diagrams below
retain the API distinction; the current daemon uses one atomic stage_and_commit.

## 1. Purpose and granularity

[owner requirement]

Commit publishes the filesystem delta represented by a stable capture, including
every affected supported name, inode, attribute and byte. `.git`, `.git/index`,
ignored files, installed dependencies, caches and build output participate in the
same state. Git ignore patterns are not a LayerFS inclusion policy. The next
Workspace can bind the completed root and immediately execute using that state;
it does not reinstall dependencies or restore filtered paths.

The smallest generally supported orchestration granularity is one tool call:

```text
previous acknowledged root Rn
             |
             v
       mount complete Rn       no base copy, scan or preparation install
             |
             v
       exec ordinary Bash      real syscalls; no automatic runtime timeout
             |
             v
       commit captured delta --+--> acknowledged root Rn+1 / existing root
             |                 |    and exact history outcome
             v                 |
       terminal unmount        +--> next tool call mounts that complete state
       owns local cleanup
```

The same operations support a long-lived Workspace serving multiple sequential
or concurrent calls with repeated incremental Commits. Workspace lifetime is
independent of any call or task. Exec does not implicitly capture or Commit;
Commit does not unmount or reset the Workspace. A caller may Commit while other
calls/processes continue, with the capture boundary below. Unmount never
implicitly publishes history; see [Exec](exec.md) and [unmount](unmount.md).

Commit captures the Workspace-wide published frontier, not exclusively changes
made by its invoking call. On known install, the captured state becomes the new
base and later active mutations remain over it. Further calls and the next
incremental Commit use that same live Workspace; no remount or dependency restore
is required. One active Commit lifecycle per Workspace remains the admission rule.

Per-tool-call is the expected common orchestration mode; per-task is also
required. Execs can be short or long-lived in either. Commit scheduling and
capture/resource lifetimes must follow actual activity, not assumed command
duration or a command classifier.

The current history API returns `UpToDate` if candidate root and intended base
already describe the Branch. It does not create an empty immutable Commit for
every no-change invocation. Thus every tool call can have a Commit attempt and
exact outcome; a mandatory invocation/audit event even without a filesystem
change would be a separate API requirement, not a property of current history.

## 2. Is this load-bearing?

[proposed design; required prerequisites, not achieved performance]

The direction is load-bearing only after the required mechanisms and proofs are
complete. Immutable base binding avoids work proportional to the base at mount;
indexed change enumeration, localized construction, progressive object output
and backpressure avoid retaining an entire Commit in RAM. They do not make a
large change free or remove downstream resident structures by themselves.

| Requirement | Mechanism required | Current qualification |
| --- | --- | --- |
| Full development state on the next tool call | Complete committed root, stable inode/attributes, no filtering | Import of the full supported base and mounted survival remain required |
| Many tiny files and names | Generation-selective keyset pages; streamed directory changes and backed membership | Directory `Vec` and new-parent map still exist in cluster one |
| Arbitrarily many edits over time | Latest-state payload representation and bounded-backed normalized construction | `EDIT_DEFERRED_LIMIT` still refuses sufficiently fragmented construction |
| Large files | Request/read/object windows and incremental stream/edit APIs | Inherited Workspace/transport caps must be removed |
| Sparse files | Explicit canonical hole/run semantics | Current Commit inputs supply zeros and pay logical length |
| Continuous activity through Commit | Stable capture, separate active state, short install/failure transition | Bounded failure composition and orphan lifetime algorithm unfinished |
| Simultaneous Workspace Commits | Separate captures/Saves, fair shared service, bounded queued bytes | Sound Save lifetime/interleaving and scheduling proof required |
| Long-lived Workspace with repeated incremental Commits | Same mount/view/identity; advance base, retain later active changes and reclaim obsolete ownership during activity | Same-mount cache, repeated failure/orphan composition and sustained reclaim proofs required |

No artificial Workspace bytes, file count, changed-name count, edit count, total
upload length or elapsed-time ceiling may be derived from a fixed resident
container or wire counter. Persistent state and history can grow with the actual
workload; processing windows remain bounded. Real device capacity, platform
address/format limits and explicit resource admission still exist and are
reported accurately. A resource setting must not disguise a reintroduced
Phase 4.5 metadata or whole-stream cap.

### 2.1 Efficiency model and limits of the claim

Capture, incremental construction and install are efficient **directions**, not
measured results. Required capture work is bounded binding/generation/domain
bookkeeping rather than an O(base entries), O(changed rows) or O(payload bytes)
copy. SQL admission/queue and page I/O still cost time. The selected ownership
algorithm must actually preserve this bound; generation labels alone do not prove it.

| Part | Work avoided / intended scope | Real cost or missing proof |
| --- | --- | --- |
| Capture | Reuse existing changed rows and retain their stable domain; no full snapshot copy | Short SQL/state transition, queue wait, capture ownership algorithm |
| Enumeration | Visit captured membership, not the whole base or expanding active namespace | Indexed seek plus visited rows and replay passes; R2 |
| Construction | Reuse unchanged structure, stream new/replacement data | Replacement compare and construct passes, small-file assembly, affected tree work; R3 |
| Save | Bounded batches, exact reuse, no complete-upload spool | Authentication/reuse reads, hashing/encoding, transport and publication |
| Install | Preserve active changes over equivalent new base | Short transition and exact lifetime/coherence proof; no per-orphan loop |
| Cleanup | Automatic batched local SQL deletion, reusable database space | Real shared-writer work/debt; not removed by returning early |

A small edit in a large complete Workspace should not pay for copying unchanged
files or reinstalling dependencies. A new/full replacement large file still pays
O(actual content); many tiny files pay metadata/index/tree work. Sparse holes need
the required hole-aware format/input change. CAS avoids some writes, not every
lookup/read/comparison, and physical PREFIX selection is not guaranteed.

Judge the smallest granularity by the complete mount -> Exec/output drain ->
Commit -> terminal-unmount path, plus accounted automatic cleanup debt. Fixed
lifecycle/history/transport costs can dominate a tiny change. Concurrent work
spans overlap: report the critical path and phase observations without adding
nested/overlapping counters as a fabricated total. Shared SQLite job serialization
can limit aggregate service; fair scheduling is not a throughput measurement.

Acceptance needs no-change, one small edit in the full root, tiny-file/wide-dir,
large/sparse, continuous-log and concurrent-Workspace cases. Fragmentation,
deferred nodes/namespace memory, failure composition/orphan retention and reclaim
capacity remain required proofs. Do not call the model "very efficient" solely
because capture changes a generation field or cleanup is asynchronous.

## 3. Request, completion and ownership

[proposed public contract; exact revised wire types remain integration work]

The request selects a Workspace ID plus incarnation, not merely a mount path.
The daemon validates routing and authority and admits one Commit per Workspace.
A second request receives a typed in-flight/uncertain result; it does not silently
capture another generation or automatically repeat the first attempt.

The capture retains:

- Workspace incarnation and generation/domain identity;
- Branch, expected head/base/effective root and actual construction base;
- allocation scope, filesystem profile and intended Commit base;
- immutable payload/snapshot ownership and captured namespace membership;
- operation records, Save custody, candidate root and exact stage token when known.

These identities survive until exact outcome/install/cleanup rules release them.
A response reports captured generation, `Committed` or `UpToDate`, root/head and
local install disposition. A failure reports phase and knowledge: definite before
publication, uncertain, or known publication with local install failure. Never
flatten the latter into “Commit did not happen.” Observation is exposed through
[status](status.md), which is not required to advance the operation.

The retired SDK had a fixed600,000ms Commit call and legacy phase names in
`layerfs-api/sdk/src/workspace.rs` and
`layerfs-bridge/src/contract/workspace_commit.rs` at the historical source pin
above. These are historical limitations, not the revised contract. Long operations
need bounded progress/result transport independent of a fixed total runtime or
total response count. Explicit cancellation and bounded individual RPC deadlines
remain separate; a deadline cannot classify an already-entered history operation
as unpublished.

## 4. End-to-end workflow

[proposed design over current cluster-one APIs]

`layerfs-server` is removed from the target architecture. Existing cluster-one
libraries are embedded in the host application's cluster-one runtime. Its
runtime-owned object/history/Save adapters cross the sandbox boundary where
necessary. Operation names below describe adapter calls, not existing HTTP URLs
or a new standalone server. Current persistence remains host-local macOS SQLite.

```text
 caller / SDK       daemon: Workspace + content      embedded cluster-one runtime
      |                        |                         Storage + History
      | commit(W, incarnation) |                              |
      +----------------------->|                              |
      |                        | validate / admit Commit      |
      |                        |       |                      |
      |                        | bounded SQL CAPTURE          |
      |                        | C = fixed changed domain     |
      |                        | A = later mutations          |
      |                        |       |                      |
      |                        +---------- begin Save ------->| Storage::begin_save
      |                        |<-------- capability ---------|
      |                        |       |                      |
      |                        |  one construction producer   |
      |                        |  +-----------------------+   |
      |                        |  | read captured windows |   |
      |                        |  | normalize final edits |   |
      |                        |  | construct file roots  |   |
      |                        |  | update namespace      |   |
      |                        |  +-----------+-----------+   |
      |                        |              |               |
      |                        +--- bounded finalized batch ->| validate role/refs,
      |                        |<--- accept / backpressure ----| identity + authority;
      |                        |              |               | Save::accept
      |                        |  repeat until candidate root |
      |                        |              |               |
      |                        +---------- finish Save ------>| Save::finish
      |                        |<--------- saved outcome ------|
      |                        |              |               |
      |                        +---------- StageRequest ------>| stage_changes
      |                        |<--------- exact token --------|
      |                        |              |               |
      |                        +------- CommitStagedRequest -->| compare expectations,
      |                        |<--- Committed / UpToDate -----| publish Branch
      |                        |              |               |
      |                        | bounded SQL INSTALL          |
      |                        | bind reported root/head      |
      |                        | preserve A and inode identity|
      |<----- exact result ----+                              |
      |                        | bounded background retirement|
      |                        | / consolidation              |
```

No transaction or Workspace mutex spans hashing/chunking, output acceptance,
network wait, Save completion or Bash. Construction requests bounded captured
data/operation records jobs from the daemon SQL engine and yields between them. Waiters are
parked without consuming all FUSE dispatch workers.

One shared overlay database gives one SQLite writer, not parallel writer
transactions. Connection/read topology is an engine decision in
[daemon-sqlite](../daemon-sqlite.md). Concurrent callers, constructors, cache hits
and transport can proceed while short SQL jobs take turns. Fairness and bounded
work are required; high aggregate throughput is not yet measured.

## 5. Capture and live activity timeline

[proposed invariant]

Capture is ordered with ordinary accepted mutations. The generation used by a
mutation is resolved inside admission/transaction execution, never cached in an
open handle. The captured cursor has generation-selective membership and a fixed
termination domain; later creates must not extend its EOF.

```text
time    Bash / FUSE                    live view                  Commit input
----    -----------                    ---------                  ------------
t0      writes X acknowledged          X over R                   --
                 |
t1      CAPTURE ordered after X        A(empty) over C(X) over R   C(X) over R
                 |
t2      writes Y acknowledged into A   A(Y) over C(X) over R       C(X) over R
                 |                          |                         |
t3      writes Z / rename / unlink     A(Y,Z) over C(X) over R     same fixed C
                 |                          |                         |
t4      commands continue              A over C over R             construct R'
                 |                          |                      R' = C over R
t5      known history success          A over C over R             published R'
                 |                          |
t6      INSTALL preserves view         A over R'                  retire old C
                 |
t7      next Commit can capture Y,Z    current state               new capture
```

Each locally published mutation belongs wholly before or after capture. A syscall
split across several requests may straddle it. A successful kernel write return
implies its mutation was already locally published; the converse is not assured.
Pinned fuser's reply methods return no delivery/kernel-application receipt.
Capture therefore records the actual published view, including a locally
successful mutation whose reply delivery was lost or uncertain. It must not
exclude such state or guess a rollback from the caller's observed error.

The admission/order fence resolves earlier admitted mutation attempts and prior
reply-send attempts according to their exact state before sealing the captured
domain. Deferred jobs need explicit queued/attempted/published/reply-attempted
ownership; reads already planned retain their original roots. No SQL/Workspace
lock or dispatch worker is held while waiting. The actual drain/queue wait remains
part of capture latency, not a constant-time promise. Unresolved SQL publication
prevents pretending the frontier is known.

Dirty mmap stores not locally published are outside this boundary; Exec exit is
not a substitute for a proved kernel flush/lifetime rule. No command-specific
checkpoint is introduced. The source-qualified
[request-path investigation](../fuse-investigation/01-kernel-and-request-path.md)
explains the local publication versus reply-delivery distinction.

Captured payload must stay stable through every replayed pass. Shrink cleanup,
last unlink, orphan writes, install and reclamation use explicit ownership/leases.
Generation labels alone do not freeze shared mutable BLOBs.

### 5.1 Existing captured rows, new active rows and operation records

Capture does not copy the whole overlay into a temporary table. Existing namespace
and payload ownership at generation G become stable Commit input. The operation
must retain immutable byte/mask/cutoff state; later changes go to G+1 and cannot
update captured visible BLOBs. Only affected keys/payload units need new active
state; there is no copy of every file at capture.

Commit additionally creates operation-keyed operation records rows for replayable edit/run
indexes, ordering and constructed roots. Those rows are construction bookkeeping,
not another full payload snapshot and not a separate SQLite database/file.

```text
BEFORE CAPTURE             DURING COMMIT                   AFTER KNOWN INSTALL
--------------             -------------                   -------------------
existing G rows ----------> captured G, retained stable ---> obsolete if unreferenced
base R -------------------> same immutable R --------------> history retains R
                            active G+1 created on mutation -> remains live over R'
                            operation records rows --------> reclaim after last use
                            construct/save R' --------------> cluster one retains R'

capture: retain existing state; no bulk copy or delete
install: live A over C over R becomes equivalent A over R'
cleanup: delete only unreachable LOCAL rows; no shared-object/history deletion
```

No captured input is destroyed before its readers/construction and exact outcome
custody permit release. An ordinary write buffer is also not a delayed accepted
write: the overlay transaction owns acknowledged bytes before replying.

## 6. Construction routes and buffers

[source-verified API constraints; proposed bounded producers]

| Captured case | Route | Work that remains |
| --- | --- | --- |
| New / fully replaced regular file | `construct_stream` over stable captured bytes | O(content length); bounded prefix, CDC and output windows |
| Localized change to committed file | `apply_edits` with `EditSequence` / `EditSource` | Replacement compare and construction passes; changed boundaries/paths and necessary base reads |
| Append to committed log | Normalized tail edit | No ordinary whole-log reconstruction; representation transitions can require broader work |
| File create then delete before capture | No final binding/value emitted if no surviving ownership needs it | Local garbage still reclaimed/accounted |
| Rename/link/unlink | Final namespace rows and reference-count derivation | Changed paths/values; no chronological operation replay |
| Sparse ranges | Required hole-aware construction | Current source instead streams zeros; prerequisite, not qualified fast path |

Small whole-file representations and representation transitions can require
assembling or reading the whole bounded small value. Localized edits are not
promised to produce the identical canonical root as a fresh CDC scan. Do not
retry a failed edit constructor with an error-driven whole-file alternative.

```text
 captured SQLite / immutable base
             |
       bounded read window               no whole-file mutable spool
             |
       EditSource / stream
             |
       content constructor               one producer for this Commit
       prefix + chunk/mapping windows
             |
       FinalizedConsumer
             |
       bounded adapter batch             byte + object bounds before allocation
             |
       runtime Save::accept
       reuse / encode / reference-closed publication
             |
       acknowledgement -------- backpressure --------> request next input
```

The finalized-object sink must not accumulate an entire file, namespace or
Commit. Queue accounting includes producer-held output, the blocked batch,
decoder/encoder workspaces, journal/operation records and every live Save's caches.
OperationRecord tables are Workspace/operation keyed in the daemon database and accessed
through bounded jobs. Namespace input needs streamed directory changes and
backed new-parent membership; merely replacing the caller with a paged cursor
does not remove cluster one's resident `Vec` and map.

“Delta” here means the logical affected state and reuse of unchanged structure.
CDC splits content, CAS recognizes exact canonical objects, and storage may
select physical PREFIX encoding against an admitted predecessor. PREFIX is not
a FUSE write log or a guarantee that every changed payload stores as a delta.

The current 16 MiB canonical object, 512-object ordinary Save batch, and
4 MiB-minus-one ordinary publication-byte limit are object/work windows, not
total Commit limits. Readers use bounded calls, currently at most 4,096 IDs and
32 MiB per demand. A full operation continues through multiple windows. The
legacy 256 MiB prepared-stream cap and 4 GiB Workspace file cap are removed from
the replacement route. Individual fixed wire counts cannot cap saved-file totals;
progress counters must represent or explicitly saturate large counts without
refusing a workload.

## 7. Concurrent Workspaces and mutable history

[proposed design]

Each Workspace owns its capture, active state, operation records, Commit slot and Save
capability. They share the daemon database, devices/caches and cluster-one runtime.
One producer per Commit may run concurrently with another Workspace's producer;
no helper lane increases a single Commit's construction concurrency.

```text
Workspace A              Workspace B              fair shared service
-----------              -----------              -------------------
capture CA               capture CB               overlay jobs take turns
construct A              construct B              computation outside SQL
Save capability SA       Save capability SB        separate logical custody
      |                        |                         |
      +-- accept A1 -----------+------------------------>| serve SA batch
      |                        +-- base read ----------->| serve demand read
      |                        +-- accept B1 ---------->| serve SB batch
      +-- operation records window ------+------------------------>| bounded overlay job
      +-- accept A2 -----------+------------------------>| serve SA batch
      |                        +-- finish SB ---------->| finish gets service
      +-- finish SA -----------+------------------------>| finish gets service
      |                        +-- stage / transition -->| history transaction
      +-- stage / transition --+------------------------>| history transaction
install A                install B                independent local bindings
```

This is an illustrative interleaving, not a frozen priority order. Scheduler
shares/bypass rules must guarantee progress for reads, accepts, finish, history,
overlay mutations and cleanup under the admitted load. Strict read priority can
starve Commits. Whole-Save connection checkout can exhaust demand-read capacity;
Save capabilities therefore outlive individual transport calls/checkouts.

The current `Save<'_>` borrows its `Storage` and mutable indexes. Multiple
logical sessions need sound ownership/lifetime arrangements over separate
Storage handles and the shared provider; the comment that separate handles may
write concurrently does not itself prove this runtime session registry. No unsafe
lifetime extension or serialization of entire Commits substitutes for the proof.

Immutable objects make content identity, reuse and authenticated acquisition
natural candidates for distributed storage. They do not remove mutable Branch
atomic head publication, exact stage ownership, inode serial allocation or
coordination. If A and B capture the same head, both may publish: the last
database effect determines the Branch head, while each retains its captured
parent and root. No merge or rebase is performed. Different Branches progress
independently within shared capacity.
There is no distributed provider or endpoint implied by this diagram.

## 8. Completion and unknown outcomes

[source-verified cluster-one distinctions; proposed caller custody]

```text
construct root
     | objects accepted by consumer       NOT storage completion
     v
Save.finish succeeds                     storage completion
     |
     v
stage_changes returns exact token        saved candidate staged
     |                                   NOT Branch publication
     v
commit_staged returns Committed/UpToDate  history completion
     |
     v
local INSTALL succeeds                   Workspace binding installed
```

`StageRequest` carries captured expectations, construction base, candidate root,
scope/profile/generation and intended base. Stage insertion does not establish
savedness and does not check that the Branch still has the expected head.
`Save::finish` must succeed first. The separate-stage transition uses the returned
exact token. `FilesystemResult.root` is `FilesystemRootId`; use `.0` for the
history ObjectId field, not `.object()`.

```text
                         Captured / Constructing / Saving
                                      |
                +---------------------+-----------------------+
                | definite failure    | Save reply unknown    | Save finished
                v                     v                       v
     short LOCAL RESOLUTION    preserve possible waves      STAGING
     preserve accepted view    history never requested         |
                |                     |             +---------+----------+
                +---------------------+             | known token        | unknown
                |                                   v                    v
                v                              TRANSITIONING        UNCERTAIN
      bounded consolidation                  +------+-------+       retain capture,
      (algorithm still required)             |              |       candidate/context
                                           known         reply unknown     |
                                             |              |             |
                                +------------+-----+        +-------------+
                                |                  |                      |
                         Committed/UpToDate      refusal                   |
                                |                  |                      |
                             INSTALL       exact owned-stage discard      |
                                |            / disposition               |
                       success / known       known -> local resolution    |
                       published local       unknown ---------------------+
                       install failure                                  |
                                                                         v
                                               no next Commit / normal unmount
                                               until exact disposition established
```

A known history success is not undone if local install fails. Report the
published root/head and fail-stop unsafe local state. Definite pre-stage failure
can preserve the current view locally without claiming storage completion;
previous reference-closed Save waves may remain immutable in the Store.

An uncertain stage/transition/required discard retains exact phase, candidate,
expectations and token when known. It cannot be folded into Idle by assuming
absence. Reads of stage/Commit exist, but an original queued operation can still
execute after an unfenced “absent” observation. An exact resolver requires
permitted policy, operation completion fencing and coherent authorized reads;
none is invented here. No automatic resend, rebase, reopen or guessed deletion.

Cancellation fences queued/in-flight work before classifying its outcome. Before
staging it can resolve locally after that fence; staged work needs exact
disposition; a transition cannot be guessed cancelled. A short control RPC
deadline is not a whole-Commit stop rule. See [unmount](unmount.md) for explicit
forced teardown and acknowledged-unknown custody.

Provider quarantine is shared by storage/history handles over the same session.
It can affect every Workspace's uncached base/history access. Immutable content
does not prevent device loss, corrupt encodings, unavailable delta bases or
authority errors. Crash guarantees are exactly those of selected profiles.

## 9. Lifetime and retained history

[proposed design; replacement algorithms required]

Failed Commit resolves the composed live view by a short metadata transition,
then consolidates independently. A payload-sized merge while marking a hot inode
busy is withdrawn. The replacement must bound read depth/versions and prove
progress under repeated failures plus continuing writes; a chain of failed
generations is not a solution.

Open-unlinked content belongs to an orphan owner independent of namespace capture.
Successive Commits must not pin one new orphan generation per success. Namespace
Commit removes the last binding without constructing orphan-only payload, while
pre-unlink captures retain the exact snapshot they require. These ownership rules
are unfinished integration work, not a claimed two-version bound.

### 9.1 Automatic SQL row deletion and retention gates

"Logical retirement" makes local state invisible/unreachable to new operations.
"SQL row deletion" actually removes its inode/name/payload/operation records/ownership rows
in bounded database transactions. Both refer to rows in overlay.sqlite, not
separate per-file payload files. These are distinct from shrinking the database.

| State | Release / cleanup rule |
| --- | --- |
| Captured G during Commit | Retain exact data/domain; no early deletion |
| Active G+1 after success | Remains live over installed R'; never deleted as completed Commit operation records |
| Captured G after known success and successful install | Retire from the live view; enqueue unreachable rows once retained readers/operations release ownership |
| Operation records | Enqueue after its last consumer; retain any records required for exact unfinished/uncertain outcome custody |
| Definite pre-stage failure/refusal | Preserve uncommitted composed state; only unreachable operation records/duplicates are reclaimable after proper local resolution |
| Unknown stage/transition/required discard or failed local install | Keep exact custody and required local state; no guessed success or blanket deletion |
| Successful terminal unmount | Fence all owners, close namespace and own removal of all remaining local rows; no separate close/cleanup call |

```text
known history success -> successful install -> captured state logically retired
                                                    |
                                        retained reader/owner exists?
                                           | yes             | no
                                           v                 v
                                      keep exact rows    enqueue reclaim target
                                           |                 |
                                    last release event ------+
                                                             v
                                             daemon runs bounded SQL deletes
                                             advance cursor/debt -> yield
                                                             |
                                             automatic progress until complete
```

There is no intentional TTL for unreachable rows. Enqueue/last-owner-release
wakes automatic daemon maintenance, which must continue even with no further
Exec, Commit, mount or status call. No per-call cleanup API or next mutation is
needed to trigger it. Fair service and reclaim capacity must be proved under
repeated tool calls and continuous writes. If capacity/debt/headroom constrain
admission, expose that pressure; do not claim asynchronous cleanup is free or
promise an unmeasured completion deadline.

Deleting rows releases SQLite cell/page space for reuse; not every removed row
frees an entire page. Database high-water allocation can remain for subsequent
Workspaces. Do not DROP shared tables, unlink shared SQLite at Workspace unmount
or conflate freelist reuse with file shrinking.

Many per-tool-call Commits intentionally retain historical roots and changed
canonical objects. Exact reuse can reduce storage growth but does not bound it.
Current cluster one has no deletion path; abandoned Save waves can also remain.
Local reclaim/unmount never deletes those shared objects or history by guess.
Future global retention/GC needs root/orphan/in-flight and physical delta dependency
leases before deleting content. Local row cleanup is not that collector.

## 10. Workloads and cost expectations

[required coverage; historical fixture identity, no new scan]

The full development fixture is **130,045 entries / 3,475,776,149 regular-file
bytes**: 103,108 files, 16,867 directories, 10,070 symlinks. Its copied source HEAD
is `639ed015397290b3745d163aafe02ffee4aa3f84`; prepared manifest SHA-256 is
`98fd26440b9087bcfd5c23bec9e5497434a2dcb9a27fc85ad0b823ba9c516658`.
Dependency replay is **95,021 entries / 2,126,509,110 bytes**. These are retained
preparation observations on `codex/phase7-experiment-305` at
`1451b68a720bbe2175a103dd9b35693ad05e2be1`,
`core/docs/issues/305/PREPARATION-REPORT.md`, not current measurements.

| Workload | Captured effects and cost | Required evidence |
| --- | --- | --- |
| Per-call `true` / read-only tool | Complete base binding, possible ordinary metadata effects; unchanged candidate may be UpToDate | Lifecycle/Commit overhead separately; no base scan hidden in setup |
| `git status`, traversal and `.git/index` refresh | Supported metadata and any index updates participate in capture | Full next-mount tree/index survival and stable identity; no ignored-path exclusions |
| Compiler/build | Source edits, new tiny files and large outputs; actual changed content/structure work | Native-compatible execution, full outputs/caches available next call |
| Copy and hard-link install replay | All 95,021 replay entries, payload copies or inode alias changes | Bounded processing memory, reference correctness, full affected-state Commit |
| Many names in one directory | Changed-name enumeration plus parent/tree reconstruction | Streamed per-directory input, no resident name-count refusal |
| Fragmented repeated edits | Final stable normalized edit inputs, replacement passes and boundary construction | No inherited deferred-node/edit cap, no chronological-write replay |
| Large file / full replacement | O(actual content), progressive construction/Save | No 4 GiB Workspace or 256 MiB whole-stream ceiling; bounded queued bytes |
| Large sparse file | Hole/run and written-byte work | Current zero-stream prerequisite corrected; persisted hole semantics |
| Continuous file logger | Captured tail edit while later writes continue, rotation/truncate/alias lifetime | Same-inode progress, orphan bounds, failure resolution and debt accounting |
| Concurrent Workspaces | Separate captures/Saves; shared queue/device service | Finite per-Workspace progress, ordered same-Branch overwrites, captured provenance and authority isolation |
| Repeated per-call history | New state when changed, no-op outcome otherwise, retained ancestry | Historical roots stay readable; real storage/maintenance cost reported |

Full-state proofs include ignored data, `.git/index`, symlinks, dependencies,
caches/build output and hard-link aliases. Historical experiment verification
exclusions do not authorize exclusions from this product contract. Native Init's
current symlink refusal and retained scan collections need a faithful bounded
provisioning correction; Commit/mount must not invoke a partial importer to make
each call usable.

## 11. Required proofs and future measurement

[proposed validation; none run for this document]

1. Block acceptance after capture; overwrite, shrink, rename, create and unlink.
   Published state equals capture; live state retains later mutations; next
   Commit publishes them.
2. Capture one key while the active state receives the full dependency replay.
   Count visited/returned rows on every pass; fixed EOF never follows active keys.
3. Commit fragmented/wide/new-parent/sparse cases through the real cluster-one
   APIs with fixed processing windows, correcting their source constraints first.
4. Keep one unlinked log descriptor through repeated successes and failures;
   bound reachable versions/read depth and prove byte/attribute correctness.
5. Fail late with large captured/active streams; no payload-sized busy interval.
6. Two Saves under continuous reads plus overlay mutations; finish/history and
   independent demand reads receive finite service without entire-Save checkout.
7. Drive unknown Save/stage/transition/discard, known publication/local install
   failure, cancellation and forced unmount; audit exact custody and no guessed
   retry/deletion.
8. Run the complete per-tool-call sequence with the full supported tree, including
   ignored/index/cache/output changes, and inspect next-mount state/history.

Future measurements follow [07](../07-implementation-validation.md) and root
[AGENTS](../../../../../AGENTS.md): read `benchmark_agent_report.md` before every
invocation, acquire/validate fixtures once and use independent `--setup clone`
where applicable, pin release/locked identities, use one construction worker and
one sample per case/arm. Cache contracts name/enforce base, attribute, pager,
guest FUSE/overlay and host caches equally; clone is not a cold claim. No priming
or moving changed-content work into setup.

Report complete command, capture, construction, Save, history, install, logical
unmount and deferred cleanup separately without adding overlapping spans. Preserve
failures/INELIGIBLE/NOT_RUN and old receipts. Required command budgets remain
15 s, or a prospectively declared exception up to 25 s; separate verification
is under its bound. If a full selection cannot fit, cite qualifying evidence or
retain NOT_RUN with its reason—do not shrink/filter the workload, increase
workers/timeouts or use a warm repeat. Those harness budgets are not Bash runtime
or product Commit-size limits. Use labelled count diagnostics to investigate
mechanisms rather than repeat gate samples. No benchmark was run here.
