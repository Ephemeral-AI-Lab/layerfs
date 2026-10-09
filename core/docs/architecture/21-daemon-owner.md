# Daemon-owned fair overlay service

Terminology update2026-10-07: the current API/SQL names below use overlay
schema16. Earlier algorithm and proof pins retain their original scope; see
[the naming checkpoint](../issues/307/PRE-S8-TERMINOLOGY-20261007.md).

> **Status:** Current general guide.

S5 update after `f5558fc22`: `SourceRead` provides one composed local byte
window, with base I/O outside the owner. `DatabaseWork` exposes a connection
counter snapshot after route validation, and `PayloadPlans` exposes the actual
payload plans under an owned source. Their response capacities are credited;
large work snapshots are boxed so ordinary replies retain their prior size.
See [payload streams](31-payload-streams.md) for retained whole-operation profiles
and caller/native credit limitations. These diagnostics do not measure residency.

S6 adds [independent custody jobs](33-independent-custody.md) for opens, read
windows, sealed readers and processing operation records, with exact retained observations
and releases. Physical reservation/headroom remains unfinished.

Implemented source: the S2 checkpoint after `d9d8d1b04`, for
[tracker #307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307).
This active daemon library owns SQL service; native executable/FUSE/control/Exec
and complete S2/S8 acceptance remain unfinished.

[Owner](../../crates/layerfs-daemon/src/overlay/owner.rs) starts one connection thread,
creates/checks the overlay there and acknowledges readiness only after schema/
profile readback. Clients submit typed short SQL windows. A command cannot contain
a closure or hold the database across construction, transport or an entire Exec.
No dependency or route invokes the retired server or old daemon implementation.

R7 update, 2026-10-09 (combined turns, reply attempts): a submitting
thread's turn does not end at a collision. When its job is done and another
runnable job was queued meanwhile, the same thread serves that one too
(`Shared::pass`): the next job of the same fair selection, at most one per
turn (`COMBINED`), and never ahead of maintenance that is due. Only then is
the turn returned, and the owner thread is woken only if work remains. A
request that arrives while another thread holds the turn is therefore served
without waking the owner thread. A published mutation's reply attempt is no
longer an owner job: `OwnerClient::reply_attempted` records it in the
engine's memory from the replying thread
([overlay](19-daemon-overlay.md)). A parked capture is still released by a
Lifecycle job: the attempt that empties a watched namespace submits
`Command::ReplySettled`.

R7 update, 2026-10-09: the connection is no longer confined to that thread.
It lives in the owner's shared state and is used one exclusive **turn** at a
time (`State::busy`). A turn is one job or one maintenance step. The owner
thread still takes turns for queued work and for maintenance. A thread that
submits a job while nothing else is queued and no turn is held takes the turn
itself: `OwnerClient::try_submit` admits and queues the job exactly as
before, takes it back through the same namespace rotation and lane fences,
and runs the same readiness check and service on the submitting thread. The
job is one transaction on the one connection either way; only the two thread
hand-offs are gone. Any other submission (something queued, a turn held, a
job a fence holds back) wakes the owner thread as before. The end of a
submitter's turn wakes the owner thread only when a job is queued or the
connection reports possible maintenance (`Overlay::maintenance_pending`);
credit release and progress no longer wake it for nothing. A panic inside a
submitter's turn stops the owner like the loss of its thread: admission
closes, queued jobs get their commands back unattempted and the turn is never
returned. The connection is stored before readiness is reported and closed by
the owner thread when it exits, after any turn still held.

[Queues](../../crates/layerfs-daemon/src/overlay/queue.rs) rotate between namespaces and
six service classes. Their roster contains only admitted outstanding requests,
not filesystem entries; configured concurrent slots/bytes bound it. When no job
is runnable, the owner thread waits on a condition variable for a queued job,
reported maintenance or shutdown. There is no idle polling or a busy-inode
waiter occupying the connection.

An admitted capture orders after earlier queued mutations in the same namespace.
It parks until their published reply-send-attempt tickets settle, using a read-only
readiness check before its one capture attempt. Later mutations remain deferred
behind that finite fence, so continuous later publication cannot starve capture.
Reads, lifecycle/reply attempts and other namespaces remain runnable. After sealing,
later mutations resolve the new active generation inside their ordinary transaction.
These are service/SQL proofs; no native kernel delivery receipt is inferred.

[Credits](../../crates/layerfs-daemon/src/overlay/credits.rs) count queued, executing and
caller-retained results. They include actual owned Vec capacity, fixed cell bytes,
declared worst reply windows and bookkeeping allowance. Weak backreferences avoid
an ownership cycle. Completion data stays borrowed from its charged owner. Ordinary
jobs leave reserved lifecycle/control bytes and slots; startup verifies those
reserves can cover the configured lifecycle slots. Status observations and reply/
release work use that lane. This is logical admission accounting, not measured
whole-process/pager/kernel residency. Native producers still need their own credits
before copying request arguments; that dispatch integration is S8 work.

Owner shutdown fences future admission and returns Stopped for unattempted queued
work. A capture returning from readiness cannot requeue behind the stopping fence.
An already attempted short job can complete with its actual result; stopping does
not claim rollback of published state. Each job has one publisher and one
publication (see the completion-ownership checkpoint below; the earlier
single-slot result channel is replaced). No queue lock spans SQL, result
delivery or worker join.
Whole daemon shutdown is distinct from one Workspace's terminal unmount.

The overlay's new schema v3 records an installed-generation floor. A known install
checks the exact capture, advances base/floor, clears that capture and enqueues
retirement in fixed metadata work. Active rows and pending later replies remain;
retired rows cannot shadow the new base while awaiting physical deletion. Captured
inode/name cursors have fixed generation and keyset EOF; name cursors use the
`directory_entry_capture` index and `(parent,binary name)` resume keys. The trusted caller
must provide the root actually constructed/saved/published from that capture.
Reader eligibility, orphan composition and automatic physical deletion remain S6
work, so this is not integrated Commit or reclamation acceptance.

Fair dispatch examines at most W configured outstanding namespace lanes and five
classes, with at most J configured queued jobs for a finite capture fence. Each
served job pays its actual indexed SQL/window cost; rotation provides service turns,
not a device-latency or sustained-throughput guarantee. Queue wait and service wall
are separately aggregated in fixed arrays. Capture with blocked earlier replies has
real wait; unrelated progress and lifecycle capacity are proven through public
commands. The 24-cycle install proof preserves later metadata/cell/reply state,
and names paginate without revisiting prior prefixes. These small proofs do not
establish aggregate end-to-end/resource acceptance.

The dormant daemon is temporarily relocated at `layerfs-daemon-legacy`, excluded,
with its package manifest explicitly renamed. Its 2,151 implementation LOC remain
counted and no active consumer uses it. This additional preservation step follows
the same temporary-reference pattern as Workspace; S11 must remove it once actual
native integration has replacement coverage. Root reference remains for S13.

Remaining: weighted automatic maintenance, physical admission/debt, full generation/
reader/orphan/failure composition, mutable filesystem semantics, actual deferred
FUSE replies and coherent mmap caches, ordinary Bash streams, authenticated runtime,
canonical Commit and integrated qualification. No completed milestone is inferred.

## Terminal maintenance checkpoint (#307)

The slice after `c6df039e6` adds schema v4, exact close eligibility and bounded
automatic terminal deletion during live/idle daemon periods. See
[terminal reclaim](25-terminal-reclaim.md) for actual indexed scope, original
error retention and remaining live-generation/orphan/pressure/kernel criteria.
Earlier schema/profile evidence retains its original pin.

## S2 completion reconciliation

The closure after `7019801f9` completes the generation/service exits, including
lost internal completion custody, full capture identity, engine-affine routing
and bounded charged name/cell/ticket replies. [S2 exit audit](../issues/307/S2-EXIT-AUDIT.md)
maps every exit to source/host/Linux evidence. The earlier checkpoint limitations
above retain their source scope; live-generation/orphan/normal failed-Commit
resolution, physical pressure and native deferred replies remain S6/S8 work.

## Base-source prerequisite checkpoint

The slice after `6e84b9181` adds schema6 transient source custody and maintained
install readiness. The owner now has six classes: new source acquisitions wait
behind a finite known-install fence while existing reads/releases, mutations and
other namespaces progress. See [base-source windows](28-base-source-windows.md).
Those waiting acquisitions are admitted against their own lane slots; see
[R6 slot accounting](#r6-slot-accounting-source-acquisitions-have-their-own-bound).
Earlier schema/class/query receipts keep their original pins. Actual prepared
Workspace/actor/native composition and effective merge remain required.

## S3 completion reconciliation

The completion after `bdc6ed4af` adds effective source-qualified read/ordered-name
composition, actual paired actor install, original unattempted-command custody and
owning SDK stat lengths. See [effective base view](29-effective-base-view.md) and
[S3 exit audit](../issues/307/S3-EXIT-AUDIT.md). Earlier limitations/evidence above
retain their source scope; native/logical runtime transport, mutable byte semantics
and aggregate resource acceptance remain unfinished.

## S4 namespace jobs

The checkpoint after `8d691ab8a` adds `Command::Namespace`, a Mutation-class job
carrying one complete Workspace operation round as data, and a Read-class
`SourceCell`. The owner evaluates and publishes inside that one job and performs
no provider work; a round that needs base facts returns them to the caller's
thread. `OwnerClient` implements the Workspace job port, so unadmitted jobs keep
returning their original command and an attempted job's failure is returned as
`OwnerError::Workspace` without replay. Multi-round operations hold no ticket
before their publication, so they never delay a capture. See
[namespace operations](30-namespace-operations.md). Native deferred replies,
per-request credit residency and source/reply job consolidation remain S7/S8.

S6 completion update after `be651a048`: schema14 now includes backed resource
accounting and indexed source waits; physical reservation, cleanup headroom,
bounded generation wakes and nonduplicating orphan migration are implemented.
See [shared physical capacity](35-shared-physical-capacity.md) and the
[S6 exit audit](../issues/307/S6-EXIT-AUDIT.md) for current scope/evidence. Earlier
checkpoint limitations and numbers above retain their original source identity.
Native/runtime/kernel and integrated qualification remain later milestones.

## S7 completion ownership and family receipts (#307)

Implemented source: the checkpoint after `490c3ab3a`. Admission numbers are
unchanged: 8 MiB total, 64 KiB lifecycle reserve, 16 namespaces, two lifecycle
slots each. This describes source and scoped functional proofs, not S7
qualification; see the [checkpoint report](../issues/307/S7-COMPLETION-OWNERSHIP-20261007.md).

[Completion](../../crates/layerfs-daemon/src/service/completion.rs) gives each
admitted job one typed cell shared by the caller's `Pending`/`Completion` and
the owner's single publisher. The cell carries the job's credit, so the credit
lasts exactly as long as anything can still reach the result. A publication is
handed out once; later takes report `Disconnected`, and a job lost without an
outcome disconnects its waiter. A blocked `wait` registers its thread before
the transition the publisher observes and is unparked by that publication.
The later R2 future extension below adds one fixed notification mutex; no
per-job channel or condition variable is allocated.

### R2 event-driven pending completion, 2026-10-08

Source extension after4236225ee: `Pending` now implements the standard
`Future<Output = Result<Completion, OwnerError>>`. A native service can register
its task and relinquish its worker until publication or publisher loss wakes it.
The existing synchronous `wait` and nonblocking `try_complete` remain available.
The [selected component receipts](../issues/307/checks/r2-completion-future-20261008/01-selection.json)
identify this source/verification scope separately from native acceptance.

Each completion cell contains one fixed `Mutex<Option<Waker>>` slot. Poll checks
and consumes the original atomic result state while holding that registration
lock. If pending, it installs the latest task's waker before releasing the lock.
Publication/loss changes the atomic state, takes the registered waker under the
same lock, then wakes outside it. Thus completion before registration is returned
directly; a racing pending return receives an event. Re-poll replaces the earlier
task's registration. Waker clone/drop/callback behavior runs outside the lock.
Notification never submits, retries or consumes the original command itself.

The slot is charged through the existing `size_of<Cell>()` admission formula.
Executor task/queue storage is the executor's separate resource domain; a Waker
is not evidence that that domain is bounded. Original outcome, family receipts
and credit remain owned by the cell until Publisher/Pending/Completion disposal.
Terminal notification removes the slot's reference to the task. A dropped future
does not cancel an admitted job or revoke backed filesystem ownership.

This is the completion event primitive only. Admission-credit events, the Fuse
dispatcher, Store reader admission and resumable filesystem semantics still need
their own implementation. A native worker must use this event path rather than
calling wait or repeatedly polling try_complete. No runtime, dependency, SQL
algorithm, request scheduler or native readiness claim is added by this change.

A job's storage is one of three stages which never coexist: the boxed queued
job, the boxed outcome (result plus receipt) allocated when the job finishes,
or an unattempted outcome returning the original boxed command and cause. The
queued box is released before the outcome is allocated.
[Queues](../../crates/layerfs-daemon/src/overlay/queue.rs) hold job pointers in
a lane table whose capacity is allocated once at startup from the configured
namespace and slot limits; it never grows. Those fixed scheduler bytes are
reported as `OwnerWork::scheduler_bytes` and subtracted from the bytes
available to jobs, so the configured total still bounds scheduler plus credits.
`queued`/`peak_queued` count admitted jobs waiting in a lane, including parked
ones; they exclude the executing job and caller-held results.

[JobSql](../../crates/layerfs-daemon/src/service/job_sql.rs) is the receipt's
statement-family attribution: one exact row per family that observed work in
this job's exclusive turns, including readiness turns which parked it. The
owner takes one connection delta per turn and gives the same delta to the
foreground aggregate and to the job, publishing the aggregate before the
completion is visible. Original-job family sums therefore equal the foreground
family deltas, on success, failure, parked and unattempted paths. Every
terminal outcome other than a stop cancellation is counted in `completed`.

A job is charged the largest of its three stages, the cell, the declared reply
(never less than the inline result value) and the unchanged 512-byte
bookkeeping allowance. Lifecycle jobs never park and are charged for seven
family rows: the pre-BEGIN freelist read, BEGIN, COMMIT or ROLLBACK, and at
most four domain families. That bound is a maintained source invariant reached
exactly by `ResolveFailed` and `ReleaseClosedCapture`; it limits the admitted
charge only. A further actual family is retained, charged when observed and
reported in `receipt_overruns`/`receipt_overrun_bytes`, never dropped. Other
classes are charged for all fourteen families in whichever stage holds them.
Startup refuses a configuration whose lifecycle slots, at this real charge, do
not fit the reserve, or whose scheduler state leaves no ordinary bytes.

These are logical ownership credits over requested Rust allocation sizes.
Allocator rounding is the allowance; SQLite, pager, kernel and process
residency are not measured here. The lanes table is O(namespaces x slots)
pointers; admission scans at most the configured namespaces.

A later pre-S8 diagnostic adds `OwnerWork.closed_namespaces`, counted only after
an acknowledged terminal reclaim step completes a namespace. Live maintenance
and logical Close do not increment it. This memory observation permits an owner
to observe an exact finite drain without submitting a SQL/status cleanup pump;
it is cumulative, not a new retention or runtime capacity limit. The
[finite engine plan/proof](../issues/307/PRE-S8-F10-F14-ENGINE-PLAN-20261007.md)
retains the earlier partial-drain observations and the successor receipts.

The pre-S8 [accounting checkpoint](../issues/307/PRE-S8-F14-20261007.md) now
registers and executes the direct finite-engine and Store-half Commit routes on
macOS/Linux. Independent validation reconciles original family receipts and all
opened Store-handle counters, with supported process/cgroup/file observations.
Continuous phase peaks, exclusive physical attribution and numerical acceptance
remain explicitly unqualified; diagnostic observations do not change Owner policy.
The [focused growth follow-up](../issues/307/PRE-S8-RESOURCE-GROWTH-RESULTS-20261007.md)
adds external Rust live-request accounting, actual increasing-size/repeated-Commit
observations and exact released ownership. It changes no product allocator or
admission policy; whole-process/OS cache remains a separate resource domain.


## R2 event-driven admission and worker exit

The R2 admission checkpoint adds
[`OwnerClient::submit_when_available`](../../crates/layerfs-daemon/src/overlay/admission.rs).
It retains the original unattempted command and returns a Future that yields the
same `Pending` used by immediate admission. Registering/arming precedes checking
credit, so release before registration, release during the check and terminal
stop cannot lose the event. Spurious polls replace the current task without
trying admission again until a credit/stop event. SQL executes only after one
successful admission; there is no failed-operation replay.

The notification table has exactly `namespaces * (jobs_per_namespace +
lifecycle_jobs_per_namespace)` slots at startup. Its actual allocation capacity
is charged to `OwnerWork.scheduler_bytes` and removed from available job bytes.
Registration has an explicit before-effect capacity refusal. Native assembly must
budget one registration for every admitted ingress continuation waiting on SQL;
this table does not itself charge the caller's original command payload or task
allocation. Native request credits and fair Fuse resumption remain required.
An input larger than its entire class capacity is refused immediately, while
transient occupancy can wait for last-credit release. `Admission::into_command`
returns an unattempted input and frees the slot; dropping a returned `Pending`
does not cancel an admitted command.

Last-credit disposal and terminal stop wake tasks outside the queue and notifier
locks. Notification scans only the fixed configured table, without allocating
an event-sized vector. It does not scan filesystem objects or queued commands.
`Pending` separately implements Future for one original completion, retaining its
credit until all Publisher/Pending/Completion owners are gone.

The SQL thread's exit fence now stops admission and returns original unattempted
queued jobs on normal exit or unwind. It does not rewrite a result already
published before a callback panic. Explicit `Owner::stop` retains the original
panic in `OwnerError::WorkerPanicked`, whether the owner thread or a
submitting thread's turn panicked; its mutex preserves Send/Sync error custody
for a payload that is only Send. A panic in a submitting thread's turn does
not unwind into the submitter: admission closes, the turn is never returned
and that job's result reads disconnected. The fence also closes the
connection, after any turn a submitting thread still holds. Possible
maintenance is never held back by more than eight served jobs: a submitter
does not take its own turn while that bound is reached. Owner Drop remains best-effort cleanup;
call explicit stop when the result must be observed. This component is not the
native session/namespace drain conjunction.

[Component receipts](../issues/307/checks/r2-admission-future-20261008/)
retain the oversized-input regression, the initial service-class fixture error,
the worker-loss three-second failed wait, the Send/Sync compile failure and the
subsequent host/Linux selections. No native Ready, scheduling throughput, cold
cache or process-residency qualification follows from this component.

## R6 slot accounting: Source acquisitions have their own bound

R6 track A changes how a lane counts its admitted jobs
([plan, section 2.1](../issues/307/checks/r6-concurrency-teardown-20261009/01-deepest-file-plan.md)).
A [lane](../../crates/layerfs-daemon/src/overlay/queue.rs) keeps three
counters, and `Lane::slots` selects the one a class is admitted against:

| Counter | Classes | Bound per lane |
| --- | --- | --- |
| `lifecycle` | Lifecycle | `lifecycle_jobs_per_namespace` (2) |
| `source` | Source: `AcquireBaseSource`, the native `Source`, `FileSource`, `HandleSource` and `OpenSource`, and the directory `Read` | `jobs_per_namespace` (16) |
| `ordinary` | Read, Mutation, Capture and OperationRecord together | `jobs_per_namespace` (16) |

`try_submit` refuses `AdmissionFull` at the matching bound and otherwise
increments that counter; dropping the job's credit decrements it; the lane
leaves the rotation when all three are zero. Each counter still covers queued,
executing and caller-held results.

Before this change Source jobs shared the `ordinary` counter. A queued known
install holds back every later Source job of its lane and itself parks until
the earlier base readers release. The install and fifteen held-back
acquisitions then filled all sixteen ordinary slots, and the request whose
source the install waited for could not admit its next Read or Mutation job:
it waited for a slot that only its own release could free. Held-back
acquisitions now occupy Source slots, which a holder does not need. After its
one acquisition a native request submits only Read, Mutation and Lifecycle
jobs. An acquisition arriving at the Source bound is refused before any effect
and waits holding nothing.

No fixed bound moved. `OwnerConfig`, the credited-byte limit, the lifecycle
reserve and the notification table are unchanged, and no queue grows: every
class queue already had its bound as capacity, and the Source queue is limited
by the Source counter. A lane can now hold up to
`2 * jobs_per_namespace + lifecycle_jobs_per_namespace` admitted jobs (34 with
the defaults, 18 before), so `outstanding`, `queued` and `peak_queued` can read
higher under the same byte limit. `Lane` grows by one word, which
`scheduler_bytes` reports for each configured lane.

[`install_slots.rs`](../../crates/layerfs-daemon/tests/install_slots.rs)
stages the cycle through the public owner API with the default configuration:
one held source, a parked install, fifteen held-back acquisitions, then the
holder's Read job. It also fills the Source and ordinary bounds beside each
other and checks that each refuses on its own. On the source before this
change the same test stops at the holder's Read job with `AdmissionFull`; that
receipt and the host and Linux runs are in the
[R6 receipts](../issues/307/checks/r6-concurrency-teardown-20261009/). This is
an owner-scope functional proof. It makes no timing, fairness or residency
claim, and it does not stage the mounted interleaving. A request that stays
retained with its source still parks an install; see
[product Commit limits](79-product-commit.md#limits).
