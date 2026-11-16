# Fork and merge

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`branch.fork`, changes on the new Branch, `branch.merge` and `branch.publish`: four Store calls that run where the Store's calls run, with no Sandbox daemon unless the changes come through a mounted Workspace.

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

 changes: file.write ... -> private Workspace
          (or workspace.mount + workspace.commit in a Sandbox)
          Commit: Save, lane, commit_branch -----> [lane HELD]
                                                          1 write tx

 branch.merge(p, name, a, b, {units?, message?})
                                    ancestor? -> fast-forward
                                    else merge base -----> read
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

Every write transaction takes the Store's one writer; only the Commit in the
middle holds the publish lane across several transactions. `main` is the
project's line of Layers, not a Branch: it is forked from and published to,
never written.

## Steps

| # | What happens | Owning topic |
| --- | --- | --- |
| 1 | The call is checked and routed like any Store call. | [A call, end to end](call.md), [Control: where each call runs](../control/README.md#where-each-call-runs) |
| 2 | `branch.fork` resolves its source to a Layer or a Commit and writes one Branch row; no tree is copied and no object read. | [SDK: `branch.fork`](../api/sdk/branch.md#branchfork), [Store calls: Branch history](../engine/store-calls/README.md#branch-history), [History: records](../core/history/README.md#records) |
| 3 | `file.*` writes bind a private Workspace in the process that runs the call and commit through the same capture, construction, merge and lane as a mounted Workspace. | [Store calls: file changes](../engine/store-calls/README.md#file-changes), [A Commit](commit.md) |
| 4 | `branch.merge` checks its arguments first, then fast-forwards when one side is an ancestor of the other. | [SDK: `branch.merge`](../api/sdk/branch.md#branchmerge), [Store calls: Branch history](../engine/store-calls/README.md#branch-history) |
| 5 | Otherwise it finds the merge base, decides merge units, and builds the merged tree with `/.lww` rules in one Save. | [Compare and merge: merge](../core/compare-and-merge/merge.md), [Compare and merge](../core/compare-and-merge/README.md) |
| 6 | A conflict drops the Save and creates nothing; a clean merge writes the new Branch at a merge Commit in one transaction that checks both sides are unchanged. | [Store: history calls](../engine/store/README.md#history-calls), [History: operations](../core/history/README.md#operations) |
| 7 | `branch.publish` adds the Branch head (or a named Commit) as a new `main` Layer by compare-and-set on the stack head and Branch base. Publication is overwrite-only: skipped Layers are counted and listed, not merged. | [SDK: `branch.publish`](../api/sdk/branch.md#branchpublish), [Store calls: Branch history](../engine/store-calls/README.md#branch-history) |

## Related

- [Store calls](../engine/store-calls/README.md), [Store](../engine/store/README.md)
- [Core: Compare and merge](../core/compare-and-merge/README.md), [Core: History](../core/history/README.md)
- [SDK: branch](../api/sdk/branch.md)
