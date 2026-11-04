# Identity

[Architecture](../../README.md) › [Core](../README.md) › [Objects](README.md) › Identity

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How an object's ID is derived, and why equal content gives an equal ID.

| | |
| --- | --- |
| Component | [Objects](README.md) |
| Source | [`core/crates/layerfs-content/src/object/id.rs`](../../../../core/crates/layerfs-content/src/object/id.rs), [`core/crates/layerfs-content/src/object/codec.rs`](../../../../core/crates/layerfs-content/src/object/codec.rs), [`core/crates/layerfs-content/src/object/predecessor.rs`](../../../../core/crates/layerfs-content/src/object/predecessor.rs) |

## Object IDs

```text
 canonical object bytes (envelope + value, see "Canonical codec")
 +-----------------------------------------------------------+
 | 4C 46 53 4F 01 00 00 00 25 00 00 00 21 4C 46 53 35 ...     |
 +-----------------------------------------------------------+
                              |
                              v
 BLAKE3( "layerfs/object/v2\0"  ||  canonical bytes )
         ^^^^^^^^^^^^^^^^^^^^^
         OBJECT_DOMAIN, 18 bytes, frozen
                              |
                              v
 ObjectId: 32 bytes        shown as 64 lowercase hex characters
```

| Rule | Detail | Source |
| --- | --- | --- |
| Hash | BLAKE3 (`blake3 =1.8.5`) over `OBJECT_DOMAIN`, then the canonical bytes | [`id.rs`](../../../../core/crates/layerfs-content/src/object/id.rs), [`Cargo.toml`](../../../../core/crates/layerfs-content/Cargo.toml) |
| Domain | `OBJECT_DOMAIN = b"layerfs/object/v2\0"`; an ID is never a raw digest of the payload | [`id.rs`](../../../../core/crates/layerfs-content/src/object/id.rs) |
| Width | `DIGEST_BYTES = 32`; `ObjectId::from_bytes` refuses any other width with `InvalidIdentityLength` | [`id.rs`](../../../../core/crates/layerfs-content/src/object/id.rs) |
| Text | 64 hex characters; `FromStr` accepts either case and refuses other text with `InvalidIdentityText` | [`id.rs`](../../../../core/crates/layerfs-content/src/object/id.rs) |
| Streaming | `ObjectId::stream()` returns an `IdentityStream`; parts added in order give the ID of their concatenation, and the stream keeps no bytes | [`id.rs`](../../../../core/crates/layerfs-content/src/object/id.rs) |
| Ordering | `ObjectId` is `Ord` and `Hash` over its 32 bytes, so it can key sorted rows and maps | [`id.rs`](../../../../core/crates/layerfs-content/src/object/id.rs) |

The ID covers every canonical byte, including the envelope and the role magic
inside the value. Equal canonical bytes always give an equal ID, and one
changed byte gives a different ID (test
`identity_of_a_damaged_canonical_object_changes` in
[`object_identity.rs`](../../../../core/crates/layerfs-content/tests/object_identity.rs)).

Equal *logical* content gives an equal ID only where construction is
deterministic for that content:

| Case | Same ID for the same logical bytes? | Why |
| --- | --- | --- |
| Whole-file object, chunk object, symlink target | Yes | The value is a fixed header plus the bytes |
| Fresh complete-file construction | Yes | Chunk boundaries come from the content ([Files: Chunking](../files/chunking.md)) |
| Chunked file produced by an edit | Not in general | The edit keeps the base's chunks and re-pages them ([Files: Edits](../files/edits.md)) |
| Filesystem root | Only for the same allocation scope and serials | The root records the scope, root serial and inode-table root ([Filesystem trees: Representation](../filesystem-trees/representation.md)) |

Chunk construction hashes while it scans. `ChunkIdentity` in
[`mapping/codec.rs`](../../../../core/crates/layerfs-content/src/file/mapping/codec.rs)
feeds the envelope, the chunk magic and then the payload into an
`IdentityStream`, so a chunk's ID needs no second pass.

Physical storage names packs by a different digest, `ObjectKey`: SHA-256 of
the sealed pack bytes
([`port/pack.rs`](../../../../core/crates/layerfs-storage/src/port/pack.rs)).
An `ObjectId` never depends on compression, delta encoding or pack layout.

## Canonical codec

Every canonical object uses one envelope. The role is not in the envelope; the
value starts with the role's own magic ([Kinds](kinds.md)).

```text
 offset 0        4      5              9              13
        +--------+------+--------------+--------------+-----------------+
        | "LFSO" | kind | payload_len  | value_len    | value           |
        | magic  | 0x01 | u32 BE       | u32 BE       | value_len bytes |
        +--------+------+--------------+--------------+-----------------+
        |<----- HEADER_LEN = 9 ------->|<- VALUE_LEN_BYTES = 4
                                       |<--------- payload_len -------->|

 payload_len = value_len + 4            total = 9 + payload_len

 Frozen fixture (46 bytes, a whole-file object):
   4C46534F 01 00000025 00000021 | 4C465335534D4C00 0001 6C617965...
   "LFSO"   k  37       33       | "LFS5SML\0"      v1   23 payload bytes
```

| Field | Width | Rule | Source |
| --- | --- | --- | --- |
| `OBJECT_MAGIC` | 4 | `LFSO`; anything else is `UnsupportedFraming` | [`codec.rs`](../../../../core/crates/layerfs-content/src/object/codec.rs) |
| kind | 1 | `BYTES_KIND = 1`; anything else is `WrongLogicalRole` | [`codec.rs`](../../../../core/crates/layerfs-content/src/object/codec.rs) |
| `payload_len` | 4 | At most `MAX_PAYLOAD_BYTES` (16 MiB − 9), and exactly `value_len + 4` | [`codec.rs`](../../../../core/crates/layerfs-content/src/object/codec.rs) |
| `value_len` | 4 | At most `MAX_OBJECT_FIELD_BYTES` (8 MiB) | [`codec.rs`](../../../../core/crates/layerfs-content/src/object/codec.rs), [`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |
| value | `value_len` | Role magic, role grammar, body; nothing may follow it | [`codec.rs`](../../../../core/crates/layerfs-content/src/object/codec.rs) |

| Function | What it does |
| --- | --- |
| `canonical_len(value_len)` | Checked total width; refuses a value over 8 MiB or a total over 16 MiB |
| `encode_bytes_object_to(value, writer)` | Writes the header and value once into a caller-sized sink |
| `encode_bytes_object(value)` | The same, into one allocation of exactly `canonical_len` bytes |
| `decode_bytes_object(canonical)` | Checks lengths, magic, kind and trailing bytes; returns the value as a borrowed slice, without a copy |

`decode_bytes_object` checks in a fixed order: total ceiling, minimum length,
magic, kind, payload ceiling, value ceiling, then truncation (`UnexpectedEof`)
and trailing bytes (`TrailingBytes`). The test
`malformed_envelopes_are_rejected_exactly` covers each case.

## Predecessor links

A predecessor is a hint for Physical storage, never a logical dependency. It
names a stored object that the new object probably resembles, so a Save may
try a delta against it.

```text
 FinalizedObject
   id, role, canonical bytes, references
   predecessors: AdvisoryPredecessors   (at most 4, in preference order)
       [0] (ObjectId, OriginalBase)
       [1] (ObjectId, UnchangedPrefix)
       ...
           |
           | into_parts() moves them with the object
           v
 Physical storage (save/select.rs, save/wave.rs) reads predecessors().ids()
 and decides whether one is acquired and worth a delta; Content never does
```

| Provenance | Meaning | Proposed by |
| --- | --- | --- |
| `OriginalBase` | The base object this operation was declared to edit | `AdvisoryPredecessors::explicit`, from the whole-file edit route ([`edit/apply.rs`](../../../../core/crates/layerfs-content/src/file/edit/apply.rs)) |
| `UnchangedPrefix` | The object that held the bytes just before the replaced range | Mapping build ([`mapping/build.rs`](../../../../core/crates/layerfs-content/src/file/mapping/build.rs), [`mapping/repeated.rs`](../../../../core/crates/layerfs-content/src/file/mapping/repeated.rs)) and sorted tree pages ([`sorted/page.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/page.rs)) |
| `ReusedRange` | A stored object covering a reused subtree or extent range | Chunk-run construction ([`construction/chunk_runs.rs`](../../../../core/crates/layerfs-content/src/file/construction/chunk_runs.rs)) |

| Rule | Value or behaviour | Source |
| --- | --- | --- |
| Bound | `MAXIMUM_ADVISORY_PREDECESSORS = 4`; a fifth `push` fails `BoundedCapacityExceeded { what: "object.predecessors" }` | [`predecessor.rs`](../../../../core/crates/layerfs-content/src/object/predecessor.rs) |
| Duplicates | A repeated ID is ignored; the first provenance stays | [`predecessor.rs`](../../../../core/crates/layerfs-content/src/object/predecessor.rs) |
| Identity | Predecessors are outside the canonical bytes, so they never change an ID | [`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs) |
| Consumer | Physical storage chooses a delta base from them | [`save/select.rs`](../../../../core/crates/layerfs-storage/src/save/select.rs) |

## Related

- [Kinds](kinds.md): the value grammars inside the envelope.
- [Access](access.md): `FinalizedObject`, which carries the ID, references and predecessors.
- [Files: Chunking](../files/chunking.md): chunk objects and chunk delta bases.
- [Physical storage: Encoding](../physical-storage/encoding.md): full and delta encoding of these objects.
- [Glossary](../../glossary.md) for LayerFS terms.

---

← [Core: Objects](README.md) · [Up: Objects](README.md) · [Objects: Kinds](kinds.md) →
