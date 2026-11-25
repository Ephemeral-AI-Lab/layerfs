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
`live`, `publish`, `install`; a merge adds `merge.changed`, an in-lane
step `merge.lane.changed`, and a fused Commit `publish.fused`).

A Commit over its bound head is fused (#768 stage 2,
[`commit_fused.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_fused.rs)):
step 3 reads no head and `Save::finish_prepared` holds the Save's last
publication (the Commit's own reads until then, the logical count and the
binding hints, see it whole: the Save's Storage handle serves it from the
batch itself, `read/held.rs`); after validation, that publication, the binding hints and
`commit_branch` are one write transaction (`layerfs_persistence::Committer`:
one `BEGIN IMMEDIATE` on SQLite, one pipelined message on PostgreSQL, see
[Fused Commit](../store/postgresql.md#fused-commit)), which asserts the
bound head. A definite race refusal (the head moved, another writer stored
one of the publication's objects, the Commit identity exists) wrote
nothing: the held publication and the hints are published on their own
(phase `Finish`, no intent: a lost reply there folds back as a failed
`finish` does) and the Commit continues from step 3 in the same call, once:
the head read and, if it moved, the merge before the lane in a Save of its
own, then step 4. An unknown outcome never continues, and neither does a
race refusal whose session lost a later reply (a definite `Conflict`). A
Commit whose new bindings pass one hint transaction (4,096 rows) publishes
its tail, then its hints, then attempts the Commit alone fused. Only a Commit that is not forced, names
no other `--expected` head, and whose construction kept its whole built set
(`ConstructedObjects::covers`) is fused; every other Commit takes steps 3
and 4 as drawn.

| Phase (`CommitPhase`) | Lock held | Source |
| --- | --- | --- |
| `Admission` | `committing` flag, whole Commit | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| `Capture`: UpToDate check, capture | Workspace lock, one short job each | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| `Begin`: take the producer `Storage`, `begin_save` | none | [`producer.rs`](../../../../engine/crates/layerfs-store-service/src/store/producer.rs) |
| `Construct`: captured namespace, head read, merge | Workspace lock per job only | [Construction](construction.md), [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| `Finish`: `Save::finish`, logical size, binding hints, then the lane | none, then the publication lane | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs), [`hints.rs`](../../../../engine/crates/layerfs-store-service/src/store/hints.rs) |
| `Validate`: `checked_base`, `prepare_base_install`, from the objects construction built, else through the Workspace reader; the install gets the ordinary client | none (before the lane); again in the lane only after an in-lane merge changed the candidate | [`bind.rs`](../../../../engine/crates/layerfs-store-service/src/store/bind.rs), [`install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs) |
| `Publish`: `commit_branch` | lane; one history write transaction | [`history/commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |
| `Publish`, fused: held publication, hints and Commit | one write transaction that takes the lane row (PostgreSQL) or the writer lock (SQLite) itself | [`commit_fused.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_fused.rs), [`history/fused.rs`](../../../../engine/crates/layerfs-persistence/src/history/fused.rs) |
| `Install` | Workspace lock and base write lock, one job | [`install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs) |

## What it does

| Behaviour | Detail | Source |
| --- | --- | --- |
| What is captured | Every change a filesystem request already published to the overlay, from any process, whatever launched it and whether it still runs. No Exec, command or process identity enters capture; stdout and stderr are not captured unless written to a file in the Workspace | [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |
| Capture | `Overlay::capture` copies no rows: `active` G becomes `captured`, G+1 becomes active, `captured_revision` is frozen, dirty counts restart; `base_root` is unchanged until install | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| Capture readiness | Waits (lock released) while earlier reply attempts are unrecorded or a failed capture is still folding back; mutations arriving meanwhile queue behind the capture (`behind_frontier`), so the frontier is finite. A wait is readiness, not a retry | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs), [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs), [`state.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/state.rs) |
| Captured reads | `OverlayCapturedNamespace` and `OverlayCapturedRuns` over the `CapturedReader`: pages of at most `PAGE_ROWS` rows in key order, run windows of at most `READ_WINDOW`; later active writes cannot change them | [`captured_namespace.rs`](../../../../engine/crates/layerfs-workspace/src/ports/captured_namespace.rs), [`captured_namespace_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/captured_namespace_port.rs), [`captured_run_port.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/captured_run_port.rs) |
| No change, head unchanged | `UpToDate` with no capture, Save, lane or install: no dirty rows, no capture or consolidation, base root equals the bound root, no pending conflict, head and base Layer unchanged; with `expected`, another head is `HeadMoved` here too | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| Candidate equals the head's root | `commit_branch` answers `CommitOutcome::UpToDate`; capture, construction and Save ran and the base is still installed. No Commit record is created | [`history/commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |
| Head unchanged | `Committed` against the head the Workspace bound | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| Head moved, `force` false | `merge_candidate`: `merge_roots(base, ours, theirs)` in the Commit's own Save, before the lane; published against the head it merged with (`MergeReport`) | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| Head moved again before the lane | `merge_in_lane`: only what landed in between, with its own Save, at most `LANE_MERGE_PATHS` serials | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| Merge conflict | Nothing published; the merge with marker blocks becomes the local base, `PendingMerge` is kept and the history binding moves to the head. A conflict found before the lane never takes it; one found by the in-lane step releases it before validation and install. Only a publication holds the lane | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs), [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| `force` with a moved head | No merge, and no head read decides it before the lane: the publication's own transaction, inside the lane, reads the head and publishes over it, recording it as `displaced` (the Commits before it stay on its first-parent chain) |
| `expected` (`--expected`) | `CommitOptions::expected`, the node the Branch must answer as its head, is checked when the head is read (another head: `HeadMoved` before any merge) and travels in the publish intent as `CommitRequest::required_head`, checked again inside `commit_branch`'s transaction. A head that moves again before the lane is not merged with; the publication refuses it. With `force`, only that head is displaced | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs), [`history/commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |
| Relink (unlink and re-create with equal bytes) | A fresh chunked file whose overlay digest names an already saved root reuses it (`files_reused`); others are built and `record_file_digests` stores their digest. Files below the Store's `small_file_threshold_bytes` are rebuilt to the same object id | [`digests.rs`](../../../../engine/crates/layerfs-store-service/src/store/digests.rs), [`digest.rs`](../../../../engine/crates/layerfs-overlay/src/payload/digest.rs) |
| Install | `prepare_base_install` rebinds the base to the new root without a tree walk; after publish, `install_prepared_base` waits for `base_readers == 0`, then `Overlay::install` sets `base_root`, `installed := G`, `captured := None` and queues `RETIRE` of G, in one SQL transaction | [`install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs), [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| Rows written after the capture | Untouched by the install of the Workspace's own root; still the top of the effective view. An mtime-only change keeps the base row and carries its live row into G+1 (`keep_live_row`); until a write, that row is not dirty and `active_inodes` (Dirty check, `workspace status`) leaves it out |
| Rows written after the capture, merged install | A G+1 row holds absolute facts (size, inherited cutoff, link and entry counts, inherited bits) computed against the old base, so a merge must not change what it covers. Before publishing a merge (or installing a conflicted one) the Commit takes a `MutationHold` (the Workspace's mutations wait; reads and other jobs run), lists the serials whose value differs between its candidate and the merged root (`changed_serials`, the in-lane step's added), and reads the active rows: an inode row of a changed serial, or a name binding under a changed directory, refuses the Commit `LiveConflict { paths }` (code `Busy`). Nothing is published, the capture folds back and the next Commit takes those rows too. Without overlap the hold lasts through publication and install. A carried mtime-only row is no overlap; `install_merged` deletes it when its serial changed (its live mtime is never committed), so the view and a later `UpToDate` agree with the head. Rows of unchanged serials stay valid: the merged root holds the same value for them | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs), [`inodes.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/inodes.rs) |
| Kernel caches after install | Own root installed: nothing sent, the visible tree is unchanged. A merge root: the visible tree changes where the merge changed it (no row written after the capture covers that); the daemon invalidates the entries `stale_after_install` names before it answers | [`invalidate.rs`](../../../../engine/crates/layerfs-daemon/src/control/invalidate.rs) |
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

 failure --> published, or CommitFailure::uncertain()? --yes--> keep all custody
               | no      (before Publish only local uncertainty counts)
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
`published`, `locally_settled` and, for a merge, `merged_over` (its own root and the serials the merge changed).

| Failure kind | Daemon activity | Next Commit | Source |
| --- | --- | --- | --- |
| Definite, settled, owners released | `Idle` | Admitted | [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| Published, then install or a release failed | `LocalFailure` | Its one resolution first (below); unresolved, refused: code `Unknown`, phase `admission`; the daemon keeps `published`, the SDK refusal does not carry it | [`resolve.rs`](../../../../engine/crates/layerfs-daemon/src/control/resolve.rs), [`registry.rs`](../../../../engine/crates/layerfs-daemon/src/control/registry.rs) |
| Uncertain (`CommitFailure::uncertain()`) | `Uncertain` | As above | [`commit_types.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_types.rs), [`registry.rs`](../../../../engine/crates/layerfs-daemon/src/control/registry.rs) |

`CommitFailure::uncertain()` depends on the phase. `commit_branch` is the
only write that publishes, so before the `Publish` phase nothing in history
can have changed: a Store, Content or History failure there (a merge-stage
Save read, a logical count, binding hints, removals, an unknown Storage
error from `Save::finish`) is a definite nonpublication, folded back with
`resolve_failed`, and the reply's code is `Failed`, not `Unknown`. Only local
uncertainty keeps custody there (`CommitError::locally_uncertain()`: an
uncertain or quarantined overlay, an uncertain owner job, a poisoned binding
or port). From the `Publish` phase until the answer is known,
`CommitError::uncertain()` decides. A merge-stage failure reports the Save's
own failure (`Save::take_failure`) as its first cause when there is one.

### One resolution of retained custody

A Commit whose publication answered with an unknown outcome (phase
`Publish`, nothing known published), or whose install failed definitely after
a known publication (phase `Install`), keeps a record beside its custody: the
capture, the intent, the binding, the known publication, the merge's own root
and changed serials, and the captured reader and operation owner
([`resolve.rs`](../../../../engine/crates/layerfs-store-service/src/store/resolve.rs)).
Local uncertainty (an uncertain overlay or owner job, a poisoned binding) and
a namespace receipt with unreleased custody or a release error keep no
record: their custody stays until force unmount, as before.

The Workspace's next Commit request runs `BoundWorkspace::resolve_retained`
once, before its own admission (daemon
[`resolve.rs`](../../../../engine/crates/layerfs-daemon/src/control/resolve.rs),
activity `Committing`, phase `resolve`). It is not a retry: nothing is
resent. An attempt that determines the publication's outcome uses the record
whatever its local step finds; one that cannot determine it puts the record
back, so the next request reads again instead of the Workspace refusing every
Commit with nothing left to resolve.

Before history is read the attempt passes the Store's publication lane
(`HistoryCatalog::pass_publication_lane`). On PostgreSQL a COMMIT whose reply
was lost may still be applying on the server; a non-nested publication holds
the lane row and a nested one the barrier key until its transaction ends, so
a fresh connection that takes the lane row and then the barrier exclusively,
both under `lock_timeout`, and rolls back, proves the original has ended. A
wait that reaches the timeout is `Busy`, an undetermined attempt. On SQLite
the publication ran in this process and the pass returns at once.

| History read through the control reader | Step | Then |
| --- | --- | --- |
| Publication already known | Install its root (same validation, live check and mutation hold as a merged install) | Custody released, `Idle`; the Commit is admitted |
| Head and base still the ones we published against, with another root | Nothing was published: `resolve_failed` folds the capture back | As above |
| Still that head, with our root | `UpToDate`: install it | As above |
| Moved head whose first-parent chain holds our Commit (our candidate root, parent and base Layer equal to the head and base we published against), or a forced Commit that displaced it | Install it, as above | As above |
| Moved head whose first-parent chain reaches the head we published against through Commits all made on this Branch, displacing nothing, over the same base | Nothing was published: fold back, as above | As above |
| A forced Commit displacing another head, a Commit from another Branch (a merge fast-forward), a moved base, more than 1,024 Commits, a failed read, or a lane that does not pass | None; the record is put back | Custody stays as it was; the Commit is refused `Unknown` with the reason |
| A failed install or fold-back after a determined outcome | None | Custody stays (`LocalFailure` once a publication is known); the Commit is refused `Unknown` with the reason |

Every head move keeps the previous head on the new head's first-parent chain
(an advance's parent is the head it read), except a forced Commit, which
records the head it displaced, and a merge fast-forward, which brings in
another Branch's Commits; the walk stops undetermined at either rather than
guess. History is read on the control reader's own connection, so a writer
session that the unknown outcome quarantined does not stop it. A known
install after a merge with overlapping live rows is refused like a
Commit's own (`LiveConflict` in the reason), and the custody stays.

## Invariants

| Rule | Enforced by |
| --- | --- |
| One Commit at a time per Workspace; a second never queues an implicit capture | `committing` compare-exchange in `admit_commit`, else `CommitError::InFlight`: [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| One construction producer per Commit; one `Storage` handle and one Save on the Commit thread | [`producer.rs`](../../../../engine/crates/layerfs-store-service/src/store/producer.rs), [`pipeline.rs`](../../../../engine/crates/layerfs-store-service/src/store/pipeline.rs); see [Construction](construction.md) |
| Construction reads only the capture, never later active rows | `CapturedNamespace` binds the reader's root: [`driver.rs`](../../../../engine/crates/layerfs-workspace/src/construction/driver.rs) |
| No Workspace lock across construction or Store I/O | Every overlay call is one short owner job: [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs) |
| Short holds: work that grows with the Commit (Save finish, logical count, binding hints, first merge, validation) runs before the lane. A fused Commit writes its last publication (within the publication bounds) and its hint rows inside its one transaction; they were built before it | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| A fused attempt is made once; a race refusal continues on the separate path once, never resending the message; an unknown outcome or an epoch `Conflict` has no fallback | [`commit_fused.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_fused.rs) |
| Content, Storage and Persistence are shared with Init; no Commit-only encoder | Objects go to `Save::sink`: [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |
| Install keeps later mutations and changes only base root, floor and captured state; a merged install also deletes carried mtime-only rows of serials the merge changed, and is refused while any other later row overlaps what it changed | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
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
| Marker blocks still present | `CommitError::Markers`; nothing published |
| Rows written after the capture overlap a merge | `CommitError::LiveConflict { paths }`, at most `LIVE_CONFLICT_PATHS` = 16 named; nothing published; code `Busy` | [`live.rs`](../../../../engine/crates/layerfs-store-service/src/store/live.rs) | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| Moved head without merge | `HeadMoved` from `commit_branch` unless `force` | [`history/commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |
| Base sources still open at install | Install waits; `OverlayError::BaseSourcesPending` if called unready | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| Install failed after publish | `OwnerError::Install` with the `PreparedBase`, never replayed; `CommitFailure.published` is `Some`; activity `LocalFailure` | [`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs), [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| Uncertain outcome | `CommitFailure::uncertain()` true, code `Unknown`; custody kept | [`commit_types.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_types.rs) |
| Concurrent Commits on other Workspaces | Correct, serialized by the publication lane; not the optimized case | [Store](../store/README.md) |
| A fused request too large for one message | `Fused::Overflow`, nothing sent: the tail and the hints are published first, then the Commit alone is attempted fused | [`commit_fused.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_fused.rs) |
| New bindings past one hint transaction (4,096 rows) | The tail is published, then every binding recorded, then the Commit alone is attempted fused; no hint row precedes its objects | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs), [`hints.rs`](../../../../engine/crates/layerfs-store-service/src/store/hints.rs) |

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
| [`commit_settle.rs`](../../../../engine/crates/layerfs-store-service/tests/commit_settle.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test commit_settle` | any | A failure before publication folds back; local uncertainty keeps custody; a conflicted merge takes no lane |
| [`commit_expected.rs`](../../../../engine/crates/layerfs-store-service/tests/commit_expected.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test commit_expected` | any | `--expected` is checked inside the publication and by a clean Workspace's `UpToDate`; `--force` publishes over the head read in the lane |
| [`commit_live_merge.rs`](../../../../engine/crates/layerfs-store-service/tests/commit_live_merge.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test commit_live_merge` | any | A merged install that overlaps rows written after the capture is refused `LiveConflict`; one with no overlap publishes |
| [`fused_commit.rs`](../../../../engine/crates/layerfs-persistence/tests/fused_commit.rs) | `LAYERFS_TEST_POSTGRES_URL=… cargo +1.85.1 test --locked -p layerfs-persistence --test fused_commit` | any; PostgreSQL cases need the URL | One transaction per fused Commit (one round trip warm on PostgreSQL); races, `StaleStore`, `Busy`, `Conflict` and a lost reply, each writing nothing |
| [`mount_round_trips.rs`](../../../../engine/crates/layerfs-store-service/tests/mount_round_trips.rs) | `LAYERFS_TEST_POSTGRES_URL=… cargo +1.85.1 test --locked -p layerfs-store-service --test mount_round_trips` | any | Pinned per-session work of fused Commits; a merge Commit is one race refusal and one fallback |
| [`store_commit.rs`](../../../../engine/crates/layerfs-daemon/tests/store_commit.rs) | `cargo +1.85.1 test --locked -p layerfs-daemon --test store_commit` | any | Custody and settle by phase; one resolution of an unknown outcome installs a landed Commit (also under a later one), folds a lost one back (also under a moved head) and keeps the record when a forced Commit leaves it undetermined |

Run each under a wall timeout of at most 120 s.

## Pages

| Page | What it covers |
| --- | --- |
| [Construction](construction.md) | The captured namespace constructor, its operation records and outcome, and the one-producer pipeline into the Save. |
