# Captured namespace construction

> **Status:** Current general guide.

This record describes the implemented R4 composition: how one exact Capture of
a Workspace becomes one complete canonical filesystem root. It builds on the
[streamed directory input](51-streamed-directory-input.md), the
[backed serial state](53-backed-filesystem-serial-state.md) and
[captured file normalization](56-captured-file-normalization.md). No product
code calls the producer yet: the Commit driver is unchanged and is driven by
the producer only in a daemon test. Mounted Commit is R5. Scope, receipts,
failed attempts and open decisions are in the
[R4 completion record](../issues/307/R4-COMPLETION-20261008.md).

## Three parts

| Part | Crate | What it is |
| --- | --- | --- |
| Reader jobs | Overlay, Daemon | Parent-local captured names, exact name points and symlink targets of one retained reader |
| Validation | Content | One validator for every route whose topology work follows the change |
| Producer | Workspace | `CapturedNamespace`: normalizes the capture into sealed records and runs Content's streamed backed update |

There is one encoder and one validator. The producer builds canonical bytes
only through Content's constructors (attribute trees, symlink objects, inode
values, the streamed backed update) and the existing changed-file constructor.

## Reader jobs

[namespace/captured_namespace.rs](../../crates/layerfs-overlay/src/namespace/captured_namespace.rs)
adds three reads bound to a `CapturedReader`. None reads an active row.

| Job | Reads | Statement and plan |
| --- | --- | --- |
| `reader_parent_directory_entries` | At most 64 rows of one parent after a binary name, whiteouts included | `SOURCE_NAMES`; seek on `directory_entry_capture (ns, gen, parent, name>)` |
| `reader_directory_entry` | The exact sealed row of one name, or none | `CAPTURED_DIRECTORY_ENTRY`; primary key `(ns, parent, name, gen)` |
| `reader_symlink` | The local target of a live symlink | The existing layered captured read over `(installed, captured]` |

Both name jobs read `gen = capture.generation` exactly, like the existing
captured pages. `reader_symlink` answers `Missing` for no local row, a removed
inode, another kind, or a target the immutable base still supplies; it never
falls back to the base. The Workspace port
[`OverlayCapturedNamespace`](../../crates/layerfs-workspace/src/ports/captured_namespace.rs)
exposes them as `captured_directory_entries`, `captured_directory_entry` and
`captured_symlink`; the Daemon serves them as Read-class commands
`ReaderParentDirectoryEntries`, `ReaderDirectoryEntry` and `ReaderSymlink`.

The three commands declare their reply charges: 64 rows of at most 255 name
bytes for the parent page, one row for the point, and one cell with its mask
and read header for the symlink. `Overlay::explain_reader_directory_entries`
returns the query plans of the two name statements for a given reader, beside
the existing `explain_*` diagnostics.

Inode pages and inode points use different predicates (`gen = captured` and
`(installed, captured]`). They agree because a failed capture's rows are folded
into the active generation before the next capture; a test proves both
directions, including across that fold.

## Validation

[validate.rs](../../crates/layerfs-content/src/filesystem/validate.rs) and its
submodules replace the whole-base alias walk and the per-binding subtree walks.
The base is a valid tree by induction: every root this code publishes passed
this validation.

**Aliases.** A directory or symlink has exactly one binding. The reducer derives
every final count from the base count and the operation's own additions and
removals, and refuses a non-file count above one with `multiple parents`
([references/meaning.rs](../../crates/layerfs-content/src/filesystem/references/meaning.rs)).
No base page is read for this. The refusal arrives after rebuilt directory
pages were offered to the consumer; the filesystem root never is.

**Classification.** Each binding of a directory is either the base's own
(restated) or a *placement*. One window costs one grouped inode demand, one
grouped base name lookup per stored parent and one guarded record batch
([validate/incremental.rs](../../crates/layerfs-content/src/filesystem/validate/incremental.rs)).
Rows are classified 64 at a time. On the backed streamed route each window
demands its own base records, so nothing wider than one window is resident; on
the resident routes the base records of the whole input are demanded in one
grouped read first, which keeps their pinned wave count, and
`peak_window_rows` reports the whole input. A second placement of one directory
is refused with `multiple parents` before anything is offered.

**In-place renames.** A stored directory renamed inside its own base parent did
not move. It is recognized from that parent's own change rows, one scan per
parent ([validate/in_place.rs](../../crates/layerfs-content/src/filesystem/validate/in_place.rs)):
every changed name is resolved in the parent's base listing, one grouped lookup
per 64 names, and a base binding is *displaced* when the row removes the name
or binds it to another inode. A displaced stored directory placed under that
same parent is marked in place and is then treated as unmoved. This covers a
plain rename, `mv d d.old; mkdir d`, a rotation and a swap of two names. A name
the row restates is not displaced, so a directory that keeps its base name and
gains a second one is still refused by its derived count. The pass runs only
when the territory gate below is open, and it then scans the root's change rows
too when the root holds a stored directory's placement.

**Effective tree.** [validate/cycles.rs](../../crates/layerfs-content/src/filesystem/validate/cycles.rs)
carries the soundness argument. A *territory* walk lists a moved stored
directory's surviving subtree, and it runs only when a stored directory moved
out of place and some placement lands under a stored directory, other than the
root, that itself did not move. A
step-bounded upward walk then proves that every placed directory reaches the
root or an untouched base position; a walk longer than twice the placements
plus two, or one that ends at an allocated directory nothing binds, is refused
with `effective tree cycle` before anything is offered.

Topology evidence lives in the operation's indexed records on the backed route
([validate/backed.rs](../../crates/layerfs-content/src/filesystem/validate/backed.rs)),
in key windows of at most 64:

| Kind | Key to value |
| --- | --- |
| `0x4653_0020` placed | directory to its stated parent and flags |
| `0x4653_0021` territory | stored directory to the moved directory that owns it |
| `0x4653_0022` rooted | directory an earlier walk already proved |
| `0x4653_0023` queue | ordinal to a directory awaiting its listing |
| `0x4653_0024` scanned | parent whose changed names were already resolved |

`validate::check` lost its unused dropped-parent argument and `CheckedInput` no
longer carries `additions`. The backed route refuses no total derived from
`ordering_bytes`. Resident
routes bound each topology container by the existing resident ordering budget.
The label `cycle check work limit` no longer exists.

What still costs a base walk: a directory moved across parents lists its
surviving subtree once whenever the gate is open, because neither the canonical
format nor the Capture records a parent pointer. The gate is one decision for
the whole operation: an unrelated placement under a stored directory other than
the root opens it for every moved stored directory of the same operation, also
one whose own destination is the root. Narrowing the gate to each moved
directory's own upward chain is not implemented.

## Producer

[`CapturedNamespace`](../../crates/layerfs-workspace/src/construction/driver.rs)
takes a Workspace, a provider, a `CapturedReader` and an `OperationOwner`.
`new` performs no I/O. `construct` consumes the value and makes one attempt:

1. **Bind.** The immutable base is rebound to the reader's root, never the
   Workspace's possibly later binding. A guarded context record refuses a
   second attempt in the same operation scope.
2. **Names.** One pass over the captured names writes one header per changed
   parent with its exact change count. A parent created and removed inside the
   capture is dropped with its names; a removed base directory keeps its
   whiteouts.
3. **Inodes.** One pass over the captured inodes in serial order writes a typed
   value for every live inode and a dense rank for every fresh one. A non-root
   inode with no remaining link is skipped: nothing is declared, read or
   constructed for it. Base records come from one grouped lookup per page.
   - File content: the existing `CapturedFileEdits` under its own record scope
     (`file_scope = serial`), one file at a time. It returns the base root when
     the content is unchanged. A fresh file has no base. It takes
     `CapturedFileEdits::construct_new`: the same binding, which must find no
     base file, then Content's run construction straight over the captured
     runs. No normalized-edit or context record is written, because its one
     edit, an insert of the whole file, is never read (issue #352 record 033 F2).
   - Fresh symlink: `captured_symlink`, then Content's symlink object. A stored
     symlink keeps its base content root.
   - Metadata: mode and mtime in Content's portable grammar, under the keys
     Content exports as `PORTABLE_KEYS`; a stored tree is patched so every
     other key keeps its value root, a fresh one is built.

   Both passes apply the records of one captured page before they request the
   next page, so one page and its records are resident at a time.
4. **Seal.** The context record is replaced by one that binds the reader and
   the three declared totals.
5. **Update.** `update_filesystem_streamed_backed` runs over the sealed rows.
   Names are served from the reader's parent-local pages and name points;
   headers, values and fresh ranks from the records, read as windows of up
   to 64 (key, value) rows in key order, one record job each
   (`operation_record_entries_after`, issue #352 record 033; before it, one
   job per row and a 1,024-slot residue memo per kind). A window answers
   every serial in its key range, absence included. A pass reads each sealed
   record once. A point outside the held windows reads the window that starts
   at that serial. The cursor holds a ring of 16 windows per kind, at most
   1,024 answers, whatever the size of the change. A window stops early at one
   record envelope of values and never refuses. Content's reference passes ask
   whether the serials of one row job (at most 21) are fresh in one call
   (`StreamedRowSource::are_new`); the cursor answers them from the same windows.

Producer records share the operation's file scope 0 with Content's filesystem
records: `0x434E_0000` context, `0x434E_0001` header, `0x434E_0002` value,
`0x434E_0003` fresh rank. A captured sequence ends on a page shorter than 64
rows. `FilesystemResources` are Content's defaults; no Store-derived source
exists.

### Whole-file digests and object reuse (#364)

Owner decision 2026-10-19 (#364 item 2): reuse the object only. A fresh
file whose write-path digest and length match a saved file root reuses
that object instead of being read and chunked again; it keeps its own
serial, row and metadata, and design 8 is unchanged.

- **Write path (Overlay).** A fresh regular file is followed with a BLAKE3
  state while every byte it gains is an exact append; any other write, a
  cell change, a size other than the followed length, a second link or a
  non-file ends it for good. The state lives in the journaled open state,
  never the bytes. With no descriptor open, the digest is written to
  `fs_digest` pinned to the row's content sequence. A job that changes no
  byte (a mode or time change at the same size) moves the pin; any other
  row write leaves it behind. One slot resumes the last file finished
  without a descriptor (chunked SDK writes), and an empty file still the
  row it was created as resumes from no bytes.
  [payload/digest.rs](../../crates/layerfs-overlay/src/payload/digest.rs).
- **Store.** `file_digest(digest, length, root)`, keyed by digest and
  length; recorded content takes the newer root. Older Stores gain the empty
  table at open, so they match from their second Commit of the same bytes.
  Init records none. Rows exist only for roots built from all their bytes.
- **Inodes pass.** Per captured inode page, one overlay job reads the
  fresh files' digests (`captured_digests`; a file open at that moment has
  none) and one Store read per 64 of them returns any saved root with the
  same digest and length (`FileDigests::file_roots`). A file with such a
  root takes it as its content root; its metadata is built from its own
  row as for any fresh file. Every other followed file is built in full,
  and its `(root, length, digest)` is recorded, 64 rows per write, before
  the publication lane. Names play no part: a copy at any path matches.

The reused root is the one a fresh build of the same bytes under the
Store's policy gives. Rows are written before the Save finishes, so a
Commit that fails can leave a row whose root was never saved: the lookup
answers only roots in `object_location`, so such a file takes the full
path and its row takes the new root. A root saved earlier in the same,
unfinished Commit is not reused either.

### Rename predecessors (#352 F-063)

Owner approval 2026-10-20. git (`index.lock` → `index`), editors and
package managers save by writing a new file and renaming it over the old one.
The result is a fresh inode, so it has no base file of its own and its chunks
were stored FULL. The replaced file is now offered as their delta base.

- **Write path (Overlay, schema 39).** A rename whose moved and replaced
  inodes are both regular files carries `Changes::predecessor`. Its job
  writes `fs_predecessor(inode, predecessor)` for the moved serial. When the
  replaced file was itself created since the installed base, its own
  predecessor is inherited, so a chain of renames before one Commit ends at
  the file the base holds. A replaced base file is the predecessor itself.
  The replaced serial's row is removed either way, and a file is never its
  own predecessor. It is one statement pair in the rename's job, with no
  Store I/O.
  [namespace/predecessor.rs](../../crates/layerfs-overlay/src/namespace/predecessor.rs).
- **Inodes pass.** Per captured inode page, one overlay job reads the
  predecessors of the fresh live files (`captured_predecessors`; every size
  since #88, F-063 asked only at or above the chunking cutoff). `CapturedFileEdits::construct_new` binds the
  base's regular file of that serial and calls Content
  `construct_runs_with_predecessor`. The base root is read at the first
  chunk, then its mapping pages as the edit route's `Aligner` walks forward,
  never a payload. A serial the base does not hold, or holds as another kind,
  offers nothing; any other base failure is the Commit's own.

- **Same path (#88).** A fresh file with no recorded predecessor is offered
  the regular file the base held at its path. The names pass asks the base
  (`BaseView::child`) for each captured name bound over a binding the lower
  view held (`inherited`), at most 65,536 per attempt, and keeps the
  (serial, base serial) pairs sorted for the values pass. A name the base
  does not bind, or binds to another kind or to the same serial, offers
  nothing; any other base failure is the attempt's own.
- **Whole-file results (#88).** Below the chunking cutoff the base root is
  declared whole as the object's explicit advisory predecessor, without a
  read; Storage judges its role and depth as for a whole-file edit.

The hint is advisory: the constructed root is the one a plain fresh build
gives. Storage still makes one PREFIX trial per chunk, under the chunk depth
cap and chain budgets, and keeps it only when the record is strictly smaller.

### Custody

`construct` returns `CapturedNamespaceAttempt { result, custody }` and releases
nothing. `CapturedNamespaceCustody` holds the reader, the operation owner, the
record scope's custody, a failing file's whole custody and the first original
failure; exactly one of `failure`, `file` and `records.failure` holds it, and
no provider call follows it. The caller drops custody, then releases the
reader, then the operation owner.

On failure the Content `result` is only a label (for example
`ProviderFailure { what: "captured namespace original refusal" }`). The
unchanged Commit driver classifies that label as an unknown outcome. A caller
that wants a definite refusal settled must hand the driver the original cause
from custody; the product constructor does this
([product Commit](79-product-commit.md)).

## Counted work

Counters: `ValidationWork` gained `placements`, `ancestry_steps`,
`territory_directories`, `territory_entries`, `peak_window_rows`,
`in_place_scans`, `in_place_rows` and `in_place_directories`;
`inode_pages_by_site.aliases` and `.reachability` are always zero.
[`CapturedNamespaceWork`](../../crates/layerfs-workspace/src/construction/outcome.rs)
counts the producer's rows, pages, points, record jobs and file work.

Observed through the Commit driver for one fixed twelve-step change over bases
of 310, 1,738 and 13,162 entries (host, counted, not timed; receipt
`I-attempt5-captured_commit.txt`). The change includes a directory move, an
in-place rename, and a write, a removal and a creation inside the part of the
tree that grows:

| Measure | 310 | 1,738 | 13,162 |
| --- | --- | --- | --- |
| Producer rows, pages, points, file work | identical | identical | identical |
| Placements, ancestry steps, territory, in-place scan | identical | identical | identical |
| Validation read waves | 15 | 15 | 20 |
| Validation inode pages read | 12 | 12 | 18 |
| Owner jobs | 538 | 566 | 538 |
| SQL statements (no full scan) | 2,286 | 2,370 | 2,286 |
| Canonical-client misses during the Commit | 6 | 6 | 7 |
| Sorted-merge inode pages read | 8 | 36 | 206 |
| Store pack bytes read | 55 KiB | 174 KiB | 1.1 MiB |

The job and statement differences are the producer's record point jobs, which
depend on which serials share a slot of its answer window. Validation's waves
and pages follow the height of the inode table. The sorted-merge pages and the
pack bytes grow because Content's sorted merge reads the sibling pages of every
inode branch the change touches, at most 127 per level; that is the existing
merge, bounded per touched branch, and it is not flat over these sizes. The
test asserts only the rows marked identical and the job and statement
arithmetic.

Above one 64-row window each captured row costs about 19 producer record jobs,
and about 35 record point reads in total with Content's backed state. Each is
one owner job. This is linear and window-bounded; it is the main cost to
address before a large Commit is timed.

Since record 033 the producer's sealed rows cost about one record job per 64
rows a pass reads, plus one per point that leaves the held windows. In the
host count with 1,600 files in one directory, record jobs went from 21,434 to
500. Content's backed state reads are unchanged by it.

## Not part of this record

No timing, cold-cache, storage or resident-memory claim. At R4 there was no
product caller of the producer; R5 added one without changing the producer
([product Commit](79-product-commit.md)). No change to the Commit driver, and no Durable
execution (`NOT_RUN — disabled by owner until explicit reauthorization`).
