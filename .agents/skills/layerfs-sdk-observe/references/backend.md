# Backend methods

> **Status:** Current general guide. Part of the `layerfs-sdk-observe` skill. Internal
> surface, for observation and troubleshooting only. `@layerfs/backend` and its
> names are #428's (formerly `@layerfs/sdk/operator`, `operator.*`); result shapes
> are unchanged from the examples captured live in issue #334 Phase A at
> `b30e4fb8f`, whose raw replies remain in the phase run folder.

```js
import { LayerFS } from "@layerfs/agent";
import { LayerFSBackend } from "@layerfs/backend";
const shop = await LayerFS.open("shop");
const backend = LayerFSBackend.of(shop);   // opens nothing; refused once shop is closed
try {
  console.log(await backend.store.project.get_storage());
} finally {
  await shop.close();
}
```

The agent's `call()` refuses every `backend.*` name; only this package reaches them.
`backend.store.*` reads the project's Store (in this process on Linux, in the
LayerFS worker on macOS and Windows) and needs no Sandbox. `backend.daemon.*` needs
the Sandbox holding the Workspace or process to be running and is refused
`NoSuchSandbox` otherwise. Old traces name the methods before #428; `RENAMED_METHODS`
in `@layerfs/agent` maps them.

## `backend.store.project.get_storage()`

Store totals. Terms are defined in [storage metrics](storage-metrics.md#sources).

**Returns:** `{ logical, canonical, stored, metadata, file, free, dedupe, compression, init }`.
Bytes are integers; `dedupe` and `compression` are numbers or `null`.
`init` is what Init imported and what it cost, recorded once when Init sealed the
Store: `{ entries, files, bytes, canonical, stored, metadata, millis }`. `files` counts
regular-file identities (hardlinks once) and `bytes` their size; `canonical`,
`stored` and `metadata` are the Store totals Init left; `millis` is Init's wall time.
It is `null` for a Store built before Init recorded it.

```json
{
  "logical": 101087,
  "canonical": 132603,
  "stored": 58521,
  "metadata": 251680,
  "file": 270336,
  "free": 0,
  "dedupe": 0.762328,
  "compression": 2.265905,
  "init": { "entries": 11, "files": 9, "bytes": 2012, "canonical": 8012, "stored": 4780, "metadata": 161720, "millis": 13 }
}
```

## `backend.store.branch.list_commit_storage(branch, pageSize?, pagePos?)`

Per-Commit size counters for a mutable Branch, newest first along first parents.
`main` is a Layer chain: this method refuses it with `NoSuchBranch`. Preserve that
refusal if collecting every name from `project.list_branches`; collect its
publication Layers through `backend.store.project.get_history()` instead.

**Returns:** a page whose items are `{ commit, logical, newCanonical, newStored,
newMetadata }`. `logical` is the Commit's whole tree; the `new*` counters are that
Commit's Save.

## `backend.store.fs.get_storage(ref, path)`

One file's current size and revision count.

**Returns:** the file's `size`, cumulative revision `logical` bytes, `revisions` (exact up to 1000), `revisionsMore`
(true beyond that), and `canonical`, `stored`, `metadata` as `null`: per-file physical
bytes are not attributed.

## `backend.store.fs.list_revisions(ref, path, pageSize?, pagePos?)`

A file's revisions, newest first.

**Returns:** a page; each item names the `commit` (or Layer) that changed the file and
its `message`. `newCanonical` and `newStored` are `null` (not attributed per file).

## `backend.daemon.workspace.get_storage(ws)`

What a mounted Workspace holds uncommitted.

**Returns**

| Field | Meaning |
| --- | --- |
| `inodes` | Changed inode rows |
| `entries` | Changed directory-entry rows |
| `payload` | Uncommitted file bytes stored, including validity masks |
| `metadataRows` | Inode, entry and operation-record rows together |
| `metadata` | `null` (no exclusive byte attribution) |
| `changes` | `{ added, modified, removed }` path counts |

## `backend.daemon.workspace.list_changes(ws, pageSize?, pagePos?)`

The changed paths in a mounted Workspace, compared with its base.

**Returns:** a page; each item is `{ path, kind, markers }`, with `kind` one of
`Added`, `Modified`, `Removed`, and `markers: true` when the file still holds conflict
markers.

**Notes:** walks the namespace linearly; slow on huge trees.

## `backend.store.project.get_history(since?)`

The project's Branches, their newest Commits and its Layers, read from the Store
alone (no Sandbox), with a `version`. Pass the last `version` as `since` to get
`{ kind: "unchanged", version }` until history moves. Commits are cut per Branch:
`more` names each cut Branch with the cursor `branch.list_commits` continues it from,
and `moreLayers` says older Layers were left out.

## `backend.store.project.get_comparable(id)`

The Commit with the same files as a saved Commit or Layer id, so a Layer can be
compared through `branch.diff_commits`, which takes Commits.

**Returns:** `{ commit, root, exact }`. A Commit is itself. A Layer is the listed
Commit with its root (`exact: true`), else the Commit it was published from
(`exact: false`): then check the diff answer's root equals `root`, since a publish
that merged has other files.

**Refusals:** `NotComparable` (Init's Layer, a publish that merged, or an id
`get_history` doesn't list), `InvalidInput`.

## `backend.store.project.list_overwritten(layer, pageSize?, pagePos?)`

The `main` Layers one publication replaced without containing them, newest first.

## `backend.daemon.workspace.status(ws)`

Describes a mounted Workspace from its Sandbox's daemon, and whether the Branch head
moved since mount (read in the Store).

**Returns**

| Field | Meaning |
| --- | --- |
| `base` | Commit the Workspace is based on |
| `branchHead` | The Branch's current head |
| `headMovedSinceMount` | True if a Commit now would merge first |
| `activity` | The daemon's activity state for the Workspace |
| `overlay` | `{ inodes, entries, pendingMerge }`: changed files and directory entries |
| `branch` | Branch name |

**Refusals:** `NotMounted`, `NoSuchSandbox`.

```js
const s = await backend.daemon.workspace.status(ws);   // ws: from workspace.mount
if (s.headMovedSinceMount) console.log("the next commit will merge with", s.branchHead);
```

## `backend.daemon.sandbox.get_log(sandbox, { lines? })`

The last `lines` lines (1 to 2000, default 200; at most 64 KiB) of a Sandbox's daemon
log, read once through Docker where the SDK's binding runs, so it also works on macOS
and Windows, where your process may have no Docker access. A stopped Sandbox's log is
readable until its records are reclaimed.

**Returns:** `{ sandbox, container, text, bytes }`.

**Refusals:** `NoSuchSandbox` (no record, or Docker no longer has the container),
`InvalidInput` (bad `lines`; there is no `since`).

## `backend.daemon.process.start(ws, command)`

Starts a background command in a mounted Workspace. Owned by the native executor; the
filesystem daemon supervises nothing.

**Returns:** `{ kind: "running", process, command }`. Save `process` to cancel it; its
id names its Sandbox (`proc_<n>_…`).

## `backend.daemon.process.list(pageSize?, pagePos?)`

**Returns:** a page `{ items, hasMore, nextPagePos }` of the background processes of
every running Sandbox of the project, each with its `id`, `sandbox`, `path`,
`command` and `status` (`running`, `exited` or `stopped`).

## `backend.daemon.process.cancel(process)`

Stops a background process group. A cancelled process can leave one unreaped zombie
(a 0.1.7 limit).

**Refusals:** `NoSuchProcess`.

**Notes:** `workspace.unmount` refuses with `Busy` while a background process started
here still runs in that Workspace.
