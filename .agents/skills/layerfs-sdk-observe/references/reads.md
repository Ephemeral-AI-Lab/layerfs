# Reads: history, Stores, machines and Workspaces

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-sdk-observe` skill. History, Store and machine results were captured
> from the built `layerfs` command in a private home on 2026-11-02 (ids
> shortened). Workspace and Sandbox reads were not run (no Sandbox was started);
> their fields are from source. Full arguments and refusals are in the
> `layerfs-js-sdk` skill's [method pages](../../layerfs-js-sdk/references/methods/).

## 1.1.0 reads and their 2.0 calls

| 1.1.0 (`@layerfs/backend`, `@layerfs/runtime`) | 2.0 |
| --- | --- |
| `store.branch.list_commit_storage(branch)` | `commit.list(project, branch)`: every Commit record carries `logical`, `new_canonical`, `new_stored`, `new_metadata` |
| `store.project.get_storage()` | **none.** `store.status(store)` gives the file and WAL sizes only |
| `store.project.get_history(since?)` | **none.** Page `commit.list` per Branch from `branch.list` |
| `store.project.get_comparable(id)` | **none** |
| `store.project.list_overwritten(layer)` | **none.** Only `branch.publish`'s result lists skipped Layers (`overwrote`) |
| `store.fs.list_revisions(ref, path)` | **none.** `file.blame` gives each line's Commit; `commit.diff` per Commit pair |
| `store.fs.get_storage(ref, path)` | **none** |
| `daemon.workspace.status(ws)` | `workspace.status(ws)` |
| `daemon.workspace.list_changes(ws)`, `get_storage(ws)` | **none.** `workspace.status` gives counts; `sandbox.get` per-Workspace `dirty` counts |
| `daemon.process.start`, `list`, `cancel` | **none** |
| `daemon.sandbox.get_log(sandbox)` | **none.** See [daemon logs](daemon-logs.md) |
| runtime `status`, `vm.status` | `machine.list`, `store.list`, `machine.status` |
| `project.list_sandboxes`, `sandbox.list_workspaces` | `sandbox.list`, `sandbox.get` |
| `remote.list_stores`, `store.check` | `backup.list`, `store.status` |
| `events.subscribe` | **none** (O26: trace files, not written yet) |

## History

`branch.list(project)`: every Branch with `base` (the `main` Layer under it),
`head` and `forked_from`. `main` appears with `id: null`, `head: null` and its
newest Layer as `base`.

`commit.list(project, branch, { page_size, page_pos })`: newest first along
first parents. Each record:

```json
{"id":"12a99d99faf5…","branch":"work","parent":"12c7ef811a49…","second_parent":null,
 "base":"3275838904b6…","root":"f55fa9660a73…","via":"fs","message":"one attempt",
 "merged_from":null,"time":1793574067760,"logical":118,"new_canonical":789,
 "new_stored":313,"new_metadata":250,"displaced":null,"lww":[],"lww_truncated":false}
```

- `via` says what made it: `fs` for `file.*` writes, `exec` for
  `workspace.commit` (from 1.1.0; `exec` not seen live here).
- `displaced` is set on a forced Commit (`force` does not reach the daemon in
  this build, so expect `null`).
- `commit.get(project, branch, id)` returns one record and accepts an 8+
  character prefix.

What changed between two steps: `commit.diff(project, from, to,
{ paths_only: true })` lists paths with `kind` (`Added`, `Removed`,
`Modified`), `type`, and `content` / `metadata` / `identity` flags. Folders
appear as items too; skip `type: "dir"` when counting changed files. Between
two Branch heads (including `main`): `branch.diff(project, from, to)`.

```bash
layerfs commit list shop/api work --text      # one row per Commit, all scalar fields
layerfs commit diff shop/api <from-id> <to-id> --paths-only
```

`commit.*` and `branch.diff` are refused under a folder scope; read history
with an unscoped client.

## Stores and machines

```bash
layerfs store list
layerfs store status shop
layerfs machine status native
```

```json
{"name":"shop","kind":"sqlite","state":"idle","holders":[],"op":null,"machine":"native",
 "file":{"exists":true,"bytes":212992,"wal_bytes":799312}}
```

```json
{"name":"native","platform":"native","state":"running","free_disk":null,"tool_version":null,"down_reason":null,"stores":["shop"]}
```

- `store.status`: `state` (`idle`, `in_use`, `busy`, `down`,
  `unresolved`), `holders` (processes with the Store open), `op` (a move or
  remove under way: `{id, name, phase, started, uncertain}`), and
  `file.bytes` / `file.wal_bytes`. The
  WAL can be larger than the main file during a run; add both for the Store's
  disk use.
- `store.get` / `store.list`: kind, machine, profile (`disposable`), SQLite
  version, projects. No network call.
- `machine.list`: every machine plus which platforms could be created here.
  `machine.status`: live state, free disk, tool version and `down_reason`
  (`null` fields on `native` in the live run).

## Workspaces and Sandboxes

From source, not run.

`workspace.status(ws)`:

| Field | Meaning |
| --- | --- |
| `branch`, `base`, `base_layer` | What it was mounted from |
| `branch_head` | The Branch's head now (`null` if that read failed) |
| `head_moved_since_mount` | `true` when a Commit now would merge first |
| `activity` | The daemon's activity state (`Idle`, `Busy`, …) |
| `overlay` | `{ inodes, entries, pending_merge }`: uncommitted file and entry counts |

`sandbox.get(name)`: the Sandbox record (`name`, `project`, `machine`,
`state`, `id`, `platform`, `runtime`, `image`) plus `workspaces`, each with
`directory` (its path), `branch`, `base`, `activity`, `mounted`,
`head_moved_since_mount`, `dirty` and `pending_merge`.

`sandbox.list(project?)` reads only the home's records. A Sandbox that closed
itself still shows there with `state: "running"`; `sandbox.get` on it is
refused `NoSuchSandbox`.
