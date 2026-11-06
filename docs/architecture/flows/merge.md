# Fork and merge

[Architecture](../README.md) › [Flows](README.md) › Fork and merge

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`branch.fork`, changes on the new Branch, `branch.merge` and `branch.publish`.

All four steps are Store calls. They run where the Store's calls run (see
[A call, end to end](call.md)): in the caller's process for a PostgreSQL
Store or a sqlite Store on a local platform, else in the machine's worker.
No Sandbox daemon is involved, except when the changes are made through a
mounted Workspace. `main` is the project's line of Layers, not a Branch.
Terms are in the [glossary](../glossary.md).

## The path

```text
 caller process            machine worker            Store
                           (sqlite on another        (one writer)
                            kernel only)
 ------------------------  ------------------------  -------------------
 branch.fork(p, name, from_branch, from_commit?)
   --- store.call branch.fork --->  (when routed)
                                    resolve source:
                                    Layer | Commit
                                    catalog.fork --------> 1 write tx:
                                                           Branch row only
   <-- {kind: Forked, branch, base, head}

 changes: file.write ... -> fs.* -> private Workspace
          (or workspace.mount + workspace.commit in a Sandbox)
          Commit via Files: Save, lane, commit_branch --> [lane HELD]
                                                          1 write tx

 branch.merge(p, name, a, b, {units?, message?})
                                    ancestor? -> fast-forward
                                    else merge_base -----> read
                                    begin_save
                                    units: decide, revert
                                    merge_roots ---------> object writes
                                    conflict? drop Save
                                    Save::finish, hints -> writes
                                    merge_branch --------> 1 write tx:
                                                           sides unchanged?
                                                           new Branch row
   <-- {kind: Merged, head, root, mergeBase} | Conflict

 branch.publish(p, branch, commit?)
                                    add_layer -----------> 1 write tx:
                                                           stack head and
                                                           base unchanged?
                                                           new main Layer
   <-- {kind: Added | UpToDate | NoChanges}
```

Every write transaction above takes the Store's one writer in order:
admission turn, the Linux `flock` on the Store's directory, then
`BEGIN IMMEDIATE` (PostgreSQL: the `store_lane` row). Only the Commit in the
middle holds the lane across several transactions
([Store: the publish lane](../engine/store/history-publication.md#the-publish-lane)).

| Step | Component |
| --- | --- |
| 1 | [API: Call table](../api/call-table/README.md) |
| 2 | [Engine: Store calls](../engine/store-calls/README.md) |
| 3 | [Core: Compare and merge](../core/compare-and-merge/README.md) |
| 4 | [Core: History model](../core/history-model/README.md) |
| 5 | [Engine: Store](../engine/store/README.md) |

## Steps

| # | What happens | Component / topic | Source |
| --- | --- | --- | --- |
| 1 | `branch.fork(project, name, from_branch, from_commit?)` becomes the Store method `branch.fork { name, from }`, `from` being the Commit when given, else the Branch. | [SDK: branch](../api/sdk/branch/README.md) | [`store/calls.rs`](../../../control/crates/layerfs-control/src/store/calls.rs) |
| 2 | The source resolves to a Layer (`main` or omitted: the stack's head Layer; a Branch without a head: its base) or a Commit (a Branch's head, or a Commit id). `catalog.fork` writes one Branch row. No tree is copied and no object is read: the new Branch shares its base Layer, or its Commit and that Commit's base, with the source. | [Store calls: fork](../engine/store-calls/branches.md#fork), [History model: Branches](../core/history-model/entities.md#branches) | [`history/branch.rs` (store-service)](../../../engine/crates/layerfs-store-service/src/history/branch.rs), [`history/branch.rs` (persistence)](../../../engine/crates/layerfs-persistence/src/history/branch.rs) |
| 3 | Changes on the Branch. `file.write`, `edit`, `mkdir`, `rm`, `cp`, `mv` and `apply` bind a private Workspace in the process that runs the call, apply each change in order, and commit it with `CommitVia::Files`: the same capture, construction, merge-with-moved-head and lane as a mounted Workspace's Commit. A mounted Workspace in a Sandbox commits the same way through its daemon. | [Store calls: file writes](../engine/store-calls/writes.md#file-writes), [Flow: A Commit](commit.md) | [`write/files.rs`](../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| 4 | `branch.merge(project, name, a, b, { message, units })` becomes `branch.merge { branch: a, with: b, name }`. The checks come first: sides differ, `units` valid, `name` not `main` and free. | [Store calls: merge](../engine/store-calls/branches.md#merge), [SDK facade: refusal order](../engine/store-calls/sdk-facade.md#refusal-order) | [`store/calls.rs`](../../../control/crates/layerfs-control/src/store/calls.rs), [`methods/branch/merge.rs`](../../../engine/crates/layerfs-store-service/src/methods/branch/merge.rs) |
| 5 | If one side is an ancestor of the other, the new Branch fast-forwards to the newer side: no Save. | [Store: history publication: merge](../engine/store/history-publication.md#merge) | [`history/branch.rs`](../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| 6 | Else the merge base is found (`merge_base` walks lineage runs, not single Commits). A Save begins; merge units, when given, decide each unit whole for the side with the later write and revert the losing side's units in the same Save; `merge_roots` builds the merged tree with `/.lww` rules from `a`. | [Compare and merge: merge](../core/compare-and-merge/merge.md#merge-decisions), [Store calls: merge units](../engine/store-calls/branches.md#merge-units), [Store: lineage and ancestry](../engine/store/history-publication.md#lineage-and-ancestry) | [`history/branch.rs`](../../../engine/crates/layerfs-store-service/src/history/branch.rs), [`history/units.rs`](../../../engine/crates/layerfs-store-service/src/history/units.rs) |
| 7 | A conflict drops the unfinished Save and creates nothing. A clean merge finishes the Save, counts the logical size, records binding hints, then `merge_branch` writes the new Branch at a merge Commit (`second_parent` = right side) in one write transaction that first checks both sides are still where they were merged. | [Store: history publication: merge](../engine/store/history-publication.md#merge), [History model: Commits and Layers](../core/history-model/entities.md#commits-and-layers) | [`history/merge.rs`](../../../engine/crates/layerfs-persistence/src/history/merge.rs) |
| 8 | `branch.publish(project, branch, commit?)` puts the Branch head (or the named Commit) on the stack head as a new main Layer through `add_layer`, a compare-and-set on the expected stack head and Branch base. Publish is overwrite-only: Layers the Commit skipped are counted and listed, not merged. | [Store calls: publish](../engine/store-calls/branches.md#publish), [History model: Commits and Layers](../core/history-model/entities.md#commits-and-layers) | [`history/branch.rs`](../../../engine/crates/layerfs-store-service/src/history/branch.rs), [`history/layerstack.rs`](../../../engine/crates/layerfs-persistence/src/history/layerstack.rs) |

| Answer | Fields |
| --- | --- |
| Fork | `kind: "Forked"`, `branch`, `base`, `head` |
| Merge | `kind: "Merged"`, `fastForward`, `head`, `root`, `mergeBase`, `units` |
| Publish `Added` | `source`, `publishedLayer`, `basedOn`, `overwroteCount`, `overwrote` (first 50), `overwroteHasMore` |
| Publish `UpToDate`, `NoChanges` | `layer`; `headLayer` |

## Failures along the way

| Step | What can fail | What the caller sees |
| --- | --- | --- |
| 1–2 | New name `main` | `ReservedName` |
| 1–2 | Name invalid; a Commit id with no Branch to read it through | `InvalidInput` |
| 1–2 | Name taken | `NameTaken` |
| 1–2 | Source Branch or Commit not found | `NoSuchBranch`, `NoSuchCommit` |
| 2 | The Commit is not on the source Branch's history | `NotInHistory` |
| 3 | A write to `main` | `ReadOnly` |
| 3 | A change refused | that change's refusal with its `index`; nothing committed |
| 3 | Head moved and a changed path conflicts | `Conflict`; nothing published |
| 3 | Commit outcome unknown | `Unknown`; the private Workspace is kept, its id in the detail |
| 4 | Same side twice; malformed `units` | `InvalidInput` |
| 4 | New name `main`; name taken | `ReservedName`; `NameTaken` |
| 6 | No common history | `NoMergeBase` |
| 6 | A Store without merge scratch | `Failed` (`BackendUnavailable`) |
| 7 | Conflicting changes | `Conflict` with `sources`, `mergeBase`, `conflicts`, `lww`; nothing created |
| 7 | A side moved before `merge_branch` | `HeadMoved` |
| 8 | `commit` names a Layer | `InvalidInput` "publish takes a Commit" |
| 8 | The Branch has no Commit | `NoCommit` |
| 8 | Stack head or Branch base moved | `HeadMoved`; no retry |
| any write | Writer busy; outcome unknown | `Busy`; `Unknown`, not resent |

History errors map to codes in one place,
[`history/lookup.rs`](../../../engine/crates/layerfs-store-service/src/history/lookup.rs) `history`.

## Demonstration

These commands were written from source and were not run while writing this
page.

| Test | What it sets up | What it checks | Command | Needs |
| --- | --- | --- | --- | --- |
| [`merge_units.rs`](../../../engine/crates/layerfs-store-service/tests/merge_units.rs) | Two Branches forked from one seeded tree, each changed through SDK file calls (the later call writes the later time) | Merges with and without `units`: a unit both sides changed comes whole from the later side, ours on a tie, renames, overlapping units, hard links across a unit, units before `/.lww` rules, a long list truncated, malformed units refused, units that decide nothing leave the root byte-identical | `cargo +1.85.1 test --locked -p layerfs-store-service --test merge_units` | No `cfg` gate or `#[ignore]` |
| [`sdk_merge_paths.rs`](../../../engine/crates/layerfs-store-service/tests/sdk_merge_paths.rs) | Trees with 1x, 10x and 100x the directories, one file changed at depth 6 | A merge names its changed paths without listing the tree: statement counts grow at most by a fixed step per 10x, never with the directory count | `cargo +1.85.1 test --locked -p layerfs-store-service --test sdk_merge_paths` | No `cfg` gate or `#[ignore]` |

---

← [Flow: A Commit](commit.md) · [Up: Flows](README.md) · [Flow: Backup and restore](backup.md) →
