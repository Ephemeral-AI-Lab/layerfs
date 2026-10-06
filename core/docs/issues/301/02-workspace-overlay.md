# 02 — Workspace overlay in the daemon SQLite database

> **Status:** Current planning checklist; no release candidate exists.
> Part of the [Phase 7 packet](README.md). Proposal; schema and names are
> sketches for review, not a frozen contract. Develops the overlay idea of
> [#298](https://github.com/Ephemeral-AI-Lab/layerfs/issues/298).

> **Superseded 2026-10-05** by
> [`../303/02-base-overlay.md`](../303/02-base-overlay.md) and
> [`../303/03-mutation-hot-path.md`](../303/03-mutation-hot-path.md): one
> database file per Workspace, byte-exact extents instead of a block grid, and
> no write-ahead log. The generation rules are kept. The text below is
> unchanged and is kept as context.

## 1. Purpose

A Workspace is the committed base plus what the Workspace changed. Phase 4.5
keeps the changes in a private backing directory managed by its own page format,
B+ tree and reclamation. Phase 6 moved the metadata to SQLite but kept payload
bytes in private backing files, joined to SQL through source rows, reference
counts, pins and a retirement protocol.

Phase 7 stores **both** in one place: rows of the daemon's SQLite database.
Payload bytes and the metadata that describes them change in the same
transaction, so there is no state in which one exists without the other.

## 2. Model

```text
live view      =  active generation  over  frozen generation  over  committed base
Commit input   =                           frozen generation  over  committed base
```

- The **committed base** is immutable: a filesystem root selected by the
  Workspace's base Commit, read through C1/C2 from the global stores. The overlay
  never copies it and never enumerates it.
- A **generation** is an integer. Every overlay row carries the generation it
  was written in.
- At most one generation is **active** (receives writes). While a Commit is in
  flight, the generations it captured are **frozen**. Generations already folded
  into the base are garbage.

Three integers per Workspace describe the whole state:

| Field | Meaning |
| --- | --- |
| `folded_gen` | Rows with `gen <= folded_gen` are already represented by the base; invisible, awaiting deletion |
| `frozen_gen` | `NULL` when idle. Otherwise rows in `(folded_gen, frozen_gen]` are the pending Commit's input |
| `active_gen` | Every mutation writes rows at exactly this generation |

## 3. Schema sketch

One database per daemon, shared tables, every key prefixed by a small integer
`ws` that stands for one `(workspace, incarnation)`.

```sql
CREATE TABLE ws (
  ws INTEGER PRIMARY KEY,
  workspace BLOB NOT NULL, incarnation BLOB NOT NULL,
  branch BLOB NOT NULL, base_commit BLOB NOT NULL, base_root BLOB NOT NULL,
  folded_gen INTEGER NOT NULL, frozen_gen INTEGER, active_gen INTEGER NOT NULL,
  commit_state INTEGER NOT NULL, candidate BLOB,        -- pending Commit custody
  next_ino INTEGER NOT NULL, end_ino INTEGER NOT NULL   -- reserved serial range
);

CREATE TABLE inode (                    -- one row per (inode, generation it changed in)
  ws, ino, gen,
  kind, mode, nlink, size, mtime_s, mtime_ns,
  inherit_len,                          -- leading bytes still taken from the layer below
  PRIMARY KEY (ws, ino, gen)
) WITHOUT ROWID;

CREATE TABLE dentry (                   -- ino NULL = whiteout (name removed)
  ws, parent, name, gen, ino,
  PRIMARY KEY (ws, parent, name, gen)
) WITHOUT ROWID;

CREATE TABLE block (                    -- changed payload, fixed block grid
  ws, ino, blk, gen, data BLOB NOT NULL,
  PRIMARY KEY (ws, ino, blk, gen)
);

CREATE TABLE xattr (ws, ino, key, gen, value, PRIMARY KEY (ws, ino, key, gen)) WITHOUT ROWID;
CREATE TABLE handle (ws, fh, ino, flags, PRIMARY KEY (ws, fh)) WITHOUT ROWID;
```

Five facts, five tables. There is no source table, reference count, pin table,
retirement queue, trigger or per-Commit schema.

## 4. The three rules

**Visibility.** For any key, a view takes the row with the greatest generation it
is allowed to see; if there is none, it falls through to the base.

- live view: generations in `(folded_gen, active_gen]`
- Commit view: generations in `(folded_gen, frozen_gen]`

**Write.** A mutation inserts or replaces the row at `gen = active_gen`. It never
updates or deletes a row of a lower generation.

**Capture.** One statement, in its own transaction:

```sql
UPDATE ws SET frozen_gen = active_gen, active_gen = active_gen + 1
 WHERE ws = ?1 AND frozen_gen IS NULL;
```

Everything else follows from these. Frozen rows are immutable **because of their
key** — no writer can produce that key again — not because a lock protects them.
The capture cut is exact because SQLite admits one writing transaction at a
time: a mutation committed before the `UPDATE` is in the frozen generation with
all of its metadata and bytes, and a mutation that begins after it is entirely in
the successor. There is no "partially prepared write" to resolve, which is the
race #298 had to specify for separate backing files.

## 5. Operations

**Write.** Split the request on the block grid. A fully covered block is one
`INSERT … ON CONFLICT DO UPDATE` at the active generation. A partly covered block
that has no active row is **copied up**: read that block through the view below
(frozen row, else base, else zeros), overlay the new bytes, insert one active
row. A partly covered block that already has an active row is updated in place.
The inode row at the active generation gets the new size and mtime in the same
transaction.

**Truncate and holes.** `inherit_len` is the number of leading bytes this
generation still inherits from the layer below; shrinking a file lowers it and
it never grows back. Bytes at or beyond `inherit_len` without a block row read as
zero, so data cut off by a truncate can never show through a later extension.
Shrinking also deletes the active generation's own block rows beyond the new
size; lower generations are untouched.

**Read.** Plan before fetching: active rows first; then, only for positions
below that generation's `inherit_len`, the layer beneath — frozen rows under the
same rule, and finally the committed base for whatever is still uncovered. A
range the Workspace has overwritten never causes an object-store request
(#300 item 1).

**Names.** `dentry` rows are keyed by parent inode and one component, the
identity-relative model Phase 4.5 already uses (#264): renaming a directory
changes two rows and rewrites no descendant. Lookup is overlay first, then base.
Listing merges overlay rows with the base directory by keyset page; a whiteout
hides a base name. A removed-and-recreated directory gets a new inode, so it has
no base children and needs no opaque marker.

**Unlink while open.** The name becomes a whiteout; the inode row stays with
`nlink = 0` until the last handle closes. Blocks are keyed by inode, so hard
links and open-unlinked files need no special source lifetime.

**Opening a Workspace** inserts one `ws` row. Nothing from the base is imported.

## 6. Install and retire

After a **known** publication of root `R'`:

```sql
UPDATE ws SET base_commit = ?, base_root = ?,       -- R'
              folded_gen = frozen_gen, frozen_gen = NULL
 WHERE ws = ? AND frozen_gen = ?;
```

One statement switches every reader from "frozen rows over old base" to "new
base" atomically; the two are the same content by construction. Rows of the
active generation need no change: they describe differences from the layer
below, and the layer below has the same content before and after.

**Retire** is then garbage collection with no reader interaction:
`DELETE … WHERE ws = ? AND gen <= folded_gen` in bounded batches per table,
interleaved with ordinary writes.

**A refused or conflicting Commit** clears `frozen_gen` and changes nothing else.
The frozen rows are still the Workspace's changes; the live view already reads
them. The next capture freezes `(folded_gen, active_gen]`, which now spans two
generations. A background compaction may delete a lower row that a higher row of
the same key fully shadows — safe because blocks, inodes and names are whole-row
replacements. This is the only case where a key can have more than two rows.

**Trade-off (D9).** After install, a read of bytes that were local a moment ago
is a read of the committed base. The first slice accepts that and relies on the
bounded chunk cache; keeping folded rows as a local cache of committed content is
possible under the same visibility rule but needs per-inode retirement ordering
and a byte budget, so it is deferred until measured.

## 7. The two design questions

### "Only the latest payload is recorded" versus a non-pausing Commit

They do not conflict, because *latest* is scoped to a generation.

| Moment | Event | Rows for block 7 of file F |
| --- | --- | --- |
| idle | 1,000 rewrites of block 7 | `(F,7,g1)` — one row, replaced in place |
| Commit starts | capture: `frozen=g1`, `active=g2` | `(F,7,g1)` frozen |
| during Commit | 10,000 more rewrites | `(F,7,g1)` frozen, `(F,7,g2)` replaced in place |
| publication known | install: `folded=g1` | `(F,7,g1)` garbage, `(F,7,g2)` live |
| retire | batch delete | `(F,7,g2)` |

The extra space is bounded by what is **touched while a Commit runs**, not by the
number of edits: at most one additional row per touched key. Commit reads exactly
the bytes that existed at capture; nothing written afterwards can reach it, and
nothing it reads can change. No command waits for construction, upload or
publication.

### Is it sound to manipulate rows so Commit can run during Exec?

Yes, provided the manipulation is *adding a generation*, not *editing rows*.
The alternatives were considered:

| Mechanism | Cost at capture | Cost while Commit runs | Why not |
| --- | --- | --- | --- |
| Pause commands | none | commands blocked for the whole Commit | Excluded by requirement |
| One long SQLite read snapshot | none | WAL cannot be checkpointed while the reader is open, so it grows with every write | Unbounded file growth; a transaction spans construction and upload |
| Copy dirty rows to a staging table | proportional to changed rows and bytes | none | A pause that grows with the change; payload stored twice |
| Row versions with `born`/`until`, pins, retirement (Phase 6) | O(1) | every write closes a version and maintains pin and retirement state | #299 recorded about 40 SQL statements per write at 4,097 writes; complex custody |
| **Generation key (proposed)** | **one `UPDATE`** | first touch of a key: one extra row; later touches in place | — |

Because frozen rows cannot change, Commit does not need snapshot isolation. It
pages through them with ordinary short read transactions (keyset on
`(ino, blk)`), releasing the database between pages. In WAL mode (§10) those
reads run on their own connection and neither block the writer nor are blocked
by it. Commit uses the writer only for capture, install and retirement batches —
each a bounded, short transaction.

## 8. Open parameters

| Parameter | Question | How to decide |
| --- | --- | --- |
| Block size (D3) | Small blocks: more rows, less copy-up and rewrite. Large blocks: fewer rows, more bytes rewritten per small write | Count rows, statements and bytes written per operation across the registered write patterns. Blocks no larger than the smallest CDC chunk keep an edit from widening the region C1 must re-chunk |
| Database page size | A BLOB as large as a page spills to overflow pages | Choose page size and block size together; count pages written per block write |
| Extents instead of blocks | Variable-length rows avoid copy-up but need interval trimming and splitting on every overlap | Kept as the alternative if copy-up from the base proves costly; block rows are the recommendation because shadowing is whole-row and one statement |
| Write batching | One transaction per FUSE request, or grouping several | Count transactions per write; no change to accepted-write semantics |
| Storage quota | Explicit user quota as a page budget; no default cap | Carried from #296 |

## 9. Costs and risks

Stated plainly; each needs a count-driven diagnostic in M6 and M7 before any claim.

1. **Payload is written twice locally.** In WAL mode a block goes to the WAL and
   is later checkpointed into the database file. That is the price of readers
   that never block the writer and of surviving a daemon crash (§10).
2. **Page-cache growth.** Database and WAL pages are file-backed. Root
   `AGENTS.md` §1 forbids excusing file-size-proportional page-cache growth; a
   large write must be measured at the cgroup level, and the SQLite pager cache
   bound alone proves nothing.
3. **One writer per database.** All Workspaces in a daemon serialize their
   mutations through one writing connection. Each transaction is short and does
   no I/O wait when unsynchronized, but this is the daemon's write ceiling and
   must be measured under concurrent Workspaces (#249).
4. **Copy-up can reach the base.** A partial write to a block that exists only
   in the committed base reads that block first, which can mean a locator lookup
   and a ranged GET. Whole-file rewrites, appends beyond the base and
   block-aligned page writes do not.
5. **Space is reused, not returned.** Deleted rows free pages inside the file.
6. **C1's deferred-edit allowance must stop refusing.** `EDIT_DEFERRED_LIMIT`
   (`8 MiB − 1`, `layerfs-content/src/file/edit/tree.rs`) bounds unfinished
   mapping nodes for a heavily fragmented edit and fails the operation beyond it.
   The overlay does not lift it. Owner direction, 2026-10-03: Phase 7 keeps no
   refusal for accumulated change, so this allowance becomes a resident working
   window with progress beyond it. That is a cluster 1 change, requested by
   cluster 2 as integration-contract item C9 (#303 plan).
7. **FUSE cache mode.** Phase 4.5 mounts with direct I/O and no writeback cache,
   so an accepted write has reached the daemon. That assumption is kept; changing
   it is a separate declared decision.

## 10. SQLite settings

The overlay database is embedded SQLite, private to one daemon process. Values
marked *measure* are starting points to confirm with a count-driven diagnostic
in M6; the rest are proposed as fixed. Every setting is applied and read back on
every connection, and an unsupported one is a startup failure, not a fallback.

| Setting | Value | Why |
| --- | --- | --- |
| `journal_mode` | `WAL` | Readers (FUSE reads, Commit paging) never block the writer and are never blocked by it. A killed daemon leaves a consistent database; a `MEMORY` journal can leave a corrupt one, losing every unpublished change |
| `synchronous` | `OFF` | Workspace backing: no sync and no durability claim (root `AGENTS.md` §4). Committed transactions survive a daemon crash; they are not promised across an OS or VM crash |
| `locking_mode` | `NORMAL` | `EXCLUSIVE` would restrict the database to one connection and lose concurrent readers |
| Connections | One writer, a small fixed set of read-only connections | All mutations go through the one writer under a mutex, so `SQLITE_BUSY` cannot arise between writers and no busy handler or retry is needed |
| `busy_timeout` | `0` | With one writer and WAL readers nothing should ever wait; a `BUSY` is a defect to surface |
| Write transactions | `BEGIN IMMEDIATE`, one per FUSE mutation | Takes the write lock up front; payload and metadata commit together |
| `wal_autocheckpoint` | `0`, with explicit `PASSIVE` checkpoints at a declared WAL size (*measure*) | Keeps the WAL bounded and makes the checkpoint cost attributable instead of landing on an arbitrary write. `PASSIVE` never waits for a reader |
| `journal_size_limit` | Same bound as the checkpoint threshold | Truncates the WAL file after a checkpoint so it does not stay at its high-water size |
| `page_size` | `4096` (*measure*, together with the block size, D3) | Count pages written per block write before fixing either |
| `cache_size` | A fixed byte allowance per connection, 2 MiB to start (*measure*) | Bounds the pager's heap. It is not a bound on OS page cache, which is measured separately |
| `mmap_size` | `0` | Keeps reads on the ordinary I/O path, so memory accounting stays clear |
| `temp_store` | `MEMORY` | No temp files; every query is a keyed lookup or keyset page and needs no large sort |
| `auto_vacuum` | `NONE` | Freed pages are reused. Returning space to the filesystem is a later decision; incremental vacuum adds work to every write |
| `secure_delete` | `OFF` | Retirement deletes many rows; do not zero freed pages |
| `foreign_keys` | `OFF` | The schema declares none; integrity comes from the key structure and one-transaction mutations |
| Table options | `STRICT`; `WITHOUT ROWID` for `inode`, `dentry`, `xattr`, `handle`; ordinary rowid table for `block` | Small rows cluster on their key; large BLOB rows do not belong in a `WITHOUT ROWID` tree |
| `application_id`, `user_version` | Set | Schema identity, as the existing stores do |
| Linkage | Bundled SQLite, version pinned | The daemon image is static musl; the SQLite version becomes part of the build identity |
| File location | A daemon-private directory on the container's local disk, root-only | Not on the FUSE mount and not a host bind mount; commands cannot read it |
