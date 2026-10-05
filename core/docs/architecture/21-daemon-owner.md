# Daemon-owned fair overlay service

> **Status:** Current general guide.

Implemented source: the S2 checkpoint after `d9d8d1b04`, for
[tracker #307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307).
This active daemon library owns SQL service; native executable/FUSE/control/Exec
and complete S2/S8 acceptance remain unfinished.

[Owner](../../crates/layerfs-daemon/src/owner.rs) starts one connection thread,
creates/checks the overlay there and acknowledges readiness only after schema/
profile readback. Clients submit typed short SQL windows. A command cannot contain
a closure or hold the database across construction, transport or an entire Exec.
No dependency or route invokes the retired server or old daemon implementation.

[Queues](../../crates/layerfs-daemon/src/queue.rs) rotate between namespaces and
five service classes. Their roster contains only admitted outstanding requests,
not filesystem entries; configured concurrent slots/bytes bound it. When no job
is runnable, a condition variable waits for admission/progress/shutdown events.
There is no idle polling or a busy-inode waiter occupying the connection thread.

An admitted capture orders after earlier queued mutations in the same namespace.
It parks until their published reply-send-attempt tickets settle, using a read-only
readiness check before its one capture attempt. Later mutations remain deferred
behind that finite fence, so continuous later publication cannot starve capture.
Reads, lifecycle/reply attempts and other namespaces remain runnable. After sealing,
later mutations resolve the new active generation inside their ordinary transaction.
These are service/SQL proofs; no native kernel delivery receipt is inferred.

[Credits](../../crates/layerfs-daemon/src/credits.rs) count queued, executing and
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
not claim rollback of published state. Single-slot result channels have one sender
and one send per job. No queue lock spans SQL, result delivery or worker join.
Whole daemon shutdown is distinct from one Workspace's terminal unmount.

The overlay's new schema v3 records an installed-generation floor. A known install
checks the exact capture, advances base/floor, clears that capture and enqueues
retirement in fixed metadata work. Active rows and pending later replies remain;
retired rows cannot shadow the new base while awaiting physical deletion. Captured
inode/name cursors have fixed generation and keyset EOF; name cursors use the
`dentry_capture` index and `(parent,binary name)` resume keys. The trusted caller
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
