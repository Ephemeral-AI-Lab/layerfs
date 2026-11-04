# Entities

[Architecture](../../README.md) › [Core](../README.md) › [History model](README.md) › Entities

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

LayerStacks, Branches, Commits, Layers and stages.

| | |
| --- | --- |
| Component | [History model](README.md) |
| Source | [`core/crates/layerfs-history/src/contract/records.rs`](../../../../core/crates/layerfs-history/src/contract/records.rs), [`core/crates/layerfs-history/src/contract/identity.rs`](../../../../core/crates/layerfs-history/src/contract/identity.rs) |

```text
 +------------------+ 1      n +------------------+
 | LayerStackRecord |----------| LayerRecord      |
 | id, name, scope, | head_    | id, stack,       |
 | profile,         | layer    | parent ----------+--> LayerRecord (older)
 | head_layer ------+--------->| root, logical,   |
 +------------------+          | source_branch,   |
        | 1                    | source_commit ---+--> CommitRecord
        | n                    | overwrote_count  |
 +------------------+          +------------------+
 | BranchRecord     | base_layer       ^
 | id, stack, name, |------------------+
 | base_layer,      |                  |
 | head_commit -----+--+               | base_layer
 | forked_from      |  |  +------------------+
 +------------------+  +->| CommitRecord     |
                          | id, stack, root, |
                          | parent ----------+--> CommitRecord (older)
                          | base_layer,      |
                          | second_parent ---+--> NodeId (Commit or Layer)
                          | meta: CommitMeta |
                          +------------------+
```

Every record is plain data: no connection, handle, path, clock or live
Workspace ([`records.rs`](../../../../core/crates/layerfs-history/src/contract/records.rs)
module docs).

## LayerStacks

| Field of `LayerStackRecord` | Meaning |
| --- | --- |
| `id` | `LayerStackId`, supplied by the caller |
| `name` | Store-unique `HistoryName` |
| `scope` | Allocation scope shared by every Branch and Commit in the stack; see [Allocation](allocation.md) |
| `profile` | Frozen filesystem profile identity |
| `head_layer` | Current head of the main line of Layers |

A LayerStack is one project's linear Layer timeline. `initialize_layerstack`
takes a `StackInitialization` (identity, name, scope, profile, an already
saved `genesis_root` and its `logical` bytes) and writes the stack and its
genesis Layer in one transaction.

## Branches

| Field of `BranchRecord` | Meaning |
| --- | --- |
| `id`, `stack`, `name` | Identity, owning stack, name unique inside the stack |
| `base_layer` | The Layer this Branch builds on |
| `head_commit` | Newest Commit; absent for a Branch forked from a Layer |
| `forked_from` | The Layer or Commit selected at creation |

`BranchSnapshot` adds resolved roots: `head_root`, `base_root`,
`effective_root` (head root, else base root), `scope` and `profile`. A caller
captures one snapshot and constructs against it.

| `ForkSource` | New Branch starts with | Source |
| --- | --- | --- |
| `Layer(id)` | `base_layer = id`, no head | [`branch.rs`](../../../../engine/crates/layerfs-persistence/src/history/branch.rs) |
| `Commit { branch, commit }` | the Commit's base Layer, `head_commit = commit`; the Commit must be in `branch`'s ancestry, else `NotInHistory` | same |

## Commits and Layers

| Field of `CommitRecord` | Meaning |
| --- | --- |
| `root` | Complete filesystem root (`ObjectId`) |
| `parent` | Previous Commit; recorded ancestry, not a diff |
| `base_layer` | Layer the Commit was built from |
| `second_parent` | Other side of a merge: a `NodeId` (Commit or Layer) |
| `meta` | `CommitMeta`, below |

| Field of `CommitMeta` | In identity | Meaning |
| --- | --- | --- |
| `branch` | yes | Branch it was made on; set by the catalog |
| `via` | yes | `CommitVia`: `Workspace` (1), `Files` (2), `Merge` (3) |
| `message` | yes | At most 4096 bytes; the provider fills `"<via> filesystem change"` when absent |
| `merged_from` | yes | Head a merge-on-commit started from |
| `displaced` | yes | Head a forced publication overwrote |
| `lww`, `lww_truncated`, `units` | no | Persisted merge decisions and merge units |
| `statistics` | no | `CommitStatistics`: `logical`, `new_canonical`, `new_stored`, `new_metadata` |
| `time_ms` | no | Set by the catalog on insert |

| Field of `LayerRecord` | Meaning |
| --- | --- |
| `parent` | Previous publication in the stack; absent only for genesis |
| `root`, `logical` | Published root and its regular-file bytes, counting every hardlink path |
| `source_branch`, `source_commit` | Provenance; absent only for genesis |
| `overwrote_count` | How many earlier main Layers this publication displaced |

Publication is overwrite-only: `add_layer` puts the Commit's root on the stack
head, whatever main gained since the Branch's base, and counts the Layers the
Commit does not contain in `overwrote_count`. They stay in history;
`HistoryCatalog::overwritten` pages them
([`layerstack.rs`](../../../../engine/crates/layerfs-persistence/src/history/layerstack.rs)).

## Stages

There is no stage entity. `CommitRequest` documents it: "There is no stage row
and no token: the expected head is the only precondition, carried by the
request itself" ([`records.rs`](../../../../core/crates/layerfs-history/src/contract/records.rs)).
Staging was removed by issue #327; the stage tables and calls described in
[`16-history.md`](../../../../core/docs/architecture/16-history.md) are historical.

## Identity

```text
 BranchId / LayerStackId   17 bytes   [tag][16-byte authority body]
 CommitId / LayerId        33 bytes   [tag][32-byte BLAKE3 digest]
 WorkspaceId, CatalogId    32 bytes   untagged
```

| Type | Tag | Made by | Source |
| --- | --- | --- | --- |
| `BranchId` | `0x11` | `from_authority(body)`, or `BranchId::named(stack, name)`: BLAKE3 of `layerfs/sdk-branch/v1`, stack and name, first 16 bytes | [`identity.rs`](../../../../core/crates/layerfs-history/src/contract/identity.rs) |
| `CommitId` | `0x12` | `derive`: BLAKE3 over `layerfs/commit/v3`, root, parent, base Layer, optional second parent, then the identity fields of `CommitMeta` | same |
| `LayerStackId` | `0x31` | `from_authority(body)` | same |
| `LayerId` | `0x32` | `derive`: BLAKE3 over `layerfs/layer/v2`, stack, parent and root; provenance is outside it | same |
| `WorkspaceId` | none | `from_authority`; all-zero is `InvalidInput` | same |
| `CatalogId` | none | `derive(binding_key)`: BLAKE3 of `layerfs/history/catalog/v1` and a 1–128 byte key | same |
| `NodeId` | from the bytes | `Commit(CommitId)` or `Layer(LayerId)`, told apart by the leading tag | same |

A wrong tag or width is `Integrity` on decode. Because a Layer's provenance is
outside its identity, two records with the same derived identity must agree
on every stored field; a mismatch is an integrity failure, never an overwrite.
`IDENTITY_FORMAT` is `1`.

## Related

- [Catalog and queries](catalog-and-queries.md): the operations that create and read these records.
- [Allocation](allocation.md): the scope a LayerStack carries.
- [Objects](../objects/README.md): the `ObjectId` roots records name.
- [Compare and merge](../compare-and-merge/README.md): the merge records `CommitMeta` persists.

---

← [Core: History model](README.md) · [Up: History model](README.md) · [History model: Catalog and queries](catalog-and-queries.md) →
