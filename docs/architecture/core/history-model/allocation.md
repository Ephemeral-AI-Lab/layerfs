# Allocation

[Architecture](../../README.md) › [Core](../README.md) › [History model](README.md) › Allocation

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Scoped allocation of history identities.

What is allocated is inode serials, not history identities: Stack, Branch and
Workspace identities come from the caller, and Commit and Layer identities are
derived ([Entities](entities.md#identity)). The contract types live in
`layerfs-history`; the allocator itself is in `layerfs-persistence`.

| | |
| --- | --- |
| Component | [History model](README.md) |
| Source | [`core/crates/layerfs-history/src/lib.rs`](../../../../core/crates/layerfs-history/src/lib.rs), [`core/crates/layerfs-history/src/contract`](../../../../core/crates/layerfs-history/src/contract); allocator: [`engine/crates/layerfs-persistence/src/history/allocation.rs`](../../../../engine/crates/layerfs-persistence/src/history/allocation.rs), table `scope_allocator` in [`sql/sqlite/history.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/history.sql) |

## Scoped allocation

```text
 scope_allocator row (one per scope)
 +----------------+-----------+--------------+
 | scope_id (32)  | highwater | authority_id |
 +----------------+-----------+--------------+
                       |
 reserve_inodes(scope, count)   one write transaction
   1. read row; absent -> insert with highwater 0
   2. authority_id != CatalogId -> Integrity("scope authority")
   3. end = highwater + count; end >= i64::MAX -> Capacity("inode serials")
   4. UPDATE highwater = end WHERE highwater = old  (1 row, else Integrity)
   5. return Reservation { scope, start: old + 1, count }

 serials:  0 | 1 2 3 | 4 5 | 6 7 8 9 ...
             | first | 2nd |
             +-------+-----+---> highwater after each reservation: 3, 5
```

A scope is an `ObjectId` carried by the LayerStack (`LayerStackRecord::scope`)
and shared by its Branches and Commits. Every `CommitRequest` names it, and
`commit_branch` refuses a scope that differs from the stack's
([`commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs)).

| Type or rule | Meaning | Source |
| --- | --- | --- |
| `ReserveRequest { scope, count }` | Ask for `count` serials; `count == 0` is `InvalidInput("inode reservation")` | [`records.rs`](../../../../core/crates/layerfs-history/src/contract/records.rs), [`allocation.rs`](../../../../engine/crates/layerfs-persistence/src/history/allocation.rs) |
| `Reservation { scope, start, count }` | Half-open range `[start, start + count)`; the first serial of a scope is 1 | same |
| `Reservation::end()` | Exclusive end; above `i64::MAX` it is `Capacity("inode reservation")` | [`records.rs`](../../../../core/crates/layerfs-history/src/contract/records.rs) |
| Consumed on return | A range is spent when handed out, whether used, discarded or followed by a failed operation | [`allocation.rs`](../../../../engine/crates/layerfs-persistence/src/history/allocation.rs) |
| No recycling | Unused serials are never reclaimed, and no root scan recovers them | same |
| Scope survives removal | `remove_layerstack` keeps the scope row, so its serials are not reused | [`removal.rs`](../../../../engine/crates/layerfs-persistence/src/history/removal.rs) |
| Per-scope independence | Each scope starts at 1 from its own high-water mark | [`history_allocation.rs`](../../../../engine/crates/layerfs-persistence/tests/history_allocation.rs) |

### Who reserves

| Caller | Reservation | Source |
| --- | --- | --- |
| Project Init | One range of `scanned.entries` serials before the namespace is built; its `start` seeds the tree | [`import/init.rs`](../../../../engine/crates/layerfs-project/src/import/init.rs) |
| Mounted Workspace | Refill windows of `SERIAL_REFILL` (1024) through the `InodeSerials` trait | [`workspace/serials.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/serials.rs) |
| SDK file calls | One reservation of at least `POOL_BLOCK` (4096); the rest stays in a per-Store, per-scope memory pool | [`store/ports.rs`](../../../../engine/crates/layerfs-store-service/src/store/ports.rs) |

Project Init derives its scope with `scope_for_seed`: the `ObjectId` of
`layerfs/inode-scope/v1` followed by a 32-byte seed
([`filesystem/root.rs`](../../../../core/crates/layerfs-content/src/filesystem/root.rs)).

## Related

- [Entities](entities.md): the LayerStack that carries the scope.
- [Catalog and queries](catalog-and-queries.md): `reserve_inodes` among the other catalog writes.
- [Filesystem trees](../filesystem-trees/README.md): the inodes these serials name.
- [Engine: Store](../../engine/store/README.md): the provider that holds `scope_allocator`.

---

← [History model: Catalog and queries](catalog-and-queries.md) · [Up: History model](README.md) · [Core: Telemetry](../telemetry/README.md) →
