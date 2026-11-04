# Install

[Architecture](../../README.md) › [Engine](../README.md) › [Commit](README.md) › Install

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Advancing the Workspace base after a Commit.

| | |
| --- | --- |
| Component | [Commit](README.md) |
| Source | [`engine/crates/layerfs-store-service/src/install.rs`](../../../../engine/crates/layerfs-store-service/src/install.rs), [`engine/crates/layerfs-workspace/src/workspace/install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs), [`engine/crates/layerfs-overlay/src/lifetime/generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs), [`engine/crates/layerfs-store-service/src/overlay/lifetime_operations.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/lifetime_operations.rs) |

The Workspace base install is in
[`workspace/install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs)
and [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs).
[`layerfs-store-service/src/install.rs`](../../../../engine/crates/layerfs-store-service/src/install.rs)
is a different install: placing a sealed Store file into a project volume
(`install_local`). It is described in
[Project Init: Seal and install](../project-init/seal-and-install.md).

## Advancing the base

```text
 inside the lane (Validate)              after publish (Install)
 -------------------------------         ---------------------------------
 checked_base(next snapshot)             install_prepared job
 prepare_base_install(capture, root)       ready: base_readers == 0
   capture.base_root == base?              Workspace lock
   base.rebind(root)  -> PreparedBase      base write lock
   (no lock, no tree walk)                   identity == expected?
                                             Overlay::install(capture,root)
                                             base := next (original client)
                                         snapshot := next; pending := merge
```

| Step | Checks or changes | Lock | Source |
| --- | --- | --- | --- |
| `checked_base` | Opens the candidate root against the next Branch snapshot; root inode identity must not change | none | [`bind.rs`](../../../../engine/crates/layerfs-store-service/src/store/bind.rs), [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| `prepare_base_install` | Capture engine and base root match; `BaseView::rebind(root)` | none | [`install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs) |
| Install readiness | `install_ready`: no pending base-source windows (`base_readers == 0`); later source acquisitions wait behind it | Workspace lock, released while waiting | [`lifetime_operations.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/lifetime_operations.rs), [`state.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/state.rs) |
| `install_prepared_base` | Base identity still the expected one | Workspace lock and base write lock | [`install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs) |
| `Overlay::install` | `base_root := root`, `installed := capture.generation`, `captured := None`; queues `RETIRE` of that generation | same, one SQL transaction | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| Binding update | `snapshot := next`; `pending` set for a conflicted merge, else cleared | `BoundWorkspace` mutexes | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |

Install runs only after a known publication or a history `UpToDate`, or after
a conflicted merge that publishes nothing. No fallible work follows the SQL
success. A failed install returns `OwnerError::Install` with its cause and
the `PreparedBase`, never replayed; the Commit's `published` value is kept and
the daemon marks the Workspace `LocalFailure`
([`owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs),
[`operations.rs`](../../../../engine/crates/layerfs-daemon/src/control/operations.rs)).

## Later changes during install

```text
 before install                       after install
 +---------------------------+        +---------------------------+
 | base root R               |        | base root R' (published)  |
 | installed floor F         |        | installed floor G         |
 | captured G   (sealed)     |        | G retired by maintenance  |
 | active  G+1  later writes |        | active  G+1  later writes |
 +---------------------------+        +---------------------------+
 effective view = base + rows above the floor  (unchanged for G+1)
```

| What | After install | Source |
| --- | --- | --- |
| Rows written after the capture (generation G+1) | Untouched; still the top of the effective view | [`generation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/generation.rs) |
| Rows of the captured generation | At or below the floor; never enter the view; reclaimed by `RETIRE` | [`types.rs`](../../../../engine/crates/layerfs-overlay/src/contract/types.rs) |
| Inodes whose only change was mtime | Live row carried into G+1 (`keep_live_row`), so the mount keeps showing its own time | [`inodes.rs`](../../../../engine/crates/layerfs-workspace/src/construction/namespace/inodes.rs) |
| Immutable plans made before install | Keep their own client and root; they do not see the pointer change | [`install.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/install.rs) |
| Kernel caches, own root installed | No notification: the tree the mount shows is unchanged | [`mounted_install.rs`](../../../../engine/crates/layerfs-daemon/tests/mounted_install.rs) |
| Kernel caches, merge root installed | Invalidated before the reply (`stale_after_install`) | [`invalidate.rs`](../../../../engine/crates/layerfs-daemon/src/control/invalidate.rs) |

Without a merge, the installed root is the root built from this exact capture,
so base plus the later rows shows the same files as before install. A merge
root differs, which is why the daemon invalidates the kernel entries it changed. The installed floor and
retirement are described in
[Overlay: Lifetimes and reclamation](../overlay/lifetimes-and-reclamation.md).

## Related

- [Publish and merge](publish-and-merge.md): what must be known before install.
- [Capture](capture.md): the generation install ends.
- [Workspace: State](../workspace/state.md), [Workspace: Base](../workspace/base.md): the base binding being replaced.
- [Kernel routes: Coherence](../kernel-routes/coherence.md): kernel cache rules after install.

---

← [Commit: Publish and merge](publish-and-merge.md) · [Up: Commit](README.md) · [Engine: Kernel routes](../kernel-routes/README.md) →
