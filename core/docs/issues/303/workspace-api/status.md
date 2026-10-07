# workspace_api.status — bounded read-only observation

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Owner requirements revised 2026-10-05. Product APIs were read at
> `f96d97651be5299f153ccde2bc8d921dd58807ad`; design checkpoint
> `334fc743751b9a181e670d0601a24fb3169208f9` has the same product tree
> (`05c00c5d62889ae316bec9ea09dba16e93ba888e`). No implementation, workload,
> build or measurement accompanies this proposed contract.

Status owns observation semantics. It neither changes state nor drives
[Exec](exec.md), [Commit](commit.md), [mount](mount.md) or [unmount](unmount.md).
State/index/resource mechanisms belong to [daemon-sqlite](../daemon-sqlite.md),
kernel bookkeeping to [FUSE](../fuse.md), and cluster-one completion semantics to
[06](../06-cluster-one-integration.md) and the
[handbook](../../../../../cluster_one_handbook.md).

Owner supersession 2026-10-08 at R0 input `1a6bb53ef`: the SDK exposes
ProjectApi, WorkspaceApi and SandboxApi. Ordinary Sandbox/runtime or an external
executor owns command launch, standard streams, exit status and explicit
cancellation; the filesystem daemon owns no command supervisor, launcher,
per-Exec cgroup, command registration or custom Exec wire. FUSE serves every
permitted visible process. Shell exit/zero registered commands proves no
filesystem drain, and forced filesystem teardown never implicitly kills caller
processes. Current [S8 specification](../../307/S8-SPECIFICATION-20261008.md) and
[R0–R9 rollout](../../307/ROLLOUT-LEDGER-20261008.md) govern prospective work;
historical baseline pins, receipts and verdicts retain their original scope.

## 1. Purpose and no hidden work

[proposed public contract]

`workspace_api.status` answers one authorized observation of one Workspace
incarnation. It is optional: filesystem acknowledgements, process output/exit,
Commit progress and terminal cleanup do not depend on polling it.

Status reads maintained state and bounded counters. It never walks the base or
overlay namespace, counts every file on demand, reads payload, authenticates
all stored objects, refreshes the Branch, flushes processes, captures, checkpoints,
consolidates or reclaims. A Workspace with millions of entries must not turn one
status call into namespace-sized work. Expensive inventories/diagnostics, if
introduced, are separate explicitly scoped operations.

Per-tool-call orchestration can be:

```text
mount complete state -> exec ordinary Bash -> commit delta -> terminal unmount
                              |                  |
                     optional status     optional status
                              |                  |
                         observe only       observe only
```

No status call is inserted as a special Bash hook, post-write checkpoint or
pre-Commit requirement. Bash has no automatic runtime timeout. A short deadline
on an observation/control request bounds that request only; it cannot kill an
Exec or classify a Commit outcome.

## 2. Request and snapshot semantics

[proposed design]

Input selects Workspace ID and incarnation. The daemon authenticates the caller
and validates the registry route. It retains the Workspace reference, releases
the registry lock and copies the bounded observation. Stale identities never
resolve to a reused namespace. Missing/denied/fenced routes return typed outcomes.

The result carries the selected identity and an observation sequence/revision.
Lifecycle, binding and Commit custody are copied coherently under their short
state synchronization. Resource counters copied from other owners carry their
own scope/epoch; the result is not an atomic transaction over the entire daemon,
kernel and host. Do not sum asynchronously observed overlapping counters and
call them exact phase memory.

```text
caller               daemon registry             Workspace observation owner
  |                         |                              |
  | status(W, incarnation)  |                              |
  +------------------------>| authenticate / select        |
  |                         | retain reference              |
  |                         | release registry lock         |
  |                         +------------------------------>|
  |                         |                  copy lifecycle/binding/phase
  |                         |                  copy maintained counters
  |                         |                  no SQL scan / payload read
  |                         |<------------------------------|
  |<---- bounded snapshot --+                              |
  | identity, sequence, knowledge and scoped resources      |
```

An engine may use one indexed state-row lookup if that is the selected source of
truth. It is a bounded point query through fair SQL admission, not `COUNT(*)` over
Workspace tables. Prefer already maintained post-transaction observation state
where this avoids competing with mutation service. It becomes visible only
after its corresponding transaction commits; never publish speculative success.

Status must remain available while Commit construction/transport waits. It must
not acquire the dormant daemon-wide slot held across Exec/Commit. Fair observation
admission also prevents aggressive polling from starving mutations or history.

## 3. Proposed result fields and their scope

[proposed fields; final typed wire representation remains integration work]

| Group | Fields / meaning | Source of the observation |
| --- | --- | --- |
| Identity | Workspace ID, incarnation, daemon incarnation, observation sequence | Validated registry and local state |
| Lifecycle | Attaching/ready/unmounting/terminal/failed, mounted/routing state, retained cleanup disposition | Lifecycle owner; one coherent local copy |
| Base | Authorized Branch, installed head/root, scope/profile | Installed immutable binding; not a fresh Branch query |
| Activity | Active filesystem/control operation/handle counts and deferred-waiter counts | Maintained acquisition/release bookkeeping |
| Mutation | Acknowledged revision, active generation, maintained dirty-inode/name counters | Post-commit mutation/capture bookkeeping |
| Commit | Idle/running/uncertain/local-failure; phase, captured generation and known root/head/token | Commit custody owner |
| Queues | Admitted/queued bytes/jobs by bounded class, queue/service observations | Scheduler counters with explicit scope/epoch |
| Local storage | Logical charged data/operation records/retention/reserve/debt; global allocation observations separately | Admission/reclamation counters; not inferred freed pages |
| Caches | Workspace-attributable counters when available; shared pager/base/kernel domains labelled shared | Cache/resource owners, explicit sampling scope |
| Projection | Fixed callback-class counters and upstream call counts | Product counters; never admission gates |

These are a bounded summary, not an unbounded list of Execs, open descriptors,
every changed inode, stages or ancestry. If such lists are later exposed, use
separate paged operations with fixed continuation semantics. Fields may be
unavailable rather than trigger expensive calculation.

Use sufficiently wide checked counters or explicitly marked saturation for
diagnostics. Reaching an observation counter limit must not refuse a file write,
Commit or a long-running command. In particular, the legacy `saved_files: u16`
and `saved_metadata: u16` progress fields cannot become total-workload caps.
Generation/incarnation identity exhaustion is a platform/identity condition to
handle explicitly, not silently wrap into old state.

An installed binding does not necessarily equal the Branch's current external
head: another Workspace can publish afterward. Report what this Workspace is
using. Shared cache sizes, global SQLite page count and host Save statistics are
not per-Workspace exclusive memory or disk. Logical admission accounting and
physical allocated/free-page accounting are distinct.

## 4. Commit knowledge is not a percentage

[proposed outcome observation]

The phase tells the caller what work has entered and which acknowledgements are
known. It is not a speculative progress percentage or a completion receipt.

```text
local phase            status may know                 status must not infer
-----------            ---------------                 ---------------------
Captured               generation/domain + base        candidate is saved
Constructing           emitted work/counters           bytes remaining exactly
Saving                 accepted object windows         Save completion
SaveFinished           saved candidate root            history publication
Staged                 exact stage token               Branch transition
Transitioning          request context                  absent means unpublished
Committed/UpToDate     exact acknowledged head/root     install succeeded
Installed              local binding/revision           every old row reclaimed
Uncertain              retained phase/token/context     retry/delete is safe
```

A known publication plus failed install remains known publication with local
failure. An unknown Save before any history request is not unknown Branch
publication. Unknown stage/transition/discard retains exact context; status
cannot resolve it by looking at an unfenced “absent” row.

```text
running phase -------------------------> exact completion known
      |                                              |
      | lost/unknown required reply                  v
      v                                     known result + local disposition
UNCERTAIN
      |
      +--> status exposes retained knowledge
      +--> no automatic resend / guessed cleanup / hidden resolution
      +--> next Commit and normal unmount refuse pending exact disposition
```

Save and history run in-process in each daemon over its opened shared Store.
Status remains local and issues no Store/history calls to fill gaps. A future exact resolver is a separate
authorized/fenced protocol, not a status side effect. See [Commit](commit.md).

## 5. Exec, terminal unmount and retained cleanup

[owner lifecycle requirements]

An Exec may run indefinitely and multiple Execs may share a Workspace. Status reports actual filesystem/control activity, never command counts or
process completion. Ordinary runtime owns command exit/output/status, and those
observations never authorize filesystem unmount or reclamation.

Per-tool-call is the expected common mode; per-task and long-lived multi-call
reuse are also supported. Exec duration is independent of mode. Repeated
incremental Commit leaves the Workspace Ready with its current base/live state
and may expose ongoing eligible cleanup. Status must not infer terminal lifetime
from a completed call, completed Commit or command class.

Terminal unmount includes logical close and automatic cleanup. Success establishes
mount/routing/activity fences and transfers unreachable local state to bounded
cleanup. Physical row reclamation may continue. There is no separate public
close operation and no resumable overlay implied by status.

```text
READY ---------------------> UNMOUNTING ---------------------> TERMINAL
  | admitted activity          | fences + detach                  |
  |                            |                                 |
  +-- Busy/Uncertain refusal <--+                         owned reclaim debt
      keeps Workspace usable                             may remain in daemon
```

A short-lived bounded terminal observation/tombstone may be retained to distinguish
completed teardown from stale/unknown routing. Its exact retention period is an
implementation decision; do not retain every tool-call Workspace in RAM forever.
After expiry, a terminal/missing route is typed and must not be interpreted as a
history outcome. Eventual cleanup reporting may belong to daemon resource
observation rather than a live Workspace API. Stale jobs keep their original
namespace/custody and cannot mutate a newly mounted Workspace.

## 6. Current source versus target

The original 2026-10-05 dormant SDK/Bridge/daemon status path and serialized
Exec slot are historical baseline observations at the source pin above. Those
source paths were retired/relocated and are not current public APIs.

Current [native control](../../../architecture/68-native-workspace-control.md)
provides a bounded authenticated local observation: coherent binding/control
activity/publication knowledge plus one indexed engine State point, with separate
scope/epoch and no Store SQL. It implements the pre-S8 component route; real
WorkspaceApi facade and native request/owner/lookup/retirement fields remain
R1/R2/R6. No command registry/status/output route belongs to this filesystem
observation. Runtime observes commands independently.

Old consumer-accounted bytes or node/cookie quotas never establish phase-memory,
page-cache or workload caps. Actual maintained counters, field unavailability,
scoped resource observations and exact known/unknown custody govern implementation.

## 7. Workloads and required proofs

[proposed validation; no runs here]

| Workload | Required behavior | Proof / diagnostic |
| --- | --- | --- |
| Full development tree | Same bounded observation cost regardless of untouched base size | No namespace/payload acquisition; maintained-state access counters |
| Many tiny files / edits | Counters update alongside accepted state without dirty-frontier memory growth | Increasing state/counts; bounded summary allocation |
| Large files / sparse files | Status cost independent of logical/content length | No file read or hole expansion |
| Long Bash / continuous logger | Observation available without changing process lifetime or flushing files | No timeout/status dependency; event runner independently reports exit |
| Commit plus live writes | Captured and active fields remain correctly scoped | Coherent phase transitions, no speculative revision or inferred completion |
| Concurrent Workspace Commits | No daemon-wide observation lock over their work | Fair service; one poller cannot starve others |
| Unknown history reply | Exact retained context, no guessed resolution | Fenced test protocol; status issues no history calls |
| Unmount with reclaim debt | Logical terminal versus physical cleanup distinguished | No inline full-row count/delete; bounded tombstone retention |
| Full disk / provider quarantine | Local versus shared failure scopes reported | No hidden reopen/retry; unavailable fields marked explicitly |

Full-tree coverage uses the retained **130,045 entries / 3,475,776,149 regular-file
bytes**, including ignored `.git`, dependencies, symlinks, caches and output.
The **95,021-entry / 2,126,509,110-byte** dependency replay is also retained.
Their historical source/manifest identities are recorded in [Commit](commit.md);
these figures are not new scans or status benchmarks.

Tests must verify that callbacks updating maintained counters do not impose an
artificial file/edit count, that observation allocation is bounded, and that
observers do not hold Workspace locks while waiting on SQL/transport. Counter
diagnostics are labelled and do not substitute for correctness or throughput.

No performance target or measured latency is claimed. Future measurements follow
[07](../07-implementation-validation.md) and root
[AGENTS](../../../../../AGENTS.md): declared equal cache states, one sample per
case/arm, pinned identities, append-only evidence, reused preparation and separate
verification. Ordinary benchmark command/proof budgets remain separate from
unlimited Bash runtime. Status is never used to prime the namespace or data a
later timed phase will read.
