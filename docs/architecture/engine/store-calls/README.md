# Store calls

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Typed reads, file changes and Branch history over the global Store, run once in the process that opened it, and the SDK facade that names them.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-store-service` |
| Source | [`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs), [`bootstrap.rs`](../../../../engine/crates/layerfs-store-service/src/bootstrap.rs), [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs), [`methods/`](../../../../engine/crates/layerfs-store-service/src/methods), [`read/`](../../../../engine/crates/layerfs-store-service/src/read), [`write/`](../../../../engine/crates/layerfs-store-service/src/write), [`history/`](../../../../engine/crates/layerfs-store-service/src/history), [`operator/`](../../../../engine/crates/layerfs-store-service/src/operator) |
| Uses | [Store](../store/README.md), [Overlay](../overlay/README.md), [Workspace](../workspace/README.md), [Commit](../commit/README.md), [Core: Compare and merge](../../core/compare-and-merge/README.md), [Core: History](../../core/history/README.md) |
| Used by | [Control: Stores and projects](../../control/stores-and-projects/README.md), [API: Calls](../../api/calls/README.md), [Daemon](../daemon/README.md) (shares the `store` and `overlay` modules) |
| Flows | [A call, end to end](../../flows/call.md), [Fork and merge](../../flows/merge.md) |

The same crate holds the `store` and `overlay` ports a daemon's Workspace
Commits use; those are described under [Commit](../commit/README.md) and
[Overlay](../overlay/README.md). This topic covers the Store calls.

## Picture

```text
 caller's process (SDK binding, CLI, or the Linux worker on macOS/Windows)
   SdkCall { method, args }
        |
        v
 methods::call  (one file per public method; args read at first use)
        |                 |                     |
        v                 v                     v
   read/             write/               history/
   snapshot, reads,  files::commit:       fork, merge, publish,
   walk, search,     one private          lists, saved lookups,
   blame, compare,   Workspace, changes,  merge units
   saved pages       one Commit
        |                 |                     |
        v                 v                     v
 StoreService { store: Arc<Store>, owner: OwnerClient, bound <= 64 }
   kernel_guard::check -> open (Disposable) -> overlay template
        |                                     |
        v                                     v
   Store (SQLite file or PostgreSQL)     overlay owner: one private
   one writer + read-only sessions       overlay per file call
```

## What it does

### Where Store calls run

| Behaviour | Detail | Source |
| --- | --- | --- |
| One route, in the caller's process | `fs.*`, `branch.*`, `project.*` and `store.stats` run on this engine in the process that opened the Store. There is no daemon route for them and no fallback. | [`lib.rs`](../../../../engine/crates/layerfs-store-service/src/lib.rs), [`service.rs`](../../../../control/crates/layerfs-store/src/service.rs) |
| Daemon serves only Workspace calls | Mount, exec, Workspace Commit, status and unmount go to a Sandbox daemon; the daemon reuses this crate's `store` and `overlay` modules, not `StoreService`. | [`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs), [Daemon](../daemon/README.md) |
| Kernel guard first | Before any file opens or is created, `kernel_guard::check` verifies without modifying permissions that the Store's own directory (`store.parent()`, or `.` when empty) exists as a real `0700` directory owned by the current effective user (refusing a missing path, a symlink, wrong owner or `mode & 0o077 != 0` with a `chmod 700` hint) and checks the filesystem type. Linux: ext4, xfs, btrfs. macOS: apfs or hfs (native Sandbox, one shared macOS kernel). Anything else: `OpenRefusal::Unsupported` with its reason. | [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs) |
| macOS and Windows hosts | A process that cannot open the Store under the guard sends its Store calls to a loopback worker: a Linux process (a container on macOS, a WSL2 distribution on Windows) that runs this same engine and opens the Store directly. The engine knows no worker. | [Control: Workers and reach](../../control/machines/workers-and-reach.md) |
| PostgreSQL | `open_postgres` (feature `postgresql`) runs no kernel guard: the Store is a server reached over a connection. Without the feature a PostgreSQL selection fails `BackendUnavailable`. | [`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs), [`bootstrap.rs`](../../../../engine/crates/layerfs-store-service/src/bootstrap.rs) |

### Opening the service

```text
StoreService::open_direct(store, binding, cursor_key, template)
  1 kernel_guard::check(store)              -> Unsupported (nothing opened)
  2 store.is_file()                         -> Missing
  3 open_store_observed(sqlite, Disposable, CALLS=64, cache 64 KiB)
       writer session + read-only control reader
                                            -> Busy("Store open: ..") /
                                               Failed("Store open: ..")
  4 Owner::start(template, pager 4096 KiB, namespaces 64)
                                            -> Failed("overlay template: ..")
```

A writable `Session::connect` takes the Store directory's writer lane, so two
openers queue instead of meeting SQLite's `Busy`; a `Busy` open therefore
means a writer that bypasses the lane (an uncoordinated raw SQLite process)
held the lock, and it is a definite refusal that changed nothing: the caller
may repeat the call. Any other open failure stays `Failed`.

| Field or constant | Meaning | Source |
| --- | --- | --- |
| `CALLS` = 64 | Private Workspaces one process holds at once (one per SDK binding worker thread); `bind` refuses `Capacity` at the limit. | [`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs) |
| `CACHE_BYTES` = 64 KiB | The Store's shared immutable-object cache. | same |
| `PAGER_KIB` = 4096 | Each private overlay's page cache, so one write window's transaction does not spill. | same |
| `retained` | Private Workspaces whose Commit outcome is uncertain, kept with their failure until `close`. | same |
| `opened` | The Store plus writer and reader SQL observations (`SqlSessions::totals`, `diagnostics`). | [`bootstrap.rs`](../../../../engine/crates/layerfs-store-service/src/bootstrap.rs) |

### Method table

`methods::call` runs one public method once. `read`, `write` and `history`
take typed values and know no method name; only `methods/` turns an
`SdkValue` map into them. `layerfs-control` re-exports `methods::call`.

| Family | Methods | Engine call | Source |
| --- | --- | --- | --- |
| File reads | `fs.read`, `fs.list`, `fs.stat`, `fs.grep`, `fs.glob`, `fs.blame`, `fs.diff_files`, `fs.revision` | `read::*` ([Reads](reads.md)) | [`methods/file/`](../../../../engine/crates/layerfs-store-service/src/methods/file) |
| File changes | `fs.write`, `fs.edit`, `fs.mkdir`, `fs.rm`, `fs.cp`, `fs.mv`, `fs.apply` | `write::files::commit` | same |
| Branch history | `branch.fork`, `branch.merge`, `branch.publish`, `branch.list_commits`, `branch.commits`, `branch.get_commit` | `history::branch`, `history::saved` | [`methods/branch/`](../../../../engine/crates/layerfs-store-service/src/methods/branch) |
| Branch compare | `branch.diff_commits`, `branch.diff_branches` | `read::compare`, `read::branch_compare` | same |
| Project | `project.list_branches`, `project.stats` | `history::branch::list_bounded`, `read::tree_stats` | [`methods/project.rs`](../../../../engine/crates/layerfs-store-service/src/methods/project.rs) |
| Store | `store.stats` | `operator::store::stats` (publication counters; `logical` renamed `historyLogical`, ratios dropped) | [`methods/store.rs`](../../../../engine/crates/layerfs-store-service/src/methods/store.rs) |

The table is [`methods/dispatch.rs`](../../../../engine/crates/layerfs-store-service/src/methods/dispatch.rs).
A non-null `message` is checked first (`CommitMeta::check`); any other name
is `UnknownMethod`. A single-change method builds one change and goes
through the same `commit` as `fs.apply`; a change that declares `streamed`
bytes is refused, since no bytes follow a call on this route
([`methods/args.rs`](../../../../engine/crates/layerfs-store-service/src/methods/args.rs)).

**Refusal order.** A method reads each argument just before the engine step
that first uses it; an argument needed only after a lookup travels as
`Arg<T>`. A call with several faults is refused by the first one the engine
meets: `fs.list { path: "/absent", pageSize: 0 }` is `NoSuchPath`, not
`InvalidInput`.

### File changes

```text
files::commit(service, project, branch, changes, message)
  resolve  branch "main" -> ReadOnly; unknown -> NoSuchBranch
  bind     fresh WorkspaceId; private Workspace (never listed or mounted)
  change   Mutation per change, in order; first refusal stops, {index}
  commit   commit_captured_via(message, CommitVia::Files)
           head moved -> merged in the same Save; conflict -> Conflict
  release  close Workspace, retire overlay namespace
  answer   Committed { head, commit } | UpToDate { head, root }
```

| Change | Fields | Effect | Source |
| --- | --- | --- | --- |
| `write` | `path`, `content` or `bytes`, `mode?` | Replace bytes; a new file is 0o644, an existing one keeps its mode. Applied in pieces of `MAX_PLAINTEXT_BYTES` (65,519); an equal piece is not rewritten. | [`write/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) |
| `edit` | `path`, `oldText`, `newText` | Replace the one occurrence; scans and shifts in 32 KiB windows. `NoMatch`, `AmbiguousMatch`, `Binary`, `NotFile`. | [`write/edit.rs`](../../../../engine/crates/layerfs-store-service/src/write/edit.rs) |
| `mkdir` | `path`, `mode?` | Create with missing parents; `mode` default 0o755, parents 0o755. | [`write/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) |
| `chmod` | `path`, `mode` | Set permission bits. | same |
| `rm`, `cp`, `mv` | `path` / `from`, `to` | Recursive remove; recursive copy keeping hard links; one rename. Overlap and an existing `to` are refused. | [`write/recursive.rs`](../../../../engine/crates/layerfs-store-service/src/write/recursive.rs) |

A path is absolute, at most 4096 bytes, without NUL, trailing `/`, empty,
`.` or `..` components; a mode is 0..=0o7777, and set-ID or sticky bits on a
file are refused ([`write/files.rs`](../../../../engine/crates/layerfs-store-service/src/write/files.rs)).
Recursive walks hold one directory page per level, and the copy's hard-link
map lives in the private overlay's indexed operation records (kind
`0x53444b43`), not in memory. Each change runs as short locked jobs on the
private overlay; serials come from the Store's pooled reservation
([`store/ports.rs`](../../../../engine/crates/layerfs-store-service/src/store/ports.rs)).
The Commit itself is the Workspace Commit; see [Commit](../commit/README.md).

### Branch history

| Call | What happens | Source |
| --- | --- | --- |
| `branch.fork` | One Branch row through the catalog; no tree copied, no object read. Source is `main`'s head Layer, a Branch, or a Commit/Layer id. `ReservedName`, `NameTaken`. | [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `branch.merge` | Creates a new Branch `name`; neither source changes. An ancestor side fast-forwards with no Save. Otherwise merge base (`NoMergeBase`), optional merge units, `merge_roots` with `/.lww` rules from `branch`, Save finish and bindings, all before the lane; then one `merge_branch` publish. A conflict drops the Save and answers `Conflict`. | same, [`history/units.rs`](../../../../engine/crates/layerfs-store-service/src/history/units.rs) |
| `branch.publish` | `add_layer` with expected stack head and Branch base: a compare-and-set, no retry. `Added` (first 50 overwritten Layers reported), `UpToDate`, `NoChanges`; moved heads are `HeadMoved`. Publish overwrites; skipped Layers are reported, not merged. | [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `branch.list_commits`, `project.list_branches` | Newest first / byte-name order; catalog pages of `MAXIMUM_PAGE_RECORDS` (128). | same |
| `branch.commits`, `branch.get_commit` | Saved history from a Branch-qualified pin (`resolve_context`): `main` follows publication Layers, a Branch its Commit parents; a Commit outside the Branch history is `NoSuchCommit`. | [`history/saved.rs`](../../../../engine/crates/layerfs-store-service/src/history/saved.rs), [`methods/branch/commits.rs`](../../../../engine/crates/layerfs-store-service/src/methods/branch/commits.rs) |

A merge unit (a glob list) that both sides changed comes whole from the side
with the later write (ours on a tie); the losing side is reverted in the same
Save before the ordinary merge. The decision algorithm is in
[Core: Compare and merge](../../core/compare-and-merge/README.md).

### Read sessions

| Handle | Opened | Closed | Source |
| --- | --- | --- | --- |
| Control reader (session 0) | At open, or at first use with shared readers | `stop_reads` | [`store/read_service.rs`](../../../../engine/crates/layerfs-store-service/src/store/read_service.rs) |
| Private or mounted Workspace reader | At bind or mount; reuses an idle reader first | Returned to idle (`IDLE_READERS` = 4) if it served without failure | same |

Every published-history lookup (`Store::published`) uses the read-only
control session, so no read waits for a Commit holding the publication
lane ([`store/open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs)).
A session whose demand had an unknown outcome keeps that failure, refuses
later demands and never returns to the idle pool
([`store/read_handle.rs`](../../../../engine/crates/layerfs-store-service/src/store/read_handle.rs)).
A PostgreSQL Store opens with shared readers; an idle one is checked
(`StoreReader::usable`) before reuse and dropped, not retried, if it fails.

### Operator calls

`operator::dispatch` is a separate table of `backend.store.*` names; it is
not in `methods::call`, and in this repository only tests call it.

| Call | Answer | Source |
| --- | --- | --- |
| `project.get_history` | Branches, newest Commits and Layers, `version` digest (`unchanged` when `since` matches); 56 KiB answer, 12 KiB for Layers. The daemon's `sandbox.list_workspaces` uses the same snapshot with its Workspaces. | [`operator/inspect.rs`](../../../../engine/crates/layerfs-store-service/src/operator/inspect.rs) |
| `project.get_storage`, `branch.list_commit_storage`, `project.list_overwritten` | Publication counters, per-Commit bytes, overwritten Layers; never scans the object catalog. | [`operator/store.rs`](../../../../engine/crates/layerfs-store-service/src/operator/store.rs) |
| `fs.get_storage`, `fs.list_revisions` | A path's size and revisions along first parents; the count stops at 1000 (`revisionsMore`). | [`operator/files.rs`](../../../../engine/crates/layerfs-store-service/src/operator/files.rs) |

## Invariants

| Rule | Enforced by |
| --- | --- |
| No FUSE, socket or Sandbox in this crate; no unsafe Rust. | [`Cargo.toml`](../../../../engine/crates/layerfs-store-service/Cargo.toml), `#![forbid(unsafe_code)]` in [`lib.rs`](../../../../engine/crates/layerfs-store-service/src/lib.rs) |
| The guard runs before any file is opened; a refused setup leaves the Store and template untouched. | [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs), [`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs) |
| A SQLite Store is opened Disposable explicitly, with no busy handler, retry or fsync. | `open_direct` in [`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs) |
| `read`, `write` and `history` know no SDK method names. | [`methods/mod.rs`](../../../../engine/crates/layerfs-store-service/src/methods/mod.rs) |
| Each call runs once; no retry, replay or fallback. | [`methods/dispatch.rs`](../../../../engine/crates/layerfs-store-service/src/methods/dispatch.rs) |
| Reads never take the publication lane. | [`store/open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs) |
| Merge work runs before the lane; only `merge_branch` publishes. | [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `main` changes only through `branch.publish`. | `ReadOnly` in [`write/files.rs`](../../../../engine/crates/layerfs-store-service/src/write/files.rs) |
| An uncertain file-call Commit keeps its Workspace and answers `Unknown` with Workspace id and namespace. | `StoreService::retain` in [`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs) |
| Saved lookups never cross projects: an id from another LayerStack is `InvalidInput`. | [`history/saved.rs`](../../../../engine/crates/layerfs-store-service/src/history/saved.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Unsupported directory, platform or filesystem | `OpenRefusal::Unsupported`, nothing opened or created | [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs) |
| Store file absent | `OpenRefusal::Missing` | [`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs) |
| Private Workspaces per process | 64; then `Capacity` | same |
| Overlay maintenance stopped | `bind` refuses | same |
| `pageSize` | 1..=1000, default 50 | [`operator/args.rs`](../../../../engine/crates/layerfs-store-service/src/operator/args.rs), [`read/query.rs`](../../../../engine/crates/layerfs-store-service/src/read/query.rs) |
| Reply budget | 48 KiB (`window::BYTES`) | [`read/window.rs`](../../../../engine/crates/layerfs-store-service/src/read/window.rs) |
| Saved-history walk per request | `MAXIMUM_QUERY_WORK` / `LOOKUP_WORK` 10,000 nodes; then `Capacity` | [`read/query.rs`](../../../../engine/crates/layerfs-store-service/src/read/query.rs), [`history/saved.rs`](../../../../engine/crates/layerfs-store-service/src/history/saved.rs) |
| Merge serial reservation | 64 at a time (`MERGE_SERIALS`) | [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| Merge on a Store without merge scratch | `BackendUnavailable` | [`store/open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs) |
| Unknown method, bad argument | `UnknownMethod`, `InvalidInput` at the argument's first use | [`methods/dispatch.rs`](../../../../engine/crates/layerfs-store-service/src/methods/dispatch.rs), [`methods/args.rs`](../../../../engine/crates/layerfs-store-service/src/methods/args.rs) |
| History errors | `HeadMoved`/`StackMoved` → `HeadMoved`; `Missing` → `NoSuchBranch`/`NoSuchCommit`/`NotFound`; `Busy`, `Capacity`; `UnknownOutcome` → `Unknown`; else `Failed` | [`history/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| Workspace errors in a change | `NoSuchPath`, `Exists`, `IsDirectory`, `NotDirectory`, `NotEmpty`, `InvalidInput`; no-space `Capacity`; anything else `Unknown` (Workspace kept) | [`write/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/write/mutation.rs) |

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`sdk_method_golden.rs`](../../../../engine/crates/layerfs-store-service/tests/sdk_method_golden.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test sdk_method_golden` | macOS or Linux | Every public method's answer and main refusals against [`sdk_methods.txt`](../../../../engine/crates/layerfs-store-service/tests/golden/sdk_methods.txt); `LAYERFS_GOLDEN_PRINT=1` prints instead. |
| [`method_refusal_order.rs`](../../../../engine/crates/layerfs-store-service/tests/method_refusal_order.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test method_refusal_order` | macOS or Linux | Two-fault calls are refused by the fault the engine meets first. |
| [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/tests/kernel_guard.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test kernel_guard` | Linux (tmpfs refused); macOS (apfs allowed) | `Unsupported` before any file opens: Store bytes unchanged, no template. |
| [`sdk_reads_during_publish.rs`](../../../../engine/crates/layerfs-store-service/tests/sdk_reads_during_publish.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test sdk_reads_during_publish` | macOS or Linux | With the lane held 3 s, `fs.read`, `fs.stat` and `fs.list` each finish within 750 ms. |
| [`merge_units.rs`](../../../../engine/crates/layerfs-store-service/tests/merge_units.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test merge_units` | macOS or Linux | A unit both sides changed comes whole from the later write. |
| [`write_refusals.rs`](../../../../engine/crates/layerfs-store-service/tests/write_refusals.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test write_refusals` | macOS or Linux | A write onto a directory is `IsDirectory`, onto another non-file `NotFile`, a bad name `InvalidInput`; nothing is committed. |

The tests build a Disposable Store with Project Init in a temporary
directory ([`tests/support/sdk.rs`](../../../../engine/crates/layerfs-store-service/tests/support/sdk.rs)).

## Pages

| Page | What it covers |
| --- | --- |
| [Reads](reads.md) | Snapshots, text windows, walks and search, numbered pages and continuations, blame, compare and tree statistics (`read/`). |
