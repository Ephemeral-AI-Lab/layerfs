# Packs

[Architecture](../../README.md) › [Core](../README.md) › [Physical storage](README.md) › Packs

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Pack layout, assembly and the pack cache.

| | |
| --- | --- |
| Component | [Physical storage](README.md) |
| Source | [`core/crates/layerfs-storage/src/pack`](../../../../core/crates/layerfs-storage/src/pack), [`core/crates/layerfs-storage/src/encoding/pack_cache.rs`](../../../../core/crates/layerfs-storage/src/encoding/pack_cache.rs) |

## Pack layout

A pack as written today (sized directory, versions 23, 24 and 25). All
integers are little-endian `u32`.

```text
 offset 0       8      12      16      20      24
 +--------------+------+-------+-------+-------+
 | "LFPACK\0\0" | ver  | groups| used  |   0   |   control area (24 B)
 +--------------+------+-------+-------+-------+
 | directory: exactly `groups` entries         |
 |  ordinary, chunk, singleton (16 B each):    |
 |   start | encoded len | decoded len |       |
 |   codec u8 (0 raw, 1 zstd) + 3 zero bytes   |
 |  whole file (4 B each): start               |
 +---------------------------------------------+
 | group 0 body | group 1 body | ...           |   ends exactly at `used`
 +---------------------------------------------+

 group body (raw form):
 +-------+--------+--------+-----+----------+----------+-----+
 | count | end[0] | end[1] | ... | record 0 | record 1 | ... |
 +-------+--------+--------+-----+----------+----------+-----+
   end[i] = byte offset where record i ends inside the record area
```

| Lane | Holds | Domain | Version written | Versions read | Group codec | Source |
| --- | --- | --- | --- | --- | --- | --- |
| `Ordinary` | tree and metadata roles, pooled leaves, inode-table branches | Metadata | 24 | 9, 19, 24 | raw or zstd | [`layout.rs`](../../../../core/crates/layerfs-storage/src/pack/layout.rs) |
| `Chunk` | chunk payload records | Payload | 25 | 10, 15, 20, 25 | raw | same |
| `WholeFile` | compact whole-file records | Payload | 23 | 11, 14, 17, 18, 23 | raw | same |
| `Singleton` | one oversized whole-file record | Payload | 16 | 13, 16 | raw | same |

Every other version is refused with
`UnsupportedPolicy { field: "pack framing version" }`. This includes the
pre-v9 framings 1, 2, 4, 6 and 7 of the schema-8 Store, and 21 and 22 of the
retired pooled value-group lane. Versions 9 to 20 reserve directory slots
(256 or 16), so their bodies start after the reserved region; `parse_header`
derives each version's own body offset.

| Field check on read | Failure | Source |
| --- | --- | --- |
| Magic, known version, reserved word zero | `Integrity("pack magic")`, `UnsupportedPolicy`, `Integrity("pack reserved field")` | `parse_control_header` in [`layout.rs`](../../../../core/crates/layerfs-storage/src/pack/layout.rs) |
| Group count 1 to the version's slots; singleton exactly 1 | `Integrity("pack group count")` | same |
| `used` equals the descriptor's length and the lane's pack limit | `Integrity("pack length")` | same |
| Directory entries continuous; bodies end exactly at `used` | `Integrity("group directory continuity")`, `("pack trailing bytes")` | `ordinary_directory`, `whole_file_directory` |
| Raw codec means encoded = decoded; chunk and singleton are raw only | `Integrity("group codec")`, `("lane group codec")` | same |
| Record ends strictly increase and fill the record area | `Integrity("record end ordering")`, `("record area length")` | `record_range` |

## Assembly

```text
 record --> open Group (per lane) --GROUP_TARGET 48 KiB or 512 KiB canonical-->
   build_group: frame; ordinary lane tries zstd level 1
       |
       v
 Queue (per lane; all lanes share 256 KiB and 512 rows)
       |  flush on pressure, reference closure or finish
       v
 flush_lane: pack groups while append_fits (<= 16 groups, <= pack limit)
       |
       v
 place: take next reserved pack id, assemble_consuming, SHA-256 key
       |
       v
 Sealed { PackInfo, body, member locators }  --> publication
```

| Step | Rule | Source |
| --- | --- | --- |
| Group target | A group seals when its framed length would pass `GROUP_TARGET` (48 KiB) or its canonical bytes `GROUP_CANONICAL_BYTES_LIMIT` (512 KiB). | [`policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs), [`save/select.rs`](../../../../core/crates/layerfs-storage/src/save/select.rs) |
| Group body limit | Ordinary and chunk 65,536 bytes; whole file `PACK_LIMIT` minus control area and reserved directory; singleton its own pack limit. | `frame_group_bounded`, `WHOLE_FILE_GROUP_BODY_LIMIT` in [`assemble.rs`](../../../../core/crates/layerfs-storage/src/pack/assemble.rs) |
| Ordinary compression | Kept only when the frame plus its 16-byte entry fits inside the raw body. | `compress_group_body` in [`codec.rs`](../../../../core/crates/layerfs-storage/src/encoding/codec.rs) |
| Fit | `append_fits` uses a running total and charges the reserved directory width, so the sized pack written is never longer than planned. | [`layout.rs`](../../../../core/crates/layerfs-storage/src/pack/layout.rs) |
| Assemble | One pass: control area, directory sized to the groups, bodies. `assemble_consuming` frees each group as it is copied. | [`assemble.rs`](../../../../core/crates/layerfs-storage/src/pack/assemble.rs) |
| Place | Pack ids come from the Save's reserved block; running out is `Integrity("pack reservation exhausted")`. | `Packer::place` in [`save/seal.rs`](../../../../core/crates/layerfs-storage/src/save/seal.rs) |

A sealed pack is never appended to. The directory therefore holds exactly its
groups; older versions that reserved spare slots remain readable.

## Pack cache

```text
 PackCache (one per Reader, ReadCache or Save)
 +---------------------------------------------------------+
 | key (pack_id, None)   -> whole body                     |
 | key (pack_id, Some g) -> one encoded group (PackUnit)   |
 | retained <= 2 MiB and <= 4,096 entries                  |
 | one body > 2 MiB (singleton, <= 16 MiB + 4 KiB) alone   |
 +---------------------------------------------------------+
   eviction: least recently touched entry first
```

| Property | Value | Source |
| --- | --- | --- |
| Owner | One reader, kept read cache or Save; never shared with another operation | [`pack_cache.rs`](../../../../core/crates/layerfs-storage/src/encoding/pack_cache.rs) |
| Byte bound | `DEPENDENCY_PACK_CACHE_BYTES` = 2 MiB | [`policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs) |
| Count bound | `READ_OBJECT_LIMIT` = 4,096 entries | same |
| Oversized body | Admitted alone after every other entry is evicted | `PackCache::insert` |
| Changed bytes for a cached id | `Integrity("immutable cached pack changed")` | same |
| Acquisition | `acquire_groups` fetches only the groups still missing, as one whole body or selected units | same, and [Reads](reads.md#fetch-and-prefetch) |
| Work counts | `PackCacheWork`: evictions, evicted bytes, peak bytes | same |

Pack ids only grow and published packs never change, so a kept body cannot go
stale. Eviction costs a later read, never correctness.

## Related

- [Encoding](encoding.md) — the records inside group bodies.
- [Saves](saves.md) — when groups seal and packs are placed.
- [Reads](reads.md) — how packs and groups are acquired.
- [Ports](ports.md) — `PackInfo`, `ObjectKey` and the read carriers.
- [Engine: Store, metadata and objects](../../engine/store/metadata-and-objects.md) — where pack rows are stored.

---

← [Physical storage: Encoding](encoding.md) · [Up: Physical storage](README.md) · [Physical storage: Saves](saves.md) →
