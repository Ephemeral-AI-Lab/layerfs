# Kinds

[Architecture](../../README.md) › [Core](../README.md) › [Objects](README.md) › Kinds

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The object kinds, inode leaves, admission and the content policy.

| | |
| --- | --- |
| Component | [Objects](README.md) |
| Source | [`core/crates/layerfs-content/src/object/inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs), [`core/crates/layerfs-content/src/object/admission.rs`](../../../../core/crates/layerfs-content/src/object/admission.rs), [`core/crates/layerfs-content/src/contract/policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs), [`core/crates/layerfs-content/src/object/output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs) |

## Object kinds

A kind is an `ObjectRole`: the logical meaning of a canonical object. It is
not a physical choice such as full or delta. The Store records the role code
next to each object; the canonical bytes carry the role as the value's magic
and, for paged roles, a role byte.

```text
 FilesystemRoot --> InodeBranch ..> InodeLeaf   one row per inode
  (LFS6FSR)         (LFS6INT/8)     (LFS6INT/7)
                                       |
       +-------------------------------+----------------+
       | content_root, by inode kind                    | metadata_root
       v                                                v (root form)
 regular file        directory           symlink     AttributeBranch
 WholeFile (LFS5SML) DirectoryBranch ..> Symlink       ..> AttributeLeaf
 or FileState        DirectoryLeaf       (LFS4LNK)            |
   (LFS4MAP)         (LFS6NSP/2, /1)                          v
     |               rows: name -> serial              FileState
     v                                                 (value bytes)
 ExtentBranch ..> ExtentLeaf --> Chunk (LFS4CHK)

 "..>" = zero or more branch levels down to the leaf level
```

| Code | `ObjectRole` | Value starts with | Names | Direct references | Owner page |
| --- | --- | --- | --- | --- | --- |
| 1 | `WholeFile` | `LFS5SML\0`, version 1 | Whole payload of a file with `0 < length < cutoff` | none | [Files: Construction](../files/construction.md) |
| 2 | `Chunk` | `LFS4CHK\0` | One CDC chunk payload | none | [Files: Chunking](../files/chunking.md) |
| 3 | `ExtentLeaf` | `LFS4MAP\0`, v3, role `0x08` | A page of payload slices | chunk IDs | [Files: Construction](../files/construction.md) |
| 4 | `ExtentBranch` | `LFS4MAP\0`, v3, role `0x09` | A page of child summaries | child page IDs | [Files: Construction](../files/construction.md) |
| 5 | `FileState` | `LFS4MAP\0`, v3, role `0x0a` | Root of a chunked or empty file | mapping root | [Files: Construction](../files/construction.md) |
| 6 | `InodeLeaf` | `LFS6INT\0`, v1, role `7` | `(serial, inode value)` rows | each row's content root and metadata root | [Inode leaves](#inode-leaves) |
| 7 | `DirectoryLeaf` | `LFS6NSP\0`, v1, role `1` | `name -> inode serial` rows | none (serials are not IDs) | [Filesystem trees: Representation](../filesystem-trees/representation.md) |
| 8 | `DirectoryBranch` | `LFS6NSP\0`, v1, role `2` | Child summaries of one directory | child page IDs | [Filesystem trees: Representation](../filesystem-trees/representation.md) |
| 9 | `InodeBranch` | `LFS6INT\0`, v1, role `8` | Child summaries of the inode table | child page IDs | [Filesystem trees: Representation](../filesystem-trees/representation.md) |
| 10 | `FilesystemRoot` | `LFS6FSR\0`, v1, role `6` | Profile, allocation scope, root serial, inode-table root | inode-table root | [Filesystem trees: Representation](../filesystem-trees/representation.md) |
| 11 | `AttributeLeaf` | `LFS4MET\0`, v1, role `9` | `domain + key -> value root` rows | each value root | [Filesystem trees: Attributes and links](../filesystem-trees/attributes-and-links.md) |
| 12 | `AttributeBranch` | `LFS4MET\0`, v1, role `10` | Child summaries of an attribute tree | child page IDs | [Filesystem trees: Attributes and links](../filesystem-trees/attributes-and-links.md) |
| 13 | `Symlink` | `LFS4LNK\0`, v1, role `5` | Symlink target bytes | none | [Filesystem trees: Attributes and links](../filesystem-trees/attributes-and-links.md) |

Codes and `ObjectRole::from_code` are in
[`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs);
an unknown code fails `InvalidRecord("object role code")`. The magics are in
[`construction/bytes.rs`](../../../../core/crates/layerfs-content/src/file/construction/bytes.rs),
[`mapping/codec.rs`](../../../../core/crates/layerfs-content/src/file/mapping/codec.rs),
[`sorted/format.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/format.rs),
[`root.rs`](../../../../core/crates/layerfs-content/src/filesystem/root.rs),
[`attributes/codec.rs`](../../../../core/crates/layerfs-content/src/filesystem/attributes/codec.rs)
and [`symlink.rs`](../../../../core/crates/layerfs-content/src/filesystem/symlink.rs).
The `ObjectRole` code and the role byte inside a value are separate numbers:
`InodeLeaf` is code 6 but carries role byte 7.

History metadata (Commits, Branches, Layers) is not a canonical object kind;
see [History model](../history-model/README.md).

## Inode leaves

[`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs)
owns one grammar: the 73-byte inode value and the leaf page of
`(serial, value)` rows. Physical storage pools these values, so the grammar
lives here where both Content and Storage can use it.

```text
 InodeLeaf canonical object
 +---------------+------------------------------+------------------------+
 | envelope 13 B | node header 31 B             | rows: count x 81 B     |
 +---------------+------------------------------+------------------------+
 |<------- POOLED_PREFIX_BYTES = 44 ----------->|

 node header (value offsets)
   0..8   "LFS6INT\0"    8..10 version = 1     10 role = 7    11 level = 0
   12     flags = 0      13..15 count (u16)    15..23 subtree_count = count
   23..31 subtree_bytes = count * 81

 row (81 B)
   0..8   serial (u64 BE, > 0, strictly increasing)
   8..81  inode value (73 B):
          0       kind code: 1 file, 2 directory, 3 symlink
                  (bit 0x80 set = inline metadata form)
          1..9    namespace_ref_count (u64 BE)
          9..41   content_root (ObjectId)
          41..73  root form:   attribute-tree root (ObjectId)
                  inline form: 41..45 mode (u32), 45..53 mtime s (i64),
                               53..57 mtime ns (u32), 57..73 zero
```

| Rule | Value or behaviour | Source |
| --- | --- | --- |
| Rows per leaf | 1 to `MAXIMUM_LEAF_ROWS = 100` for this grammar; a non-root leaf of an inode table holds at least `MINIMUM_INODE_LEAF_ROWS = 50` | [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs), [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Page size | At most `MAXIMUM_NODE_OBJECT_BYTES = 8192` canonical bytes | [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| `subtree_bytes` | Row width (`count * 81`), not value width | [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| One encoding per value | Unknown kind, nonzero inline padding or inline metadata failing `PortableMetadata::validate` is refused | [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| Placement | `InodeValue::validate(is_root)`: the root is a directory with count 0; any other inode has count ≥ 1, and only a regular file may exceed 1 | [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| One grammar | `InodeLeaf::encode`/`decode` and the sorted tree engine call the same `encode_leaf_value`/`decode_leaf_value`, so one malformation has one error | [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |

The pooled layout splits a leaf into a body that names shared values:

```text
 pooled body  = 44 B prefix + count x (serial u64 | ordinal u32)   12 B rows
 pooled value = envelope + "LFSIVL1\0" + 73 B inode value          94 B
 rebuild_leaf(prefix, rows, values) -> the identical canonical leaf
```

`pooled_physical_length`, `pooled_body`, `decode_pooled_body` and
`rebuild_leaf` define this layout. A zero ordinal, a wrong width or unordered
serials is refused. `rebuild_leaf` re-encodes from the rows, never from the
supplied prefix bytes, so the rebuilt leaf has the original ID. Which leaves are
pooled is decided by [Physical storage: Saves](../physical-storage/saves.md).

## Admission

`FinalizedObject::admit` accepts an object from outside a trusted constructor:
an ID, a role and canonical bytes.

```text
 admit(id, role, canonical, policy, inode_scope, timing)
   1. policy.validated()                       UnsupportedPolicy
   2. decode_bytes_object (ceilings first)     framing / size errors
   3. ObjectId::for_bytes(canonical) == id ?   IdentityMismatch
   4. decode_references(role, canonical)       WrongLogicalRole,
        role grammar, local summaries,         InvalidRecord, ...
        root scope == inode_scope              ScopeMismatch
   5. FinalizedObject { references derived here, no predecessors }
```

| Role | Extra local check in [`admission.rs`](../../../../core/crates/layerfs-content/src/object/admission.rs) |
| --- | --- |
| `WholeFile` | Payload is nonempty and at most `whole_file_raw_limit` |
| `ExtentLeaf`, `ExtentBranch` | Decoded level must match the supplied role |
| `FileState` | Empty state has no extents and level 0; a nonempty one has 1 to `logical_len` extents |
| `InodeLeaf` | Each value passes `validate` for a root or non-root place |
| `DirectoryBranch`, `AttributeBranch` | `subtree_count` is at least the child count |
| `FilesystemRoot` | Its scope equals the expected `inode_scope` |

Admission trusts no supplied reference list or predecessor. It checks one
object only: authority, saved dependency closure and tree topology stay with
the caller. `validate_context` adds direct-child checks
([Access](access.md#access-context)). At HEAD only tests call `admit`; product
objects come from in-process constructors through `FinalizedObject::new`, which
checks the envelope and computes the ID.

## Content policy

`ConstructionPolicy` is the checked construction profile a Store records. It
has three fields, and `validated()` refuses anything outside the supported
range with `UnsupportedPolicy` before work starts.

```text
 logical length L        representation(L)
 L == 0           --->   Empty      FileState over an empty mapping page
 0 < L < cutoff   --->   WholeFile  one WholeFile object
 L >= cutoff      --->   Chunked    FileState -> extent tree -> Chunks
```

| Field | Accepted | Default | Source |
| --- | --- | --- | --- |
| `small_file_threshold_bytes` (cutoff) | Power of two, 131,072 to 1,048,576 | 131,072 | [`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |
| `whole_file_delta_max_depth` | 0 to `MAXIMUM_DELTA_MAX_DEPTH = 50`; 0 disables whole-file delta | 16 | [`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |
| `chunk_delta_max_depth` | 0 to 50; 0 disables chunk delta | 8 | [`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |

`capacities()` derives `ConstructionCapacities` by checked arithmetic:

| Capacity | Derivation |
| --- | --- |
| `whole_file_raw_limit` | cutoff − 1 |
| `whole_file_frame_limit` | max(135,168, raw + raw/128 + 1024) |
| `whole_file_canonical_limit` | raw + 23 (13-byte envelope + 10-byte whole-file header) |
| `chunk_raw_limit`, `chunk_minimum_raw` | CDC maximum and minimum chunk sizes ([Files: Chunking](../files/chunking.md)) |
| `canonical_object_limit` | `MAX_CANONICAL_OBJECT_BYTES` = 16 MiB |
| `stream_flush_entries` | `MAX_MAPPING_ENTRIES` (128) + 64 |
| depth fields | Copied from the policy |

The delta depths are recorded for Physical storage; Content never runs a
delta codec. The whole-file codec window log is 18 for a cutoff up to
256 KiB and 20 above (`whole_file_window_log`).

## Related

- [Identity](identity.md): the envelope and ID every kind shares.
- [Access](access.md): `FinalizedObject`, contextual checks and the provider.
- [Files](../files/README.md) and [Filesystem trees](../filesystem-trees/README.md): the constructors of these kinds.
- [Physical storage: Saves](../physical-storage/saves.md): pooling and delta depth use.
- [Glossary](../../glossary.md) for LayerFS terms.

---

← [Objects: Identity](identity.md) · [Up: Objects](README.md) · [Objects: Access](access.md) →
