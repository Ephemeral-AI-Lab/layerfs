# O. Workspace overlay and FUSE request path (appendix notes)

Code identity: `origin/layerfs_v2_0_0` at `8cfb6e2734` (read with `git show`,
never checked out). Builds on `docs/research/paper/notes/B-engine.md`
sections 1-4 and 9; this file goes one level deeper and does not repeat their
Commit/merge material.

Citation conventions (same as B-engine):
- `src:<path>:<lines>`: source file on `origin/layerfs_v2_0_0`. Overlay crate
  paths are abbreviated `ov/` = `engine/crates/layerfs-overlay/`, `ws/` =
  `engine/crates/layerfs-workspace/src/`, `mp/` =
  `engine/crates/layerfs-mount-ports/src/`, `fu/` = `engine/crates/layerfs-fuse/src/`.
- `doc:<path>:<lines>`: page under `docs/architecture/` on the same branch.
- **[inferred]** marks a statement I derived from code/structure rather than
  read stated in source or docs.

---

## 1. Overlay database schema (user_version 39)

Source of truth: `src:ov/sql/schema.sql:1-114`. All tables `STRICT`;
`PRAGMA application_id=1279676210` (line 88), `PRAGMA user_version=39`
(line 114). One file per mounted Workspace; "Runtime custody is in the engine"
(line 2). No Workspace row exists: `WorkspaceState` lives in memory
(doc:engine/overlay/README.md:85-87).

### 1.1 Tables

| Table | Key | WITHOUT ROWID | Columns | Meaning |
| --- | --- | --- | --- | --- |
| `fs_inode` | PK `(inode, generation)` | yes | kind 1..3 (file/dir/symlink), mode, mtime s/ns, nlink, size, `inherited_cutoff`, `born`, `entries`, `subdirs`, shrink `epoch`, `height`, `referenced` 0/1, content `sequence` | One row per changed inode per generation (schema.sql:3-23). Index `fs_inode_capture(generation, inode)` (line 24) serves capture pages. |
| `fs_entry` | PK `(parent, name, generation)` | yes | `inode` NULL = whiteout; `inherited` 0/1 = lower view binds this name | One row per changed name per generation (25-32). `name` is a BLOB of 1..255 bytes (binary names). Index `fs_entry_capture(generation, parent, name)` (33). |
| `fs_data` | `rowid INTEGER PRIMARY KEY`; `UNIQUE(inode, generation, cell_offset)` | **no** (rowid table) | `cell_offset` multiple of 4096, `epoch`, `data` 1..32,768 B, `validity` mask | Payload: one cell (≤4,096 B + optional per-byte validity mask) or a dense run of whole cells inside one aligned 32 KiB slot, no mask (34-45). Index `fs_data_generation(generation, inode, cell_offset)` (85). |
| `fs_truncate` | PK `(inode, generation, depth)` | yes | `cell_offset` (boundary), `epoch` | Shrink "staircase" step rows (46-53). Index `fs_truncate_generation` (86). |
| `fs_data_ref` | PK `(generation, inode, file_offset)` | yes | `length`, `epoch`, `source` (32-byte content root), `source_offset` | Reference piece: bytes `[file_offset, file_offset+length)` of this layer equal committed bytes of `source` from `source_offset`; holds no bytes; pieces of one layer never overlap (54-67). |
| `fs_digest` | PK `inode` | no (INTEGER PK = rowid alias) | `sequence`, `length`, `digest` (BLAKE3, 32 B) | Whole-content digest of a file written only as appends from offset 0 with all descriptors closed; valid only for the inode row with that `sequence` (68-77). |
| `fs_predecessor` | PK `inode` | no (rowid alias) | `predecessor` (≠ inode) | "The file a rename put this one in place of": advisory delta base for Commit, never a fact about bytes; a chain keeps one row ending at the oldest file (78-84). |
| `cleanup_task` | PK `(kind, resource, target)` | yes | `phase`, `cursor`, `aux`, `name`, `ready` | Queued reclamation, kinds 1..12 (90-100). Indexes `cleanup_task_ready(ready,kind,resource,target)`, `cleanup_task_generation(target,kind,resource)` (101-102). |
| `commit_record` | PK `(operation, file_scope[8], kind, key[32])` | yes | `value` ≤ 65,536 B | Indexed operation records: Commit construction scratch and SDK hard-link maps; overlay gives them no meaning (105-112; doc:engine/overlay/README.md:157-161). |

Why `fs_data` is a rowid table **[inferred]**: in-place writes into a live
dense row use SQLite incremental blob I/O
(`blob_open(MAIN_DB, "fs_data", "data", row, …)`, src:ov/src/payload/runs.rs:149-164),
which requires a rowid table; several statements address rows by `rowid`
(`ROW_DROP`, `ROW_KEEP`, `ROW_MOVE`, `ROW_SLICE`,
src:ov/src/database/statements.rs:53-58).

Generation values: positive generations are view layers; `-1` is the
**orphan domain** (`DOMAIN = -1`, src:ov/src/lifetime/orphan.rs:10; schema CHECKs
`generation=-1 OR generation>0` at schema.sql:5,37,48,59).

**No accounting rows.** "Ordinary jobs keep no accounting row";
`resources()` projects row counts, bytes, pages, freelist and a debt upper
bound on demand (doc:engine/overlay/README.md:105). The only bookkeeping
tables are `cleanup_task` (maintenance queue) and `commit_record`.

### 1.2 What a row means: the layered view

```
generation:  -1        1 .. installed   installed+1 .. captured   active
             orphan    retired: never   sealed by Capture         new rows
             domain    in the view      (read by Commit)          go here
                       |<---------- view: (installed, active] ---------->|
```
(doc:engine/overlay/README.md:109-116.) Lookup of serial s = newest row with
`installed < generation <= active` (`INODE_LOOKUP`,
src:ov/src/database/statements.rs:23-25); no row = the committed base decides,
merged by Workspace outside SQL. Name lookup reads the active row and the
newest lower row in one seek (`NAME_LAYERS ... LIMIT 2`, statements.rs:135-136).

Key inode columns:
- `inherited_cutoff`: bytes below it not written in this generation fall
  through to the lower view. On a serial's first row in a generation it is the
  lower view's length; every shrink lowers it (src:ov/src/contract/types.rs:81-86;
  src:ov/src/namespace/inode.rs:166-258).
- `born`: generation that created the serial locally; 0 = inherits a base
  inode; `born > installed` proves the base has no such inode
  (types.rs:87-89, 317-321). Used to answer ENOENT with no base read for names
  under a directory created above the floor (doc:engine/workspace/operations.md:20).
- `entries`, `subdirs`: exact visible child count and child-directory count of
  a directory; POSIX link count = subdirs + 2 (types.rs:90-95).
- `epoch`, `height`: shrink epoch and live staircase height (§4.2).
- `referenced`: this layer has ever held a reference piece; 0 means no piece
  statement is issued on reads (schema.sql:18-19; doc:engine/overlay/payload.md:88).
- `sequence`: the Workspace revision that last wrote the row; set to
  `revision+1` on every write (inode.rs:296-298). Digests and carried write
  claims are honoured only while the sequence is unchanged (schema.sql:68-71;
  types.rs:171-179).

### 1.3 SQLite profile and pragmas

`src:ov/src/database/profile.rs:56-64` (all read back, mismatch is
`Invalid("profile readback differs")`, lines 107-120):

```
busy_timeout = 0 ; SQLITE_DBCONFIG_DEFENSIVE = on
page_size=4096; auto_vacuum=NONE; journal_mode=MEMORY; synchronous=OFF;
locking_mode=EXCLUSIVE; mmap_size=0; foreign_keys=ON; temp_store=MEMORY;
cache_size=-<pager_kib>; max_page_count=<max_pages or u32::MAX-1>
```
- Library default `pager_kib` 2,048 (profile.rs:16-23); the daemon passes
  4,096 KiB "one 256 KiB write window's transaction dirties a few hundred
  pages, and a 1 MiB pager spilled it" (src:control/crates/layerfs-sandbox/src/lifecycle.rs:85-88).
- Only SQLite 3.51.0 or 3.53.2 accepted (profile.rs:100-106).
- Connection opened `READ_WRITE | NO_MUTEX` (src:ov/src/database/startup.rs:121-124).

Transaction rule (src:ov/src/database/connection.rs:291-335): reads before a
job's first write run in autocommit (the connection is exclusive and runs one
job at a time); the first writing statement issues `BEGIN IMMEDIATE`
(doc:engine/overlay/README.md:102); success commits SQL **and** the in-memory
open-state journal together; failure rolls back once and restores open
state; a nested job is refused; an unknown COMMIT outcome quarantines the
engine for good.

### 1.4 Template creation and copy at mount

- Daemon start: `Overlay::create_observed` builds the schema at the template
  path (`include_str!("../../sql/schema.sql")`, startup.rs:131-137), then
  closes the file; its size is recorded and a mount refuses a template whose
  size changed (doc:engine/overlay/README.md:62).
- Mount: `copy_template` (startup.rs:59-61) opens the destination
  `create_new` with mode 0600 and does exactly one `std::io::copy` of the
  template bytes (startup.rs:72-85): "Copies into fresh, independently writable
  files; never a hardlink" (startup.rs:26). On Linux it re-opens the file
  `O_NOFOLLOW`, checks dev/ino and `nlink==1`, and `fstatfs` (startup.rs:86-118).
  The profile is applied to the copy; schema is not re-run (startup.rs:131).
- `Backing::new` records (dev, ino, nlink 1); terminal unmount unlinks only
  after a terminal mark and a checked close with the identity unchanged
  (doc:engine/overlay/README.md:64, 174).
- First job: `open_workspace(base_root)` (doc:engine/overlay/README.md:65).

---

## 2. Data-structure diagrams

### 2.1 A file's bytes composed from layers

Read composition (src:ov/src/payload/stream.rs:242-436; doc:engine/overlay/payload.md:13-23):
layers are the file's local inode rows newest-first in `(installed, top]`
(`LAYERS ... LIMIT 4`, statements.rs:124-126). For each layer, cells/runs
decide first, then that layer's reference pieces, then everything at or above
the layer's `inherited_cutoff` is decided (written byte or zero); the rest falls
to the next lower layer; a lower layer's own EOF yields zeros beyond it
(stream.rs:330-398). What remains undecided is the **inherited mask**, read
from the committed base file as one range `span` outside the lock
(src:ws/operations/file/read.rs:127-180); reference ranges are read from the
named committed content roots (read.rs:181-217); "a range beyond the end of its
source is an error, never zeros" (read.rs:208).

Example (one file, base length 200 KiB, active generation G+1 over captured G):

```
file offset (KiB)  0     32    64    96    128   160   192  200      260
                   |-----|-----|-----|-----|-----|-----|----|--------|
committed base     [================ base extents (chunks) ======]        (immutable, Store)
                   size 200 KiB

captured G layer   cutoff=200 size=200
  fs_data          .....[run 32K].......................................  dense run @32K (no mask)
  fs_data_ref      ...........[piece 64K..128K -> root R', off 0]......   no bytes stored

active G+1 layer   cutoff=200 size=260 (appended)
  fs_data          ..[c]...........................[run][run][c ]       cell @8K (masked, 100 B),
                                                                         dense runs @192K.. and a
                                                                         partial cell at the tail

decided by:        B c B  G-run  G-piece(R')   B     B    G+1 runs/cells
                   (B = inherited from base;  bytes past base EOF but below
                    a layer's size and not written = zero)
```

Per-byte decision: `0` undecided, `1` decided locally, `n+2` supplied by
reference range n (stream.rs:304-306). A dense row over undecided bytes is one
`copy_from_slice`; masked cells are per byte (stream.rs:362-386). Fast paths:
a single unreferenced layer with cutoff ≤ start needs no decision map
(stream.rs:294-303), and an unshrunk layer (`height==0`) needs no staleness
check (stream.rs:447-453, 503-568).

Payload row geometry (doc:engine/overlay/payload.md:36-50; types.rs:4-12):

```
file offset:  0        4096      8192           32768          65536
              | cell 0 | cell 1  | ...          | next slot ...
              |<---------- one slot (RUN_BYTES = 32 KiB) ------>|
fs_data row:  [ one cell: 1..4096 bytes + optional validity mask (512 B) ]
          or  [ dense run: whole cells, no mask, inside one slot          ]
```
"Every row that can hold a byte is found by one indexed range of its slot"
(`CELL_COVER`, `CELL_RANGE`, statements.rs:35-39, 61-63). Holes cost no rows.
A one-cell row is trimmed to its last valid byte (payload.md:47).

### 2.2 Generations before and after capture, after install

State machine (doc:engine/overlay/lifetimes.md:13-40; src:ov/src/lifetime/generation.rs):

```
            capture()                         install(capture, root)
 [active=g] ---------> [active=g+1, captured=g] ---------------------> [active=g+1,
  installed=i           installed=i, base=B      (base_readers==0)       installed=g,
  base=B                dirty counts := 0                                captured=None,
                        captured_revision frozen                         base=B'  ; RETIRE g queued]
                              |
                              | definite Commit failure: resolve_failed_capture
                              v
                        [consolidating=g ; FOLD g queued]  -- FOLD steps move g's rows
                                                              into the active generation
```
Preconditions of `capture()`: no capture in flight, not consolidating, no
pending reply ticket (generation.rs:79-90); it changes only in-memory
`WorkspaceState` ("copies no rows", generation.rs:77-117). `install()` is one
atomic job that sets `base_root`, `installed`, clears `captured`, enqueues
`RETIRE g`, wakes orphan sources (generation.rs:12-28).

Rows before/after, for one file f edited before and after capture:

```
before capture (active=5, installed=4, base=B):
  fs_inode (f,5) size=10K cutoff=8K      fs_data (f,5,@8K)
  view = (4,5]  ->  f = rows(5) over B

after capture (active=6, captured=5):    a post-capture write to f:
  fs_inode (f,5)  [sealed, read by Commit through CapturedReader]
  fs_inode (f,6) size=12K cutoff=10K     fs_data (f,6,@8K..12K)   (new layer:
  view = (4,6]  ->  f = rows(6) over rows(5) over B                cutoff = lower size)

after install (installed=5, base=B' = constructed from capture 5):
  view = (5,6]  ->  f = rows(6) over B'         rows of gen 5 out of view,
  RETIRE 5 deletes them a page at a time       B' holds exactly what gen 5 + B showed
```
**[inferred]** correctness of the rebinding: the post-capture layer's
`inherited_cutoff` was set from the captured layer's length (inode.rs:225-226,
244-255), and B' is constructed from exactly the sealed generation over B
(doc:engine/commit/construction.md:65-69), so the bytes that fell through to
gen 5 now fall through identically to B'.

---

## 3. FUSE request path

### 3.1 Concurrency skeleton

- 2 receive loops per mount (`RECEIVE_LOOPS = 2`); a receive loop runs its
  request itself; a request parked on a Store reader holds its loop, two loops
  let one proceed (doc:engine/kernel-routes/fuse.md:46, 103-104).
- One `Mutex<State>` per Workspace holding the connection, maintenance
  cursor, backing, counters (doc:engine/overlay/README.md:66). A job closure
  "performs at most one transaction and does no provider I/O"
  (src:engine/crates/layerfs-store-service/src/overlay/owner.rs:214-215).
- Frontier: while a Capture waits, later **mutations** wait (before any
  effect); while a known install waits, later **base-source acquisitions** wait
  (`behind_frontier`, src:engine/crates/layerfs-store-service/src/overlay/state.rs:60-68).
- After the job, the caller unlocks and drains ready maintenance, one step per
  lock hold, yielding to foreground waiters (doc:engine/overlay/lifetimes.md:110-131).
- Base reads: "Base reads run between owner visits, after the visit's receipt
  dropped, so no request holds the Workspace lock while it waits on the Store"
  (doc:engine/kernel-routes/README.md:100-103). Inside a job only a memory-only
  `CanonicalClient::resident` may answer base facts, at most `ROUNDS = 4`
  re-decisions (src:ws/operations/kernel_visit.rs:29, 285;
  doc:engine/workspace/operations.md:117-130).
- Kernel visits record no request source and carry only (mount, serial,
  handle); no Exec identity (doc:engine/workspace/README.md:60).

### 3.2 Per-request service

| Request | Owner job(s) | Base I/O (outside lock) | Memory state touched | Source |
| --- | --- | --- | --- | --- |
| LOOKUP | `observe_visit`: fence + `NAME_LAYERS` seek; if no local row and parent not created above floor, `Needs` base child | `BaseView::child` (directory B+tree) and inode, then re-visit | `add_kernel_lookup(mount, serial)` (+1 count); directory: `set_kernel_parent` | src:ov/src/lifetime/kernel_visit.rs:194-231; doc:engine/workspace/operations.md:13-27 |
| GETATTR | same visit, `INODE_LOOKUP` | base inode-table page if no local row | none | doc:engine/kernel-routes/README.md:88 |
| OPEN/OPENDIR | same visit; records descriptor in the deciding job | as LOOKUP | `files`/`directories` owner tables, `custody` opens+1 | kernel_visit.rs:232-260 |
| READ (≤128 KiB) | `read_visit`: read-only job composing local layers (`LocalRead`); records nothing | one `plan_read` for the inherited span + content reads for reference ranges | none ("holds nothing after its job") | src:mp/operations/read.rs:57-92; src:ws/operations/kernel_data.rs:1-7, 133-156 |
| WRITE (≤128 KiB on FUSE) | recognition (no lock), then `mutate_visit` loop; publishing job = one transaction | recognition reads mapping pages of the file's own committed content; `Needs` rounds read facts | `KernelEffect`; reply ticket | src:mp/operations/mutation.rs:239-324 |
| CREATE | `reserve_serial` (outside lock, refill 1,024), then mutate visit(s) | parent's base binding (EEXIST check) | `KernelEffect::Open` (descriptor + lookup) | mutation.rs:230-238; doc:engine/workspace/operations.md:179-184 |
| RENAME / UNLINK | mutate visit; ≤4 inodes, ≤2 names one transaction | base facts of the names/inodes | orphan record if unlinked inode still held | doc:engine/overlay/README.md:120; doc:engine/overlay/lifetimes.md:66-79 |
| READDIR | read-only visit with descriptor fence and one window of ≤1,024 local names; merged with base page in or outside the job; publishing visit records accepted cookies | base directory page (if not resident) | cookie pages in `cookies` | doc:engine/workspace/operations.md:148-170 |
| READDIRPLUS | **not implemented**: `ENOSYS` (no capability negotiated) | — | — | src:fu/request/callbacks.rs:641-662; doc:engine/kernel-routes/README.md:96 |
| FORGET | `kernel_forget`: decrement, drop row at 0 | none | `kernel` map | src:ov/src/lifetime/kernel.rs:214-238 |

Mutation driver (src:mp/operations/mutation.rs:208-324): `reserve_serial`
for creates → `recognize` for an uncached WRITE → loop
{`mutate_visit` → `Applied{publication}` | `Unchanged` | `Refused` | `Needs`;
on `Needs`: take a Store reader, `facts.supply(base, needs)`, drop reader};
after the single reply attempt, `reply_attempted(publication)` settles the
ticket "exactly once" (mutation.rs:188-200, 326-335). "An undisturbed
operation settles in two jobs" (doc:engine/workspace/README.md:111).
`FACT_WINDOW` (768) facts per supply round (same row).

Read driver (src:mp/operations/read.rs:57-92): one `read_visit`; if
`whole()` (no inherited span and no references) reply directly from the job's
bytes; else clone the window, drop the receipt, take one Store reader only if
`inherits()`, `finish()` at the root the visit named
(src:ws/operations/kernel_data.rs:146-196). "A window at or beyond a remembered
end of file take[s] no reader" (read.rs:33-35).

### 3.3 Kernel references, lookups, open handles (in memory)

`OpenState` (src:ov/src/open/kernel.rs:17-47) is a set of `BTreeMap`s with
per-job undo journals (`JournalMap`, src:ov/src/open/journal.rs:6-41):
- `kernel: (mount_owner, serial) -> {owner, count, implicit}` lookup counts
  (root has an implicit reference, kernel.rs:28-31);
- `parents: serial -> (mount, parent)` retained directory parents (used for
  cross-parent directory-move ancestry checks, doc:engine/workspace/operations.md:39);
- `files`, `directories`, `reads`, `lookups`, `operations`, `captured`:
  `OwnerTable`s indexed both by minted owner and by the caller's request, so
  custody after a lost reply is observable (doc:engine/overlay/README.md:142);
- `custody: serial -> {opens, lookups, readers}` (src:ov/src/open/files.rs:4-13),
  deciding orphan handling on last unlink;
- `orphans`, `digests` (BLAKE3 append states, ~2 KiB each), `kept_live`,
  `sources` (held `BaseSource` windows), `cookies`.
None is ever written to the file; it commits/rolls back with the job's SQL
(doc:engine/overlay/README.md:129-130). FORGET below zero is `Invalid`
(README.md:144).

### 3.4 Coherence and invalidation

Entry/attr TTL 60 s; `FOPEN_KEEP_CACHE` on, `FOPEN_DIRECT_IO` off; replies are
built from published state; WRITE returns the full count or an error
(doc:engine/kernel-routes/README.md:127-136). After install:
- own captured root: nothing sent (visible tree unchanged) (README.md:138);
- a merge root: `stale_after_install` streams the inode-table difference
  between shown and installed roots and calls `inval_inode` per changed serial
  that existed before and `inval_entry` per rebound name in directories present
  in both (src:engine/crates/layerfs-store-service/src/store/commit_merge.rs:268-310;
  src:engine/crates/layerfs-daemon/src/control/invalidate.rs:12-52).
Base-object cache needs no invalidation: keyed by `ObjectId`
(doc:engine/workspace/README.md:145).

---

## 4. Copy-up semantics (there is none)

### 4.1 First write to a committed file

`ws/operations/file/write.rs:62-87`: the job publishes the new inode value
(size = max(old, end), mtime) and one `PayloadWrite` of only the written
bytes. Comment: "No base payload is read: bytes this window does not cover stay
inherited through the layer's cut-off" (write.rs:75-76). `put_inode_domain`
inserts the serial's first row in the active generation with
`cutoff = size = lower length` (inode.rs:225-227, 244-255). **No whole-file
copy, no base read, no metadata copy beyond one inode row.**

Partial cell: "one point read and one upsert", merging with the existing
local cell (if any) and setting validity bits only for written bytes
(src:ov/src/payload/stream.rs:167-209). Unwritten bytes of that cell remain
"undecided" in the mask and fall through to the base at read time
(stream.rs:378-386) — the base is never read to fill a cell.

Whole cells: one dense row per 32 KiB slot; existing live dense row of exactly
that range is overwritten in place via incremental blob I/O; otherwise
overlapping rows of the range are dropped and one row inserted
(stream.rs:95-166). Shapes of upcoming slots are read once per window
(stream.rs:47-58, 78-82).

### 4.2 Truncate (shrink) and regrow

`ov/src/payload/layers.rs:127-203`: cut the one row holding `to` (≤32 KiB),
cut the piece crossing `to`, binary-search the staircase for the first live
step ≥ new boundary, `epoch += 1`, `height = d`, put step
`(d, boundary, epoch)`, `inherited_cutoff = min(cutoff, to)`, queue `STALE`,
`STEPS` (and `STALE_PIECES`) maintenance. A cell stamped `e` is stale iff
`e < epoch of the newest step with boundary ≤ cell` (layers.rs:71-97). "A
shrink touches one row of at most RUN_BYTES, one step row and a binary search;
it visits no discarded row" (doc:engine/overlay/payload.md:71). Regrow: bytes
between the cut and the new size are zeros, "discarded bytes are hidden at once
and never read again after a regrow" (doc:engine/workspace/operations.md:75-76);
mechanism: cutoff lowered, stale cells hidden by epoch.

`O_TRUNC` on a committed file **[inferred]**: arrives as SETATTR size=0
(src:mp/operations/write.rs:46-72) → new layer with cutoff = base length then
shrink to 0 → cutoff 0, so no base byte is visible; no row deleted at request
time.

### 4.3 Append

Kernel resolves `O_APPEND` under its inode lock; the engine "never chooses an
end position on this path" (src:mp/operations/write.rs:14-16). Overlay flag
`appended` (window starts at or after the pre-job length): for a never-shrunk
layer no row shape is read at all (src:ov/src/namespace/compound.rs:197-216;
stream.rs:55-57). SDK `Position::End` resolves against the size current in the
publishing job (doc:engine/workspace/operations.md:98). Appends of a
Workspace-created file are followed by a BLAKE3 state for relink
(doc:engine/overlay/payload.md:91-98).

### 4.4 Overwrite in the middle

Same as §4.1: only the window's cells/runs are written; neighbors stay
inherited. Optionally, **recognition** turns written whole chunks that equal
the file's own committed chunks into `WriteClaims`, stored as reference
pieces (no bytes) (src:ws/operations/file/recognize.rs:1-21;
src:ov/src/payload/pieces/rows.rs:95-150). Bounds: writes shorter than one
minimum chunk (8 KiB) are not examined; 1 MiB of consecutive unclaimed bytes
stops examination of a file; ≤34 claims per job (recognize.rs:34-38;
types.rs:41-45).

### 4.5 Rename-over

One compound job: unbind source, bind destination, dereference the replaced
inode (nlink−1), parent counts (doc:engine/workspace/operations.md:34). A file
renamed over a file sets `Changes.predecessor = (moved, replaced)`
(types.rs:151-154). `record_predecessor`: if the replaced file is base-held,
`INSERT OR REPLACE (moved, replaced)`; if it is fresh (born above floor),
inherit its predecessor (chain collapses to the oldest base file); then delete
the replaced file's row (src:ov/src/namespace/predecessor.rs:1-43). Commit
reads `captured_predecessors` to pick a chunk delta base (predecessor.rs:45-72).
No bytes are moved; a still-open replaced file becomes an orphan (§4.7).

Directory rename: O(1) rows; `moved_directory` updates the retained kernel
parent index in memory "without enumerating open handles"
(types.rs:132-134; compound.rs:258-269). Cross-parent directory moves need
ancestry evidence (kernel: retained parent index) (operations.md:39).

### 4.6 Hard links and symlinks

- `Link`: nlink+1 and a new name; regular files and symlinks only, never
  following a symlink (src:ws/operations/namespace/create.rs:107-144).
  "Aliases share one serial, so one kernel inode"; FUSE inode number =
  serial+1, root = 1 (doc:engine/kernel-routes/README.md:133;
  doc:engine/kernel-routes/fuse.md:121). Removing one name keeps the others
  (operations.md:43).
- `Symlink`: target stored as one payload cell in the same transaction; "A
  symlink target is at most one cell and never changes after creation"
  (create.rs:72-100); FUSE limit: shorter than one page
  (doc:engine/kernel-routes/README.md:164); mode fixed 0o777 (schema.sql:7).

### 4.7 Unlink of an open file (orphans)

Last unlink with a holder: `orphans[s] = {root, top=active, floor=installed}`
and one `fs_inode(s, -1)` row "metadata and size, no byte copy"; ORPHAN steps
move one page of the newest lower layer into generation −1 at a time
(`ROW_MOVE` is an `UPDATE ... SET generation`, statements.rs:56); an orphan
reads its own domain plus at most two retained lower layers over its own base
root (doc:engine/overlay/lifetimes.md:66-86). Unheld: `SERIAL_RETIRE` queued.
A removed inode keeps a row with nlink 0 so serial-addressed access does not
fall through to the base (operations.md:38).

---

## 5. Capture and install (overlay side)

1. **Seal.** `capture()` (generation.rs:79-117): waits (lock released) until no
   reply ticket is pending and no fold is running; later mutations queue behind
   it (`behind_frontier`). Then `captured = active`, `active += 1`,
   `captured_revision = revision`, dirty counts 0. No row copied.
2. **What Commit reads.** A `CapturedReader` over generation `g` only, through
   the capture indexes: `INODE_CAPTURE` (`fs_inode_capture`), `DIRECTORY_ENTRY_CAPTURE`
   (`fs_entry_capture`), pages of ≤1,024 rows in key order; captured run steps
   of ≤128 KiB (`Continue`, `Gap(Inherited|Zero)`, `Window`, `Local`,
   `Reference`, `End`); `captured_digests`, `captured_predecessors`
   (statements.rs:26-31, 138-143; doc:engine/overlay/payload.md:100-116;
   doc:engine/commit/README.md:61). The reader is "valid across install and
   failure until its exact release" (doc:engine/overlay/README.md:124).
   Construction scratch lives in `commit_record` (doc:engine/commit/construction.md:92-95).
3. **Rebind base without a tree walk.** `prepare_base_install` opens the new
   root with no lock ("No whole-tree validation/import/materialization is
   hidden in this step", src:ws/workspace/install.rs:23-47);
   `install_prepared_base` takes the base write lock, runs the one SQL step
   `overlay.install(...)`, swaps the `BaseView` pointer; nothing fallible after
   (install.rs:53-86). Install requires `base_readers == 0`
   (generation.rs:29-34).
4. **Post-capture writes stay on top.** Rows of generation `g+1` are untouched;
   the view floor moves from `i` to `g` (doc:engine/commit/README.md:70-71). An
   mtime-only capture carries its live row into `g+1` (`keep_live_row`,
   inode.rs:86-120).
5. **Reclamation after reply.** `install` queues `RETIRE g`; the Commit's
   releases use `call_deferred`, and the daemon drains after the Commit's
   answer on a `layerfs-reclaim` thread, one `cleanup_task` step per lock hold
   (14 cells / 64 rows per step), skipping what an orphan reads; FOLD/RETIRE of
   a generation a captured reader holds is parked `ready = 0`
   (doc:engine/overlay/lifetimes.md:88-131).

---

## 6. Bounds and complexity

Notation: `w` = request bytes (READ ≤128 KiB, WRITE ≤128 KiB on FUSE /
≤256 KiB per job), `F` = file size, `N` = inodes in the committed tree,
`D` = entries in one directory, `R` = overlay rows (changes so far),
`L` = local layers of the file (≤4 read, `LIMIT 4`), `h` = shrink staircase
height.

| Operation | Overlay work (one job unless noted) | Base work | O(1) in tree size? | O(1) in file size? |
| --- | --- | --- | --- | --- |
| Mount | copy fixed-size template; one open job | one snapshot + one root object | yes | n/a |
| LOOKUP / GETATTR | 1-2 index seeks: O(log R) | if no local row: directory and inode-table B+tree descent O(log D + log N) pages [inferred from B+tree layout], LRU-cached | no SQL scan; base is logarithmic [inferred] | yes |
| READ w | `LAYERS` (≤4 rows) + per layer one `CELL_RANGE` over ≤ w/4K+8 rows, staleness O(log h) per old-epoch row, ≤34 piece rows: O(w + L·log R) | one inherited span: mapping pages O(log F) [inferred] + O(w) chunk bytes (whole-file objects <128 KiB read whole) | yes | overlay yes; base logarithmic [inferred] |
| WRITE w | inode row + ≤ w/32K+2 row upserts + ≤2 partial-cell point reads; shapes read skipped for appends: O(w + log R) | none (recognition: bounded mapping-page reads, optional) | yes | yes |
| Truncate | one row ≤32 KiB + one step + O(log h) binary search; reclamation deferred | none | yes | yes (discarded rows reclaimed later in 14-cell/64-row steps) |
| CREATE / UNLINK / RENAME / LINK | ≤4 inode rows, ≤2 name rows, ≤1 cell, one transaction | name/inode facts | yes | yes (rename of any size file or directory moves no bytes, walks no subtree) |
| READDIR window | ≤1,024 local names (active + captured) | ≤1,024 base names per page | yes per window | n/a |
| Capture | in-memory state change, no rows | none | yes | yes |
| Install | one SQL step + enqueue RETIRE | one root object (prepared) | yes | yes |
| Kernel invalidation (merge root only) | — | inode-table difference walk, streaming | proportional to the merged difference | — |
| RETIRE / STALE / ORPHAN | per step ≤14 cells / 64 rows, off the request path | none | proportional to rows reclaimed | — |

Supporting limits: windows "bound one job, not a file or Workspace";
"Total files, names, edits or Workspace size: No cap"
(doc:engine/overlay/README.md:180; doc:engine/workspace/README.md:156-157);
largest offset `i64::MAX` (README.md:158); base demand ≤4,096 ids, 16 MiB/object,
32 MiB total (README.md:161); shared base cache 64 MiB per daemon
(lifecycle.rs:77-83).

---

## 7. Known limits

- **Open state uncapped.** "Grows with open files, lookups, readers and kernel
  references; no cap (the one resident exception)"
  (doc:engine/overlay/README.md:181). Maps are `BTreeMap`s (journal.rs:6-9), so
  per-access cost is O(log n) in held references **[inferred]**. The NFS route
  instead caps its synthetic kernel at 65,536 lookups, 1,024 files, 256
  directories (doc:engine/kernel-routes/README.md:171).
- **FLUSH/FSYNC/FSYNCDIR = ENOSYS**, sticky per connection: "the kernel answers
  every later one itself, with success"; nothing is buffered and no durability
  is claimed (src:fu/request/callbacks.rs:274-285). The overlay itself runs
  `synchronous=OFF`, `journal_mode=MEMORY` and "is disposable and never
  synchronized to stable media" (doc:engine/kernel-routes/README.md:138-140).
  Consequence **[inferred]**: `fsync` returning success does not mean durable;
  durability is defined only at Commit.
- **Not implemented (ENOSYS):** xattrs, ACCESS, READDIRPLUS, locks, BMAP, IOCTL,
  POLL, LSEEK, FALLOCATE, COPY_FILE_RANGE; FIFOs, sockets, devices, foreign
  uid/gid → `EPERM` (doc:engine/kernel-routes/README.md:96). For fallocate and
  copy_file_range "the kernel remembers the answer and copies through ordinary
  READ and WRITE"; for ACCESS/READDIRPLUS/locks "the kernel keeps them local or
  falls back" (callbacks.rs:609-610, 641-642). **[inferred]** POSIX locks
  therefore work only within the mount's kernel, not coordinated by the daemon.
- **No atime/ctime storage:** one stored mtime shows as atime, mtime, ctime,
  crtime; mount is `noatime` (doc:engine/kernel-routes/fuse.md:124;
  src:mp/operations/write.rs:27-29).
- **Kernel writeback off.** `FUSE_WRITEBACK_CACHE` never selected; flags are
  only `ASYNC_READ | BIG_WRITES (| MAX_PAGES)`; `max_write = max_readahead =
  128 KiB`; `max_background = 1` (src:fu/mount/profile.rs:31-46). Consequences:
  every `write(2)` is a synchronous FUSE WRITE published (one overlay
  transaction) before it returns (doc:engine/kernel-routes/README.md:153); a
  FUSE WRITE is never larger than 128 KiB, so on FUSE each WRITE is exactly one
  job (WRITE_WINDOW 256 KiB is the NFS transfer size, types.rs:30-35)
  **[inferred]**; small writes are not coalesced by the kernel, so a stream of
  tiny writes costs one job each **[inferred]**; dirty shared-mmap pages not
  yet sent as WRITE are outside a capture — "a command's exit is not a flush
  fence" (doc:engine/commit/README.md:79-81); mapped stores are clipped to the
  current size and never extend a file (src:ws/operations/file/write.rs:42-47).
- **Name and target sizes:** names ≤255 B; symlink target < one page.
- **Admission:** daemon `namespaces` Workspaces; one more is `AdmissionFull`
  (doc:engine/overlay/README.md:182).
- **Failure model:** one attempt, `busy_timeout=0`, no replay; an unknown
  outcome quarantines the overlay (README.md:171, 184); a full device rolls the
  job back and replies ENOSPC (README.md:183; kernel-routes README.md:166).
- **SQLite versions pinned** to 3.51.0 / 3.53.2 (profile.rs:100-106).

---

## Workspace overlay and request path (paper-ready appendix, ~900 words)

**Overlay database.** Each mounted Workspace owns one SQLite file, created by a
byte copy of a schema-only template built at daemon start; mount therefore
costs a fixed-size file copy and one root-object read, independent of the tree.
The file runs a disposable profile (`journal_mode=MEMORY`, `synchronous=OFF`,
`locking_mode=EXCLUSIVE`, 4 KiB pages, no mmap, `busy_timeout=0`) and holds
only changes. `fs_inode(inode, generation)` stores one row per changed inode
per *generation*; `fs_entry(parent, name, generation)` stores one row per
changed name, with a NULL inode as a whiteout. File bytes live in `fs_data` as
4 KiB cells (with a per-byte validity mask, so a written zero differs from a
hole) or dense runs of whole cells inside one aligned 32 KiB slot.
`fs_data_ref` records *reference pieces*: ranges declared equal to committed
bytes of a content root, holding no bytes. `fs_truncate` holds a shrink
staircase, `fs_predecessor` the file a rename replaced (an advisory delta base
for Commit), `fs_digest` append-only BLAKE3 digests, and `cleanup_task` the
reclamation queue. All but the payload and the two single-key tables are
`WITHOUT ROWID`; `fs_data` stays a rowid table so live runs can be overwritten
in place with incremental blob I/O.

**Layered view.** A Workspace's view is the committed base root plus overlay
rows with `installed < generation ≤ active`. Each inode row carries
`inherited_cutoff`: bytes below it not written in that generation fall through
to the next lower layer and finally to the base file. Reads compose at most
four local layers newest-first and return the locally decided bytes, an
*inherited* bitmask with one covering span, and reference ranges; the request
then reads the span and ranges from the immutable Store *after* releasing the
Workspace lock.

**Request path.** All state of one Workspace sits behind one mutex. A job holds
it for at most one transaction and performs no Store I/O; base facts a job
lacks are returned as `Needs`, read without the lock, and supplied to a second
job that decides over the then-current rows. Kernel lookup counts, open
descriptors, directory cookies and read windows live in in-memory maps that
commit or roll back with the job's SQL; nothing about kernel references is
written to disk. Windows bound each job, not files: 256 KiB per write job
(FUSE negotiates 128 KiB), 128 KiB per read, 1,024 rows per page.

```
Listing 1: WRITE(serial, off, data)            // |data| <= 128 KiB on FUSE
  claims := recognize(serial, off, data)       // no lock; optional, bounded
  facts := {}
  loop:
    lock(W)                                    // one job, <= 1 transaction
      old := newest row of serial in (installed, active], else facts.base
      if old unknown: unlock; facts += read_base_inode(serial); continue
      new := old with size = max(old.size, off+|data|), mtime = now
      BEGIN IMMEDIATE
        if no row (serial, active): insert layer, cutoff := old.size
        for each 32 KiB slot s overlapping [off, off+|data|):
          whole cells -> overwrite live dense row in place, or replace with one run
          partial cell -> point read + merge + set validity bits (no base read)
        claimed whole cells -> fs_data_ref piece (no bytes)
        revision += 1; issue reply ticket
      COMMIT                                   // with in-memory journal
    unlock; reply(|data|); reply_attempted(ticket); return
```

```
Listing 2: READ(serial, off, len)              // len <= 128 KiB
  lock(W)                                      // read-only job, no BEGIN
    layers := newest <= 4 rows of serial in (installed, active]
    for L in layers:                           // newest first
      copy live cells/runs of L into undecided bytes
      if L.referenced: mark bytes covered by L's pieces as reference ranges
      mark bytes >= L.cutoff (below EOF) decided (zero if unwritten)
    root := base root at this instant
  unlock
  if every byte decided locally: reply(bytes)
  base := take Store reader                    // outside the lock
  fill inherited span from base file at root; fill reference ranges
  reply(bytes)
```

**No copy-up.** The first write to a committed file inserts one inode row whose
cutoff is the base length and stores only the written cells; no base byte is
read or copied, and partially written cells keep unwritten bytes inherited via
their masks. A truncate edits at most one 32 KiB row, adds one staircase step
(stale cells are identified by epoch through a binary search) and lowers the
cutoff; discarded rows are reclaimed later. Appends past the old length skip
even the row-shape read. Renames, including directory renames and rename-over,
change at most four inode rows and two name rows and move no bytes; a
rename-over records the replaced file as a predecessor. Hard links share one
serial, hence one kernel inode; a symlink target is one immutable cell.

**Capture and install.** Commit seals the active generation by an in-memory
state change: `active` becomes `captured` and new mutations go to `active+1`;
no row is copied. Capture waits until every earlier mutation's reply attempt is
recorded, and later mutations wait behind it, so the captured frontier is
finite. Construction reads only the sealed generation through capture indexes
in pages of ≤1,024 rows. After publication, install is one SQL step that sets
the new base root and raises the floor to the captured generation, plus a
pointer swap of the base view; no tree is walked, and rows written after the
capture remain the top of the view, since their cutoffs were taken from the
sealed layer the new root reproduces. Rows of the retired generation are
deleted after the Commit replies, in steps of at most 14 cells or 64 rows.

**Complexity.** Table O-1 summarises per-request cost. Overlay work grows with
the request size and logarithmically with the number of *changed* rows; base
work is a B+tree descent and is shared through a 64 MiB per-daemon object
cache.

| Operation | Overlay cost | Base cost | Tree size | File size |
| --- | --- | --- | --- | --- |
| Mount | fixed template copy | one root object | O(1) | — |
| LOOKUP / GETATTR | O(log R) seeks | O(log D + log N) pages if unchanged | logarithmic | O(1) |
| READ w | O(w + L log R) | O(w) + O(log F) mapping pages | O(1) | logarithmic (base only) |
| WRITE w | O(w + log R) | none | O(1) | O(1) |
| Truncate | one ≤32 KiB row + O(log h) | none | O(1) | O(1) |
| CREATE/UNLINK/RENAME/LINK | ≤4 inode + ≤2 name rows | name facts | O(1) rows | O(1) |
| Capture / Install | in-memory / one SQL step | one root object | O(1) | O(1) |

(R changed rows, N inodes, D directory entries, F file size, L ≤ 4 layers,
h staircase height. Logarithmic base costs are inferred from the B+tree layout.)

**Limits.** Kernel writeback caching is off, so each `write(2)` is a
synchronous, published FUSE WRITE and dirty shared-mapping pages not yet sent
are outside a capture. FLUSH/FSYNC answer ENOSYS once (the kernel then reports
success): the overlay is disposable and durability exists only at Commit.
Extended attributes, READDIRPLUS, remote locks, fallocate and copy_file_range
are not implemented. Per-Workspace open state (lookups, descriptors, readers)
is the one resident structure with no cap.
