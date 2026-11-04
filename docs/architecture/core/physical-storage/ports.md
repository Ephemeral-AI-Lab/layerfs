# Ports

[Architecture](../../README.md) › [Core](../README.md) › [Physical storage](README.md) › Ports

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The ports a persistence backend implements.

| | |
| --- | --- |
| Component | [Physical storage](README.md) |
| Source | [`core/crates/layerfs-storage/src/port`](../../../../core/crates/layerfs-storage/src/port), [`core/crates/layerfs-storage/src/store`](../../../../core/crates/layerfs-storage/src/store) |

```text
 core: layerfs-storage                  engine: layerfs-persistence
 +---------------------------+          +---------------------------------+
 | Storage --> Fetch --------+--------->| PackPersistence                 |
 |   Save, Reader            |  trait   |   StorageProvider (SQLite)      |
 |                           |          |   PostgresStorage (PostgreSQL)  |
 | Project Init (engine) ----+--------->| Acquisition                     |
 |                           |  trait   |   AcquisitionProvider (SQLite)  |
 |                           |          |   PostgresAcquisition           |
 +---------------------------+          +---------------------------------+
   storage owns formats, checks          provider owns SQL, transactions,
   and authentication                    locks and physical I/O
```

Implementations: [`storage/provider.rs`](../../../../engine/crates/layerfs-persistence/src/storage/provider.rs),
[`backend/postgresql/storage/provider.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/storage/provider.rs),
[`backend/sqlite/acquisition/provider.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/acquisition/provider.rs),
[`backend/postgresql/acquisition/provider.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/acquisition/provider.rs).
Only the Disposable SQLite profile (WAL, `synchronous=OFF`) may be run; see
[Engine: Store](../../engine/store/README.md).

## Persistence ports

`PackPersistence` ([`persistence.rs`](../../../../core/crates/layerfs-storage/src/port/persistence.rs)).
Every call is one attempt with no recovery loop.

| Method | Contract |
| --- | --- |
| `policy` | Read the persisted `StoragePolicy`, once per handle. |
| `locate` | At most 4,096 distinct ids; absent ids are omitted. |
| `read_packs` | Whole packs totalling ≤ 2 MiB, or one singleton ≤ 16 MiB + 4 KiB, each as a `PersistedPack`. |
| `read_pack_selection` | Strict selected read under a storage-owned `PackReadPlan`; the whole pack is hashed. Default: `BackendUnavailable`. |
| `read_scoped_pack` | Selected read for canonical consumers; returned units are not hashed. Default: `BackendUnavailable`. |
| `inode_values` | At most 4,096 ascending ordinals in one statement; missing ordinals omitted. |
| `signatures` | At most 8,192 signature-ring rows, in insertion order. |
| `reserve` | One block of consecutive pack ids and/or ordinals (`Reserve` → `Reserved`). |
| `publication_pack_cost` | Rows and bytes one pack adds to a publication. Default: one row, the body length. |
| `publish` | One atomic `Publication`: ≤ 8,191 rows and the byte budgets, locators insert-if-absent. Returns `Published { lost }`. |

| `Publication` field | Meaning |
| --- | --- |
| `packs` | `PublishedPack { info: PackInfo, body }` for each sealed pack, both domains |
| `objects` | `ObjectLocation` rows; the first writer wins |
| `inode_values` | `InodeValueRow { ordinal, value[73] }` from this Save's window |
| `signatures` | `SignatureRow` ring changes |
| `release_ordinals` | unused ordinal tail `(first, count)` |

| `PersistenceError` | Meaning | Becomes `StorageError` |
| --- | --- | --- |
| `Busy` | lock or session held; no write effect | `Busy` |
| `BackendUnavailable` | engine unavailable; no fallback | `Io` |
| `Missing` | required state absent | `Io` |
| `Refused { status }` | definite refusal; `"Capacity"` means out of space | `Io`; `is_capacity_refusal()` |
| `Malformed` | reply breaks the port grammar or bounds | `Io` |
| `Uncertain` | acknowledgement or outcome unknown | `UnknownOutcome` |

Mapping: `From<PersistenceError> for StorageError` in
[`store/error.rs`](../../../../core/crates/layerfs-storage/src/store/error.rs).

## Acquisition and read ports

Read carriers ([`read_pack.rs`](../../../../core/crates/layerfs-storage/src/port/read_pack.rs),
[`read_selection.rs`](../../../../core/crates/layerfs-storage/src/port/read_selection.rs),
[`read_scoped.rs`](../../../../core/crates/layerfs-storage/src/port/read_scoped.rs)):

```text
 provider read_at(offset, buf)          storage PackReadPlan
        |                                      ^
        | 1 read 24-byte control area          |
        | 2 read directory (<= 4,120 B prefix) |
        +------------- select(info, prefix) ---+
        |                   |
        |        Whole      |       Ranges
        v                   v
 read rest, check     strict: scan + SHA-256 all bytes -> PersistedPackRanges
 length + SHA-256     scoped: read ranges only          -> AcquiredPackUnits
 -> PersistedPack
```

| Carrier | Guarantee |
| --- | --- |
| `PersistedPack` | Length and SHA-256 equal the `PackInfo`; fields are private, so the bytes cannot change after the check. |
| `PersistedPackRead::Ranges` | Every byte was read once and hashed; only the chosen ranges are returned. |
| `AcquiredPackRead::Units` | Framing is checked; no whole-pack claim. Each object used is authenticated by its id. |

Ranges must be non-empty, ascending, non-overlapping, ≤ 4,096, inside the
pack, and with the prefix and scan buffer ≤ 2 MiB. A metadata pack over 256 KiB
is `Malformed`.

`Acquisition` ([`acquisition/contract.rs`](../../../../core/crates/layerfs-storage/src/port/acquisition/contract.rs))
is the working-row port of one initial root acquisition in
[Project Init](../../engine/project-init/import.md). It states typed units;
no SQL text, table name or connection crosses it.

| Part | Rule |
| --- | --- |
| Owner | `Owner { operation, epoch }` from `begin`; a stale owner fails `Stale`. |
| Phases | `Scanning`, `Files`, `Tree`, `Discarding`, `Failed`, `Uncertain` |
| Read window | ≤ 512 rows and 256 KiB after the first row; resumed from the last key |
| Write window | ≤ 4,096 rows and 1 MiB; 160 bytes charged per row besides name and path |
| Order | breadth first; each directory's children by name bytes |
| Cleanup | `discard`/`dispose` remove a bounded number of rows; `release` needs zero rows; `discard_abandoned` assumes the old owner is fenced |
| Errors | `Persistence`, `Stale`, `Bounds`, `Changed { position }`; nothing is retried |

## Store handle, policy and location

| Type | Role | Source |
| --- | --- | --- |
| `Storage` | The handle: reads the policy once, derives capacities, owns the locator cache and signature ring; `begin_save`, `reader`, `diagnostics`, `save_work`. | [`store/handle.rs`](../../../../core/crates/layerfs-storage/src/store/handle.rs) |
| `StoragePolicy` | Persisted row: format profile 1, small-file cutoff, whole-file, chunk and metadata delta depths. `validated` refuses anything else. | [`store/policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs) |
| `StorageCapacities` | Every bound derived from the policy by checked arithmetic. | same |
| `ObjectLocation`, `LocatedObject` | Object id, role, canonical length, pack, group, record; with its `PackInfo`. | [`store/location.rs`](../../../../core/crates/layerfs-storage/src/store/location.rs) |
| `PackInfo`, `PackDomain` | Pack id, `Metadata` or `Payload`, `ObjectKey` (SHA-256), exact length. | same, [`port/pack.rs`](../../../../core/crates/layerfs-storage/src/port/pack.rs) |
| `InodeValueRow`, `SignatureRow` | One stored inode value; one signature-ring slot. | [`store/location.rs`](../../../../core/crates/layerfs-storage/src/store/location.rs) |
| `Source` | Internal read view the encoders use; implemented by `Fetch` and by a Save's wave view. | [`store/source.rs`](../../../../core/crates/layerfs-storage/src/store/source.rs) |
| `StorageError` | Typed failures; `is_unknown_outcome`, `is_capacity_refusal`. Read failures reach `layerfs-content` as `MissingObject` only for absence. | [`store/error.rs`](../../../../core/crates/layerfs-storage/src/store/error.rs) |

## Related

- [Saves](saves.md) and [Reads](reads.md) — the callers of these ports.
- [Engine: Store backends](../../engine/store/backends.md) — the implementations.
- [Engine: Project Init](../../engine/project-init/README.md) — the `Acquisition` caller.

---

← [Physical storage: Reads](reads.md) · [Up: Physical storage](README.md) · [Core: History model](../history-model/README.md) →
