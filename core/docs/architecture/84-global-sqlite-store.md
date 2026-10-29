# Global SQLite Store

> **Status:** Current design reference, written 2026-10-18 from the code at
> `dsh-history-replay` (`876b58ffb`) and the owner decisions of that day.
> Implemented behaviour is labeled **[impl]**; decided but not yet merged work
> is labeled **[decided]** with its fix id. Source and public contracts still
> decide what is built; this note does not add requirements beyond the cited
> decisions. Companion notes:
> [Store concurrency model](83-store-concurrency-model.md),
> [direct Store adapter](61-direct-store-adapter.md),
> [shared Store foundation](60-shared-store-foundation.md),
> [one Commit per file write](85-commit-per-write-storage.md).

## 1. What it is

One SQLite database file holds every project's committed state: content
objects, metadata objects, Branches, Commits, Layers and the indexes that find
them. It is the only global database (owner, 2026-10-18). Workspaces do not
write into it directly: each mounted Workspace has its own overlay database
(MEMORY/OFF/EXCLUSIVE, one per Workspace, deleted at unmount); a Commit
captures the overlay and publishes into the global Store.

```
          ┌────────────── one Linux kernel, one local filesystem ──────────────┐
          │                                                                    │
 SDK process (runner)        daemon (Sandbox A)         daemon (Sandbox B)     │
  Store calls in-process      Workspace Commits          Workspace Commits     │
  [decided, F-020]                 │                          │                │
          │                        │                          │                │
          └───────────────┬────────┴──────────────┬───────────┘                │
                          ▼                       ▼                            │
              /layerfs-store/global/store.sqlite  (+ -wal, -shm)               │
          └────────────────────────────────────────────────────────────────────┘
```

## 2. Lifecycle

| Stage | Who | What happens |
| --- | --- | --- |
| Init | `project.init` (SDK, host) | Project Init writes a new Store under `home/<name>/`, publishes the first Branch, and seals it: `Handles::seal()` consumes the sole session, checkpoints, closes and verifies one file [impl] |
| Install | first Sandbox of the project | the sealed file is installed into the shared volume at `/layerfs-store/global/store.sqlite` [impl] |
| Live | daemons, and the SDK process [decided, F-020] | every opener opens the same file directly, in WAL mode; Saves and Commits publish while it stays open; a live Store is never sealed per file or per Commit [impl] |
| Snapshot (#352 replay) | tooling | a sealed copy of the file, stored as content-addressed blocks; restored or forked by copying [impl, replay tooling] |

## 3. Profile (one, fixed)

**Disposable / WAL / synchronous=OFF is the only permitted profile** (owner,
2026-10-07). Durable code exists and compiles but is not executed. Every
connection applies and then checks this profile (`backend/sqlite/profile.rs`,
`connection.rs`) [impl]:

| Setting | Value | Why |
| --- | --- | --- |
| `journal_mode` | WAL | readers run beside the one writer |
| `synchronous` | OFF (0) | Disposable: no fsync; durability is not claimed |
| `page_size` | 4096 | fixed at creation; never changed |
| `auto_vacuum` | INCREMENTAL (when created with table acquisition) | bounded reclamation of free pages; pointer-map pages are the "other" part of the file |
| `foreign_keys` | ON | history rows reference each other |
| `wal_autocheckpoint` | 1000 pages | WAL folds back regularly |
| `journal_size_limit` | 4 MiB | bounded WAL after a checkpoint |
| `cache_size` | 2 MiB per connection | bounded memory |
| `mmap_size` | 0 | no mapped I/O |
| `temp_store` | memory | sorts and temp tables never touch disk files |
| `busy_timeout` | 0 | no busy handler: waiting is done by admission (§6), failure is exact |

No fsync, no `fdatasync`, no space reserved ahead of use on Disposable
backing (AGENTS.md §4).

## 4. Schema

The stored schema is validated exactly on open (`schema.rs`, `user_version`);
adding a table or index is a schema-version change with an additive migration
(REBUILD.md kind 2), never a silent drift [impl].

| Group | Tables (and indexes) | Holds |
| --- | --- | --- |
| Objects | `pack` (immutable: update/delete triggers refuse), `pack_unit` (+ `pack_unit_mapping`), `object_location` | packed object bodies; where each object id lives (pack, offset, length) |
| Pooled metadata | `inode_value` | pooled inode rows referenced by inode leaves |
| History | `branch` (+ `branch_identity`, `branch_names`), `"commit"` (+ `commit_identity`), `layer` (+ `layer_identity`, `layers_child`, `layers_genesis`, `layers_source`), `layer_stack` (+ `layer_stack_names`) | Branch heads, Commits, Layers and their lineage |
| Accounting | `store_totals` (kept by `commit_logical` and `layer_logical` triggers), `store_policy`, `scope_allocator`, `history_meta` | logical sizes, the frozen storage policy, inode-serial allocation, schema metadata |
| Hints and rings | `binding_hint` (#352, additive), `content_signature` (bounded ring, ≤ 8,192 rows) | parent bindings for O(changed) diffs; similarity signatures for delta bases |
| Init | `init_entry`, `init_host_file`, `init_operation` | Project Init's resumable records |

The Commit table appends nullable `author` and `labels` columns through an
additive writable-open migration. Labels are one JSON array; both columns have
4096-byte checks. They are creation-time annotations outside Commit identity,
and old NULL rows read as no author and no labels.

## 5. Objects inside the file

- **Content-addressed.** An object's id is the hash of its canonical bytes;
  identical content is stored once. Ids do not depend on how bytes are stored
  (compression, delta, pack layout) [impl].
- **Roles.** Whole files (below a 128 KiB cutoff), CDC chunks (8/16/32 KiB),
  extent leaves and branches, file states, inode leaves and branches,
  directory leaves and branches, filesystem roots, attribute leaves, symlinks
  (`layerfs-content/src/object/output.rs`) [impl].
- **Packs.** Objects are grouped into packs in two domains: payload (file
  content) and metadata (pooled, zstd level 1). Lanes per Save: whole-file,
  chunk and ordinary. Each Save reserves a block of pack ids and seals its own
  packs; packs are immutable once written [impl].
- **Encoding.** Whole files and inode leaves may be stored as deltas against a
  predecessor (depth caps 16 / 8 / 8 for whole-file / chunk / metadata, and a
  per-chain read budget); full records use zstd level 3 [impl].
- **Storage anatomy.** At range 11 of the #352 replay: content 25.9%,
  metadata 31.7%, index rows 23.0%, page waste 19.2%; ~5.1 KB per Commit
  (STORAGE_STUDY.md §2).
- **[decided, F-027]:** pack directories sized to their groups, mode and mtime
  in the inode value, inode branches as deltas (−41% simulated at range 11;
  STORAGE_BATCH_SPEC.md). Old forms stay readable.

## 6. Connections and concurrency

Per opening process [impl, F-020 for the SDK process]:

| Connection | Mode | Used for |
| --- | --- | --- |
| Writer session | read-write, one per process | Saves (object inserts) and publication; entered through the admission queue |
| Control reader | read-only session | history lookups (Branch, Commit, Layer, project) without waiting for the writer (F-018) |
| Mount-time readers | read-only, from a reader factory | a Workspace's base reads and SDK reads |

Concurrency (full rule in [83](83-store-concurrency-model.md)):

- **One writer at a time** for the whole file: SQLite's own lock, the Linux
  directory lock on the Store's parent directory across processes, and a
  first-come queue per process (`admission.rs`, deadline 2 s → exact `Busy`
  refusal, never a retry) [impl].
- **Publication lane:** head check, merge if the head moved, and publish run
  as one exclusive step; one Commit per Workspace at a time, Commits of
  different Workspaces overlap everywhere else [impl].
- **Short holds** are the performance model: construction, hashing,
  compression and object saves happen before the lane; reads never take the
  writer. Work inside the lane that grows with tree size, history or Branch
  count is a defect (#352 F-025) [decided].
- **Safety condition:** every opener shares one Linux kernel and a local
  filesystem (ext4, xfs, btrfs); anywhere else direct opening fails
  `Unsupported` before any write [decided, F-020].

## 7. Transactions

- **Save:** a sequence of short writer transactions (reserve ids, locate
  bases, insert packs, `object_location` and `inode_value` rows), each one
  attempt; `Save::finish()` completes the saved objects while the Store stays
  open [impl].
- **Publish:** one transaction in the lane: Branch and Layer lookups, insert
  the Commit row (the trigger updates `store_totals`), move the Branch head.
  A lost reply does not undo a published Commit [impl].
- **Reads:** WAL snapshot transactions on read-only sessions [impl]. Object
  locates, pack reads (descriptor, units, body) and stored inode-value reads
  run with no BEGIN/COMMIT, each statement in its own snapshot:
  `object_location` is insert-only, `pack`/`pack_unit` refuse UPDATE and
  DELETE, and `inode_value` gets one plain INSERT per ordinal (an ordinal is
  given back only while nothing was written under it), so no two such reads
  can disagree. Reads that pick a snapshot (Branch heads, roots, Layer and
  Commit rows) keep their transaction (#352 F-035) [impl].
- **Never:** automatic retry, busy handlers, re-prepare loops or replay of a
  failed SQL attempt (AGENTS.md §4).

## 8. Reclamation and growth

- History is not deleted: removed files keep their bytes in older Commits.
  Safe garbage collection of unreachable history needs its own contract and is
  not implemented [impl].
- Free pages are reclaimed explicitly and boundedly in a reclaimable Store
  (`reclamation.rs`); macOS seal releases unused extents beyond EOF [impl].
- Growth is per Commit (~3.9–5.1 KB at ranges 0–11), plus `node_modules`
  installs from P3; forecast 3.37 GiB at range 201 (STORAGE_STUDY.md §5).

## 9. Rules that do not change

| Rule | Source |
| --- | --- |
| One global database; no sharding, second database or objects outside it | owner 2026-10-18, 2026-10-13 |
| One writer at a time; no concurrent-writer mode, no patched SQLite | owner 2026-10-18; AGENTS.md §4 (no third-party edits) |
| Disposable/WAL/OFF only; Durable not executed | owner 2026-10-07 |
| Page size 4 KiB | AGENTS.md; #352 PROMPT |
| One attempt per operation; no retry or busy handler | AGENTS.md §4 |
| Linux-only direct opening; no daemon route for Store calls | owner 2026-10-18 (F-020) |
| Format changes are forward-only or additive unless justified | REBUILD.md |
