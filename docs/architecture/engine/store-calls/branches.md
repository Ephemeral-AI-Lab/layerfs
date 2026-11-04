# Branches

[Architecture](../../README.md) › [Engine](../README.md) › [Store calls](README.md) › Branches

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Fork, merge and publish.

| | |
| --- | --- |
| Component | [Store calls](README.md) |
| Source | [`engine/crates/layerfs-store-service/src/history`](../../../../engine/crates/layerfs-store-service/src/history) |

`main` is the project's line of Layers (the stack's head Layer); every other
Branch has a base Layer and an optional head Commit
([`history/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/history/lookup.rs)).
Terms are in the [glossary](../../glossary.md).

## Fork

```text
branch.fork(name, { from })                       history/branch.rs::fork
  name == "main"                  -> ReservedName
  from absent or "main"           -> ForkSource::Layer(stack.head_layer)
  from is a Branch name           -> Commit{branch, head} | Layer(base)
  from is a 66-hex id:
    Layer id                      -> ForkSource::Layer(id)
    Commit id                     -> Commit{meta.branch, id}
                                     (no meta.branch -> InvalidInput)
  name invalid                    -> InvalidInput
  Branch name exists              -> NameTaken
  catalog.fork(ForkRequest{stack, BranchId::named(stack, name), ..})
  -> { kind: "Forked", branch, base, head }
```

A fork writes metadata only: one Branch row through the catalog. It copies no
tree and reads no object ([`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs)).
The Branch id is derived from the stack and the name (`BranchId::named`).

## Merge

```text
branch.merge(branch, with, { name, units?, message? })
  branch == with -> InvalidInput     Units::new(units) -> InvalidInput
  name "main"    -> ReservedName     name exists       -> NameTaken
  sides: ln = node of `branch`, rn = node of `with` (main = head Layer)
  |
  +-- one is an ancestor of the other (catalog.is_ancestor)
  |     merge_branch(MergeHead::Node(newer))     fast-forward, no Save
  |
  +-- else base = catalog.merge_base(ln, rn)     none -> NoMergeBase
        storage = store.producer(); save = begin_save()
        units?  -> units::decide (revert losing sides)
        store.merge_records(|r| merge_roots(save, base, ours, theirs))
        not clean -> drop save -> Conflict {conflicts, lww, units}
        save.finish(); logical size; record_new_bindings
        catalog.merge_branch(MergeHead::Commit(MergeCommit{
            base, parent: ln, second_parent: rn, root, meta }))
        -> { kind: "Merged", fastForward, head, root, mergeBase, ... }
```

| Step | Lane? | Source |
| --- | --- | --- |
| Ancestry, merge base, units, `merge_roots`, Save finish, logical size, bindings | No: heavy work runs before publish. | [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `catalog.merge_branch(..)` | The one publish write; timed as `branch.merge.publish`. | same; [Store: History publication](../store/history-publication.md) |

A merge never changes either source; it creates the new Branch `name`. The
merged tree takes `/.lww` rules from `branch` (`RulesFrom::Ours`). A conflict
creates nothing: the unfinished Save is dropped. The merge Commit's base Layer
is `main`'s Layer when either side is `main`, else the left Branch's base.
Fresh inode serials for merged entries are reserved 64 at a time
(`MERGE_SERIALS`). Merge build records live in the Store's merge scratch
(`Store::merge_records`); a Store without it refuses merges
`BackendUnavailable` ([`store/open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs)).

## Publish

```text
branch.publish({ branch, commit? })
  commit given: must be a Commit id (a Layer -> InvalidInput)
  else Branch head; none -> NoCommit
  catalog.add_layer(AddLayerRequest{ stack, branch, commit,
                    expected_stack_head, expected_branch_base })
    Added { layer }    -> overwritten(layer, 0, 50) read after publish
    UpToDate { layer }
    NoChanges { head }
```

| Outcome | Answer fields | Source |
| --- | --- | --- |
| `Added` | `source`, `publishedLayer`, `basedOn`, `overwroteCount`, `overwrote` (first 50), `overwroteHasMore` | [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `UpToDate` | `layer` | same |
| `NoChanges` | `headLayer` | same |
| Head or base moved | `HeadMoved` (from `HistoryError::HeadMoved` or `StackMoved`) | [`history/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |

Publish is overwrite-only: the new main Layer holds the Branch head, and the
Layers it skipped are reported, not merged. The expected heads make the publish
a compare-and-set on the catalog; there is no retry when they moved. The
remaining overwritten Layers page through `backend.store.project.list_overwritten`.

## Lookup

| Function | Looks up | Refusal | Source |
| --- | --- | --- | --- |
| `project(store, name)` | Stack by name | `NoSuchProject` | [`history/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `branch(store, stack, name)` | Branch through the stack's name index; never `main` | `NoSuchBranch` | same |
| `commit(store, id)` | Commit record | `NoSuchCommit` | same |
| `layer(store, id)` | Layer record | `NotFound` | same |
| `node(text)` | Commit or Layer id from hex | `InvalidInput` | same |
| `name(text)` | A `HistoryName` | `InvalidInput` | same |

Every lookup goes through `Store::published`, the control reader's read-only
session, so none waits on the publication lane
([Service](service.md#read-handles)).

| Listing | Order and page | Source |
| --- | --- | --- |
| `project.list_branches` | `main` and every Branch in byte-name order; catalog pages of 128 (`MAXIMUM_PAGE_RECORDS`), answer by `pageSize` and 48 KiB. With `name`, one Branch from the name index, never a listing. | [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `branch.list_commits` | Newest first from the head; same paging. | same |

| History error | Refusal code |
| --- | --- |
| `HeadMoved`, `StackMoved` | `HeadMoved` |
| `Missing(Branch)` / `Missing(Commit)` / other `Missing` | `NoSuchBranch` / `NoSuchCommit` / `NotFound` |
| `InvalidInput` | `InvalidInput` |
| `NotInHistory` | `NotInHistory` |
| `Busy` | `Busy` |
| `Capacity` | `Capacity` |
| `UnknownOutcome` | `Unknown` |
| anything else | `Failed` |

## Merge units

```text
units: [ "pattern" | { name, patterns: [..] } ]       (#352 F-057)
  classify ours  = unit_changes(base -> ours)
  classify theirs only if ours changed some unit
  decide_units: a unit both sides changed -> kept from the later write
                (ours on a tie)
  for each side that lost a unit:
    revert_units(base, side, lost) in the same Save -> new side root
  ordinary merge runs on the reverted roots
```

A unit both sides changed comes whole from the side with the later write, so
nothing inside it conflicts. A unit is a list of glob patterns; a bare pattern
is its own name. Overlapping units belong to the first listed. The answer's
`units` list reports each decision and is cut to a byte window
(`unitsTruncated`)
([`history/units.rs`](../../../../engine/crates/layerfs-store-service/src/history/units.rs),
[`methods/branch/merge.rs`](../../../../engine/crates/layerfs-store-service/src/methods/branch/merge.rs),
test [`merge_units.rs`](../../../../engine/crates/layerfs-store-service/tests/merge_units.rs)).
The decision algorithm itself is in [Core: Compare and merge](../../core/compare-and-merge/merge.md).

## Related

- [Store: History publication](../store/history-publication.md) — `fork`, `merge_branch`, `add_layer` and the publish lane.
- [Core: History model](../../core/history-model/README.md) — Branches, Commits, Layers.
- [Core: Compare and merge](../../core/compare-and-merge/README.md) — `merge_roots`, units, `/.lww`.
- [Reads](reads.md#compare) — `branch.diff_commits` and `branch.diff_branches`.
- [Commit: Publish and merge](../commit/publish-and-merge.md) — the Workspace Commit's merge with a moved head.

---

← [Store calls: Writes](writes.md) · [Up: Store calls](README.md) · [Store calls: SDK facade](sdk-facade.md) →
