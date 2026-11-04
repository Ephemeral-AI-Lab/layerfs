# Representation

[Architecture](../../README.md) › [Core](../README.md) › [Filesystem trees](README.md) › Representation

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Directories, inodes and rows in canonical order.

| | |
| --- | --- |
| Component | [Filesystem trees](README.md) |
| Source | [`core/crates/layerfs-content/src/filesystem/directory`](../../../../core/crates/layerfs-content/src/filesystem/directory), [`core/crates/layerfs-content/src/filesystem/inode`](../../../../core/crates/layerfs-content/src/filesystem/inode), [`core/crates/layerfs-content/src/filesystem/rows`](../../../../core/crates/layerfs-content/src/filesystem/rows), [`core/crates/layerfs-content/src/filesystem/sorted`](../../../../core/crates/layerfs-content/src/filesystem/sorted), [`core/crates/layerfs-content/src/filesystem/root.rs`](../../../../core/crates/layerfs-content/src/filesystem/root.rs), [`core/crates/layerfs-content/src/filesystem/path.rs`](../../../../core/crates/layerfs-content/src/filesystem/path.rs) |

## Directories and inodes

```text
 FilesystemRoot ---------------------------+
   scope (32) + root serial (8)            | inode_table
                                           v
                      +--------------------------------------+
                      | inode branch (LFS6INT, role 8)       |
                      |  serial 1 --> page | serial 87 --> . |
                      +--------------------------------------+
                           |
                           v
 inode leaf (LFS6INT, role 7): rows serial -> InodeValue
 +--------+------------------------------------------------------+
 | 1      | Directory  count 0  content_root --> dir tree of "/"  |
 | 2      | Directory  count 1  content_root --> dir tree of "src"|
 | 3      | RegularFile count 2 content_root --> file root        |
 | 4      | Symlink    count 1  content_root --> LFS4LNK target   |
 +--------+------------------------------------------------------+
            each InodeValue also carries its metadata (mode, mtime)

 dir tree of "/" (LFS6NSP leaf, role 1):  ".git" -> 5 | "src" -> 2
 dir tree of "src":                       "a.rs" -> 3 | "b.rs" -> 3
```

There are two kinds of B+tree. The **inode table** is one tree per root, keyed
by inode serial. Each **directory** is its own tree keyed by name, whose leaf
rows bind a name to a serial; the directory inode's `content_root` is that
tree's root page. A name never points at an object directly: it points at a
serial, and the serial's row says what the inode is.

| Inode value field | Bytes | Meaning | Source |
| --- | --- | --- | --- |
| `kind` | 0 | 1 `RegularFile`, 2 `Directory`, 3 `Symlink`; high bit set marks inline metadata | [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| `namespace_ref_count` | 1..9 | bindings that name this inode; derived by the operation | [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| `content_root` | 9..41 | file root, directory tree root or symlink target object | [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| `metadata_root` | 41..73 | attribute-tree root, or inline mode and mtime | [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |

The 73-byte value and the leaf grammar are owned by
[Objects: Kinds](../objects/kinds.md); this module adds the typed page views
([`inode/codec.rs`](../../../../core/crates/layerfs-content/src/filesystem/inode/codec.rs),
[`directory/codec.rs`](../../../../core/crates/layerfs-content/src/filesystem/directory/codec.rs)). A serial belongs to the
allocation **scope** stored in the root ([`identity.rs`](../../../../core/crates/layerfs-content/src/filesystem/identity.rs));
serials from two scopes are not comparable.

## Rows

```text
 canonical page = LFSO envelope (13) + node header (31) + rows
 node header:
 0        8      10    11     12     13      15            23          31
 +--------+------+-----+------+------+-------+-------------+-----------+
 | magic  | ver  |role |level |flags | count | subtree     | subtree   |
 | 8 B    | =1   |     |      | =0   | u16   | count u64   | bytes u64 |
 +--------+------+-----+------+------+-------+-------------+-----------+
```

| Page | Magic | Role | Row | Row bytes | Fill rule (non-root) | Source |
| --- | --- | --- | --- | --- | --- | --- |
| Directory leaf | `LFS6NSP` | 1 | name length (u16), name, serial (u64) | 2 + name + 8 | at least 3,277 canonical bytes (2/5 of 8,192) | [`sorted/format.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/format.rs) |
| Directory branch | `LFS6NSP` | 2 | name length, last name of child, child ID | 2 + name + 32 | same | [`sorted/format.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/format.rs) |
| Inode leaf | `LFS6INT` | 7 | serial (u64), `InodeValue` | 81 | 50 to 100 rows | [`inode/codec.rs`](../../../../core/crates/layerfs-content/src/filesystem/inode/codec.rs), [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Inode branch | `LFS6INT` | 8 | serial (u64), child ID | 40 | 64 to 127 children | [`inode/codec.rs`](../../../../core/crates/layerfs-content/src/filesystem/inode/codec.rs) |

All integers are big-endian. A leaf's `subtree bytes` is the sum of its row
widths and is checked on decode; a branch records its subtree totals so a
merge can update them without reading untouched children. Page sizes are exact
arithmetic (`44 + sum of row widths`); the engine never encodes a candidate
page to see whether it fits. The root page of either tree is exempt from the
fill rule. An empty directory is one empty directory leaf
([`empty_directory`](../../../../core/crates/layerfs-content/src/filesystem/directory/update.rs)); an empty inode page is never
valid.

## Sorted order

```text
 strictly ascending keys, compared as raw bytes

 directory leaf:  ".git"  <  "Makefile"  <  "README.md"  <  "src"  <  "tests"
                   2E...      4D...          52...          73...     74...
 inode leaf:       1  <  2  <  3  <  7  <  12           (u64 serials)
 attribute leaf:   (domain, key) by domain bytes, then key bytes
```

| Rule | Effect | Source |
| --- | --- | --- |
| Names compare as bytes (`PathName::cmp`), not by locale or case. | Uppercase sorts before lowercase. | [`path.rs`](../../../../core/crates/layerfs-content/src/filesystem/path.rs) |
| The encoder refuses a row not strictly greater than the one before. | `NonCanonicalOrdering`; equal keys cannot coexist. | [`sorted/format.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/format.rs) |
| The decoder refuses unordered rows in a stored page. | A malformed tree is never re-sorted. | `reject_unordered` in [`sorted/format.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/format.rs) |
| Callers supply changes already sorted and unique. | Input is checked once and never normalized. | [`sorted/merge.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/merge.rs), [`input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs) |

The sorted engine is one B+tree mutation algorithm over both page formats
(the `Format` trait in [`sorted/format.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/format.rs)). A merge
reads only the children its changes reach, the neighbour an underfull page
joins, and the other children of a branch that splits or is joined, in
batches of at most `BATCH_CHILDREN` = 256 ([`sorted/page.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/page.rs)).
Every unread child keeps the identity its parent row records.

## Root and paths

```text
 FilesystemRoot value (116 bytes, inside the LFSO envelope)
 0        8      10    11     12            44            76      84         116
 +--------+------+-----+------+-------------+-------------+-------+----------+
 |LFS6FSR | ver  |role | flags| profile     | scope       |root   |inode     |
 |  \0    | =1   | =6  | =0   | ID (32)     | ID (32)     |serial |table ID  |
 +--------+------+-----+------+-------------+-------------+-------+----------+

 resolve("src/a.rs"):
   root serial --inode table--> Directory "/" --lookup "src"--> serial 2
   serial 2 --inode table--> Directory --lookup "a.rs"--> serial 3
   serial 3 --inode table--> RegularFile value
```

| Item | Meaning | Source |
| --- | --- | --- |
| `profile` | `profile_id()`, the ID of the frozen description `layerfs/namespace-profile/scoped-inline/v1...`; any other profile is `UnsupportedProfile` | [`root.rs`](../../../../core/crates/layerfs-content/src/filesystem/root.rs) |
| `scope` | `scope_for_seed(seed)` = ID of `layerfs/inode-scope/v1\0` + 32-byte seed | [`root.rs`](../../../../core/crates/layerfs-content/src/filesystem/root.rs) |
| root serial | serial of the root directory inode | [`root.rs`](../../../../core/crates/layerfs-content/src/filesystem/root.rs) |
| `inode_table` | root page of the inode table; the root object's only reference | [`root.rs`](../../../../core/crates/layerfs-content/src/filesystem/root.rs) |
| `LogicalPath` | empty for the root, else checked names joined by single `/` | [`path.rs`](../../../../core/crates/layerfs-content/src/filesystem/path.rs) |

[`FilesystemRead`](../../../../core/crates/layerfs-content/src/filesystem/read.rs) loads the checked root once and resolves one
component at a time; it never loads the whole inode table. A name no directory
binds is `PathNotFound`, which is distinct from a provider's missing object.
Lookups of several names or serials share ancestor page reads
([`directory/read.rs`](../../../../core/crates/layerfs-content/src/filesystem/directory/read.rs), [`inode/read.rs`](../../../../core/crates/layerfs-content/src/filesystem/inode/read.rs)),
and a provider may keep decoded pages in a bounded least-recently-used cache
([`decoded.rs`](../../../../core/crates/layerfs-content/src/filesystem/decoded.rs)); position checks still run on every use.

## Related

- [Objects: Kinds](../objects/kinds.md) — the inode value and leaf grammar.
- [Objects: Identity](../objects/identity.md) — the canonical envelope and object IDs.
- [Files](../files/README.md) — the file roots a regular-file inode names.
- [Attributes and links](attributes-and-links.md), [Updates](updates.md), [Validation](validation.md).
- [Glossary](../../glossary.md).

---

← [Core: Filesystem trees](README.md) · [Up: Filesystem trees](README.md) · [Filesystem trees: Attributes and links](attributes-and-links.md) →
