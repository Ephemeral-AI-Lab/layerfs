# Metadata and objects

[Architecture](../../README.md) › [Engine](../README.md) › [Store](README.md) › Metadata and objects

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Allocation, locations, signatures and object reads.

| | |
| --- | --- |
| Component | [Store](README.md) |
| Source | [`engine/crates/layerfs-persistence/src/metadata`](../../../../engine/crates/layerfs-persistence/src/metadata), [`engine/crates/layerfs-persistence/src/objects`](../../../../engine/crates/layerfs-persistence/src/objects), [`engine/crates/layerfs-persistence/src/storage`](../../../../engine/crates/layerfs-persistence/src/storage) |

```text
 core Storage (Save, Reader)  -- PackPersistence port -->  StorageProvider
                                                             |
              +----------------------+-----------------------+
              v                      v                       v
        metadata/              objects/read.rs        storage/publication.rs
  policy, allocation,          read_packs             validate + publish
  locations, values,                 |                       |
  signatures                         |                       |
              \                      |                       /
               +---------> Mechanics (engine rows) <--------+
```

`StorageProvider` (SQLite) and `PostgresStorage` implement the core
`PackPersistence` port. The shared modules check counts and identities around
each engine call; the engine runs the SQL
([`provider.rs`](../../../../engine/crates/layerfs-persistence/src/storage/provider.rs)).
How Saves and reads use the port is in
[Core: Physical storage](../../core/physical-storage/README.md).

## Allocation

| Step of `reserve(Reserve { packs, ordinals })` | Rule | Source |
| --- | --- | --- |
| Bound the request | `packs <= TRANSACTION_ROW_LIMIT` (8 191), `ordinals <= METADATA_INDEX_VALUES` | [`allocation.rs`](../../../../engine/crates/layerfs-persistence/src/metadata/allocation.rs) |
| Read the cursor | `store_policy.next_pack_id`, `next_ordinal` | [`sqlite/metadata/allocation.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/metadata/allocation.rs) |
| Advance | new end pack `> 0`; end ordinal `<= 2^32`; else `Capacity` | [`allocation.rs`](../../../../engine/crates/layerfs-persistence/src/metadata/allocation.rs) |
| Return | `Reserved { first_pack_id, first_ordinal }` (0 when none asked) | same |

`reserve` is one write transaction. Pack ids never go back. A publication
may give back an unused ordinal tail (`release_ordinals`) only while
`next_ordinal` still equals the end of that tail, so nothing written under it
is reused. Inode serials of a Workspace scope are a separate history
allocator ([History publication](history-publication.md#branch-and-commit-rows)).

## Locations and signatures

| Read | Bound | Checks | Source |
| --- | --- | --- | --- |
| `locate(ids)` | at most `READ_OBJECT_LIMIT` (4 096) ids | each returned row was asked for, once; pack id matches; length `1..=CANONICAL_LIMIT`; group `< 256`, record `< 8 191` | [`locations.rs`](../../../../engine/crates/layerfs-persistence/src/metadata/locations.rs) |
| `signatures()` | at most 8 192 rows | `slot < 8192`, `stamp > 0`, `(stamp - 1) % 8192 == slot`; returned in stamp order | [`signatures.rs`](../../../../engine/crates/layerfs-persistence/src/metadata/signatures.rs) |
| `policy()` | one row | `StoragePolicy::validated` | [`policy.rs`](../../../../engine/crates/layerfs-persistence/src/metadata/policy.rs) |

`object_location` is insert-only (`ON CONFLICT DO NOTHING`): the first
publication of an object wins and later ones report it as `lost`. The
signature ring is an upsert by slot that keeps the newer stamp
([`sqlite/metadata/signatures.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/metadata/signatures.rs)).

## Values

```text
 inode_value
 +-----------+------------------------------+
 | ordinal   | value (73 bytes)             |   one row per ordinal,
 | 1..2^32-1 |                              |   INSERT only, never updated
 +-----------+------------------------------+
 read: ascending distinct ordinals -> JSON runs [[first,last],..]
       -> one statement: json_each JOIN inode_value BETWEEN first AND last
```

`inode_values(ordinals)` requires ascending, distinct, nonzero ordinals (at
most 4 096) and refuses a reply that names an ordinal not asked. A second
INSERT of an ordinal fails its transaction. Sources:
[`values.rs`](../../../../engine/crates/layerfs-persistence/src/metadata/values.rs),
[`sqlite/metadata/values.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/metadata/values.rs).

## Object reads

| Read | Snapshot | Bound | Source |
| --- | --- | --- | --- |
| `read_packs(ids)` | `read_immutable` | at most 4 096 ids, each `> 0`; body length must equal the descriptor | [`objects/read.rs`](../../../../engine/crates/layerfs-persistence/src/objects/read.rs) |
| `read_pack_selection(id, plan)` | `read_immutable` | exact offset and length BLOB reads planned by core | [`selection.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/objects/selection.rs) |
| `read_scoped_pack(id, plan)` | `read_immutable` | as above, for acquisition | same |

On layouts 17 and 18 a pack is a `control` row plus one `pack_unit` row per
group, so a selected group is read without the rest of the pack
([`units_read.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/objects/units_read.rs)).
PostgreSQL reads ranges of the `EXTERNAL` body with `substring`
([`postgresql/objects/read.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/objects/read.rs)).
All of these run without `BEGIN` because the rows they read never change once
visible ([SQLite: Sessions](sqlite.md#sessions)).

## Storage publication

```text
 publish(Publication { packs, objects, inode_values, signatures,
                       release_ordinals })
   validate (no SQL)                       publication.rs
     rows <= 8 191; canonical and physical bytes <= 4 MiB - 1
       (a single payload pack with one object is exempt)
     pack ids unique and > 0; length 32..=SINGLETON_PACK_LIMIT;
       metadata packs <= 256 KiB; key = hash(body); header lane = domain
     object ids unique; ordinals unique, nonzero; signature slots valid
   SQLite: pack bytes leave >= 64 MiB free (counted twice)   headroom.rs
   one write transaction                   sqlite/objects/publish.rs
     INSERT pack (or control + pack_unit rows)
     INSERT object_location ... ON CONFLICT DO NOTHING RETURNING
     UPDATE store_totals (only when bytes were added)
     INSERT inode_value
     give back the ordinal tail (release_ordinals)
     UPSERT content_signature
   -> Published { lost }   objects another publication registered first
```

Each publication is its own short write through the one writer; a Save
publishes many of them and none holds the publication lane. On PostgreSQL the
pack bodies go first in a `Stage` transaction and the rows in a second, lane
ordered one ([PostgreSQL: Transactions](postgresql.md#transactions)).
Sources: [`publication.rs`](../../../../engine/crates/layerfs-persistence/src/storage/publication.rs),
[`sqlite/objects/publish.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/objects/publish.rs),
[`units_publish.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/objects/units_publish.rs).

## Related

- [Core: Physical storage](../../core/physical-storage/README.md): packs, Saves and the port.
- [SQLite](sqlite.md#schema): the tables these rows live in.
- [Backends](backends.md): `Mechanics` and the writer.
- [History publication](history-publication.md): the history rows that name these roots.

---

← [Store: History publication](history-publication.md) · [Up: Store](README.md) · [Engine: Store calls](../store-calls/README.md) →
