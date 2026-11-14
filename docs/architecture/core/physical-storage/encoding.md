# Encoding

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How one canonical object becomes a stored record and back: codec frames,
FULL, STORED and PREFIX selection, chain decoding, pooled inode leaves,
program pages and the pack cache, within [Physical storage](README.md).

| | |
| --- | --- |
| Source | [`encoding/`](../../../../core/crates/layerfs-storage/src/encoding): [`codec.rs`](../../../../core/crates/layerfs-storage/src/encoding/codec.rs), [`full.rs`](../../../../core/crates/layerfs-storage/src/encoding/full.rs), [`decode.rs`](../../../../core/crates/layerfs-storage/src/encoding/decode.rs), [`delta/`](../../../../core/crates/layerfs-storage/src/encoding/delta), [`pool/`](../../../../core/crates/layerfs-storage/src/encoding/pool), [`pack_cache.rs`](../../../../core/crates/layerfs-storage/src/encoding/pack_cache.rs), [`pack_units.rs`](../../../../core/crates/layerfs-storage/src/encoding/pack_units.rs) |

## codec.rs and full.rs

```text
 caller-owned aligned region (zeroed once)
 +----------------------------------------------+
 | static ZSTD_CCtx  (encode, 16 MiB region)    |
 | static ZSTD_DCtx  (decode, 1 MiB region)     |
 +----------------------------------------------+
   no allocator-backed context; a call that does not fit fails

 canonical object --role--> lane
   ordinary roles --> FULL [0][canonical bytes]       (program pages: pool/)
   whole file / chunk --> raw payload
     len > 1 KiB and a 1 KiB probe saves <= 1/32 --> STORED [2][raw]
     compress(raw); frame >= raw              --> STORED [2][raw]
     otherwise                                --> FULL   [0][frame]
     one eligible base B?  --> PREFIX [1][B id][frame vs B], if shorter
```

| Frame | Level | Window log | Decode checks |
| --- | --- | --- | --- |
| Chunk payload | 3 | 20; raw ≤ 32,768, frame ≤ 33,024 | content size = declared raw length, window within profile, no dictionary id, checksum present |
| Whole-file payload | 3 | `whole_file_window_log` (18 or 20); raw ≤ canonical limit − 23 | same |
| PREFIX payload | 3 | as its role | same, against the exact base payload |
| Ordinary group body | 1 | ≤ 16 | decoded length = the directory's declared length |

`codec.rs` is the only module allowed `unsafe`; its header lists the whole
zstd FFI inventory and each block has a SAFETY note. A failed codec call is
returned; no other codec is tried. Finished workspace regions go to a
process-wide idle list, at most one per available CPU per kind.

| Record per lane (LE `u32` lengths) | Layout |
| --- | --- |
| chunk | `tag | raw_len | [base 32] | frame` |
| whole file | `tag | [base 32] | frame` (lengths dropped in the pack) |
| singleton | `tag | raw_len | frame_len | [base 32] | frame` |
| ordinary | `0 | canonical bytes`, or `1 | program` for program pages |

`plan_lane` sends a whole-file record to the singleton lane when its compact
form would not fit a 256 KiB pack.

## delta/

**Selection** (`select` in
[`delta/select.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/select.rs)),
for one whole-file or chunk payload:

| Step | Rule |
| --- | --- |
| 1 Prepare FULL | always encoded first |
| 2 Depth cap 0 | the role's delta depth is 0: FULL |
| 3 Candidate | chunk: the first advisory predecessor only; whole file: the advisory list, else the content-signature ring (`Candidates::find`) |
| 3a Losing base | a chunk base whose last 8 trials in this Save lost is skipped, then retried after 1, 2, 4, … offers (at most 2^16); a win saving less than 1/8 of the FULL record is stored but counts as a loss |
| 4 Eligibility | stored, same role, below the depth cap; else FULL |
| 5 Chain budget | from stored record sizes; a delta candidate that does not fit is replaced by its chain's independent root; still over budget: FULL |
| 6 One trial | the base is read and authenticated, then one PREFIX frame is built; chain plus object must still fit |
| 7 Compare | PREFIX wins only when its record is shorter than FULL |

A whole-file object stored FULL enters the signature ring: 8,192 slots,
`INDEX_BYTES` 704 KiB, rows persisted through `signatures`
([`delta/candidates.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/candidates.rs)).
When no PREFIX can be narrower than FULL (`prefix_cannot_win`: the base id
plus a zstd frame of at least 14 bytes), the ring proposal is dropped and no
seal is forced ([`delta/record.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/record.rs)).

**Decoding** ([`delta/read.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/read.rs)):

```text
 locator root R --base_of--> B1 --base_of--> B2 (FULL)
   walk: read each record's base id, locate it, check role and depth
 decode base first:
   B2: FULL frame       -> canonical(B2); hash == B2 ? else Integrity
   B1: frame vs raw(B2) -> canonical(B1); hash == B1 ?
   R:  frame vs raw(B1) -> canonical(R);  hash == R  ?
```

| Check | Failure |
| --- | --- |
| Pack id not above the read's ceiling, before any cache | `VisibilityCeiling` |
| No id repeated; length within the role's depth | `Integrity("dependency cycle" / "dependency chain depth")` |
| Base has the same role | `Integrity("dependency role")` |
| Canonical and encoded chain work within budget | `Integrity("dependency canonical work" / "dependency encoded work")` |
| Group body and rebuilt object lengths | `Integrity("group body length")`, `Integrity("… record length")` ([`decode.rs`](../../../../core/crates/layerfs-storage/src/encoding/decode.rs)) |
| Rebuilt object hashes to its id | `Integrity("dependency identity")` |

The walk is iterative, so stack depth does not grow with chain length. The
base id inside a PREFIX record is the only stored copy of a dependency edge.
Decoded ordinary group bodies are kept in a `GroupCache` of 512 KiB.

## pool/

```text
 pooled leaf body:  prefix 44 B || (serial u64, ordinal u32) * rows
 record:  0 | body                                 (FULL)
          1 | base id | out_len | count | program  (COPY/INSERT)
 ordinal --> 73-byte inode value, one stored row (PackPersistence::inode_values)

 program page (same ordinary record grammar, tag 0 = canonical page):
   InodeBranch, DirectoryLeaf, DirectoryBranch, ExtentLeaf, ExtentBranch
```

| Part | Rule | Source |
| --- | --- | --- |
| COPY/INSERT | 4,096-bucket table over 16-byte seeds, 128 KiB match budget (an exhausted budget yields no program); program ≤ 64 KiB | [`pool/delta.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/delta.rs) |
| Leaf record | ≤ 8,192 bytes | [`pool/leaf.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/leaf.rs) |
| Program pages | candidate is the first advisory predecessor (the replaced page, or for extent pages the stored page at the same level and position) with the same role; one trial; kept only when shorter, and for a directory page only when it saves at least a quarter | [`pool/branch.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/branch.rs), [`delta/branch.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/branch.rs) |
| Re-base | a candidate chain at its depth cap or budgets restarts at the whole page at its root (depth one) instead of storing the page whole | [`delta/branch.rs`](../../../../core/crates/layerfs-storage/src/encoding/delta/branch.rs) |
| Metadata chain budgets | depth 8 by default; 65,536 canonical and 139,281 encoded bytes | [`store/policy.rs`](../../../../core/crates/layerfs-storage/src/store/policy.rs) |
| Read | rebuild the body from the chain, look up all ordinals in one call, rebuild the leaf, check its id; a demand of several leaves prepares up to 128 leaves (1 MiB of bodies) ahead and looks up their ordinals together | [`pool/read.rs`](../../../../core/crates/layerfs-storage/src/encoding/pool/read.rs) |

The locator's role decides how tags 0 and 1 are read, so a Store written
before program pages reads unchanged. Ordinary packs that may hold a
directory or extent program are written at version 26, which an older reader
refuses by version rather than inside a decode.

## pack_cache.rs and pack_units.rs

```text
 PackCache (one per Reader, ReadCache or Save)
   (pack_id, None)    -> whole body
   (pack_id, Some g)  -> one encoded group (PackUnit)
   retained <= 2 MiB and <= 4,096 entries; least recently touched first
   a body over 2 MiB (singleton) is admitted alone after evicting the rest
```

| Property | Value |
| --- | --- |
| Owner | one reader, kept read cache or Save; never shared |
| Changed bytes for a cached id | `Integrity("immutable cached pack changed")` |
| Acquisition | `acquire_groups` fetches only missing groups, as one body or selected units |
| Work counts | `PackCacheWork`: evictions, evicted bytes, peak bytes |

Pack ids only grow and published packs never change, so a kept body cannot
go stale; eviction costs a later read, never correctness.

| Type ([`pack_units.rs`](../../../../core/crates/layerfs-storage/src/encoding/pack_units.rs)) | What it is |
| --- | --- |
| `PackAcquisition::Whole` | a complete pack body, with its descriptor when it came from the Store |
| `PackAcquisition::Units` | complete encoded groups cut from a ranged read; the directory is checked and each used record is authenticated by its id |
| `PackUnit` | one complete group; a range that splits a group or names the singleton lane is refused |
| `GroupSlice` | a borrow of one validated group from either form, handed to the decoder |

## Related

- [Physical storage](README.md): packs, Saves, reads and ports around these records.
- [Objects](../objects/README.md): roles, canonical forms and advisory predecessors.
- [Files](../files/README.md): where chunk hints and extent-page hints come from.
- Tests: [`delta_base_budget.rs`](../../../../core/crates/layerfs-storage/tests/delta_base_budget.rs),
  [`losing_chunk_bases.rs`](../../../../core/crates/layerfs-storage/tests/losing_chunk_bases.rs),
  [`page_program_deltas.rs`](../../../../core/crates/layerfs-storage/tests/page_program_deltas.rs)
  (`cargo +1.85.1 test --locked -p layerfs-storage --test page_program_deltas`, any platform).
