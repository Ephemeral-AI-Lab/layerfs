# sandbox calls

> **Status:** Current general guide for the LayerFS 2.0 call surface. Part of
> the `layerfs-js-sdk` skill. Outputs marked "seen" were printed by the built
> `layerfs` command on a throwaway home on 2026-11-18 (macOS, a `native`
> Sandbox; paths shortened only where marked); the rest is from the table,
> the spec rows and the manual, not run. `sandbox.close` (now `remove`) and
> `logs.sandbox` (now `sandbox.log`) are gone, with no alias.

```js
sandbox.create(project, name, { machine, image, mount, on_progress })
sandbox.list({ project, machine, limit, cursor })
sandbox.get(sandbox, { with: ["stats"] })
sandbox.remove(sandbox, { force, on_progress })
sandbox.files(sandbox, path?, { depth, limit, cursor, byte_offset, byte_length })
sandbox.log(sandbox, { limit, cursor })
sandbox.doctor(sandbox, { on_progress })
```

A Sandbox is named (unique in the home), bound to one project when created,
and runs one LayerFS daemon on one machine. It holds up to 16 Workspaces of
any Branches of its project. `sandbox.create` is the only call that starts
one; `workspace.mount(sandbox, branch)` mounts in the one it names. It runs
until `sandbox.remove`.

```js
import layerfs from "@layerfs/node";

const P = "shop/api";
await layerfs.sandbox.create(P, "build-1");
try {
  const { workspace } = await layerfs.workspace.mount("build-1", "agent");
  try {
    const run = await layerfs.workspace.exec(workspace, "timeout 110 npm run build");
    await layerfs.workspace.commit(workspace, { message: `step 20: npm run build (exit ${run.exit_code})` });
  } finally {
    await layerfs.workspace.unmount(workspace);
  }
} finally {
  await layerfs.sandbox.remove("build-1");
}
```

## State

A Sandbox's `state` comes from its record and its machine's record: `busy`
with `op { name, phase, started_at }` while a `create` or `remove` runs;
`broken` `unresolved` once one lost its answer and its process ended;
`broken` `missing` with no runtime; `blocked` with `blocked_by { machine,
reason }` while its machine is not ready; `broken` `unsupported` for the
retired docker backend; `broken` `lost` or `not_answering` when a `get`
found it so; else `ready`. Every broken Sandbox's `fix` is
`layerfs sandbox remove <sandbox>`.

## `sandbox.create`

Starts a named Sandbox of a project. With no `machine` it runs on its sqlite
Store's machine, or on the one machine attached to its PostgreSQL Store
(none attached, or several: `InvalidInput`; `store.attach` attaches one);
another machine than a sqlite Store's is `Unsupported`. `image` (a `sha256:` digest, a Docker image id or a Docker
reference) picks its root on every machine but macOS `native`, which refuses
it `Unsupported`; an image the machine does not hold is copied from the
host's Docker first, as [`machine.load_image`](machine.md) does, with its
refusals. With `mount: "B"` it also mounts Branch `B` and answers that
Workspace's `{ workspace, path }`; otherwise `{"ok":true}`.

```bash
$ layerfs sandbox create shop/api tests --progress                             # seen
{"phase":"start","done":0,"total":1}
{"phase":"start","done":1,"total":1}
{"ok":true}
```

Progress: with `image`, the copy's `inspect`, `lock`, `export`, `send`,
`unpack`; then `start` and, with `mount`, `mount`. The long form adds
`outcome`, `sandbox`, `project`, `machine`, `platform`, `image`, `backend`,
`route`, `state`, `created_at` and `forward_failed`.

| `error` | When | Do |
| --- | --- | --- |
| `Exists` | A Sandbox of that name is recorded in the home | Pick another name, or `sandbox.remove` the old one |
| `InvalidInput` | A bad name, or `native` / `docker` (seen: `a Sandbox is not named native: that names a built-in machine`) | Fix the name |
| `NotFound` | No such machine, Store or project | `machine.list`, `project.get` |
| `Unsupported` | Not the sqlite Store's machine; a platform that cannot run it; an image on macOS `native` | Use the Store's machine, or `store.move` the Store |
| `Unavailable` | The machine is stopped, or a server or Docker cannot be reached | `machine.start`; read `message` |
| the mount's own code | With `mount`: the Sandbox started and the mount was refused; the Sandbox is kept and named as `sandbox`, with `fix: layerfs workspace mount <sandbox> <B>` | Mount again, or fix the Branch |
| `Unknown` | The outcome is not known; the record keeps its `op`, uncertain | Stop and report; `sandbox.remove` clears it |

## `sandbox.list`

Every Sandbox in the home, or one project's or one machine's, from records;
it asks no machine or daemon. Rows: `sandbox`, `project`, the Workspaces the
home records (`[{ workspace, branch }]`) and the state when not ready;
`workspaces_not_listed: true` for a Sandbox whose Workspace records live in
its machine. Long adds `message`, `machine`, `platform`, `image`,
`created_at`. Paged by `limit` (default 100) and `cursor`.

```bash
$ layerfs sandbox list                                                         # seen
{"items":[{"sandbox":"tests","project":"shop/api"}]}
```

## `sandbox.get`

One Sandbox: its state, and its Workspaces from a live read of its daemon.
Short: `{ state }` with `workspaces` (each `{ workspace, branch }`, plus the
exec ids `running` in it and its `state` when not ready) and the state's
companion. A machine that does not answer gives `unknown` with `observed_at`
and `workspaces` null. Long adds, per Workspace, `path`, `commit`,
`base_layer`, `mounted_at`, `branch_moved`, `pending_merge`, `commands`
(`[{ exec, started_at, program, tag }]`) and `id`, and for the Sandbox
`message`, `sandbox`, `project`, `machine`, `platform`, `image`, `backend`,
`route`, `created_at` and `workspace_capacity` (16).

```bash
$ layerfs sandbox get tests                                                    # seen, no Workspace mounted
{"state":"ready"}
$ layerfs sandbox get tests --with stats                                       # seen
{"state":"ready","workspaces":[{"workspace":"tests:agent","branch":"agent"}],"stats":{"daemon":{"pid":75696,"started_at":"2026-11-18T22:05:16Z"},"usage_reason":"host_process","daemon_capacity":{"admission":{"pool_vacant":7,"pool_running":1,"reserved_vacant":1,"reserved_running":0},"retained":{"limit":64,"ttl_ms":3600000,"count":0,"dropped":0,"recorded":0}},"workspaces":[{"workspace":"tests:agent","overlay_bytes":77824,"usage_reason":"host_process"}]}}
```

`with: ["stats"]` adds `stats` (null unless ready): `daemon`
(`{ pid, started_at }`), `usage` (`{ cpu_ms, memory_bytes,
memory_peak_bytes, memory_file_bytes, pid_count, oom_kill_count }` of its
cgroup, or null with `usage_reason`: `host_process` on a macOS `native`
Sandbox), `daemon_capacity` and per-Workspace `overlay_bytes`.

## `sandbox.remove`

Stops a Sandbox and removes its record. Answers `{"ok":true}`; with `force`
also `unmounted` (the Workspaces it ended). Long `outcome`: `removed` or
`already_gone`, with `left` saying what may remain: its runtime was gone,
or only its records went (a Sandbox of the retired docker backend, one
whose record this build does not read, or one on a machine whose record
has no 2.0 name), with no signal sent. Progress: `cancel` (with `force`),
`stop`, `delete`.

```bash
$ layerfs sandbox remove tests                                                 # seen, exit 125
{"error":"InUse","message":"Sandbox tests still holds 1 Workspace(s); commit or unmount them first","workspaces":["tests:agent"]}
$ layerfs workspace unmount tests:agent                                        # seen
{"ok":true}
$ layerfs sandbox remove tests --progress                                      # seen
{"phase":"stop","done":1,"total":3}
{"phase":"delete","done":2,"total":3}
{"phase":"delete","done":3,"total":3}
{"ok":true}
```

| `error` | When | Do |
| --- | --- | --- |
| `InUse` | A Workspace is mounted and no `force`; flat `workspaces` | Commit or unmount them first |
| `Busy` | A command runs and no `force`; flat `running` (exec ids) | `workspace.cancel`, or wait |
| `Unknown` | `force`, and a command's cancel was not confirmed; flat `running` | Stop and report |

`force` cancels each running command, then stops the Sandbox anyway: its
Workspaces' uncommitted changes end with it. Commit first what you want to
keep. A lost answer keeps the record with its `op` uncertain (`broken`
`unresolved`) until another `remove`.

## `sandbox.files`

Reads what is in a Sandbox: its roots, one page of a folder, or one byte
window of a file. It takes no lock, asks no daemon, starts nothing and writes
nothing, the call log included.

- With no `path`: the roots (long form: each `role`): one per mounted
  Workspace (`not_read: "mount"`), its home, its daemon log and, for a
  Sandbox with a root of its own (every one but a macOS `native` Sandbox),
  `/`.
- A folder: `items` (`{ path, type, size, not_read }`, `type` `file`, `dir`,
  `symlink` or `other`; long adds `role, depth, mode, modified_at, target,
  fs, workspace`) to `depth` levels (default 1), `limit` a page (default
  100), with `next_cursor`.
- A file: a window from `byte_offset` (default 0) of `byte_length` (default
  262144): `text` and `next_byte_offset` while more remains, or
  `binary: true` with its `size`.
- The daemon log is refused `InvalidInput` (read it with `sandbox.log`), as
  are `..`, a link on the way, a FIFO, socket or device, and a private file.

```bash
$ layerfs sandbox files tests                                                  # seen
{"items":[{"path":"/tmp/demo/home/sandbox/tests/workspaces/agent","type":"dir","not_read":"mount"},{"path":"/tmp/demo/home/machine/macos-host/run/layerfs-8a7387b2-tests/tmp","type":"dir"},{"path":"/tmp/demo/home/machine/macos-host/run/layerfs-8a7387b2-tests/daemon.log","type":"file","size":38}]}
```

## `sandbox.log`

The Sandbox's daemon log, `daemon.log` in its run folder: `items`
(`[{ text, continued? }]`, oldest first; `continued` marks a piece of a
longer line), the last `limit` lines (default 200, 1 to 1000), or with
`cursor` the lines before it, at most 64 KiB a call. `next_cursor` continues
with older lines and is absent at the start of the log. Control characters
are escaped. Like `files`, it takes no lock and writes nothing; at most two
inspection reads run on a machine at a time (`Busy`), and a machine that is
stopped or does not answer within 8 s is `Unavailable`.

```bash
$ layerfs sandbox log tests --limit 2                                          # seen
{"items":[{"text":"LAYERFS_DAEMON_LISTEN 127.0.0.1:62804"}]}
```

## `sandbox.doctor`

Checks one Sandbox (record, runtime, daemon), then each of its Workspaces.
The report is the [doctors'](doctor.md) shared shape.

```bash
$ layerfs sandbox doctor tests                                                 # seen
{"ok":true}
```

## Clearing idle Workspaces

Nothing unmounts a Workspace for you: a script that exits or throws before its
`unmount` leaves it mounted, and its Sandbox cannot be removed. To clear
leftovers, look at each Workspace and unmount only those that are idle, hold
nothing uncommitted and that no agent is keeping on purpose (a build
Workspace, one running a dev server):

```js
const keep = new Set([/* handles agents said they keep */]);
for (const w of (await layerfs.workspace.list({ limit: 1000 })).items) {
  if (keep.has(w.workspace)) continue;
  const s = await layerfs.workspace.status(w.workspace);
  if (!s.state && (s.changes ?? []).length === 0 && !s.partial) {
    await layerfs.workspace.unmount(w.workspace);   // no discard: a Dirty one stays for its owner
  }
}
```

Don't pass `discard` for a Workspace another agent mounted. Without it, a
Workspace with uncommitted changes is refused `Dirty` and stays mounted.
