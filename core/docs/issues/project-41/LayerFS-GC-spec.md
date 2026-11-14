# LayerFS garbage collection: soft removal and Store compaction

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

> **Authority.** This is the only GC design document in the tree. It replaces the
> six-file set merged by #658 (removed by #671; last text in the #677 branch).
> The contract it states is the frozen set of layerfs-project#42 (items 1–13,
> 2026-11-13) as amended by #42 items 14–26 (2026-11-14). Where this file and
> an older document disagree, this file wins; a change to a contract sentence
> needs a new decision entry on #42 first.
>
> **Scope.** layerfs-project#41 and its sub-issues #42–#52. The durability
> profile (#155 §7) is outside this contract (#42 item 26).
>
> **Source.** Every code reference is to `layerfs_v2_0_0` at `e8fa0a7e00`.

## 1. Requirements (owner policy, #41)

- Four operations, on SQLite and PostgreSQL: `branch.remove(name)`,
  `project.delete(name)`, `project.remove_file(project, path)` and
  `store.compact(name)`. Each takes only a resource identifier: no confirm,
  force, dry-run, stop-Sandboxes, Store, URL or volume option.
- The three removals are logical. They change visibility and retention and
  physically reclaim nothing. Even deleting the sole project keeps the Store
  and its volume addressable. No undo is promised.
- `store.compact(name)` alone physically reclaims rows and data that no
  retained reference needs. Shared content, delta bases and pooled
  dependencies survive.
- Existing immutable IDs never silently resolve to changed contents.
- Admission (§5): mounts in scope → `MountsActive`; other conflicting users →
  `Busy`. The caller prepares inactivity: no implicit unmount, no waiting, one
  attempt, no retry or replay; a lost reply stays uncertain.
- No automatic collection, no online sweep, no compaction of remote
  object-storage copies. PostgreSQL autovacuum stays on and is not counted.
- Report logical bytes discarded, backend-reusable space and physical shrink
  separately.

## 2. Decisions

| # | Decision | Section |
|---|---|---|
| S1 | Reachability is computed per Store, over every project in it | §6 |
| S2 | Exclusion reuses the Store lock, lease and op record; PostgreSQL adds advisory locks (#42 item 20) | §5 |
| S4 | No object age; collection depends only on reachability | §6 |
| S5 | The unit that frees bytes is the pack. A rebuild copies only retained rows into a fresh Store; a published `pack` row is never updated or deleted in place | §7, §8 |
| S6 | A SQLite Store compacts on the machine where it lives (`place::plan`, `control/crates/layerfs-store/src/place.rs:40-49`) | §7 |
| S10 | Logical removal never rewrites history or changes an ID | §4 |
| S11 | Compaction is exclusive over the whole Store; reads are unavailable during it | §5 |
| 7 | Removal markers live in marker tables; `branch` and `layer_stack` stay append-only | §4 |
| 14 | Compaction collects history rows no retained root reaches; Commits of a removed Branch that nothing retains become `NotFound` | §6.1 |
| 15 | A name held by a removed Branch or deleted project returns `NameTaken` until compaction removes the row; then it is free. `NameRetired` is not added | §4.4 |
| 16 | `remove_file` hides a path across the project's history at once and lets compaction reclaim what only that path reached; reads through it return `Removed` | §4.3, §6.2 |
| 17 | `store.compact` has two paths, `none` and `rebuild`; there is no light path. C3 has no caller in 2.0 | §3 |
| 18 | A leftover is judged by the Store generation; no `OrphanArtifact`, no leftover cap | §9 |
| 19 | API codes follow #41 (`MountsActive`, `Busy`); one `StaleStore` for both backends | §10 |
| 20 | PostgreSQL presence is a session advisory lock per opener connection; the fence is the generation check carried in each statement | §5.2 |
| 21 | PostgreSQL compaction builds in batches, then swaps two tables in one short transaction | §8 |
| 22 | SQLite compaction is its own module: the walk is fused with the copy, the visited set is a temp-file table, verification is cheap by default | §6.3, §7 |
| 23 | Removed: the `pinned` set, the per-transaction SQLite generation check, the capacity pre-check, the extra report fields | §6, §7, §11 |

Out of this release: object age (Q3), repacking partly dead packs (reported,
not performed), in-place deletion of packs on either backend, a read-available
compaction window (A'), pruning (#46).

## 3. `store.compact(name)` paths

| Path | When | Work | Generation |
|---|---|---|---|
| `none` | Bytes in wholly dead packs are below **10%** of the Store's bytes | Mark only; nothing is written; the report gives the reclaimable bytes | unchanged |
| `rebuild` | 10% or more | SQLite §7, PostgreSQL §8 | `G → G+1` |

The 10% value is a starting point, set after the first measurement (#52).
Below it a rebuild would copy at least nine live bytes for each byte freed.
There is no path that deletes rows in place: it would free no payload, touch
most index pages of the hash-keyed tables and need its own deletion contract.

## 4. Soft removal

All three removals write one marker row in one short write transaction. No
pack, object, Commit or Layer row changes, and no ID changes. They do not
bump the Store generation.

| Call | Marker table | Effect |
|---|---|---|
| `branch.remove(name)` | `branch_retired(branch_id, removed_ms)` | Hidden from listings and name resolution; fork, merge, publish and mount refused; `main` → `ReservedName` |
| `project.delete(name)` | `layer_stack_retired(layer_stack_id, deleted_ms)` | Project hidden; its data stays until compaction; the Store stays addressable |
| `project.remove_file(project, path)` | `path_removed(layer_stack_id, path, removed_ms)` | §4.3 |

### 4.1 Representation

The marker tables are added at open by an additive migration under the
current schema identity, as `add_file_digests` was (#42 item 6). 1.0 Stores
are converted offline. `branch` and `layer_stack` keep their full unique name
indexes.

### 4.2 Visibility filter

Every query that lists or resolves a Branch or project goes through one
`*_live` view over the anti-join with the marker tables. A query that bypasses
it would resurrect a removed Branch. The list is a #42 deliverable; the
minimum is the Branch and project listings and name lookups
(`branch_snapshot`, `branch_snapshot_named`, `layerstack_stack_by_name`),
fork, merge, publish and mount resolution, and the project listing
`layerfs_project::store::projects`. That listing also drives
`identity::list_projects` and, since #678, the record rebuild in
`backup.restore` (`control/crates/layerfs-store/src/backup/restore.rs`), so an
unfiltered listing would write records for deleted projects back into a
home.

### 4.3 `remove_file`

- The marker names a path in one project. From the write on, every Commit of
  that project behaves the same way for that path, before and after any
  compaction:
  - a read that resolves through the path returns `Removed`;
  - a listing omits the entry and says that entries were removed (shape owned
    by #47/#51);
  - a new mount of any of the project's Commits does not show the path.
- History rows and IDs are unchanged. Because the answer is `Removed`, not
  different bytes, an old ID never silently resolves to changed contents.
- The call reports no freed bytes; compaction reports what it reclaimed (§6.2).
- The path is scoped to one project. The same bytes reached from another
  project, another path, or as a delta base of a live object stay.
- If #47 cannot deliver the path-aware walk in 2.0.0, the call ships as
  visibility only and its documentation says plainly that it frees nothing.

### 4.4 Names

`BranchId::named = blake3(stack ‖ name)`
(`core/crates/layerfs-history/src/contract/identity.rs:266-275`) assumes a
name is never reused while its old rows exist. So:

- A create or fork whose name a live row holds returns `NameTaken` (existing:
  `engine/crates/layerfs-store-service/src/history/branch.rs:232,397`).
- A name held by a removed Branch or deleted project also returns
  `NameTaken`, with the holder's state in the detail.
- Compaction removes a retired `branch` or `layer_stack` row once no retained
  row references it (`layer.source_branch_id`, `commit.branch_id` and the
  other history edges, `engine/crates/layerfs-persistence/sql/sqlite/history.sql:62-65`).
  The name is then free, and no retained row carries the old ID. The comment
  on `BranchId::named` ("never reused") is updated by #44 to say "never
  reused while its rows exist".
- A Branch whose old Commits stay retained (for example through a live
  Layer's `source_commit_id`) keeps its row, so its name stays taken.

## 5. Admission

| Operation | Refused while | Code |
|---|---|---|
| `branch.remove` | a mount or Sandbox of that Branch exists | `MountsActive` |
| `project.delete`, `project.remove_file` | any mount of the project (any Branch, any historical Commit), or another user of the project | `MountsActive`, `Busy` |
| `store.compact` | any mount, client or operation on the Store, or unfinished Init/merge rows (#155 D-6) | `MountsActive`, `Busy` |

The check and the exclusion are one atomic step: a new conflicting user is
refused from the check until the operation completes.

### 5.1 SQLite

The existing Store lock, lease and op record (`control/crates/layerfs-store/src/lease.rs`,
`record.rs`, `control/crates/layerfs-home/src/op.rs`). A live lease, reported
by the lease code as `InUse`, is returned as `Busy` at the API.

### 5.2 PostgreSQL

Leases are files in one home and cannot see another machine. So:

- **Presence.** Every opener holds `pg_advisory_lock_shared(key)` on its own
  connection for the connection's lifetime, with
  `key = blake3("layerfs/advisory/v1\0" ‖ catalog_id)[..8]` (#42 item 12a).
  Compaction takes `pg_try_advisory_lock(key)` on its own connection; failure
  is `Busy`, naming the holders (`pid`, `client_addr`, `backend_start`).
  This also makes an idle opener, such as a remote daemon with a mount,
  visible.
- **Fence.** The generation check rides in the statements a client already
  sends: in the `BEGIN` batch of a transaction, and as a joined column of
  `history_meta.catalog_incarnation` in the reads that run without a
  transaction (`Mode::Immutable`/`Statement` skip `BEGIN`:
  `engine/crates/layerfs-persistence/src/backend/postgresql/session/transaction.rs:263-268`).
  No extra round trip. A mismatch is `StaleStore`.
- **Connections.** An opener needs a direct or session-pooled connection;
  transaction pooling cannot hold a session lock and is `Unsupported`.
- **Dead clients.** A client whose TCP connection died holds its lock until
  the server notices. The guide documents `tcp_keepalives_idle`,
  `client_connection_check_interval` and `pg_terminate_backend`.

## 6. Mark

Inside the exclusive window, at the frozen generation `G`. No caching across
operations; a missing locator or corrupt dependency aborts with no writes.

### 6.1 History closure

A recursive query from the retained roots over the history edges:

- Branch (not retired) → `head_commit_id`, `base_layer_id`, `forked_from_id`
- Stack (not retired) → `head_layer_id`, published Layers
- Commit → `parent_commit_id`, `second_parent_id`, `base_layer_id`
- Layer → `parent_layer_id`, `source_commit_id`

`merged_from_id` and `displaced_id` are identity values, not roots (#155
D-10). A fork keeps its source's history up to the fork point. History rows
outside the closure are collected by the rebuild (§7, §8); this is required
because the full doctor qualifies every Commit and Layer root it finds
(`engine/crates/layerfs-persistence/src/diagnostic/history.rs:155-198`).

### 6.2 Object closure

- Start from the retained Commits' and Layers' roots.
- Follow logical references with `FinalizedObject::admit(..).references()`
  (`core/crates/layerfs-content/src/object/output.rs:121,212`) and delta bases
  with `ChainBases::base_of` (`core/crates/layerfs-storage/src/encoding/delta/read.rs`).
  Do not reuse `qualify`: it skips file mappings, symlink targets and
  attribute trees and ignores delta bases.
- Two visit levels: *base-only* (needed only as a delta base) and *full*. A
  base-only object is upgraded if later reached fully.
- `remove_file`: when descending a root of a project with `path_removed`
  rows, the walk does not enter a removed path. An object stays live if any
  other path, project or delta chain reaches it.
- Window by window, sort lookups by object ID before locating them.
- Whole files and chunks have no logical references; only their delta base
  is needed. Reading it still loads and authenticates the whole group
  (`delta/read.rs:561-571`), so the payload part of the walk runs inside the
  copy (§7 step 4), not as a separate pass.

### 6.3 Visited set

A scratch table `visited(object_id PRIMARY KEY, level, pack_id) WITHOUT ROWID`
in a temporary file owned by the compaction connection (`temp_store=FILE`):
bounded memory, no second persistent database. Session connections keep
`temp_store=2` (`engine/crates/layerfs-persistence/src/backend/sqlite/session/profile.rs:88`).
On PostgreSQL, a temporary table.

### 6.4 Decision

- `live packs` = packs holding at least one live object; the rest are dead.
- `none` or `rebuild` from §3, using the bytes of packs that hold no live
  object after the metadata walk (an upper bound that §7 step 4 refines).
- Dead rows: `object_location`, `content_signature`, `file_digest` (keep a
  row only if its root is retained, #155 R-4), `inode_value_block` (a block
  is dead when every ordinal in it is dead), history rows outside §6.1, and
  retired `branch`/`layer_stack` rows (§4.4).

## 7. SQLite rebuild

Runs as one operation on the Store's machine (S6).

1. Checkpoint the WAL and confirm no `-wal` remains (an immutable attach
   ignores the WAL).
2. Create `<store>.compact` beside the Store with a raw connection:
   `page_size=4096`, the source's `auto_vacuum`, `schema::scripts`,
   `application_id`, `user_version`. Give this connection a large
   `cache_size`, and the attached old file a large `mmap_size`; the session
   defaults are a 2 MiB cache and no mmap (`session/profile.rs:86-87`).
3. `ATTACH` the old file `immutable=1`; it stays byte-identical.
4. One rollback-journal transaction, foreign keys on, in this order:
   - Metadata walk (§6.1–§6.2 logical part) into `visited`.
   - Payload sweep: live packs in descending `pack_id`. Copy each live pack
     once, add the delta bases of its live records to `visited`. A base found
     in a pack already passed goes on a late list and is copied at the end.
     (#48 confirms the base-before-delta ordering this relies on.)
   - `pack_unit` for the copied packs, as a complete unit set
     (`engine/crates/layerfs-persistence/src/backend/sqlite/objects/units_read.rs:55-75`).
   - `object_location` by an ordered join with `visited`.
   - `content_signature` entries that name a copied object; `file_digest`,
     `inode_value_block`, history and catalog tables per §6.4;
     `history_meta` with `catalog_incarnation = G + 1`.
   - `store_totals`: `logical` is rebuilt by the history triggers and must
     equal the old value minus the collected history rows' `logical`;
     `canonical`, `stored` and `metadata` are summed.
5. Switch the new file to WAL and run the sidecar checks
   (`engine/crates/layerfs-persistence/src/backend/sqlite/snapshot.rs:101-140`).
6. Verify: a real Session open (`validate`), `heads_digest` before and after
   (`engine/crates/layerfs-persistence/src/backend/sqlite/heads.rs:83`), row counts against the mark,
   `foreign_key_check`, the catalog allocator and totals checks. No sha256 of
   either file. `quick_check` and the full doctor stay separate commands.
7. Install: re-read `G`; check `(catalog_id, incarnation, dev, ino)` and that
   no sidecars exist; `sync_all` the new file; `rename` it over the Store;
   fsync the directory. The old inode is freed at once.
8. A definite failure of compaction's own write (disk full, write refused)
   aborts the transaction and deletes `<store>.compact` in the same call,
   returning `Capacity` or the failure (#155 D-22).

The existing transfer pipeline is not reused for this: its `Install` refuses
an existing Store file and its `Retire` deletes one
(`control/crates/layerfs-store/src/sqlite/transfer/steps.rs:262-288`).

Cost: read about the live bytes plus the metadata, write about the live
bytes; peak disk = old Store + live bytes + the visited table.

## 8. PostgreSQL rebuild

Only `object_location.pack_id` references `pack`, so the payload lives in two
tables. `content_signature` (at most 8192 rows, no foreign key:
`sql/postgresql/schema.sql:45-50`) is cleaned in place.

**Phase A — build (exclusive advisory lock held, no table lock).**

1. Load the live object and pack IDs into temporary tables; `ANALYZE` them.
2. Create `pack` and `object_location` in a scratch schema with the Store's
   column definitions and `STORAGE EXTERNAL` on `pack.body`, without the
   trigger and the foreign key.
3. Copy live rows with `INSERT … SELECT` and a semi-join (`object_location`
   has no unique key, `schema.sql:29-38`, so a plain join can duplicate rows),
   in batches of about 1 GB by `pack_id`, each batch its own transaction. No
   pack bytes cross the network. Batches may pause for standby lag.
4. Build `object_location_prefix` and the foreign key, then `ANALYZE`.

**Phase B — swap (one short transaction).**

5. `LOCK TABLE` the Store's tables `ACCESS EXCLUSIVE NOWAIT`.
6. Re-check `catalog_incarnation = G`, the heads digest, and that
   `store_policy.next_pack_id` and the ordinal allocator have not moved.
   Any change abandons the round with no writes (`Conflict`).
7. Delete dead rows in place: `content_signature`, `file_digest`,
   `inode_value_block`, history rows and retired name rows (§6.4). `logical`
   in `store_totals` is reduced by the collected history rows, as
   `removal_totals` does today
   (`engine/crates/layerfs-persistence/src/history/removal.rs`); `canonical`,
   `stored` and `metadata` are recomputed.
8. Copy the old tables' owner and ACL onto the new ones.
9. `DROP TABLE` the old `object_location` and `pack`; `ALTER TABLE … SET SCHEMA`
   the new ones into the Store's schema; create the `pack_immutable` trigger
   there, bound to the Store schema's `pack_immutable()` so doctor sees
   `EXECUTE FUNCTION pack_immutable()`
   (`diagnostic/postgresql_contract_expected.rs:213`).
10. `catalog_incarnation = G + 1`; `SET LOCAL synchronous_commit` to the
    Store's setting or `local`, whichever is stronger; `COMMIT`. Then drop
    the empty scratch schema.

- **Crash:** a crash in phase A leaves only the scratch schema, which the op
  record names and the next attempt removes. Phase B is atomic.
- **Lost `COMMIT` reply:** first prove the old backend has ended (the
  exclusive advisory lock is free, or its recorded `pg_backend_pid()` is
  gone), then read the incarnation. A read, not a replay.
- **Lock time:** `ACCESS EXCLUSIVE` is held for phase B only, seconds rather
  than the length of the copy. No long transaction holds back vacuum for
  other Stores in the same database.
- **Space:** old tables + new heap/TOAST + indexes + up to the live bytes of
  WAL. The guide states the WAL headroom requirement; a full `pg_wal` stops
  the whole server.
- **Privileges:** ownership of the Store's tables and `CREATE` on the schema,
  otherwise `Unsupported`.

## 9. Failure and leftovers

- The op record names `G` and the staged artifact (`<store>.compact` or the
  scratch schema).
- The next `store.compact` judges it: the Store at `G` means not installed, so
  the named artifact is removed; `G+1` means installed, so the outcome is
  recorded. A machine that cannot be reached reports `unknown`, never clean.
- A file or schema at the staged name that no record names is refused with
  the existing `Exists` and is not deleted.
- Install is the only irreversible step: the Store is wholly old or wholly new.
- `backup.create` before compaction is the recovery path for a mark bug; no
  undo copy is kept (#155 D-24).

## 10. Fences after install and error codes

- **SQLite:** at the start of every transaction, compare the Store path's
  `(dev, ino)` with the one recorded at open; a mismatch is `StaleStore`
  before anything is read or written. This covers both a reader and a writer
  still holding the old file (formerly C8 and C8b).
- **PostgreSQL:** the generation check of §5.2.
- **`lfq1` tokens** of an older generation already fail, because their
  authenticator is keyed by the incarnation.

| Code | Status |
|---|---|
| `MountsActive`, `Busy`, `NotFound`, `NoSuchStore`, `Unsupported`, `ReservedName`, `Conflict`, `NameTaken`, `Capacity`, `Exists` | existing |
| `StaleStore` | new; one `HistoryError` variant with an explicit mapping arm, because the `_ => "Failed"` wildcard (`engine/crates/layerfs-store-service/src/history/lookup.rs:54,64`) would hide it |
| `Removed` | new; reading a path removed by `remove_file` |

New `HistoryCatalog` methods get default `Unsupported` bodies so the eight
implementations need not change at once (C9).

## 11. Report

`store.compact` returns, and `store.status` shows while running:

| Field | Meaning |
|---|---|
| `path` | `none` or `rebuild` |
| `generation` | `{ before, after }` |
| `logicalDiscarded` | canonical bytes of collected objects |
| `backendReusable` | SQLite: the old file's free pages; PostgreSQL: 0 for rebuilt tables |
| `physicalShrink` | SQLite: old file size minus new; PostgreSQL: `pg_total_relation_size` delta |
| `keptWaste` | bytes of dead objects in retained packs, and bytes kept only as delta bases |

A zero is always explained. Estimates are marked.

## 12. Acceptance (#52)

- **Crash injection** (`kill -9`, per backend) at mark, build, verify, before
  and after install: wholly old or wholly new; the leftover is judged by §9.
- **No live object lost:** property tests over random fork / commit / merge /
  publish / `branch.remove` / `project.delete` / `remove_file` histories;
  closure sets compared before and after.
- **Independent walk:** after compaction, walk the new Store with the
  existing reference walk (`diagnostic/scan.rs`) and run the full doctor on
  the fixtures (#155 D-23).
- **Regression:** after compaction, a Save of bytes whose `file_digest` row
  named a collected root succeeds.
- **Fences:** a connection that bypassed admission gets `StaleStore`; a second
  compaction gets `Busy`; a remote PostgreSQL opener blocks compaction with
  `Busy` naming it.
- **Names:** `NameTaken` after removal, a free name after compaction collects
  the row, `ReservedName` for `main`.
- **`remove_file`:** `Removed` before and after compaction; bytes reclaimed
  only when no other path, project or delta chain uses them.
- **Speed and space** (one sample per arm, declared caches, generated
  fixtures): downtime per GB of live data and per GB freed, bytes written,
  peak disk, `ACCESS EXCLUSIVE` duration on PostgreSQL. The rebuild's build
  phase is compared with plain `VACUUM INTO` of the same Store; the mark is
  reported separately. A storage option that costs about 50% speed for about
  5% storage is rejected.

## 13. Estimates, not measurements

For a 50 GB SQLite Store with 30% in wholly dead packs (35 GB live) on NVMe,
the review of 2026-11-14 estimated about 2–3 minutes of downtime for §7, and
hours if the full doctor ran before and after. For PostgreSQL with the same
Store, about 9 minutes in total with `ACCESS EXCLUSIVE` held for about two
seconds. Nothing has been measured; #52 owns the numbers.
