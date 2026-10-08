# Native request dispatcher and direct service ports

> **Status:** Current general guide.

The replacement `layerfs-fuse` is an active core member. The daemon depends on
Fuse, which consumes Workspace plans and existing Content/Overlay domain types.
The former excluded Fuse is preserved byte-for-byte in `layerfs-fuse-legacy`;
its 1447 production lines are reclassified as a predecessor, not retired.

This component implements the shared dispatcher, actual asynchronous engine and
admitted Store adapters, native read continuations and an initial Linux callback
adapter. It does not yet mount the application: READDIR/RELEASEDIR, complete
callback accounting, session ownership, Attach/Locate/Ready and normal unmount
composition remain required R2 work. R1 ControlReady remains distinct from native
Ready. These interfaces are implementation documentation, not mounted acceptance.

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
admitted requests retain their original continuations through terminal fencing.

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
the original per-request scope and the reader's quarantine custody.

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

READ and READLINK acquire a completed local window and reuse Workspace's existing
composition algorithm through `read_file_window` and `readlink_window`. The Store
reader returns before the data reply is consumed. Reply data is disposed before
FileRead and source release. Open and kernel lookup owners remain independent;
the integration test reads an open file after all its lookup references vanish.

[Linux callbacks](../../crates/layerfs-fuse/src/request/callbacks.rs) currently
wire LOOKUP, GETATTR, OPEN, OPENDIR, READ, READLINK, RELEASE and FORGET into that
service. The default batch-forget callback invokes the counted single-unit path.
Remaining callback families and complete opcode/unit observations still need
integration before native qualification. In particular, directory handles need
their complete native callback consumer and GETATTR association.

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
optional capability; writeback stays absent. The current compilation/attribute
tests do not prove a kernel-negotiated receipt, serving loops, mount flags,
permission isolation, reversible EBUSY or complete normal drain.
