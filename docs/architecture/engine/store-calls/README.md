# Store calls

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Typed reads, file changes and Branch history over the global Store, run once in the process that opened it, and the SDK facade that names them.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-store-service` |
| Source | [`service.rs`](../../../../engine/crates/layerfs-store-service/src/service.rs), [`open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs), [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/store/kernel_guard.rs), [`families/`](../../../../engine/crates/layerfs-store-service/src/families), [`filesystem/`](../../../../engine/crates/layerfs-store-service/src/filesystem), [`change/`](../../../../engine/crates/layerfs-store-service/src/filesystem/change), [`history/`](../../../../engine/crates/layerfs-store-service/src/history), [`families/`](../../../../engine/crates/layerfs-store-service/src/families) |
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
 StoreService { store: Arc<Store>, overlay: Later | Started, bound <= 64 }
   kernel_guard::check -> SQLite read open  -> overlay template (first file change)
        |                                     |
        v                                     v
   Store (SQLite file or PostgreSQL)     overlay owner: one private
   SQLite writer at first mutation      overlay per file call
```

## What it does

### Where Store calls run

| Behaviour | Detail | Source |
| --- | --- | --- |
| One route, in the caller's process | `fs.*`, `branch.*`, `project.*` and `store.stats` run on this engine in the process that opened the Store. There is no daemon route for them and no fallback. | [`lib.rs`](../../../../engine/crates/layerfs-store-service/src/lib.rs), [`service.rs`](../../../../control/crates/layerfs-store/src/admin/open/service.rs) |
| Daemon serves only Workspace calls | Mount, exec, Workspace Commit, status and unmount go to a Sandbox daemon; the daemon reuses this crate's `store` and `overlay` modules, not `StoreService`. | [`service.rs`](../../../../engine/crates/layerfs-store-service/src/service.rs), [Daemon](../daemon/README.md) |
| Kernel guard first | Before any file opens or is created, `kernel_guard::check` verifies without modifying permissions that the Store's own directory (`store.parent()`, or `.` when empty) exists as a real `0700` directory owned by the current effective user (refusing a missing path, a symlink, wrong owner or `mode & 0o077 != 0` with a `chmod 700` hint) and checks the filesystem type. Linux: ext4, xfs, btrfs. macOS: apfs or hfs (`process` Sandbox, one shared macOS kernel). Anything else: `OpenRefusal::Unsupported` with its reason. | [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/store/kernel_guard.rs) |
| macOS and Windows hosts | A process that cannot open the Store under the guard sends its Store calls to a loopback worker: a Linux process (a container on macOS, a WSL2 distribution on Windows) that runs this same engine and opens the Store directly. The engine knows no worker. | [Control: Workers and reach](../../control/machines/workers-and-reach.md) |
| PostgreSQL | `open_postgres` runs no kernel guard: the Store is a server reached over a connection. | [`service.rs`](../../../../engine/crates/layerfs-store-service/src/service.rs), [`open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs) |

### Opening the service

Each process has one open owner per Store path. The owner reserves a cache
entry, releases the shared cache locks, then opens the provider. Calls on
other Stores can progress during that open or close. Same-Store callers
waiting on an open receive its outcome.

The host worker keeps at most 16 SQLite Store entries per home, including
entries still opening or closing. At capacity it closes the least recently
used idle Store or waits for a slot. Each admission checks the authoritative
Store operation and identity under the Store record lock, even when it
reuses an open entry. The worker holds its own real-home lease until physical
release. The host worker schedules no idle checkpoint for a WAL owned by
another opener; its own writes keep the log bounded through the writing
session's background checkpoint thread. A fenced close releases an idle Store and acknowledges
only after its lease is removed. An opening, active, checkpointing or
closing Store answers `InUse` immediately. Close reports record the
checkpoint outcome and whether WAL content was observed after release,
since another opener can retain WAL frames.

A PostgreSQL read service (`StoreService::open_postgres_read_only`) holds
one Full-checked read-only session and no writer. The host worker may keep
it open between CLI reads (#819 P-B); `OpenedStore::postgres_session_current`
runs the Light check in one round trip and answers whether the session
still serves the Store it opened (binding, catalog id, schema version,
generation and policy) before it serves another call. The host worker also
keeps writable PostgreSQL services (`StoreService::open_postgres`) open
between CLI writes (#819 P-B writes); for one of those the same check runs
on its writer session, and its readers check themselves when used.
The writable open prepares the writer's create statements in one flight
(`prepare_sdk_writes`, #818 W4), once per held service; the read service
sends no writes and prepares none. The read service's Store has no writer,
but its session pays a round trip per demand, so its reads plan as a
PostgreSQL writer's Store plans them (`Store::batched_reads`, #818 R/H).

```text
StoreService::open_direct(store, binding, cursor_key, template)
  1 kernel_guard::check(store)              -> Unsupported (nothing opened)
  2 store.is_file()                         -> Missing
  3 open_sqlite_for_calls(Normal, CALLS=64, cache 64 KiB)
       validated read-only control reader
       writer opens on the first mutation or explicit writer maintenance
       overlay owner starts at the first file-changing call
StoreService::bind / bind_named (the first file-changing call only)
  Owner::start(template, pager 4096 KiB, namespaces 64)
                                            -> that call fails "overlay template: .."
StoreService::open_postgres(selection, binding, cursor_key, template)
  1 open_store_observed(PostgreSQL)         writer, shared readers
  2 OpenedStore::prepare_sdk_writes          one flight (#818 W4)
       overlay owner starts at the first file-changing call
```

The overlay owner and its template file start at the first file-changing call
and serve every later one (#819). A process that only reads (`fs.read`,
`fs.list`, `fs.stat`, ...) never creates the template, so a one-call process
such as the `layerfs` command does not pay for creating and deleting it. A
start that fails fails that call alone; the next file-changing call makes its
own attempt. `StoreService::new(opened, &owner)` serves through an owner its
caller started.

SQLite validates the Store and opens its control reader first. The first
mutation opens the writer for the same file identity. One initializer per
Store publishes the session, committer and merge scratch together. If
initialization fails, the call receives that failure and the next mutation
can make its own attempt. The directory lane coordinates writable opens. A
raw SQLite writer can still cause a definite pre-effect `Busy`. Ordinary
close and idle checkpoints use the writer only if it started. An explicit
Sandbox release can start it for its checkpoint. At service close the local
readers drop before the writer so the last writable handle can finish SQLite
WAL and SHM cleanup.
See [SQLite open](../store/sqlite.md) and
[`sqlite.rs`](../../../../engine/crates/layerfs-persistence/src/config/sqlite.rs).

| Field or constant | Meaning | Source |
| --- | --- | --- |
| `CALLS` = 64 | Private Workspaces one process holds at once (one per SDK binding worker thread); `bind` refuses `Capacity` at the limit. | [`service.rs`](../../../../engine/crates/layerfs-store-service/src/service.rs) |
| `CACHE_BYTES` = 64 KiB | The Store's shared immutable-object cache. | same |
| `PAGER_KIB` = 4096 | Each private overlay's page cache, so one write window's transaction does not spill. | same |
| `retained` | Private Workspaces whose Commit outcome is uncertain, kept with their failure until `close`. | same |
| `opened` | The Store plus writer and reader SQL observations (`SqlSessions::totals`, `diagnostics`). | [`open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs) |

### Method table

`methods::call` runs one public method once. `read`, `write` and `history`
take typed values and know no method name; only `methods/` turns an
`SdkValue` map into them. `layerfs-control` re-exports `methods::call`.

| Family | Methods | Engine call | Source |
| --- | --- | --- | --- |
| File reads | `fs.read`, `fs.list`, `fs.stat`, `fs.grep`, `fs.glob`, `fs.blame`, `fs.diff_files`, `fs.revision` | `read::*` ([Reads](reads.md)) | [`families/file/`](../../../../engine/crates/layerfs-store-service/src/families/file) |
| File changes | `fs.write`, `fs.edit`, `fs.mkdir`, `fs.rm`, `fs.cp`, `fs.mv`, `fs.apply` | `write::files::commit` | same |
| Branch history | `branch.fork`, `branch.merge`, `branch.publish`, `branch.list_commits`, `branch.commits`, `branch.get_commit` | `history::branch`, `history::saved` | [`families/branch/`](../../../../engine/crates/layerfs-store-service/src/families/branch) |
| Branch compare | `branch.diff_commits`, `branch.diff_branches` | `read::compare`, `read::branch_compare` | same |
| Project | `project.list_branches`, `project.stats` | `history::branch::list_bounded`, `read::tree_stats` | [`branch/list.rs`](../../../../engine/crates/layerfs-store-service/src/families/branch/list.rs) |
| Store | `store.stats` | `operator::store::stats` (publication counters; `logical` renamed `historyLogical`, ratios dropped) | [`families/store.rs`](../../../../engine/crates/layerfs-store-service/src/families/store.rs) |

The table is [`families/dispatch.rs`](../../../../engine/crates/layerfs-store-service/src/families/dispatch.rs).
A non-null `message` is checked first (`CommitMeta::check`); any other name
is `UnknownMethod`. A single-change method builds one change and goes
through the same `commit` as `fs.apply`; a change that declares `streamed`
byte lengths is refused, since no route sends them
([`families/args.rs`](../../../../engine/crates/layerfs-store-service/src/families/args.rs)).

`methods::call_streamed(service, call, source)` runs `fs.write` or
`fs.apply` with its content or changes pulled from a `methods::Source`
(#819 S2.2, the host worker's streamed writes). `Source::next` returns the
next change and `Source::piece` the next piece of a write marked
`streamed: true`. The call holds one change and one piece at a time. The
source returns `None` only after its whole input was received, so an input
that stops early is a refusal at that point, the private Workspace is closed
and nothing commits. A refused change stops pulling: the rest of the input
is never read. Without a source a `streamed: true` write is `InvalidInput`.
Other methods ignore the source and run as `methods::call`.

**Refusal order.** A method reads each argument just before the engine step
that first uses it; an argument needed only after a lookup travels as
`Arg<T>`. A call with several faults is refused by the first one the engine
meets: `fs.list { path: "/absent", pageSize: 0 }` is `NoSuchPath`, not
`InvalidInput`.

### File changes

```text
files::commit(service, project, branch, changes, message)
  bind     fresh WorkspaceId; private Workspace (never listed or mounted),
           found by project and Branch name with its snapshot in one read:
           unknown project -> NoSuchProject; unknown -> NoSuchBranch
  resolve  only when the bind cannot run by name or refused before reading:
           project, branch argument, "main" -> ReadOnly, Branch by name
  hidden   bound node's serial + markers kept by the bind; else read them
  plan     PostgreSQL, listed changes: before each 1,024 (PLAN_PATHS),
           their paths' base reads level by level, one page demand + one
           inode wave; streamed changes are not planned
  change   Mutation per change, in order; first refusal stops, {index}
  commit   commit_captured_via(message, CommitVia::Files)
           head moved -> merged in the same Save; conflict -> Conflict
  release  close Workspace, retire overlay namespace
  answer   Committed { head, commit } | UpToDate { head, root }
```

| Change | Fields | Effect | Source |
| --- | --- | --- | --- |
| `write` | `path`, `content` or `bytes`, `mode?` | Replace bytes; a new file is 0o644, an existing one keeps its mode. Applied in pieces of `MAX_PLAINTEXT_BYTES` (65,519); an equal piece is not rewritten. | [`change/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/filesystem/change/mutation.rs) |
| `write`, streamed | `path`, `streamed: true`, `mode?` | The same, its bytes from the source (`Mutation::write_stream`). Its length is known only at the end: a piece within the old length is compared while every byte before it was equal; after the first difference the rest is written; the file is truncated to the streamed length when anything differed. | same |
| `edit` | `path`, `oldText`, `newText` | Replace the one occurrence; scans and shifts in 32 KiB windows. `NoMatch`, `AmbiguousMatch`, `Binary`, `NotFile`. | [`change/edit.rs`](../../../../engine/crates/layerfs-store-service/src/filesystem/change/edit.rs) |
| `mkdir` | `path`, `mode?` | Create with missing parents; `mode` default 0o755, parents 0o755. | [`change/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/filesystem/change/mutation.rs) |
| `chmod` | `path`, `mode` | Set permission bits. | same |
| `rm`, `cp`, `mv` | `path` / `from`, `to` | Recursive remove; recursive copy keeping hard links; one rename. Overlap and an existing `to` are refused. | [`change/recursive.rs`](../../../../engine/crates/layerfs-store-service/src/filesystem/change/recursive.rs) |

A path is absolute, at most 4096 bytes, without NUL, trailing `/`, empty,
`.` or `..` components; a mode is 0..=0o7777, and set-ID or sticky bits on a
file are refused ([`change/run.rs`](../../../../engine/crates/layerfs-store-service/src/filesystem/change/run.rs)).
Recursive walks hold one directory page per level and take the page's
kinds in one demand (`SourceView::kinds`: the overlay's own inodes, then one
`BaseView::values` inode lookup for the rest; no metadata or length, #818
W5); the copy still stats each entry it copies. The copy's hard-link
map lives in the private overlay's indexed operation records (kind
`0x53444b43`), not in memory. Each change runs as short locked jobs on the
private overlay; serials come from the Store's pooled reservation
([`store/ports.rs`](../../../../engine/crates/layerfs-store-service/src/store/ports.rs)).
The Commit itself is the Workspace Commit; see [Commit](../commit/README.md).

**Path walks.** A change resolves its paths from the root, overlay first,
with a full stat of each component on a Store that plans no reads (SQLite:
a directory's stat reads its children's values, which the lookup below it
reuses). On a Store whose provider pays a round trip per demand
(PostgreSQL), the directories a walk passes are bound by kind only
(`SourceView::lookup_kind`: serial and kind, no metadata, length or
child-directory count); only the last component of a path the change reads
gets a full stat, and a parent only its serial (#818 W6). The checks keep
their order either way: the name's grammar, then `NotDirectory` at the
entry before it, then `NoSuchPath`. On that Store (PostgreSQL,
`batched_chain_reads`), the call first plans its paths' base reads
([`change/plan.rs`](../../../../engine/crates/layerfs-store-service/src/filesystem/change/plan.rs)):
the change list's paths, `PLAN_PATHS` (1,024) changes at a time, are
walked level by level, each level's directory pages read as one grouped
demand (at most 128 directories, 1 MiB of names) and the children found,
with all children of each directory whose root page holds its entries and
whose count is not remembered (the count is then remembered, as the
directory's full stat would derive it), as one inode wave of at most
`BaseView::PLAN_SERIALS` (1,024) serials (`BaseView::plan_level`). The plan
only fills the Workspace client's caches; every change still resolves on
its own, so answers are unchanged. A window's plan stops at its first
change that is refused before any read: its arguments, an empty `oldText`,
a `mkdir` mode, `rm` of the root, or an overlapping `cp` or `mv`. That
change answers its own refusal, and nothing is planned when it is the
window's first change. A failed plan read refuses the call at the window's
first change as a failed read (`Failed`). That change's own walk would also
read first, but it might have read only paths that answer, and then
refused or applied itself. A fault can therefore move a failure to an
earlier change, or replace a change's later refusal; the plan is read once
and never retried (an order change approved by the owner, 2026-11-27).
SQLite plans nothing. Nor does a streamed call (`Changes::Streamed`, the
host worker's streamed writes): a plan needs the window's paths ahead, and
a stream gives one change at a time, so each streamed change walks its own
paths with per-level reads. Answers are the same; only its round trips on
PostgreSQL differ.

**Binding by name.** A call on a named Branch other than `main` without
`expected` binds through `Store::bind_named_private`: on PostgreSQL the
writer's one named snapshot statement and the admission message, on SQLite
the Workspace reader's named snapshot, with no control-reader read (#818
W2). The refusal order stays the lookups': a refused `branch` argument,
`main` (`ReadOnly`) and an `expected` Commit (which needs the project
record) take the lookups on the control reader first, as do a missing
Workspace key and a Store that refuses the bind before reading (SQLite's
deferred writer refused at its start, overlay maintenance stopped or its
start failed, call capacity full, a Workspace reader that does not open), so `NoSuchProject` and `NoSuchBranch` still come first and such a
refusal is `Failed` with the bind's text. The bind reports whether it read
the project and Branch; only then do its `NoSuchProject` /
`NoSuchBranch` answer with the lookups' texts, and its snapshot read
failure as a lookup read failure, also when the reader's close after it
failed.

**Hidden paths.** The private overlay already whiteouts a path a
`project.remove_file` marker hides, so `edit` and a `cp` source that come
back `NoSuchPath` ask the base tree whether the path exists there and is
hidden (`Removed`, with its frontier) or never existed (`NoSuchPath`). The
check uses the bound node's `history_serial` and markers that the bind kept
under its mount admission (`BoundWorkspace::kept_markers`, the
`KnownRemovals` of [PostgreSQL](../store/postgresql.md) (h), on both
backends), with no read; the base tree is opened on the control reader only
when such a refusal asks (#818 W1). When the bind kept none (no admission,
or markers charging over `KEPT_REMOVAL_BYTES`), the call reads the markers
and the node's serial and opens the tree first, as before, so a failed
marker read still refuses every change (SPEC D16).

### Branch history

| Call | What happens | Source |
| --- | --- | --- |
| `branch.fork` | One Branch row through the catalog; no tree copied, no object read. Source is `main`'s head Layer, a Branch, or a Commit/Layer id. `ReservedName`, `NameTaken`. | [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `branch.merge` | Creates a new Branch `name`; neither source changes. An ancestor side fast-forwards with no Save. Otherwise merge base (`NoMergeBase`), optional merge units, `merge_roots` with `/.lww` rules from `branch`, Save finish and bindings, all before the lane; then one `merge_branch` publish. A conflict drops the Save and answers `Conflict`. | same, [`history/merge_units.rs`](../../../../engine/crates/layerfs-store-service/src/history/merge_units.rs) |
| `branch.publish` | `add_layer` with expected stack head and Branch base: a compare-and-set, no retry. Before it, a publish that would skip a Layer `main` gained since the Branch's base is `HeadMoved` unless `force` (#691); `expectedBase`/`expectedHead` pin `main`'s head and the Branch head. A refused Commit already published answers `UpToDate` (one `published_layer` read, on the refusal path only). `Added` (first 50 overwritten Layers reported), `UpToDate`, `NoChanges`. | [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `branch.list_commits`, `project.list_branches` | Newest first / byte-name order; catalog pages of `MAXIMUM_PAGE_RECORDS` (128). | same |
| `branch.commits`, `branch.get_commit` | Saved history from a Branch-qualified pin (`resolve_context`): `main` follows publication Layers, a Branch its Commit parents; a Commit outside the Branch history is `NoSuchCommit`. | [`history/saved.rs`](../../../../engine/crates/layerfs-store-service/src/history/saved.rs), [`src/families/commits.rs`](../../../../engine/crates/layerfs-store-service/src/families/commits.rs) |

A merge unit (a glob list) that both sides changed comes whole from the side
with the later write (ours on a tie); the losing side is reverted in the same
Save before the ordinary merge. The decision algorithm is in
[Core: Compare and merge](../../core/compare-and-merge/README.md).

### Read sessions

| Handle | Opened | Closed | Source |
| --- | --- | --- | --- |
| Control reader (session 0) | At open, or at first use with shared readers | `stop_reads` | [`bind/readers.rs`](../../../../engine/crates/layerfs-store-service/src/workspace/bind/readers.rs) |
| Private or mounted Workspace reader | At bind or mount; reuses an idle reader first | Returned to idle (`IDLE_READERS` = 4) if it served without failure | same |

Every published-history lookup (`Store::published`) uses the read-only
control session, so no read waits for a Commit holding the publication
lane ([`store/open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs)).
A session whose demand had an unknown outcome keeps that failure, refuses
later demands and never returns to the idle pool
([`bind/reader.rs`](../../../../engine/crates/layerfs-store-service/src/workspace/bind/reader.rs)).
A PostgreSQL Store opens with shared readers; an idle one is checked
(`StoreReader::usable`) before reuse and dropped, not retried, if it fails.

### Operator calls

`operator::dispatch` is a separate table of `backend.store.*` names; it is
not in `methods::call`, and in this repository only tests call it.

| Call | Answer | Source |
| --- | --- | --- |
| `project.get_history` | Branches, newest Commits and Layers, `version` digest (`unchanged` when `since` matches); 56 KiB answer, 12 KiB for Layers. The daemon's `sandbox.list_workspaces` uses the same snapshot with its Workspaces, or, asked with `history: false`, lists its Workspaces alone (`workspaces`) with none of the history read. | [`operator/list_workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/families/operator/list_workspaces.rs) |
| `project.get_storage`, `branch.list_commit_storage`, `project.list_overwritten` | Publication counters, per-Commit bytes, overwritten Layers; never scans the object catalog. | [`operator/storage.rs`](../../../../engine/crates/layerfs-store-service/src/families/operator/storage.rs) |
| `fs.get_storage`, `fs.list_revisions` | A path's size and revisions along first parents; the count stops at 1000 (`revisionsMore`). | [`operator/files.rs`](../../../../engine/crates/layerfs-store-service/src/families/operator/files.rs) |

## Invariants

| Rule | Enforced by |
| --- | --- |
| No FUSE, socket or Sandbox in this crate; no unsafe Rust. | [`Cargo.toml`](../../../../engine/crates/layerfs-store-service/Cargo.toml), `#![forbid(unsafe_code)]` in [`lib.rs`](../../../../engine/crates/layerfs-store-service/src/lib.rs) |
| The guard runs before any file is opened; a refused setup leaves the Store and template untouched. | [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/store/kernel_guard.rs), [`service.rs`](../../../../engine/crates/layerfs-store-service/src/service.rs) |
| A SQLite Store is opened Normal explicitly, with no busy handler, retry or fsync. | `open_direct` in [`service.rs`](../../../../engine/crates/layerfs-store-service/src/service.rs) |
| `read`, `write` and `history` know no SDK method names. | [`families/mod.rs`](../../../../engine/crates/layerfs-store-service/src/families/mod.rs) |
| Each call runs once; no retry, replay or fallback. | [`families/dispatch.rs`](../../../../engine/crates/layerfs-store-service/src/families/dispatch.rs) |
| Reads never take the publication lane. | [`store/open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs) |
| Merge work runs before the lane; only `merge_branch` publishes. | [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `main` changes only through `branch.publish`. | `ReadOnly` in [`change/run.rs`](../../../../engine/crates/layerfs-store-service/src/filesystem/change/run.rs) |
| An uncertain file-call Commit keeps its Workspace and answers `Unknown` with Workspace id and namespace, plus the evidence a caller needs to settle it: `phase` (a stable name from `CommitPhase::as_str`, not its `Debug` text), whether it `published`, its `attempt` (hex), `expected_head` and `candidate_root`. | `StoreService::retain` in [`service.rs`](../../../../engine/crates/layerfs-store-service/src/service.rs) |
| Saved lookups never cross projects: an id from another LayerStack is `InvalidInput`. | [`history/saved.rs`](../../../../engine/crates/layerfs-store-service/src/history/saved.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Unsupported directory, platform or filesystem | `OpenRefusal::Unsupported`, nothing opened or created | [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/store/kernel_guard.rs) |
| Store file absent | `OpenRefusal::Missing` | [`service.rs`](../../../../engine/crates/layerfs-store-service/src/service.rs) |
| Store of an older schema version (SQLite 2, PostgreSQL 2 or 3; #776) | `OpenRefusal::Outdated` with the persistence refusal's exact status (`layerfs_persistence::outdated_store`), nothing opened or changed; control answers it `OldStore` (public `Unsupported`) with a fix | same |
| Private Workspaces per process | 64; then `Capacity` | same |
| Overlay maintenance stopped | `bind` refuses | same |
| `pageSize` | 1..=1000, default 50 | [`families/args.rs`](../../../../engine/crates/layerfs-store-service/src/families/args.rs), [`paged/query.rs`](../../../../engine/crates/layerfs-store-service/src/filesystem/paged/query.rs) |
| Reply budget | 48 KiB (`window::BYTES`) | [`text/window.rs`](../../../../engine/crates/layerfs-store-service/src/filesystem/text/window.rs) |
| Saved-history walk per request | `MAXIMUM_QUERY_WORK` / `LOOKUP_WORK` 10,000 nodes; then `Capacity` | [`paged/query.rs`](../../../../engine/crates/layerfs-store-service/src/filesystem/paged/query.rs), [`history/saved.rs`](../../../../engine/crates/layerfs-store-service/src/history/saved.rs) |
| Merge serial reservation | 64 at a time (`MERGE_SERIALS`) | [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| Merge on a Store without merge scratch | `BackendUnavailable` | [`store/open.rs`](../../../../engine/crates/layerfs-store-service/src/store/open.rs) |
| Unknown method, bad argument | `UnknownMethod`, `InvalidInput` at the argument's first use | [`families/dispatch.rs`](../../../../engine/crates/layerfs-store-service/src/families/dispatch.rs), [`families/args.rs`](../../../../engine/crates/layerfs-store-service/src/families/args.rs) |
| History errors | `HeadMoved`/`StackMoved` → `HeadMoved`; `Missing` → `NoSuchBranch`/`NoSuchCommit`/`NotFound`; `Busy`, `Capacity`; `UnknownOutcome` → `Unknown`; else `Failed` | [`src/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/lookup.rs) |
| Workspace errors in a change | `NoSuchPath`, `Exists`, `IsDirectory`, `NotDirectory`, `NotEmpty`, `InvalidInput`; no-space `Capacity`; anything else `Unknown` (Workspace kept) | [`change/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/filesystem/change/mutation.rs) |

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`method_golden.rs`](../../../../engine/crates/layerfs-store-service/tests/families/method_golden.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test families method_golden` | macOS or Linux | Every public method's answer and main refusals against [`sdk_methods.txt`](../../../../engine/crates/layerfs-store-service/tests/families/sdk_methods.txt); `LAYERFS_GOLDEN_PRINT=1` prints instead. |
| [`method_refusal_order.rs`](../../../../engine/crates/layerfs-store-service/tests/families/method_refusal_order.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test families method_refusal_order` | macOS or Linux | Two-fault calls are refused by the fault the engine meets first. |
| [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/tests/store/kernel_guard.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test store kernel_guard` | Linux (tmpfs refused); macOS (apfs allowed) | `Unsupported` before any file opens: Store bytes unchanged, no template. |
| [`overlay_template.rs`](../../../../engine/crates/layerfs-store-service/tests/mount/overlay_template.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test mount overlay_template` | macOS or Linux | Open and a read create no overlay template; the first write starts the owner and later writes reuse it. |
| [`reads_during_publish.rs`](../../../../engine/crates/layerfs-store-service/tests/filesystem/reads_during_publish.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test filesystem reads_during_publish` | macOS or Linux | With the lane held 3 s, `fs.read`, `fs.stat` and `fs.list` each finish within 750 ms. |
| [`units.rs`](../../../../engine/crates/layerfs-store-service/tests/merge/units.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test merge units` | macOS or Linux | A unit both sides changed comes whole from the later write. |
| [`refusals.rs`](../../../../engine/crates/layerfs-store-service/tests/change/refusals.rs) | `cargo +1.85.1 test --locked -p layerfs-store-service --test change refusals` | macOS or Linux | A write onto a directory is `IsDirectory`, onto another non-file `NotFile`, a bad name `InvalidInput`; nothing is committed. |

The tests build a Normal Store with Project Init in a temporary
directory ([`tests/support/sdk.rs`](../../../../engine/crates/layerfs-store-service/tests/support/sdk.rs)).

## Pages

| Page | What it covers |
| --- | --- |
| [Reads](reads.md) | Snapshots, text windows, walks and search, numbered pages and continuations, blame, compare and tree statistics (`read/`). |
