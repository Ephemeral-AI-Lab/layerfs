# project family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `project` calls: import a folder on this computer into a Store as `<store>/<project>`, list and read projects with their state, count or export a committed tree, retire a project, hide one path from its whole history and check a project, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/project`](../../../../api/crates/layerfs-api/src/project) (one file per call), [`layerfs-store/src/project`](../../../../control/crates/layerfs-store/src/project), [`layerfs-store/src/calls/project.rs`](../../../../control/crates/layerfs-store/src/calls/project.rs), [`layerfs-control/src/project.rs`](../../../../control/crates/layerfs-control/src/project.rs) |
| Runs by default | `list`, `get`: the home's records, then each ready Store's own list where its calls run. `init`, `stats`, `export`, `delete`, `hide_path`: where the Store's calls run (the caller's process for PostgreSQL and for a sqlite Store on a locally executing machine, else that machine's worker). `doctor`: [the doctors](../../control/doctor/README.md) |
| Design | [Stores and projects](../../control/stores-and-projects/README.md), [projects](../../control/stores-and-projects/projects.md), [Project Init](../../engine/project-init/README.md) |

## Calls

```text
 store create | store connect          (a Store must exist first)
        |
        v
 init <store>/<project> <folder> --> main's first publication (commit)
        |
        +--> list [--store S]   get <store>/<project>   (state, main's head)
        +--> stats [--branch B] [--at C]    export <destination>
        +--> branch.*, commit.*, file.*, sandbox create ...
        +--> doctor [--full]
        v
 delete <store>/<project>     hide-path <store>/<project> <path>
```

| Call | Short answer | MCP |
| --- | --- | --- |
| [`project.init`](#projectinit) | `{ commit }` | `admin` |
| [`project.list`](#projectlist) | `{ items: [{ project }] }` | `admin` |
| [`project.get`](#projectget) | `{ store, state, commit }` | `admin` |
| [`project.stats`](#projectstats) | `{ file_count, dir_count, symlink_count, size }` | `admin` |
| [`project.export`](#projectexport) | `{ file_count, size }` | `admin` |
| [`project.delete`](#projectdelete) | `{ ok }`, or `{ home_entry_kept }` | `admin` |
| [`project.hide_path`](#projecthide_path) | `{ ok }`, or `{ unchanged }` | `admin` |
| [`project.doctor`](#projectdoctor) | the doctor report | `admin` |

A `project` argument is `<store>/<project>`: both parts 1 to 63 characters
of `a-z 0-9 _ . -`, starting and ending with a letter or digit, not a
Windows device name, checked before dispatch
([`calls/check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs),
[`names.rs`](../../../../control/crates/layerfs-home/src/names.rs)); a bare
`api` is `InvalidInput`. A folder scope refuses every `project` call
(`out of scope: project.<call>`,
[`calls/scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs)).

**A project's state** ([`project/state.rs`](../../../../control/crates/layerfs-store/src/project/state.rs))
comes from the records, with no probe: `busy` while an Init or a delete of
it runs (`op { name, phase, started_at }`, `name` `init` or `delete`);
`blocked` when its Store, or that Store's machine, is not ready or the Store
cannot be read (`blocked_by { store | machine, reason }`); `broken` with
`reason: unresolved` for a delete or an Init that stopped part-way, with
`fix: layerfs project delete <project>`; else `ready`. A project is never
`stopped` or `unknown`. A project on record that its Store does not list is
hidden from `list` and `NotFound` to `get` (`project doctor` reports it as a
notice). An export is a read and records no `op`.

### project.init

Import a folder on this computer as a new project into an existing Store.

```text
CLI     layerfs project init <project> <folder> [--timeout 3600000] [--progress]
Node    await layerfs.project.init(project, folder, { timeout, on_progress })
Python  layerfs.project.init(project, folder, *, timeout=None, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>`, new in an existing Store |
| `folder` | text | an absolute path of a folder on this computer |
| `timeout` option | int | the time limit in milliseconds; default `3600000` |
| `on_progress` option | function |  |

| Answer | Keys |
| --- | --- |
| short | `commit` |
| long | `commit, outcome, project, store, engine, machine, root, created_at` |
| call | sends progress; writes one call-log line; MCP `admin` |

`init` never creates a Store; a missing one is `NotFound`. `folder` is read
whole, with no Git-ignore filtering. The project's entry is claimed first
(a name the home already holds is `Exists`), so the project is listed `busy`
while Init runs; a refusal removes the entry, and an outcome that is not
known (a lost reply) keeps it `broken` `unresolved`
([`project/family.rs`](../../../../control/crates/layerfs-store/src/project/family.rs),
[`project/add.rs`](../../../../control/crates/layerfs-store/src/project/add.rs)).
`commit` is main's first publication; the long form adds `outcome`
(`created`), `project`, `store`, `engine`, `machine`, `root` and
`created_at`. Progress events are `{phase, done, total}`: `check`, `send`
(bytes, to a worker only), `install`, `record`.

| Store | Runs in |
| --- | --- |
| PostgreSQL | the caller's process |
| sqlite on a locally executing machine (the host machine, `linux-host` or `macos-host`) | the caller's process; the kernel guard admits the Store's directory only when it is owned by this user and private (`0700`) on Linux ext4, xfs or btrfs, or macOS apfs or hfs |
| sqlite on any other machine (which must be running), and the `host` platform on Windows | that machine's worker, on a staged copy of the folder sent from the caller, deleted when its Init ends ([`project/send.rs`](../../../../control/crates/layerfs-store/src/project/send.rs)) |

| Refusal | When | Source |
| --- | --- | --- |
| `InvalidInput` | a relative `folder`, a `timeout` that is not positive, a source entry Init refuses (device, FIFO, socket, set-user-ID or sticky bits), a missing folder | [`project/init.rs`](../../../../control/crates/layerfs-store/src/project/init.rs), [`project/send.rs`](../../../../control/crates/layerfs-store/src/project/send.rs) |
| `NotFound` | no Store of that name, or no Store file at its path | [`project/add.rs`](../../../../control/crates/layerfs-store/src/project/add.rs) |
| `Exists` | the project's entry or its name in the Store exists (raised `NameTaken`) | same |
| `Busy` | another project is being added to the same Store | same |
| `Capacity` | worker route: the machine's free disk is below the copy plus headroom | [`project/send.rs`](../../../../control/crates/layerfs-store/src/project/send.rs) |
| `Unsupported` | the kernel guard refuses the Store's directory | [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs) |
| `Failed` | the deadline passed, a read error, or another engine failure | [`project/add.rs`](../../../../control/crates/layerfs-store/src/project/add.rs) |
| `Unknown` | worker route: a stage or the answer was lost; the entry stays `broken` `unresolved` until `project delete` | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |

Run on macOS (`macos-host`) with a debug build:

```console
$ layerfs project init shop/api /tmp/demo/src --progress
{"phase":"check","done":null,"total":null}
{"phase":"install","done":null,"total":null}
{"phase":"record","done":null,"total":null}
{"commit":"32919f5590656a0ee8daeae5a82fe8a3ecd320349b661147c5bc3b7f912997ae6c"}
```

### project.list

Every project in the home, or one Store's.

```text
CLI     layerfs project list [--store S] [--limit 100] [--cursor C] [--with stats]
Node    await layerfs.project.list({ store, limit, cursor, with })
Python  layerfs.project.list(*, store=None, limit=None, cursor=None, with_=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `store` option | name | list only this Store's projects |
| `limit` option | int | the most rows in one page, 1 to 1000; default `100` |
| `cursor` option | text | the `next_cursor` of the page before; it carries the filters only |

| Answer | Keys |
| --- | --- |
| short | `items`; when set: `next_cursor` |
| long | `items, next_cursor` |
| `items[]` short | `project`; when set: `state`, `op`, `blocked_by`, `reason`, `fix` |
| `items[]` long | `project, state, op, blocked_by, reason, fix, store, engine, machine, commit, created_at` |
| `--with stats` | adds `size` |
| call | pages by `limit` and `cursor`; writes one call-log line; MCP `admin` |

Pages by Store, then name; the `next_cursor` carries the `--store` filter
only, and one given with another filter, or to another call, is
`InvalidInput`. A row's short form is `{ project }` with its `state` and
companion when not `ready`; its long form adds `store`, `engine`, `machine`,
`commit` (main's head, null for a project whose Store was not read) and
`created_at`. A Store whose records say it is not ready is not asked: its
recorded projects are listed `blocked`. `--with stats` adds `size` to each
row: the length main's head publication records, one record per project and
no tree walk; it is left out for a blocked project
([`project/listing.rs`](../../../../control/crates/layerfs-store/src/project/listing.rs)).

```console
$ layerfs project list
{"items":[{"project":"shop/api"}]}
$ layerfs project list --with stats --detail
{"items":[{"project":"shop/api","state":"ready","op":null,"blocked_by":null,"reason":null,"fix":null,"store":"shop","engine":"sqlite","machine":"macos-host","commit":"32919f5590656a0ee8daeae5a82fe8a3ecd320349b661147c5bc3b7f912997ae6c","created_at":"2026-11-18T22:04:02Z","size":19}],"next_cursor":null}
```

### project.get

One project: its Store, state and main's head.

```text
CLI     layerfs project get <project>
Node    await layerfs.project.get(project)
Python  layerfs.project.get(project)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |

| Answer | Keys |
| --- | --- |
| short | `store`, `state`; when set: `commit`, `op`, `blocked_by`, `reason`, `fix` |
| long | `store, state, commit, op, blocked_by, reason, fix, project, engine, machine, created_at` |
| call | writes one call-log line; MCP `admin` |

One project's record and state; `commit` is main's head. A project whose
Store cannot be opened answers `blocked`, not a refusal. It takes no
`--with`.

```console
$ layerfs project get shop/api
{"store":"shop","state":"ready","commit":"32919f5590656a0ee8daeae5a82fe8a3ecd320349b661147c5bc3b7f912997ae6c"}
```

### project.stats

Entry counts and size of the tree at a Branch's head or a Commit.

```text
CLI     layerfs project stats <project> [--branch B] [--at C]
Node    await layerfs.project.stats(project, { branch, at })
Python  layerfs.project.stats(project, *, branch=None, at=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `branch` option | name | the Branch counted; default `main` |
| `at` option | name | read as of this Commit: a full id, or a prefix of 8 or more characters |

| Answer | Keys |
| --- | --- |
| short | `file_count`, `dir_count`, `symlink_count`, `size`; when set: `partial` |
| long | `file_count, dir_count, symlink_count, size, partial, commit, root` |
| call | writes one call-log line; MCP `admin` |

Counts files, folders and symlinks by a bounded walk of the tree at the
Branch's head (`main` by default) or, with `--at`, at one Commit of its
history. The pin is resolved once; uncommitted Workspace changes are not
counted. The root folder is excluded, hard links count per path and
symlinks are not followed. `size` is the length the counted node records,
less the files `project hide-path` hides. The walk shares the saved-read
work bound; past it the counts are partial and `partial: true`, never a
silent cap. An object read error is a refusal, not a partial answer
([`read/tree_stats.rs`](../../../../engine/crates/layerfs-store-service/src/read/tree_stats.rs)).

```console
$ layerfs project stats shop/api
{"file_count":2,"dir_count":1,"symlink_count":0,"size":19}
```

### project.export

Export a Branch's committed tree to a folder on this computer.

```text
CLI     layerfs project export <project> <destination> [--branch B] [--at C] [--progress]
Node    await layerfs.project.export(project, destination, { branch, at, on_progress })
Python  layerfs.project.export(project, destination, *, branch=None, at=None, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `destination` | text | an absolute path on this computer: absent, or an empty folder |
| `branch` option | name | the Branch exported; default `main` |
| `at` option | name | export this Commit instead of the Branch's head: a full id, or a prefix of 8 or more characters |
| `on_progress` option | function |  |

| Answer | Keys |
| --- | --- |
| short | `file_count`, `size` |
| long | `file_count, size, dir_count, symlink_count, commit, root` |
| call | sends progress; writes one call-log line; MCP `admin` |

Exports one committed tree to an ordinary folder on this computer, even for
a worker Store. It takes a Branch (`main` by default) and pins its head, or
with `--at` one Commit of its history; the node and root are pinned once, so
later writes and Workspace overlays are not in it. Short: `{ file_count,
size }` (`size` the bytes written); the long form adds `dir_count`,
`symlink_count`, `commit` (the exported node, its full id) and `root`.

`destination` must be absent or an empty real folder, outside the home, with
an existing parent. Everything is written into a private `0700` staging
folder beside it, then renamed onto it once complete; any failure removes
this call's staging folder only
([`project/export/`](../../../../control/crates/layerfs-store/src/project/export/mod.rs)).
Progress events are `{phase, done, total}`: `prepare`, `export` and
`finalize`, `done` the bytes written so far and `total` null; `finalize`
comes only once the folder is in place, and `export` events come at most
every 200 ms ([`export/writer.rs`](../../../../control/crates/layerfs-store/src/project/export/writer.rs)).

| Fidelity | Contract |
| --- | --- |
| Entries | every folder (empty ones too), `.git`, ignored files, dependencies and caches; exact file bytes through bounded buffers |
| Symlinks | the original target (relative, absolute or dangling); never followed |
| Metadata | permission bits (folder modes after their children) and modification times with nanoseconds, where the host supports them |
| Not kept | hard-link identity, ownership, ACLs, extended attributes, sparse allocation, creation time |

| Store | Reader | Writer |
| --- | --- | --- |
| sqlite on a locally executing machine | the caller's process | this computer |
| sqlite on a worker machine | that worker, streaming bounded entry and byte frames over one request | this computer |
| PostgreSQL | the caller's process | this computer |

| Refusal | When |
| --- | --- |
| `InvalidInput` | a bad path or Branch; a destination that is not empty (raised `NotEmpty`) or whose parent is not a folder (raised `NotDirectory`); an ambiguous prefix (`matches`) |
| `NotFound` | the Branch, the `--at` Commit or the destination's parent is missing |
| `Exists` | the destination is a file or symlink (not followed) |
| `Conflict` | host names collide (a case-insensitive filesystem); nothing is overwritten |
| `Unavailable` | a required worker is stopped or not reached; nothing starts it |
| `Capacity`, `Failed` | a local write or metadata failure; a Store format this build does not open |

Export only reads history: a lost reply changes nothing in the Store.

```console
$ layerfs project export shop/api /tmp/demo/export --branch agent --progress
{"phase":"prepare","done":null,"total":null}
{"phase":"export","done":0,"total":null}
{"phase":"export","done":14,"total":null}
{"phase":"finalize","done":14,"total":null}
{"file_count":2,"size":14}
```

### project.delete

Retire one project in its Store and remove its home entry.

```text
CLI     layerfs project delete <project>
Node    await layerfs.project.delete(project)
Python  layerfs.project.delete(project)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |

| Answer | Keys |
| --- | --- |
| short | each when set: `ok`, `home_entry_kept` |
| long | `ok, home_entry_kept, outcome, deleted_at` |
| call | writes one call-log line; MCP `admin` |

Retires one project in its Store and removes its home entry. The project's
publications, Branches, Commits and objects stay until a later
[`store.compact`](store.md#storecompact): the project is hidden from
listings and name resolution. The Store marker is the commit point: when the
home entry cannot be removed, the call still succeeds with
`home_entry_kept: true` and keeps its intent
(`store/<store>/delete/<project>.json`), so running it again finishes the
entry. Running it again also clears a `broken` `unresolved` project,
including the entry of an Init that never reached the Store
([`project/delete.rs`](../../../../control/crates/layerfs-store/src/project/delete.rs)).
The long form is `ok, home_entry_kept, outcome ("deleted"), deleted_at`.

| Refusal | When | Source |
| --- | --- | --- |
| `NotFound` | the Store or project is missing (or already retired) | [`project/delete.rs`](../../../../control/crates/layerfs-store/src/project/delete.rs) |
| `Busy` | a Sandbox of the project is recorded (`detail.related` lists each), or another call's `op` holds the project | same |
| `InUse` | a Workspace of the project is mounted (raised `MountsActive`) | [`history/admission.rs`](../../../../engine/crates/layerfs-store-service/src/history/admission.rs) |
| `Unknown` | worker route: the answer was lost; the intent stays | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |

### project.hide_path

Hide one absolute path from the project's whole history; frees no bytes.

```text
CLI     layerfs project hide-path <project> <path>
Node    await layerfs.project.hide_path(project, path)
Python  layerfs.project.hide_path(project, path)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `path` | text | an absolute path in the project |

| Answer | Keys |
| --- | --- |
| short | each when set: `ok`, `unchanged` |
| long | `ok, unchanged, outcome, path, frontier_serial, hidden_at` |
| call | writes one call-log line; MCP `admin` |

Hides one absolute path from the project's whole history. It writes one
marker, not a Commit, so it takes no `-m` and frees no bytes; reads of the
path afterwards are `NotFound`, and the marker's frontier governs later
re-creations. A path already hidden answers `{"unchanged":true}`. The long
form is `ok, unchanged, outcome ("hidden" or "unchanged"), path,
frontier_serial, hidden_at`. The path must be a normalized absolute POSIX
path of at most 4096 bytes and 256 components
([`path.rs`](../../../../core/crates/layerfs-history/src/contract/path.rs)).
It was `project.remove_file`, removed with no alias.

```console
$ layerfs project hide-path shop/api /x.txt
{"ok":true}
```

### project.doctor

Check one project's record; with --full, also read and check its data.

```text
CLI     layerfs project doctor <project> [--full] [--progress]
Node    await layerfs.project.doctor(project, { full, on_progress })
Python  layerfs.project.doctor(project, *, full=None, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `full` option | bool | also read and check everything the project's Branches reach; can take minutes |
| `on_progress` option | function |  |

| Answer | Keys |
| --- | --- |
| short | each when set: `ok`, `findings`, `not_checked` |
| long | `ok, findings, not_checked, status, counts, observed_at, tree` |
| `findings[]` short | `<thing>`, `check`, `severity`, `message`; when set: `fix`, `from` |
| `findings[]` long | `<thing>, check, severity, message, fix, from, evidence` |
| `not_checked[]` short | `<thing>`; when set: `check`, `blocked_by`, `reason` |
| `not_checked[]` long | `<thing>, check, blocked_by, reason` |
| call | sends progress; writes one call-log line; MCP `admin` |

One of [the doctors](../../control/doctor/README.md), with their one report
shape. Its checks, in order: `project.record` (the home's record of the
project and its folder read, and its Store on record), `project.listed` (the
Store's own list holds the project, by its stack id, or by its name for a
record without one; a record the Store does not list is a `notice` whose
`fix` `layerfs project delete <project>` removes the entry) and, with
`--full` only, `project.data`: everything main's head and each Branch's head
reach (what a mount can reach) is read and checked where the Store's calls
run. `--full` writes nothing to the Store: its visited set is a temporary
database outside the Store, removed when the walk ends, so memory does not
grow with the project. While it reads it sends `{phase: "data", done,
total}` events counting trees, then the `self` event with the project's
node.

Run on macOS (`macos-host`) with a debug build:

```console
$ layerfs project doctor shop/api
{"ok":true}
$ layerfs project doctor shop/api --full --detail
{"ok":true,"findings":[],"not_checked":[],"status":"ok","counts":{"ok":1,"notice":0,"warning":0,"error":0,"not_checked":0},"observed_at":"2026-11-18T22:05:32Z","tree":{"project":"shop/api","status":"ok","coverage":"full","checks":[{"check":"project.record","outcome":"pass","from":null},{"check":"project.listed","outcome":"pass","from":null},{"check":"project.data","outcome":"pass","from":null}],"blocked_by":null,"reason":null}}
```

## Family errors

Every `project` call except `list`, `get` and `doctor` can also meet the
Store refusals: `NotFound` for the Store or project; `Busy` or `Unresolved`
while the Store's record holds another operation
([`op.rs`](../../../../control/crates/layerfs-home/src/op.rs)); `Unavailable`
when the Store's worker is not recorded or not reached, with `fix: layerfs
machine start <machine>`; nothing starts a machine
([`place.rs`](../../../../control/crates/layerfs-store/src/place.rs)). The
catalog is in [errors and events](../calls/errors-and-events.md).

## Related

- [SDK index](README.md), [calls](../calls/README.md).
- [Stores and projects](../../control/stores-and-projects/README.md): records, placement, leases.
- [Project Init](../../engine/project-init/README.md) and the [Init flow](../../flows/init.md).
- [branch family](branch.md), [commit family](commit.md): a project's history.
- [Doctors](../../control/doctor/README.md).
