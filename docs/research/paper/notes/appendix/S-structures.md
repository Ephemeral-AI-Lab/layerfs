# S. Committed-state structures and physical layout (appendix notes)

Read-only notes, 2026-11-14. Nothing was built, run, edited or checked out.

- **Code identity:** `origin/layerfs_v2_0_0` @ `8cfb6e273470d0bb2a4ec5a9256fcf4972441eb5`.
  `v2:<path>` means `git -C ~/Ephemeral-AI-Lab/layerfs show origin/layerfs_v2_0_0:<path>`.
- Abbreviations: `CT` = `core/crates/layerfs-content/src`, `ST` = `core/crates/layerfs-storage/src`,
  `PE` = `engine/crates/layerfs-persistence`, `HI` = `core/crates/layerfs-history/src`.
- Builds on `docs/research/paper/notes/C-core-storage.md` §1, §2, §5, §7 (not repeated
  where already exact). Everything below was re-read from source at the pinned ref.
- Labels: **[src]** read directly in source or SQL; **[doc]** from the 2.0 architecture
  page; **[arith]** arithmetic from source constants; **[inference]** my reading, not a
  stated contract.
- Byte order: all canonical (identity-bearing) integers are **big-endian**; all
  physical (pack, record, program, block) integers are **little-endian** or LEB128.
  [src: `CT/object/codec.rs`, `CT/file/mapping/codec.rs`, `ST/pack/assemble.rs`,
  `ST/encoding/pool/delta.rs` `read_u32` = `from_le_bytes`, `PE/src/blocks/frame.rs`]

---

## 1. Object envelope, ObjectId, the 13 roles

### 1.1 Envelope [src `CT/object/codec.rs`]

| Offset | Width | Field | Value / rule |
| ---: | ---: | --- | --- |
| 0 | 4 | magic | `"LFSO"` (`OBJECT_MAGIC`) |
| 4 | 1 | kind | `0x01` (`BYTES_KIND`); anything else `WrongLogicalRole` |
| 5 | 4 | `payload_len` u32 BE | = `value_len + 4` (`VALUE_LEN_BYTES`), ≤ 16 MiB − 9 |
| 9 | 4 | `value_len` u32 BE | ≤ `MAX_OBJECT_FIELD_BYTES` 8 MiB |
| 13 | n | value | role magic (8 B) + role grammar |

- `HEADER_LEN = 9`; envelope overhead is 13 B. Canonical object ≤
  `MAX_CANONICAL_OBJECT_BYTES` = 16 MiB [src `CT/contract/policy.rs` L203, L209].
- Decode order: total ceiling, length ≥ 13, magic, kind, payload ceiling, value ceiling,
  truncation, trailing bytes (`decode_bytes_object`). Refused, never repaired.
- **ObjectId** = `BLAKE3("layerfs/object/v2\0" ‖ canonical bytes)`, 32 B
  [src `CT/object/id.rs`: `OBJECT_DOMAIN`, `DIGEST_BYTES = 32`]. Streaming form
  `ObjectId::stream()`; a chunk's ID is computed from its frozen 21-byte framing then the
  payload, without building the object (`ChunkIdentity`, `CT/file/mapping/codec.rs`).
- The ID covers envelope + role magic, so equal payload bytes under two roles have
  different IDs; physical form (FULL/STORED/PREFIX/program/pack) is outside the ID
  [doc `docs/architecture/core/objects/README.md` Invariants].

### 1.2 Role codes and value grammars

Role code (persisted in `object_location.role`, 1–13) is distinct from the role byte inside
the value [src `CT/object/output.rs` `from_code`; doc objects/README "role code and role
byte ... are separate numbers"].

| Code | Role | Value grammar (after the 13 B envelope) | Canonical size | Refs | Source |
| ---: | --- | --- | --- | --- | --- |
| 1 | WholeFile | `"LFS5SML\0"` · ver u16 = 1 · raw payload | raw + 23; raw ∈ [1, cutoff−1] | — | `CT/file/construction/bytes.rs` (`WHOLE_VALUE_HEADER = 10`) |
| 2 | Chunk | `"LFS4CHK\0"` · raw payload (no version, no length) | raw + 21 ≤ 32,789 | — | `CT/file/mapping/codec.rs` `encode_chunk_object` |
| 3 | ExtentLeaf | node header(31, magic `"LFS4MAP\0"`, ver 3, role `0x08`) · n × 40 B entry | 44 + 40n ≤ 5,164 [arith] | chunk IDs | `CT/file/mapping/codec.rs` |
| 4 | ExtentBranch | header (role `0x09`) · n × 48 B entry | 44 + 48n ≤ 6,188 [arith] | children | same |
| 5 | FileState | `"LFS4MAP\0"` · ver 3 · `0x0a` · flags 0 · len u64 · extent count u64 · level u8 · profile ID 32 · mapping root 32 = 93 B | 106 (`FILE_STATE_CANONICAL_BYTES`) | mapping root | same, `encode_file_state` |
| 6 | InodeLeaf | header (`"LFS6INT\0"`, ver 1, role 7) · n × (serial u64 · 73 B value) | 44 + 81n ≤ 8,144 [arith] | each content root + attribute root | `CT/object/inode_leaf.rs`, `CT/filesystem/sorted/format.rs` |
| 7 | DirectoryLeaf | header (`"LFS6NSP\0"`, ver 1, role 1) · n × (name_len u16 · name · serial u64) | ≤ 8,192; non-root ≥ 3,277 | none | `CT/filesystem/sorted/format.rs` |
| 8 | DirectoryBranch | header (role 2) · n × (name_len u16 · max name of child · child ID 32), n ≥ 2 | same | children | same |
| 9 | InodeBranch | header (role 8) · n × (max serial of child u64 · child ID 32) = 40 B rows | 44 + 40n ≤ 5,124 [arith] | children | same |
| 10 | FilesystemRoot | `"LFS6FSR\0"` · ver u16 1 · role 6 · flags 0 · profile ID 32 · scope ID 32 · root serial u64 · inode-table root 32 = 116 B | 129 [arith] | inode table | `CT/filesystem/root.rs` (`ROOT_VALUE_BYTES = 116`) |
| 11 | AttributeLeaf | header (`"LFS4MET\0"`, ver 1, role 9) · n × (dom_len u16 · domain · key_len u16 · key · required u8 = 1 · value root 32) | ≤ 8,192; non-root ≥ 3,277 | value roots | `CT/filesystem/attributes/codec.rs` (`LEAF_ROW_OVERHEAD = 37`) |
| 12 | AttributeBranch | header (role 10) · n × (dom_len u16 · domain · key_len u16 · key · child 32), n ≥ 2 | same | children | same (`BRANCH_ROW_OVERHEAD = 36`) |
| 13 | Symlink | `"LFS4LNK\0"` · ver u16 1 · role 5 · flags 0 · len u16 · target (≤ 4,096 B, no NUL) | 27 + len [arith] | none | `CT/filesystem/symlink.rs` |

**Shared 31-byte node header** (all tree pages, roles 3, 4, 6–9, 11, 12)
[src `CT/filesystem/sorted/format.rs` `node_header`; `CT/file/mapping/codec.rs` `encode_node`]:

| Offset in value | Width | Field |
| ---: | ---: | --- |
| 0 | 8 | magic (`LFS4MAP\0`, `LFS6INT\0`, `LFS6NSP\0`, `LFS4MET\0`) |
| 8 | 2 | grammar version u16 BE (3 for extent pages, 1 otherwise) |
| 10 | 1 | role byte |
| 11 | 1 | level (0 = leaf, ≤ `MAX_LEVEL` 31) |
| 12 | 1 | flags, must be 0 |
| 13 | 2 | entry count u16 BE |
| 15 | 8 | subtree entry count u64 BE (extent pages: subtree **logical bytes**) |
| 23 | 8 | subtree byte total u64 BE (extent pages: subtree **extent count**; inode leaf: n × 81) |

- Envelope 13 + header 31 = 44 B = `EMPTY_PAGE_BYTES` = `POOLED_PREFIX_BYTES`
  [src `CT/filesystem/limits.rs` L79, `CT/object/inode_leaf.rs`].
- Page sizes are exact arithmetic `44 + Σ row widths`; no trial encoding
  [src `CT/filesystem/sorted/format.rs` module doc].
- Note: for extent pages, header field order is logical bytes then extent count
  [src `encode_node`: `logical_len()` then `extent_count()`]; for namespace pages the
  fields are subtree entry count then subtree bytes.

### 1.3 Frozen profile identities

- Mapping profile = `BLAKE3("layerfs/mapping-profile/bplus-extent/v3\0" ‖ ver ‖ tags ‖
  MIN 64 ‖ MAX 128 ‖ root-leaf-min 0 ‖ root-min 2 ‖ MAX_LEVEL 31 ‖ 32,768 ‖ … ‖
  cdc::profile_id())`; embedded in every FileState; a mismatch is
  `InvalidRecord("file state profile")` [src `CT/file/mapping/codec.rs` `profile_id`].
- Namespace profile = `ObjectId::for_bytes("layerfs/namespace-profile/scoped-inline/v1\0
  scope32;serial8;inode81;leaf50-100;branch64-127;page8192;depth31;directory-fill2/5")`;
  only this profile is accepted [src `CT/filesystem/root.rs` `PROFILE_DESCRIPTION`].
- Scope = `ObjectId::for_bytes("layerfs/inode-scope/v1\0" ‖ seed32)`
  [src `CT/filesystem/root.rs` `scope_for_seed`]; the seed is supplied by the Init
  request (`scope_seed`, `engine/crates/layerfs-project/src/import/init.rs` L43, L107).

---

## 2. Filesystem tree

### 2.1 Shape

```
FilesystemRoot (LFS6FSR, 129 B)
  profile | scope | root serial | inode-table root
                                       |
                    InodeBranch (LFS6INT role 8): rows (max serial of child, child ID)
                    64..127 children; ≤ 31 levels
                                       |
                    InodeLeaf  (LFS6INT role 7): rows (serial u64, InodeValue 73 B)
                    50..100 rows (root page exempt), serials strictly increasing
                                       |  content_root / metadata (by kind)
          +----------------------------+-----------------------------+
          v                            v                             v
   Directory: B+tree             RegularFile root:              Symlink (LFS4LNK)
   name -> serial                WholeFile (< cutoff)  or
   (LFS6NSP roles 1/2)           FileState -> extent B+tree -> Chunks
```

[src `CT/filesystem/root.rs`, `CT/filesystem/sorted/format.rs`, `CT/filesystem/limits.rs`;
doc `docs/architecture/core/filesystem-trees/README.md` Picture]

- **Fanout and fill** [src `CT/filesystem/limits.rs`]:
  - page ≤ 8,192 canonical B (`MAXIMUM_PAGE_BYTES`), level ≤ 31 (`MAXIMUM_TREE_LEVEL`
    = `mapping::MAX_LEVEL`);
  - inode leaf 50–100 rows (`MINIMUM/MAXIMUM_INODE_LEAF_ROWS`);
  - inode branch 64–127 children (`MINIMUM/MAXIMUM_INODE_BRANCH_CHILDREN`);
  - directory and attribute pages: non-root ≥ 3,277 B (`MINIMUM_FILLED_PAGE_BYTES`,
    2/5 of 8,192, `filled_page`); directory leaf ≤ 743 rows
    (`(8192−44)/(2+1+8)`, `MAXIMUM_DIRECTORY_LEAF_ROWS`) [arith];
  - root pages exempt from fill; an empty directory is one empty leaf; an empty inode page
    is never valid [doc filesystem-trees/README "Pages and the root"].
- **Branch keys are the child's maximum key** (inode: max serial; directory: max name)
  [src `CT/filesystem/sorted/page.rs` L403 `max: page.entries.last()`, L561, L723].
- **Split point:** `nearest_half` picks the prefix whose doubled width is nearest the
  total, ties to the smaller left page [src `CT/filesystem/sorted/format.rs` ~L585].
- **Ordering:** names compare as raw bytes; serials as u64; duplicates/unsorted rows
  refused (`reject_unordered`, `NonCanonicalOrdering`) [src `sorted/format.rs`].
- **Copy-on-write merge:** a merge reads only children its changes reach (plus join
  neighbours / split siblings, ≤ 256 per batch); every unread child keeps its ID
  [doc filesystem-trees/README; src `CT/filesystem/sorted/page.rs`, `sorted/merge.rs`].

### 2.2 The 73-byte inode value [src `CT/object/inode_leaf.rs` `encode_inode_value`]

| Bytes | Root form (byte 0 bit 7 clear) | Inline form (bit 7 set) |
| --- | --- | --- |
| 0 | kind: 1 file, 2 dir, 3 symlink | kind \| `0x80` |
| 1..9 | `namespace_ref_count` u64 BE | same |
| 9..41 | `content_root` ObjectId | same |
| 41..73 | attribute-tree root ObjectId | 41..45 mode u32 BE; 45..53 mtime s i64 BE; 53..57 mtime ns u32 BE; 57..73 zero |

- Validation: root inode = directory with count 0; others count ≥ 1; directories exactly
  1; only files and symlinks may exceed 1 (hard links) [src `InodeValue::validate`].
- Unknown kind, nonzero inline padding or invalid portable metadata is refused, so each
  value has one encoding per form; forms may mix in a leaf [src module doc].
- Inline form since owner decision 2026-10-18 (#360 item 2) [src module doc].
- Kept metadata: `portable/mode` (4 B) and `portable/mtime` (i64 s, u32 ns < 10^9);
  owner, group, atime, ctime, devices are not kept; xattrs answer `ENOSYS`
  [doc filesystem-trees/README "Attributes"].
- Leaf `subtree_bytes` header field = n × 81 (row width, not value width); an earlier
  revision wrote n × 73 and produced different IDs [src `inode_leaf.rs` module doc].

### 2.3 Directories, symlinks, hard links

- Directory leaf row: `name_len u16 BE | name (1–255 B UTF-8, no NUL, '/', '\\', not
  '.'/'..') | serial u64 BE (≠ 0)`; branch row: `name_len | max name | child ID 32`
  [src `sorted/format.rs` `CompactDirectory::decode`, `width`; limits `path.rs`].
- A name binds a **serial**, never an object; the serial's inode row says what it is.
  Two names → one serial = hard link (count 2) [doc filesystem-trees/README].
- Symlink target: opaque bytes ≤ 4,096, no NUL; never resolved or normalized
  [src `CT/filesystem/symlink.rs`; `limits.rs` `MAXIMUM_SYMLINK_TARGET_BYTES`].
- Attribute values are "extent-only value roots" (≤ 32,768 B, `MAXIMUM_ATTRIBUTE_VALUE_BYTES`
  = `MAXIMUM_CHUNK_BYTES`) [src `attributes/codec.rs`, `limits.rs` L58]. No production
  caller writes a non-portable key [doc].

### 2.4 Regular file representation [src `CT/contract/policy.rs`; notes C §2]

- length 0 → FileState over an empty extent leaf (root leaf may have 0 entries,
  `MINIMUM_ROOT_LEAF_ENTRIES = 0`) [src `CT/file/mapping/types.rs`];
- 1 … cutoff−1 → one WholeFile (default cutoff 131,072, power of two up to 1 MiB,
  recorded in `store_policy.small_file_threshold_bytes`);
- ≥ cutoff → FileState → extent B+tree → CDC Chunks (8/16/32 KiB min/avg/max).

### 2.5 ASCII figure: one file and one directory (illustrative)

```
FilesystemRoot ─► InodeLeaf (single-page inode table, root page)
                    serial 1: Dir  cnt 0 content=D1  inline(mode 0755, mtime)
                    serial 2: File cnt 1 content=FS  inline(mode 0644, mtime)
                    serial 3: File cnt 1 content=W   inline(...)
                        │                │                 │
          D1 DirectoryLeaf (root)        │                 └─► W WholeFile "LFS5SML\0"|v1|bytes
            "big.bin" -> 2               │                     (3 KiB file: 3,095 B canonical)
            "small.txt" -> 3             ▼
                              FS FileState (106 B): len=4,194,304, extents=256,
                                 level=1, profile=P, root=B
                                         │
                              B ExtentBranch level 1 (root, 2 children):
                                 (2,097,152 | 128 | L0)  (4,194,304 | 256 | L1)
                                 │                        │
                              L0 ExtentLeaf: 128 x (chunk id, src_off, len)   L1: 128 entries
                                 (C1, 0, 16,384) (C2, 0, 16,384) ...
                                 │
                              Chunk C1 "LFS4CHK\0"|16,384 B  (shared by every file/version
                                                              that contains these bytes)
```

[illustrative: uniform 16 KiB chunks assumed so cumulative ends are round; real CDC chunks
vary 8-32 KiB (EOF chunk may be shorter). Both leaves hold 128 entries, satisfying the
64-128 non-root rule; root branch has 2 children (minimum). Leaf canonical size
44 + 128x40 = 5,164 B; branch 44 + 2x48 = 140 B [arith].]

---

## 3. Extent tree details [src `CT/file/mapping/types.rs`, `CT/file/mapping/codec.rs`]

- Leaf entry (40 B): `chunk ObjectId 32 | source_offset u32 BE | logical_length u32 BE`.
  `ExtentSlice::new` requires `logical_length > 0` and `source_offset + logical_length
  ≤ 32,768` (the chunk grammar maximum), checked at decode.
- Branch entry (48 B): `cumulative_logical_end u64 BE | cumulative_extent_end u64 BE |
  child ObjectId 32` — prefix sums over siblings, so offset → child is a binary search
  within one page [src `ChildDescriptor`; **[inference]** for the search].
- Page entries: `MAX_ENTRIES = MAX_MAPPING_ENTRIES = 128` [src `CT/contract/policy.rs`
  L212], `MIN_ENTRIES = 64` for non-root pages, root branch ≥ 2, root leaf ≥ 0,
  `MAX_LEVEL = 31`, `MAX_NODE_OBJECT_BYTES = 8,192`.
- **Slices.** Extents are slices `(chunk, offset, length)` of stored chunk payloads, so a
  localized edit can keep the untouched part of a boundary chunk without rewriting it
  [src types.rs module doc "maps logical file offsets onto slices of canonical chunk
  payloads"; doc `files/edits.md`]. Fresh construction always produces whole-chunk slices
  (offset 0, full length) **[inference from construction path; edits produce partial
  slices]**.
- **Zero runs.** There is no zero-extent or repeat-count field in the format. A hole of
  length L is scanned as head ≤ 32 KiB, `repeats = (L − head) / 32,768` identical all-zero
  32 KiB chunks, then a tail [src `CT/file/cdc/zeros.rs`]. The builder
  `push_repeated_chunk(raw, count)` emits ordinary leaf entries naming one zero chunk
  repeatedly and reuses complete identical mapping pages, "work depends on boundary pages
  and tree height, not the repeat count" [src `CT/file/mapping/repeated.rs`]. Storage
  dedups the identical pages and chunk by ID, so a sparse region costs O(height) new
  pages [arith/inference consistent with source comment].
- **Profile hash in FileState** (§1.3): pins the CDC parameters and page partition rules;
  a different profile fails decode, so the partition is frozen per format.
- FileState also records `extent_count` and `tree_level` so a reader knows the root's
  level and the total entries without reading pages [src `FileState`].
- `Reader::file_lengths`: whole files from locator `canonical_length − 23`, chunked files
  from the FileState only [doc physical-storage/README "File lengths"].

---

## 4. Physical records

### 4.1 Payload records (WholeFile, Chunk) [src `ST/encoding/delta/record.rs`; doc `encoding.md`]

Tags: `FULL_TAG = 0`, `PREFIX_TAG = 1`, `STORED_TAG = 2`.

| Lane | Record layout (LE u32 lengths) |
| --- | --- |
| Chunk | `tag u8 | raw_len u32 | [base ObjectId 32 if PREFIX] | frame` |
| WholeFile (compact) | `tag u8 | [base 32] | frame` — both length fields dropped (`WHOLE_FILE_COMPACT_DROP = 8`); raw length = `canonical_length − 23` from the locator |
| Singleton | `tag u8 | raw_len u32 | frame_len u32 | [base 32] | frame` |

- FULL: frame = zstd level 3 frame of the raw payload (window log 20 chunk, 18/20 whole
  file, checksum on, no dictionary). STORED: raw bytes. PREFIX: zstd frame compressed with
  the base's raw payload as prefix (`refPrefix`) [notes C §4.1; src `ST/encoding/codec.rs`].
- The record stores the **raw payload**, not the canonical object; the envelope and role
  magic are rebuilt at decode, then hashed [doc encoding.md decoding diagram;
  **[inference]** from "rebuild to canonical form"].
- A STORED record never carries a base [src `record.rs` `STORED_TAG` doc].
- The base ID inside a PREFIX record is the only stored copy of the dependency edge
  [src `record.rs` module doc].

### 4.2 Ordinary records (tree and metadata roles) [doc `encoding.md`; src `ST/encoding/pool/*`]

- `0 | canonical bytes` (uncompressed per record; compression is per group, zstd level 1,
  kept only if smaller) [doc encoding.md "ordinary"].
- **InodeLeaf** records are always pooled: `0 | pooled body` or
  `1 | base ObjectId 32 | out_len u32 LE | count u32 LE | program`
  (header 41 B) [src `ST/encoding/pool/leaf.rs` `parse`: base `[1..33]`, `read_u32(33)`,
  `read_u32(37)`, program `[41..]`]. Pooled body = 44 B canonical prefix (envelope + node
  header, verbatim) + n × (serial u64 BE, ordinal u32) = 44 + 12n
  (`POOLED_ROW_BYTES = 12`) [src `CT/object/inode_leaf.rs`].
- **Program pages** (InodeBranch, DirectoryLeaf, DirectoryBranch, ExtentLeaf,
  ExtentBranch): tag 0 = canonical page; tag 1 = same 41 B header + program, output =
  canonical page [src `ST/encoding/pool/branch.rs` module doc, `is_program_role`]. The
  locator's role decides how tags are read.
- **COPY/INSERT grammar** [src `ST/encoding/pool/delta.rs` `validate`/`apply`]:
  - `COPY  = 0x00 | offset u32 LE | length u32 LE` (bytes from the base body/page);
  - `INSERT = 0x01 | length u32 LE | literal[length]`;
  - every length > 0; sum = `out_len`; `count` ∈ [1, 8,191]; `out_len` ≤ 65,536
    (`GROUP_LIMIT`); program ≤ 64 KiB (`PROGRAM_LIMIT`); cursor must end exactly.
  - Producer: 4,096-bucket × 4-slot table over 16-byte seeds; 128 KiB match budget; an
    exhausted budget yields no program [src constants `TABLE_BUCKETS`, `TABLE_SLOTS`,
    `SEED_BYTES`; `ST/store/policy.rs` `METADATA_MATCH_BUDGET_BYTES`].
  - Kept only if strictly shorter than FULL; directory pages only if ≥ 25 % saving
    [doc encoding.md "Program pages"].
- Metadata chain: depth 8 default; budgets 65,536 canonical / 139,281 encoded B
  [src `ST/store/policy.rs` L141–145].

### 4.3 Pooled inode values: ordinals and value blocks

- Each distinct 73 B value gets a **u32 ordinal** from the Save's reserved block
  (`reserve(packs 4096, ordinals 16384)`; `store_policy.next_ordinal` ≤ 2^32)
  [doc physical-storage/README "Saves"; src `PE/sql/sqlite/metadata.sql`].
- An ordinal is reused for the same value when a named base leaf or earlier object in the
  Save already gave it one (memo ≤ 131,072, `METADATA_INDEX_VALUES`) [src
  `ST/store/policy.rs` L158; doc]. **[inference]** The same 73 B value can therefore hold
  several ordinals across Saves; ordinals are physical names, never identity.
- Values publish before any leaf naming them; reading rebuilds the canonical leaf from
  body + looked-up values and checks the leaf's ID [doc encoding.md pool/].
- **Field-packed value blocks** (`inode_value_block`, #91) [src `PE/src/blocks/values.rs`,
  `frame.rs`]:

| Bytes | Field |
| --- | --- |
| 1 | format = 1 |
| 1 | entry count n ∈ [1, 255]; ordinals `first ..= first+n−1` (implicit) |
| 1 | distinct-mode count m ∈ [0, 255] |
| 4m | mode table (4 bytes each, as stored) |
| … | n entries |
| 4 | CRC32C (Castagnoli, reflected poly `0x82F63B78`, init/final xor all-ones) of all bytes before, LE |

  - Root-form entry: `tag (=byte 0) | LEB128 ref count | content root 32 | attribute root 32`.
  - Inline entry: `tag | LEB128 ref count | content root 32 | mode-table index u8 |
    zig-zag LEB128 (mtime_s − previous inline mtime_s) | mtime ns 4 B`.
  - Anything else: `0xFF | 73 raw bytes`.
  - Body ≤ `BODY_LIMIT = 4,000` B ("one row per 4 KiB SQLite page with no overflow, two
    rows per 8 KiB PostgreSQL page, stored PLAIN"). No compression; decoding rebuilds the
    exact 73 bytes, so no identity depends on the block layout.
  - SQL: `first_ordinal` PK, `last_ordinal − first_ordinal < 255`, `length(body)` 9–4,000
    [src `PE/sql/sqlite/metadata.sql`]. A block overlapping a stored one fails its
    publication [doc engine/store/README Invariants].
- **Binding-hint blocks** (`binding_hint_block(grp, seq)`, grp = child serial >> 6):
  `format 1 | LEB128 n | entries | CRC32C`; entry = Δchild LEB128, zig-zag Δparent,
  shared-prefix length u8, LEB128 suffix length, suffix bytes; rows never changed
  [src `PE/src/blocks/hints.rs`]. Hints, not facts: checked against the tree before use
  [src `HI/contract/records.rs` `BindingHint` doc].
- **Vestigial grammar [inference]:** `CT/object/inode_leaf.rs` still defines a 94-byte
  pooled value object (`"LFSIVL1\0"` + 73 B, `encode_pooled_value`), but `git grep` finds no
  caller in `core/` or `engine/` outside its own module; pack versions 21/22 of the
  "retired pooled value-group lane" are refused [doc physical-storage/README Packs].
  Values now live only in `inode_value_block` rows.

---

## 5. Packs

### 5.1 Layout [src `ST/pack/layout.rs`, `ST/pack/assemble.rs`]

| Offset | Width | Field |
| ---: | ---: | --- |
| 0 | 8 | `"LFPACK\0\0"` |
| 8 | 4 | framing version u32 LE |
| 12 | 4 | group count u32 LE |
| 16 | 4 | used (assembled) length u32 LE |
| 20 | 4 | reserved, must be 0 |
| 24 | 16·g or 4·g | directory: ordinary/chunk/singleton entries `start u32 | encoded len u32 | decoded len u32 | codec u8 (0 raw, 1 zstd) | 3 pad`; whole-file entries `start u32` |
| … | … | group bodies, ending exactly at `used` |

- Group body: `record count u32 LE | end offset u32 LE × count | records`
  [src `assemble.rs` `framed_group_length` doc].
- Lanes and written versions [src `PackLane::version`, `for_role`]:

| Lane | Roles | Writes version | Domain |
| --- | --- | ---: | --- |
| Ordinary | 3–13 (all tree/metadata roles) | 26 (`VERSION_ORDINARY_PAGES`) | Metadata (0) |
| Chunk | 2 | 25 (`VERSION_CHUNK_SIZED`) | Payload (1) |
| WholeFile | 1 | 23 (`VERSION_WHOLE_FILE_SIZED`) | Payload (1) |
| Singleton | 1, when compact form does not fit 256 KiB | 16 (`VERSION_SINGLETON_STORED`) | Payload (1) |

  Older readable versions: ordinary 9, 19, 24; chunk 10, 15, 20; whole file 11, 14, 17,
  18; singleton 13; 21/22 and pre-v9 refused [doc physical-storage/README; src layout.rs].
  Domain mapping [src `ST/store/location.rs` `PackDomain::for_lane`;
  `PE/src/backend/records.rs` L90 `1 => Payload`].
- Limits [src `ST/store/policy.rs`]: `PACK_LIMIT` 256 KiB; `GROUP_TARGET` 48 KiB,
  `GROUP_CANONICAL_BYTES_LIMIT` 512 KiB; group body ≤ `GROUP_LIMIT` 64 KiB; ≤ 16 groups
  written (`TIGHT_GROUPS`), 256 readable (`GROUP_COUNT_LIMIT`); ≤ 8,191 records/group
  (`RECORD_COUNT_LIMIT`); singleton ≤ 16 MiB + 4,096.
- Ordinary groups may be zstd level 1 (window ≤ 16) when smaller; payload groups are raw
  because records are already compressed [doc encoding.md].
- **Key / sealing:** `ObjectKey` = SHA-256 of all sealed pack bytes, computed once in
  `PublishedPack::seal`; stored in `pack.digest`; checked with length on every whole-pack
  read (`PersistedPack::authenticate`) [src `ST/port/persistence.rs` L41;
  doc Invariants]. A sealed pack is never appended to; v23–v26 directories hold exactly
  `group count` entries since every Save seals packs whole (#352 F-027) [src layout.rs doc].
- **Placement:** pack ids from the Save's reserved block (`next_pack_id`, blocks of 4,096),
  one-pass assembly, one SHA-256 (`Packer::place`, `ST/save/seal.rs`) [doc]. All lanes share
  a 256 KiB / 512-row seal queue.

### 5.2 Locator

`object_location(object_id → role, canonical_length, pack_id, group_number 0–255,
record_number 0–8,190)` [src `PE/sql/sqlite/metadata.sql`; `ST/store/location.rs`
`ObjectLocation`]. Insert-if-absent; first writer wins and the loser's bytes are compared
(`Published { lost }`) [src `ST/port/persistence.rs` `Publication`, `Published`].
Locator + pack directory → group → record end offsets → record; no per-object byte offset
is stored [**inference** from the schema and group framing].

---

## 6. Store schema

### 6.1 SQLite (product layout: Monolithic + acquisition tables, `user_version` 42)

Default `sqlite_pack_layout = Monolithic` (36) [src `PE/src/store/config.rs` L116, L173];
product openers add `SqliteAcquisitionSchema::Tables` (+6)
[src `engine/crates/layerfs-project/src/store/init.rs` L124, `live.rs` L118]; no product
caller selects GroupRows (37/38) [git grep, **inference**]. All tables `STRICT`.

| Table | Key | Columns (abridged) | Mutability | Source |
| --- | --- | --- | --- | --- |
| `store_policy` | id = 1 | schema_version, format_profile, cutoff, 3 depths, `next_pack_id`, `next_ordinal` | counters advance | `sql/sqlite/metadata.sql` |
| `pack` | `pack_id` > 0 | domain 0/1, digest 32, length 32…16,781,312, body (= length); `CHECK(domain=1 OR length ≤ 262,144)` | **triggers abort UPDATE/DELETE** | `sql/sqlite/objects.sql` |
| `pack_unit` (37/38 only) | unit_id; UNIQUE(pack, group) | group body rows; `pack.control` holds header+directory (24–4,120 B) | triggers abort | `objects_units.sql` |
| `object_location` | `object_id` BLOB PK, WITHOUT ROWID | role 1–13, canonical_length, pack_id FK, group, record | insert-only | `metadata.sql` |
| `inode_value_block` | `first_ordinal` | last_ordinal, body 9–4,000 | insert-only, no overlap | `metadata.sql` |
| `content_signature` | slot 0–8,191 | stamp (`(stamp−1)%8192 = slot`), object_id FK, signature 32 | upsert (ring) | `metadata.sql` |
| `store_totals` | id = 1 | logical, canonical, stored, metadata, init_* | counters | `metadata.sql` |
| `history_meta` | id = 1 | catalog_id, incarnation, binding_key, identity_format = 3, schema text | fixed | `sql/sqlite/history.sql` |
| `layer_stack` | 17 B id, tag `0x31` | name (1–63, `[a-z0-9._-]`), scope_id, profile_id, head_layer_id | head moves | `history.sql` |
| `layer` | 33 B id, tag `0x32` | stack, parent, root_id, source_branch, source_commit, logical, overwrote (legacy, now empty) | immutable | `history.sql` |
| `"commit"` | 33 B id, tag `0x12` | stack, root, parent, base_layer, second_parent (Commit or Layer), branch, via 1–3, message ≤ 4,096 B, merged_from, created_ms, logical/new_canonical/new_stored/new_metadata, `lww`, displaced | immutable | `history.sql` |
| `branch` | 17 B id, tag `0x11` | stack, name, base_layer, `forked_from_id` (33 B node), head_commit | head moves | `history.sql` |
| `scope_allocator` | scope_id | highwater, authority_id | highwater only grows | `history.sql` |
| `binding_hint_block` | (grp, seq) | body 9–4,000 | append-only | `history.sql` |
| `file_digest` | (digest, length) WITHOUT ROWID | root | **upsert** (newer root wins) | `sql/sqlite/file_digest.sql`; `queries/history/digest_insert.sql` |
| `layer_lineage` | layer_id | stack, height, overwrote_count | insert | `sql/sqlite/lineage.sql` |
| `commit_lineage` | commit_id | run (33 B), depth ≥ 1, newest, merged | insert | `lineage.sql` |
| `absorbed` | (run, depth) | height | insert | `lineage.sql` |
| `reach` | (key, x) | height | insert + prune (frontier) | `lineage.sql`; `queries/history/lineage_reach_prune.sql` |
| `merge_operation`, `merge_record` | operation / (op, kind, key) | scratch for in-lane merge | swept | `merge_scratch.sql` |
| `init_*` (42–44) | — | Project Init working rows | truncated on release | `sql/sqlite/acquisition/schema.sql` |

Indexes: `layer_stack_names(name)` UNIQUE; `layers_genesis(stack) WHERE parent IS NULL`;
`layers_child(stack, parent)` UNIQUE partial (linear chain); `layers_source(source_branch,
source_commit)` UNIQUE partial; `branch_names(stack, name)` UNIQUE; `layer_heights(stack,
height)` UNIQUE; `commit_runs(run, depth)` UNIQUE; `run_merges` partial;
`absorbed_heights(run, height)`; `pack_unit_mapping` (38 only) [src `history.sql`,
`lineage.sql`, `objects_units_index.sql`]. Triggers `commit_logical`, `layer_logical` add
each new row's `logical` to `store_totals` [src `history.sql`]. History FKs are
`DEFERRABLE INITIALLY DEFERRED` [src].

Session profile `sqlite-wal-off-v2`: `journal_mode=wal`, `synchronous=0`,
`wal_autocheckpoint=0` (own PASSIVE checkpoint thread past 4 MiB), `page_size=4096`,
`busy_timeout=0`, `cache_size=-2048`, `mmap_size=0` [doc `engine/store/sqlite.md`].
Durable: `NOT_RUN — disabled by owner until explicit reauthorization`.

### 6.2 PostgreSQL deltas [src `PE/sql/postgresql/schema.sql`]

- One schema per Store; `store_lane(id=1)` row is the cross-process writer lock.
- `store_policy.schema_version = 2`, `history_meta.schema_version = 2`.
- `pack.body bytea STORAGE EXTERNAL` (no TOAST compression, ranged reads); one trigger
  function `pack_immutable` on UPDATE OR DELETE. No `pack_unit`.
- `object_location` has **no** primary key; index `object_location_prefix` on the first
  8 bytes of `object_id` cast to bigint; uniqueness by insert-when-absent under the lane
  (#90). Columns reordered for alignment (bigint, integer, smallint…) and `object_id`
  `STORAGE PLAIN`.
- Short `bytea`/`text` columns `STORAGE PLAIN`; names `COLLATE "C"` with a regex check.
- Same history, lineage and `file_digest` tables; `UNIQUE` constraints inline.

### 6.3 Which path writes what

- **Object path (parallel construction, serialized short writes).** Construction,
  representation selection, compression and pack sealing run in the caller with no Store
  lock; separate storage handles may save concurrently [doc physical-storage/README
  "One Save per handle ... separate handles may save concurrently"]. Each batch's
  `publish` is **one short write transaction** (`session.run(true, …)`,
  `PE/src/storage/provider.rs` L137) that writes `pack` (or `pack`+`pack_unit`),
  `object_location` (`ON CONFLICT DO NOTHING RETURNING`), `store_totals`,
  `inode_value_block`, the released ordinal tail in `store_policy`, and
  `content_signature` upserts [doc `engine/store/sqlite.md` "Objects"]. `reserve`
  (also `run(true)`, provider.rs L109) advances `next_pack_id`/`next_ordinal`.
  These take the one-writer lock per transaction but **not** the held publication lane.
- **Pre-lane history writes:** binding hints (`binding_hint_block`), file digests
  (`file_digest`), inode-serial reservations (`scope_allocator`) — each its own short
  write [doc engine/store/README "The publish lane in a Commit": "Save.finish; count;
  binding hints; validate — short writes" before the lane].
- **Publication lane** (`publish_lane()`, `engine/crates/layerfs-store-service/src/store/
  commit.rs` L255): re-read head, merge what landed (`merge_operation`/`merge_record`
  scratch), then `commit_branch` = one write: insert/verify `"commit"`, insert
  `commit_lineage`, advance `branch.head_commit_id`, trigger on `store_totals`
  [doc engine/store/README; src `PE/src/history/commit.rs`].
- `add_layer`, `fork`, `merge_branch`, `initialize_layerstack` check expected heads inside
  their own write transaction (layer, layer_lineage, absorbed, reach, layer_stack head;
  branch rows) [doc engine/store/README "History calls"].

---

## 7. History model [src `HI/contract/identity.rs`, `records.rs`; doc `core/history/README.md`]

- **LayerStack** = one project's linear timeline of published Layers; id 17 B
  (`0x31` ‖ 16 B from authority); holds `scope`, `profile`, `head_layer`.
- **Layer** = one publication on the main line; id 33 B = `0x32` ‖
  `BLAKE3("layerfs/layer/v2\0" ‖ stack ‖ opt(parent) ‖ root)`; provenance
  (source Branch/Commit) outside the ID [src identity.rs L353–357]. Genesis has no parent
  and no provenance; every other Layer has all three (SQL CHECK). `add_layer` is
  overwrite-only: Layers not contained are counted in `overwrote_count` (lineage) and
  paged by height.
- **Branch** = 17 B id (`0x11`; `named(stack, name)` = first 16 B of
  `BLAKE3("layerfs/sdk-branch/v1", stack, name)` or from authority); `base_layer`,
  optional `head_commit` (absent when forked from a Layer), `forked_from` (33 B node:
  Commit or Layer) [doc history/README Records; src `history.sql`].
- **Commit** = 33 B id = `0x12` ‖ `BLAKE3("layerfs/commit/v3\0" ‖ root ‖ opt(parent) ‖
  base_layer ‖ ["\0second-parent\0" ‖ second] ‖ "\0meta\0" ‖ opt(branch) ‖ via u8 ‖
  msg_len u64 BE ‖ opt(message) ‖ opt(merged_from) ‖ opt(displaced))`; `opt(x)` = `0x00`
  or `0x01 ‖ x` [src identity.rs L293–311, `update_optional`]. Outside the ID:
  `created_ms`, statistics, `lww` (encoded LWW decisions: path, rule, flags; then
  `FF FF FF FF` and merge units) [src `PE/src/history/facts.rs`]. An identical
  re-derived Commit is verified, not inserted twice.
  `via` = Workspace 1, Files 2, Merge 3.
- **Ancestry without walking** (#353) [src `PE/src/history/lineage.rs` module doc]:
  - `layer_lineage.height` = position in the stack chain;
  - `commit_lineage(run, depth, newest, merged)`: `run` = first-parent path named by its
    first Commit, `depth` = first-parent depth above the base Layer, `newest` = height of
    newest Layer in its ancestry, `merged` = second parent for a merge;
  - `absorbed(run, depth, height)`: the first Layer whose ancestry holds a Commit
    absorbs it; one publication absorbs a set of run prefixes;
  - `reach(key, x, height)`: frontiers of Layers published from a source, keeping only
    non-dominated rows so the maximum is one indexed lookup.
  - "Walks visit run segments and merges, never ordinary Commits or the Layers between two
    heights, and have no budget: every answer is exact."
  - `merge_base` tie-break: newest Layer ancestry, then deepest first parent, then
    smallest identity [doc history/README].
- **Inode serials** (`scope_allocator`): `reserve_inodes(scope, count)` returns
  `[highwater+1, highwater+count]` and advances highwater with a compare-and-set; a range
  is spent whether used or not; `remove_layerstack` keeps the scope row; end ≥ i64::MAX
  is `Capacity` [doc history/README "Inode-serial allocation"]. Workspaces refill 1,024
  at a time; SDK file calls ≥ 4,096.
- Removal: `remove_layerstack` deletes a whole project's history rows; packs, objects,
  digests and the allocator scope stay (no GC) [doc].

---

## 8. Invariants (with enforcement)

| # | Invariant | Enforced by |
| --- | --- | --- |
| I1 | **Authenticate on read**: every decoded object (each delta-chain element, pooled leaf, program page) is rebuilt to canonical bytes and hashed against its locator's ID; whole packs are length+SHA-256 checked | `ST/encoding/delta/read.rs` `Resolver::resolve_charged`; `ST/port/read_pack.rs` `PersistedPack::authenticate`; doc encoding.md |
| I2 | **One encoding per value**: decoders refuse padding, trailing bytes, unsorted/duplicate rows, nonzero flags, wrong version, non-canonical partitions; never repair | `CT/object/codec.rs`; `CT/filesystem/sorted/format.rs` `reject_unordered`, `decode_node_value`; `CT/object/inode_leaf.rs` |
| I3 | **References derived from bytes**, never trusted from a caller; publication closed under direct references | `CT/object/admission.rs`; `ST/save/publication.rs` `register_ready` |
| I4 | **Physical form outside identity** (FULL/STORED/PREFIX, programs, ordinals, value blocks, pack layout) | `CT/object/id.rs`; `PE/src/blocks/values.rs` doc |
| I5 | **Append-only physical state**: packs and pack units immutable (triggers); locators insert-only, first writer wins, loser compared byte-for-byte; value and hint blocks insert-only (overlap fails) | `PE/sql/sqlite/objects*.sql`; `PE/sql/postgresql/schema.sql`; `ST/save/publication.rs`; engine/store/README Invariants |
| I6 | **Pack ids never reused**: `next_pack_id` only advances through `reserve`; only the unused *ordinal* tail can be released, and only while the cursor still ends there | `PE/src/metadata/allocation.rs`; engine/store/README "Allocation" |
| I7 | **Serials never recycled**; a reserved range is spent; scope rows survive stack removal | `PE/src/history/allocation.rs`; history/README |
| I8 | **Immutable history records**: Commits and Layers immutable, a re-derived ID must match every stored field; heads move only by expected-head checks | `PE/src/history/commit.rs` `new_commit`; `layerstack.rs` `verify_layer` |
| I9 | **Frozen profiles**: mapping profile ID in FileState, namespace profile in root; `store_policy` validated at open, never migrated | `CT/file/mapping/codec.rs`; `CT/filesystem/root.rs`; `ST/store/policy.rs` `StoragePolicy::validated` |
| I10 | **Unchanged subtrees keep their IDs and are not read** (path-copy updates) | `CT/filesystem/sorted/merge.rs` |
| I11 | **Bounded delta chains**: depth caps (16/8/8) and work budgets checked on write and re-checked on read | `ST/encoding/delta/select.rs`, `read.rs`; `ST/store/policy.rs` |

Exceptions to "append-only" worth stating honestly: mutable rows are `store_policy`
counters, `store_totals`, `content_signature` (ring upsert), `file_digest` (upsert,
newer root wins), `scope_allocator.highwater`, `layer_stack.head_layer_id`,
`branch.head_commit_id`, lineage `reach` pruning, and whole-project removal of history
rows. None of these is an object or a pack [src SQL above].

## 9. Discrepancies and stale points found

- `C-core-storage.md` §1.2 lists InodeLeaf value magic as `LFS6INT\0 v1`; correct, but the
  role-byte column of the 2.0 doc (`7`) vs role code (6) are different numbers — already
  noted in doc.
- `CT/object/inode_leaf.rs` keeps the `LFSIVL1` pooled-value object grammar with no
  production caller at this ref (see §4.3) [inference].
- SQLite `store_policy.schema_version` is CHECKed `= 1` while PostgreSQL's is `= 2`; the
  #91 "schema v2" refers to the PostgreSQL schema / block format, and SQLite carries the
  same block tables at its own version numbering [src both SQL files; **inference** on
  naming].
- `layer.overwrote` BLOB is written empty since #353; overwritten Layers are addressed by
  height [src `PE/src/history/layerstack.rs` L351–353].

---

## Appendix section (paper-ready, ~900 words)

### A. Committed state: objects, trees and packs

All committed state of a LayerFS Store lives in one SQLite file (or one PostgreSQL
schema) as two kinds of data: immutable, content-addressed *objects* that encode
filesystem trees, and a small set of mutable *history* rows that name tree roots.

**Objects.** Every object is a canonical byte string in a fixed envelope (Table A1). Its
identity is `ObjectId = BLAKE3("layerfs/object/v2\0" ‖ canonical bytes)`, 32 bytes. The
value begins with an 8-byte role magic, so equal payload bytes under different roles
have different identities. There are thirteen roles: whole file, chunk, extent leaf and
branch, file state, inode leaf and branch, directory leaf and branch, filesystem root,
attribute leaf and branch, and symlink. All identity-bearing integers are big-endian.
Decoders refuse padding, trailing bytes, nonzero flags, unknown versions and unsorted
rows rather than repairing them, so each logical value has exactly one encoding. Direct
references are always derived from the bytes, never supplied by a caller.

*Table A1. Canonical byte layouts (offsets in bytes).*

| Structure | Layout |
| --- | --- |
| Envelope (13 B) | `"LFSO"`[0:4] · kind `0x01`[4] · payload_len u32[5:9] = value_len+4 · value_len u32[9:13] · value |
| Tree-page header (31 B) | magic[0:8] · version u16[8:10] · role[10] · level[11] · flags=0[12] · count u16[13:15] · subtree total u64[15:23] · subtree total u64[23:31] |
| Whole file | `"LFS5SML\0"` · version u16 · payload (canonical = raw + 23 B) |
| Chunk | `"LFS4CHK\0"` · payload ≤ 32,768 B (canonical ≤ 32,789 B) |
| File state (93 B) | `"LFS4MAP\0"` · v3 · `0x0a` · 0 · length u64 · extents u64 · level u8 · profile id 32 · mapping root 32 |
| Extent leaf entry (40 B) | chunk id 32 · source offset u32 · length u32 |
| Extent branch entry (48 B) | cumulative end bytes u64 · cumulative end extents u64 · child id 32 |
| Inode value (73 B) | kind (bit 7 = inline) · link count u64 · content root 32 · {attribute root 32 \| mode u32 · mtime s i64 · ns u32 · 16 zero} |
| Filesystem root (116 B) | `"LFS6FSR\0"` · v1 · role · 0 · profile id 32 · scope id 32 · root serial u64 · inode-table root 32 |

**Trees.** A filesystem root names an inode table: a B+tree keyed by 64-bit serial whose
leaves hold 50–100 rows of (serial, 73-byte inode value) and whose branches hold 64–127
(maximum serial, child) rows (Figure A1). An inode value records the kind, a namespace
link count, a content root, and either an attribute-tree root or the mode and mtime
inline. A directory's content root is its own B+tree from raw-byte name to serial; a name
binds a serial, never an object, so a hard link is two names for one serial. Every tree
page is at most 8,192 bytes; directory and attribute pages are at least 40% full except
at the root, and branch keys are the maximum key of the child. A regular file below the
cutoff (128 KiB by default) is one whole-file object; a larger file is a file state over
an extent B+tree of 64–128 entries per page whose leaves map logical ranges to *slices*
(chunk, offset, length) of content-defined chunks. Slices let a localized edit keep the
untouched parts of boundary chunks; prefix sums in branch entries locate an offset with
one binary search per level. Holes are not special: a zero run is a sequence of
identical all-zero 32 KiB chunks, whose identical mapping pages are shared by identity.
Profile identities hashed from the frozen CDC and page parameters are embedded in each
file state and root and checked on decode. Updates copy only the root-to-change paths;
every unread subtree keeps its identity.

```
Figure A1. One directory and one chunked file.
FilesystemRoot ─► InodeLeaf (root page of the inode table)
                   1: Dir  links 0  ─► DirectoryLeaf { "a.bin" → 2, "b.txt" → 3 }
                   2: File links 1  ─► FileState(len, extents, level 1, profile)
                   3: File links 1  ─► WholeFile "LFS5SML\0" | bytes
FileState ─► ExtentBranch [(end₀,n₀,L0) (end₁,n₁,L1)]
               L0 ExtentLeaf [(C1,0,ℓ1) (C2,0,ℓ2) … ]   (64–128 entries)
               L1 ExtentLeaf [(C2,4096,ℓ) (C9,0,ℓ9) … ]  ← a slice of C2
                    ▼
               Chunk C1, C2, C9 …  (shared across files, versions and projects)
```

**Physical records.** Identity excludes physical form. Payload roles (whole file, chunk)
are stored as `FULL` (a zstd frame), `STORED` (raw bytes, chosen by an incompressibility
probe) or `PREFIX` (a zstd frame compressed against one base object's payload, recording
the 32-byte base id). Tree pages are stored whole or, for inode branches and directory
and extent pages, as a COPY/INSERT program against a predecessor page (`COPY = 0 · offset
u32 · length u32`; `INSERT = 1 · length u32 · bytes`), kept only when strictly smaller.
Inode leaves are pooled: each 73-byte value is replaced by a 4-byte ordinal, and values
live in append-only, field-packed blocks of at most 4,000 bytes and 255 entries that end
in a CRC32C. Every read rebuilds canonical bytes, base first along a delta chain, and
hashes each element against its id; delta chains are bounded by depth (16 whole-file, 8
chunk, 8 metadata) and by decoded and encoded work budgets.

**Packs.** Records are grouped (≤ 8,191 records, ≤ 64 KiB body) into immutable packs of at
most 256 KiB, in four lanes: ordinary (tree pages, optionally zstd per group), chunk,
whole file, and singleton for one record too large for a pack. A pack is a 24-byte header
(`"LFPACK\0\0"`, version, group count, used length), a directory of group start, encoded
and decoded lengths and codec, and the group bodies. A pack's key is the SHA-256 of its
sealed bytes. A locator row maps each object id to (role, canonical length, pack, group,
record).

**Schema.** Table A2 lists the tables. Objects are encoded and packed in parallel outside
any lock; each batch is published in one short write transaction that inserts packs,
locators (first writer wins, losers compared byte for byte) and value blocks. Only the
Commit's head check, merge step and Commit insertion run under the held single-writer
publication lane. History identities are domain-separated BLAKE3 digests with a one-byte
kind tag; a Commit id covers its root, parents, base Layer and identity metadata but not
its timestamp. Lineage tables record each Commit's first-parent run and depth and the
Layer height that absorbed it, so ancestry queries walk run segments, not Commits.

*Table A2. Store tables (SQLite; PostgreSQL is equivalent except as noted).*

| Group | Table | Key → contents | Written by |
| --- | --- | --- | --- |
| Physical | `pack` | pack_id → domain, SHA-256, length, body (UPDATE/DELETE refused by trigger) | object publication |
| | `object_location` | object id → role, canonical length, pack, group, record (PostgreSQL: 8-byte prefix index) | object publication (insert-if-absent) |
| | `inode_value_block` | first ordinal → ≤ 255 packed values + CRC32C | object publication |
| | `content_signature` | ring slot → object id, 32-byte sketch | object publication (upsert) |
| | `store_policy`, `store_totals` | singleton: cutoff, depths, next pack id/ordinal; byte totals | reservation, publication |
| History | `layer_stack` | 17-byte id → name, scope, profile, head Layer | Init, `add_layer` |
| | `layer` | 33-byte id → stack, parent, root, source Branch/Commit | `add_layer` |
| | `"commit"` | 33-byte id → root, parent, base Layer, second parent, message, statistics, LWW facts | lane (`commit_branch`) |
| | `branch` | 17-byte id → name, base Layer, forked-from node, head Commit | fork, merge, lane |
| | `scope_allocator` | scope → serial high-water mark | serial reservation |
| Lineage | `layer_lineage`, `commit_lineage`, `absorbed`, `reach` | heights, runs, absorption, reach frontiers | with each Layer/Commit |
| Hints | `binding_hint_block`, `file_digest` | serial group → packed (child, parent, name); BLAKE3 of file bytes → root | before the lane |

**Invariants.** (i) Every read authenticates; (ii) one encoding per value; (iii)
references are derived from bytes; (iv) packs, locators and value blocks are append-only;
(v) pack ids and inode serials are never reused; (vi) Commits and Layers are immutable,
and only Branch and stack heads move, by expected-head checks.
