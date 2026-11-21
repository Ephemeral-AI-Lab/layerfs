# project calls

> **Status:** Current general guide for the LayerFS 2.0 call surface. Part of
> the `layerfs-js-sdk` skill. Outputs marked "seen" were printed by the built
> `layerfs` command on a throwaway home on 2026-11-18 (ids shortened); the
> rest is from the table, the spec rows and the manual, not run.
> `project.remove_file` is gone (now `project.hide_path`), with no alias.

A project is imported once from a host folder into an existing Store, and is
always named with its Store: `<store>/<project>`, such as `shop/api`. A bare
`api` is refused `InvalidInput`.

```js
project.init(project, folder, { timeout, on_progress })
project.list({ store, limit, cursor, with: ["stats"] })
project.get(project)
project.stats(project, { branch, at })
project.export(project, destination, { branch, at, on_progress })
project.delete(project)
project.prune(project)
project.hide_path(project, path)
project.doctor(project, { full, on_progress })
```

A project's `state` is `ready`, `busy` (an Init or a delete of it runs:
`op.name` `init` or `delete`), `blocked` (its Store, or that Store's machine,
is not ready: `blocked_by`) or `broken` `unresolved` (a delete or an Init
stopped part-way, with `fix: layerfs project delete <project>`).

## `project.init`

Imports the absolute host folder `folder` as a new project into the Store
named before the `/`. Init never creates a Store: make one first with
`store.create` (a missing one is `NotFound`). Everything in the folder is
imported, including `.git` and ignored files. `timeout` is in milliseconds
(default 3600000, one hour). Short: `{ commit }`, `main`'s first
publication; long: `commit, outcome ("created"), project, store, engine,
machine, root, created_at`.

```js
import layerfs from "@layerfs/node";
await layerfs.store.create("shop");
const { commit } = await layerfs.project.init("shop/api", "/work/api", { on_progress: (e) => console.log(e) });
```

```bash
$ layerfs store create shop                                                    # seen
{"ok":true}
$ layerfs project init shop/api /tmp/demo/src --progress                       # seen
{"phase":"check","done":null,"total":null}
{"phase":"install","done":null,"total":null}
{"phase":"record","done":null,"total":null}
{"commit":"32919f55…"}
```

The new project has one line, `main`, and no Branch: fork one before you
change anything. Into a sqlite Store on `native` or a PostgreSQL Store, Init
runs in your process. Into a sqlite Store on another machine (which must be
running), the folder is copied to that machine's worker (progress `send`, in
bytes), which runs Init on the copy and deletes it: the machine needs free
room for the copy (`Capacity` otherwise).

| `error` | When | Do |
| --- | --- | --- |
| `NotFound` | No Store of that name | `store.create`, or check the name |
| `Exists` | The home already holds that project | Don't init again |
| `InvalidInput` | A bad name, a relative path, a bare project name; a folder with a socket, FIFO or device file | Fix the call |
| `Unavailable` | The Store's machine is stopped | `machine.start`, then Init |
| `Unknown` | The outcome is not known; the project stays `broken` `unresolved` | Stop and report; `project.delete` clears it |

**Start from a small skeleton** (shell), with a `.gitignore` and the
[`/.lww`](../rules/lww.md) it needs:

```bash
mkdir -p /work/api/.git
printf 'ref: refs/heads/main\n' > /work/api/.git/HEAD
printf '# API\n' > /work/api/README.md
printf 'node_modules/\ndist/\ncoverage/\n*.log\n' > /work/api/.gitignore
printf '%%include .gitignore\n' > /work/api/.lww
```

## `project.list`

Every project in the home, or one Store's (`store`), paged by Store then
name (`limit` default 100). A row's short form is `{ project }` with its
state and companion when not ready; long adds `store`, `engine`, `machine`,
`commit` (`main`'s head) and `created_at`. `with: ["stats"]` adds each row's
`size` (bytes at `main`'s head, from its record; no tree walk).

```bash
$ layerfs project list                                                         # seen
{"items":[{"project":"shop/api"}]}
$ layerfs project list --with stats --detail                                   # seen
{"items":[{"project":"shop/api","state":"ready","op":null,"blocked_by":null,"reason":null,"fix":null,"store":"shop","engine":"sqlite","machine":"native","commit":"32919f55…","created_at":"2026-11-18T22:04:02Z","size":19}],"next_cursor":null}
```

## `project.get`

Short: `{ store, state }` with `commit` (`main`'s head) and the state's
companion; long adds `op, blocked_by, reason, fix, project, engine,
machine, created_at`.

```bash
$ layerfs project get shop/api                                                 # seen
{"store":"shop","state":"ready","commit":"32919f55…"}
```

## `project.stats`

Counts files, folders and symlinks by a bounded walk of the tree at a
Branch's head (`branch`, default `main`) or at one Commit (`at`). Short:
`{ file_count, dir_count, symlink_count, size }`, with `partial: true` when
the walk stopped at its bound; long adds `partial, commit, root`.
Uncommitted Workspace changes are not counted.

```bash
$ layerfs project stats shop/api                                               # seen
{"file_count":2,"dir_count":1,"symlink_count":0,"size":19}
```

## `project.export`

Exports a Branch's committed tree (`branch`, default `main`; or `at`) to
`destination`, an absolute path on this computer that is absent or an empty
folder, outside the home. Short: `{ file_count, size }`; long adds
`dir_count, symlink_count, commit, root`. Progress: `prepare`, `export`
(`done` the bytes written), `finalize`.

```bash
$ layerfs project export shop/api /tmp/demo/export --branch agent --progress   # seen
{"phase":"prepare","done":null,"total":null}
{"phase":"export","done":0,"total":null}
{"phase":"export","done":14,"total":null}
{"phase":"finalize","done":14,"total":null}
{"file_count":2,"size":14}
```

## `project.delete`

Retires one project in its Store and removes its home entry; its history
stays until a later `store.compact`. Short: `{"ok":true}`, or
`{"home_entry_kept":true}` when the entry could not be removed (run it
again). Refused `Busy` while a Sandbox of the project is recorded or another
call's `op` holds it.

## `project.prune`

Removes a real folder under `store/<store>/project/` that has no
`record.json`: the leftover of an Init that never reached the Store, or of a
delete from another home. It reads the home only, takes its lock and opens
no Store. A Store the home does not record is `NotFound`; an entry on
record (valid, broken or unreadable) is `Exists` with
`fix: layerfs project delete <project>`; a symlink is `InvalidInput`. An
entry that exists only because a project is on record is `delete`'s, never
`prune`'s. `project init` of such a name answers `NameTaken` with the prune
line. Short: `{ ok: true }`; long: `ok, outcome ("pruned"), project, store`.

```js
await layerfs.project.prune("shop/orphan");
```

## `project.hide_path`

Hides one absolute path from the project's whole history. It writes a
marker, not a Commit, and frees no bytes. Short: `{"ok":true}`, or
`{"unchanged":true}` for a path already hidden. Later reads of the path are
`NotFound`; a later write of it is visible.

```bash
$ layerfs project hide-path shop/api /x.txt                                    # seen
{"ok":true}
```

## `project.doctor`

Checks the project's record against its Store's own list; with `full`, also
reads and checks everything the heads of `main` and each Branch reach (it
can take minutes, sends `{ phase: "data", done, total }` progress, and
writes nothing). The report is the [doctors'](doctor.md) shared shape.

```bash
$ layerfs project doctor shop/api                                              # seen
{"ok":true}
```

An agent working on a project never needs `delete`, `prune` or `hide_path`; they are
for whoever owns the home.
