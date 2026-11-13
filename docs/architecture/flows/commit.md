# A Commit

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`workspace.commit`: the daemon captures everything a Workspace's filesystem requests already published, constructs and publishes it as one Commit on the Workspace's Branch, then installs the new base.

## The path

```text
 caller              worker             Sandbox daemon        Store
                     (if not local)     (Commit thread)       (shared file)
 ------------------  -----------------  --------------------  -------------
 ws -> Sandbox record
 ==|== sandbox.host workspace.commit ==>|
                     control lease
                     Commit{token} ---> registry: Idle?
                                        admit_commit
                                        state job; head ----> read
                                        UpToDate? -> reply
                                        capture job (seal G)
                                        begin_save
                                        construct: overlay
                                        jobs, Save::sink ---> object writes
                                        head read ----------> read
                                        moved? merge in Save
                                        Save::finish, hints -> writes
                                        lane ===============> [HELD: turn +
                                        head re-read           writer lock]
                                        moved again? merge
                                        validate, prepare
                                        commit_branch ------> 1 write tx
                                        drop lane ==========> [FREE]
                                        install job (base :=
                                        new root; G+1 stays)
                     <--- Committed | MergeCommitted | Conflict
 <== {kind, head, commit, merged, lww} | Conflict refusal ==|
```

The publish lane is held only for the head re-check and the publish; all
work that grows with the Commit runs before it. Publication is one history
write transaction: once it commits, the Commit exists whether or not the
reply arrives, and nothing guesses or resends.

## Steps

| # | What happens | Owning topic |
| --- | --- | --- |
| 1 | The API finds the Workspace's Sandbox record, refuses a held intent, and sends `workspace.commit` where the Sandbox's Workspace calls run (here, the worker, or a forwarded daemon). | [Control: where each call runs](../control/README.md#where-each-call-runs), [SDK: `workspace.commit`](../api/sdk/workspace.md#workspacecommit) |
| 2 | One pooled control connection carries one `Commit` request to the daemon. | [Daemon channel and exec: the control channel](../control/sandboxes/daemon-channel-and-exec.md#daemon-the-control-channel), [Bridge: control records](../engine/bridge/README.md#control-records) |
| 3 | The daemon's registry admits the Workspace only when it is idle; one Commit at a time per Workspace. | [Daemon control calls: registry](../engine/daemon/control-calls.md#registry), [Commit: invariants](../engine/commit/README.md#invariants) |
| 4 | Nothing changed and the head unchanged: `UpToDate`, with no capture, Save, lane or install. | [Commit: what it does](../engine/commit/README.md#what-it-does) |
| 5 | Capture seals the active generation as the published frontier and opens the next one; it copies no rows and filters nothing by process or Exec. | [Commit: what it does](../engine/commit/README.md#what-it-does), [Overlay lifetimes: generations and frontier](../engine/overlay/lifetimes.md#generations-and-frontier) |
| 6 | One producer constructs the candidate root from the capture; unchanged input is reused, changed files are chunked into the Commit's Save. | [Commit: construction](../engine/commit/construction.md), [Files: edits](../core/files/edits.md), [Filesystem trees: updates](../core/filesystem-trees/updates.md), [Physical storage: saves](../core/physical-storage/README.md#saves) |
| 7 | If the head moved and `force` is false, the candidate is merged with the head in the same Save; then `Save::finish`, before the lane. | [Commit: what it does](../engine/commit/README.md#what-it-does), [Compare and merge: merge](../core/compare-and-merge/merge.md) |
| 8 | Under the lane: head re-read, an in-lane merge of only what landed, then one write transaction that checks the expected head and advances the Branch. | [Store: the publish lane in a Commit](../engine/store/README.md#the-publish-lane-in-a-commit), [History: operations](../core/history/README.md#operations) |
| 9 | After the lane, install sets the base to the published root; rows written after the capture stay on top of the view. | [Commit: what it does](../engine/commit/README.md#what-it-does), [Workspace: state, serials and install](../engine/workspace/README.md#state-serials-and-install) |
| 10 | For a merged root, the daemon invalidates the kernel entries that changed before it replies. | [Kernel routes: coherence](../engine/kernel-routes/README.md#coherence) |
| 11 | The reply is `Committed` or `UpToDate`; a conflict is the `Conflict` refusal and nothing is published. A published Commit whose install failed, or an uncertain outcome, is kept as custody and refuses the next Commit. | [Commit: custody and settle](../engine/commit/README.md#custody-and-settle), [Errors and events](../api/calls/errors-and-events.md) |

## Related

- [Commit](../engine/commit/README.md), [Overlay](../engine/overlay/README.md), [Workspace](../engine/workspace/README.md)
- [Daemon](../engine/daemon/README.md), [Store](../engine/store/README.md)
- [A Workspace session](workspace.md), [Fork and merge](merge.md)
