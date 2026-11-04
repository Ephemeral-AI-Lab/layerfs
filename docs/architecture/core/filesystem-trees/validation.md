# Validation

[Architecture](../../README.md) › [Core](../README.md) › [Filesystem trees](README.md) › Validation

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Validation, qualification and limits of a tree.

| | |
| --- | --- |
| Component | [Filesystem trees](README.md) |
| Source | [`core/crates/layerfs-content/src/filesystem/validate`](../../../../core/crates/layerfs-content/src/filesystem/validate), [`core/crates/layerfs-content/src/filesystem/qualify`](../../../../core/crates/layerfs-content/src/filesystem/qualify), [`core/crates/layerfs-content/src/filesystem/limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |

## Validation

```text
 check_operation (before any page is emitted)
   |
   +--> declared totals match cursors; resident row ceiling
   +--> FilesystemTopology::load: root decodes, scope and root serial match
   +--> new serials: absent from base, 64 per read wave
   +--> classification, windows of 64 rows:
   |      each binding is the base's own, or a placement
   +--> root invariants: root is a Directory, never bound, never "new"
   +--> build only: every Directory value has a binding list
   +--> effective tree: territory pass + rooted walks --> no cycle,
          no directory left unreachable
   v
 FilesystemTopology --> directories phase
                         (counts checked later by the reducer)
```

| Check | Refusal | Source |
| --- | --- | --- |
| Base root profile, scope, root serial | `UnsupportedProfile`, `ScopeMismatch`, `InvalidRecord("filesystem root serial")` | [`validate.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate.rs), [`root.rs`](../../../../core/crates/layerfs-content/src/filesystem/root.rs) |
| A declared new serial already in the base | `InvalidRecord("reused inode serial")` | `check_new_identities` in [`validate.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate.rs) |
| Root inode not a directory, bound by a name, or declared new on update | `InvalidRecord("root inode kind" / "root directory binding" / "root inode allocation")` | `check_root_invariants` in [`validate.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate.rs) |
| A directory placed twice in one batch | refused before any object is offered | [`validate.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate.rs) |
| A cycle, or a walk ending at an allocated, unbound directory | refused | [`validate/cycles.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate/cycles.rs) |
| A directory or symlink whose derived count is not 1 | refused by the reducer | [`references/reduce.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/reduce.rs) |
| Unsorted or duplicate change rows | `NonCanonicalOrdering` | [`input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs), [`sorted/merge.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/merge.rs) |

Validation is incremental: on an update it reads the affected bindings and
records, not the whole base. A stored directory renamed inside its own base
parent keeps its ancestry and is not walked
([`validate/in_place.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate/in_place.rs)). The territory pass lists a
moved directory's subtree only when some other placement could land inside it
([`validate/cycles.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate/cycles.rs)). Without a backing the
evidence lives in resident maps charged to `ordering_bytes`; with backed
serial state it lives in the caller's records and only two windows of base
records are resident ([`validate/backed.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate/backed.rs)).

Authentication of object bytes is the provider's job; validation checks
topology, not bytes.

## Qualification

```text
 qualify_root(reader, records, RootContext, work)
   |
   +--> decode root; scope, profile, root serial must match the context
   +--> pass 1: every inode-table row once  --> one record per inode
   +--> pass 2: FIFO from the root directory, every reachable
   |            directory listed once, 64 bindings per window
   +--> closure: each inode bound exactly namespace_ref_count times
   v
 QualifiedRoot { id, root, inodes, directories, bindings }
   .admit(context) --> FilesystemRoot   (same root and context only)
```

| Refused | Source |
| --- | --- |
| A root of another scope, profile or serial | [`qualify/proof.rs`](../../../../core/crates/layerfs-content/src/filesystem/qualify/proof.rs) |
| An invalid inode row, or one overlapping its left sibling | [`qualify/walk.rs`](../../../../core/crates/layerfs-content/src/filesystem/qualify/walk.rs) |
| A binding to a serial the table does not hold, or to the root directory | [`qualify/walk.rs`](../../../../core/crates/layerfs-content/src/filesystem/qualify/walk.rs) |
| A directory bound more than once (every alias and every reachable cycle) | [`qualify/walk.rs`](../../../../core/crates/layerfs-content/src/filesystem/qualify/walk.rs) |
| An inode bound more or fewer times than it declares (unreachable inodes and islands) | [`qualify/walk.rs`](../../../../core/crates/layerfs-content/src/filesystem/qualify/walk.rs) |

Qualification is an explicit, paid whole-root check: decoding a root proves
nothing about the rest of the graph. State lives in caller-owned indexed
records, so memory does not grow with the namespace. File mappings, symlink
targets and attribute trees are not walked; their readers check them on
demand. At HEAD no engine or API caller invokes `qualify_root`; it is
exercised by [`filesystem_qualify.rs`](../../../../core/crates/layerfs-content/tests/filesystem_qualify.rs).

## Limits

| Limit | Value | Constant | Source |
| --- | --- | --- | --- |
| Name bytes | 1 to 255 | `MAXIMUM_NAME_BYTES` | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Path bytes | 4,096 | `MAXIMUM_PATH_BYTES` | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Path components | 256 | `MAXIMUM_PATH_COMPONENTS` | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Tree page | 8,192 canonical bytes | `MAXIMUM_PAGE_BYTES` | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Tree level | 31 (the file mapping's `MAX_LEVEL`) | `MAXIMUM_TREE_LEVEL` | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Inode leaf rows | 50 to 100 for a non-root leaf | `MINIMUM_` / `MAXIMUM_INODE_LEAF_ROWS` | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Inode branch children | 64 to 127 for a non-root branch; any branch has at least 2 | `MINIMUM_` / `MAXIMUM_INODE_BRANCH_CHILDREN` | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs), [`sorted/format.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/format.rs) |
| Non-root directory or attribute page | at least 3,277 bytes (2/5 fill) | `MINIMUM_FILLED_PAGE_BYTES` | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Directory leaf rows | at most 740 (`(8192 - 44) / 11`); the exact encoded size decides | `MAXIMUM_DIRECTORY_LEAF_ROWS` | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Operation scratch | 4 MiB | `MAXIMUM_OPERATION_SCRATCH_BYTES` | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Symlink target | 4,096 bytes | `MAXIMUM_SYMLINK_TARGET_BYTES` | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Attribute domain / key / value | 64 / 255 / 32,768 bytes | `MAXIMUM_ATTRIBUTE_*` | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Keys per attribute listing | 4,096 | `MAXIMUM_ATTRIBUTE_KEYS` | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Inode serial | 1 to `i64::MAX` | `MAXIMUM_INODE_SERIAL` | [`identity.rs`](../../../../core/crates/layerfs-content/src/filesystem/identity.rs) |
| Objects per read wave | 4,096 | `MAXIMUM_READ_DEMANDS` | [`objects.rs`](../../../../core/crates/layerfs-content/src/filesystem/objects.rs) |
| Reference run tiers | 32 | `MAXIMUM_LEVELS` | [`references/runs.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/runs.rs) |

There is no cap on entries per directory, inodes per tree or bindings per
update: these grow as B+tree pages and streamed rows. On the resident route an
update with more rows than `ordering_bytes / 16` is refused with
`ObjectLimitExceeded`; the backed route declares no such ceiling.
[`filesystem_limits.rs`](../../../../core/crates/layerfs-content/tests/filesystem_limits.rs) probes each reachable limit
on both sides and names the ones no fixture can reach (tree level 31 and the
33rd run tier).

## Related

- [Representation](representation.md) — the page formats these limits bound.
- [Attributes and links](attributes-and-links.md) — derived counts and the reducer.
- [Updates](updates.md) — the operation whose first phase this is.
- [Objects: Access](../objects/access.md) — authenticated reads.

---

← [Filesystem trees: Updates](updates.md) · [Up: Filesystem trees](README.md) · [Core: Compare and merge](../compare-and-merge/README.md) →
