# 02 — Committed base plus live overlay

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Part of the [cluster two design set](README.md). Written 2026-10-05 against
> `main` `f96d97651`. The schema and algorithms are proposals; statement and
> page figures are design targets and SQLite file-format arithmetic, not
> measurements. Claim labels are defined in the
> [entry point](README.md#claim-labels).

## 1. Model

[proposed design]

```text
                         Workspace operation
                                 |
                      semantics / read planning          layerfs-workspace
                                 |
                +----------------+----------------+
                |                                 |
          committed base                     live overlay
          root R, immutable                  generations A (active) and C (captured)
          canonical objects                  rows in ws-<n>.db
                |                                 |
          layerfs-content                    layerfs-overlay
          over the base client               one connection, one mutex
                |                                 |
          store host, global SQLite          container-local file
                +----------------+----------------+
                                 |
                  view = A over C over base(R)

  Commit input            =  C over base(R)
  after a known Commit    =  A over base(R'),  where base(R') == C over base(R)
```

**Base identity and binding.** A Workspace is opened on a Branch. The daemon
obtains `HistoryCatalog::branch_snapshot` through the store host and keeps, in
memory, the `BaseBinding`: Branch, head Commit, base Layer, effective root `R`,
allocation scope, profile, and the root directory serial (read from the root
object; it is not a field of `BranchSnapshot`,
`core/crates/layerfs-history/src/records.rs:58-73`). The binding is replaced
only by install. The base needs no lease: cluster one has no deletion path, so
a root stays readable ([source-verified] pack rows are immutable by trigger,
`core/crates/layerfs-persistence/sql/sqlite/objects.sql:9-12`). The day a
collector exists, a lease becomes mandatory; that is recorded in
[06 §7](06-cluster-one-integration.md#7-failure-boundaries).

**The overlay never copies the base and never enumerates it.** Opening a
Workspace creates an empty database. Every row in it is a change.

## 2. Database placement and profile

[proposed design]

One file per Workspace: `<overlay_dir>/ws-<n>.db`, in a daemon-private
directory (mode 0700) on the container's own disk or a named volume. Never on
the FUSE mount; never on a bind mount from macOS. At start the daemon deletes
every file in that directory. It never opens a database it did not create in
this process.

| Setting | Value | Reason |
| --- | --- | --- |
| Connections | Exactly one per database, owned by the Workspace, behind `Workspace.core` | One page cache, never invalidated by another connection; no `SQLITE_BUSY` is possible |
| `locking_mode` | `EXCLUSIVE` | The file lock is taken once; no lock system call per transaction |
| `journal_mode` | `MEMORY` | Rollback works inside the process, so a failed statement or a full disk rolls one transaction back. No journal file, no write-ahead log, **no checkpoint** |
| `synchronous` | `OFF` | Root `AGENTS.md` §4 forbids `fsync` on Workspace backing. This is the only setting under which SQLite issues no sync call |
| `page_size` | 4,096 (starting value; frozen by the count diagnostic of [07](07-implementation-validation.md)) | Smallest rewrite for small rows |
| `cache_size` | A fixed byte allowance per Workspace, 8 MiB to start | Bounds the pager heap. It is not a bound on OS page cache (§12) |
| `auto_vacuum` | `NONE` | Freed pages are reused before the file grows. The file is unlinked at close |
| `mmap_size` | 0 | Reads stay on the ordinary path |
| `temp_store` | `MEMORY` | Every query is a keyed seek or a keyset page |
| `secure_delete`, `foreign_keys` | `OFF` | The bundled build defaults `foreign_keys` on; set it explicitly |
| `max_page_count` | The Workspace quota in pages, when a quota is configured | Enforced by SQLite at page allocation |
| `application_id`, `user_version` | Set | Schema identity |
| Tables | `STRICT` | — |

Every setting is applied and read back at open; a value that does not read back
is a startup failure, not a substitute.

**Declared profile.** Runtime atomicity of each SQL transaction while the
daemon lives. Nothing across a daemon crash, an OS or VM crash, or power loss.
A Workspace does not survive its daemon. This is the shape of cluster one's own
Disposable profile.

**Alternative if a Workspace must survive a daemon crash (owner question
O-3):** `journal_mode = WAL` with `locking_mode = EXCLUSIVE`, still one
connection. Payload is then written twice, and the write-ahead log needs a
size-triggered `PASSIVE` checkpoint run by the maintenance step, never by a
mutation. Every other part of this design is unchanged. The prepared profile
(a writer plus reader connections) is not carried forward: other connections
see only committed data and reset their page cache after every commit.

**Linkage.** `rusqlite =0.40.2` with the `bundled` feature, which compiles
SQLite 3.53.2 and adds no new package (`cc` is already locked). It must be
enabled only for the Linux daemon so that the host's linked SQLite, which is
part of every cluster one receipt's binary scope, does not change. Not
established: that it builds for `aarch64-unknown-linux-musl`.

## 3. Schema

[proposed design]

```sql
CREATE TABLE ws (
  id     INTEGER PRIMARY KEY CHECK (id = 1),
  folded INTEGER NOT NULL,      -- rows with gen <= folded are already part of the base
  frozen INTEGER,               -- the captured generation C; NULL when none is held
  active INTEGER NOT NULL,      -- the generation A every mutation writes
  CHECK (folded >= 0 AND active > folded),
  CHECK (frozen IS NULL OR (frozen > folded AND frozen < active))
) STRICT;

CREATE TABLE inode (            -- one row per (inode, generation that changed it)
  ino       INTEGER NOT NULL,   -- the cluster one serial; also st_ino
  gen       INTEGER NOT NULL,
  kind      INTEGER NOT NULL CHECK (kind IN (1, 2, 3)),   -- file, directory, symlink
  mode      INTEGER NOT NULL,
  nlink     INTEGER NOT NULL CHECK (nlink >= 0),          -- 0: no name is left
  size      INTEGER NOT NULL CHECK (size >= 0),
  mtime_s   INTEGER NOT NULL,
  mtime_ns  INTEGER NOT NULL CHECK (mtime_ns BETWEEN 0 AND 999999999),
  born      INTEGER NOT NULL,   -- generation that created the serial; 0 = it exists in a base
  stream    INTEGER,            -- payload stream this generation wrote; NULL = none
  lower_len INTEGER NOT NULL CHECK (lower_len >= 0),      -- leading bytes still taken from below
  target    BLOB CHECK (target IS NULL OR length(target) <= 4096),  -- symlink target
  pinned    INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1)),    -- kept for an open, unlinked file
  PRIMARY KEY (ino, gen)
) STRICT, WITHOUT ROWID;

CREATE TABLE dentry (           -- one row per (name, generation that changed it)
  parent INTEGER NOT NULL,      -- serial of the directory
  name   BLOB NOT NULL CHECK (length(name) BETWEEN 1 AND 255),
  gen    INTEGER NOT NULL,
  ino    INTEGER,               -- NULL = the name is removed (whiteout)
  kind   INTEGER,               -- d_type of the target, so a listing needs no inode read
  below  INTEGER NOT NULL CHECK (below IN (0, 1)),   -- a lower layer binds this name
  PRIMARY KEY (parent, name, gen)
) STRICT, WITHOUT ROWID;

CREATE TABLE extent (           -- payload: non-overlapping byte runs of one stream
  id     INTEGER PRIMARY KEY,   -- rowid, required for in-place BLOB I/O
  stream INTEGER NOT NULL,
  off    INTEGER NOT NULL CHECK (off >= 0),
  data   BLOB NOT NULL CHECK (length(data) BETWEEN 1 AND 131072)
) STRICT;
CREATE UNIQUE INDEX extent_at ON extent (stream, off);

CREATE TABLE reclaim (          -- streams no inode row refers to any more
  seq    INTEGER PRIMARY KEY,
  stream INTEGER NOT NULL
) STRICT;
```

Five tables. What is deliberately **not** a table:

| State | Where | Why |
| --- | --- | --- |
| Base binding, capture context, stage token, candidate root | `Workspace` memory | The database dies with the daemon (§2), so nothing reads these back |
| Serial allocator `[next, end)`, stream allocator | `Workspace.core` memory | Same; they change under the same mutex as the rows |
| Open counts, unlinked-open records | `Workspace.core` memory | Bounded by the commands' descriptor limit |
| Directory cursors | The directory handle | One reply per handle |
| xattrs | Nothing | The mount refuses them, as today |

Key choices:

- **`(ino, gen)` and `(parent, name, gen)`.** The newest row of a key is one
  descending seek, whatever the number of generations. A generation's rows are
  found by a keyset scan that skips the other generation's rows; with at most
  two live generations (§4) that is at most twice the rows of the change.
- **`stream` indirection.** Payload rows carry a stream number, not an inode
  and a generation. Truncating a file to zero, relabelling an inode row to
  another generation, and keeping an unlinked file alive never rewrite payload.
- **`extent` is a rowid table with a unique index.** SQLite's incremental BLOB
  I/O requires a rowid table, and large BLOBs in a `WITHOUT ROWID` tree destroy
  its fan-out. The price is two B-tree descents per extent access.
- **`name` is a BLOB.** `memcmp` order is cluster one's `PathName` order, so
  overlay rows and base listings merge without re-sorting.

## 4. Generations and visibility

[proposed design]

A generation is an integer label. Three numbers in the `ws` row describe a
Workspace: `folded < [frozen <] active`.

| Rule | Statement |
| --- | --- |
| Write | A mutation inserts or updates rows at `gen = active`. It never touches a row of another generation |
| Visibility | For a key, the view takes the row with the greatest `gen`, if that `gen > folded` or the row is `pinned`. Otherwise the key falls through to the base |
| Capture | `UPDATE ws SET frozen = active, active = active + 1 WHERE frozen IS NULL` |
| Install | `UPDATE ws SET folded = frozen, frozen = NULL`, with the base binding replaced in the same critical section |
| Retire | Rows with `gen <= folded` and `pinned = 0` are deleted in bounded steps |
| Fold | After a Commit that did not succeed, the captured rows are relabelled into the active generation before the failure is returned ([04 §8](04-concurrency-commit.md#8-fold-after-a-commit-that-did-not-succeed)) |

Captured rows are immutable **because of their key**: after capture no writer
can produce `gen = frozen` again. No lock protects them.

**How many versions can be live.** At most **two per key**: one in the captured
generation while a Commit is in flight, one in the active generation. One
Commit per Workspace and fold-after-failure make that a hard bound. The single
exception is an unlinked file that is still open: its `pinned` rows stay until
the last close ([04 §7](04-concurrency-commit.md#7-open-unlinked-files)).
Retired rows awaiting deletion are invisible garbage, bounded in
[03 §7](03-mutation-hot-path.md#7-maintenance).

## 5. Payload: extents

[proposed design]

**Rules.**

1. An extent is a run of bytes `[off, off + length(data))` of one stream.
2. Extents of one stream never overlap.
3. An extent is at most 128 KiB, the mount's `max_write`, so one WRITE request
   creates at most one new row. (Candidates 64 KiB and 128 KiB are counted
   before freezing.)
4. An extent is never trimmed, split or moved. Bytes inside it are replaced in
   place.
5. A position with no extent, at or beyond `lower_len`, reads as zero. A
   position with no extent below `lower_len` reads from the layer below.

**A write never reads the base.** This is the central property. The prepared
design used a fixed block grid with copy-up: the first partial write to a base
block read that block from the base. The base is now behind a bridge call to
another operating system. A one-byte overwrite must not wait for it.

```text
WRITE [a, b) to inode i
  1. top := newest visible inode row of i          (overlay, else the base attributes)
  2. if top.gen != active:  upsert the row at active, lower_len = top.size, stream = NULL
  3. if stream IS NULL:     stream := next stream number
  4. overlapped := extents of the stream that intersect [a, b)
        - the last extent with off <= a            (one descending seek)
        - extents with a < off < b                 (one range scan, bounded by the request)
  5. for each covered part:  write the new bytes into that extent in place   (BLOB write)
     for each gap:           INSERT one extent holding exactly the gap's bytes
  6. upsert the inode row:   size = max(size, b), mtime
  all in one transaction; reply after COMMIT
```

Work is proportional to the bytes written plus the extents they touch. It does
not depend on the file size, on earlier edits elsewhere in the file, or on the
Workspace.

**Small appends.** A write that begins exactly where the stream's last extent
ends, when that extent is stored inline in its leaf and the combined length
still fits inline (about 3.9 KiB at 4 KiB pages), rewrites that one row instead
of adding a row. Ten thousand 100-byte appends therefore produce about 260 rows
of one megabyte, and each append rewrites one leaf cell.

**The alternative considered.** One extent per 128 KiB cell with spare
capacity, filling the gap from below when a second write in the same cell is
disjoint from the first. It has fewer rows for scattered writes, but that gap
fill is a base read on the mutation path, bounded by a cell. A database file
updated one 4 KiB page at a time would pay it on the second touch of every
cell. It is rejected for that reason.

**Truncate.**

| Request | Work |
| --- | --- |
| To zero (`O_TRUNC`, the common case) | Upsert the inode row with `size = 0`, `lower_len = 0`, `stream = NULL`; insert the old stream into `reclaim`. Three statements whatever the file size. The next write takes a fresh stream |
| Grow | Upsert `size`. The new range has no extent and lies at or beyond `lower_len`, so it reads as zero |
| Shrink to `n > 0` | Upsert `size = n`, `lower_len = min(lower_len, n)`; zero in place the tail of the extent that straddles `n` (at most 128 KiB); delete extents with `off >= n`. Up to 64 rows are deleted in the same transaction. Beyond that the inode is marked busy and the rest is deleted in bounded transactions before the reply, so other inodes proceed in between |

`lower_len` only ever decreases within a generation. Bytes cut off by a shrink
can therefore never reappear when the file grows again.

**Read plan.** All overlay reads of one request happen in one critical section,
so they are one consistent snapshot:

```text
READ [a, b) of inode i, clamped to the newest row's size
  rows := visible inode rows of i, newest first      (at most 2, plus a pinned one)
  want := [a, b)
  for row in rows:
     copy the row's extents that intersect `want`; remove those ranges from `want`
     ranges of `want` at or beyond row.lower_len become zeros; remove them
  release the mutex
  whatever is left in `want` is read from the base file of i     (base client, §7)
```

A range the Workspace has overwritten never causes a base request.

**Holes.** A sparse write at a 10 GiB offset is one extent row and one inode
upsert; no row represents the hole. The cost reappears at Commit: cluster one
has no hole segment, so a hole is committed as zeros run through the chunker
([06 §6](06-cluster-one-integration.md#6-prerequisites-outside-cluster-two)).

**Storage arithmetic.** [proposed design; file-format arithmetic, not a count] At 4 KiB
pages a row keeps about 489 bytes in its leaf and the rest in overflow pages of
4,092 bytes each, so a 128 KiB extent occupies about 33 pages (about 3%
overhead) and a file under about 4 KiB is stored inline with no overflow page.
A 4 KiB block on 8 KiB pages, the prepared starting pair, fits one block per
leaf: about half of every leaf is empty.

## 6. Names and inodes

[proposed design]

**Lookup `(parent, name)`.** If the overlay holds no `dentry` row at all (a
counter in `Workspace.core`), no SQL runs. Otherwise one descending seek on
`(parent, name)`. A row with `ino IS NULL` is a miss. No visible row: ask the
base, unless `parent` was created in the overlay and is not yet committed
(`born > folded`), in which case it has no base children and the answer is a
miss without leaving the daemon.

**Attributes of `ino`.** One descending seek on `(ino)`. No visible row: the
base attributes (§7).

**Enumeration.** A merge of two name-ordered keyset sequences: the overlay's
`dentry` rows for the directory and the base listing page
(`FilesystemRead::list_inode(serial, after, max_entries, max_bytes)`). A
whiteout drops a base name. The cursor is the last name returned. Memory is one
page of each sequence; a directory of any size is listed with bounded memory.

**Mutations.** Each is one transaction.

| Operation | Rows written |
| --- | --- |
| create, mkdir, symlink | `dentry` insert (with `below` from the existence check the operation already made); `inode` insert with a serial from the in-memory range and `born = active`; parent `inode` upsert for its mtime |
| unlink | If `below = 0` and the row is in the active generation: delete the `dentry` row. Otherwise upsert a whiteout. Target `inode` upsert with `nlink - 1`; parent upsert |
| A file that loses its last name and is not open | If `born = active` and it has no captured row: delete the `inode` row. Otherwise keep the row with `nlink = 0`, `stream = NULL`, `lower_len = 0`. In both cases its stream goes to `reclaim` |
| rmdir | As unlink, after the emptiness check below |
| rename | Source: delete or whiteout as unlink. Destination: upsert pointing at the **same serial**. A replaced destination loses a link as in unlink. Both parents upserted. No descendant row is created: a moved directory's children are still found by its serial, in the overlay and in the base |
| link | `dentry` insert; target `inode` upsert with `nlink + 1`; parent upsert |
| chmod, utimens | `inode` upsert |

A file created and removed inside one generation leaves no row. A removed and
recreated directory gets a new serial, so it has no base children and needs no
opaque marker.

**Directory emptiness** (rmdir, rename over a directory). A directory born in
the overlay is checked with one seek. A base directory is empty only if every
base name has a whiteout; the check enumerates the merged view and stops at the
first visible child. The directory is marked busy for the check so no child can
be created underneath it. `rm -rf` of a base directory of N entries costs N
unlinks and one O(N) check, so the check is amortised constant per removed
entry.

**Refused at the mutation, not at Commit.** Cluster one's inode model is
narrower than POSIX. Anything it cannot express is refused when the command
asks, so a Commit never fails later for a reason the command could have been
told:

| Request | Result | Cluster one source |
| --- | --- | --- |
| Name that is not UTF-8, or contains `\` | `EILSEQ` | `core/crates/layerfs-content/src/filesystem/path.rs:178-196` |
| Name over 255 bytes; symlink target over 4,096 bytes | `ENAMETOOLONG` | `filesystem/limits.rs:12`, `:43` |
| `mknod` of a device, FIFO or socket | `EPERM` | `object/inode_leaf.rs:50-58` |
| setuid, setgid or sticky bit on a file | `EPERM` | `filesystem/attributes/portable.rs:28-39` |
| `link` to a directory or a symlink | `EPERM` | `object/inode_leaf.rs:99-112` |
| `chown` | `EPERM`; ownership is the configured command identity | no uid or gid in the inode value |

## 7. Reading the base

[proposed design]

The daemon runs `layerfs-content`'s read code (`FilesystemRead`, `FileView`,
`read_range`) over a **base client** that implements `AuthenticatedObjects`:

```text
read_canonical_batch(ids)
   for each id: BaseCache hit?  -> bytes
   misses -> one ReadObjects(ids) call to the store host
   authenticate every returned object: BLAKE3(domain || bytes) == id
   insert into BaseCache; return in demand order
```

| Need | Calls | Note |
| --- | --- | --- |
| Child by parent serial and name | `FilesystemRead::resolve_child(parent, &name)` | One object per directory-tree level, then the inode-table levels |
| Inode by serial | `resolve_inode(serial)` | Inode leaves hold 50–100 inodes, so neighbours share a fetch |
| Listing page | `list_inode(serial, after, max_entries, max_bytes)` | Keyset; names in byte order |
| Symlink target | `readlink_inode(serial)` | — |
| File bytes | `FileView::open` then `read_range` | A file below 128 KiB is one whole-file object |
| Size, mode, mtime | `Attributes` call to the store host, batched per directory | See below |

**Why an `Attributes` call exists.** A cluster one inode value holds a kind, a
reference count, a content root and a metadata root: no size
(`core/crates/layerfs-content/src/object/inode_leaf.rs:83-93`). The size of a
base file is known only from its file root, which for a file below the cutoff
is the whole file. Without help, a `stat` of every small base file would pull
that file's content across the bridge. The store host answers
`Attributes[(kind, content_root, metadata_root)] -> (length, mode, mtime)`
locally and the daemon caches the answer under those two immutable identities.

## 8. Caches: owner, key, visibility, invalidation

[proposed design]

| Cache | Owner | Key | Holds | Invalidated by |
| --- | --- | --- | --- | --- |
| `BaseCache` | daemon, shared by all Workspaces | `ObjectId` | Authenticated canonical bytes | Nothing, ever. Objects are immutable; eviction is by byte budget |
| Attribute cache | daemon, shared | `(content_root, metadata_root)` | `(length, mode, mtime)` | Nothing; both keys are immutable |
| SQLite page cache | one Workspace | page number | Overlay pages | Nothing; there is one connection |
| Append hint | `Workspace.core`, per recently written inode, bounded | `ino` | The stream's end offset, so a sequential append skips the two overlap queries | Maintained in the mutating critical section; dropped on eviction |
| Kernel dentry, attribute and page caches | kernel, per mount | — | — | [05 §4](05-fuse-assessment.md#4-coherence-a-lifetime-is-not-a-design) |

No cache holds overlay rows outside the SQLite page cache, and no cache is
keyed by root. That is why install needs no cache invalidation and why a new
Workspace on a new head starts warm.

Every cache has a configured byte budget, and every benchmark receipt must
declare its state (root `AGENTS.md` §1–§2): a warm `BaseCache` is setup reuse,
never a cold claim.

## 9. Bounded queries

[proposed design]

| Purpose | Statement shape | Bound |
| --- | --- | --- |
| Newest inode row | `… FROM inode WHERE ino = ?1 ORDER BY gen DESC LIMIT 1` | one seek |
| Newest name row | `… FROM dentry WHERE parent = ?1 AND name = ?2 ORDER BY gen DESC LIMIT 1` | one seek |
| Overlay listing page | `… FROM dentry WHERE parent = ?1 AND name > ?2 ORDER BY name, gen DESC LIMIT ?3` | one page; newest row per name taken while scanning |
| Extent covering an offset | `SELECT id, off, length(data) FROM extent WHERE stream = ?1 AND off <= ?2 ORDER BY off DESC LIMIT 1` | one seek; `length` does not load the BLOB |
| Extents in a range | `… WHERE stream = ?1 AND off > ?2 AND off < ?3 ORDER BY off` | the request range |
| Captured inodes for Commit | `… FROM inode WHERE (ino, gen) > (?1, ?2) ORDER BY ino, gen LIMIT ?3`, keeping rows with `folded < gen <= frozen` | keyset page |
| Captured names for Commit | `… FROM dentry WHERE (parent, name, gen) > (?1, ?2, ?3) ORDER BY parent, name, gen LIMIT ?4` | keyset page; already in cluster one's required order |
| Retirement and reclamation | keyset page, then `DELETE … WHERE id IN (…)` | fixed rows and fixed pages freed per step |

Nothing is materialised: not a directory, not a file, not the change set.

## 10. Limitation inventory

Each old limit is [source-verified] on `main` unless marked. Paths are under
`core/crates/`; "root" paths are under the repository-root `crates/`.

| Old limit | Source | Owning mechanism | User-visible consequence | Replacement | Remaining real limit | Validation that it is gone |
| --- | --- | --- | --- | --- | --- | --- |
| `MAX_EDITS_PER_FILE = 4,096` | root `layerfs-workspace-core/src/file_edit.rs:5` before `ead812e78` (removed 2026-09-12) | Per-file pending-edit counter | The 4,097th edit before a Commit failed | No counter exists | — | 10,240 and 100,000 writes to one file: statements and pages per write flat against the write index |
| `MAX_PIECES_PER_FILE = 8,193` | root `layerfs-workspace-core/src/file_edit.rs:5`, `:1008-1009` | Piece table per file | About 4,096 separated edits of one file refused | Extents are rows; no count | Disk quota | Same test with scattered offsets |
| Emergent edit cap between 8,192 and 10,240 writes | `layerfs-workspace/src/commit/active_reconcile.rs:34-53`, `:120-132`; threshold from `core/docs/issues/286/FAMILY4-CHECKPOINT-20260930.md:29-30` (document, not re-measured) | Post-publication reconcile charges every dirty extent to an 8 MiB budget | Commit refused **after** it was published upstream | Install is one `UPDATE`; nothing is resident per extent | — | The 10,240-write case commits and installs |
| Checkpoint-like work per mutation | `layerfs-workspace/src/backing/active/pages.rs:447-698`; `layerfs-workspace/src/backing/active/generation.rs:679-844` | Each mutation publishes a full copy-on-write index revision: page files created, written, read back, hashed; reclamation and a compaction plan inline | Latency on every write; never measured | One SQL transaction; no file create, no read-back, no reclamation on the path | — | Per-operation counters: zero file creates, unlinks, sync calls and maintenance steps on the acknowledgement path |
| WAL `stat` and inline `PASSIVE` checkpoint after every write | #305 prototype, `core/experiment/real-tree/src/overlay/db.rs:97`, `:136-155` on `codex/phase7-experiment-305` | Size check on the mutating thread; a busy checkpoint failed the mutation | A write could fail because a checkpoint was busy | No write-ahead log exists (§2) | — | The profile reads back `journal_mode = memory` |
| `EditCheckpoint` per SDK edit | root `layerfs-workspace/src/file_io.rs:214-225`; root `layerfs-workspace/src/lifecycle.rs:886` | Clone of the node and the path map for rollback | Cost grows with the Workspace per edit | SQL rollback | — | Not applicable to the mount path |
| One memory budget for all Workspaces, 8 MiB (16 MiB in the sandbox) | `layerfs-workspace/src/types.rs:5`; `backing/budget.rs:22-32`; `layerfs-sandbox/src/docker.rs:209-212` | Charge ledger for resident structures | `ENOSPC` on write, create or lookup; refused Commit | Per-Workspace page cache allowance; state in rows | Configured memory | Two Workspaces: one fills its quota, the other is unaffected |
| Dirty identities and changed names charged per mutation | `layerfs-workspace/src/runtime/state.rs:299-355` | Resident dirty frontier | `ENOSPC` growing with changed files and names | Rows | Disk quota | Install replay of 95,021 entries completes |
| Directory population: 128 entries per page, a charged cookie per listed entry | `types.rs:7`; `filesystem/directory.rs:18-27`; `runtime/state.rs:19` | Per-handle cookie maps | `ENOSPC` while listing large directories | Cursor of one reply per handle | — | 100,000-entry directory listed with bounded memory |
| 128 open handles | `runtime/state.rs:17`; `filesystem/open.rs:97-103` | Fixed handle table | The 129th open fails with `ENOSPC` | A per-inode open count | Descriptor limit of the commands | 10,000 simultaneously open files |
| 32 captures, 160 pinned revisions, 32 metadata roots | `backing/active/index.rs:23-24`; `backing/metadata.rs:25` | Fixed arrays | `Busy` or `Capacity` | Two generations | — | — |
| Index depth 7; key 272 bytes; value 512 bytes | `backing/active/keyed.rs:10-14` | Hand-built tree | `Capacity` | SQLite B-tree | SQLite database size | — |
| 4 GiB per file | `layerfs-bridge/src/contract/request.rs:16`; `filesystem/write.rs:219-221` | Bridge contract constant | `ENOSPC` past 4 GiB | Offsets are 64-bit integers; no file-size constant on the Workspace path | Disk quota; owner question O-11 | A 5 GiB sparse file: write at the end, read back |
| 1 GiB private backing | `layerfs-sandbox/src/docker.rs:207-216` | Launch constant | `ENOSPC` | A configured quota, or none | Disk | — |
| A write of 129 bytes allocates 8 KiB | `backing/segments.rs:25-37` | One file per write, 4 KiB header | Quota consumed by small writes | Inline rows | — | Storage growth reported in bytes for the small-write cases |
| Commit input: prepared stream at most 256 MiB | `layerfs-bridge/src/contract/prepared_stream.rs:58` | Whole stream sized up front | Large Commit refused | Keyset pages into cluster one's row contract | One directory's changed names in one `Vec` (cluster one, §11) | 95,021-entry Commit |
| One upstream call at a time per daemon | `layerfs-workspace/src/runtime/host.rs:37-40`; `layerfs-workspace/src/runtime/state.rs:690-698` | Atomic flag | `EBUSY` for a second base read | Upstream pool; calls wait | Configured pool size | Two concurrent base reads |
| Mutation refused while another callback replies | `runtime/coherence.rs:482-493` | Reply permits | `EBUSY` returned to applications | Wait on the Workspace mutex | — | Four Execs writing: zero `EBUSY` |
| Whole node-table scan on every FORGET and close | `runtime/state.rs:429-465` | Resident node table | Cost grows with referenced inodes | Lookup-count map | Kernel inode cache size | — |
| One host round trip per created inode | `filesystem/active_create.rs:119-128` | No local allocator | Latency per create | Reserved range | Serial space `1 ..= i64::MAX` | Zero upstream calls on the create path, by counter |

The plan's item "128 affected extents" is not a cap: `MAX_AFFECTED` is a scan
page size in a loop that continues (`backing/active/extents.rs:393-422`).

## 11. What grows with what

[proposed design]

| Dimension | Bound and mechanism |
| --- | --- |
| Memory per operation | One request buffer (128 KiB), one extent, the pages a transaction dirties (kept by the in-memory journal until its commit) |
| Queues | The upstream pool and the store host queue are bounded; callers wait. The Commit object stream is synchronous: the host's `Save::accept` is the backpressure |
| File count | Rows. Memory per file is zero unless it is open (a count), recently written (an append hint, bounded) or in the kernel's cache (a lookup count) |
| Payload bytes | Rows. A file of any size is at most `size / extent` rows plus scattered small ones |
| Version retention | Two per key, plus pinned rows of open unlinked files (§4) |
| Database and disk growth | Live rows, plus the garbage backlog of [03 §7](03-mutation-hot-path.md#7-maintenance), up to the quota. No journal file and no write-ahead log exist |
| Kernel lookup counts | One map entry per inode the kernel holds; released by FORGET; bounded by the kernel's own cache |

**Limits that remain, and are real.**

| Limit | Value | Kind |
| --- | --- | --- |
| Name; path component rules | 255 bytes, UTF-8 | cluster one format |
| Symlink target | 4,096 bytes | cluster one format |
| Inode kinds, mode bits, no ownership, no ctime | §6 | cluster one format |
| Inode serial | `1 ..= i64::MAX`, never recycled | identifier space |
| Canonical object | 16 MiB | cluster one format |
| Changed names of one directory in one Commit | Must fit one `Vec` (`DirectoryUpdate.changes`, `core/crates/layerfs-content/src/filesystem/input.rs:29-34`) | cluster one contract shape; a memory bound cluster two cannot remove |
| Fragmented edit of one file | `EDIT_DEFERRED_LIMIT`, 8 MiB − 1 of unfinished mapping nodes (`file/edit/tree.rs:31`, `:358-363`) | cluster one refusal, still present; see [06 §6](06-cluster-one-integration.md#6-prerequisites-outside-cluster-two) |
| SQLite row | 1,000,000,000 bytes | engine; rows are at most 128 KiB |
| SQLite database | page size × 4,294,967,294 pages, about 17.6 TiB at 4 KiB | engine; the quota is the operative limit |
| File size | 64-bit signed offsets | platform |
| FUSE request | 128 KiB | window |
| Disk, descriptors, memory | — | platform; expressed as the quota and the configured counts |

## 12. Quota, disk full and errors

[proposed design]

| Condition | Result |
| --- | --- |
| Quota reached | Before a mutation that needs pages, if the quota would be exceeded and garbage is queued, reclamation runs first ([03 §7](03-mutation-hot-path.md#7-maintenance)). If live data still exceeds the quota: `ENOSPC`, and nothing is attempted. `statfs` reports from `page_count`, `freelist_count` and `max_page_count` |
| `SQLITE_FULL` inside a transaction | The transaction rolls back from the in-memory journal; `ENOSPC`. No acknowledged operation is affected, because each acknowledged operation committed on its own |
| `SQLITE_IOERR`, `SQLITE_CORRUPT`, `SQLITE_NOTADB` | The Workspace is fail-stopped: every later request gets `EIO`, Commit is refused |
| `SQLITE_BUSY`, `SQLITE_LOCKED` | Impossible with one connection. If seen, a defect: fail-stop. No busy handler, no retry (`core/AGENTS.md`) |
| Base object missing, identity mismatch, provider failure, transport loss | `EIO`; one attempt |
| Serial range exhausted and the refill refused | `ENOSPC` |

**Page cache.** Overlay pages are ordinary file pages that the kernel writes
back. Payload written through the overlay therefore appears in the container's
`file` cache in proportion to what was written. Root `AGENTS.md` §1 forbids
excusing that. Whether the overlay may start writeback and drop clean pages on
its own files, which would bound it, depends on whether that counts as a
forbidden sync: owner question O-7. Until it is answered, the cache state is
declared and the cgroup `file` figure is reported per phase.

## 13. Isolation and cleanup

[proposed design]

- **Between Workspaces.** Separate files, locks, quotas, page caches and
  mounts. A full or failed overlay affects one Workspace. Shared, and therefore
  configured: the disk, the base cache budget, the upstream pool, the store
  host.
- **Close.** Close the connection and unlink the file. Cost does not depend on
  the Workspace's contents.
- **Close with active Execs, open handles or a Commit in flight:**
  [04 §10](04-concurrency-commit.md#10-second-commit-close-and-unmount).
- **Daemon start.** The overlay directory is emptied.
