# SDK facade

[Architecture](../../README.md) › [Engine](../README.md) › [Store calls](README.md) › SDK facade

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How SDK method names map onto Store calls.

| | |
| --- | --- |
| Component | [Store calls](README.md) |
| Source | [`engine/crates/layerfs-store-service/src/methods`](../../../../engine/crates/layerfs-store-service/src/methods), [`engine/crates/layerfs-store-service/src/operator`](../../../../engine/crates/layerfs-store-service/src/operator) |

## Arguments

```text
SdkCall { method: "fs.read", args: SdkValue::Map{ project, branch,
                                                  commit?, path, ... } }
   |
   v  methods/file/read.rs
   snapshot(service, args)   project -> stack; commit else branch -> Snap
   path(args, "path")        checked absolute path
   line(args, "startLine")   Arg<u64>: read now, refused at first use
   |
   v  read::reads::read(reader, snap, path, start, end)   typed engine call
```

| Reader | Accepts | Refusal | Source |
| --- | --- | --- | --- |
| `text(args, name)` | Required text | `InvalidInput` "missing text argument" | [`operator/args.rs`](../../../../engine/crates/layerfs-store-service/src/operator/args.rs) |
| `project(service, args)` | `project` name | `NoSuchProject` | [`methods/args.rs`](../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `snapshot(service, args)` | `commit` if given, else `branch` | as [Reads](reads.md#reads-and-windows) | same |
| `path` / `optional_path` | Checked absolute path; `/` when absent | `InvalidInput` | same |
| `paging(args)` | `pageSize` 1..=1000 (default 50), `pagePos` >= 0 (default 0) | `InvalidInput` | [`operator/args.rs`](../../../../engine/crates/layerfs-store-service/src/operator/args.rs) |
| `line(args, name, default)` | Positive safe integer (<= 2^53 - 1) | `InvalidInput` | [`methods/args.rs`](../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| `diff_page(args)` | Paging, `cursor` (path or null), `hunks` (bool, default true) | `InvalidInput` | same |
| `selector(args, name, branch)` | `{ path, commit? }` | `InvalidInput` | same |
| `change(value)` | One `{ op, ... }` change; `op` is `write`, `edit`, `mkdir`, `chmod`, `rm`, `cp` or `mv` | `InvalidInput` | same |

A single-change method (`fs.write`, `fs.edit`, `fs.mkdir`, `fs.rm`, `fs.cp`,
`fs.mv`) builds its change from its own fields with `op` added (`single`), then
goes through the same `commit` as `fs.apply`. A `write` change that declares
`streamed` bytes to follow the call is refused: no bytes follow a call on the
in-process route.

## Dispatch

```text
methods::call(service, call)
  "message" present and not null?
     not text or CommitMeta::check fails -> InvalidInput
  match call.method:
    project.*  -> methods/project.rs   -> history::branch
    branch.*   -> methods/branch/*.rs  -> history::branch | read::compare
    fs.<read>  -> methods/file/*.rs    -> read::{reads, blame, compare}
    fs.<write> -> methods/file/*.rs    -> args::commit -> write::files
    other      -> UnknownMethod
```

| Method | Method file | Engine call |
| --- | --- | --- |
| `project.list_branches` | [`project.rs`](../../../../engine/crates/layerfs-store-service/src/methods/project.rs) | `history::branch::list` |
| `branch.fork` | [`branch/fork.rs`](../../../../engine/crates/layerfs-store-service/src/methods/branch/fork.rs) | `history::branch::fork` |
| `branch.list_commits` | [`branch/list_commits.rs`](../../../../engine/crates/layerfs-store-service/src/methods/branch/list_commits.rs) | `history::branch::list_commits` |
| `branch.merge` | [`branch/merge.rs`](../../../../engine/crates/layerfs-store-service/src/methods/branch/merge.rs) | `history::branch::merge` |
| `branch.publish` | [`branch/publish.rs`](../../../../engine/crates/layerfs-store-service/src/methods/branch/publish.rs) | `history::branch::publish` |
| `branch.diff_commits` | [`branch/diff_commits.rs`](../../../../engine/crates/layerfs-store-service/src/methods/branch/diff_commits.rs) | `read::compare::diff_commits` |
| `branch.diff_branches` | [`branch/diff_branches.rs`](../../../../engine/crates/layerfs-store-service/src/methods/branch/diff_branches.rs) | `read::compare::diff_branches` |
| `fs.read` | [`file/read.rs`](../../../../engine/crates/layerfs-store-service/src/methods/file/read.rs) | `read::reads::read` |
| `fs.list` | [`file/list.rs`](../../../../engine/crates/layerfs-store-service/src/methods/file/list.rs) | `read::reads::list` |
| `fs.stat` | [`file/stat.rs`](../../../../engine/crates/layerfs-store-service/src/methods/file/stat.rs) | `read::reads::stat` |
| `fs.grep` | [`file/grep.rs`](../../../../engine/crates/layerfs-store-service/src/methods/file/grep.rs) | `read::reads::grep` |
| `fs.glob` | [`file/glob.rs`](../../../../engine/crates/layerfs-store-service/src/methods/file/glob.rs) | `read::reads::glob` |
| `fs.diff_files` | [`file/diff_files.rs`](../../../../engine/crates/layerfs-store-service/src/methods/file/diff_files.rs) | `read::compare::diff_files` |
| `fs.blame` | [`file/blame.rs`](../../../../engine/crates/layerfs-store-service/src/methods/file/blame.rs) | `read::blame::blame` |
| `fs.write`, `fs.edit`, `fs.mkdir`, `fs.rm`, `fs.cp`, `fs.mv` | [`file/`](../../../../engine/crates/layerfs-store-service/src/methods/file) (one file each) | `write::files::commit` with one change |
| `fs.apply` | [`file/apply.rs`](../../../../engine/crates/layerfs-store-service/src/methods/file/apply.rs) | `write::files::commit` with `changes[]` |

Source of the table: [`methods/dispatch.rs`](../../../../engine/crates/layerfs-store-service/src/methods/dispatch.rs).
The front ends reach it through `layerfs-control`, which strips its own
routing fields and calls `methods::call` once
([`service.rs`](../../../../control/crates/layerfs-store/src/service.rs));
see [API: Call table](../../api/call-table/README.md).

## Refusal order

A method reads each argument just before the engine step that first uses it.
An argument used only after a lookup travels as an `Arg<T>`
(`Result<T, SdkRefusal>`): read, but refused only where the engine uses it.
So a call with several faults is refused by the first one the engine meets
([`methods/args.rs`](../../../../engine/crates/layerfs-store-service/src/methods/args.rs),
[`history/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/history/lookup.rs)).

```text
fs.list { path: "/absent", pageSize: 0 }
  project -> snapshot -> path -> directory lookup  => NoSuchPath
                                (paging not reached)
fs.list { pageSize: 0 }
  project -> snapshot -> path "/" found -> paging   => InvalidInput
```

| Call | Checked first | Then | Source |
| --- | --- | --- | --- |
| any | `message` (if not null) | method name | [`methods/dispatch.rs`](../../../../engine/crates/layerfs-store-service/src/methods/dispatch.rs) |
| `fs.list`, `fs.read`, `fs.stat` | project, snapshot, path lookup | page or line arguments | [`read/reads.rs`](../../../../engine/crates/layerfs-store-service/src/read/reads.rs) |
| `fs.blame` | entry is a regular file | `fromLine` | [`read/blame.rs`](../../../../engine/crates/layerfs-store-service/src/read/blame.rs) |
| `branch.diff_branches` | `from` resolved | `to`, then page arguments | [`methods/branch/diff_branches.rs`](../../../../engine/crates/layerfs-store-service/src/methods/branch/diff_branches.rs) |
| `fs.diff_files` | left side resolved | right side | [`read/compare.rs`](../../../../engine/crates/layerfs-store-service/src/read/compare.rs) |
| file changes | project, `branch` (`ReadOnly` for `main`), Branch lookup | each change in order, refused at its `index` | [`write/files.rs`](../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| `branch.merge` | sources differ, `units`, reserved name, name taken | name grammar, sides | [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |

The order is pinned by
[`method_refusal_order.rs`](../../../../engine/crates/layerfs-store-service/tests/method_refusal_order.rs)
and by two-fault calls in
[`sdk_method_golden.rs`](../../../../engine/crates/layerfs-store-service/tests/sdk_method_golden.rs).
Command (not run while writing this page; no platform, feature, Docker or root
requirement in source):
`cargo +1.85.1 test --locked -p layerfs-store-service --test method_refusal_order`

## Operator calls

```text
operator::dispatch(service, call)     (separate table; not in methods::call)
  backend.store.project.get_history       -> inspect::snapshot(store, args, [])
  backend.store.project.get_storage       -> store::stats
  backend.store.branch.list_commit_storage -> store::commits
  backend.store.project.list_overwritten  -> store::overwritten
  backend.store.fs.get_storage            -> files::stats
  backend.store.fs.list_revisions         -> files::history
  other                                   -> UnknownMethod
```

| Call | Answer | Bound | Source |
| --- | --- | --- | --- |
| `project.get_history` | Branches, newest Commits and Layers, Workspaces (none here), `version` digest; `unchanged` when `since` names the version. | 56 KiB answer, 12 KiB for Layers, first 64 Branches. | [`operator/inspect.rs`](../../../../engine/crates/layerfs-store-service/src/operator/inspect.rs) |
| `project.get_storage` | Logical, canonical, stored, metadata, file and free bytes; Init totals. From publication counters; never scans the object catalog. | Fixed. | [`operator/store.rs`](../../../../engine/crates/layerfs-store-service/src/operator/store.rs) |
| `branch.list_commit_storage` | Per-Commit logical and new bytes. | `pageSize`. | same |
| `project.list_overwritten` | Layers a publication overwrote, newest first. | `pageSize`. | same |
| `fs.get_storage` | A path's size and revision count along first parents. | Stops at 1000 revisions (`revisionsMore`). | [`operator/files.rs`](../../../../engine/crates/layerfs-store-service/src/operator/files.rs) |
| `fs.list_revisions` | Revisions where a path changed (`Added`, `Removed`, `Modified`). | `pageSize` and 48 KiB. | same |

The same `inspect::snapshot` with a daemon's Workspaces answers the daemon's
`sandbox.list_workspaces`
([`layerfs-daemon/src/operator/mod.rs`](../../../../engine/crates/layerfs-daemon/src/operator/mod.rs)).
A production caller of `operator::dispatch` outside tests was not found in
this repository; Not established from source which front end routes
`backend.store.*` names to it.

## Related

- [Reads](reads.md), [Writes](writes.md), [Branches](branches.md) — the engine calls behind each method.
- [Service](service.md) — the `StoreService` every method takes.
- [API: Call table](../../api/call-table/README.md) and [Control: Routing](../../control/routing/README.md) — how a front-end call reaches `methods::call`.

---

← [Store calls: Branches](branches.md) · [Up: Store calls](README.md) · [Engine: Project Init](../project-init/README.md) →

## Store statistics

`store.stats` is a public Store method over the existing `StoreStatistics`
publication counters, through `methods/store.rs`. It uses the same fixed
statistics read as the operator, renames logical to `historyLogical`, and
omits text ratios. The control call adds name/kind and makes PostgreSQL free
null. No project is required; the standard Store lease and worker route apply.

`store.storage_report` resolves each explicit `{project, branch, commit}` pin
like `branch.get_commit`, then walks every object the pin's filesystem root
reaches through `methods/storage_report.rs` and `store/storage_report.rs`,
under one global object budget and one canonical-byte budget. Object roles and
lengths come from the Store's locators (`layerfs_storage::Reader::describe_objects`);
references are derived by admitting each authenticated object. With
`includeEncoding` it also reads each selected WholeFile/Chunk object's own
stored record width and direct base. `project.storage_report` is the same
method with every pin in one project.

`project.stats` resolves a Branch-qualified pin and walks its immutable tree through `read/tree_stats.rs`, bounded by the saved-read candidate budget. It returns explicit partial-count completeness on exhaustion; logical bytes remain the full pin's recorded total.
