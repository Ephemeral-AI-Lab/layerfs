# Workspace

[Architecture](../../README.md) › [Engine](../README.md) › Workspace

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Filesystem semantics of a mounted Workspace: the committed base, overlay changes and the effective view.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-workspace` |
| Source | [`engine/crates/layerfs-workspace/src/base`](../../../../engine/crates/layerfs-workspace/src/base), [`engine/crates/layerfs-workspace/src/operations`](../../../../engine/crates/layerfs-workspace/src/operations), [`engine/crates/layerfs-workspace/src/mutation`](../../../../engine/crates/layerfs-workspace/src/mutation), [`engine/crates/layerfs-workspace/src/workspace`](../../../../engine/crates/layerfs-workspace/src/workspace), [`engine/crates/layerfs-workspace/src/ports`](../../../../engine/crates/layerfs-workspace/src/ports), [`lib.rs`](../../../../engine/crates/layerfs-workspace/src/lib.rs) |
| Uses | [Overlay](../overlay/README.md), [Core: Filesystem trees](../../core/filesystem-trees/README.md), [Core: Files](../../core/files/README.md) |
| Used by | [Store calls](../store-calls/README.md), [Commit](../commit/README.md), [Kernel routes](../kernel-routes/README.md), [Daemon](../daemon/README.md) |
| Flows | [A Workspace session](../../flows/workspace.md), [A Commit](../../flows/commit.md) |

A **Workspace** is a complete mutable filesystem view over one immutable
committed root. Terms such as *overlay*, *generation* and *whiteout* are in the
[glossary](../../glossary.md). The crate's `construction/` folder builds a
Commit from a captured Workspace; it is described under
[Commit](../commit/README.md), not here.

## Topics

| Topic | What it covers |
| --- | --- |
| [Base](base.md) | The committed base a Workspace reads through. |
| [Operations](operations.md) | Filesystem operations on a Workspace. |
| [Mutation](mutation.md) | Planning and running changes. |
| [Ports](ports.md) | What kernel routes and Commit call into. |
| [State](state.md) | Workspace state, serials, route and view. |

## The picture

```text
  any process that can see the mount (Exec, shell, external executor)
        | ordinary syscalls                    SDK file.* writes
        v                                             |
  +--------------+                                    v
  | Kernel route |                           +-------------+
  | FUSE / NFS   |                           | Store calls |
  +--------------+                           +-------------+
        | kernel visits                             | MutationPlan
        v                                           v
  +-------------------------------------------------------------+
  | Workspace                                                   |
  |   State: Route, base cell (BaseView), Serials               |
  |                                                             |
  |   Operations ----------------> Mutation                     |
  |   lookup, stat, list, read     plan -> NamespaceJob rounds  |
  |        |                                |                   |
  |        v                                v                   |
  |   Base: BaseView,            Ports: OverlayRead,            |
  |   CanonicalClient            OverlayJobs, OverlayFileRead   |
  +--------|-------------------------------|--------------------+
           | demand reads, outside         | one short job under
           | every overlay lock            | the Workspace's lock
           v                               v
  +----------------------+      +-----------------------------+
  | global Store         |      | Workspace overlay file      |
  | immutable objects    |      | changed names, inodes,      |
  | (committed base)     |      | payload cells, records      |
  +----------------------+      +-----------------------------+
```

The two arrows at the bottom never overlap: base I/O happens outside the
overlay lock, and an overlay job does no Store I/O
([`ports/overlay.rs`](../../../../engine/crates/layerfs-workspace/src/ports/overlay.rs)).

## What it does

### The effective view

```text
   effective view  =  overlay rows  over  committed base
  +---------------------------------------------------------+
  | active generation   names, inodes, cells (newest)       |
  +---------------------------------------------------------+
  | lower generation    captured rows not yet installed     |
  +---------------------------------------------------------+
  | committed base      immutable root in the Store         |
  +---------------------------------------------------------+
   A row in a higher layer decides; a whiteout row hides the
   name below it; no row falls through to the next layer.
```

| Caller can rely on | How | Source |
| --- | --- | --- |
| A process sees the whole tree: `.git` and its index, ignored files, dependencies, symlinks, caches and output | The base is a complete canonical root; no Workspace code filters names by Git rules or any pattern | [`base/view.rs`](../../../../engine/crates/layerfs-workspace/src/base/view.rs) |
| Mount binds the root without a scan or copy | `BaseView::open` acquires the root object and checks its scope; nothing is listed or copied | [`base/view.rs`](../../../../engine/crates/layerfs-workspace/src/base/view.rs), [`store/bind.rs`](../../../../engine/crates/layerfs-store-service/src/store/bind.rs) |
| File access needs no Exec registration | No type in this crate names an Exec, command or process; kernel visits carry a mount, serial and handle only | [`operations/kernel_visit.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_visit.rs) |
| A change is visible to every later call on the same Workspace | It is published to the one overlay file the Workspace owns | [`mutation/job.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/job.rs) |
| Each operation is atomic | All effects of one operation publish in one overlay transaction, or none do | [`mutation/job.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/job.rs) |
| A lost reply does not undo a change | `Outcome::Applied` carries a `Publication`; the reply attempt releases it, sent or not | [`operations/types.rs`](../../../../engine/crates/layerfs-workspace/src/operations/types.rs) |

### Per-tool-call and per-task use

```text
  per tool call:  mount -> call -> commit -> unmount
  per task:       mount -> call, call, commit, call, commit ... -> unmount
```

The Workspace has no notion of a tool call, Exec or task. One mounted
Workspace serves any number of sequential or concurrent calls; each call is
more filesystem operations on the same overlay. Mount cost does not grow with
the tree, which keeps the per-tool-call pattern cheap.

## Invariants

| Rule | Enforced by |
| --- | --- |
| A Workspace is the complete tree, including `.git` and ignored files | No filtering anywhere in [`operations/`](../../../../engine/crates/layerfs-workspace/src/operations); owner rule in [AGENTS.md](../../../../AGENTS.md#layerfs-mental-model) |
| Mount binds the root without scanning, copying or restoring it | `BaseView::open`: one root acquisition and a scope check ([`base/view.rs`](../../../../engine/crates/layerfs-workspace/src/base/view.rs)) |
| Filesystem access needs no Exec identity | Kernel visits take `KernelMount`, serial and handle ([`operations/kernel_visit.rs`](../../../../engine/crates/layerfs-workspace/src/operations/kernel_visit.rs)) |
| An overlay job does no Store, provider or content I/O | `OverlayRead` / `OverlayJobs` contract ([`ports/overlay.rs`](../../../../engine/crates/layerfs-workspace/src/ports/overlay.rs)); missing base facts become `Need`s ([`mutation/eval.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/eval.rs)) |
| One lock per Workspace overlay connection, held for one short job | `OwnerClient` runs each job on the calling thread under that Workspace's lock ([`overlay/owner.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/owner.rs)) |
| One attempt per operation; no replay | `MutationPlan::accept` finishes the plan on any error or publication ([`mutation/plan.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/plan.rs)) |
| Base facts stay true while a source is owned | Install is fenced behind source release ([`mutation/facts.rs`](../../../../engine/crates/layerfs-workspace/src/mutation/facts.rs)) |
| Inode serials come from the scope's allocator, never invented | `InodeSerials` ([`workspace/serials.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/serials.rs)) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| One write operation | `WRITE_WINDOW` = 128 KiB; larger writes arrive as several operations | [`operations/types.rs`](../../../../engine/crates/layerfs-workspace/src/operations/types.rs), [`file/write.rs`](../../../../engine/crates/layerfs-workspace/src/operations/file/write.rs) |
| One read request | `READ_WINDOW` = 128 KiB | [`file/read.rs`](../../../../engine/crates/layerfs-workspace/src/operations/file/read.rs) |
| One listing window | `PAGE_ROWS` = 1,024 names, whiteouts included; a continuation resumes | [`namespace/list.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/list.rs) |
| Total files, names, edits or Workspace size | No cap: windows bound one request, not the file or the flow | [`operations/types.rs`](../../../../engine/crates/layerfs-workspace/src/operations/types.rs) |
| Largest file offset | `i64::MAX`; past it `Refusal::TooLarge` (EFBIG) | [`file/write.rs`](../../../../engine/crates/layerfs-workspace/src/operations/file/write.rs) |
| Name length | 255 bytes | [`namespace/list.rs`](../../../../engine/crates/layerfs-workspace/src/operations/namespace/list.rs) |
| Definite pre-effect refusals | `Refusal` (EEXIST, ENOENT, ENOTDIR, EISDIR, ENOTEMPTY, EPERM, EOPNOTSUPP, EINVAL, EMLINK, EFBIG, two ancestry cases) | [`operations/types.rs`](../../../../engine/crates/layerfs-workspace/src/operations/types.rs) |
| Base root changed under a source | `WorkspaceError::BaseChanged` | [`workspace/view.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/view.rs) |
| Stale route or engine | `OverlayError::Stale` | [`workspace/view.rs`](../../../../engine/crates/layerfs-workspace/src/workspace/view.rs) |

Large files and directories stay in indexed overlay rows and are read in
bounded windows; [Operations](operations.md) lists each window.

## Demonstration

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`namespace_view.rs`](../../../../engine/crates/layerfs-workspace/tests/namespace_view.rs) | `layerfs-workspace` | Listing under mutation, no name or inode cap, known install keeping later changes, interleaved owner rounds |
| [`mutation_plan.rs`](../../../../engine/crates/layerfs-workspace/tests/mutation_plan.rs) | `layerfs-workspace` | Plan stages, no provider work inside owner rounds, one publication, no re-entry after an error |
| [`kernel_directory.rs`](../../../../engine/crates/layerfs-workspace/tests/kernel_directory.rs) | `layerfs-workspace` | READDIR merge of base and overlay names, resume cookies, rewind, removed-directory custody |

All three use the shared harness
([`tests/harness/mod.rs`](../../../../engine/crates/layerfs-workspace/tests/harness/mod.rs)):
an in-memory content Store holding a small canonical tree
([`tests/common/mod.rs`](../../../../engine/crates/layerfs-workspace/tests/common/mod.rs)),
a fresh overlay SQLite file in the system temp directory and a counting serial
allocator. They need no Docker, root, mount, environment variable or cargo
feature, and carry no `#[cfg(target_os)]` or `#[ignore]`.

```text
  in-memory Store (canonical tree)     temp overlay .sqlite file
               \                          /
              CanonicalClient + Workspace
                          |
     the test drives operations through OverlayJobs, and can run
     another whole operation between two rounds of the first
```

- **`namespace_view.rs`** lists 200 names while mutating and resuming, and
  checks each persistent name appears once. It creates 3,000 names in one
  directory and checks every listing window holds at most `PAGE_ROWS` names and that
  3,001 serials took three allocator calls. It captures, mutates, installs
  and checks that the later changes survive.
  `cargo +1.85.1 test --locked -p layerfs-workspace --test namespace_view`
- **`mutation_plan.rs`** checks that `prepare_mutation` and owner rounds make
  no base demand, that the first round returns `Needs` without changing the
  overlay revision, that the second round publishes exactly once, and that a
  finished plan refuses another `accept` or `supply`.
  `cargo +1.85.1 test --locked -p layerfs-workspace --test mutation_plan`
- **`kernel_directory.rs`** drives `kernel_directory_visit` with a
  memory-only client and checks merged names, cookie offsets and rewinds.
  `cargo +1.85.1 test --locked -p layerfs-workspace --test kernel_directory`

These commands were not run while writing this page.

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`docs/general/workspace-filesystem-view.md`](../../../general/workspace-filesystem-view.md): covered in [Base](base.md) and [Operations](operations.md); its source pin predates per-Workspace overlay files.
- [`core/docs/architecture/17-workspace-active-components.md`](../../../../core/docs/architecture/17-workspace-active-components.md): historical only.
- [`core/docs/architecture/20-workspace-base.md`](../../../../core/docs/architecture/20-workspace-base.md): covered in [Base](base.md) and [State](state.md).
- [`core/docs/architecture/29-effective-base-view.md`](../../../../core/docs/architecture/29-effective-base-view.md): covered in [Operations](operations.md).
- [`core/docs/architecture/32-live-composition.md`](../../../../core/docs/architecture/32-live-composition.md): historical for this component; generation lifetimes are in [Overlay: Lifetimes and reclamation](../overlay/lifetimes-and-reclamation.md).
- [`core/docs/architecture/50-backed-file-edit-state.md`](../../../../core/docs/architecture/50-backed-file-edit-state.md): covered in [Ports](ports.md) (operation records) and [Commit: Construction](../commit/construction.md).

---

← [Overlay: Operation records](../overlay/operation-records.md) · [Up: Engine](../README.md) · [Workspace: Base](base.md) →
