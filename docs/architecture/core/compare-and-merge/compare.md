# Compare

[Architecture](../../README.md) › [Core](../README.md) › [Compare and merge](README.md) › Compare

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Comparing trees, files and text.

| | |
| --- | --- |
| Component | [Compare and merge](README.md) |
| Source | [`core/crates/layerfs-content/src/compare`](../../../../core/crates/layerfs-content/src/compare) |

## Tree comparison

A tree is two kinds of sorted B+tree: one inode table (serial to
`InodeValue`) and one listing per directory (name to serial). See
[Representation](../filesystem-trees/representation.md). Every comparison
starts from the page-skipping difference of two such trees.

```text
 difference(left, right): one ordered frontier per side

   left frontier            right frontier
   [P a1][P a2][P a3]       [P a1][P b2][P a3]     P = page (ID, level)
     |                        |                    R = row (key, value)
     +-- same ID: pop both, pages_shared += 1, nothing read
   [P a2][P a3]             [P b2][P a3]
     +-- IDs differ: open the higher-level page (both if equal level)
   [R k1][R k4][P a3]       [R k1][R k3][P a3]
     +-- rows: compare keys
         k1 = k1, same value -> skip
         k3 only right       -> visit(k3, None, v)
         k4 only left        -> visit(k4, v, None)
   [P a3]                   [P a3]   -> shared, skipped
```

| Function | Answers | Source |
| --- | --- | --- |
| `difference::<F>` | Every key whose value differs, in key order; an absent tree is empty. Counts `DifferenceWork` (`pages_read`, `pages_shared`, `rows_compared`). | [`sorted.rs`](../../../../core/crates/layerfs-content/src/compare/sorted.rs) |
| `TreeTable::open` | One root decoded once: its `FilesystemRoot` and inode table. | [`tree.rs`](../../../../core/crates/layerfs-content/src/compare/tree.rs) |
| `inode_changes` | Inodes whose value differs, ignoring the reference count and metadata form (`same_inode`). | [`tree.rs`](../../../../core/crates/layerfs-content/src/compare/tree.rs) |
| `inode_differences` | Inodes whose stored value differs, reference counts included. | [`tree.rs`](../../../../core/crates/layerfs-content/src/compare/tree.rs) |
| `binding_changes` | Names whose binding differs between two directory listings. | [`tree.rs`](../../../../core/crates/layerfs-content/src/compare/tree.rs) |
| `new_bindings` | Bindings `right` holds that `left` does not, as `(child, parent, name)`. | [`tree.rs`](../../../../core/crates/layerfs-content/src/compare/tree.rs) |
| `root_changes` | A bounded list of `PathChange` (`Added`, `Removed`, `Modified`) with a `truncated` flag. | [`tree.rs`](../../../../core/crates/layerfs-content/src/compare/tree.rs) |

`same_value` compares kind, content root and metadata root. `same_inode`
adds a decoded check: when only the metadata forms differ, the portable
values are compared, so a root-form row and an inline row of the same
metadata are equal ([`tree.rs`](../../../../core/crates/layerfs-content/src/compare/tree.rs)).

`root_changes` names changed inodes with `locate` on the right tree. A
directory on both sides expands its changed names, and each added or removed
directory lists its subtree with `subtree`. A file on both sides yields one
`Modified` path. Entries whose two sides keep the same serial and an equal
inode are dropped. A hard-linked file is reported at its first breadth-first
path only.

## Named and sorted comparison

Paths are not stored in the canonical tree, so a changed inode must be named.

```text
  locate (breadth first)              name_from_hints (top down)
  queue: / -> /a /b -> /a/x ...       changed serial 42
  list each directory, stop when      hints.bindings_of(42)
  every wanted serial is found or       -> (7,"x"), (9,"y")
  `limit` directories are listed      check: tree binds 7/"x" -> 42 ?
                                      climb parent 7 the same way to /
  cost: directories above the         keep fewest components, then name
  serial                              order (same path locate gives)
```

| Way | Used when | Gives up when | Source |
| --- | --- | --- | --- |
| `locate` | No hints, or hints could not name every inode | `limit` directories listed; unnamed serials are absent from the answer | [`tree.rs`](../../../../core/crates/layerfs-content/src/compare/tree.rs) |
| `name_from_hints` | The caller passes `BindingHints` | an inode has fewer checked bindings than `namespace_ref_count`, depth reaches 4,096, or an inode has more than 1,024 paths | [`named.rs`](../../../../core/crates/layerfs-content/src/compare/named.rs) |

`BindingHints::bindings_of` returns recorded `(parent, name)` bindings of a
serial. The engine supplies them from the history catalog (`CatalogHints`).
A hint counts only when the tree binds that name to that serial, so a stale
hint can make naming give up, never give a wrong path. `NamingWork` counts
inodes asked for, inodes climbed, binding reads and object read demands.

## Byte and text comparison

```text
 same_file_bytes(left, right)
   left == right           -> true (no read)
   open both FileViews; lengths differ, or length > limit -> false
   walk both extent lists together:
     offset  0        64K        128K       200K       256K
     left    [ c1    ][ c2       ][ c9     ][ c4      ]
     right   [ c1    ][ c2       ][ c7 ][ c8][ c4      ]
             shared   shared      read and compare     shared
   compare unshared bytes in 256 KiB windows; stop at first difference
```

| Function | Rule | Source |
| --- | --- | --- |
| `same_file_bytes` | Equal bytes, whatever the roots. A localized edit keeps its predecessor's extents, so a file edited away and back has its old bytes under another root. | [`bytes.rs`](../../../../core/crates/layerfs-content/src/compare/bytes.rs) |
| `same_file_bytes_within` | As above for files of at most `limit` bytes; larger files with different roots answer `false`. Used where the caller's hold must stay bounded. | [`bytes.rs`](../../../../core/crates/layerfs-content/src/compare/bytes.rs) |
| `same_content` | Same kind and root, or two regular files with the same bytes within `limit`. Other kinds compare roots. | [`bytes.rs`](../../../../core/crates/layerfs-content/src/compare/bytes.rs) |
| `as_text` | Valid UTF-8 with no NUL byte, else binary. | [`text.rs`](../../../../core/crates/layerfs-content/src/compare/text.rs) |
| `lines` | Split on every `\n`; a trailing newline gives a final empty line. | [`text.rs`](../../../../core/crates/layerfs-content/src/compare/text.rs) |
| `line_diff`, `diff_lines` | Longest common subsequence, removals before additions on ties. | [`text.rs`](../../../../core/crates/layerfs-content/src/compare/text.rs) |
| `match_lines` | For each line of `a`, the matching line of `b`; used by merge and blame. | [`text.rs`](../../../../core/crates/layerfs-content/src/compare/text.rs) |

A difference whose `(m + 1) * (n + 1)` table exceeds `MAXIMUM_DIFF_CELLS`
(2^20) keeps the common prefix and suffix and diffs the middle. If the
middle is still too large, it is reported as one removed block followed by
one added block. `MAXIMUM_TEXT_BYTES` (1 MiB) is the largest file compared
or merged line by line.

## Change lists

`ChangeWalk` yields every path of either root in component order, with both
sides' entries, and descends only when the caller calls `enter`. The engine
pages `branch.diff_commits` and `branch.diff_branches` with it
([`read/compare.rs`](../../../../engine/crates/layerfs-store-service/src/read/compare.rs)).

```text
  Plan::make(hints, left, right)
    inode_differences(left, right) --> changed serials (<= 65,536)
    for each side, each changed serial:
      climb checked bindings to /  --> dirty: dir serial -> {names}
    any binding missing             --> None (use full walk)

  ChangeWalk                    enter(pair) on a directory pair
    stack of Frames               same serial and plan present:
    next_pair() -> (path,L,R)       visit dirty names + changed names
                                  otherwise:
                                    merge both listings, 64 names /
                                    8 KiB per window
```

| Mode | Reads | Source |
| --- | --- | --- |
| `ChangeWalk::planned` | Only directories on a path to a changed inode, or whose listing or identity differs; inside a kept directory only the changed names. | [`changes.rs`](../../../../core/crates/layerfs-content/src/compare/changes.rs) |
| `ChangeWalk::full` | Both trees completely, listings merged by name in windows. | [`changes.rs`](../../../../core/crates/layerfs-content/src/compare/changes.rs) |
| `enter_from(pair, from)` | A resumed page: names at or after `from` only; a listed directory starts at `from` by lookup. | [`changes.rs`](../../../../core/crates/layerfs-content/src/compare/changes.rs) |

A pair either walk omits is equal on both sides, with everything below it.
A plan depends only on the two immutable roots, so one plan serves every
page of their difference; the engine keeps made plans by root pair
([`read/plans.rs`](../../../../engine/crates/layerfs-store-service/src/read/plans.rs)).

## Memoization

`Memo` wraps an `AuthenticatedObjects` reader for one comparison. Every
lookup descends its B-tree from the root, so without a memo each lookup
re-reads the upper pages.

```text
  lookup 1: root -> inner -> leaf1    reads: root, inner, leaf1
  lookup 2: root -> inner -> leaf2    reads: leaf2 (root, inner held)
  ...
  memo full (256 objects): evict least recently used; root and inner
  are used by every lookup, so they stay
```

| Property | Value | Source |
| --- | --- | --- |
| Capacity | `MEMO_PAGES` = 256 objects | [`memo.rs`](../../../../core/crates/layerfs-content/src/compare/memo.rs) |
| Eviction | Least recently used; a batch larger than the capacity is served but not held | [`memo.rs`](../../../../core/crates/layerfs-content/src/compare/memo.rs) |
| Correctness | A re-read after eviction returns the same bytes; batch size mismatches fail `BatchCardinality` | [`memo.rs`](../../../../core/crates/layerfs-content/src/compare/memo.rs) |

## Related

- [Merge](merge.md): uses tree comparison, naming, byte and text comparison.
- [Filesystem trees: Representation](../filesystem-trees/representation.md):
  the sorted trees, inode values and directory listings compared here.
- [Files: Reads and views](../files/reads-and-views.md): `FileView` and
  extent cursors used by `same_file_bytes`.
- [Objects: Access](../objects/access.md): the `AuthenticatedObjects` reader.
- [Engine: Store calls](../../engine/store-calls/README.md): the diff calls.
- [Glossary](../../glossary.md)

---

← [Core: Compare and merge](README.md) · [Up: Compare and merge](README.md) · [Compare and merge: Merge](merge.md) →
