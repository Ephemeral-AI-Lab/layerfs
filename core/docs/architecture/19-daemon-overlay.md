# Daemon overlay: initial indexed engine

> **Status:** Current general guide.

Implemented source: the S0/S1 checkpoint introducing `layerfs-overlay` on local
main for [#307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307).
Complete Workspace/FUSE/daemon and milestone qualification remain outstanding;
the [design](../issues/303/README.md) and
[progress](../issues/307/PROGRESS.md) retain the broader target and unfinished gates.

The active [crate](../../crates/layerfs-overlay/src/lib.rs) owns one SQLite
connection and SQL/payload/scratch/custody. It creates fresh disposable state
once; `open_workspace` inserts only a root/incarnation/routing row, without
another connection, schema or immutable-root walk/materialization. Root authority
and filesystem semantic checks remain the Workspace/runtime caller's obligation.

The [profile](../../crates/layerfs-overlay/src/profile.rs) reads back MEMORY
journal, OFF synchronization, EXCLUSIVE locking, zero mmap/busy timeout, foreign
keys, 4096-byte pages, configured pager/max-page settings, SQLite version and
compile options. Pager size is a suggestion, not a resident ceiling. Temporary
work selects FILE; ordinary indexed statements need no temporary sort. macOS
retains system SQLite; Linux alone enables the existing driver's bundled feature.
Global Store persistence is unchanged. Disposable state has no sync/checkpoint,
crash-survival or restart-recovery claim.

The [schema](../../crates/layerfs-overlay/sql/schema.sql) has eight application
tables plus SQLite's AUTOINCREMENT allocator. Namespace IDs are not reused.
Every job constrains namespace and checks incarnation against its routing row.
Inode/name keys support both current lookup and generation-selective capture;
cell keys include serial, generation and aligned offset. Metadata queries exclude
payload. Names, data and validity are BLOBs. Scratch/owners are independently keyed
backed state, without resident membership collections.

The following S3 checkpoint advances the disposable schema to version 2: portable
mtime uses separate signed seconds and nanoseconds; mode validation preserves
directory sticky bits, restricts regular files and fixes symlink mode. Startup
reads back schema/application identity. The initial version-1 receipts below keep
their source identity; new profiles accompany the version-2 statements.

The S2 checkpoint advances the disposable schema to version 3 with an installed
generation floor and fixed metadata install/retirement enqueue. Current lookups
exclude retired rows; later active rows/replies survive. Physical cleanup and exact
reader/orphan eligibility are still unfinished. New files are created mode 0600.

`publish` is a primitive for one changed inode, optional final name and optional
4096-byte cell/512-byte mask. It is not S4 compound namespace semantics or S5
byte-write normalization. Its transaction resolves active generation, changes
values/counters and records a publication ticket. A later reply-send attempt
removes only that ticket, including when delivery is lost; published bytes remain.
`capture` refuses outstanding send-attempt custody, then advances fixed metadata
fields without copying inode/name/payload populations. Resolution/install and
automatic reclamation are not yet implemented.

`captured_inodes` uses fixed `(ns,gen,serial)` membership, keyset pagination and
64-row windows, explicitly selecting `inode_capture`. Initial macOS EXPLAIN chose
the primary `(ns,serial,gen)` key and residual-filtered generation: one returned
captured row cost 540/4124/16412 actual VM steps with 128/1024/4096 later active
rows. The [failure](../issues/307/checks/s1-initial/residual-plan-failure.log) and
[corrected profiles](../issues/307/checks/s1-initial/repaired-engine.log) are retained.
An indexed SEARCH/fullscan counter of zero alone was insufficient evidence.

The [profiler](../../crates/layerfs-overlay/src/metrics.rs) observes actual step
executions, VM/fullscan/sort/autoindex/reprepare counters, returned/attempted changed
rows, bound bytes and inclusive statement wall. It resets cached counters before
each invocation and aggregates in thirteen fixed slots. Exclusive owner-job
snapshots provide attribution; shared lifetime counters are not phase/per-Workspace
resource peaks. Bootstrap PRAGMA/schema work is outside job counters. B-tree
internal page visits, journal/dirty/device bytes, copy allocation and process/kernel
residency remain unavailable. `pages()` reports shared page/freelist observations.

Point access costs O(log N) B-tree work. A capture page returns K<=64 rows with
O(log N+K) index visits plus noncovering fetches, which can add O(K log N).
Fixed-cell replacement binds 4608 data/mask bytes plus keys per call and incurs
SQL/index/page/journal work independent of older fragment count. First-touch and
tiny-file amplification require S5/S7 qualification. Scratch paging holds at most
64 records of at most 65536 bytes; total records continue through windows. Capture
has fixed metadata SQL work, but daemon queue/reply drain is not a constant wall
claim. No speed/cache/residency claim follows from these count diagnostics.

Missing acceptance: fair queued service, retained reads across install, complete
metadata operations, cell transitions/truncate/holes, orphan/failure composition,
physical headroom, automatic live/idle reclaim, authenticated runtime, mounted
cache/mmap/coherence/Exec, canonical construction and integrated qualification.
No S0–S13 completion follows from this initial checkpoint.
