# Saves

[Architecture](../../README.md) › [Core](../README.md) › [Physical storage](README.md) › Saves

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How objects are saved: batches, reservations and publication.

| | |
| --- | --- |
| Component | [Physical storage](README.md) |
| Source | [`core/crates/layerfs-storage/src/save`](../../../../core/crates/layerfs-storage/src/save) |

A `Save` is one producer's bounded write. A construction (Project Init or a
Commit) hands it `FinalizedObject`s directly through `Save::accept` or through
`Save::sink`, the `FinalizedConsumer` adapter. `Save::finish` seals and
publishes the rest and returns a `WriteOutcome`.

```text
 caller            Save / State                     PackPersistence
   |  begin_save()     |                                  |
   |------------------>| reserve(packs 4096, ordinals 16384)
   |                   |--------------------------------->|
   |  accept(obj) x N  |                                  |
   |------------------>| PendingBatch.push                |
   |                   |  bound reached -> wave:          |
   |                   |   locate(ids, refs, preds) ----->|
   |                   |   reuse or select + group        |
   |                   |   register_ready: publish ------>|  one transaction
   |  finish()         |                                  |  per batch
   |------------------>| last wave; seal every lane;      |
   |                   | flush; register_ready (+ release |
   |                   | unused ordinals) --------------->|
   |<------------------| WriteOutcome                     |
```

## Batches and waves

| Bound | Value | Source |
| --- | --- | --- |
| Objects per pending batch | 512 (`BATCH_OBJECT_LIMIT`) | [`store/policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs) |
| Canonical bytes per batch (and per wave) | The provider's `publication_bytes`: 4 MiB − 1 (`WAVE_CANONICAL_BYTES_LIMIT`) by default and on SQLite, 16 MiB − 1 (`PUBLICATION_BYTES_MAX`) on PostgreSQL; `StorageCapacities::with_publication_bytes` refuses anything outside that range | same |
| Object larger than the byte bound | accepted alone into an empty batch | [`batch.rs`](../../../../core/crates/layerfs-storage/src/save/batch.rs) |

One wave ([`wave.rs`](../../../../core/crates/layerfs-storage/src/save/wave.rs)):

| Step | What happens |
| --- | --- |
| Membership | Collect every object id, direct reference and predecessor in the wave; locate them in one paged call. |
| Duplicate in wave | Same bytes and role: counted as reused. Different: `Collision`. |
| Exact reuse | Already stored with the same role and length: counted as reused and not read back. A role or length mismatch is `Collision`. A corrupt stored copy fails at its first read. |
| New object | Check every reference is stored, pending or sealed (else `MissingDependency`), select a representation ([Encoding](encoding.md#full-and-delta-encoding)) and add the record to its lane's open group. |
| Flush | Write signature-ring changes, then publish every ready pack. |

A whole-file object whose ring proposal is still held by this Save (pending
or sealed, not published) would first seal and publish it, so selection can
charge the winner's chain. When no PREFIX record can be narrower than the
object's FULL record (`delta::record::prefix_cannot_win`: the PREFIX adds the
32-byte base id and a zstd frame of at least 14 bytes), the proposal is dropped
instead: selection would answer FULL whatever the base. The FULL record is
prepared once and handed to selection. Before this, 1,000 similar 23-byte
files cost 1,000 publications and 1,000 one-object packs; now one wave
(layerfs-project #67).

A Storage handle runs one Save at a time: a second `begin_save` while one is
open fails with `Integrity("one save per storage handle")`. Separate handles
may save concurrently ([`store/handle.rs`](../../../../core/crates/layerfs-storage/src/store/handle.rs),
[`state.rs`](../../../../core/crates/layerfs-storage/src/save/state.rs)).

## Reservations

```text
 begin_save --> reserve(packs = 4096, ordinals = 16384)   one combined call
 placing a pack, remaining ids < needed
            --> reserve(packs = max(needed, 4096), ordinals = 0)
 pooled leaf, remaining ordinals < fresh values
            --> reserve(packs = 0, ordinals = max(fresh, 16384))
 finish     --> Publication.release_ordinals = unused ordinal tail
```

| Setting | Default | Allowed | Source |
| --- | --- | --- | --- |
| `ReservationBlocks.packs` | 4,096 | 1 to 8,191 (`TRANSACTION_ROW_LIMIT`) | [`store/reservations.rs`](../../../../core/crates/layerfs-storage/src/store/reservations.rs) |
| `ReservationBlocks.ordinals` | 16,384 | 1 to 131,072 (`METADATA_INDEX_VALUES`) | same |

Blocks size allocation windows only; they never limit what one Save accepts.
`Storage::with_reservations` sets them per handle. A reply with a non-positive
first id is `Integrity("initial Save reservation")`
([`reservation.rs`](../../../../core/crates/layerfs-storage/src/save/reservation.rs)).
A refill replaces the current window. Only the tail of the last ordinal window
is released, at finish.

## Pooled saves

```text
 inode leaf rows: (serial, value[73]) ...
   value held by a named base leaf -----> reuse that ordinal
   value already assigned in this Save -> reuse (memo, by fingerprint,
                                          then full bytes compared)
   otherwise -------------------------> next ordinal from the window
 body = prefix || (serial, ordinal) ... --> FULL or COPY/INSERT vs base
```

| Rule | Source |
| --- | --- |
| New values are kept by ordinal and published before any leaf that names them, in the same batch or an earlier one. | [`pooled.rs`](../../../../core/crates/layerfs-storage/src/save/pooled.rs), [`publication.rs`](../../../../core/crates/layerfs-storage/src/save/publication.rs) |
| The memo holds at most 131,072 fingerprints and is cleared whole at that bound; a miss costs one extra stored row. | [`pooled.rs`](../../../../core/crates/layerfs-storage/src/save/pooled.rs) |
| Nothing is rebuilt from history: only the named base leaves and the memo are consulted. | same |

## Publication

`register_ready` turns sealed packs into one or more `Publication` batches
([`publication.rs`](../../../../core/crates/layerfs-storage/src/save/publication.rs)):

| Step | Rule |
| --- | --- |
| Closure | Any ready member that references an object still in an open group forces that lane to seal; repeat until nothing changes. |
| External references | Locate references outside the ready set; one that is not stored is `MissingDependency`. |
| Order | Topological: an object follows everything it references. A cycle is `Integrity("registration reference cycle")`. |
| Batch contents | Inode values first, then packs with their locators, then signature changes; at finish the unused ordinal tail. |
| Split | Start a new batch before one would pass 8,191 rows, `publication_bytes` canonical bytes or `publication_bytes` sealed pack bytes (4 MiB − 1 on SQLite, 16 MiB − 1 on PostgreSQL). An item larger than a bound goes alone. |
| Publish | One `publish` call per batch: one attempt, one atomic unit in the backend. |
| First writer wins | `Published.lost` lists locators already present. Each lost id is read back from the winner and compared with this Save's bytes and role; a difference is `Collision`. |

Batches already acknowledged stay published if the Save fails later. After a
failed `accept`, the Save is terminal and later calls return `Aborted`.
Dropping an unfinished Save invalidates the signature ring so it reloads from
the Store ([`operation.rs`](../../../../core/crates/layerfs-storage/src/save/operation.rs)).

| `WriteOutcome` field | Meaning |
| --- | --- |
| `inserted`, `reused` | Locators this Save added; objects already stored or lost to another writer |
| `full_records`, `prefix_records` | Representation of inserted objects |
| `packs`, `stored_bytes`, `metadata_bytes` | Packs published, their bytes, and the bytes in the metadata domain |
| `canonical_bytes` | Canonical bytes of inserted objects |
| `pool`, `delta` | Pooled and delta selection counters |

`Storage::save_work` returns `SaveHistory`: invocation counts and inclusive
wall time for nine stages (begin, membership, admission, flush, publication,
metadata, pack reserve, ordinal reserve, finish close), in total and for the
last four completed Saves ([`work.rs`](../../../../core/crates/layerfs-storage/src/save/work.rs)).

## Seal

Two different things are called seal.

| Seal | What it does | Where |
| --- | --- | --- |
| Group and pack seal | `Packer::seal` closes a lane's open group into an `EncodedGroup` and queues it; `flush_lane` and `place` turn queued groups into sealed packs with ids and SHA-256 keys. `finish` seals all four lanes. | [`seal.rs`](../../../../core/crates/layerfs-storage/src/save/seal.rs), [`operation.rs`](../../../../core/crates/layerfs-storage/src/save/operation.rs) |
| Store seal | `SqliteHandles::seal` consumes the sole SQLite session, checkpoints once and closes the file for a host or offline handoff. It is not part of a Save; a live daemon keeps its Store open across Saves. | engine: [`layerfs-persistence/src/store/seal.rs`](../../../../engine/crates/layerfs-persistence/src/store/seal.rs); see [Project Init: seal and install](../../engine/project-init/seal-and-install.md) |

`Save::finish` completes the saved objects while the Store stays open. A
retained `Storage`, `Reader` or `Save` makes the Store seal refuse with `Busy`
before any checkpoint.

## Related

- [Encoding](encoding.md) — representation selection run for each new object.
- [Packs](packs.md) — group and pack assembly.
- [Ports](ports.md) — `reserve`, `publish` and `Publication`.
- [Engine: Store](../../engine/store/README.md) — the one writer and the publish lane around these calls.
- [Engine: Commit](../../engine/commit/README.md), [Engine: Project Init](../../engine/project-init/README.md) — the two producers.

---

← [Physical storage: Packs](packs.md) · [Up: Physical storage](README.md) · [Physical storage: Reads](reads.md) →
