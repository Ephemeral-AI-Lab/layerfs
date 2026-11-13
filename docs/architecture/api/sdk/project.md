# project family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `project` calls: import a host folder into a Store as `<store>/<project>`, read the home's project records, count or export a saved tree, and remove a project, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/project`](../../../../api/crates/layerfs-api/src/project), [`layerfs-store/src/project`](../../../../control/crates/layerfs-store/src/project), [`layerfs-control/src/project.rs`](../../../../control/crates/layerfs-control/src/project.rs) |
| Runs by default | `list`, `get`: the caller's process, home records only. `init`, `stats`, `export`, `remove`: where the Store's calls run (the caller's process for PostgreSQL and a sqlite Store on a locally executing machine, else that machine's worker) |
| Design | [Stores and projects](../../control/stores-and-projects/README.md), [projects](../../control/stores-and-projects/projects.md), [Project Init](../../engine/project-init/README.md) |

## Calls

```text
 store.create | store.connect          (a Store must exist first)
        |
        v
 init <store>/<project> <folder> --> main's first Layer
        |
        +--> list [store]   get <store>/<project>   (home records only)
        +--> stats          export <destination>    (one pinned saved tree)
        +--> branch.*, commit.*, file.*, sandbox.create ...
        v
 remove <store>/<project>   (no Sandbox of it may be recorded)
```

| Call | Returns |
| --- | --- |
| [`project.init`](#projectinit) | the project summary plus `layer`, `root` |
| [`project.list`](#projectlist) | `{ items: [summary], next_page_pos }` |
| [`project.get`](#projectget) | the project summary |
| [`project.stats`](#projectstats) | entry counts of one pinned tree |
| [`project.export`](#projectexport) | pinned identity and local output counts |
| [`project.remove`](#projectremove) | `{ stack, packs_kept: true, project }` |

The **project summary** is `ProjectRecord::to_sdk`
([`project/record.rs`](../../../../control/crates/layerfs-store/src/project/record.rs)):
`{ project: "<store>/<name>", store, name, kind, machine, created }`. `kind`
and `machine` come from the Store's record; `machine` is `native` when none is
recorded; `created` (ms since the epoch) is null when the project's record is
missing or unreadable.

A `project` argument is `<store>/<name>`: both parts 1–63 characters of
`a-z 0-9 _ . -`, starting and ending with a letter or digit, not a Windows
device name, checked before dispatch
([`calls/check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs),
[`names.rs`](../../../../control/crates/layerfs-home/src/names.rs)).
A folder scope refuses every project call (`out of scope: project.<call>`,
[`calls/scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs)).

### project.init

Imports a folder on the caller's host as a new project of an existing Store:
the project's first Layer and its hidden first Branch.

```text
JS      await layerfs.project.init(project, folder, { timeout, on_progress })
Python  layerfs.project.init(project, folder, *, timeout=None, on_progress=None)
CLI     layerfs project init <project> <folder> [--timeout <timeout>] [--progress]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | Project | the Store must have a record (`store.create` or `store.connect`) |
| `folder` | Text | an absolute path on the caller's host; read whole, with no Git-ignore filtering |
| `timeout` option | Int | Init's deadline in ms, sent as `timeoutMs`; default 3,600,000 (`INIT_TIMEOUT_MS`); must be positive |
| `on_progress` option | Function | `{ phase, done, total }`: `check`, `send` (worker route only, bytes), `install`, `record` |

Returns the summary plus `layer` (the Layer Init published, the first
Branch's base) and `root` (that Branch's effective root)
([`project/family.rs`](../../../../control/crates/layerfs-store/src/project/family.rs)).

| Store | Runs in |
| --- | --- |
| PostgreSQL | caller's process (a build with the `postgresql` feature) |
| sqlite on a locally executing machine (`native` on Linux or macOS) | caller's process; the kernel guard admits only Linux ext4, xfs or btrfs, or macOS apfs or hfs |
| sqlite on any other machine, and `native` on Windows | that machine's worker, on a staged copy of the folder sent from the caller |

| Error | When | Source |
| --- | --- | --- |
| `InvalidInput` | relative `folder`, non-positive `timeout`, a source entry Init refuses (device, FIFO, socket, set-user-ID or sticky bits); worker route: folder missing or not a directory | [`project/family.rs`](../../../../control/crates/layerfs-store/src/project/family.rs), [`project/send.rs`](../../../../control/crates/layerfs-store/src/project/send.rs) |
| `NotFound` | no Store record of that name | `project/family.rs` |
| `Exists` | the project's record, entry or LayerStack name exists | `project/family.rs`, [`identity.rs`](../../../../control/crates/layerfs-store/src/identity.rs) |
| `Busy` | also when another add's working rows remain in the Store | `identity.rs` `add_failure` |
| `Capacity` | worker route: the machine's free disk is below the copy plus headroom | `project/send.rs` `room` |
| `Unsupported` | the kernel guard refuses the Store's filesystem; PostgreSQL in a build without `postgresql` | [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs) |
| `NoStore` | no Store file at the Store's path | `identity.rs` |
| `Failed` | deadline passed, read error or any other engine failure, **including an Init whose outcome is unknown**; the entry this call made is kept | `identity.rs` `add_failure` |
| `Unknown` | worker route: a stage or the `project.init` answer was lost; the home gains no record, and the worker removes its staged copy when its Init ends | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |

After a `Failed` add the kept entry makes `project.list` show the project with
`created: null` while `project.get` answers `NoSuchProject`; a second Init of
the same name is `Exists` or `Busy` and changes nothing.

### project.list

Pages through the projects the home records, for every Store or one Store,
without opening any Store.

```text
JS      await layerfs.project.list(store?, { page_size, page_pos })
Python  layerfs.project.list(store=None, *, page_size=None, page_pos=None)
CLI     layerfs project list [<store>] [--page-size <page-size>] [--page-pos <page-pos>]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `store` (optional) | Name | its record must exist, else `NotFound`; omitted: every Store in the home |
| `page_size` option | Int | clamped to 1..=1000, default 50; never refused |
| `page_pos` option | Int | items to skip; negative is taken as 0 |

Returns `{ items: [summary], next_page_pos }`, ordered by Store name then
project name; `next_page_pos` is null after the last page. There is no
`has_more`. The whole listing is built from home folders, then paged in
memory; a project folder without `record.json` is listed with
`created: null` ([`project/family.rs`](../../../../control/crates/layerfs-store/src/project/family.rs) `list`).
Errors: `InvalidInput` (bad Store name), `NotFound`, `Failed` (a home folder
cannot be read).

### project.get

One project's summary from the home's records.

```text
JS      await layerfs.project.get(project)
Python  layerfs.project.get(project)
CLI     layerfs project get <project>
```

Returns the summary (`View::Full`), the same shape as one `project.list`
item. It says what the home records, not whether the Store still holds the
project, and holds no Branch, Layer or size. Errors: `NoSuchStore`,
`NoSuchProject` (no `record.json`, including the entry a failed Init keeps)
([`resolve.rs`](../../../../control/crates/layerfs-store/src/resolve.rs)).

### project.stats

Entry counts of one pinned saved tree.

```text
JS      await layerfs.project.stats(project, branch?, commit?)
Python  layerfs.project.stats(project, branch=None, commit=None)
CLI     layerfs project stats <project> [<branch>] [<commit>]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `branch` (optional) | Name | default `main` |
| `commit` (optional) | Name | a full saved id admitted by the Branch context; default the Branch's saved head |

```text
{ project, at: { id, kind, branch, root }, files, directories, symlinks,
  others, logical, complete, reason? }
```

The pin is resolved once; uncommitted Workspace changes are not counted. The
root directory is excluded, hard links count per path, symlinks are not
followed, and `others` is 0 (the canonical types are files, directories and
symlinks). `logical` is the whole pinned tree's recorded regular-file bytes.
The walk shares the saved-read budget of 10,000 entries
(`MAXIMUM_QUERY_WORK`); past it the answer has partial counts,
`complete: false` and a `reason`, never a silent cap. An object read error is
a refusal, not a partial answer
([`read/tree_stats.rs`](../../../../engine/crates/layerfs-store-service/src/read/tree_stats.rs),
[`read/query.rs`](../../../../engine/crates/layerfs-store-service/src/read/query.rs)).

### project.export

Exports one committed Branch snapshot to an ordinary directory on the
caller's host.

```text
JS      await layerfs.project.export(project, destination, { branch, on_progress })
Python  layerfs.project.export(project, destination, *, branch=None, on_progress=None)
CLI     layerfs project export <project> <destination> [--branch <branch>] [--progress]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `destination` | Text | an absolute directory path on the caller's host, absent or an existing empty real directory; its parent must exist; not `/`, not containing `..`, not inside the home |
| `branch` option | Name | default `main`; a Branch name, never a Commit or Layer id |
| `on_progress` option | Function | `prepare`, `export`, `finalize` (below) |

There is no `commit` option. `main` selects its published Layer; another
Branch its head Commit, or its base Layer when it has none. The node and root
are pinned once, so later writes do not change the exported view; Workspace
overlays are excluded.

```text
{ project, store, name, kind, machine, branch, snapshot, node, root,
  destination, files, directories, symlinks, bytes }
```

| Fidelity | Contract |
| --- | --- |
| Entries | every directory (empty ones too), `.git`, ignored files, dependencies and caches; exact regular-file bytes through bounded buffers |
| Symlinks | the original target (relative, absolute or dangling); never followed |
| Metadata | supported permission bits (directory modes after their children) and modification times with nanoseconds, where the host supports them |
| Not kept | hard-link identity, ownership, ACLs, extended attributes, sparse allocation, creation time |

Everything is written into a private `0700` staging directory
`.layerfs-export-<random>` beside the destination, then renamed onto it once
complete; `finalize` is sent only after that rename. Any failure removes this
call's staging directory only
([`project/export/`](../../../../control/crates/layerfs-store/src/project/export/mod.rs)).
Progress payloads are `{ phase, files, directories, symlinks, bytes, done,
total }`: counters are local output, `done` is `bytes` (null in `prepare`),
`total` is always null (no prescan), and `export` events come at most once
per 200 ms besides the first and last
([`export/writer.rs`](../../../../control/crates/layerfs-store/src/project/export/writer.rs)).

| Store | Snapshot reader | Writer |
| --- | --- | --- |
| sqlite on a locally executing machine | caller's process | caller's host |
| sqlite on a worker machine | that worker, streaming bounded entry and byte frames with backpressure over one request | caller's host |
| PostgreSQL | caller's process (`postgresql` feature) | caller's host |

| Error | When |
| --- | --- |
| `InvalidInput` | bad path or Branch, a `commit` option, any folder scope (`out of scope: project.export`) |
| `NoSuchBranch` | a valid Branch name that does not exist |
| `NotFound`, `NotDirectory` | the destination's parent is missing or not a directory |
| `Exists` | the destination is a file or symlink (not followed) |
| `NotEmpty` | the destination has entries, at the start or at publication |
| `Conflict` | host names collide (case-insensitive filesystem); nothing is overwritten |
| `Unavailable` | a required worker is stopped or unreachable; nothing starts it |
| `Capacity`, `Failed` | a local write or metadata failure, or a Store format this build does not open (refused at open, before `prepare` and before any staging) |
| `Unknown`, `Unavailable` | the worker stream broke; staging is removed and the source is unchanged |

Export only reads history: a lost reply creates no source mutation and no
uncertain intent.

### project.remove

Drops one project's LayerStack (Layers, Branches, Commits) from its Store in
one write, then the project's home entry; other projects stay.

```text
JS      await layerfs.project.remove(project)
Python  layerfs.project.remove(project)
CLI     layerfs project remove <project>
```

Returns `{ stack, packs_kept: true, project }`: `stack` is the removed
LayerStack's id; `packs_kept` is always true because the project's packs and
objects stay in the Store
([`project/remove.rs`](../../../../control/crates/layerfs-store/src/project/remove.rs)).
It runs through `layerfs_control::project::remove`
([`project.rs`](../../../../control/crates/layerfs-control/src/project.rs)),
under the Store's lease, locally or as the worker operation `project.remove`.

| Error | When | Source |
| --- | --- | --- |
| `NoSuchProject` | also when the Store holds no LayerStack of that name | [`identity.rs`](../../../../control/crates/layerfs-store/src/identity.rs) |
| `InUse` | a Sandbox of the project is recorded; `detail.related` lists `{ kind: "sandbox", name }` | [`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) |
| `Unsupported` | a PostgreSQL Store ("removing a project from a PostgreSQL Store is not built"); the kernel guard refuses the Store's filesystem | [`project/family.rs`](../../../../control/crates/layerfs-store/src/project/family.rs) |
| `Failed` | the Store write failed, its outcome included | `identity.rs` `remove_project` |
| `Unknown` | worker route: the answer was lost | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |

After `Failed` or `Unknown` the home entry is kept (it is removed only after
success). `branch.list` answering `NoSuchProject` from the Store means the
stack is gone.

## Family errors

Every `project` call except `list` and `get` can also meet the Store
refusals: `NoSuchStore`, `NoSuchProject`; `Busy` or `Unresolved` while the
Store's record holds another operation's intent
([`op.rs`](../../../../control/crates/layerfs-home/src/op.rs)); `Unavailable`
when the Store's worker is not recorded or not reached
([`place.rs`](../../../../control/crates/layerfs-store/src/place.rs)). The
catalog is in [errors and events](../calls/errors-and-events.md).

## Related

- [SDK index](README.md), [calls](../calls/README.md).
- [Stores and projects](../../control/stores-and-projects/README.md): records, placement, leases.
- [Project Init](../../engine/project-init/README.md) and the [Init flow](../../flows/init.md).
- [branch family](branch.md): reading a project's history.
