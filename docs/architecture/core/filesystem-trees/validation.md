# Validation

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Topology checks before an update emits pages, whole-root qualification, and
every named limit of a tree, within [Filesystem trees](README.md).

| | |
| --- | --- |
| Source | [`validate.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate.rs), [`validate/`](../../../../core/crates/layerfs-content/src/filesystem/validate), [`qualify/`](../../../../core/crates/layerfs-content/src/filesystem/qualify), [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |

## validate/

```text
 check_operation (before any page is emitted)
   +--> declared totals match cursors; resident row ceiling
   +--> FilesystemTopology::load: root decodes, scope and root serial match
   +--> new serials absent from base, 64 per read wave
   +--> classification, 64 rows per window:
   |      each binding is the base's own, or a placement
   +--> root: a Directory, never bound, never declared new
   +--> build only: every Directory value has a binding list
   +--> effective tree: territory pass + rooted walks
          --> no cycle, no directory left unreachable
   v
 FilesystemTopology --> directories phase (counts checked by the reducer)
```

| Check | Refusal | Source |
| --- | --- | --- |
| Base profile, scope, root serial | `UnsupportedProfile`, `ScopeMismatch`, `InvalidRecord("filesystem root serial")` | [`validate.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate.rs) |
| Declared new serial already in the base | `InvalidRecord("reused inode serial")` | `check_new_identities` |
| Root not a directory, bound by a name, or declared new on update | `InvalidRecord("root inode kind" / "root directory binding" / "root inode allocation")` | `check_root_invariants` |
| A directory placed twice in one batch | refused before any object is offered | [`validate.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate.rs) |
| A cycle, or a walk ending at an allocated unbound directory | refused | [`validate/cycles.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate/cycles.rs) |
| A non-root directory whose derived count is not 1 | refused by the reducer | [`references/reduce.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/reduce.rs) |
| Unsorted or duplicate change rows | `NonCanonicalOrdering` | [`input.rs`](../../../../core/crates/layerfs-content/src/filesystem/input.rs), [`sorted/merge.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/merge.rs) |

Validation is incremental: an update reads the affected bindings and records,
not the whole base. A stored directory renamed inside its own base parent
keeps its ancestry and is not walked
([`validate/in_place.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate/in_place.rs)).
The territory pass lists a moved directory's subtree only when another
placement could land inside it. Without a backing the evidence lives in
resident maps charged to `ordering_bytes`; with backed serial state it lives in
the caller's records and only two windows of base records are resident
([`validate/backed.rs`](../../../../core/crates/layerfs-content/src/filesystem/validate/backed.rs)).
Validation checks topology, not bytes: authentication is the provider's job.

## qualify/

```text
 qualify_root(reader, records, RootContext, work)
   +--> decode root; scope, profile, root serial must match the context
   +--> pass 1: every inode-table row once --> one record per inode
   +--> pass 2: FIFO from the root directory, every reachable directory
   |            listed once, 64 bindings per window
   +--> closure: each inode bound exactly namespace_ref_count times
   v
 QualifiedRoot { id, root, inodes, directories, bindings }
   .admit(context) --> FilesystemRoot   (same root and context only)
```

| Refused | Source |
| --- | --- |
| A root of another scope, profile or serial | [`qualify/proof.rs`](../../../../core/crates/layerfs-content/src/filesystem/qualify/proof.rs) |
| An invalid inode row, or one overlapping its left sibling | [`qualify/walk.rs`](../../../../core/crates/layerfs-content/src/filesystem/qualify/walk.rs) |
| A binding to a serial the table lacks, or to the root directory | same |
| A directory bound more than once (aliases and reachable cycles) | same |
| An inode bound more or fewer times than it declares (unreachable inodes, islands) | same |

Qualification is an explicit, paid whole-root check: decoding a root proves
nothing about the rest of the graph. State lives in caller-owned indexed
records, so memory does not grow with the namespace. File mappings, symlink
targets and attribute trees are not walked; their readers check them on
demand. The Store's history diagnostic calls it for each recorded root
context ([`diagnostic/history.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/history.rs));
no Commit or read path does.

## limits.rs

| Limit | Value | Constant | Source |
| --- | --- | --- | --- |
| Name bytes | 1 to 255 | `MAXIMUM_NAME_BYTES` | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Path bytes / components | 4,096 / 256 | `MAXIMUM_PATH_BYTES`, `MAXIMUM_PATH_COMPONENTS` | same |
| Tree page | 8,192 canonical bytes | `MAXIMUM_PAGE_BYTES` | same |
| Tree level | 31 (the file mapping's `MAX_LEVEL`) | `MAXIMUM_TREE_LEVEL` | same |
| Inode leaf rows (non-root) | 50 to 100 | `MINIMUM_` / `MAXIMUM_INODE_LEAF_ROWS` | same |
| Inode branch children | 64 to 127 non-root; any branch ≥ 2 | `MINIMUM_` / `MAXIMUM_INODE_BRANCH_CHILDREN` | same, [`sorted/format.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/format.rs) |
| Non-root directory or attribute page | ≥ 3,277 bytes (2/5 fill) | `MINIMUM_FILLED_PAGE_BYTES` | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Directory leaf rows | ≤ 740 (`(8192 − 44) / 11`); the exact size decides | `MAXIMUM_DIRECTORY_LEAF_ROWS` | same |
| Operation scratch | 4 MiB; a reservation that does not fit fails | `MAXIMUM_OPERATION_SCRATCH_BYTES` | same, [`sorted/budget.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/budget.rs) |
| Symlink target | 4,096 bytes | `MAXIMUM_SYMLINK_TARGET_BYTES` | [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Attribute domain / key / value | 64 / 255 / 32,768 bytes | `MAXIMUM_ATTRIBUTE_*` | same |
| Keys per attribute listing | 4,096 | `MAXIMUM_ATTRIBUTE_KEYS` | same |
| Inode serial | 1 to `i64::MAX` | `MAXIMUM_INODE_SERIAL` | [`identity.rs`](../../../../core/crates/layerfs-content/src/filesystem/identity.rs) |
| Objects per read wave | 4,096 | `MAXIMUM_READ_DEMANDS` | [`objects.rs`](../../../../core/crates/layerfs-content/src/filesystem/objects.rs) |
| Reference run tiers | 32 | `MAXIMUM_LEVELS` | [`references/runs.rs`](../../../../core/crates/layerfs-content/src/filesystem/references/runs.rs) |

There is no cap on entries per directory, inodes per tree or bindings per
update: they grow as B+tree pages and streamed rows. The resident route
refuses more than `ordering_bytes / 16` rows with `ObjectLimitExceeded`; the
backed route has no such ceiling.
[`filesystem_limits.rs`](../../../../core/crates/layerfs-content/tests/filesystem_limits.rs)
probes each reachable limit on both sides and names the two no fixture can
reach (tree level 31 and the 33rd run tier):
`cargo +1.85.1 test --locked -p layerfs-content --test filesystem_limits`
(any platform, ≤ 120 s).

## Related

- [Filesystem trees](README.md): the page formats these limits bound.
- [Updates](updates.md): the operation whose first phase this is.
- [Objects](../objects/README.md): admission and context checks of one object.
- [Engine: Store diagnostics](../../engine/store/diagnostics.md): the
  diagnostic that qualifies recorded roots.
