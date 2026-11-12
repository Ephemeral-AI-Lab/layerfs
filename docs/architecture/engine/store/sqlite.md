# SQLite

[Architecture](../../README.md) › [Engine](../README.md) › [Store](README.md) › SQLite

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The SQLite backend.

| | |
| --- | --- |
| Component | [Store](README.md) |
| Source | [`engine/crates/layerfs-persistence/src/backend/sqlite`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite), [`engine/crates/layerfs-persistence/sql`](../../../../engine/crates/layerfs-persistence/sql) |

## Schema

```text
 physical (Objects and Metadata)            history
 +--------------------+                     +----------------+
 | store_policy  id=1 |                     | history_meta   |
 | next_pack_id,      |                     | id=1, binding  |
 | next_ordinal       |                     +----------------+
 +--------------------+                     +----------------+
 +--------------------+  pack_id            | layer_stack    |
 | object_location    |----------+          | head_layer_id -+--+
 | object_id PK       |          |          +----------------+  |
 +--------------------+          v                              v
 +--------------------+   +-------------+   +----------------+ +-------------+
 | inode_value_block  |   | pack        |   | branch         | | layer       |
 | first_ordinal PK   |   | pack_id PK  |   | base_layer_id -+>| layer_id PK |
 +--------------------+   +-------------+   | head_commit_id | +-------------+
 +--------------------+          ^          +-------+--------+        ^
 | content_signature  |          |                  v                 |
 | slot PK 0..8191    |   +-------------+   +----------------+        |
 +--------------------+   | pack_unit   |   | "commit"       |        |
 +--------------------+   | (37 and 38) |   | commit_id PK   |        |
 | store_totals  id=1 |   +-------------+   | base_layer_id -+--------+
 +--------------------+                     +----------------+
 lineage: layer_lineage, commit_lineage, absorbed, reach
 other:   scope_allocator, binding_hint_block, file_digest,
          merge_operation, merge_record
 export:  change_state, change_log
 Init:    init_operation, init_entry, init_host_file   (versions 42-44)
```

| Table | Key | Holds | File |
| --- | --- | --- | --- |
| `store_policy` | `id=1` | Frozen policy, schema version, `next_pack_id`, `next_ordinal` | [`metadata.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/metadata.sql) |
| `object_location` | `object_id` (32 bytes) | Role, canonical length, pack, group and record number | same |
| `inode_value_block` | `first_ordinal` | The 73-byte inode values of ordinals `first_ordinal..=last_ordinal` (at most 255) in one block of at most 4,000 bytes (#91; [blocks](metadata-and-objects.md#inode-value-and-binding-hint-blocks)) | same |
| `content_signature` | `slot` 0..8191 | Ring of content signatures, ordered by `stamp` | same |
| `store_totals` | `id=1` | Logical, canonical, stored, metadata and Init totals | same |
| `pack` | `pack_id` | Layout 36: whole `body`; 37–38: `control` only; immutable by trigger | [`objects.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/objects.sql), [`objects_units.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/objects_units.sql) |
| `pack_unit` | `unit_id`; unique `(pack_id, group_number)` | One encoded group per row (layouts 37–38; 38 adds index `pack_unit_mapping`) | [`objects_units.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/objects_units.sql) |
| `history_meta` | `id=1` | Catalog id, incarnation, binding key, recorded schema source and definition | [`history.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/history.sql) |
| `layer_stack` | `layer_stack_id`; unique `name` | A project and its head Layer | same |
| `layer` | `layer_id` | Root, parent Layer, source Branch and Commit, `logical` | same |
| `"commit"` | `commit_id` | Root, parent, base Layer, second parent, Branch, message, statistics, LWW facts, displaced head | same |
| `branch` | `branch_id`; unique `(layer_stack_id, name)` | Base Layer, `forked_from_id`, head Commit | same |
| `scope_allocator` | `scope_id` | Inode high-water per scope | same |
| `binding_hint_block` | `(grp, seq)` | One write's name bindings of saved trees for the 64 serials of group `grp` (`child >> 6`), in a block of at most 4,000 bytes (#91) | same |
| `layer_lineage`, `commit_lineage`, `absorbed`, `reach` | see file | Ancestry without walking history | [`lineage.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/lineage.sql) |
| `merge_operation`, `merge_record` | `operation`; `(operation, kind, key)` | One merge's spilled build records | [`merge_scratch.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/merge_scratch.sql) |
| `file_digest` | `(digest, length)` | Saved file root by content digest, chunked files only (length at or above `small_file_threshold_bytes`) | [`file_digest.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/file_digest.sql) |
| `change_state`, `change_log` | `id=1`; `seq` | The export cursor, and the key of every row written to the logged tables since the Store was marked as an export base ([Change export](#change-export)) | [`change_log.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/change_log.sql), [`change_log_units.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/change_log_units.sql) |
| `init_*` | operation-prefixed | Init's acquisition working state | [`acquisition/schema.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/acquisition/schema.sql) |

Every table is `STRICT`; history keys are `WITHOUT ROWID` blobs whose first
byte tags the kind (`0x31` stack, `0x32` Layer, `0x12` Commit, `0x11`
Branch). History foreign keys are `DEFERRABLE INITIALLY DEFERRED`. Triggers
add each new Commit's and Layer's `logical` to `store_totals`. Open checks
`application_id`, `user_version`, the exact table set and the recorded schema
text ([`schema.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/schema.rs)).

## Sessions

| Step in `Session::connect` | Detail | Source |
| --- | --- | --- |
| Open | `READ_WRITE` or `READ_ONLY`, `NO_MUTEX`; statement cache 64 | [`connection.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/connection.rs) |
| Guard | `busy_timeout=0`; `SQLITE_DBCONFIG_DEFENSIVE` on | same |
| Settings | `profile::apply`, then read back and `profile::check` | [`profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/profile.rs) |
| Version | `user_version` 36–38 or 42–44 (schema version 2, #91); anything else, including the schema version 1 Stores 16–18 and 22–24, `Integrity` | [`connection.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/connection.rs) |
| Checkpoint thread | Writable Disposable session only: a second `READ_WRITE` connection on its own thread; failure to start it refuses the open. See [Lifecycle: Background checkpoint](lifecycle.md#background-checkpoint) | [`background.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/background.rs) |

| Call | Transaction | Takes the writer lock | Source |
| --- | --- | --- | --- |
| `run(false, ..)` | `BEGIN` .. `COMMIT` | no | [`transaction.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/transaction.rs) |
| `run(true, ..)` | `BEGIN IMMEDIATE` .. `COMMIT` | yes, unless nested in the lane | same |
| `read_mapped` | one read-only statement, implicit snapshot | no | same |
| `read_immutable` | no `BEGIN`; each statement its own snapshot | no | same |

`read_immutable` is limited to rows that never change once visible:
`object_location` (insert-only), `pack` and `pack_unit` (triggers refuse
UPDATE and DELETE), and `inode_value_block` (append-only blocks; an INSERT of
a block whose ordinals a stored block holds inserts nothing and fails the
publication). Anything that
picks a snapshot, such as Branch heads or Commit rows, uses `run`. Each turn
also adds its SQL work to the calling thread's counters
([`thread_work.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/thread_work.rs)).

## The lane

```text
 Session::hold_lane()                         LaneHold drop
   writable? else ReadOnly                      writer.release()
   admission.enter(Writer)                        fails -> quarantine,
     nested? -> Integrity                         keep the lock
   Writer::acquire (directory flock)            admission.release_hold()
   turn.hold()  holder = this thread
   -> LaneHold { session, writer }
          |
          v  inside: run(true, ..) enters nested: no flock, BEGIN IMMEDIATE
```

The lane is one held admission turn plus, on Linux, the directory lock, kept
by one thread across several short transactions. No publication of any
process interleaves. `LaneHold` is `!Send`: it belongs to the thread that took
it. Source: [`lane.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lane/lane.rs).

## Heads and snapshots

| Function | What it returns | Source |
| --- | --- | --- |
| `read_heads(path)` | Sorted lines `stack <name> <head_layer_id>` and `branch <project> <name> <base_layer_id> <head_commit_id or ->`, ids in lowercase hex | [`heads.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/heads.rs) |
| `heads_digest(path)` | sha256 of those lines, each ending in `\n` | same |
| `snapshot(destination)` | A closed WAL copy made by `VACUUM INTO` | [`snapshot.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/snapshot.rs) |

`read_heads` opens its own connection read-write but `query_only`, because
SQLite cannot open a WAL file read-only without a `-shm`. It reads in one
transaction and closes; a busy Store is reported, never waited on. The heads
change exactly when history changes, so the digest identifies a backup copy
([Control: Backups](../../control/backups/README.md)).

## Acquisition

`AcquisitionProvider` gives Project Init indexed working tables
(`init_operation`, `init_entry`, `init_host_file`) in the same Store file, so
a native directory is imported in bounded windows rather than in memory. Each
unit is one transaction or one read snapshot on the shared session; a released
or foreign operation is refused first. A Store created without
`SqliteAcquisitionSchema::Tables` refuses every unit. Source:
[`acquisition/`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/acquisition).
The import algorithm is in [Project Init: Import](../project-init/import.md).

## Lifecycle

```text
 live Store (WAL, writers continue)        closed Store
   snapshot(destination)                     seal()  (sole owner only)
     own READ_ONLY connection                  checkpoint TRUNCATE, close
     VACUUM INTO destination                   no sidecar left
     reopen copy, journal_mode=wal             macOS: release extents
     wal_checkpoint(TRUNCATE), close
     check user_version, application_id,
     page_size, auto_vacuum; no sidecar
          \                                   /
           +-------> SealedStore <-----------+
```

| Operation | Bound or rule | Source |
| --- | --- | --- |
| Free-space guard | A pack batch must leave `STORE_HEADROOM_BYTES` (64 MiB) after counting its bytes twice (WAL, then file); else `Capacity` | [`headroom.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/headroom.rs) |
| `reclaim_space(pages)` | `1..=RECLAMATION_PAGE_LIMIT` (512) pages by `incremental_vacuum`, one short write; only Stores created with `auto_vacuum=2` | [`reclamation.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/reclamation.rs) |
| `seal` | Sole session; see [Lifecycle: Seal](lifecycle.md#seal) | [`seal.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/seal.rs) |
| `snapshot` | Destination must not exist; a busy source is `Busy`; a failed copy may leave a file for the caller to remove | [`snapshot.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/snapshot.rs) |

## Change export

```text
 closed Store              live Store (writers continue)       offline replica
   mark_base(path)           export_changes(path, cursor,        apply_changes(
   -> base cursor              bundle) -> next cursor              replica, bundle)
   copy = replica              one read snapshot, no writer        one transaction
```

Triggers on every physical and history table except `merge_*` and `init_*`
(19 tables in layouts 37–38, 18 without `pack_unit`) log each inserted,
updated or deleted row's key into `change_log`, in the single writer's commit
order; `pack` and `pack_unit` log inserts only. They log only while
`change_state` holds its row, so a Store that was never marked logs nothing.

| Function | What it does | Source |
| --- | --- | --- |
| `mark_base(path, token)` | Starts a chain on an offline Store: records a fresh state and the current sequence and turns logging on; a copy of the closed file is a replica at the returned cursor | [`base.rs`](../../../../engine/crates/layerfs-persistence/src/replication/base.rs) |
| `export_changes(path, after, bundle)` | Writes every row changed after `after`, at its current value, and the deleted keys into a new bundle file (SQLite, the logged tables' exact definitions); written under a temporary name and linked into place | [`export.rs`](../../../../engine/crates/layerfs-persistence/src/replication/export.rs) |
| `apply_changes(replica, bundle)` | Checks the bundle's meta, starting state, digest and ending state, then installs it in one transaction with foreign keys checked at commit; rows it installs are not logged on the replica | [`apply.rs`](../../../../engine/crates/layerfs-persistence/src/replication/apply.rs) |

Each bundle's ending state derives from its starting state and its digest, so
a bundle applies only to the state it was exported from, and two replicas that
diverge never share a later state. The digest detects damage and misordering;
it is not a signature. `mark_base` and `apply_changes` checkpoint after they
commit and return `NotCheckpointed` when another reader blocks it. A Store
written before these tables gains them at its first writable open, as it does
the lineage, merge and digest tables.

## Related

- [Lifecycle](lifecycle.md): creation, open and the profile.
- [Backends](backends.md): admission and the writer lock.
- [History publication](history-publication.md): what the history tables record.
- [Metadata and objects](metadata-and-objects.md): what the physical tables record.
- [PostgreSQL](postgresql.md): the same rows on the other engine.

---

← [Store: Backends](backends.md) · [Up: Store](README.md) · [Store: PostgreSQL](postgresql.md) →
