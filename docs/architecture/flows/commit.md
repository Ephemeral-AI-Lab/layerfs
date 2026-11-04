# A Commit

[Architecture](../README.md) › [Flows](README.md) › A Commit

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`workspace.commit`: capture, construction, publish, and install of the new base.

`workspace.commit(ws, { message, force })` publishes everything the
Workspace's filesystem requests have already published to its overlay, from
any process, as one Commit on the Workspace's Branch. The daemon does the
work; the Store's publish lane is held only for the head check and the
publish. Terms are in the [glossary](../glossary.md).

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
                                        head re-read           flock]
                                        moved again? merge
                                        validate, prepare
                                        commit_branch ------> 1 write tx
                                        drop lane ==========> [FREE]
                                        install job (base :=
                                        new root; G+1 stays)
                     <--- Committed | MergeCommitted | Conflict
 <== {kind, head, commit, merged, lww} | Conflict refusal ==|
```

| Step | Component |
| --- | --- |
| 1 | [Engine: Daemon](../engine/daemon/README.md) |
| 2 | [Engine: Commit](../engine/commit/README.md) |
| 3 | [Engine: Workspace](../engine/workspace/README.md) |
| 4 | [Engine: Overlay](../engine/overlay/README.md) |
| 5 | [Core: Physical storage](../core/physical-storage/README.md) |
| 6 | [Engine: Store calls](../engine/store-calls/README.md) |
| 7 | [Engine: Store](../engine/store/README.md) |

## Steps

| # | What happens | Component / topic | Source |
| --- | --- | --- | --- |
| 1 | The API parses `ws`, checks the Sandbox record holds no intent, and sends `workspace.commit` where the Sandbox's host operations run (this process, the machine's worker, or directly to a forwarded daemon). `message` is checked as `CommitMeta` before the daemon is reached. | [Routing: daemon channel](../api/routing/daemon-channel.md#workspace-calls), [Routing table: Workspace calls](../api/routing/routing-table.md#workspace-calls) | [`workspace/calls.rs`](../../../api/crates/layerfs-control/src/workspace/calls.rs), [`sandbox/dispatch.rs`](../../../api/crates/layerfs-control/src/sandbox/dispatch.rs) |
| 2 | One control connection is lent from the Sandbox's pool; one `Commit { token, force, message }` request is sent. | [Routing: daemon channel](../api/routing/daemon-channel.md#channel-and-pool), [Daemon: bridge](../engine/daemon/bridge.md#wire-types) | [`workspace/serve.rs`](../../../api/crates/layerfs-control/src/workspace/serve.rs) `commit`, [`daemon/workspace/commit.rs`](../../../api/crates/layerfs-control/src/daemon/workspace/commit.rs) |
| 3 | The daemon's registry admits the Workspace only when its activity is `Idle`; `admit_commit` sets the Workspace's one `committing` flag. | [Daemon: control calls](../engine/daemon/control-calls.md#registry) | [`control/registry.rs`](../../../engine/crates/layerfs-daemon/src/control/registry.rs), [`store/commit.rs`](../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| 4 | UpToDate shortcut: one state job and one head read. No dirty rows, no capture, base unchanged and head unchanged → `UpToDate` with no Save, lane or install. | [Publish and merge: UpToDate](../engine/commit/publish-and-merge.md#uptodate) | [`store/commit.rs`](../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| 5 | Capture: one short job seals the active generation G and opens G+1. It waits until earlier reply attempts are recorded; it copies no rows and filters nothing by process or Exec. | [Commit: capture](../engine/commit/capture.md#the-published-frontier), [Overlay: generations and frontier](../engine/overlay/lifetimes-and-reclamation.md#generations-and-frontier) | [`lifetime/generation.rs`](../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs), [`store/captured.rs`](../../../engine/crates/layerfs-store-service/src/store/captured.rs) |
| 6 | Construction: one producer on this thread reads the captured generation page by page and builds the candidate root. Unchanged input is reused; changed files are chunked and their objects go to the Commit's Save. The Workspace lock is held per job only, never across Store I/O. | [Commit: construction](../engine/commit/construction.md), [Files: edits](../core/files/edits.md), [Trees: updates](../core/filesystem-trees/updates.md), [Physical storage: saves](../core/physical-storage/saves.md) | [`construction/driver.rs`](../../../engine/crates/layerfs-workspace/src/construction/driver.rs), [`store/producer.rs`](../../../engine/crates/layerfs-store-service/src/store/producer.rs) |
| 7 | Head read; if the head moved and `force` is false, the candidate is merged with the head in the same Save. Then `Save::finish`, logical size and binding hints: all work that grows with the Commit, before the lane. | [Publish and merge: merge with a moved head](../engine/commit/publish-and-merge.md#merge-with-a-moved-head), [Compare and merge: merge](../core/compare-and-merge/merge.md) | [`store/commit_merge.rs`](../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| 8 | Lane: the admission turn plus the Linux `flock` on the Store's directory (PostgreSQL: the `store_lane` row). Under it: head re-read; if it moved again, an in-lane merge of only what landed (at most 1,024 serials); `checked_base` and `prepare_base_install`; then `commit_branch`, one write transaction that checks the expected head and advances the Branch. | [Store: the publish lane](../engine/store/history-publication.md#the-publish-lane), [Store: one writer](../engine/store/backends.md#one-writer) | [`store/commit.rs`](../../../engine/crates/layerfs-store-service/src/store/commit.rs), [`history/commit.rs`](../../../engine/crates/layerfs-persistence/src/history/commit.rs), [`sqlite/lane/lane.rs`](../../../engine/crates/layerfs-persistence/src/backend/sqlite/lane/lane.rs) |
| 9 | Install, after the lane is dropped: one job under the Workspace lock and the base write lock sets the base root to the published root and the installed floor to G. Rows written after the capture (G+1) stay on top of the view. | [Commit: install](../engine/commit/install.md#later-changes-during-install), [Workspace: state](../engine/workspace/state.md#install) | [`workspace/install.rs`](../../../engine/crates/layerfs-workspace/src/workspace/install.rs) |
| 10 | When the installed root differs from the Workspace's own candidate (a merge), the daemon invalidates the kernel entries that changed, then replies. | [Kernel routes: coherence](../engine/kernel-routes/coherence.md#invalidation) | [`control/invalidate.rs`](../../../engine/crates/layerfs-daemon/src/control/invalidate.rs) |
| 11 | The API shapes the reply: `Committed` or `UpToDate`, with `head`, `commit`, `merged` (when the head had moved) and `lww`; a conflict becomes the `Conflict` refusal. | [SDK: workspace](../api/sdk/workspace/README.md) | [`daemon/values.rs`](../../../api/crates/layerfs-control/src/daemon/values.rs) `commit` |

### UpToDate, conflicts and uncertain outcomes

| Case | Published? | Workspace afterwards | Caller sees |
| --- | --- | --- | --- |
| Nothing changed (shortcut) | no | unchanged | `kind: "UpToDate"` |
| Candidate root equals the head root | no | base installed | `kind: "UpToDate"` |
| Head unchanged | yes | base = new root | `kind: "Committed"` |
| Head moved, merge clean | yes, against the head it merged with | base = merged root | `kind: "Committed"` with `merged { from, head }` |
| Head moved, conflict | no | the merge with marker blocks becomes the local base | `Conflict`, detail `conflicts`, `written`, `markers: false` |
| Next Commit while markers remain | no | unchanged | `Conflict`, `markers: true` |
| `force` with a moved head | yes, over the head; the displaced head recorded | base = new root | `kind: "Committed"` with `displacedHead` |
| Published, install failed | yes | activity `LocalFailure`; the daemon keeps `published`; next Commit refused | the daemon's refusal (code and `detail.phase`) |
| Outcome uncertain | maybe | activity `Uncertain`; next Commit refused | `Unknown` |

Publication is one history write transaction. Once it commits, the Commit
exists whether or not the reply reaches the caller; nothing reads history to
guess or resends ([Publish and merge: settle](../engine/commit/publish-and-merge.md#settle)).

## Failures along the way

| Step | What can fail | What the caller sees |
| --- | --- | --- |
| 1 | `ws` malformed; no Workspace of that name | `InvalidInput`; `NotMounted` |
| 1 | The Sandbox record holds an intent | `Busy` or `Unresolved` |
| 1 | `message` not text, or longer than 4,096 bytes | `InvalidInput` |
| 1 | Machine's worker not reached | `Unavailable`; nothing sent |
| 2 | Every control connection stays in use | `Busy`; nothing attempted |
| 2 | Connection lost after the request was sent | through a worker: `Unknown`; direct to the daemon: `Failed`, its message the exchange phase with `attempted: true`. Not resent. |
| 3 | Another Workspace operation or Commit is running | `Busy` |
| 3 | A previous Commit's custody is kept (`Uncertain` or `LocalFailure`) | `Unknown`, phase `admission`, "original Commit custody retained" |
| 5 | A capture already held, or the overlay is consolidating | `Busy` |
| 6–7 | Storage or construction error | `Failed` (or `Busy`, `Capacity` by cause) |
| 7–8 | Moved again past 1,024 merge serials | `Failed` (`CommitError::LaneMerge`); nothing published |
| 8 | `commit_branch` finds the head moved under `force` with another base | `HeadMoved` |
| 8 | History write outcome unknown | `Unknown`; custody kept |
| 9 | Install fails after publish | the daemon's refusal (`Failed` unless uncertain); the Commit stays published and the daemon keeps it as custody |
| 11 | Conflict | `Conflict` refusal; nothing published |

Codes from the daemon are its `ControlCode` names with `detail.phase`
([`daemon/values.rs`](../../../api/crates/layerfs-control/src/daemon/values.rs) `refusal`,
[`control/failure.rs`](../../../engine/crates/layerfs-daemon/src/control/failure.rs)).

## Demonstration

These commands were written from source and were not run while writing this
page.

| Test | What it sets up | What it checks | Command | Needs |
| --- | --- | --- | --- | --- |
| [`mounted_commit.rs`](../../../engine/crates/layerfs-daemon/tests/mounted_commit.rs) | An installed Store, a real FUSE mount, and ordinary processes that change every entry kind | Each published root, read from a fresh bind and a fresh mount, equals the tree the mount showed; `UpToDate`; incremental Commits; writes through shared mappings; an mtime-only edit | `cargo +1.85.1 test --locked -p layerfs-daemon --test mounted_commit` | Linux (`#![cfg(target_os = "linux")]`); `/dev/fuse`; root, since commands run as uid 65534 |
| [`concurrent_commits.rs`](../../../engine/crates/layerfs-daemon/tests/concurrent_commits.rs) | Several Workspaces bound through the control service on one Branch, changed through overlay jobs (no kernel mount); barriers start the Commits together | Stale Commits of different files all publish with a merge; one shared file gives one conflict; concurrent `fs` writes of different files all publish | `cargo +1.85.1 test --locked -p layerfs-daemon --test concurrent_commits` | No `cfg` gate, no `#[ignore]` |

---

← [Flow: A Workspace session](workspace.md) · [Up: Flows](README.md) · [Flow: Fork and merge](merge.md) →
