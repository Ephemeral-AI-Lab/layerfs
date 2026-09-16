# LayerFS 0.1.6 storage format

> **Status:** LayerFS 0.1.6 Developer Preview manual.

## File and connection

A Store is one local SQLite database file. Create refuses an existing path.
Connect validates the exact supported schema and obtains exclusive ownership;
never inspect a Store through another connection while its owner writes it.
SQLite must support STRICT tables (3.37.0 or later). New Stores use:

```text
application_id=0x4c46534c
user_version=10
page_size=4096
foreign_keys=ON
journal_mode=MEMORY
synchronous=OFF
temp_store=MEMORY
cache_size=-32768
cache_spill=OFF
mmap_size=0
threads=0
locking_mode=EXCLUSIVE
busy_timeout=5000ms
```

Connect accepts supported Stores with 4096- or 65536-byte pages and preserves
their page size. The page cache is a named SQLite allowance, not a total process
memory limit. The Store serializes SQLite access. For managed container
execution, the host owns SQLite, canonical construction/publication and physical
spool; the container owns daemon/FUSE/workload execution.

Publication is readable from the same live local Store process. MEMORY journaling
and synchronous OFF provide no process-crash, OS-crash or power-loss durability
guarantee. Filesystem fsync does not upgrade this database profile. The bounded
fsync batching used on ordinary write bursts reduces the number of backing
fences; it does not change this contract.

## Compatibility and migration

Create produces **schema 10**. Connect accepts exact schema 10 and supported
legacy schema 6, 7, 8 and 9; it does not promote them, and writes retain each
schema's supported construction/encoding policy. Schema-6 Stores read and write
legacy version-1 packs; a research schema-6 Store containing native packs
without a writer fence is rejected during preflight.

`LayerStackStore::upgrade_format(path)` is the explicit offline promotion for a
closed schema-7, 8 or 9 Store: read-only preflight, exclusive revalidation, then
one SQLite transaction using DELETE journal/FULL synchronous. It promotes to
schema 9 without rewriting payloads, history or page size, is an idempotent
no-op on schema 9, and errors on every other version. There is **no in-place
promotion to schema 10**: schema-10 Stores are created by `create`. Failure
before COMMIT preserves the original schema; failure after COMMIT reports
promotion. Require no live owners and keep a backup before any promotion.

Published v0.1.3 uses schema 5, which v0.1.6 rejects. Schema 4 and other
unsupported versions, unexpected schema objects and WAL-mode Stores are also
rejected. There is no in-place migration, downgrade or retained-history transfer
command. Retain the original Store with its matching binaries; directory import
into a new Store copies a selected filesystem state and starts new history.
Canonical identity preservation does not imply file-format compatibility.

## No format change in v0.1.6

v0.1.6 does not move the storage boundary. `SCHEMA_VERSION` stays **10**, no
schema or static SQL change landed between v0.1.5 and v0.1.6 (`crates/layerfs-layerstack-store/sql`
is untouched), and the crates that define canonical identity
(`layerfs-content`, `layerfs-layerstack-store`) are byte-identical to the v0.1.5
release. Every benchmark receipt of the v0.1.6 qualification observed schema 10.
The v0.1.6 change is the runtime route — where live mutable state lives and how it
reaches the Commit — not the format; see the
[v0.1.6 release contract](../../../release-notes/0.1.6/release-contract.md).

## Explicit compaction is removed

v0.1.5 removed the product compaction route and v0.1.6 keeps it removed: there is no
`LayerStackStore::compact_into`, no compaction options or receipt export, and no
`layerfs-store-compact` binary. The [removal decision](../../roadmap/0.1/0.1.5/compaction-removal.md)
records the owner rationale and the historical compaction measurements, which
remain historical evidence rather than a current product step or storage claim.

Stores compacted by an earlier build remain **readable**: the authenticated
whole-file record path (`LFCNT1` magic, version 107) is retained with its
whole-owner/native-slice reconstruction and dependency traversal, and no old
data is deleted or reinterpreted. That path is read compatibility only; its
writers exist only in tests. Do not reintroduce compaction as background or
asynchronous rewriting, mandatory maintenance or hidden benchmark preparation.
Storage work happens on the ordinary admission/publication path.

## Schema

Schema 10 has nine STRICT tables and 35 columns, with seven named unique indexes
and one table-level unique constraint:

| Table | Columns | Role |
| --- | ---: | --- |
| `scope_allocator` | 2 | Per-scope never-recycled inode/identity high-water marks |
| `object_packs` | 2 | Physical pack ID and packed bytes |
| `metadata_value_groups` | 5 | Pooled authenticated shared metadata values and their digest |
| `objects` | 5 | ObjectId, canonical length and pack/group/record locator |
| `commits` | 4 | Immutable Commit root, parent and base Layer |
| `branches` | 5 | Named Branch and publication pointers |
| `layer_stacks` | 3 | Named LayerStack and head Layer |
| `layers` | 6 | Immutable Layer lineage and root |
| `workspace_stages` | 3 | Workspace-owned completed root awaiting retirement |

`object_packs` uses an INTEGER PRIMARY KEY; the other tables use WITHOUT ROWID.
The exact [schema-10 DDL](../../../crates/layerfs-layerstack-store/sql/schema/v10.sql),
the [legacy schema-6 DDL](../../../crates/layerfs-layerstack-store/sql/schema/v6.sql)
and [connection validation](../../../crates/layerfs-layerstack-store/src/schema.rs)
are authoritative. A stage is not a Branch head or a restartable Workspace.

`scope_allocator` burns a serial range inside its own durable transaction before
canonical construction; a failed candidate cannot roll a burned range back or
reuse an identity it exposed. `metadata_value_groups` stores shared metadata
values once per group with a 32-byte digest inside `object_packs`; schema-10
namespaces use the compact inline framing and the pooled value index, while
earlier schemas keep their own namespace representation.

## Canonical identity and physical encoding

Content-defined chunking, canonical bytes, ObjectId domains, CAS/COW semantics
and immutable Layer/Commit identities are preserved. Physical pack representation
is separate from canonical identity. Locators identify records; reads validate
framing, reconstruct the canonical bytes and authenticate their ObjectId.

Regular-file content below 128 KiB uses the whole-file canonical role
(`LFS5SML\0`, version 1, 1..131071 raw bytes; canonical length = raw length +
23). Equal new small content has one ObjectId across names, Init and edit
surfaces regardless of its physical FULL/DELTA encoding. Larger regular-file
content uses content-defined chunking and extents; eligible chunk slices can be
owned by native FULL/PREFIX records, and unsupported input retains the canonical
fallback. Dependency ordering and validation are required before publication.
See the [small-content role](../../../crates/layerfs-content/src/file/content.rs),
[pack grammar](../../../crates/layerfs-layerstack-store/src/objects/pack.rs) and
[read implementation](../../../crates/layerfs-layerstack-store/src/objects/read.rs).

Packs use the `LFPACK\0\0` magic, a 16-byte header and a 16-byte group directory
with at most 256 groups. An ordinary pack is bounded at 256 KiB and an ordinary
group at 64 KiB; native records are bounded at 32 KiB raw / 33,024 bytes framed.
Pack version 3 adds small-content groups (one SmallContent object per group,
`record_number` 0, RAW group codec, ≤192 KiB group, ≤135,168-byte frame) with
kinds FULL, FULL-base DELTA and bounded-chain DELTA. A chain uses at most 8
edges, at most 512 KiB summed canonical bytes including the target and at most
256 KiB retained encoded record capacity. Oversized canonical objects use the
explicitly bounded legacy fallback. These physical limits are distinct from SQL
transaction and operation ownership limits. They do not authorize changing
object bytes under an existing ObjectId.

## Admission, reuse and publication

Physical batches remain bounded. Coalesced SQL transactions contain at most
8,191 objects and no more than 4 MiB canonical payload. Ordinary pack rows are
coalesced rather than appended one row per pack. Other candidate, queue, index,
reconstruction and SQLite allocations have separate limits.

Directory Init can retain authenticated comparison bytes across admission
batches within its existing owner allowance: a 2 MiB reservation replaces part
of the previous allowance rather than raising it. Every reuse still performs
locator lookup, validates physical location and canonical length, and compares
exact bytes. An ID, membership-filter hit or earlier occurrence alone is never
sufficient collision validation. Ineligible or uncached records use the normal
authenticated read path. The cache is owner-scoped and bounded.

Admission validates dependencies and completes selected output before exposing
its final root. Pending and earlier committed batches owned by failed admission
are rolled back through the checked ownership path; failed cleanup quarantines
writes. Root publication remains atomic within the live Store transaction.

Workspace publication validates its retained stage and expected Branch base/head,
inserts or verifies the immutable Commit, conditionally advances the Branch and
retires the stage. No-op publication can retire a stage without creating history.
A stale head returns the competing state. Presentation failure after successful
publication does not undo that Commit; use the [SDK recovery API](sdk.md#commit-and-recovery).
Explicitly completed admission and subsequently rejected publication are distinct
boundaries: unreachable objects may remain, and automatic garbage collection is
not provided. These live-process integrity rules do not promise crash durability.
