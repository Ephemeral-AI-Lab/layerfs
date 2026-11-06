# Publish and merge

[Architecture](../../README.md) › [Engine](../README.md) › [Commit](README.md) › Publish and merge

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Publishing a Commit, including after the head moved.

| | |
| --- | --- |
| Component | [Commit](README.md) |
| Source | [`engine/crates/layerfs-store-service/src/store/commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs), [`engine/crates/layerfs-store-service/src/store/commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs), [`engine/crates/layerfs-store-service/src/store/commit_types.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_types.rs), [`engine/crates/layerfs-store-service/src/store/settle.rs`](../../../../engine/crates/layerfs-store-service/src/store/settle.rs), [`engine/crates/layerfs-persistence/src/history/commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |

## Commit

`BoundWorkspace::commit_admitted` runs every phase on one thread. Each phase
is a telemetry child named below.

```text
 Commit thread          Workspace lock       Store                 lane
 ----------------------------------------------------------------------
 uptodate.state  ------> state job
 uptodate.head   ---------------------------> branch_snapshot (read)
 capture         ------> capture job
 producer, begin ---------------------------> begin_save
 construct       ------> many short jobs ---> Save::sink (objects)
 head            ---------------------------> branch_snapshot (read)
 merge (if moved) --------------------------> merge into same Save
 logical.known, finish, logical.count, bindings
                 ---------------------------> Save::finish, hints
                 (one reader_after_save serves both: no read-back of
                  what the Save just published)
 validate        checked_base, prepare_base_install (before the lane)
 lane            ---------------------------------------------> HOLD
 head.lane       ---------------------------> branch_snapshot      |
 merge.lane, finish.lane (if moved again, <= 1,024 serials)        |
 validate.lane   only when the in-lane merge changed the candidate |
 publish         ---------------------------> commit_branch (1 tx) |
 drop(lane)      ---------------------------------------------> FREE
 install         ------> install job (base write lock)
```

| Phase | `CommitPhase` | Lock held | Source |
| --- | --- | --- | --- |
| Admission | `Admission` | `committing` flag (whole Commit) | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| UpToDate check, capture | `Capture` | Workspace lock, one job each | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| Producer, `begin_save` | `Begin` | none | [`producer.rs`](../../../../engine/crates/layerfs-store-service/src/store/producer.rs) |
| Construct | `Construct` | Workspace lock per job only | [Construction](construction.md) |
| Head read, merge, then `Save::finish`, logical size, binding hints | `Construct`, then `Finish` | none (work that grows with the Commit stays outside the lane) | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| Lane, head re-read, in-lane merge | `Finish` | publication lane | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| `checked_base`, `prepare_base_install` | `Validate` | none (before the lane); again in the lane only after an in-lane merge step | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs), [`bind.rs`](../../../../engine/crates/layerfs-store-service/src/store/bind.rs) |
| `commit_branch` | `Publish` | lane; one history write transaction | [`commit.rs` (persistence)](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |
| Install | `Install` | Workspace lock and base write lock, one job | [Install](install.md) |

The publication lane is a FIFO admission turn plus the cross-process writer
lock on SQLite ([`lane.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lane/lane.rs)).
On a conflicted merge nothing is published and the lane guard is dropped when
the Commit's closure returns, after the local install.

`CommitRequest` names the Branch, the expected head, base Layer and root, the
`candidate_root` and `CommitMeta` (message, statistics, `merged_from`, `lww`,
`via`). `commit_branch` refuses a moved head with `HeadMoved` unless `force`,
and answers `UpToDate` when the candidate equals the current root
([`commit.rs` (persistence)](../../../../engine/crates/layerfs-persistence/src/history/commit.rs)).

## Merge with a moved head

```text
          bound root B (Workspace's base)
           /                        \
   candidate (ours)              head H (theirs) read before the lane
           \                        /
            merge_roots(base B, ours, theirs) --> M   (same Save,
                                                      merge records)
   in lane: head H2 != H ?
            merge_roots(base H, ours M, theirs H2, bound 1,024) --> M2
                                                      (in memory)
```

| Case | Result | Source |
| --- | --- | --- |
| Head unchanged | Candidate published against the bound head | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| Head moved, `force` false | `merge_candidate`: merge in the Commit's Save before the lane | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| Head moved again before the lane | `merge_in_lane`: only what landed in between, own Save, at most `LANE_MERGE_PATHS` = 1,024 serials, else `LaneMerge` | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| Clean merge | Published against the head it merged with; `MergeReport` returned | [`commit_types.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_types.rs) |
| Conflict | Nothing published; merge with marker blocks installed as local base; `PendingMerge` kept; binding moves to the head | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| Next Commit while markers remain | `CommitError::Markers` | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| `force` true | No merge; `commit_branch` with `force`, displaced head recorded | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |

A merge reserves fresh inode serials 64 at a time (`MERGE_SERIALS`). When the
installed root differs from the Workspace's own candidate, `replaced` names
both roots, and the daemon invalidates the kernel entries
`stale_after_install` visits before it answers
([`invalidate.rs`](../../../../engine/crates/layerfs-daemon/src/control/invalidate.rs)).
Merge rules are in [Core: Compare and merge](../../core/compare-and-merge/README.md).

## Relink

A relink is an unlink and re-create of files with the same bytes, as a package
manager does after removing `node_modules`. New inodes, equal bytes.

```text
 values pass page:  fresh files --> captured_digests (overlay)
                                --> file_roots(digest, length) (Store, 64/call)
   saved root found  --> reuse it: no read, no build   (files_reused)
   none              --> build; record_file_digests    (digests_recorded)
```

| Step | Source |
| --- | --- |
| The overlay keeps a whole-content digest for a file it followed from creation; an open file has none | [`digest.rs`](../../../../engine/crates/layerfs-overlay/src/payload/digest.rs) |
| Saved roots are looked up and recorded through the History catalog, outside the lane | [`digests.rs`](../../../../engine/crates/layerfs-store-service/src/store/digests.rs) |
| A fresh file a rename put in place of another gets that file as delta base | [`normalize.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/normalize.rs) |

[`relink_commit.rs`](../../../../engine/crates/layerfs-store-service/tests/relink_commit.rs)
exercises this over a replay Store (`#[ignore]`, needs `LAYERFS_364_RELINK_*`).

## Settle

```text
 failure --> published or uncertain? --yes--> keep all custody
               | no
               v
           capture taken? --no--> locally_settled = true
               | yes
               v
           resolve_failed(capture) job --ok--> locally_settled = true
                                       --err-> local_error kept
```

| Failure kind | Daemon activity | Next Commit | Source |
| --- | --- | --- | --- |
| Definite, settled, owners released | `Idle` | Admitted | [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| Published, then install or release failed | `LocalFailure` | Refused: internal `Failure::Custody`; on the wire code `Unknown`, phase `admission`. The daemon keeps the original `published`; the SDK refusal does not carry it | [`registry.rs`](../../../../engine/crates/layerfs-daemon/src/control/registry.rs) |
| Uncertain (`uncertain()` true) | `Uncertain` | Refused: internal `Failure::Custody`; on the wire code `Unknown`, phase `admission` | [`commit_types.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_types.rs) |

`settle_failure` never reads history to guess an outcome and never resends
([`settle.rs`](../../../../engine/crates/layerfs-store-service/src/store/settle.rs)).
Publication is one history write transaction: once it succeeds, the Commit
exists in the Store whether or not the reply reaches the caller.
`CommitFailure` carries `phase`, `intent`, `capture`, `prepared`, `saved`,
`published` and `locally_settled` so the caller sees exactly what happened.

## UpToDate

| Path | Condition | Work done | Source |
| --- | --- | --- | --- |
| Shortcut | `dirty_inodes` and `dirty_directory_entries` 0, no capture, no consolidation, base root equals the bound root, no pending conflict, head and base Layer unchanged | One state job and one head read; no capture, Save, lane or install | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| History | Candidate root equals the current root | Capture, construction and Save ran; `commit_branch` returns `UpToDate` and the base is installed | [`commit.rs` (persistence)](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |

An `UpToDate` answer creates no Commit. An mtime-only change keeps the base
row ([Construction](construction.md#namespace-construction)), so it can also
end in `UpToDate`.

## Related

- [Construction](construction.md) and [Install](install.md): the phases on either side.
- [Store: History publication](../store/history-publication.md): the lane and `commit_branch`.
- [Core: Compare and merge](../../core/compare-and-merge/README.md): merge decisions and markers.
- [Core: History model](../../core/history-model/README.md): Branches, Commits and Layers.
- [Daemon: Control calls](../daemon/control-calls.md): the control reply for each outcome.

---

← [Commit: Construction](construction.md) · [Up: Commit](README.md) · [Commit: Install](install.md) →
