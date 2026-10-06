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
3. **Reserve and directories.** After prerequisite publication, C5 consumes one
   serial reservation for every path. A later file failure cannot recycle it.
   One assembly Save then accepts directory roots built from narrow bindings;
   those roots are recorded in bounded working-row windows. Childless directories
   use the shared canonical empty page.
4. **Stream files and inodes.** Four Namespace Init constructors receive jobs
   from the indexed identity stream. `Job` includes the alias count fixed by the
   completed scan. At most512 identities are admitted across queued, running and
   completed-but-unconsumed states. Only canonical inode consumption releases a
   slot. While admission is full, the owner continues draining bounded object
   frames into the same assembly Save. Each completion follows its file's objects
   in that producer's stream and is exposed only after those objects are accepted.
   The inode iterator pulls the next canonical file's root and alias count directly
   from the bounded slots; Init writes no file-root completion and makes no
   file-root read pass. Aliases share the first position and are skipped as inode
   rows. Descriptor/path checks remain, and later paths are rechecked after all
   constructors have drained. Only then is the filesystem root accepted as the
   Save's final object. Fresh sorted constructors consume already constructed
   dependency IDs without rereading unpublished children. Accepted objects may
   enter ordinary reference-closed incremental publications; acceptance itself
   is not durable completion.
5. **Cleanup before publication.** With the tree Save still unfinished, working
   rows are removed in budgeted disposal jobs until none remain. SQLite removes
   the empty operation record in its final job; an adapter that reports release
   remaining is explicitly released after successful cleanup. Only then is the tree Save finished and the stack initialized, so
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
D directory-root updates, and E + U row removals, in write windows of at most4096
rows and1 MiB charged payload. It reads E bindings and E inode rows (another E
entry rows when A>0), U job rows, D directory rows and V unplaced rows through
bounded keyset windows. The SQLite provider narrows native-path windows by their
actual byte lengths. Alias validation adds indexed point evidence/path reads.
Per-statement plans and engine work are in [acquisition backing](44-acquisition-backing.md).
With N stored rows, indexed work remains O((E+U+A+D+V) log N), plus actual content
bytes, path/name bytes and canonical construction. The fixed completion map has
at most R=512 keys and O(log R) insert/update/removal; it is not a population mirror.

Resident state includes one read window per stream, a write window, the512-child
buffer and at most512 admitted file identities. A slot retains final alias count
and optional root; queued/active jobs retain their bounded paths/evidence. The
four-slot output channel and four producer frames retain the existing byte/object
limits and canonical oversized-singleton exception. Each successful Done flushes
its preceding events immediately, creating at least U received frames; extra
object-only frames serve large files. This replacement transport cost is explicit.
Skew can stall new admission behind a slow earlier file and leave workers idle;
the owner still drains the earlier file's output and memory does not grow with U.

`NamespaceWork.file_admission_rows`, `file_completed_rows` and
`file_output_batches` record high-water admission/completions and drained frames.
Other window counts exclude owned in-flight names/paths, Save state, allocator and
page-cache behavior. Logical cleanup charges are not pages, file growth or RSS.

Directory construction reads the port's narrow `Binding` projection; inode and
alias validation retain full `Entry` reads. Existing adapters can derive bounded
bindings from their entry windows without a second importer. The SQLite provider
decodes typed results directly and serves single-statement windows through an
owner-gated implicit snapshot; length/payload path windows retain one explicit
transaction. These are provider access changes, not a changed canonical format.

The backing is the selected Store. Under a Durable Store every write unit is a
synchronized commit and every row is journaled; that cost did not exist with
unsynchronized scratch files. The [first component measurement](../issues/307/NAMESPACE-INIT-ACQUISITION-RESULTS-20261006.md)
records complete Init and inclusive COMMIT observations, with no physical page/
synchronization-call attribution. The [window execution correction](44-acquisition-backing.md#window-execution-correction-2026-10-06)
retains its original source identity. The new streaming flow changes Project orchestration
and the Job projection; schema scripts, canonical constructors and persistence
profiles remain unchanged. Its candidate and evidence are separately recorded in
[the streaming selection](../issues/307/INIT-STREAMING-PLAN-20261007.md).

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
