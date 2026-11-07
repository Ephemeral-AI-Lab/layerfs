# Encoding

[Architecture](../../README.md) › [Core](../README.md) › [Physical storage](README.md) › Encoding

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Codec frames, full and delta encoding, and decoding.

| | |
| --- | --- |
| Component | [Physical storage](README.md) |
| Source | [`core/crates/layerfs-storage/src/encoding`](../../../../core/crates/layerfs-storage/src/encoding) |

## Codec frames

```text
 caller-owned aligned region (Vec<u64>, zeroed once)
 +-----------------------------------------------+
 | static ZSTD_CCtx  (encode, 16 MiB region)     |  ZSTD_initStaticCCtx
 +-----------------------------------------------+
 | static ZSTD_DCtx  (decode, 1 MiB region)      |  ZSTD_initStaticDCtx
 +-----------------------------------------------+
   no allocator-backed context; a call that does not fit fails
```

| Frame kind | Level | Window log | Checks on decode | Source |
| --- | --- | --- | --- | --- |
| Chunk payload | 3 | 20; raw ≤ 32,768, frame ≤ 33,024 | content size = declared raw length, window within profile, no dictionary id, checksum present | [`codec.rs`](../../../../core/crates/layerfs-storage/src/encoding/codec.rs) |
| Whole-file payload | 3 | from `StorageCapacities.whole_file_window_log`; raw limit = whole-file canonical limit − 23 | same | same |
| PREFIX payload | 3 | as its role | same, decoded against the exact base payload | same |
| Ordinary group body | 1 | ≤ 16 | decoded length = the directory's declared length | same |

`codec.rs` is the only module where `unsafe` is allowed. Its header lists the
whole zstd FFI inventory, and each block carries its own SAFETY note. A prefix
is borrowed for one call and cleared on success and on failure. A failed codec
call is returned; another codec is never tried.

Workspace regions are reused across Saves and reads: finished regions go to a
process-wide idle list, at most one per available CPU per kind
([`save/state.rs`](../../../../core/crates/layerfs-storage/src/save/state.rs),
[`read/objects.rs`](../../../../core/crates/layerfs-storage/src/read/objects.rs)).

## Full and delta encoding

```text
 canonical object --role--> lane
   |
   +-- ordinary roles ------> FULL: [0][canonical bytes]          (no codec)
   |    (InodeBranch may become a COPY/INSERT program, see Pools)
   |
   +-- whole file / chunk --> raw payload
         |
         +- len > 1 KiB, probe saves <= 1/32 --> STORED [2][raw]
         +- compress(raw); frame >= raw ----------> STORED [2][raw]
         +- otherwise ----------------------------> FULL   [0][frame]
         |
         +- one eligible base B? --> PREFIX [1][B id][frame vs B]
                                     kept only if shorter than FULL
```

| Record tag | Name | Base | Body | Source |
| --- | --- | --- | --- | --- |
| 0 | `FULL_TAG` | none | Zstandard frame of the raw payload (ordinary lane: canonical bytes) | [`delta/record.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/record.rs) |
| 1 | `PREFIX_TAG` | one 32-byte `ObjectId` | frame compressed with the base's raw payload as prefix | same |
| 2 | `STORED_TAG` | never | the raw payload itself | same |

Record grammar per lane (lengths are little-endian `u32`):

```text
chunk       : tag | raw_len | [base 32] | frame
whole file  : tag | [base 32] | frame        (lengths dropped in the pack;
                                              raw = canonical length - 23)
singleton   : tag | raw_len | frame_len | [base 32] | frame
ordinary    : 0   | canonical bytes
```

Selection for one payload object (`select` in
[`delta/select.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/select.rs)):

| Step | Rule |
| --- | --- |
| 1. Prepare FULL | Always encoded first (`encode_full` in [`full.rs`](../../../../core/crates/layerfs-storage/src/encoding/full.rs)). |
| 2. Depth cap 0 | The role's delta depth is 0: store FULL. |
| 3. Candidate | Chunk: the caller's first advisory predecessor only. Whole file: the advisory list first; if none is acquired, the content-signature ring (`Candidates::find`). |
| 3a. Losing base | Chunk: a base whose last 8 trials in this Save (`LOSING_BASE_TRIALS`) all lost is skipped, FULL with no trial (`losing_skipped`), then tried again after 1, 2, 4, … offers (at most 2^16); a win forgets it (`LosingBases`). |
| 4. Eligibility | Candidate must be stored, same role and below the role's depth cap; otherwise FULL by policy (`absent_candidates`, `ineligible_candidates`, `depth_capped`). |
| 5. Base chain budget | Chain cost from stored record sizes; over budget means FULL (`work_exceeded`). |
| 6. One trial | The base is read, authenticated, then one PREFIX frame is built. The chain plus the new object must still fit the budgets. |
| 7. Compare | PREFIX wins only when its record is shorter than the FULL record (`full_losses` otherwise). |

A whole-file object stored FULL enters the signature ring: 8,192 slots,
`INDEX_BYTES` = 704 KiB, rows persisted in the Store's `content_signatures`
table ([`delta/candidates.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/candidates.rs)).
A whole-file record goes to the singleton lane when a pack of its compact form
would exceed `PACK_LIMIT` (`plan_lane` in [`full.rs`](../../../../core/crates/layerfs-storage/src/encoding/full.rs)).

## Decoding

```text
 locator root R ---base_of---> B1 ---base_of---> B2 (FULL)
   walk: read each record's base id, locate it, check role and depth

 decode, base first:
   B2: FULL frame       -> canonical(B2), hash == B2 ? else Integrity
   B1: frame vs raw(B2) -> canonical(B1), hash == B1 ?
   R : frame vs raw(B1) -> canonical(R),  hash == R  ?
```

| Check | Failure | Source |
| --- | --- | --- |
| Visibility: pack id not above the read's ceiling, checked before any cache | `VisibilityCeiling` | [`delta/read.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/read.rs) |
| No id repeated in the chain | `Integrity("dependency cycle")` | same |
| Chain length within the role's depth | `Integrity("dependency chain depth")` | same |
| Base has the same role | `Integrity("dependency role")` | same |
| Canonical and encoded chain work within budget | `Integrity("dependency canonical work")`, `("dependency encoded work")` | same |
| Compressed group body equals its declared decoded length | `Integrity("group body length")` | [`decode.rs`](../../../../core/crates/layerfs-storage/src/encoding/decode.rs) |
| Rebuilt object has the locator's canonical length | `Integrity("… record length")` | same |
| Rebuilt object hashes to its id | `Integrity("dependency identity")` | [`delta/read.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/read.rs) |

The walk is iterative, so stack depth does not grow with chain length. The
base id inside a PREFIX record is the only stored copy of a dependency edge;
`stored_base` is its one reader. Decoded ordinary group bodies are kept in a
`GroupCache` of at most 512 KiB (`DECODED_GROUP_CACHE_BYTES`), which releases
least-recently-used bodies.

## Pools

Inode leaves and inode-table branches have their own metadata grammar
([`pool/`](../../../../core/crates/layerfs-storage/src/encoding/pool)):

```text
 pooled leaf physical body:  prefix || (serial, ordinal) * rows
 record:  0 | body                                  (POOLED_FULL_TAG)
          1 | base id | out_len | count | program   (POOLED_DELTA_TAG)
 ordinal ---> inode value (73 bytes), stored by the Store (in blocks, #91)
```

| Part | Rule | Source |
| --- | --- | --- |
| Values | Each 73-byte inode value is one stored row named by an ordinal. A value keeps the ordinal a base leaf already gave it; a new value takes one from the Save's reserved block. | [`save/pooled.rs`](../../../../core/crates/layerfs-storage/src/save/pooled.rs) |
| COPY/INSERT | 4,096-bucket seed table over 16-byte seeds, 128 KiB match budget; an exhausted budget yields no program. Program ≤ 64 KiB. | [`pool/delta.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/delta.rs) |
| Leaf record | ≤ 8,192 bytes (`METADATA_RECORD_LIMIT`) | [`pool/leaf.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/leaf.rs) |
| Inode-table branch | Tag 0 is the canonical page; tag 1 is a program against the predecessor branch. Kept only when shorter. | [`pool/branch.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/branch.rs), [`delta/branch.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/branch.rs) |
| Chain budgets | depth 8; 65,536 canonical and 139,281 encoded bytes | [`store/policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs) |
| Read | Rebuild the body from the chain, look up all ordinals in one call, rebuild the leaf, check its id. A demand of several leaves rebuilds the bodies of the next ones ahead (`prepare_leaves`: at most 128 leaves, 1 MiB of bodies and the 7,182 values the reader retains) and looks up all their ordinals in one call, so a demand costs one value lookup per window instead of one per leaf (layerfs-project #95). Each leaf is still rebuilt and checked by its own id. | [`pool/read.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/read.rs), [`read/objects.rs`](../../../../core/crates/layerfs-storage/src/read/objects.rs) |

The locator's role decides how tag 0 and 1 are read, so a Store written before
branch deltas reads unchanged.

## Pack units

| Type | What it is | Source |
| --- | --- | --- |
| `PackAcquisition::Whole` | A complete pack body, with its descriptor when it came from the Store. | [`pack_units.rs`](../../../../core/crates/layerfs-storage/src/encoding/pack_units.rs) |
| `PackAcquisition::Units` | Complete encoded groups cut from a selected-range read. The directory is checked; each used record is still authenticated by its object id. | same |
| `PackUnit` | One complete group: descriptor, group number, header, extent and bytes. A range that splits a group or names the singleton lane is refused. | same |
| `GroupSlice` | A borrow of one validated group from either cache form, handed to the decoder. | same |

## Related

- [Packs](packs.md) — the byte layout these records sit in, and the pack cache.
- [Reads](reads.md) — how a demand reaches the resolver.
- [Saves](saves.md) — where selection runs.
- [Objects: Kinds](../objects/kinds.md) — roles and canonical forms.
- [Files: Chunking](../files/chunking.md) — where chunk payloads come from.

---

← [Core: Physical storage](README.md) · [Up: Physical storage](README.md) · [Physical storage: Packs](packs.md) →
