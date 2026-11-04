# Store calls

[Architecture](../../README.md) › [Engine](../README.md) › Store calls

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Typed reads, file changes and Branch history over the Store, and the SDK facade that names them.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-store-service` |
| Source | [`engine/crates/layerfs-store-service/src/read`](../../../../engine/crates/layerfs-store-service/src/read), [`engine/crates/layerfs-store-service/src/write`](../../../../engine/crates/layerfs-store-service/src/write), [`engine/crates/layerfs-store-service/src/history`](../../../../engine/crates/layerfs-store-service/src/history), [`engine/crates/layerfs-store-service/src/methods`](../../../../engine/crates/layerfs-store-service/src/methods), [`engine/crates/layerfs-store-service/src/operator`](../../../../engine/crates/layerfs-store-service/src/operator) |
| Uses | [Store](../store/README.md), [Overlay](../overlay/README.md), [Workspace](../workspace/README.md), [Core: Compare and merge](../../core/compare-and-merge/README.md) |
| Used by | [Commit](../commit/README.md), [Daemon](../daemon/README.md), [Connect: Backups](../../connect/backups/README.md), [API: Control](../../api/control/README.md) |
| Flows | [A call, end to end](../../flows/call.md), [A Commit](../../flows/commit.md) |

The crate also holds the `store` and `overlay` ports that a daemon's Workspace
Commits use ([`lib.rs`](../../../../engine/crates/layerfs-store-service/src/lib.rs));
those are described under [Commit](../commit/README.md) and
[Overlay](../overlay/README.md). These pages cover the Store calls.

## Topics

| Topic | What it covers |
| --- | --- |
| [Service](service.md) | The StoreService: opening, binding and read handles. |
| [Reads](reads.md) | Reads, windows, search, blame and compare. |
| [Writes](writes.md) | File writes, edits and recursive changes. |
| [Branches](branches.md) | Fork, merge and publish. |
| [SDK facade](sdk-facade.md) | How SDK method names map onto Store calls. |

## The picture

```text
 SdkCall { method, args }      (from layerfs-control, in the caller's process)
        |
        v
 +--------------------------------------------------------------+
 | methods::call  (SDK facade: one file per public method)      |
 |   args.rs reads each argument just before its first use      |
 +--------------------------------------------------------------+
        |                    |                     |
        v                    v                     v
 +-------------+     +---------------+     +------------------+
 | read/       |     | write/        |     | history/         |
 | snapshot,   |     | files::commit |     | branch: fork,    |
 | reads, walk,|     | Mutation on a |     | merge, publish,  |
 | search,     |     | private       |     | list; lookup;    |
 | blame,      |     | Workspace     |     | units            |
 | compare     |     |               |     |                  |
 +-------------+     +---------------+     +------------------+
        |                    |                     |
        |              bind / commit               |
        v                    v                     v
 +--------------------------------------------------------------+
 | StoreService { store: Arc<Store>, owner: OwnerClient, ... }  |
 |   Store: control reader, Workspace readers, writer, catalog  |
 +--------------------------------------------------------------+
        |                                          |
        v                                          v
   Store (layerfs-persistence)              overlay owner (one private
   SQLite or PostgreSQL                     overlay per file call)
```

## What it does

| Family | Calls that run here | Engine module |
| --- | --- | --- |
| File reads | `fs.read`, `fs.list`, `fs.stat`, `fs.grep`, `fs.glob`, `fs.diff_files`, `fs.blame` | [`read/`](reads.md) |
| File changes | `fs.write`, `fs.edit`, `fs.mkdir`, `fs.rm`, `fs.cp`, `fs.mv`, `fs.apply` | [`write/`](writes.md) |
| Branch history | `branch.fork`, `branch.merge`, `branch.publish`, `branch.list_commits` | [`history/`](branches.md) |
| Branch compare | `branch.diff_commits`, `branch.diff_branches` | [`read/compare`](reads.md#compare) |
| Project | `project.list_branches` | [`history/branch`](branches.md#lookup) |
| Operator | `backend.store.*` (six names) | [`operator/`](sdk-facade.md#operator-calls) |

The public method table is [`methods::call`](../../../../engine/crates/layerfs-store-service/src/methods/dispatch.rs).
The SDK names the file family `fs.*`; the table has no `file.*` or `commit.*`
name. `read`, `write` and `history` take typed values (paths, `Snap`,
`BranchRecord`, `Change`) and know no method name; only `methods/` turns an
`SdkValue` map into those types
([`methods/mod.rs`](../../../../engine/crates/layerfs-store-service/src/methods/mod.rs)).
This keeps one engine under every front end: `layerfs-control` re-exports
`methods::call` and runs it in the caller's process
([`layerfs-control/src/store/service.rs`](../../../../api/crates/layerfs-control/src/store/service.rs)).

Every call runs once, in the process that opened the Store. A read resolves a
snapshot and reads immutable objects. A file change binds one private
Workspace, applies its changes and commits it once. A Branch call writes one
history record through the Store's catalog.

## Invariants

| Rule | Enforced by |
| --- | --- |
| No FUSE, socket or Sandbox in this crate. | Crate description and dependency list in [`Cargo.toml`](../../../../engine/crates/layerfs-store-service/Cargo.toml); [`lib.rs`](../../../../engine/crates/layerfs-store-service/src/lib.rs) |
| `read`, `write`, `history` know no SDK method names. | [`methods/mod.rs`](../../../../engine/crates/layerfs-store-service/src/methods/mod.rs); [engine guide](../../../../engine/AGENTS.md) |
| Each call runs once; no retry, replay or fallback. | [`methods::call`](../../../../engine/crates/layerfs-store-service/src/methods/dispatch.rs); [repository AGENTS.md](../../../../AGENTS.md) |
| Reads do not take the publication lane. | `Store::published` reads through the control reader's read-only session ([`store/open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs)); test `sdk_reads_during_publish` |
| A read resolves its snapshot once, then reads only immutable objects under that root, so a publish during the read cannot mix two heads. | [`read/snapshot.rs`](../../../../engine/crates/layerfs-store-service/src/read/snapshot.rs) |
| `main` changes only through `branch.publish`. | `ReadOnly` refusal in [`write/files.rs`](../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| An uncertain file-call Commit keeps its Workspace and answers `Unknown`. | `StoreService::retain` ([`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs)) |
| No unsafe Rust. | `#![forbid(unsafe_code)]` in [`lib.rs`](../../../../engine/crates/layerfs-store-service/src/lib.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Page size (`pageSize`) for list, grep, glob, diffs, `list_commits`, `list_branches` | 1..=1000, default 50 | [`operator/args.rs`](../../../../engine/crates/layerfs-store-service/src/operator/args.rs) |
| Reply byte budget of one answer | 48 KiB (`window::BYTES`) | [`read/window.rs`](../../../../engine/crates/layerfs-store-service/src/read/window.rs) |
| Lines in one `fs.read` window | 2000 (`window::LINES`) | [`read/window.rs`](../../../../engine/crates/layerfs-store-service/src/read/window.rs) |
| Private Workspaces one process holds at once | 64 (`CALLS`); then `Capacity` | [`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs) |
| Unknown method | `UnknownMethod` | [`methods/dispatch.rs`](../../../../engine/crates/layerfs-store-service/src/methods/dispatch.rs) |
| Invalid argument | `InvalidInput`, raised at the argument's first use | [`methods/args.rs`](../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| Direct open on a platform or filesystem the guard does not allow | `OpenRefusal::Unsupported` | [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs) |

Refusal order is described in [SDK facade](sdk-facade.md#refusal-order).

## Demonstration

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`sdk_method_golden.rs`](../../../../engine/crates/layerfs-store-service/tests/sdk_method_golden.rs) | `layerfs-store-service` | Every public method's answer and main refusals, compared to a recorded golden. |
| [`sdk_diff_pages.rs`](../../../../engine/crates/layerfs-store-service/tests/sdk_diff_pages.rs) | `layerfs-store-service` | Diff pages resume at a cursor and list each change once. |
| [`sdk_reads_during_publish.rs`](../../../../engine/crates/layerfs-store-service/tests/sdk_reads_during_publish.rs) | `layerfs-store-service` | Reads do not wait for a held publication lane. |

All three use the shared fixture in
[`tests/support/sdk.rs`](../../../../engine/crates/layerfs-store-service/tests/support/sdk.rs):
a disposable Store built by Project Init in a temporary directory, opened with
`open_store_observed`, and a `StoreService` over it. They carry no
`#[cfg(target_os)]` and no `#[ignore]`, and need no cargo feature, Docker or
root. The commands were written from source and were not run while writing
this page.

```text
sdk_method_golden                     sdk_reads_during_publish
  seed tree -> Init -> StoreService     thread A: store.history()
  call every method via methods::call            .publish_lane(), held 3 s
  mask hex ids, times, stored bytes     thread B: fs.read, fs.stat, fs.list
  compare tests/golden/sdk_methods.txt           each must end < 750 ms
```

- **`sdk_method_golden`** seeds a small tree, calls each public method through
  the method table, masks identities, times and stored byte counts, and
  compares the transcript with
  [`tests/golden/sdk_methods.txt`](../../../../engine/crates/layerfs-store-service/tests/golden/sdk_methods.txt).
  Two calls with two faults pin which refusal comes first.
  `LAYERFS_GOLDEN_PRINT=1` prints the transcript instead of comparing.
  `cargo +1.85.1 test --locked -p layerfs-store-service --test sdk_method_golden`
- **`sdk_diff_pages`** pages a Commit difference by cursor and checks each
  change is listed once, that a listing without hunks fits its page by paths,
  and that cursor pages pay only for their own entries.
  `cargo +1.85.1 test --locked -p layerfs-store-service --test sdk_diff_pages`
- **`sdk_reads_during_publish`** holds the Store's publication lane on one
  thread for 3 s and checks that `fs.read`, `fs.stat` and `fs.list` on a
  Branch and `fs.read` on `main` each finish within 750 ms.
  `cargo +1.85.1 test --locked -p layerfs-store-service --test sdk_reads_during_publish`

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`core/docs/architecture/61-direct-store-adapter.md`](../../../../core/docs/architecture/61-direct-store-adapter.md) — the Store adapter and its readers are covered in [Service](service.md); the rest is historical only.
- [`core/docs/architecture/69-sdk-project-workspace-facades.md`](../../../../core/docs/architecture/69-sdk-project-workspace-facades.md) — historical only; the current facade is covered in [SDK facade](sdk-facade.md).

---

← [Store: Metadata and objects](../store/metadata-and-objects.md) · [Up: Engine](../README.md) · [Store calls: Service](service.md) →
