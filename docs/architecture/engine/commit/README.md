# Commit

[Architecture](../../README.md) › [Engine](../README.md) › Commit

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Capturing a Workspace's published frontier, constructing it and publishing a Commit, then installing the new base.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-workspace`, `layerfs-store-service` |
| Source | [`engine/crates/layerfs-workspace/src/construction`](../../../../engine/crates/layerfs-workspace/src/construction), [`engine/crates/layerfs-store-service/src/store`](../../../../engine/crates/layerfs-store-service/src/store), [`engine/crates/layerfs-store-service/src/install.rs`](../../../../engine/crates/layerfs-store-service/src/install.rs) (Store-file install, see [Install](install.md)), [`engine/crates/layerfs-workspace/src/workspace/install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs), [`engine/crates/layerfs-daemon/src/control/operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |
| Uses | [Workspace](../workspace/README.md), [Overlay](../overlay/README.md), [Store calls](../store-calls/README.md), [Store](../store/README.md), [Core: Compare and merge](../../core/compare-and-merge/README.md) |
| Used by | [Daemon](../daemon/README.md) |
| Flows | [A Commit](../../flows/commit.md) |

Terms such as Capture, generation and Save are in the [glossary](../../glossary.md).

## Topics

| Topic | What it covers |
| --- | --- |
| [Capture](capture.md) | Capturing the published frontier. |
| [Construction](construction.md) | Constructing the captured namespace. |
| [Publish and merge](publish-and-merge.md) | Publishing a Commit, including after the head moved. |
| [Install](install.md) | Advancing the Workspace base after a Commit. |

## The picture

```text
 any process            Workspace overlay file        global Store
 (no Exec needed)       (one per Workspace)           (one writer)
      |                 +--------------------+
      | write, rename   | active generation  |
      +---------------->|  published rows    |
                        +---------+----------+
   Capture: seal the active       |
   generation, open a new one     v
                        +--------------------+
                        | captured generation|-- CapturedReader --+
                        +--------------------+                    |
                                                                  v
                     Construction: CapturedNamespace    +------------------+
                     (one producer, its own thread) --->| Save (objects)   |
                       bounded queue, 32 batches        | (Commit thread)  |
                                                        +--------+---------+
                     Publish and merge: head check,              |
                     merge, publish lane, commit_branch -------->| history row
                                                                  |
                     Install: overlay.install(capture, root)      |
                     base := new root; later rows stay  <---------+
```

| Step | Owner | Source |
| --- | --- | --- |
| Admission | `BoundWorkspace::admit_commit` (one Commit per Workspace) | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| [Capture](capture.md) | `Overlay::capture`, through `OwnerClient::capture` | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| [Construction](construction.md) | `CapturedNamespace::construct` | [`driver.rs`](../../../../engine/crates/layerfs-workspace/src/construction/driver.rs) |
| [Publish and merge](publish-and-merge.md) | `BoundWorkspace::commit_admitted` | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| [Install](install.md) | `Workspace::install_prepared_base`, `Overlay::install` | [`install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs) |

## What it does

| Case | Result | Source |
| --- | --- | --- |
| Changes published by any process | All are captured, whatever launched the process and whether it still runs | [`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs) |
| No changes, head unchanged | `UpToDate` with no capture, Save, lane or install | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| Candidate root equals the head's root | `CommitOutcome::UpToDate`; the base is still installed | [`commit.rs` (persistence)](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |
| Head unchanged | `Committed` against the head the Workspace bound | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| Head moved, no conflict | Merged in the same Save, published against the new head (`MergeCommitted`) | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| Head moved, conflict | Nothing published; the merge with markers becomes the local base (`Conflict`) | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| `force` with a moved head | Published over the head; the displaced head is recorded | [`commit.rs` (persistence)](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |
| SDK `file.*` writes | Same route through a private Workspace, `CommitVia::Files` | [`files.rs`](../../../../engine/crates/layerfs-store-service/src/write/files.rs) |

A Commit captures the Workspace's shared published frontier: every change that
a filesystem request has already published to the overlay. Commit has no
notion of Exec, command or process; nothing is filtered by who made a change.
A control Commit runs on the daemon's control thread
([`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs)),
which spawns the Commit's one construction thread
([Construction: One producer](construction.md#one-producer)).

## Invariants

| Rule | Enforced by |
| --- | --- |
| One Commit at a time per Workspace | `committing` flag in `admit_commit`, else `CommitError::InFlight` ([`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs)) |
| One construction producer per Commit | One `Storage` handle from `Producer::take` and one Save, both on the Commit thread; files constructed one at a time, in order, on one producer thread that hands objects to the Save through one bounded queue (owner approval, 2026-11-07) ([`producer.rs`](../../../../engine/crates/layerfs-store-service/src/store/producer.rs), [`pipeline.rs`](../../../../engine/crates/layerfs-store-service/src/store/pipeline.rs), [`normalize.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/normalize.rs)) |
| No Workspace lock across construction or Store I/O | Each overlay call is one short job ([`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs)); doc of `BoundWorkspace::commit` |
| Content, Storage and Persistence are shared with Init | Objects go to `Save::sink`; no Commit-only encoder ([`captured.rs`](../../../../engine/crates/layerfs-store-service/src/store/captured.rs)) |
| Construction reads only the capture, never later rows | `CapturedNamespace` binds the reader's root ([`driver.rs`](../../../../engine/crates/layerfs-workspace/src/construction/driver.rs)) |
| Install keeps later mutations | `Overlay::install` changes only base root, floor and captured state ([`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs)) |
| A lost reply does not remove a published Commit | Publication is one history transaction; a later failure keeps `published` in the daemon and blocks the next Commit, which is refused with code `Unknown`, phase `admission`; the SDK refusal does not return `published` ([`commit_types.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_types.rs), [`registry.rs`](../../../../engine/crates/layerfs-daemon/src/control/registry.rs)) |
| No retry or replay | `settle_failure` resolves only a definite nonpublication ([`settle.rs`](../../../../engine/crates/layerfs-store-service/src/store/settle.rs)) |

`LAYERFS_CONSTRUCTION_WORKERS` is not read by any engine source file; the
single producer holds by construction, as the
[`r5_10_producer_profile.rs`](../../../../engine/crates/layerfs-daemon/tests/r5_10_producer_profile.rs)
header records. Measurement runs still export `LAYERFS_CONSTRUCTION_WORKERS=1`.
Project Init keeps its own parallel profile ([Project Init: Construction](../project-init/construction.md)).

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Second Commit on the same Workspace | `CommitError::InFlight` (control code `Busy`) | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs), [`failure.rs`](../../../../engine/crates/layerfs-daemon/src/control/failure.rs) |
| Captured page and record window | `PAGE_ROWS` = 1,024 rows | [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) |
| Moved again after the merge | In-lane merge up to `LANE_MERGE_PATHS` = 1,024 serials, else `CommitError::LaneMerge` | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| Paths listed for a conflict | 4,096 (`written_truncated` when cut) | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| Conflict markers still present | `CommitError::Markers`; nothing published | [`commit_merge.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_merge.rs) |
| Concurrent Commits on other Workspaces | Correct; serialized by the publication lane, not the optimized case | [Store](../store/README.md) |
| Uncertain outcome | `CommitError::uncertain()` true, control code `Unknown`; custody kept, the Workspace refuses the next Commit | [`commit_types.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit_types.rs), [`registry.rs`](../../../../engine/crates/layerfs-daemon/src/control/registry.rs) |
| Published, install failed | `CommitFailure.published` is `Some`; activity `LocalFailure` | [`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs) |

No total file, edit or Commit size cap exists: captured input is paged and
construction state is kept in indexed operation records.

## Demonstration

Commands were written from source and were not run while writing this page.

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`mounted_commit.rs`](../../../../engine/crates/layerfs-daemon/tests/mounted_commit.rs) | `layerfs-daemon` | Changes made by external processes through a FUSE mount are committed and read back on a fresh mount |
| [`concurrent_commits.rs`](../../../../engine/crates/layerfs-daemon/tests/concurrent_commits.rs) | `layerfs-daemon` | Stale Workspaces merge with the current head; one shared path gives one conflict |
| [`captured_namespace.rs`](../../../../engine/crates/layerfs-workspace/tests/captured_namespace.rs) | `layerfs-workspace` | One captured reader becomes one complete root, compared with a model |
| [`relink_commit.rs`](../../../../engine/crates/layerfs-store-service/tests/relink_commit.rs) | `layerfs-store-service` | Unlink and recreate with equal bytes reuses saved file roots |
| [`commit_pipeline.rs`](../../../../engine/crates/layerfs-store-service/tests/commit_pipeline.rs) | `layerfs-store-service` | The two-thread Commit gives the single-thread roots and stored bytes; a refused Save write stops the producer; a failed construction abandons the Save |

```text
mounted_commit:  bash in mount --> write/rename/unlink --> control Commit
                 --> fresh bind + fresh mount --> tree == tree seen before
```

- **`mounted_commit.rs`** sets up an installed Store and a real kernel mount,
  runs ordinary processes that change every entry kind, commits, then compares
  each published root, read from a fresh bind and a fresh mount, with the tree
  the mount showed. It also checks `UpToDate`, five incremental Commits and
  writes through shared mappings. Command:
  `cargo +1.85.1 test --locked -p layerfs-daemon --test mounted_commit`.
  Needs Linux (`#![cfg(target_os = "linux")]`); it mounts FUSE with `mount(2)`
  ([`syscalls.rs`](../../../../engine/crates/layerfs-fuse/src/mount/syscalls.rs)),
  so it needs `/dev/fuse` and mount privilege.
- **`concurrent_commits.rs`** binds several Workspaces through the control
  service, writes through overlay jobs (no kernel mount) and starts their
  Commits together on barriers. Different files all publish; one shared file
  gives one conflict. Command:
  `cargo +1.85.1 test --locked -p layerfs-daemon --test concurrent_commits`.
  No `cfg` gate and no `#[ignore]`.
- **`captured_namespace.rs`** drives `CapturedNamespace` over a test overlay
  and checks creates, deletes, renames, mode and time changes, and that
  mutations after the capture land only in the next one. Command:
  `cargo +1.85.1 test --locked -p layerfs-workspace --test captured_namespace`.
  No `cfg` gate.
- **`relink_commit.rs`** is `#[ignore]` and needs a replay Store:
  `LAYERFS_364_RELINK_STORE`, `LAYERFS_364_RELINK_BINDING` and
  `LAYERFS_364_RELINK_FROM`. Command:
  `cargo +1.85.1 test --locked -p layerfs-store-service --test relink_commit -- --ignored`.
- **`commit_pipeline.rs`** commits the same changes to two copies of one
  installed Store, one through `commit_captured` and one with the same
  constructor on the calling thread, and compares roots and Save outcomes for
  a 12 MiB and a small Commit. Its failure cases hold the Store's write lock
  from the start of the `construct` phase. Command:
  `cargo +1.85.1 test --locked -p layerfs-store-service --test commit_pipeline`.
  No `cfg` gate.

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`core/docs/architecture/26-capture-custody.md`](../../../../core/docs/architecture/26-capture-custody.md) — covered in [Capture](capture.md).
- [`core/docs/architecture/27-prepared-base-install.md`](../../../../core/docs/architecture/27-prepared-base-install.md) — covered in [Install](install.md).
- [`core/docs/architecture/65-store-commit-composition.md`](../../../../core/docs/architecture/65-store-commit-composition.md) — covered in [Publish and merge](publish-and-merge.md); its staging API is removed (historical only).
- [`core/docs/architecture/78-captured-namespace-construction.md`](../../../../core/docs/architecture/78-captured-namespace-construction.md) — covered in [Construction](construction.md); its "no product caller" note is historical.
- [`core/docs/architecture/79-product-commit.md`](../../../../core/docs/architecture/79-product-commit.md) — covered in [Capture](capture.md) and [Construction](construction.md).

---

← [Workspace: State](../workspace/state.md) · [Up: Engine](../README.md) · [Commit: Capture](capture.md) →
