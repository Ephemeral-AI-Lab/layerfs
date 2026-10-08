# Native request dispatcher and direct service ports

> **Status:** Current general guide.

The replacement `layerfs-fuse` is an active core member. The daemon depends on
Fuse, which consumes Workspace plans and existing Content/Overlay domain types.
The former excluded Fuse is preserved byte-for-byte in `layerfs-fuse-legacy`;
its 1447 production lines are reclassified as a predecessor, not retired.

This component implements the shared dispatcher, actual asynchronous engine and
admitted Store adapters, native read continuations and the Linux callback
adapter with a declared disposition and count for every operation. Session
ownership, Attach/Locate/Ready and normal unmount are composed on top of it and
described in [native mount session](76-native-mount-session.md). R1 ControlReady
remains distinct from native Ready. The native surface is the read path;
mutation and mounted Commit are later rollout steps. R6 adds the terminal fence
and the gated ports described [below](#terminal-fence-and-gated-ports). The
connection's one abort write and forced drain are implemented in
[`session/force.rs`](../../crates/layerfs-fuse/src/session/force.rs); the
daemon's forced unmount operation that calls them, and its description in
[native mount session](76-native-mount-session.md), belong to a later R6 track.

## Dispatch and ownership

[Dispatch](../../crates/layerfs-fuse/src/dispatch/workers.rs) creates one fixed
`K = read_handles + 2` worker set for its daemon service. Startup waits for actual
worker entry. The pool persists with zero registered mounts. Failed partial spawn
joins every created worker and retains original errors and panic payloads.
Explicit normal stop refuses Busy without effect while any mount remains.

Each fixed mount lane has sixteen admitted slots, a bounded runnable FIFO and
two borrowed receive guards. A receiver enters accounting before copying names
or data. Only the wait for one of the sixteen slots can block it; that condition
wait releases the scheduler mutex. An admission refusal retains its receive
guard until the caller attempts its terminal reply or no-reply disposal. Already
admitted requests retain their original continuations through terminal fencing:
`stop_admission`, the fence of the normal path, revokes new handoff and wakes
receive waiters and does nothing else to an admitted request.

Workers select one bounded continuation step in round-robin mount order. They
poll outside scheduler and future-storage locks. Pending work parks with a
notification; wakes during polling are coalesced into one later runnable turn.
`NextTurn` yields between completed bounded windows even when all prerequisites
were immediately available. Old notifications validate both the queue instance
and exact request before scheduling a reused slot. Notifications hold weak
request references: a completion waker is not another request owner and cannot
form a request/completion ownership cycle.

Successful disposition drops the original continuation before returning its
slot credit. A failed or panicking request retains the exact error/payload and
continuation, keeps its slot occupied and fences its mount's new admission.
Other mounts remain serviceable. Worker joins alone never establish filesystem
detach or consumer drain. The daemon must retain the Dispatch owner outside its
request workers; a request must not drop or join the pool executing it.

Counters distinguish received, admitted, queued, running, parked and retained
requests. Owned-input charge is capped at the existing128KiB window plus two
255-byte names. `future_bytes` measures the boxed future's inline allocation,
not its nested payloads, native library buffers, provider memory or RSS. Bounded
input charges and slot counts do not substitute for the native memory proof.

## Real service boundaries

[Fuse ports](../../crates/layerfs-fuse/src/ports.rs) contain no daemon commands,
registry or application settings. Each request obtains a fresh StoreOperation
failure scope over the same bound Workspace/cache. The
[daemon adapter](../../crates/layerfs-daemon/src/service/filesystem_port.rs) awaits
the real Owner admission and Pending futures. No helper thread, polling loop,
synchronous owner wait or replay of an attempted job implements this boundary.

`ServiceReply<T>` keeps a projected value and its original Completion together.
Its projection drops before the Completion/credit owner. Payload clones remain
subject to that receipt's lifetime; scalar engine tokens own independent backed
references and can be copied after consuming their receipt. Failed completions
themselves become the retained error, including original unattempted commands.
Local read payloads use the borrowed form directly over the original Completion:
the port adds no data-window clone. Workspace composition owns one mutable window
and an inherited/output buffer in addition to that original. This bounded-copy
source structure does not establish native resident-memory qualification.

The [admitted read provider](../../crates/layerfs-daemon/src/store/read_scope.rs)
uses one already granted ReadLease and the existing CanonicalClient/cache.
Provider calls use a private try-lock only to borrow that owned reader; contention
is an explicit ConcurrentDemand error, never a blocking admission wait. Store
pool locks do not span provider work. All clones of the admitted client/view must
be disposed before parking for another engine job. Provider failure remains in
the original per-request scope and the reader's quarantine custody; how the
request that observed it ends is described under
[Failed base demand](#failed-base-demand).

## Terminal fence and gated ports

Forced teardown needs one thing the normal path does not: a request parked
before an attempt must end without waiting for a resource that may never free.
Each lane owns a [`Fence`](../../crates/layerfs-fuse/src/ports.rs): a stop flag
and a count of terminal replies, shared by the lane, its requests and their
service ports. Nothing sets it on the normal path, so the gates below never
refuse there. A lane already released yields a fence that reads stopped.

[`MountQueue::stop_service`](../../crates/layerfs-fuse/src/dispatch/admission.rs)
is called once, after a written abort. Under one scheduler lock it makes the
lane terminal, stops the fence, moves every parked request to the runnable
queue and marks every running request notified; then it wakes workers and
receive waiters. A receive waiter answers `ENOTCONN` once, as it does after
`stop_admission`. Each parked request gets exactly one extra turn. Every
awaited future registers its wakeup again on each poll, so a request that is
waiting on a job it already submitted simply parks on that job again.
`stop_service` cancels, drops and replays nothing.

`MountServices::request` takes the fence, and the
[daemon adapter](../../crates/layerfs-daemon/src/service/filesystem_port.rs)
consults it only before an attempt, at three kinds of point:

| Gate | Where | A stopped fence |
| --- | --- | --- |
| Owner admission | on entry, and on every poll of the admission wait | returns `Fenced`; the unsubmitted command and its notification slot are dropped |
| Store reader | on entry, and on every poll of the ticket wait | returns `Fenced`; the unstarted ticket is cancelled |
| Entry check | `reserve_serial`, the one synchronous acquiring call | returns `Fenced` before the allocator is asked |

Acquiring calls are gated: `source`, `open_source`, `observe`, `mutate`,
`local_read`, `immutable`, `reserve_serial`, `directory`, `directory_read`,
`directory_page`, `directory_cookies` and `publish_cookies`. Disposal calls are
never gated: `release_read`, `release_source`, `reply_attempted`, `close_file`,
`close_directory` and `forget`. A stopped mount starts nothing new and still
gives back what it holds. `Fenced` is produced only before submission: once a
job is admitted its `Pending` is awaited to the original result, and the
request continues from that result until its next acquiring call.

The fence check and the submission share no lock. A request that passed its
check while the stop was being made can still submit that one acquiring job,
including a mutation. The job is awaited to its original result and the drain
waits for it, so nothing is lost; "starts nothing new" holds from that
request's next gate onwards.

A request whose failure is `Fenced` takes the
[terminal path](../../crates/layerfs-fuse/src/request/terminal.rs): one
`ENOTCONN` reply attempt, one count on the fence, then release of what it holds
through the ungated calls, and `Complete`. A failed release ends `Retained` with
what remains, exactly as a failed release does on the normal path; the reply
was attempted and is counted either way. What can be held at a fenced step:

| Request | Held when fenced | Released |
| --- | --- | --- |
| LOOKUP, GETATTR, OPEN, OPENDIR | nothing, or its source (a fact round acquires no read) | source |
| READ, READLINK | source and, after the deciding job, its read | read, then source |
| Mutation | nothing, or its source; never a publication ticket | source |
| READDIR | nothing, or its source with a page or an unpublished cookie plan | page, listing and plan dropped, then source |
| RELEASEDIR | nothing | — |

No gated call follows a publishing mutation job or a deciding OPEN, LOOKUP or
CREATE job before its reply, so a fenced request never owes a reply for an
effect. A fenced mutation that nevertheless held a ticket would end `Retained`
with it; nothing releases a ticket on this path. An unpublished cookie plan
made no offset valid, and its read row goes with the source. FORGET and RELEASE
use only disposal calls and are never fenced. RELEASEDIR differs from RELEASE:
it first finds its handle through the gated `directory` call, so under a
stopped fence it replies `ENOTCONN` and leaves the handle row for revocation to
retire, while RELEASE still closes its file.

The fence is not a drain. A request waiting for admission of a disposal call,
or on a submitted job, stays admitted until that completes, and the connection
drain observes it. [`fence.rs`](../../crates/layerfs-fuse/tests/fence.rs) covers
the dispatcher half and
[`fenced_port.rs`](../../crates/layerfs-daemon/tests/fenced_port.rs) the port's
gates, the disposal calls and a submitted job, with a real owner and Store and
no kernel mount; receipts are in the
[R6 checks](../issues/307/checks/r6-concurrency-teardown-20261009/). The
callback-level terminal reply on a real aborted mount is proven by the later
forced-unmount rows, not here.

## Failed base demand

A canonical base demand that fails belongs to the request that made it. The
mount keeps serving: the request replies `EIO` once, gives back what it holds
and ends `Complete`. It is not retained and nothing is fenced. No second demand
is made on its behalf and nothing is retried.

The classification is a typed marker, never an error's text.
[`BaseDemandFailed`](../../crates/layerfs-fuse/src/ports.rs) carries the
original cause and has one constructor, `Fence::failed_demand`, which the
[daemon adapter](../../crates/layerfs-daemon/src/service/filesystem_port.rs)
alone calls, for the Store read path only:

| Step | Where it fails | Cause carried |
| --- | --- | --- |
| Reader admission | `immutable`: the read ticket is refused or its wait fails | the scope's earlier failure, or `PortError::ReadAdmission` |
| Fact round | `plan.supply` on the admitted view, in the read and mutation drivers | the `PortError` this request's scope recorded |
| File or link window | `read_file_window`, `readlink_window` | same |
| Directory listing | `native_directory_listing` | same |

The reads of the last three rows run inside Fuse on the view `immutable`
returned, so Fuse cannot see whether the provider failed. It hands the step's
error to
`RequestServices::failed_base_read`, and the adapter answers from this
request's `StorePorts` failure scope: a recorded request-scoped `PortError`
becomes the marker, and otherwise the step's own error is returned unchanged. A
failure at one of those steps with no recorded provider failure is therefore
not a failed base demand: a stale source, contention on the admitted reader, a
poisoned scope.

Only the Store's own answer to this request's read is request-scoped:
`PortError::Storage` (a provider read failure, including one that quarantined
its reader) and `PortError::ReadAdmission` with `NoReaders` or `Stopped` (no
reader is left to admit it). Every other cause is returned unchanged and ends
`Retained` as before: a poisoned pool or scope lock, the admission table bound
(`Capacity`), `InvalidLimits`, a concurrent demand, a reader of another Store
or Workspace refused by `admitted_client`, and `PortError::History`, which is
the serial allocator's write. Those are not staged by a test; they are
classified by source.

Everything else ends `Retained` exactly as before: any owner job's failure, an
uncertain mutation or publication outcome, a failed reply-ticket step, a failed
release, and a mutation that holds a publication ticket whatever its failure.
The [terminal path](../../crates/layerfs-fuse/src/request/terminal.rs) and the
drivers' relinquishers are the ones the fence uses; what a request can hold at
the failed step is the table of the previous section, with `EIO` in place of
`ENOTCONN` and no count on the fence's terminal replies.

The cause survives the request twice. A failure that left a reader's outcome
unknown quarantines that reader, and `Store::reader_failures` keeps the reader's
index with the same original `PortError`. Independently, each lane's `Fence`
keeps a fixed record, read with `MountQueue::fence().failed_demands()`: a count
of failed base demands, the first cause and the most recent cause, each as the
same allocation the request's scope held. Two slots per mount, nothing queued,
never cleared: the first original cause is never replaced, and a cause between
the first and the latest is dropped when a later one arrives. The count is
taken when the demand fails, so a request whose release then fails and is
retained is counted too. A failure that quarantines nothing, such as stopped
read admission, is recorded there alone. The connection's drain receipt
(`Drained`, and `Undrained` at a stop) copies the record, so it leaves with the
terminal unmount's native receipt or stays with the retained custody. No wire
record, status field or `NativeWork` counter carries it.

[`cold_failure_scope.rs`](../../crates/layerfs-daemon/tests/cold_failure_scope.rs)
proves this with a real owner, a Store with two read sessions and a dispatcher
lane, and no kernel mount: a lookup served by a session in an uncertain state
completes holding nothing, the lane is neither terminal nor retaining, the
record and the quarantine hold one and the same `PortError`, later requests
read exact bytes, a read, a mutation and an enumeration refused a reader each
release what they hold, and an owner job's failure is still retained. The
callback-level `EIO` on a real mount is exercised by the mounted failure-scope
rows, not there.

## Native semantic and reply flow

[NativeRead](../../crates/layerfs-fuse/src/operations/lookup.rs) acquires the exact
native source and drives Workspace's existing NativeReadPlan. SQL performs each
bounded local decision; a Needs result releases its original Completion before
waiting for an immutable reader. Supply uses the admitted view, disposes it, then
yields before the next owner round. A positive answer retains the original
observation, processing FileRead and source through the reply consumer. Definite
semantic refusals release their processing source after the reply attempt.
Unknown/failed steps retain request identity, source/read candidates, observation
and exact service/provider failures without guessed cleanup.

READ and READLINK consume their metadata answer before requesting a local window.
The original metadata value/Arc and Completion are disposed; independent FileRead
and source capabilities remain owned. This prevents sixteen metadata consumers
from occupying all sixteen ordinary SQL slots while each requests another job.
The unchanged capacity passes the explicit full-handoff pressure case. They then
acquire a completed local window and reuse Workspace's existing
composition algorithm through `read_file_window` and `readlink_window`. The Store
reader returns before the data reply is consumed. Reply data is disposed before
FileRead and source release. Open and kernel lookup owners remain independent;
the integration test reads an open file after all its lookup references vanish.

[Linux callbacks](../../crates/layerfs-fuse/src/request/callbacks.rs)
wire LOOKUP, GETATTR, OPEN, OPENDIR, READ, READLINK, READDIR, RELEASEDIR,
RELEASE and FORGET into that service. R3 added the mutating callbacks and the
request-service ports they use (`open_source`, `reserve_serial`, `prepare`,
`mutate`, `reply_attempted`); see
[native mutation and kernel coherence](77-native-mutation-coherence.md). The default batch-forget callback invokes
the counted single-unit path. Every other operation has a declared inline answer
or refusal, and each received unit is counted once by opcode and disposal; the
table is in [native mount session](76-native-mount-session.md#callback-dispositions-and-accounting).
GETATTR classifies file/directory handles
through one indexed union in an atomic source acquisition, without failed-kind
fallback. Closed, foreign or mismatched handles cannot acquire a new source.

The [directory consumer](../../crates/layerfs-fuse/src/operations/directory.rs)
holds one independently owned read source. A bounded deep copy consumes the
original read reply; sharing an Arc alone is not a custody transfer. Each page
completion remains through canonical reads and is consumed before the next SQL
job. An offered cookie plan is copied into bounded request storage before its
original completion is consumed, leaving ordinary SQL credit for publication.
Sixteen offered batches can therefore publish prefixes at the unchanged limits.
The consuming `DirectoryBatch::accept` permits one attempt for the exact prefix
that fit, including zero; unused reservations do not become positions. One
published window ends the reply: a directory read owns one cookie plan, so a
full 64-name window with names remaining is answered short and the kernel's
next READDIR resumes after its last cookie. Empty
whiteout pages continue with a yielded turn. Existing sources and cookie plans
survive descriptor close; new handle acquisitions fail. Failure retains original
page/cookie responses, source, input offset and requested publication prefix.

The native reply adapter limits encoded directory output to128KiB using the pinned
24-byte fuse_dirent header and8-byte alignment. Actual fuser insertion determines
the accepted prefix. Further names resume through the last published cookie;
this is no namespace-size cap. Kernel buffer and resource qualification remain
unrun. [Consumer checkpoint evidence](../issues/307/checks/r2-native-consumers-20261008/36-results.md)
records the original sixteen-slot stall and the subsequent source/receipt scope.

The pinned fuser reply consumes one reply attempt but exposes no delivery result.
No success is inferred from it. Conversion failures retain the decided request.
The root maps to kernel inode1; other canonical serials map by checked `serial+1`,
with the inverse rejecting the root's unused alias. This preserves stable identity
without assuming the canonical root serial is1 or allocating a resident map.
Attributes project configured command UID/GID, portable mode and signed fractional
mtime; atime/ctime equal mtime, directory nlink2, and block size4096. Overflow is an
explicit error. Historical fractional signed-minimum native timestamp limitations
remain unchanged by these attribute conversion tests.

[Negotiation](../../crates/layerfs-fuse/src/mount/profile.rs) records the actual
offered flags separately from pinned fuser defaults selected by this adapter.
It requests128KiB windows, background/congestion1 and1ns granularity. It adds no
optional capability; writeback stays absent. The kernel-negotiated receipt,
serving loops, mount flags, permission isolation, reversible EBUSY and normal
drain are exercised by real mounts whose scope and receipts are listed in the
[R2 completion record](../issues/307/R2-COMPLETION-20261008.md). That record
also lists what remains unrun; none of it is a resource or timing measurement.
