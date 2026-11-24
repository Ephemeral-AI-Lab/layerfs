# Reads: history, Stores, machines and Workspaces

> **Status:** Current general guide for the LayerFS 2.0 call surface. Part of
> the `layerfs-sdk-observe` skill. Outputs were printed by the built
> `layerfs` command on a throwaway home on 2026-11-18 (macOS, `native`; ids
> shortened). Full arguments and refusals are in the `layerfs-js-sdk`
> skill's [method pages](../../layerfs-js-sdk/references/methods/).

## 1.1.0 reads and their 2.0 calls

| 1.1.0 (`@layerfs/backend`, `@layerfs/runtime`) | 2.0 |
| --- | --- |
| `store.branch.list_commit_storage(branch)` | `commit.list(project, branch, { detail: true })`: every long row carries `size`, `new_canonical_bytes`, `new_stored_bytes`, `new_metadata_bytes` |
| `store.project.get_storage()` | `store.get(store, { with: ["stats"] })`: Store-wide `logical_bytes`, `canonical_bytes`, `stored_bytes`, `metadata_bytes`, `file_bytes`, `dedupe`, `compression` |
| `store.project.get_history(since?)` | **none.** Page `commit.list` per Branch from `branch.list` |
| `store.project.get_comparable(id)` | **none** |
| `store.project.list_overwritten(layer)` | **none** |
| `store.fs.list_revisions(ref, path)` | `file.history(project, branch, path)` |
| `store.fs.get_storage(ref, path)` | **none** |
| `daemon.workspace.status(ws)` | `workspace.status(workspace)` |
| `daemon.workspace.list_changes(ws)` | `workspace.status(workspace)`: the first 20 changed paths, `changes_count`, `partial` |
| `daemon.workspace.get_storage(ws)` | `sandbox.get(sandbox, { with: ["stats"] })`: per-Workspace `overlay_bytes` |
| `daemon.process.start`, `list`, `cancel` | **none.** `sandbox.get` lists running exec ids; `workspace.cancel` ends them |
| `daemon.sandbox.get_log(sandbox)` | `sandbox.log(sandbox)` |
| runtime `status`, `vm.status` | `home.get({ with: ["tree"] })`, `machine.list`, `machine.get(m, { with: ["live"] })`, `store.list` |
| `project.list_sandboxes`, `sandbox.list_workspaces` | `sandbox.list({ project })`, `workspace.list({ sandbox })`, `sandbox.get` |
| `remote.list_stores`, `store.check` | `backup.list`, `store.doctor` |
| `events.subscribe` | **none** |

Names of the earlier 2.0 surface, removed with no alias: `branch.commits`,
`branch.get_commit`, `branch.diff`, `file.diff`, `file.revision`,
`store.status`, `store.stats`, `machine.status`, `machine.space`,
`logs.list`, `logs.sandbox`, `logs.machine`.

## History

`branch.list(project)`: every Branch, `main` included, as
`{ branch, commit }` (the head); the long form adds `forked_from`, `id`,
`base` and `root`. `main`'s `id` is the Branch the project records for its
main line, the Branch a mount on `main` is scoped to; it is null only when
the project records no such Branch.

`commit.list(project, branch)`: newest first; on `main`, its publications
(with `source_branch`). One long row:

```json
{"commit":"12d12fbd…","message":"Apply filesystem changes","message_truncated":false,"source_branch":null,
 "branch":"agent","parent":"1244862d…","second_parent":null,"base":"32919f55…","root":"a6dd2e42…",
 "via":"file","created_at":"2026-11-18T22:04:16Z","merged_from":null,"source_commit":null,"size":16,
 "new_canonical_bytes":787,"new_stored_bytes":373,"new_metadata_bytes":334}
```

- `via` says what made it: `workspace`, `file` or `merge`, and on `main`
  `publish` or `init`.
- `commit.get(project, id)` answers one Commit by id or a prefix of 8 or
  more, without a Branch; its long form adds `displaced` (a forced Commit's
  replaced head), `overwritten` and `units`.

What changed between two steps: `commit.diff(project, ref, { from })` lists
`{ path, change }` (`added`, `removed`, `modified`); the long form adds
`type` and the `content` / `metadata` / `identity` flags; `with: ["patch"]`
adds the lines. Without `from`: since a Branch's fork point, or a Commit's
parent. One path's own history: `file.history(project, branch, path)`.

```bash
$ layerfs commit diff shop/api agent
{"items":[{"path":"/README.md","change":"modified"},{"path":"/docs/a.txt","change":"modified"},{"path":"/x.txt","change":"added"},{"path":"/y","change":"added"}]}
$ layerfs file history shop/api agent /docs/a.txt
{"items":[{"commit":"125470c8…","change":"modified","message":"edit a"},{"commit":"32919f55…","change":"added"}]}
```

The `commit` calls are refused under a folder scope (`out of scope:
history`); read history with an unscoped client.

## Stores and machines

```bash
$ layerfs home get --with tree
{"home":"/tmp/demo/home","state":"ready","version":"2.0.0","generation":43,"tree":{"machines":[{"machine":"docker","platform":"docker","state":"stopped"},{"machine":"native","platform":"native","stores":[{"store":"shop","engine":"sqlite","projects":["api"]}]}],"stores":[],"buckets":[]}}
$ layerfs store get shop --with live,stats
{"engine":"sqlite","state":"ready","machine":"native","projects":["api"],"observed_at":"2026-11-18T22:05:32Z","stats":{"size":307200,"logical_bytes":199,"canonical_bytes":6815,"stored_bytes":3354,"metadata_bytes":278242,"file_bytes":307200,"reusable_bytes":28672,"dedupe":"0.029200","compression":"2.031902"}}
$ layerfs machine stats native
{"cpus":14,"memory_used_bytes":25904775168,"memory_total_bytes":38654705664,"disk_free_bytes":129668911104,"disk_total_bytes":994662584320}
```

- `home.get({ with: ["tree"] })` reads only the home's records: every
  machine with its sqlite Stores and Sandboxes, the PostgreSQL Stores and the
  buckets, each with its state when not `ready`. `generation` moves whenever
  a record changes: re-read the tree only when it differs.
- `store.list` / `store.get`: from records, no probe. `with: ["live"]` reads
  the Store where it lives: `state`, `observed_at` and the `live` section
  (`holders`, the pids of other processes holding it open; `phases`, a
  running compaction's; `behind`, buckets whose backup is behind).
  `with: ["stats"]` opens it for its totals ([storage
  metrics](storage-metrics.md)).
- `machine.list` / `machine.get`: from records. `with: ["live"]` probes the
  provider and the worker once (10 s, starting nothing); `get`'s `live`
  section adds `disk_free_bytes`, `tool_version` and `worker`.
  `machine.stats` answers processors, memory and the data disk now (with
  `space`: where the disk goes).
- States everywhere: `ready`, `busy` (`op`), `stopped`, `blocked`
  (`blocked_by`), `broken` (`reason`, `fix`), `unknown` (`observed_at`).

## Workspaces and Sandboxes

```bash
$ layerfs workspace list
{"items":[{"workspace":"tests:agent","branch":"agent"}]}
$ layerfs workspace status tests:agent --detail
{"changes":[{"path":"/out.txt","change":"added","type":"file"},{"path":"/x.txt","change":"removed","type":"file"}],"changes_count":null,"partial":false,"branch_moved":false,"state":"ready","op":null,"reason":null,"fix":null,"branch":"agent","commit":"12d12fbd…","base_layer":"32919f55…","branch_head":"12d12fbd…","activity":"idle","overlay":{"inode_count":3,"entry_count":2},"pending_merge":false}
```

`workspace.status(workspace)`:

| Key | Meaning |
| --- | --- |
| `changes`, `changes_count`, `partial` | The first 20 uncommitted entries of one bounded read of 256 overlay rows; the count past 20; whether the read hit its bound |
| `branch_moved` | `true` when the Branch gained Commits since the mount: a Commit now would merge first |
| `state` (+ companion) | From the daemon's activity: `busy` while a mount, commit or unmount runs; `broken` with a `fix` |
| long: `branch`, `commit`, `base_layer`, `branch_head` | What it was mounted from, and the Branch's head now |
| long: `activity`, `overlay`, `pending_merge` | The daemon's own word, `{ inode_count, entry_count }`, a merge in progress |

`sandbox.get(sandbox)`: its state, and its Workspaces from a live read of
the daemon, each `{ workspace, branch }` with the exec ids `running` in it;
the long form adds per Workspace `path`, `commit`, `mounted_at`,
`branch_moved`, `pending_merge` and `commands`. `with: ["stats"]` adds the
daemon's pid, cgroup `usage` (or a `usage_reason`), `daemon_capacity` and
per-Workspace `overlay_bytes`. A machine that does not answer gives
`unknown` with `observed_at`.

`sandbox.list` and `workspace.list` read only records and ask no daemon; a
Sandbox whose Workspace records live in its machine shows
`workspaces_not_listed: true`, and `workspace.list` names Sandboxes it
could not list in `not_listed`.

## Doctors

Six calls check one thing each and change nothing: `home.doctor`,
`machine.doctor`, `store.doctor`, `project.doctor`, `sandbox.doctor`,
`backup.doctor`. Each answers `{"ok":true}` when nothing was found, else
`findings` and `not_checked`; the long form adds the checked `tree`. See the
`layerfs-js-sdk` skill's [doctors page](../../layerfs-js-sdk/references/methods/doctor.md).
What interrupted operations left behind LayerFS removes itself; each
removal is a line of the home's `log/sweep.jsonl`.

```bash
$ layerfs home doctor
{"ok":true}
```
