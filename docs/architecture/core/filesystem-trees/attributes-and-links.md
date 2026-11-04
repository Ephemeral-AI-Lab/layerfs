# Attributes and links

[Architecture](../../README.md) › [Core](../README.md) › [Filesystem trees](README.md) › Attributes and links

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Kept attributes, symlinks, hard links and references.

| | |
| --- | --- |
| Component | [Filesystem trees](README.md) |
| Source | [`core/crates/layerfs-content/src/filesystem/attributes`](../../../../core/crates/layerfs-content/src/filesystem/attributes), [`core/crates/layerfs-content/src/filesystem/symlink.rs`](../../../../core/crates/layerfs-content/src/filesystem/symlink.rs), [`core/crates/layerfs-content/src/filesystem/references`](../../../../core/crates/layerfs-content/src/filesystem/references) |

## Attributes

```text
 InodeValue.metadata_root (bytes 41..73), two forms:

 inline form (kind byte has 0x80 set)       root form (0x80 clear)
 +------+-----------+---------+--------+    +------------------------------+
 | mode | mtime sec | mtime ns| zeros  |    | attribute-tree root ID (32)  |
 | u32  | i64       | u32     | 16 B   |    +------------------------------+
 +------+-----------+---------+--------+                  |
                                                          v
                                LFS4MET leaf (role 9): rows in key order
                                (domain, key) required=1 --> value root
                                                          |
                                                          v
                                 value root = one extent-only file:
                                 chunk --> extent leaf --> file state
```

| Kept attribute | Grammar | Source |
| --- | --- | --- |
| `portable/mode` | 4 bytes; files and symlinks use only `0o777` bits, directories `0o1777` (sticky allowed); a symlink's mode is exactly `0o777` | [`attributes/portable.rs`](../../../../core/crates/layerfs-content/src/filesystem/attributes/portable.rs) |
| `portable/mtime` | 12 bytes: seconds (`i64`) then nanoseconds (`u32`, below 1,000,000,000) | [`attributes/portable.rs`](../../../../core/crates/layerfs-content/src/filesystem/attributes/portable.rs) |
| Other domains | Domain 1 to 64 bytes UTF-8, key up to 255 bytes, no NUL, value 1 to 32,768 bytes, stored opaque; the `portable` domain accepts only `mode` and `mtime` | [`attributes/keys.rs`](../../../../core/crates/layerfs-content/src/filesystem/attributes/keys.rs), [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |

Not kept: owner, group, atime, ctime, device numbers, or any platform value.
The inline form keeps mode and mtime inside the inode value with no attribute
tree (owner decision 2026-10-18, recorded in
[`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs)). Project Init writes the inline form
([`import/namespace.rs`](../../../../engine/crates/layerfs-project/src/import/namespace.rs)). Workspace
construction writes the inline form unless the stored root form carries keys
beyond the two portable ones; then it patches that tree so other keys keep
their stored value roots ([`construction/namespace/inodes.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/inodes.rs)).
No production caller writes a key outside the `portable` domain, and the FUSE
route answers the extended-attribute calls with `ENOSYS`
([`request/callbacks.rs`](../../../../engine/crates/layerfs-fuse/src/request/callbacks.rs)).

Attribute pages use the same 44-byte prefix, 8,192-byte ceiling and 2/5 fill
rule as directory pages. A leaf row is `37 + domain + key` bytes, a branch row
`36 + domain + key` ([`attributes/codec.rs`](../../../../core/crates/layerfs-content/src/filesystem/attributes/codec.rs)). A patch
streams the base tree with a sorted list of `Set` and `Remove` requests and
rebuilds it once ([`attributes/patch.rs`](../../../../core/crates/layerfs-content/src/filesystem/attributes/patch.rs)).

## Symlinks

```text
 Symlink inode --content_root--> symlink target object
 value: 0       8      10    11     12        14
        +-------+------+-----+------+---------+------------------+
        |LFS4LNK| ver  |role | flags| length  | target bytes     |
        |  \0   | =1   | =5  | =0   | u16     | (0..4096, no NUL)|
        +-------+------+-----+------+---------+------------------+
```

| Rule | Source |
| --- | --- |
| A target is opaque bytes: at most 4,096, no NUL. No resolution, normalization or UTF-8 check. | [`symlink.rs`](../../../../core/crates/layerfs-content/src/filesystem/symlink.rs) |
| Decode refuses wrong magic, version, role, nonzero flags, short or trailing bytes. | [`symlink.rs`](../../../../core/crates/layerfs-content/src/filesystem/symlink.rs) |
| A symlink's count is exactly 1; its mode is exactly `0o777`. | [`InodeValue::validate`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs), [`attributes/portable.rs`](../../../../core/crates/layerfs-content/src/filesystem/attributes/portable.rs) |
| Reading one is `FilesystemRead::readlink` or `readlink_inode`. | [`read.rs`](../../../../core/crates/layerfs-content/src/filesystem/read.rs) |

Broken, absolute and external targets are stored as given; nothing follows
them.

## Hard links

```text
 directory "a"            directory "b"          inode table
 +-----------+            +-----------+          +-------------------------+
 | "x" -> 3  |----------->|           |          | 3  RegularFile count 2  |
 +-----------+     +------| "y" -> 3  |          +-------------------------+
                   |      +-----------+                      ^
                   +-----------------------------------------+
 add binding  --> count + 1      remove binding --> count - 1
 count reaches 0 --> inode row removed (a directory: its entries released too)
```

| Case | Result | Source |
| --- | --- | --- |
| Two names bind one serial | One inode, one content root, `namespace_ref_count` = 2 | [`references/reduce.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/reduce.rs) |
| A new inode | Count = bindings this operation retained for it; the value's own count is ignored | [`references/record.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/record.rs) |
| An existing inode | Count = stored count + signed effect the merge observed; aliases outside the changed paths are not read | [`references/reduce.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/reduce.rs) |
| A move | The addition is observed before the removal, so the count never passes through 0 | `observe` in [`update.rs`](../../../../core/crates/layerfs-content/src/filesystem/update.rs) |
| A directory or symlink bound twice | Refused: only a regular file may have count above 1 | [`InodeValue::validate`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| A directory reaches 0 | Its entries lose one reference each, recursively, a page at a time; the walk stops at an inode that still has an alias | [`references/release.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/release.rs) |

Equal bytes in two files do not make them hard links: a link is a shared
serial, not a shared content root.

## References

```text
 directory merge --observe(before, after)--> reducer
                                              | pending rows (bounded)
                                              v
                            spill: sorted run --> tiered runs (log2 merges)
                                              |
                                              v
                        final rows (serial, count, value) --> inode table merge
```

| Part | What it holds | Source |
| --- | --- | --- |
| Ordering record | Fixed 96 bytes: serial, version, tag (1 new-inode count, 2 effect total), length, value flag, tally, optional 73-byte value | [`references/record.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/record.rs) |
| Pending map | At most `maximum_pending_records` rows in memory | [`references/runs.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/runs.rs) |
| Runs | Sorted spilled batches merged into tiers; newer row wins on equal keys; at most 32 tiers | [`references/runs.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/runs.rs), [`references/merge.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/merge.rs) |
| Ordering backing | Caller-supplied run storage with one byte account; released exactly once per operation | [`references/backing.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/backing.rs) |
| Indexed state | With backed serial state, the same meanings live in caller records instead of runs | [`references/indexed.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/indexed.rs), [`references/operation.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/operation.rs) |

"Reference" here means a name binding counted against an inode. Object
references (which objects an object names) belong to
[Objects](../objects/README.md).

## Related

- [Representation](representation.md) — where bindings and inode values live.
- [Updates](updates.md) — the operation that drives the reducer.
- [Validation](validation.md) — the topology checks that run before counts.
- [Objects: Kinds](../objects/kinds.md) — the inode value grammar.
- [Files](../files/README.md) — the extent-only file form an attribute value uses.
- [Engine: Project Init](../../engine/project-init/README.md) — native symlink and alias acquisition.

---

← [Filesystem trees: Representation](representation.md) · [Up: Filesystem trees](README.md) · [Filesystem trees: Updates](updates.md) →
