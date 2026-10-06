# Backed initial root acquisition

> **Status:** Current general guide. S9/P12 checkpoint; S9 remains open and no speed, RSS or cold-cache claim is made.

Project/import no longer retains scan, job, frontier, child, inode, serial or
directory-update collections proportional to the acquired root. Every input-sized
row lives in operation-owned scratch and is streamed into the existing public
canonical constructors: `build_directory`, `empty_directory` and `build_table`.
Initial acquisition does not pass through the resident whole-namespace
`build_filesystem` update. Public `init`, `InitRequest`, `Initialized`, crate
boundaries, formats and Store policy are unchanged. `NamespaceWork` gains
`sort_capacity_bytes` and `backing_bytes`; its three former collection-capacity
fields are now always zero.

## Selected backing arrangement

The scratch uses Content's existing public `OrderingBacking`/`OrderingRun`
capability with its `FileBacking`, the same interface and backend Project already
passed to the whole-namespace constructor. Project is a domain crate: the product
boundary guard refuses an engine dependency there, and the SQLite rule's scope is
cluster-two mutable state, so no SQLite, new dependency, backing port in the public
API or host-supplied adapter was introduced. A predecessor SQLite scratch attempt
was withdrawn when the guard refused it; its receipts remain under the checks
directory as superseded evidence. Runs are append-only files in one private
`import-<pid>-<n>` directory (mode 0700) under `scratch_parent`. The backing account
charges every byte before it is written and reports the simultaneous peak. Capacity
is the physical device; there is no total-input byte cap. Nothing is synchronized.

**Placement.** `scratch_parent` must lie outside the source. Before the scratch
directory is created, the parent is resolved through its links and each ancestor's
native `(device, inode)` is compared with the source root's. A parent that is the
source or inside it, under any spelling, is refused as
`ProjectError::ScratchInsideSource` with the source untouched: a scratch inside the
source would change the source directory and be acquired as part of it. The walk
is bounded by path depth and reads no source content. A second mount of a source
subtree at an unrelated path is not recognised by an ancestor walk and is outside
this check.

**Account.** The backing lists each run path with the bytes that reached it. An
append the host accepts only in part keeps the written bytes charged to that path,
returns only the unwritten remainder of its reservation, and closes the run to
further rows; the run's logical length does not include the partial record. A path
leaves the list, and returns exactly its listed bytes, only when its file is gone.

`import/runs.rs` owns the mechanics: a buffered `Writer`, a typed sequential
`Cursor` and a `Sorter`. `import/scratch.rs` owns the directory lifecycle and the
big-endian record grammar. A run is written once and then read; it is never read
while it is being written. No run is an index, B-tree or page store: ordering is
obtained only by external merge.

## Flow and order

Order is unchanged: breadth first, each directory's children by name bytes,
position equal to breadth-first index, serial equal to reservation start plus
position, and later paths of one native regular `(device, inode)` bound to its
first position. The reservation still consumes every path; alias serials stay
unused and are never recycled.

1. **Scan.** One frontier run per depth is read to its end while the next depth is
   written. One directory stream and one 512-child buffer are resident. A wider
   directory is ordered by the sorter. Each child appends one entry record; each
   regular file also enters the identity sorter keyed by device, inode, position.
2. **Identities.** The identity-ordered stream is grouped. First paths become the
   job run; later paths must carry equal evidence and enter a position-ordered alias
   run and a first-position-ordered count run.
3. **Files.** The owning thread feeds a 512-slot queue from the job run to the four
   Namespace Init constructors and alone accepts objects into the Save. A
   constructor hands over its pending batch before it waits. Completed roots are
   ordered by position in backing. Descriptor checks before and after reading and
   path checks for every first and later path are unchanged.
4. **Prerequisites.** Entries merged with aliases yield metadata and symlink-target
   roots, appended in position order.
5. **Tree.** Entries are already grouped by parent in name order, so each group
   streams into `build_directory` and its root is appended by parent position.
   Directories that bound nothing share one `empty_directory` root. The inode table
   merges, by position, the entry kinds with the alias, count, metadata, target,
   file and directory streams into `build_table`, then emits the filesystem root.

Scratch release and directory removal are checked before the tree Save is finished
or history is published; a cleanup failure is `ProjectError::Cleanup` carrying the
original construction cause, the deciding cleanup failure and a `RetainedScratch`
naming the directory, run count and run bytes still on disk. The deciding failure
is the first step that did not complete: a refused run removal is reported with
its own host error, and the directory removal that could only fail after it is not
attempted. One attempt only: no retry, refresh or replay. A checked release that
failed is final; neither the backing's nor a run's destructor attempts it again,
and the retained paths stay charged in the account for the caller to dispose of.

## Work and resident state

For E entries, N regular paths, U identities and a widest directory of W children:
scan and every stream pass are O(E); ordering is O(N log N), O(U log U) and
O(W log W) record moves through merges of fan-in 16 over 4096-record or 1 MiB
chunks. All scratch I/O is sequential. A merge holds its inputs and its output
together until the inputs are dropped.

Resident state is fixed 64 KiB read and write buffers, the 512-slot job queue, the
child buffer, one sort chunk per active sorter, and at most 15 run handles per merge
level, a count that grows with the logarithm of the stream. Owned names, targets
and paths of rows in flight, the Save's own state and allocator or page-cache
behaviour are outside these counters. `NamespaceWork` is count-driven diagnostics,
not a whole-importer memory bound.

## Evidence and limits

A public test rebuilds the published tree through `build_filesystem` and requires
the identical root for a root with a 1302-child directory, nested and empty
directories, a dangling symlink and three paths of one native file. Another
orders 17 000 symlinks with long opaque targets through a 16-way merge and a
final merge.
A refused set-id source leaves no scratch and publishes no history. Existing
complete-root, memory and both macOS Store profile tests are unchanged and pass.
Placement refusal, partial-append charging, the final failed release and the
reported cleanup cause have public tests; their receipts and limits are under
[custody checks](../issues/307/checks/s9-acquisition-custody/identity.json).
The Store oracle is gated to macOS, and other platforms prove the explicit
`BackendUnavailable` refusal instead.
Append-only receipts, the retained Linux failure of the ungated macOS-only
`init_sqlite` test and source identity are under
[checks](../issues/307/checks/s9-backed-acquisition/identity.json).

Init time and storage effect are NOT_RUN; no frozen measurement contract covers
this change. Greater-than-4 GiB native proof remains NOT_RUN. Application or daemon
assembly, context, restart custody and S9 qualification remain open, and
P3/P6/P7/P13/P14 remain later Commit prerequisites.
