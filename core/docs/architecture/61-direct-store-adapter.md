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
