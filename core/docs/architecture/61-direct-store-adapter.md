# Direct daemon Store adapter

> **Status:** Implemented source boundary and Store-half Commit; native control
> follows separately. Source introduced after `4e8247af5`.

`layerfs-daemon::bootstrap::open_store` opens the configured global provider
writer first, then a fixed set of read-only sessions. It passes Storage and
History ports into `store::Store`; the adapter has no database engine types,
paths, journal settings or provider ownership protocol. A later provider changes
application composition. Failure returns once, without conversion or fallback.

One Store owns one fixed read-handle set and one byte-bounded CanonicalCache
shared by all its Workspaces. A read chooses one handle and retains its memory
ownership only for that bounded demand. StorePorts forwards a complete object
or length batch to Reader, retaining order/cardinality and the existing4096-id,
32MiB canonical demand bounds. These are windows, not file/namespace caps.
A cached object invokes no Store port. The existing canonical object format and
Storage locator/pack algorithms are unchanged. Adapter counters expose actual
object/length demand counts and ids, and serial reservations; they are not a
substitute for lower-level SQL/I/O/resource receipts.

A fresh StorePorts belongs to one operation. Its first exact Storage or History
failure survives behind an Arc; further provider calls through that failed
operation return that same failure without attempting another demand. Successful
immutable objects remain in the shared cache. A separately requested operation
gets fresh failure custody and a scoped Workspace over the same current base
and local serial ranges. There is no remote data channel.

Bind captures one coherent BranchSnapshot, then checks the root/profile/scope,
root inode path, directory root page and portable metadata paths. It never walks
the namespace or qualifies topology. Only after those checks does it submit one
local Open. Its success retains the original Open Completion; a refusal retains
the request, captured snapshot and exact error or attempted Completion. Dropping
a receipt does not close a Workspace. FUSE attachment/readiness is separate.

Several bound Workspaces use one existing overlay database, with independent
routes, namespaces and local ranges. Inode ranges come from the shared History
allocator, are consumed once and never recycled. Simultaneous writes may return
typed Busy; the adapter never makes a second attempt. Base reads use the fixed
read set while another process owns the writer.

Every producer obtains its own Storage state through Store::producer, using
the startup ReservationBlocks passed through concrete bootstrap. Only the
opened provider is shared. No mutable Save is global, no ownership spans a
whole Commit, and Init acquisition/cleanup is unavailable through these ports.
The combined publication and paired local install are now implemented in
[Store Commit](65-store-commit-composition.md); S10 still supplies live namespace
normalization.

The external installed-store proof creates/imports/seals a real Store and copies
that closed file into an independent fixture placement, removes the native
source and original sealed file, then uses normal daemon bootstrap. That copy
is fixture setup; it does not establish the product host install/control path.
Linux fixtures live on the container filesystem, never the repository share.
See [F6/F7 evidence](../issues/307/PRE-S8-F6-F7-20261007.md). The old upstream
source is retired in [F12](../issues/307/PRE-S8-F12-20261007.md).


## R2 fixed read admission and read-only snapshots

The R2 reader checkpoint replaces blind rotating mutex checkout with
[`ReadTicket`](../../crates/layerfs-daemon/src/store/read_service.rs) and
[`ReadLease`](../../crates/layerfs-daemon/src/store/read_handle.rs). Bootstrap pairs
each actual read-only Storage provider with its History provider in StoreReader.
The fixed read set is opened once and never enlarged or reopened on error.
Bind obtains its coherent Branch snapshot through such a read-only session;
Store::history remains the write/publication authority for its existing callers.

The pool preallocates fixed per-Workspace lanes and ticket slots. Admission is
before effect and returns Capacity when those concurrent windows are full. None
selects an explicitly unscoped control/constructor lane. Native application
composition derives the lane count as admitted namespaces plus one, and the
per-lane window as R16 plus its fixed control-connection limit. Standalone
bootstrap retains an explicit default of17 lanes/18 requests; this is not a
file/namespace population limit or a whole-process resident bound.

Waiting jobs are FIFO within a Workspace. Assignment advances a cursor over the
fixed lane array, so later batches from one busy Workspace cannot repeatedly
claim the next reader ahead of another waiting lane. Ready tickets already own
their assigned reader. Poll atomically registers its waker or takes that exact
reader; last-reader return, cancellation and stop notify outside the pool lock.
There is no reader helper thread, spinning, busy-reader mutex wait or provider
call under the scheduler lock. A native worker awaits the ticket, runs its bounded
original demand on the lease, and returns the lease after its consumer finishes.
The per-lease receipt exposes reader index, Workspace identity and original
admission-to-grant wait. Aggregate counters keep their separate diagnostic scope.

Unknown provider outcomes remove the session from future service after its
current consumer finishes. The original opened handle and same Arc error remain
in reader_failures; no guessed reset, transaction rollback, replay or reopen is
performed by this pool. If every reader is quarantined, waiting and later
admissions receive NoReaders. Whole-Store stop disposes only unstarted admission;
already leased work retains its original session. Poison is explicit in work
observations and prevents a per-Workspace count from claiming clean drain.

StorePorts uses a short failure mutex and an atomic original-attempt guard;
no failure mutex spans admission waiting or provider I/O. A concurrent call on
one failure scope is a before-effect ConcurrentDemand refusal. Subsequent calls
after an actual provider error retain that first Arc. Separate requests use fresh
ports_for(scope, WorkspaceId), preserving the same immutable cache. Explicit
objects_on/file_lengths_on reject a reader from a different Store or Workspace
before provider work. Synchronous compatibility methods still wait and are for
control/constructor threads, never native Fuse workers.

[Checkpoint receipts](../issues/307/checks/r2-store-read-service-20261008/45-results.md)
cover actual Disposable Store batches, fair grants, registered wakeups, cancellation,
stop, failure-scope isolation, real provider quarantine induced through a public
uncertain read-plan result, and successful bind while the writer's own session
returns Busy. The quarantine case is not a disk-I/O fault campaign. Native
per-demand orchestration, status wire fields, mounted fairness, aggregate drain
and full R2 acceptance remain separate required integration. No new SQLite query,
canonical format, speed/storage or cold/RSS claim accompanies this scheduler.
