# 06 — Proposed tables

> **Status:** Current planning checklist; no release candidate exists.
> Part of the [Phase 7 packet](README.md). Proposal for review in M0; column
> lists show role, not final types or constraints. The PostgreSQL tables are
> derived from the Phase 4.5 schemas at `7edddbdb8`
> (`layerfs-storage/sql/schema.sql`, `layerfs-history/sql/schema-v1.sql`).

> **Superseded 2026-10-05.** The overlay tables of §1 are replaced by
> [`../303/02-base-overlay.md`](../303/02-base-overlay.md#3-schema-direction-replacement-payload-contract-required). The
> PostgreSQL tables of §2 and §3 were not built; cluster one's schema is in
> `core/crates/layerfs-persistence/sql/sqlite/`. The text below is unchanged and
> is kept as context.

Seventeen tables in two databases: six in the daemon's SQLite overlay, eleven in
PostgreSQL. File payload bytes for committed content are in no table; they are
objects in MinIO.

## 1. Daemon SQLite — overlay (`layerfs-overlay`)

Every key starts with `ws`, a small integer for one Workspace incarnation, and
every versioned row carries `gen`. Rules: [02 §4](02-workspace-overlay.md#4-the-three-rules).

| Table | Key | Other columns | Purpose |
| --- | --- | --- | --- |
| `ws` | `ws` | workspace, incarnation, branch, base_commit, base_root, folded_gen, frozen_gen, active_gen, commit_state, candidate, next_ino, end_ino | One row per open Workspace: its base, its three generation numbers, pending Commit custody, reserved inode range |
| `inode` | `(ws, ino, gen)` | kind, mode, nlink, size, mtime, inherit_len | Attributes of an inode changed in that generation |
| `dentry` | `(ws, parent, name, gen)` | ino (NULL = whiteout) | A name added, moved or removed in that generation |
| `block` | `(ws, ino, blk, gen)` | data | Changed payload, one row per block |
| `xattr` | `(ws, ino, key, gen)` | value (NULL = removed) | Extended attributes |
| `handle` | `(ws, fh)` | ino, flags | Open file handles; keeps an unlinked inode alive |

Settings for this database: [02 §10](02-workspace-overlay.md#10-sqlite-settings).

## 2. PostgreSQL — storage side (`layerfs-metadata`, C2's port)

| Table | Key | Other columns | Purpose | Phase 4.5 origin |
| --- | --- | --- | --- | --- |
| `store_policy` | single row | format profile, small-file threshold, delta depth limits | The storage profile every writer must match | `store_policy`, without its counters |
| `pack` | `pack_id` | domain, digest (unique), length, body (metadata domain only) | One row per sealed pack. A payload pack's bytes are in MinIO under its digest; a metadata pack's bytes are in `body` | `object_packs`, whose `data` held every pack |
| `object` | `object_id` | role, canonical_length, pack_id, group_number, record_number | The locator: where one canonical object is. Insert-if-absent; the first row wins | `objects`, keyed there by `(object_id, save_id)` |
| `metadata_value_group` | `first_ordinal` | count, pack_id, group_number, digest | Pooled metadata values | same name |
| `content_signature` | `slot` | stamp, object_id, signature | Bounded advisory table of delta-base candidates | `content_signatures` |

Removed: `saves` and the publication/pack ceilings in `store_policy`. They made
unpublished objects invisible to other writers of one local Store. With
immutable objects registered only after their bytes are acknowledged, a
registered object is valid for anyone to reuse, so that visibility machinery has
no job. **To confirm in M0:** that nothing else in `cas/` depends on a save
identity, and whether collection of unreferenced packs needs a registration
timestamp or a dependency table (`pack` → base `pack`) that is not listed here.

**Keep ranged reads possible (#300).** A reader that wants one compression group
needs that group's byte offset and length, which today are only in the pack's own
directory. Recording them at registration — as a small `pack_group (pack_id,
group_number, offset, length)` table or as the directory bytes on the `pack` row —
lets a later ranged read be a single request without changing packs already
written. Proposed for the M0 schema even though ranged reads themselves are a
nice-to-have ([04 §10](04-storage-interfaces.md#10-ranged-reads-300--nice-to-have)).

## 3. PostgreSQL — history side (`layerfs-metadata`, C5's `HistoryCatalog`)

Carried from `schema-v1.sql` with the same columns and constraints, expressed in
PostgreSQL types.

| Table | Key | Other columns | Purpose |
| --- | --- | --- | --- |
| `history_meta` | single row | catalog_id, incarnation, identity_format | Identity of this history store |
| `layer_stack` | `layer_stack_id` | name (unique), scope_id, profile_id, head_layer_id | A project's stack of Layers |
| `layer` | `layer_id` | layer_stack_id, parent_layer_id, root_id, source_branch_id, source_commit_id | An immutable Layer and the filesystem root it selects |
| `commit` | `commit_id` | layer_stack_id, root_id, parent_commit_id, base_layer_id | An immutable Commit |
| `branch` | `branch_id` | layer_stack_id, name, base_layer_id, **head_commit_id** | The one mutable fact: publish is a conditional update of `head_commit_id` |
| `scope_allocator` | `scope_id` | highwater, authority_id | Inode-serial reservations |

**Open in M0:** `workspace_stages`. Phase 4.5 stages a candidate in the history
store and then commits the stage. In Phase 7 the pending Commit's context lives
in the overlay's `ws` row and publication is one conditional transaction, so the
table is proposed for removal. It stays if the exact-identity lookup for an
uncertain publication (D7) is better served by a global record than by looking
up the candidate Commit directly.

## 4. Not tables

| Data | Where |
| --- | --- |
| File-content pack bytes | MinIO, key `<prefix>/packs/<hh>/<digest>` ([04 §9](04-storage-interfaces.md#9-minio-objects-and-bucket-settings)) |
| Canonical object bytes | Inside packs; never stored unpacked |
| The committed namespace | Not a table: directories and inodes are canonical objects selected by a root, found through `object` and read from `pack.body` |
