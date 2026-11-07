# Physical storage

[Architecture](../../README.md) › [Core](../README.md) › Physical storage

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How objects become bytes on disk: encoding, delta bases, packs, saves, reads and the persistence ports.

| | |
| --- | --- |
| Layer | [Core](../README.md) |
| Crates | `layerfs-storage` |
| Source | [`core/crates/layerfs-storage/src`](../../../../core/crates/layerfs-storage/src) |
| Uses | [Objects](../objects/README.md) |
| Used by | [Engine: Store](../../engine/store/README.md) |
| Flows | [A Commit](../../flows/commit.md), [Project Init](../../flows/init.md) |

The crate takes finalized canonical objects (see [Objects](../objects/README.md))
and stores them as records inside immutable packs keyed by SHA-256. It opens no
database itself. An engine backend implements its
[`PackPersistence`](../../../../core/crates/layerfs-storage/src/port/persistence.rs)
port, and [`Storage`](../../../../core/crates/layerfs-storage/src/store/handle.rs)
is the handle over that port. The SQLite and PostgreSQL providers live in
`layerfs-persistence` (see [Store backends](../../engine/store/backends.md)).

## Topics

| Topic | What it covers |
| --- | --- |
| [Encoding](encoding.md) | Codec frames, full and delta encoding, and decoding. |
| [Packs](packs.md) | Pack layout, assembly and the pack cache. |
| [Saves](saves.md) | How objects are saved: batches, reservations and publication. |
| [Reads](reads.md) | Locating and fetching stored objects. |
| [Ports](ports.md) | The ports a persistence backend implements. |

## The picture

```text
  FinalizedObject (canonical bytes, role, references, predecessors)
        |
        v
 +--------------------- Save (one producer) -----------------------+
 | PendingBatch --> wave --> Encoding: FULL, PREFIX or stored      |
 |  (512 objects,    |        record for each new object          |
 |   4 MiB)          |                |                            |
 |                   |                v                            |
 |                   |        Packs: group -> seal -> assemble     |
 |                   |                |                            |
 |                   +------> Publication batch (reference-closed) |
 +-----------------------------------|-----------------------------+
                                     | reserve() / publish()
                                     v
 +--------------------- Ports: PackPersistence --------------------+
 |  policy  locate  read_packs  read_pack_selection                |
 |  read_scoped_pack  inode_values  signatures  reserve  publish   |
 +-----------------------------------^-----------------------------+
                                     | locate() / read_scoped_pack()
 +--------------------- Reader (one operation) --------------------+
 | locate --> prefetch chains --> acquire groups --> PackCache     |
 |        --> Resolver: walk chain, decode base first, verify id   |
 +-----------------------------------------------------------------+
        |
        v
  canonical bytes, each checked against its ObjectId
```

## What it does

| Question | Answer | Source |
| --- | --- | --- |
| What is a pack? | One immutable byte string: a 24-byte control area, one directory entry per group, then the group bodies. Its key is the SHA-256 of all its bytes. | [`pack/layout.rs`](../../../../core/crates/layerfs-storage/src/pack/layout.rs), [`port/pack.rs`](../../../../core/crates/layerfs-storage/src/port/pack.rs) |
| How is an object found in one? | Its locator row `ObjectLocation` names `pack_id`, `group_number` and `record_number`, with the role and canonical length. | [`store/location.rs`](../../../../core/crates/layerfs-storage/src/store/location.rs) |
| Which lane does an object use? | Its role decides. Whole files go to the whole-file lane, chunks to the chunk lane, and every tree and metadata role to the ordinary lane. A whole-file record too big for a 256 KiB pack goes to the singleton lane. | `PackLane::for_role` in [`pack/layout.rs`](../../../../core/crates/layerfs-storage/src/pack/layout.rs), `plan_lane` in [`encoding/full.rs`](../../../../core/crates/layerfs-storage/src/encoding/full.rs) |
| When is an object stored whole? | Tree roles always, except inode-table branches. A payload when no eligible base exists, when the base chain would exceed its budgets, or when the PREFIX record is not shorter. | [`encoding/delta/select.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/select.rs) |
| When is it a delta? | A whole-file or chunk payload is stored as a Zstandard frame against one base payload (PREFIX). Inode leaves and inode-table branches use a COPY/INSERT program against one base. | [`encoding/delta/record.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/record.rs), [`encoding/pool/delta.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/delta.rs) |
| When is a payload stored verbatim? | When compression does not shrink it, or when a 1 KiB probe saves at most 1/32. The record tag is `STORED_TAG` (2), never with a base. | [`encoding/full.rs`](../../../../core/crates/layerfs-storage/src/encoding/full.rs), [`encoding/codec.rs`](../../../../core/crates/layerfs-storage/src/encoding/codec.rs) |
| Is a stored object written again? | No. A wave locates each id first. A stored id with the same role and length counts as reused and is not read back. | [`save/wave.rs`](../../../../core/crates/layerfs-storage/src/save/wave.rs) |

## Invariants

| Rule | Enforced by |
| --- | --- |
| Reads verify what they decode: every object in a chain is hashed and compared with the id its locator names. | `Resolver::resolve_charged` in [`encoding/delta/read.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/read.rs) |
| A whole pack returned across the port matches its length and SHA-256 key. | `PersistedPack::authenticate` in [`port/read_pack.rs`](../../../../core/crates/layerfs-storage/src/port/read_pack.rs) |
| A pack's SHA-256 key is computed once, when it is sealed; publication does not hash it again. | `PublishedPack::seal` in [`port/persistence.rs`](../../../../core/crates/layerfs-storage/src/port/persistence.rs) |
| The header and directory are validated before any body is touched. No reader trial-decodes. | `parse_control_header`, `visit_directory` in [`pack/layout.rs`](../../../../core/crates/layerfs-storage/src/pack/layout.rs) |
| A chain is at most the role's depth, repeats no id and keeps one role. | [`encoding/delta/read.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/read.rs) |
| A dependency chain decodes at most 4 × cutoff canonical bytes and reads at most 2 × cutoff encoded bytes. Selection refuses a base that would break this. | `CHAIN_CANONICAL_CUTOFFS`, `CHAIN_ENCODED_CUTOFFS` in [`store/policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs); [`encoding/delta/select.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/select.rs) |
| A publication batch is closed under direct references: nothing names an object that is neither stored nor in the batch. | `register_ready` in [`save/publication.rs`](../../../../core/crates/layerfs-storage/src/save/publication.rs) |
| Locators are insert-if-absent. The first writer wins, and a loser's bytes are compared with the winner's. | `Publication`, `Published::lost` in [`port/persistence.rs`](../../../../core/crates/layerfs-storage/src/port/persistence.rs); `acknowledge` in [`save/publication.rs`](../../../../core/crates/layerfs-storage/src/save/publication.rs) |
| One attempt per port call: no retry, no second codec, no error-driven whole-pack fallback. | [`store/error.rs`](../../../../core/crates/layerfs-storage/src/store/error.rs), [`port/persistence.rs`](../../../../core/crates/layerfs-storage/src/port/persistence.rs) |
| `unsafe` is allowed only in the audited zstd module. | `#![deny(unsafe_code)]` in [`lib.rs`](../../../../core/crates/layerfs-storage/src/lib.rs); [`encoding/codec.rs`](../../../../core/crates/layerfs-storage/src/encoding/codec.rs) |
| A Store's policy row is validated at open and never migrated. | `StoragePolicy::validated` in [`store/policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Pack (ordinary, chunk, whole-file lanes) | 256 KiB (`PACK_LIMIT`) | [`store/policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs) |
| Singleton pack | 16 MiB + 4,096 bytes (`SINGLETON_PACK_LIMIT`) | same |
| Groups in a written pack / in a readable older pack | 16 (`TIGHT_GROUPS`) / 256 (`GROUP_COUNT_LIMIT`) | [`pack/layout.rs`](../../../../core/crates/layerfs-storage/src/pack/layout.rs) |
| Records per group | 8,191 (`RECORD_COUNT_LIMIT`) | [`store/policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs) |
| Canonical object | 16 MiB (`CANONICAL_LIMIT`), else `CapacityExceeded { what: "encoding.canonical_length" }` | [`encoding/full.rs`](../../../../core/crates/layerfs-storage/src/encoding/full.rs) |
| Delta depth (defaults) | whole file 16, chunk 8, pooled metadata 8; never above 50 | [`store/policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs), [`layerfs-content` policy](../../../../core/crates/layerfs-content/src/contract/policy.rs) |
| Chain work at the default 128 KiB cutoff | 512 KiB canonical, 256 KiB encoded | [`store/policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs) |
| Chain over budget at read | `Integrity("dependency canonical work")` or `Integrity("dependency encoded work")` | [`encoding/delta/read.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/read.rs) |
| Disk full | `PersistenceError::Refused { status: "Capacity" }`. `StorageError::is_capacity_refusal()` is true only for that definite refusal. | [`store/error.rs`](../../../../core/crates/layerfs-storage/src/store/error.rs) |
| Unknown commit outcome | `StorageError::UnknownOutcome`; never resent | same |
| Unknown pack version | `UnsupportedPolicy { field: "pack framing version" }` | [`pack/layout.rs`](../../../../core/crates/layerfs-storage/src/pack/layout.rs) |

The topic pages list every named bound: [Packs](packs.md), [Saves](saves.md), [Reads](reads.md).

## Demonstration

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`physical_formats.rs`](../../../../core/crates/layerfs-storage/tests/physical_formats.rs) | `layerfs-storage` | Each framing is recognized by its own version, unknown versions are refused, bounds are enforced on the bytes, and written packs size their directory to their groups. |
| [`verified_pack_read.rs`](../../../../core/crates/layerfs-storage/tests/verified_pack_read.rs) | `layerfs-storage` | `PersistedPack` rejects a wrong length or digest; a pack whose hash is valid but whose domain is wrong is still refused. |
| [`delta_base_budget.rs`](../../../../core/crates/layerfs-storage/tests/delta_base_budget.rs) | `layerfs-storage` | A base whose chain exceeds the work budgets is refused and the object is stored FULL; a corrupt base inside the budgets fails the Save. |
| [`capacity_refusal.rs`](../../../../core/crates/layerfs-storage/tests/capacity_refusal.rs) | `layerfs-storage` | Only `Refused { status: "Capacity" }` counts as a full-disk refusal. |

The tests use an in-memory `PackPersistence`
([`tests/support/memory_metadata.rs`](../../../../core/crates/layerfs-storage/tests/support/memory_metadata.rs))
or no provider at all. None has a `cfg(target_os)`, `#[ignore]` or feature
gate, so they run on any host with the pinned toolchain. They need no Docker,
root or environment variables.

```text
cargo +1.85.1 test --locked -p layerfs-storage --test physical_formats
cargo +1.85.1 test --locked -p layerfs-storage --test verified_pack_read
cargo +1.85.1 test --locked -p layerfs-storage --test delta_base_budget
cargo +1.85.1 test --locked -p layerfs-storage --test capacity_refusal
```

These commands were not run while writing this page.

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`core/docs/architecture/05-storage.md`](../../../../core/docs/architecture/05-storage.md) — lanes, records and stored inode values are covered in [Packs](packs.md) and [Encoding](encoding.md); its dated measurement sections are historical only.
- [`core/docs/architecture/09-delta-hints.md`](../../../../core/docs/architecture/09-delta-hints.md) — a proposal about positional hints in file construction; historical only here. Base selection is covered in [Encoding](encoding.md#full-and-delta-encoding).
- [`core/docs/architecture/13-physical-writing.md`](../../../../core/docs/architecture/13-physical-writing.md) — codec, group framing and placement are covered in [Encoding](encoding.md) and [Packs](packs.md); its provider history is historical only.
- [`core/docs/architecture/15-multi-writer-storage.md`](../../../../core/docs/architecture/15-multi-writer-storage.md) — first-wins publication is covered in [Saves](saves.md#publication); its schema-8 writer model is historical only.
- [`core/docs/architecture/63-save-reservation-blocks.md`](../../../../core/docs/architecture/63-save-reservation-blocks.md) — covered in [Saves](saves.md#reservations).
- [`core/docs/architecture/85-commit-per-write-storage.md`](../../../../core/docs/architecture/85-commit-per-write-storage.md) — an explanation of what one Commit stores; historical only.

---

← [Compare and merge: Merge](../compare-and-merge/merge.md) · [Up: Core](../README.md) · [Physical storage: Encoding](encoding.md) →
