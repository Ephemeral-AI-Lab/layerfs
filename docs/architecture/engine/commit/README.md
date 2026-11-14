# Commit

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Capturing a Workspace's published frontier, constructing it, publishing a Commit (merging with a moved head) and installing the new base.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-workspace`, `layerfs-store-service`, `layerfs-overlay` |
| Source | [`layerfs-workspace/src/construction`](../../../../engine/crates/layerfs-workspace/src/construction), [`layerfs-workspace/src/workspace/install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs), [`layerfs-store-service/src/store`](../../../../engine/crates/layerfs-store-service/src/store) (`commit*.rs`, `captured.rs`, `pipeline.rs`, `producer.rs`, `settle.rs`, `bind.rs`), [`layerfs-overlay/src/lifetime/generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| Uses | [Workspace](../workspace/README.md), [Overlay](../overlay/README.md), [Store](../store/README.md), [Core: Compare and merge](../../core/compare-and-merge/README.md), [Core: Physical storage](../../core/physical-storage/README.md), [Core: History](../../core/history/README.md) |
| Used by | [Daemon](../daemon/README.md), [Store calls](../store-calls/README.md) (SDK `file.*` writes) |
| Flows | [A Commit](../../flows/commit.md), [Fork and merge](../../flows/merge.md) |

## Picture

```text
 any process          Workspace overlay file          global Store
 (no Exec needed)     (one per Workspace)             (one writer)
    | write, rename   +---------------------+
    +---------------->| active gen G        |
                      +----------+----------+
 1 capture: seal G, open G+1     |
                      +----------v----------+
                      | captured gen G      |--CapturedReader--+
                      +---------------------+                  |
 2 construct: one producer thread              +---------------v--+
   CapturedNamespace --- queue, 32 msgs ------>| Save (Commit     |
                                               | thread)          |
 3 head read, merge if moved, Save::finish     +--------+---------+
 4 lane: head re-read, in-lane merge,                   |
   commit_branch (one history transaction) ------------>| history row
 5 install: base := new root; G retired;                |
   rows of G+1 stay on top  <---------------------------+
```

`BoundWorkspace::commit_admitted` runs every phase on the Commit thread; each
phase is a telemetry child (`uptodate.state`, `capture`, `producer`, `begin`,
`construct`, `head`, `merge`, `finish`, `validate`, `lane`, `head.lane`,
`publish`, `install`).

| Phase (`CommitPhase`) | Lock held | Source |
| --- | --- | --- |
| `Admission` | `committing` flag, whole Commit | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| `Capture`: UpToDate check, capture | Workspace lock, one short job each | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| `Begin`: take the producer `Storage`, `begin_save` | none | [`producer.rs`](../../../../engine/crates/layerfs-store-service/src/store/producer.rs) |
| `Construct`: captured namespace, head read, merge | Workspace lock per job only | [Construction](construction.md), [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| `Finish`: `Save::finish`, logical size, binding hints, then the lane | none, then the publication lane | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs), [`hints.rs`](../../../../engine/crates/layerfs-store-service/src/store/hints.rs) |
| `Validate`: `checked_base`, `prepare_base_install` | none (before the lane); again in the lane only after an in-lane merge changed the candidate | [`bind.rs`](../../../../engine/crates/layerfs-store-service/src/store/bind.rs), [`install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs) |
| `Publish`: `commit_branch` | lane; one history write transaction | [`history/commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |
| `Install` | Workspace lock and base write lock, one job | [`install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs) |

## What it does

| Behaviour | Detail | Source |
| --- | --- | --- |
| What is captured | Every change a filesystem request already published to the overlay, from any process, whatever launched it and whether it still runs. No Exec, command or process identity enters capture; stdout and stderr are not captured unless written to a file in the Workspace | [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |
| Capture | `Overlay::capture` copies no rows: `active` G becomes `captured`, G+1 becomes active, `captured_revision` is frozen, dirty counts restart; `base_root` is unchanged until install | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| Capture readiness | Waits (lock released) while earlier reply attempts are unrecorded or a failed capture is still folding back; mutations arriving meanwhile queue behind the capture (`behind_frontier`), so the frontier is finite. A wait is readiness, not a retry | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs), [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs), [`state.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/state.rs) |
| Captured reads | `OverlayCapturedNamespace` and `OverlayCapturedRuns` over the `CapturedReader`: pages of at most `PAGE_ROWS` rows in key order, run windows of at most `READ_WINDOW`; later active writes cannot change them | [`captured_namespace.rs`](../../../../engine/crates/layerfs-workspace/src/ports/captured_namespace.rs), [`captured_namespace_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/captured_namespace_port.rs), [`captured_run_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/captured_run_port.rs) |
| No change, head unchanged | `UpToDate` with no capture, Save, lane or install: no dirty rows, no capture or consolidation, base root equals the bound root, no pending conflict, head and base Layer unchanged | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| Candidate equals the head's root | `commit_branch` answers `CommitOutcome::UpToDate`; capture, construction and Save ran and the base is still installed. No Commit record is created | [`history/commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |
| Head unchanged | `Committed` against the head the Workspace bound | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| Head moved, `force` false | `merge_candidate`: `merge_roots(base, ours, theirs)` in the Commit's own Save, before the lane; published against the head it merged with (`MergeReport`) | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| Head moved again before the lane | `merge_in_lane`: only what landed in between, with its own Save, at most `LANE_MERGE_PATHS` serials | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| Merge conflict | Nothing published; the merge with marker blocks becomes the local base, `PendingMerge` is kept and the history binding moves to the head. The lane is released when the Commit returns | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs), [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| `force` with a moved head | No merge; published over the head, the displaced head recorded | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs), [`history/commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |
| Relink (unlink and re-create with equal bytes) | A fresh chunked file whose overlay digest names an already saved root reuses it (`files_reused`); others are built and `record_file_digests` stores their digest. Files below the Store's `small_file_threshold_bytes` are rebuilt to the same object id | [`digests.rs`](../../../../engine/crates/layerfs-store-service/src/store/digests.rs), [`digest.rs`](../../../../engine/crates/layerfs-overlay/src/payload/digest.rs) |
| Install | `prepare_base_install` rebinds the base to the new root without a tree walk; after publish, `install_prepared_base` waits for `base_readers == 0`, then `Overlay::install` sets `base_root`, `installed := G`, `captured := None` and queues `RETIRE` of G, in one SQL transaction | [`install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs), [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| Rows written after the capture | Untouched by install; still the top of the effective view. An mtime-only change keeps the base row and carries its live row into G+1 (`keep_live_row`) | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs), [`inodes.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/inodes.rs) |
| Kernel caches after install | Own root installed: nothing sent, the visible tree is unchanged. A merge root: the daemon invalidates the entries `stale_after_install` names before it answers | [`invalidate.rs`](../../../../engine/crates/layerfs-daemon/src/control/invalidate.rs) |
| Cache admission | Objects the construction staged enter the shared object cache only in the `published` callback; every other path drops them | [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |
| SDK `file.*` writes | Same route through a private Workspace, `CommitVia::Files` | [`files.rs`](../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| Where a control Commit runs | On the daemon's control thread; the Commit spawns its one construction thread | [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |

A merge reserves fresh inode serials `MERGE_SERIALS` (64) at a time. Merge
rules and markers are in [Core: Compare and merge](../../core/compare-and-merge/README.md).
Dirty shared-mapping pages the kernel has not yet sent as WRITE are not
published, so they are outside the capture; a command's exit is not a flush
fence for mappings or descriptors it handed on.

[`layerfs-store-service/src/install.rs`](../../../../engine/crates/layerfs-store-service/src/install.rs)
is a different install, placing a sealed Store file into a project volume; see
[Project Init](../project-init/README.md).

### Custody and settle

```text
 capture --> acquire_captured_reader(capture, request = generation)
         --> acquire_operation(request) --> construct ... outcome known
         --> release reader, then operation owner --> answer --> reclaim

 failure --> published or uncertain? --yes--> keep all custody
               | no
             capture taken? --no--> locally_settled
               | yes
             resolve_failed(capture) --ok--> locally_settled
                                     --err-> local_error kept
```

| Owner | Released when | Source |
| --- | --- | --- |
| Admission flag | Success, or a `locally_settled` failure with no retained owner | [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |
| `CapturedReader`, then `OperationOwner` | After the outcome is known, in that order; nothing is released on `Drop` | [`captured_reader.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/captured_reader.rs), [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |
| Captured generation | Install, or `resolve_failed` after a definite nonpublication: it becomes the consolidating generation and maintenance folds its rows back under the active one; an active row for the same name or inode wins | [`composition.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/composition.rs) |
| Readied maintenance | Not drained by the releases; the daemon calls `BoundWorkspace::reclaim` after it sent the answer, else the Workspace's next job drains it | [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |

`CapturedConstruction` is the receipt: the reader or owner not known released,
the work counted and the release error that stopped the sequence
([`commit_types.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_types.rs)).
`CommitFailure` carries `phase`, `intent`, `capture`, `prepared`, `saved`,
`published` and `locally_settled`.

| Failure kind | Daemon activity | Next Commit | Source |
| --- | --- | --- | --- |
| Definite, settled, owners released | `Idle` | Admitted | [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| Published, then install or a release failed | `LocalFailure` | Refused: code `Unknown`, phase `admission`; the daemon keeps `published`, the SDK refusal does not carry it | [`registry.rs`](../../../../engine/crates/layerfs-daemon/src/control/registry.rs) |
| Uncertain (`CommitError::uncertain()`) | `Uncertain` | Refused as above | [`commit_types.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_types.rs), [`registry.rs`](../../../../engine/crates/layerfs-daemon/src/control/registry.rs) |

## Invariants

| Rule | Enforced by |
| --- | --- |
| One Commit at a time per Workspace; a second never queues an implicit capture | `committing` compare-exchange in `admit_commit`, else `CommitError::InFlight`: [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| One construction producer per Commit; one `Storage` handle and one Save on the Commit thread | [`producer.rs`](../../../../engine/crates/layerfs-store-service/src/store/producer.rs), [`pipeline.rs`](../../../../engine/crates/layerfs-store-service/src/store/pipeline.rs); see [Construction](construction.md) |
| Construction reads only the capture, never later active rows | `CapturedNamespace` binds the reader's root: [`driver.rs`](../../../../engine/crates/layerfs-workspace/src/construction/driver.rs) |
| No Workspace lock across construction or Store I/O | Every overlay call is one short owner job: [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) |
| Short holds: work that grows with the Commit (Save finish, logical count, binding hints, first merge, validation) runs before the lane | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| Content, Storage and Persistence are shared with Init; no Commit-only encoder | Objects go to `Save::sink`: [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |
| Install keeps later mutations and changes only base root, floor and captured state | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| Root inode identity never changes at install | `validate_candidate` refuses "candidate changes root inode identity": [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| A lost reply does not remove a published Commit | Publication is one history transaction; `published` is kept and blocks the next Commit: [`commit_types.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_types.rs) |
| No retry, replay or guessed outcome | `settle_failure` resolves only a definite nonpublication and never reads history to guess: [`settle.rs`](../../../../engine/crates/layerfs-store-service/src/store/settle.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Second Commit on the same Workspace | `CommitError::InFlight` (control code `Busy`) | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs), [`failure.rs`](../../../../engine/crates/layerfs-daemon/src/control/failure.rs) |
| A capture already held | `OverlayError::CaptureInFlight` | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| Captured page | `PAGE_ROWS` = 1,024 rows | [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) |
| Head moved again after the merge | Up to `LANE_MERGE_PATHS` = 1,024 serials merged in the lane, else `CommitError::LaneMerge { paths, limit }`; nothing published | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| Paths listed for a conflict | `WRITTEN_PATHS` = 4,096 (`written_truncated` when cut) | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| Marker blocks still present | `CommitError::Markers`; nothing published | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| Moved head without merge | `HeadMoved` from `commit_branch` unless `force` | [`history/commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |
| Base sources still open at install | Install waits; `OverlayError::BaseSourcesPending` if called unready | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| Install failed after publish | `OwnerError::Install` with the `PreparedBase`, never replayed; `CommitFailure.published` is `Some`; activity `LocalFailure` | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs), [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| Uncertain outcome | `CommitError::uncertain()` true, code `Unknown`; custody kept | [`commit_types.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_types.rs) |
| Concurrent Commits on other Workspaces | Correct, serialized by the publication lane; not the optimized case | [Store](../store/README.md) |

No total file, edit or Commit size cap exists: captured input is paged and
construction state lives in indexed operation records.

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`mounted_commit.rs`](../../../../engine/crates/layerfs-daemon/tests/mounted_commit.rs) | `cargo +1.85.1 test --locked -p layerfs-daemon --test mounted_commit` | Linux, `/dev/fuse` and mount privilege | Ordinary processes change every entry kind through a FUSE mount; the Commit read back from a fresh mount equals the tree the mount showed; `UpToDate`, five incremental Commits, shared-mapping writes |
| [`concurrent_commits.rs`](../../../../engine/crates/layerfs-daemon/tests/concurrent_commits.rs) | `cargo +1.85.1 test --locked -p layerfs-daemon --test concurrent_commits` | any (no kernel mount) | Stale Workspaces merge with the current head; one shared path gives one conflict |
| [`lane_merge.rs`](../../../../engine/crates/layerfs-daemon/tests/lane_merge.rs) | `cargo +1.85.1 test --locked -p layerfs-daemon --test lane_merge` | any | A Commit landing while another waits for the lane is merged in the lane; over the bound is `LaneMerge` |
| [`captured_namespace.rs`](../../../../engine/crates/layerfs-workspace/tests/captured_namespace.rs) | `cargo +1.85.1 test --locked -p layerfs-workspace --test captured_namespace` | any | One captured reader becomes one complete root; mutations after the capture land only in the next |
| [`mounted_install.rs`](../../../../engine/crates/layerfs-daemon/tests/mounted_install.rs) | `cargo +1.85.1 test --locked -p layerfs-daemon --test mounted_install` | Linux, `/dev/fuse` | Installing the Workspace's own root sends no notification; the mount reads the same through open descriptors, caches and `O_DIRECT` |

Run each under a wall timeout of at most 120 s.

## Pages

| Page | What it covers |
| --- | --- |
| [Construction](construction.md) | The captured namespace constructor, its operation records and outcome, and the one-producer pipeline into the Save. |
