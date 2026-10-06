# Backed initial root acquisition

> **Status:** Current general guide. S9 acquisition checkpoint; S9 remains open.
> Component measurements have their own pinned identities and limits.

Project/import retains no scan, job, frontier, child, inode, serial or
directory-update collection proportional to the acquired root. Every input-sized
row lives behind the [acquisition port](44-acquisition-backing.md) as one
operation's working rows and is streamed through bounded windows into the existing
public canonical constructors: `build_directory`, `empty_directory` and
`build_table`. Initial acquisition does not pass through the resident
whole-namespace `build_filesystem` update.

This replaces the ordering-run scratch files the import used through the previous
checkpoint. `import/runs.rs` and `import/scratch.rs` are removed; there is one
acquisition algorithm. `InitRequest::scratch_parent` is replaced by
`InitRequest::acquisition`, a `&dyn Acquisition` the host supplies. Project has no
engine dependency: it depends on the backend-neutral port in Storage, and the
SQLite provider is composed by the host from Persistence. Canonical formats, Store
policy, order, serials and the published root are unchanged.

## Backing and placement

The host passes the Store's own provider (`Handles::acquisition`) or any other
implementation of the port. The provider requires a Store created with the
acquisition tables; a Store without them answers every unit with
`BackendUnavailable`, which Init returns as
`ProjectError::Acquisition`. There is no second algorithm to fall back to.

Where the provider reports a native placement (the directory holding the Store's
database files), that directory must lie outside the source. Before `begin`, the
placement is resolved through its links and each ancestor's native
`(device, inode)` is compared with the source root's. A placement that is the
source or inside it, under any spelling, is refused as
`ProjectError::BackingInsideSource` with the source untouched and no operation
begun: such a backing would change while it is read and be acquired as part of the
root. The walk is bounded by path depth and reads no source content. A second mount
of a source subtree at an unrelated path is not recognised by an ancestor walk and
is outside this check.

`import/backing.rs` is the only module that calls the port. It holds one read
window per stream, one write window, the native-evidence conversion and the unit
counters. Each port call is one attempt.

## Flow and order

Order is unchanged: breadth first, each directory's children by name bytes,
position equal to breadth-first index, serial equal to reservation start plus
position, and later paths of one native regular `(device, inode)` bound to its
first position. The reservation still consumes every path; alias serials stay
unused and are never recycled.

1. **Check and begin.** The source must be one real directory; placement is
   checked; `begin` records the source identity, stack and scope.
2. **Scan with attributes.** One prerequisite Save is open for the scan. The
   directory stream is read in position order; a window is fetched only after the
   write window is flushed, so every placed directory is visible. One 512-child
   buffer is resident. A directory that fits is ordered in memory and written with
   final positions. A wider directory is written unpositioned, then positioned by
   name-ordered read windows and bounded write windows. Each child's attribute
   root, and a symlink's target root, is constructed as its row is built, so rows
   are written complete and no separate attribute pass over every entry exists.
   Adjacent equal portable fields reuse one attribute root. A positioned regular
   file is bound to its native identity in the same unit; a later path whose
   evidence differs from the first is `InvalidInput`. The prerequisite Save is
   finished after the scan.
3. **Files.** The owning thread feeds the 512-slot queue from the identity stream
   to the four Namespace Init constructors and alone accepts objects into the
   Save. A constructor hands over its pending batch before it waits. Completed
   roots are recorded against their identities in write windows. Descriptor checks
   before and after reading and the path check of every first path are unchanged.
   When the scan bound at least one later path, entries are streamed once and
   each later path is rechecked against its identity's evidence; later paths of
   one directory share one path read. A root with no later path makes no such pass.
4. **Tree.** After `reserve_inodes`, entries stream by key: each parent's group
   goes into `build_directory` and its root is recorded in write windows.
   Directories that bound nothing share one `empty_directory` root. A second
   entry stream, merged by position with the identity stream of constructed roots
   and later-path counts, feeds `build_table`, then the filesystem root is
   emitted. The entry stream is by key; the consumer counts rows and requires each
   position to be the next one, and the totals to match the scan.
5. **Cleanup before publication.** With the tree Save still unfinished, working
   rows are removed in budgeted jobs until none remain, then the operation record
   is released. Only then is the tree Save finished and the stack initialized, so
   every backing failure precedes anything final and a successful Init leaves no
   row and no record.

## Failure and custody

One attempt only: no retry, refresh or replay.

- **Definite failure during steps 2–4.** The working rows have no consumer. Init
  records `Failed` on the operation, removes its rows, releases its record and
  returns the original error.
- **Cleanup failure.** The first cleanup unit that does not complete decides.
  Init returns `ProjectError::Cleanup` with the original construction cause when
  there was one, the unit's typed error and a `RetainedAcquisition`: the owner and,
  when one further read of the charges succeeds, the rows and bytes still held.
  Nothing after the failed unit is attempted and nothing is published.
- **Unknown outcome.** When a Storage, history or backing unit reports that its
  outcome is unknown, Init attempts nothing further. It returns
  `ProjectError::Uncertain` with the cause and the owner; rows and record are
  exactly as that unit left them. With the SQLite provider the Session is
  quarantined and could execute nothing anyway.

A host that later holds a retained owner disposes of it through the port's own
units; a later provider session sees it as abandoned
([restart custody](44-acquisition-backing.md)). Init never lists or removes
another operation's rows.

Failures after cleanup (finishing the tree Save, initializing the stack) carry no
acquisition state. Objects saved by the earlier Saves may then be unreferenced,
the same bounded ownership the ordinary save path has.

## Work and resident state

For E entries, D directories, U native identities, A later paths and V children of
wide directories, one Init makes about E + V row inserts, U + A identity binds,
U file-root and D directory-root updates, and E + U row removals, in write windows
of at most 4096 rows and 1 MiB charged payload. It reads E rows twice (three times
when A > 0), U rows twice, D rows once and V rows once, in windows of at most 512
rows; windows whose rows may carry a native path are narrower in the SQLite
provider. The provider's per-statement plans and engine work are in
[acquisition backing](44-acquisition-backing.md).

Resident state is one read window per open stream, one write window, the
512-slot job queue and the child buffer. Owned names and paths of rows in flight,
the Saves' own state and allocator or page-cache behaviour are outside these
counters. `NamespaceWork` reports row, unit and window counts and the provider's
exact logical charges removed by cleanup. It is count-driven diagnostics, not a
whole-importer memory bound, and the charges are not pages, file growth or RSS.

The backing is the selected Store. Under a Durable Store every write unit is a
synchronized commit and every row is journaled; that cost did not exist with
unsynchronized scratch files. The [first component measurement](../issues/307/NAMESPACE-INIT-ACQUISITION-RESULTS-20261006.md)
records complete Init and inclusive COMMIT observations, with no physical page/
synchronization-call attribution. The [window execution correction](44-acquisition-backing.md#window-execution-correction-2026-10-06)
changes provider execution only; Project's flow, constructors and public port
contract stay unchanged.

## Evidence and limits

Public tests, on the memory ports and on both macOS Store profiles:

- The provider-backed root equals the memory-port root for the same source, and
  both pass the full namespace oracle.
- A root with a 1302-child directory, nested and empty directories, a dangling
  symlink and three paths of one native file is rebuilt through
  `build_filesystem` to the identical root, and again through 37-row read windows.
- A 17 000-child directory is positioned through many windows within the port's
  row and byte maxima.
- Placement inside the source, a Store without acquisition tables, a failed
  cleanup with and without a prior failure, an unknown outcome and a definite
  backing failure each have a test of the typed result and of what remains.

Receipts, retained failures and source identity are under
[A3 checks](../issues/307/checks/s9-acquisition-port/identity.json). Earlier
run-backed receipts remain under their own directories as superseded evidence.

The first100/1000 component Init speed/storage selection at45e2b09e8 is retained
with three speed failures and all larger tiers NOT_RUN. Revised-source acceptance
is separate. Physical pages, peak journal, synchronization-call cost and phase
residency remain unqualified. Greater-than-4 GiB native proof remains NOT_RUN. Capacity
failure and a real quarantined Session are not exercised through Init. Application
or daemon assembly, context, restart custody and S9 qualification remain open.
