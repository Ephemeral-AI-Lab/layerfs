# project.export

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [project](README.md) › export

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`project.export` exports one committed Branch snapshot to an ordinary directory
on the caller's host.

| | |
| --- | --- |
| Family | [project](README.md) |
| Library entry | [`api/crates/layerfs-api/src/project/export.rs`](../../../../../api/crates/layerfs-api/src/project/export.rs) |
| Implemented in | `layerfs_control::project::family::run` → [`project/export.rs`](../../../../../api/crates/layerfs-control/src/project/export.rs), reusing the [tree protocol and destination writer](../../../../../api/crates/layerfs-control/src/project/staged.rs) |
| Runs in | the Store reader runs in the caller or the machine worker; the destination writer always runs on the caller's host; no Sandbox daemon |

## Signature

```js
// Node.js: branch defaults to "main"; omit on_progress when unused.
await layerfs.project.export(project, destination, { branch: "main", on_progress });
await client.project.export(project, destination, { branch: "main", on_progress });
```

```python
# Python: synchronous and coroutine forms, with the same options.
layerfs.project.export(project, destination, branch="main", on_progress=handler)
await layerfs.aio.project.export(project, destination, branch="main", on_progress=handler)
# Without a handler:
layerfs.project.export(project, destination)
```

```bash
layerfs project export <store>/<project> <destination> [--branch <name>] [--progress] [--text]
```

`on_progress` is optional. In the current Python shell an explicitly supplied
`on_progress=None` is `InvalidInput`; omit the keyword instead. Both shells
require a function when the option is supplied.

## Arguments and options

| Name | Kind | Rule |
| --- | --- | --- |
| `project` | Project, required | `<store>/<project>`, recorded in the selected home; a bare project name is `InvalidInput` |
| `destination` | Text, required | absolute directory path on the invoking process's host, including when the Store is on a worker |
| `branch` | Name, optional | defaults to `main`; a Branch name, never a Commit/Layer ID selector |
| `on_progress` | Function, optional | receives the progress payload described below; CLI maps it to `--progress` |

There is no `commit` option or `--commit` switch. Export selects a Branch's
current committed view, then pins its immutable node and filesystem root once.
`main` selects its published Layer; another Branch selects its head Commit, or
its base Layer when it has no Commit. A hexadecimal ID passed as `branch` never
selects a historical node: invalid names are refused, and an absent valid name
is `NoSuchBranch`.

Concurrent `file` writes, Workspace Commits and Branch publications after that
pin do not change the exported view. Uncommitted Workspace overlay changes are
excluded. An active Sandbox or mounted Workspace does not itself prevent export.
A folder-scoped client is refused `InvalidInput: out of scope: project.export`.

## Destination and fidelity

The destination may be absent or an existing empty real directory. Its parent
must already exist and permit creation and rename. Root paths, paths containing
`..`, relative paths and paths within the Store home are refused. An existing
file or symlink at the destination is refused without following it; a nonempty
directory is `NotEmpty`. Export never merges into an existing tree.

All entries are built in a private `0700` staging directory named
`.layerfs-export-<random>` under the destination's parent, on the same filesystem.
After content and metadata are complete, `finalize` rechecks the destination and
atomically renames staging into place, replacing an allowed empty directory.
A destination populated by another process during export is refused and its
files are preserved. Failure, capacity exhaustion or disconnect cleans up only
this call's staging directory, including read-only subdirectories. A preexisting
empty destination and unrelated local files remain intact. No copy fallback
weakens the atomic publication contract.

| Entry or metadata | Export contract |
| --- | --- |
| Directories | preserve hierarchy and empty directories, including root metadata |
| `.git`, ignored files, dependencies and caches | included when present in the pinned snapshot; no Git-ignore filtering |
| Regular files | exact binary bytes, including empty files and chunked large files; streamed through bounded buffers |
| Symlinks | preserve the original target, including relative, absolute and dangling targets; never follow a link to write outside staging |
| Permissions | restore the snapshot's supported permission bits; apply directory modes after their children, so read-only directories can be populated |
| Modification times | preserve seconds and nanoseconds, including symlink metadata, where the host supports them |

The destination filesystem must represent the snapshot's names and metadata.
Case collisions such as `Foo.txt` and `foo.txt` on a case-insensitive filesystem
are `Conflict`, with staging cleaned up, rather than overwriting one entry.
Unsupported special entries (devices, FIFOs and sockets) must be refused rather
than dropped. This contract promises neither hard-link identity nor ownership,
ACL, extended-attribute, sparse-allocation or creation-time preservation. Host
filesystem timestamp precision and symlink/permission support remain limits.

## Where it runs

| Store | Snapshot reader | Destination writer |
| --- | --- | --- |
| SQLite on a locally executing machine (`native` on Linux/macOS) | caller's process, opening the Store directly under the kernel/filesystem guard | caller's host |
| SQLite on a worker machine | that machine's worker; the same snapshot traversal emits bounded entry/byte frames over one request | caller's host, consuming frames with backpressure through the shared staging writer |
| PostgreSQL, local or remote database server | caller's process through the PostgreSQL Store/content ports; requires a build with `postgresql` | caller's host |

The worker path sends entries and binary chunks, not a full archive or a complete
source-side directory copy. It keeps the pinned snapshot and Store lease alive
for the stream. Counters advance after the caller writes the received data.
A stopped or unreachable required worker is `Unavailable`; explicitly use
`layerfs machine start <machine>`. Export starts no machine, moves no Store and
makes no automatic retry. Unsupported routes are refused before destination
creation. Local and remote PostgreSQL connections use the same logical snapshot
and fidelity contract as SQLite.

### Store format prerequisite

Every route above opens the installed Store through the same format check, so
export reads only what this build's persistence layer accepts. A **schema
version 1 SQLite Store** (`user_version` 16–18 or 22–24: one row per inode value
and binding hint, before #91) is refused `Failed` at that open, and the refusal
names the offline conversion `tools/store_blocks.py migrate` for a **closed
copy** of the database. A **PostgreSQL schema version 1 Store** is refused at
open as well — `Integrity` at the PostgreSQL backend, never converted — and it
surfaces here as `Failed`; that SQLite tool is not a PostgreSQL migration path.
Export does not convert, migrate or fall back to another reader, and it creates
no `prepare` event and no staging directory when this check fails. See [Store
lifecycle](../../../engine/store/lifecycle.md#open-and-handles), the [SQLite
Store](../../../engine/store/sqlite.md#schema) and the [PostgreSQL
Store](../../../engine/store/postgresql.md#schema) format references. A Store
written by an older LayerFS that this build refuses therefore needs the offline
step first, not another export attempt.

## Progress and completion

SDK callbacks receive the payload. With `--progress`, CLI stderr carries one
JSON object per line, wrapped as `{"progress": {...}}`; stdout carries only the
final result. Without `--progress`, stderr carries no progress. `--text` changes
the final result's presentation, leaving progress as JSON Lines.

| Phase | Meaning | Counters |
| --- | --- | --- |
| `prepare` | validate and prepare the pinned export | `files`, `directories`, `symlinks`, `bytes` start at zero; `done` is `null`, `total` is `null` |
| `export` | traverse and write into local staging | monotonically increasing `files`, `directories`, `symlinks`, `bytes`; `done = bytes`, `total = null` |
| `finalize` | publish the completed staging directory | final `files`, `directories`, `symlinks`, `bytes`; `done = bytes`, `total = null`; this event alone is not success |

`bytes` counts regular-file logical bytes successfully written on the caller's
filesystem, excluding transport overhead. Counter fields (`files`, `directories`,
`symlinks`, `bytes`) describe local output, not merely received frames, and are
verified against the snapshot counts before atomic publication. `total` stays
`null`: export does no whole-tree prescan to manufacture a percentage. Large-file
chunks and batches of small entries produce live, throttled updates (every 200 ms,
4 MiB or 512 entries); event counts and timing are not fixed. `directories`
counts exported subdirectories beneath the destination (excluding the destination
root directory itself); `files` in both progress and the return value counts
regular files only; `symlinks` in both progress and the return value counts
symbolic links separately.

Every queued event is delivered before a normal call settles. Success requires
the final rename and a successful return; `finalize` can still be followed by a
refusal. Node catches a throwing handler and emits `LayerFSHandlerWarning`,
continuing the call. Synchronous Python reports callback exceptions as unraisable;
`aio` handlers run on the event loop and exceptions reach its exception handler.
Cancelling a Python `aio` task stops further callback delivery, while the native
thread continues and its result is dropped. Stopping observation is not proof
that export stopped: inspect the destination before deciding on another call.
There is no implicit cancellation, resume or replay.

## Returns

A successful call returns snake_case fields:

```text
{ project, store, name, kind, machine, branch, snapshot, node, root,
  destination, files, directories, symlinks, bytes }
```

| Fields | Meaning |
| --- | --- |
| `project`, `store`, `name`, `kind`, `machine` | project identity and the source Store's kind and machine |
| `branch`, `snapshot` | selected Branch and pinned readable snapshot label |
| `node`, `root` | pinned immutable node ID and filesystem root ID |
| `destination` | absolute local destination published by this call |
| `files`, `directories`, `symlinks`, `bytes` | actual regular files, directories, symlinks and regular-file logical bytes exported |

## Errors

| Code | When |
| --- | --- |
| `InvalidInput` | malformed project/path/Branch, unknown `commit` option, or a folder scope |
| `NoSuchStore`, `NoSuchProject`, `NoSuchBranch` | source does not exist |
| `NotFound`, `NotDirectory` | destination parent is missing or not a directory |
| `Exists` (or `NotDirectory`) | destination is an existing non-directory or symlink |
| `NotEmpty` | destination contains entries, initially or at publication |
| `Conflict` | host names collide; no silent overwrite |
| `Busy`, `Unresolved` | Store admission is held by a running or unresolved operation |
| `Unavailable`, `Unsupported` | required worker unavailable, or route/filesystem/feature unsupported |
| `Capacity`, `Failed` | local write or metadata failure, or a Store whose format this build does not read (see [Store format prerequisite](#store-format-prerequisite)); staging is cleaned up |
| `Unknown` / `Unavailable` | worker stream disconnect; cleanup local staging, preserve source, do not replay |

Export reads source history only; a lost worker reply does not create a source
mutation or an uncertain write intent. None of these events authorizes a resend.
A Store-format refusal happens at open, before `prepare`: it leaves an existing
destination exactly as it was and creates no staging directory.

## Examples

Written from the call table and native integration tests (`project_export.rs`,
`project_export.mjs`, `test_project_export.py`). `/exports` must already exist,
and each destination must be absent or empty.

```js
import layerfs from "layerfs";
const result = await layerfs.project.export("shop/api", "/exports/api", {
  branch: "agent",
  on_progress: ({ phase, bytes }) => console.error(phase, bytes),
});
console.log(result.destination);
```

```python
import layerfs
from layerfs import aio

result = layerfs.project.export("shop/api", "/exports/api", on_progress=print)
# Inside an async function, with a separate destination:
# result = await aio.project.export("shop/api", "/exports/api-agent", branch="agent", on_progress=print)
```

```bash
layerfs project export shop/api /exports/api --branch agent --progress
layerfs project export shop/api /exports/api-main --text
```

## Related

- [`project.init`](init.md): import a local folder into an existing Store.
- [`branch.get`](../branch/get.md): inspect a Branch before choosing it.
- [`store.push`](../store/push.md): move a SQLite Store, rather than export files.
- [SDK functions](../../../../versioned/2.0.0/sdk.md#functions-as-options),
  [routing](../../routing/routing-table.md), [errors](../../call-table/errors.md).

---

← [project.get](get.md) · [Up: project](README.md) · [project.remove](remove.md) →
