# Direct daemon Store adapter

> **Status:** Implemented source boundary and Store-half Commit; native control
> follows separately. Source introduced after `4e8247af5`.

`layerfs-daemon::bootstrap::open_store` opens the configured global provider
writer first, then one read-only control session and a mount-time reader factory. It passes Storage and
History ports into `store::Store`; the adapter has no database engine types,
paths, journal settings or provider ownership protocol. A later provider changes
application composition. Failure returns once, without conversion or fallback.

One Store owns one byte-bounded CanonicalCache shared by all its Workspaces.
Bind opens one read-only session for each Workspace before its root checks and
readiness. Each provider demand holds only that Workspace reader lock and at
most one short read transaction; object locates, pack reads and stored
inode-value reads take none, each statement reading rows that never change in
its own snapshot (#352 F-035). No
exclusive lease or transaction spans request steps. StorePorts forwards a complete object
or length batch to Reader, retaining order/cardinality and the existing4096-id,
32MiB canonical demand bounds. These are windows, not file/namespace caps.
`StorePorts::objects_with` serves the same demand through caches the caller
keeps across its demands (`layerfs_storage::ReadCache`, the Reader's bounded
decode arena and pack/group/pooled-value caches); an SDK read call keeps one
for the call and drops it at the end (#352 F-035). A Workspace reader keeps
one `layerfs_storage::KeptReads` (the same caches without the decode arena,
which each demand takes fresh) from its first object or length demand until
the reader is released at unmount, so a FUSE request's demand reuses the pack
bodies and decoded groups its siblings' demands read (#354 A1). It holds at
most `KeptReads::ORDINARY_BYTES` (3 MiB: 2 MiB pack bodies, 512 KiB decoded
groups, 512 KiB pooled values) between demands: a demand that fails or leaves
an isolated singleton pack drops it, and the next starts empty. It keeps only
rows that never change once visible and never an absence. The Store's control
reader (session 0), which serves every SDK call, keeps none of its own.

The daemon's CanonicalCache budget is the `cache_bytes` the SDK passes at
Sandbox create (`engine/crates/layerfs-node/src/sandbox.rs`): **64 MiB**, by owner decision
2026-10-19 (#359, [fix record 031](../issues/352/fixes/031-359-warm-daemon-cache.md));
it was 8 MiB, which one `find`, vitest or `ls -l` of 10,000 entries overran, so
a later Workspace on the same daemon read it all again. The RCA LOOKUP facts
window (record 032) charges its entries against the same allowance. A
Store-wide tier of whole pack bodies was prototyped (record 031, prototype b),
measured and dropped: it saved pack reads but not the per-object locate.
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
local Open. It returns the bound Workspace directly; a refusal retains the
request, captured snapshot and exact typed owner error. Attempted owner errors
retain the backing and original cause. Terminal Close is explicit; FUSE
attachment/readiness is separate.

Each bound Workspace owns its own overlay database and lock, with independent
routing and local ranges. Inode ranges come from the shared History
allocator, are consumed once and never recycled. Simultaneous writes may return
typed Busy; the adapter never makes a second attempt. Base reads use the fixed
Workspace reader while another process owns the writer.

Every producer obtains its own Storage state through Store::producer, using
the startup ReservationBlocks passed through concrete bootstrap. Only the
opened provider is shared. No mutable Save is global, no ownership spans a
whole Commit, and Init acquisition/cleanup is unavailable through these ports.
A bound Workspace keeps the Storage state of its last completed Commit and
gives it to its next one, so the bounded per-handle indexes are not read from
the Store again by every Commit; admission already allows one Commit of a
Workspace at a time. A Commit that does not complete drops that state, and the
next one obtains a new one. Each Commit still reports only its own counts.
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


## Workspace-owned readers (#320)

The owner authorized reintroduction with independent 10% time/RSS allowances.
The current branch implements the [reader lifecycle](../issues/320/WORKSPACE-READER-DESIGN-20261014.md);
The prior diagnostic sample meets those allowances; shared-machine timings
remain INELIGIBLE and final runtime qualification remains pending.

Bootstrap keeps a read-only opening port. Bind opens its reader once before the
coherent Branch snapshot and root checks. One control reader serves explicitly
unscoped construction/history work. Persistence verifies cache_size=-2048,
mmap_size=0 and the selected Store profile for every actual connection.
No filesystem request opens a reader. Per-request ports keep their original
first failure; concurrent demands in one failure scope still refuse before effect.
Different Workspaces have independent reader locks and share only the immutable
cache and opened write/publication authority.

The fixed shared pool, lanes, tickets, FIFO cursor, per-Workspace request quotas,
exclusive reader leases and admitted-client wrapper are removed. Each bounded
demand locks the owning session only while the synchronous provider call runs.
Its transaction ends before the result returns or any async step is polled.

BoundWorkspace::close performs the overlay Close then closes its healthy Store
reader. Normal/forced control teardown uses this typed operation after request
drain. A failed Close boundary preserves its original Owner or Reader error;
no following boundary runs and no failed operation is replayed. Uncertain or
poisoned readers stay retained outside service, with the same Arc cause visible
through reader_failures. An uncertain opening retains its exact error and
registration; no guessed new open is permitted for that identity. Whole-Store
stop closes healthy sessions and blocks new demand while keeping unknown custody.

Concrete reader SQL observations use weak providers, so instrumentation never
keeps a connection open after unmount. Closed readers add their final counters
to one fixed retired summary, emitted at reader index -2. An unavailable final
counter invalidates that summary rather than substituting zero. Storage counters
for a removed reader are unavailable. The diagnostic schema explicitly marks
current demand/lock fields and keeps historical scheduler arrays readable.

The [R2 checkpoint receipts](../issues/307/checks/r2-store-read-service-20261008/45-results.md)
remain historical evidence of the former pool, not a qualification of this
candidate. Mounted semantics, failure custody and bounded streaming remain on
the separate Phase 2 keep list. No cold/RSS or speed claim follows from this cut.

## Write-recognizer slots

Added 2026-10-11 for [#316](https://github.com/Ephemeral-AI-Lab/layerfs/issues/316),
phase 2. The Store owns the
[fixed table](../../crates/layerfs-daemon/src/store/recognizer.rs) of the
write recognizer: 64 slots, shared by every Workspace of the daemon, keyed by
Workspace route and inode serial. A slot is the fixed-size state of one
file's writes and holds no committed byte. It may hold one carried chunk in
each direction, allocated only while a chunk is carried: a hash state of
about 2 KiB, and for a writer in falling offsets the last bytes of one
chunk, at most 32 KiB of the earlier write. The allowance is the table plus
both for every slot, about 2.2 MiB for the daemon, reported as
`capacity_bytes`; no cache bound changed. `Recognizer::published` records
the revision of an applied write for the chunk that write carried. A request takes its file's state out of the table for
the length of a pass and puts it back, so the table's lock is never held
while bytes are hashed or a page is read. A full table drops the least
recently used file, and a file without state starts over: the cost is a
missed claim. `StorePorts::fresh` gives a pass its own failure scope.
`Store::recognizer_work` reports occupancy, evictions and cumulative pass
work; the diagnostic record format is unchanged.

Reader-work observations use atomic request/wait counters, the write-once original failure and the mutex poison bit; they do not wait for an in-flight provider demand. Exact failure inspection uses that same original Arc without borrowing the provider lock. Mounted provider-hold proofs exposed and regress this boundary: observing work must not cause the bounded provider hold to expire.
