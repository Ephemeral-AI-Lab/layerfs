# Daemon overlay: initial indexed engine

Terminology update2026-10-07: the current API/SQL names below use overlay
schema16. Earlier algorithm and proof pins retain their original scope; see
[the naming checkpoint](../issues/307/PRE-S8-TERMINOLOGY-20261007.md).

> **Status:** Current general guide.

Merge note, 2026-10-09: the three payload and accounting notes below were
written on a branch at schema 23, 24 and 25; merged after the inode,
custody-row and descriptor-row changes (schema 23, 24, 25, further down)
they are schema 26, 27 and 28, and `PRAGMA user_version` and its startup
readback are 28. `native_file` and its two accounting triggers do not exist
in the merged schema, so 61 accounting triggers remain, each writing its own
namespace's row (the two orphan triggers also the engine's `orphan_rows`).

R7 update, 2026-10-09 (payload delete trigger reads lengths, decision U4,
overlay schema 28, trigger text only). Implemented: `payload_account_delete`
runs BEFORE the delete and subtracts
`(SELECT length(data)+ifnull(length(validity),0) FROM payload WHERE rowid=OLD.rowid)`.
It names only `OLD.rowid` and `OLD.ns`, so a statement that deletes payload
rows no longer loads each row's `data` and `validity` in full for the
trigger (up to 32768 + 4096 bytes and eight overflow pages a row); the
trigger reads the two lengths from the row header. The counts stay exact
and inside the deleting statement. The instruction listing of the three
delete forms, and of the replaced trigger for contrast, is asserted in
[`reclaim_cost.rs`](../../crates/layerfs-overlay/tests/reclaim_cost.rs).
The update trigger still names old and new values.

R7 update, 2026-10-09 (accounting triggers write one row, decision C3,
overlay schema 27). Implemented: every accounting trigger updates the
`accounting` row of its own namespace only. The daemon aggregate that
`Overlay::resources(None)` returns is the sum of those rows, read by one
statement over the primary key (`WHERE ns>0`): one row per namespace the
engine still holds, live or closed and not yet reclaimed, on a path that
only reports run. No job on the request path reads the aggregate and no
admission or quota check uses it. One count stays maintained in the row of
namespace zero: `orphan_rows` of the whole engine, by the two orphan
triggers, because `orphans_deleted` reads it in one row after a job
deletes orphans to keep `orphan_seen` exact. The triggers stay inside their
statements, so what a failed statement or `SQLITE_FULL` rolls back is
unchanged. Trigger programs run as often as before; each changes one row
instead of two (a 128 KiB write into a fresh file changes 8 payload-family
rows where it changed 12). The aggregate against the stored rows at 1, 3
and 8 namespaces, live, closed, partly reclaimed and reclaimed, is in
[`accounting_reference.rs`](../../crates/layerfs-overlay/tests/accounting_reference.rs).

R7 update, 2026-10-09 (payload rows of several cells, decisions P1 to P5,
overlay schema 26). Implemented:

- **Row shape (P1).** A `payload` row is a cell row, as before (at most 4096
  bytes, optional validity mask), or a dense row of whole cells with no
  mask that lies inside one aligned slot of `RUN_BYTES` (32768, one named
  constant in `contract/types.rs`; the table CHECK repeats the number).
  Columns, indexes and triggers are unchanged; `payload_cells` counts rows.
  `PRAGMA user_version` and its readback are 26. Rows of one layer never
  share a cell, live or stale, and a row is wholly live or wholly stale.
- **One range finds every row.** A row that holds a byte of `[a, b)` starts
  in `[a - a % RUN_BYTES, b)`: `CELL_COVER` (the last row at or before a
  cell in its slot; it returns bytes only for a one-cell row),
  `CELL_SHAPES` (shapes of a slot's rows, no bytes) and `CELL_RANGE` and the
  captured metadata seek with the residual `cell_offset+length(data)>?`.
  All are index searches ([`payload/runs.rs`](../../crates/layerfs-overlay/src/payload/runs.rs)).
- **Write (P2).** Whole cells are written one slot at a time: one
  `CELL_SHAPES` read, then the bytes overwrite a live dense row in place
  (SQLite incremental blob write through rusqlite's safe handle: no
  statement, index entry, trigger or length change), or stale and smaller
  rows of the range are deleted by one range delete and one row is inserted,
  bound from the caller's slice. A neighbouring row is never extended. A
  partly covered cell is one `CELL_COVER` read and either an in-place write
  into a wide row or the earlier merge and upsert of a cell row. A
  128 KiB write into a fresh file is 8 Payload statements (12 executions,
  12 changed rows) where it was 32 (64, 96); the same window overwritten is
  4 reads and 4 in-place writes.
- **In-place writes are counted and fenced (P4).** `PayloadWork` gains
  `in_place_writes`, `in_place_bytes` and `in_place_ns`. `write_in_place`
  refuses unless the job's transaction has begun, which the inode upsert
  before every payload write does; the handle is closed before the job
  returns. The daemon's diagnostic rows do not carry the three counters.
- **Shrink, sparse put, raw cell.** A shrink cuts the one dense row that
  holds the boundary with `substr` statements (its whole cells below the
  boundary cell stay one row, the boundary cell's kept bytes become a cell
  row) and frees the rest at once; a masked cell row is trimmed as before;
  a shrink to a slot boundary touches no row. `put_cell` carves its cell
  out of a live wide row, which keeps its other cells as rows of their own.
  `cell`/`captured_cell`/`source_cell` return the cell of a wide row whole.
- **Moves.** Failed-capture fold and orphan migration share
  `Overlay::transfer_row`: one lower row a step. The row itself moves
  (`UPDATE ... SET gen, epoch`) when the upper layer has no live row over
  its cells and all its bytes are effective; otherwise its cells are merged
  below the upper bytes one at a time and the row is dropped.
- **Maintenance page.** `PAGE` stays 14 payload cells; a wide row counts as
  the cells it holds (`Page::fit`), so a step, and the releasing job's
  inline budget, drop at most 14 cells of bytes however the rows are
  shaped: one 32 KiB row a step while the next row is another one.
- **Captured scan.** A probe starts at the slot of its position; a window
  ends with the widest row that holds its first byte, at most `RUN_BYTES`
  with `RUN_BYTES/8` mask bytes. Workspace's scan and the daemon's
  `CapturedRun` reply charge use those bounds.

Memory (P5), per daemon and one job at a time: SQLite's bind and record
buffers of one row are at most 2 x 32768 bytes where they were 2 x 4096; one
row is loaded per read step; a captured window and its reply charge are at
most 32768 + 4096 bytes per in-flight captured read where they were 4096 +
512. Nothing grows with files, bytes or rows. Not changed: the read and write
windows, accounting triggers, secondary indexes, admission. Numbers and the
worst-case storage table are pinned in
[`costs.rs`](../../crates/layerfs-overlay/tests/costs.rs) and
[`payload_runs.rs`](../../crates/layerfs-overlay/tests/payload_runs.rs).

R7 update, 2026-10-09 (descriptor row, lead decision C2 part 2, overlay schema
25): **an open regular-file descriptor is one row in two b-trees.** The
`native_file` table (three b-trees: its key, `UNIQUE(ns,mount,request)` and
`UNIQUE(ns,owner)`, with two accounting triggers and a cascading foreign key)
is folded into `file_handle`, which gains a `mount` column: a native
descriptor stores its mount's owner and the kernel request that received it
(the same 64 bits, as an integer); any other descriptor stores mount 0 and
its caller's positive request. The table is `PRIMARY KEY(ns,owner),
UNIQUE(ns,mount,request)`: the first answers the fences and RELEASE by one
seek, the second refuses a repeated request identity (of one mount, or of one
caller) inside the descriptor's own insert, finds a lost visit's descriptor
(`retained_native_file`, `retained_file`) and orders the retirement window
of a revoked mount (`FILE_WINDOW`, no sort). `FENCE_FILE`, `FENCE_HANDLE`,
`NATIVE_HANDLE_HELD` and `native_file` read one table instead of a join.
The foreign keys of `native_file` are gone with it: the mount's row is read
by the fence of the job that inserts a descriptor, and deleted by the
maintenance job that has just read the mount's descriptor window empty; a
declared key from `file_handle` could not be kept without a third b-tree
(SQLite's child scan of a deleted mount does not use a partial index and
would visit every descriptor of the namespace). A created-and-opened file
writes 3 custody statements (6 executions), was 4 (8); OPEN writes 2 (3),
was 3 (5); RELEASE runs 3 executions, was 5 (the association's trigger and
the cascade are gone). `StoredCounts::owner_details` counts one row per open
descriptor, was two. Asserted by
[`descriptor_request`](../../crates/layerfs-overlay/tests/descriptor_request.rs)
(refusal, lookup by request, two mounted Workspaces, a caller's request of
the same number, retirement beside 2 and 40 unrelated descriptors with equal
statements and VM steps) and the count tests named in the receipts.

R7 update, 2026-10-09 (custody rows, lead decision C2 part 1, overlay schema
24): **a descriptor or a lookup reference is its own row and has no `lease`
row.** `lease` rows of kind 7 (file and directory descriptors) and kind 9
(lookup owners, native and not) were written and deleted with their owner
and never read by key; they are no longer written, and the table's CHECK is
`kind BETWEEN 1 AND 6 OR kind=8`. The custody is the owner's own row:
`file_handle`, `native_directory`, `lookup_owner`, `native_lookup`. The one
reader that used "any `lease` row of the namespace" as "the namespace is
owned" is the terminal-cleanup decision `queue_closed_at`
([`close.rs`](../../crates/layerfs-overlay/src/lifetime/close.rs), statement
`HELD`): it now also asks `file_handle`, `lookup_owner` and `native_lookup`
by the namespace prefix of their keys, beside `lease`, `native_mount` and
`native_directory` as before, so a closed Workspace is held by exactly the
owners that held it and Close, drain, unmount and forced unmount report what
they reported. `GENERATION_HELD` (kinds 1 and 6) and the operation probe
(kind 4) are unchanged. `StoredCounts::owner_rows` is the number of `lease`
rows and therefore no longer counts descriptors and lookup references;
`owner_details` counts their rows, as it did. `PRAGMA user_version` and its
startup readback are 24. Stored rows of one created and open file 6 -> 4
(b-tree entries 12 -> 8), of one kernel-referenced inode 3 -> 2 (5 -> 3).
Per created file the five request jobs attempt 38 statements (48
executions) instead of 41 (54): CREATE's Lease family 6 (12) -> 4 (8),
RELEASE's 3 (7) -> 2 (5); OPEN 4 (7) -> 3 (5). Proofs:
[`close_custody.rs`](../../crates/layerfs-overlay/tests/close_custody.rs)
(each kind of custody alone, none, another Workspace's rows, the plan of
`HELD`) and the count tests named above.

R7 update, 2026-10-09 (inode statements, lead decision C1, overlay schema
23): **a visit job reads an inode row once and writes it without a second
read.** `INODE_LOOKUP` also returns the generation that holds the row and
its `epoch` and `height`. A visit (`observe_native_visit`,
`open_native_visit`, `mutate_native_visit`) keeps the rows it read in
`JobRows`
([`job_rows.rs`](../../crates/layerfs-overlay/src/namespace/job_rows.rs)), a
value on that job's stack with `COMPOUND_INODES` (4) places, about 0.5 KiB,
gone when the job returns: a later evaluation round asks `SourceRows::inode`
again and no statement runs, and `put_inode_domain` takes the row out of it
and writes over it. Nothing is kept across jobs. A serial past the four
places, an orphan-domain row and every job that is not a visit read and
write by the indexed statements as before (`LAYER_ACTIVE`, then
`LAYER_LOWER`); nothing is refused. The upsert `INODE_PUT` is gone. A row is
written by `INODE_INSERT` (no row of the serial in the generation) or by an
update of the row its key names: `INODE_RESIZE` (time and size) or
`INODE_RECOUNT` (time, `entries`, `subdirs`) when the other columns are the
ones the job read and the layer columns did not move, `INODE_UPDATE` (every
column) otherwise. An update that changes no row fails the job. A serial
named by `Changes::created` is inserted with no read; a row that already
has it in the active generation is a constraint failure that rolls the job
back, never an update. `CHECK(kind IN (1,2,3))` on `inode` and the
eleven-value list on `maintenance.kind` are written `BETWEEN`: the same
constraints, without the ephemeral table an IN-list of three or more values
compiles to. `PRAGMA user_version` and its startup readback are 23. Per
created file the five request jobs attempt 41 statements (54 executions)
instead of 48 (61): LOOKUP's Inode family 2 -> 1, CREATE's 8 (9) -> 3 (4),
WRITE's 3 -> 2; UNLINK's 8 (9) -> 5 (6). Exact per job in
[`native_visit_cost.rs`](../../crates/layerfs-workspace/tests/native_visit_cost.rs)
and
[`native_unlink_cost.rs`](../../crates/layerfs-workspace/tests/native_unlink_cost.rs);
the stored rows, the four places and the collision in
[`job_rows.rs`](../../crates/layerfs-overlay/tests/job_rows.rs); plans in
`explain_compound`. Not changed: the name seeks of a second evaluation, the
custody rows, the accounting triggers and transaction framing.

R7 update, 2026-10-09 (directory link counts, overlay schema 22): the `inode`
row gains `subdirs INTEGER NOT NULL CHECK(subdirs>=0 AND (kind=2 OR
subdirs=0))`, the absolute number of child directories bound in a directory
in the row's view, and `Inode` gains the field. Every whole-row statement
carries it (`INODE_PUT`, now fifteen bound values and 120 declared bound
bytes; `INODE_LOOKUP`; `INODE_CAPTURE`; the orphan select; the fold's select
and its `put_inode` copy). `PRAGMA user_version` and its startup readback are
22. No statement was added and no statement count changed. `nlink` keeps its
meaning, the namespace reference count that construction writes back. The
filesystem meaning of the column belongs to Workspace; see
[namespace operations](30-namespace-operations.md).

R7 update, 2026-10-09 (OPEN visit, schema unchanged at 22):
`Overlay::open_native_visit` is `observe_native_visit` under the kernel's
lookup reference with one addition: when the decision finishes with the
regular file it was asked for, the same transaction inserts the descriptor
(`file_handle`, its `lease`, the open count) and its `native_file` row for
the kernel request (since schema 25 the one `file_handle` row, which names
the mount and the request). A decision that names another inode or another kind
fails the whole job, and an undecided or refusing one writes nothing. No
statement was added. It attempts 10 statements (13 executions) for a base
file and 9 (12) for a local one, where the source-holding OPEN attempted 64
(82) and 58 (76) over five and four jobs.

R7 update, 2026-10-09 (READ window visit, schema unchanged at 22):
`Overlay::read_native_visit` is the read-only job of a native READ or
READLINK. It runs outside a transaction and writes nothing: the fence
statement, the orphan probe while `orphan_seen` is set, the inode row, and
the cell range of the requested window when the inode has local payload.
It returns the base root the window belongs to (the orphan's retained root,
or the Workspace's current one) with the ordinary `LocalRead`. No statement
was added: it attempts 2 statements for an inode with no local row, 3 with
local payload, and one more once an orphan exists. See
[native read custody](73-native-read-custody.md).

R7 update, 2026-10-09 (statement diet, schema unchanged at 21): **a job
reads a row once.** A native visit's fence is one statement
(`FENCE_LOOKUP`, `FENCE_FILE`, `FENCE_HANDLE` in
[`statements.rs`](../../crates/layerfs-overlay/src/database/statements.rs)):
the Workspace row, its `native_mount` row and the kernel's own reference on
the named inode, each reached by key, with the earlier outcomes (no row, a
foreign or revoked mount or a missing reference is `Stale`; a closed
Workspace is `Closed` first). The Workspace row it returns is the one the
visit's evaluation, its `apply_checked` and the terminal-cleanup decision of
a RELEASE use; none of them reads it again. The descriptor the fence read is
not re-validated: only its access mode is checked, in memory. The active and
the latest lower row of one name come from one seek (`NAME_LAYERS`), and a
publication computes a name's inheritance once. A file-reference decrement
returns the references that remain (`UPDATE ... RETURNING`), and the kernel
lookup reference and descriptor of a created-and-opened file are one custody
row write. `Changes::created` names the serial a job creates: its kernel
lookup row is inserted without a read, and a row that already exists fails
the whole job. The orphan-domain probe of an inode read runs only while this
engine holds an orphan row (`Overlay::orphan_seen`; see the exact-flag update
below).
Per created file the five request jobs now attempt 48 statements (61
executions) instead of 88 (101), exact per job and family in
[`native_visit_cost.rs`](../../crates/layerfs-workspace/tests/native_visit_cost.rs);
plans are `Overlay::explain_native_visit` and the `name-layers` line of
`explain_compound`. Not changed: transaction framing, the `lease` rows, the
accounting triggers, the layer reads of `put_inode_domain` and the second
evaluation of a visit that needed a resident base fact.

R7 update, 2026-10-09 (overlay schema 21): **reply tickets are held in the
engine's memory**, not in a `request` row
([`ReplyTickets`](../../crates/layerfs-overlay/src/lifetime/tickets.rs)). The
job that advances the frontier issues the ticket; a job that fails takes its
ticket back. The one reply attempt returns it through
`ReplyTickets::attempted`, callable from any thread and without an owner
turn; a ticket that is not held is `Stale` and changes nothing, so exact
ticket identity (namespace, revision, generation) is kept. `capture_ready`,
`capture` and the terminal-cleanup hold read the pending set; a job that
finds a namespace's tickets pending marks it watched, and the attempt that
empties a watched namespace reports `Watched`: its caller owes one owner
turn, `Overlay::reply_settled`, which queues a cleanup that waited. The
database is created by its engine and never reopened, so the stored row
outlived nothing this record does not; the set is bounded by the
publications whose replies are in flight. `pending_publications` and the
`reply_tickets` count read the same set.

R7 update, 2026-10-09 (overlay schema 20): an atomic job starts its SQLite
transaction at its first writing statement. Reads before it run in
autocommit, which is the same state here: the connection is the only one,
holds the exclusive lock and runs one job at a time. A job that writes
nothing issues no `BEGIN`, `COMMIT` or freelist read. A nested atomic job is
refused. Owner identities are minted from a counter in the connection, which
creates the database and never reopens it; `workspace.next_owner` is gone,
and an identity minted by a rolled-back job is not reused. The prepared-
statement cache holds 256 statements, above the engine's fixed set of about
224 literal texts, so a steady request cycle prepares nothing twice; the set
is fixed by source and does not grow with files, bytes or operations.
Per-statement observation reads and resets six counters after execution, and
`SQLITE_STMTSTATUS_MEMUSED` is sampled only for statements prepared for one
use.

Implemented source: the S0/S1 checkpoint introducing `layerfs-overlay` on local
main for [#307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307).
Complete Workspace/FUSE/daemon and milestone qualification remain outstanding;
the [design](../issues/303/README.md) and
[progress](../issues/307/PROGRESS.md) retain the broader target and unfinished gates.

The active [crate](../../crates/layerfs-overlay/src/lib.rs) owns one SQLite
connection and SQL/payload/operation records/custody. It creates fresh disposable state
once; `open_workspace` inserts only a root/incarnation/routing row, without
another connection, schema or immutable-root walk/materialization. Root authority
and filesystem semantic checks remain the Workspace/runtime caller's obligation.

The [profile](../../crates/layerfs-overlay/src/database/profile.rs) reads back MEMORY
journal, OFF synchronization, EXCLUSIVE locking, zero mmap/busy timeout, foreign
keys, 4096-byte pages, configured pager/max-page settings, SQLite version and
compile options. Pager size is a suggestion, not a resident ceiling. Temporary
work selects FILE; ordinary indexed statements need no temporary sort. macOS
retains system SQLite; Linux alone enables the existing driver's bundled feature.
Global Store persistence is unchanged. Disposable state has no sync/checkpoint,
crash-survival or restart-recovery claim.

The original schema had eight application tables. Schema v11 added
[bounded live composition](32-live-composition.md) and
[independent file/processing custody](33-independent-custody.md), alongside
SQLite's AUTOINCREMENT allocator. Namespace IDs are not reused.
Every job constrains namespace and checks incarnation against its routing row.
Inode/name keys support both current lookup and generation-selective capture;
cell keys include serial, generation and aligned offset. Metadata queries exclude
payload. Names, data and validity are BLOBs. OperationRecord/owners are independently keyed
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
fields without copying inode/name/payload populations. Resolution/install and bounded automatic reclamation are now implemented at
the scope of the S6 checkpoint guides; physical admission remains unfinished.

`captured_inodes` uses fixed `(ns,gen,serial)` membership, keyset pagination and
64-row windows, explicitly selecting `inode_capture`. Initial macOS EXPLAIN chose
the primary `(ns,serial,gen)` key and residual-filtered generation: one returned
captured row cost 540/4124/16412 actual VM steps with 128/1024/4096 later active
rows. The [failure](../issues/307/checks/s1-initial/residual-plan-failure.log) and
[corrected profiles](../issues/307/checks/s1-initial/repaired-engine.log) are retained.
An indexed SEARCH/fullscan counter of zero alone was insufficient evidence.

The [profiler](../../crates/layerfs-overlay/src/diagnostics/metrics.rs) observes actual step
executions, VM/fullscan/sort/autoindex/reprepare counters, returned/attempted changed
rows, bound bytes and inclusive statement wall. It resets cached counters before
each invocation and aggregates in fourteen fixed slots. Exclusive owner-job
snapshots provide attribution; shared lifetime counters are not phase/per-Workspace
resource peaks. Bootstrap PRAGMA/schema work is outside job counters. B-tree
internal page visits, journal/dirty/device bytes, copy allocation and process/kernel
residency remain unavailable. `pages()` reports shared page/freelist observations.

Point access costs O(log N) B-tree work. A capture page returns K<=64 rows with
O(log N+K) index visits plus noncovering fetches, which can add O(K log N).
Fixed-cell replacement binds 4608 data/mask bytes plus keys per call and incurs
SQL/index/page/journal work independent of older fragment count. First-touch and
tiny-file amplification require S5/S7 qualification. OperationRecord paging holds at most
64 records of at most 65536 bytes; total records continue through windows. Capture
has fixed metadata SQL work, but daemon queue/reply drain is not a constant wall
claim. No speed/cache/residency claim follows from these count diagnostics.

Missing acceptance: fair queued service, retained reads across install, complete
metadata operations, cell transitions/truncate/holes, orphan/failure composition,
physical headroom, automatic live/idle reclaim, authenticated runtime, mounted
cache/mmap/coherence/Exec, canonical construction and integrated qualification.
No S0–S13 completion follows from this initial checkpoint.

## Terminal maintenance checkpoint (#307)

The slice after `c6df039e6` adds schema v4, exact close eligibility and bounded
automatic terminal deletion during live/idle daemon periods. See
[terminal reclaim](25-terminal-reclaim.md) for actual indexed scope, original
error retention and remaining live-generation/orphan/pressure/kernel criteria.
Earlier schema/profile evidence retains its original pin.

## S1 closure audit and profile correction

The closure slice after `14c8a7be4` removes the arbitrary default4GiB quota.
`ProfileConfig.max_pages=None` selects/readbacks SQLite’s format ceiling; an
explicit physical quota is separately recorded. Actual device headroom/pressure
remains S6 work, and this setting supplies no large-capacity qualification.
Operational plan diagnostics now cover exact cell/name/operation records/lease accesses
through shared production query templates. [S1 exit audit](../issues/307/S1-EXIT-AUDIT.md)
maps every criterion to source and scoped proof. S2/S4/S5/S6/S7 remain distinct exits.

## S2 exact custody closure

The closure after `7019801f9` advances schema5 with frozen capture revision,
exact ticket generation and route engine affinity. Bounded observations retain
identity after lost completions; captured bytes remain readable during close.
See [capture custody](26-capture-custody.md) and its S2 exit/evidence map.
S1 evidence keeps its schema4 pin; physical live-generation/failure/pressure
acceptance remains S6/S7.

## Base-source prerequisite checkpoint

The slice after `6e84b9181` adds schema6 transient source custody and maintained
install readiness. The owner now has six classes: new source acquisitions wait
behind a finite known-install fence while existing reads/releases, mutations and
other namespaces progress. See [base-source windows](28-base-source-windows.md).
Earlier schema/class/query receipts keep their original pins. Actual prepared
Workspace/actor/native composition and effective merge remain required.

## S3 completion reconciliation

The completion after `bdc6ed4af` adds effective source-qualified read/ordered-name
composition, actual paired actor install, original unattempted-command custody and
owning SDK stat lengths. See [effective base view](29-effective-base-view.md) and
[S3 exit audit](../issues/307/S3-EXIT-AUDIT.md). Earlier limitations/evidence above
retain their source scope; native/logical runtime transport, mutable byte semantics
and aggregate resource acceptance remain unfinished.

## S4 compound namespace job

The checkpoint after `8d691ab8a` advances the disposable schema to version 7:
every inode row carries its creation generation and a directory's exact visible
entry count. `apply` publishes at most four inode finals, two name finals and
one cell atomically with a single ticket; `source_rows` gives one owner job
consistent point reads, including a name's active and latest lower rows. A
removed name keeps a whiteout only where a lower row or the base binds it.
`publish` remains the single-inode primitive and now shares the same checked
write helpers. See [namespace operations](30-namespace-operations.md) for the
semantics, work derivation and paired plan/runtime evidence. Earlier receipts
keep their schema pins. Byte-stream transitions, removed-inode reclamation,
failure composition and pressure remain S5/S6.

## S5 payload checkpoint

The work after `f5558fc22` advances schema v8 with trimmed cells, optional
validity, engine-maintained lower-layer cutoffs and indexed shrink staircases.
Compound jobs carry one bounded byte write. Effective reads compose local layers
and one inherited range; raw cell APIs remain stored observations. See
[payload streams](31-payload-streams.md) for complexity, amplification, custody
and the paired real-owner evidence. Stale garbage, failure depth, orphans and
physical pressure remain S6; earlier receipts retain their schema/source pins.

Initial S6 after `a0dc7da9b`: schema v9 adds bounded failed-capture composition
and indexed live maintenance. [Live composition](32-live-composition.md) records
the implemented slice and remaining orphan/physical-headroom exits; S6 is not
complete. Earlier receipts keep their source/schema/work identities.

S6 completion update after `be651a048`: schema14 now includes backed resource
accounting and indexed source waits; physical reservation, cleanup headroom,
bounded generation wakes and nonduplicating orphan migration are implemented.
See [shared physical capacity](35-shared-physical-capacity.md) and the
[S6 exit audit](../issues/307/S6-EXIT-AUDIT.md) for current scope/evidence. Earlier
checkpoint limitations and numbers above retain their original source identity.
Native/runtime/kernel and integrated qualification remain later milestones.

## Read-only terminal cleanup observation

The R7 instrumentation slice adds `Overlay::observe_cleanup(namespace,
incarnation)` without changing schema 19. It performs one fixed query over the
namespace primary key, its existing terminal reclaim key and the existing
AUTOINCREMENT sequence. Live rows require the original incarnation and report
Live, Held or Queued from their actual close/owner/queue state. An absent row
reports Gone only when the namespace lies in this daemon's already allocated
domain. Namespace IDs are not reused. Gone establishes present physical absence;
it does not attest that the supplied absent-row incarnation existed historically.
Invalid, future and mismatched live identities remain errors.

The observation creates no mutable Route, advances no maintenance, adds no
retained lookup/tombstone/index, and is independent of unrelated namespace or
root size. `explain_cleanup_observation` uses the same production query. Host
proof receipt 028 in the R7 stage records one statement, 55 VM steps and zero
full-scan steps beside 64 and 256 unrelated namespaces, with its actual SQLite
profile. Linux proof 095 records 47 VM steps at both sizes with SQLite 3.53.2;
its plan and runtime profile remain separate from the host receipt. Mounted
release lifecycle proof 084 observes Gone after both normal unmounts at its
scoped tree. These are scoped counts and functionality, not qualification.
The authenticated daemon Cleanup control prices one read-only owner job
after terminal routing removal, as described in the native-control guide.

R7 update, 2026-10-09 (exact orphan flag, decision U2, schema unchanged at
22): **`Overlay::orphan_seen` is true exactly while the engine holds an
orphan row.** It is set before the only `INSERT INTO orphan`. A transaction
that deletes orphan rows (the last step of `maintain_orphan`, and the orphan
phase of closed-namespace reclamation when its page deleted rows) then reads
`orphan_rows` of accounting namespace 0, the count the orphan triggers
already keep, and clears the flag at zero: one point statement per such
transaction, no scan and no new state. Each orphan-domain inode row is
deleted before its orphan row (same transaction live, an earlier phase
closed), so false still means that neither exists in any namespace. A job
that fails restores the flag it started with, as its rollback restores the
rows. Before, the first orphan of a daemon made every later inode read of
every Workspace attempt one more statement for the daemon's life: 9 Inode
attempts per created-and-removed file (LOOKUP 2, CREATE 3, WRITE 1,
UNLINK 3), 30 instead of 21. Now a file made after the last orphan was
reclaimed, and a Workspace mounted later, pay the fresh engine's 21; the
last maintenance step of each orphan attempts one more Reclaim statement.
Exact per job and family, beside 25 and 100 kept files, with UNLINK (22
attempts), FORGET (12) and the maintenance steps of one removed file (4
when FORGET arrives first, 7 when the owner runs first), in
[`native_unlink_cost.rs`](../../crates/layerfs-workspace/tests/native_unlink_cost.rs).
Not changed: what is enqueued at UNLINK and FORGET, the steps themselves and
closed-namespace reclamation.

R7 update, 2026-10-09 (the releasing job finishes a small orphan, decision
U1, report items O3 and O4, schema unchanged at 22). Implemented:

- **UNLINK queues only what can run.** `detach_orphan` reads the file's
  references once. With none it queues the retirement of the active layer, as
  before. With any it creates the orphan and no longer queues that
  retirement, which the orphan held until the step that releases the layer
  queued it again. The orphan's own item is queued only while a descriptor
  or a reader holds the file (`opens+readers > 0`). Under kernel lookups
  alone nothing is queued and `maintenance_pending()` stays false; the first
  later descriptor or reader of such a file queues the item
  (`Overlay::file_ref`, add branch, one probe only while `orphan_seen`).
- **The last reference finishes the release.** In the `left == 0` branch of
  `Overlay::file_ref` the item is queued and made ready as before, the exact
  fallback. If the serial has an orphan row, `Overlay::finish_release` then
  runs, in the same transaction, the existing steps `maintain_orphan` and
  `retire_serial` on the serial's ready items in the owner's order (the
  orphan's item, each ready layer retirement, around again), read from the
  ready queue itself. Every hold is the step's own check; a held item leaves
  the ready queue and the loop as it does for the owner. Bound: at most 6
  step calls, which together drop at most one step's page (`PAGE`: 14
  payload cells, 64 rows of cells and shrink rows; a step that has less of
  the page left drops fewer of the rows it selected). What remains stays
  queued and ready for the owner thread. When the serial has nothing ready
  left, `maintenance_ready` returns to the value it had before this job
  queued the item, so no empty turn follows.
- **One per job, never in a step.** `Overlay::release_step` (one flag per
  engine) is set when an atomic job starts, cleared by the first last
  reference that uses it and by `Overlay::maintain`. A job that drops
  several last references finishes one and queues the rest; `retire_native`
  (64 lookups or handles a step) finishes none.

Not changed: a file with no orphan row still queues its `ORPHAN` item at the
last reference (the custody row is deleted by that step); FORGET's own reads;
closed-namespace reclamation; the daemon's wake rule. Counts, superseding
those of the update above: UNLINK under the kernel's lookup 22 -> 20
statement attempts, FORGET of a one-cell file 12 -> 47, maintenance steps
afterwards 4 (FORGET first) or 7 (owner first) -> 0, the three together 81
or 122 -> 67. A 56-cell file: the FORGET drops 14 cells and the queue ends
it in 5 steps. Pinned in
[`native_unlink_cost.rs`](../../crates/layerfs-workspace/tests/native_unlink_cost.rs).

R7 update, 2026-10-09 (item-free steps, report item O6, schema unchanged at
22). Implemented, superseding "read from the ready queue itself" above:

- **A step and its queue item are separate.** `migrate_orphan` (an orphan
  something holds), `reclaim_orphan` (an orphan nothing holds) and
  `retire_layer` do the work for `(ns, serial[, gen])` and return a `Step`:
  rows, bytes, what is left (`Next`: ready, a migration cursor, held, done)
  and the layer an orphan's step released. `maintain_orphan` and
  `retire_serial` are the queue's wrappers: they read the orphan row and the
  references, call the step, and `settle_item` queues the released layer's
  retirement and advances, parks or deletes the item. Every hold check and
  every delete exists once.
- **The releasing job writes no item for work it ends.**
  `Overlay::finish_release` takes the orphan row `file_ref` read, calls
  `reclaim_orphan` and, for each layer this job released, `retire_layer`
  (oldest first, the orphan's row as the job left it), under the same bound
  (6 calls, one `PAGE`). It neither seeks nor writes the ready queue while
  it runs. Afterwards: a finished orphan deletes the item an earlier
  descriptor or reader may have queued (one statement); an unfinished one is
  queued and made ready; a layer left unfinished is queued ready; a layer
  held by a generation owner is queued and parked (`hold_item`), which
  `wake_generation` readies as before. `maintenance_ready` returns to its
  earlier value when the job left nothing ready. A file with no orphan row
  still queues its item.
- **FORGET reads the Workspace row once**: the row read for the attachment
  check also decides the pending close (`queue_closed_at`).

Not changed: what is reclaimed and when (the last holder, the page, one
inline finish per job, `retire_native` finishing none); FORGET still reads
the mount row and the lookup row with one statement each, because the visit
fence returns only the presence of the lookup and FORGET needs its owner,
count and implicit flag. A retirement item of the same serial that was
already ready before the releasing job is left to the owner thread; the job
no longer seeks it. Counts: FORGET of a removed one-cell file 47 -> 29
statement attempts (Workspace 4 -> 3, Inode 2, Lease 17 -> 11, Reclaim
21 -> 10), UNLINK + FORGET 67 -> 49; FORGET of a 56-cell file 43 -> 33 with
the same 14 cells dropped and 5 steps left; FORGET after an open-at-unlink
migration 31 -> 19. Pinned in
[`native_unlink_cost.rs`](../../crates/layerfs-workspace/tests/native_unlink_cost.rs);
the parked layer in `layerfs-overlay` `tests/orphans.rs`
(`a_last_release_under_a_generation_hold_parks_the_layer_for_the_queue`).

R7 update, 2026-10-09 (a live file's last reference, report item O2, schema
unchanged at 22). Implemented, superseding "a file with no orphan row still
queues its item" above: in the `left == 0` branch of `Overlay::file_ref` a
serial with no orphan row has its custody row deleted by that job
(`Overlay::drop_custody`, the statement the `ORPHAN` step ran) and nothing
is queued or woken. The orphan row is not probed while `orphan_seen` is
false. This path is not gated by `release_step`: `retire_native` uses it for
each lookup or handle of its page and still finishes no orphan inline (an
orphan there is queued and made ready). Counts: FORGET as the last reference
of a file that keeps its name 11 -> 10 statement attempts (Lease 5 -> 6,
Reclaim 2 -> 0), RELEASE as the last reference 9 -> 8 (Lease 3 -> 4, Reclaim
2 -> 0), one more Lease attempt each while the engine holds an orphan;
maintenance steps afterwards 1 -> 0 and `maintenance_pending()` false.
Pinned in
[`native_unlink_cost.rs`](../../crates/layerfs-workspace/tests/native_unlink_cost.rs).
The no-orphan branch of `maintain_orphan` is kept for an item that outlives
its orphan row.
