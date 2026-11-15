# sandbox family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `sandbox` calls: start, list, read and stop a named Sandbox, one project's Workspace daemon on a machine, and read the files in it, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/sandbox`](../../../../api/crates/layerfs-api/src/sandbox), [`layerfs-sandbox/src`](../../../../control/crates/layerfs-sandbox/src) (`family.rs`, `create.rs`, `list.rs`, `get.rs`, `close.rs`, `lifecycle.rs`, `inspect/files.rs`, `inspect/roots.rs`) |
| Runs by default | `list`: the caller's process, home records only. `create`, `get`, `close`: records and locks in the caller's process, then the Sandbox's machine: this process for a local machine, else the machine's worker (`sandbox.host`); `get` may dial a forwarded daemon directly. `files`: where the Sandbox's run folder is, on the [inspect route](../../control/sandboxes/inspect.md), with no lock, lease or daemon |
| Design | [Sandboxes](../../control/sandboxes/README.md), [inspection](../../control/sandboxes/inspect.md), [daemon channel and exec](../../control/sandboxes/daemon-channel-and-exec.md), [backends](../../control/sandboxes/backends.md) |

## Calls

```text
 project.init --> create <project> <name> [machine]  (starts the daemon)
                     |
                     +--> list / get                (records, Workspaces)
                     +--> files                     (its roots, its home; a read)
                     +--> workspace.mount ... unmount
                     v
                  close <name>   (ends the daemon; clears an unresolved
                                  create or close)
```

| Call | Returns |
| --- | --- |
| [`sandbox.create`](#sandboxcreate) | the Sandbox record, `state: "running"`, `forward_failed?` |
| [`sandbox.list`](#sandboxlist) | `{ items: [record], next_page_pos }`; with `live`, each item also `live`, `live_reason?` |
| [`sandbox.get`](#sandboxget) | the record, live `state`, one page of `workspaces` (each with `running` and `overlay_bytes`), `workspace_capacity`, `daemon`, `usage`, `usage_reason?`, `down_reason?`, `down?`, `daemon_capacity?` |
| [`sandbox.close`](#sandboxclose) | how it ended |
| [`sandbox.files`](#sandboxfiles) | `{ kind: "folder", entries, next, has_more, reason? }` or `{ kind: "file", size, offset, content, binary, next }` |

The **record** is `SandboxRecord::to_sdk`
([`record/sandbox.rs`](../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs)):
the stored fields `name`, `project`, `machine`, `created`, `id`, `platform`,
`runtime`, `image`, `backend`, `route` (never `v`, `runtime_id` or
`daemon`), the derived `state` (`creating`, `closing`, `unresolved`, `gone`,
`lost` or `running`) and, while an operation holds it, `op: { id, name,
phase, started, uncertain }`
([`op.rs`](../../../../control/crates/layerfs-home/src/op.rs)). A folder scope
refuses every `sandbox` call (`out of scope: sandbox.<call>`).

### sandbox.create

Starts a named Sandbox for one project on a machine, ready for
`workspace.mount`.

```text
JS      await layerfs.sandbox.create(project, name, machine?, { image, on_progress })
Python  layerfs.sandbox.create(project, name, machine=None, *, image=None, on_progress=None)
CLI     layerfs sandbox create <project> <name> [<machine>] [--image <image>] [--progress]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `name` | Name | unique across the home; not `native` or `docker` (built-in machine names); not ending in `.root` (`<name>.root` is a Sandbox's private root in the run folder) |
| `machine` (optional) | Name | default a sqlite Store's machine, else `native`; a sqlite Store's Sandbox must run on that machine, and the support table's `sandbox_allowed` must accept the platform and Store kind |
| `image` option | Image | a digest, Docker image Id or Docker reference: one the machine holds (unpacked, or in its archive) starts at once; any other is copied from the host's Docker first, as [`machine.load`](machine.md#machineload) does (progress `inspect`, `export`, `send`, `unpack`, then `start`). Default the machine record's image (not on `docker`), else the archive's; refused on macOS `native` ([`deploy.rs`](../../../../control/crates/layerfs-sandbox/src/deploy.rs), [`image_load.rs`](../../../../control/crates/layerfs-sandbox/src/image_load.rs)) |

| Field | Meaning |
| --- | --- |
| `platform` | `docker` (the docker machine), `mac` or `linux` ([`runtime.rs`](../../../../control/crates/layerfs-sandbox/src/runtime.rs)) |
| `runtime`, `backend` | `native` (macOS host process) or `namespaces` (Linux, the `docker` machine's worker container included) |
| `image` | the namespaces root's id (a Docker image Id or a config digest), or `local`; each create sets that image's sidecar `last_used` |
| `route` | `nfs` for a macOS host process, else `fuse` ([`files.rs`](../../../../control/crates/layerfs-sandbox/src/files.rs)) |
| `forward_failed` | why the caller could not forward the daemon port; Workspace calls then go through the worker ([`sandbox/create.rs`](../../../../api/crates/layerfs-api/src/sandbox/create.rs)) |

The name is claimed under the home lock with an `op` intent
(`sandbox.create`, phases `claim`, `start`), the machine starts the daemon,
and the record is written once with the answer. For a PostgreSQL Store the
caller resolves the password and sends the URL only in this request; the
launching side checks that it reaches the server and holds the project
first. Nothing starts a stopped machine.

| Error | When |
| --- | --- |
| `InvalidInput` | `native`/`docker` as the name, or a name ending in `.root`; an image Docker does not have |
| `NoSpace` | copying the image needs `2 × size × 1.05` bytes and the machine has fewer (`detail { need, free }`) |
| `Unavailable` | the machine is stopped (`detail { what: "machine" }`) or Docker cannot be read (`detail { what: "docker" }`) |
| `NotFound` | no machine of that name |
| `NoStore` | the project has no installed Store |
| `Exists` | the name is taken, or the machine already records a started Sandbox of it |
| `Unsupported` | the platform cannot run this Store kind; a sqlite Store on another machine; no image archive in the bundle; an image on macOS `native` (`detail { reason: "host" }`) or for another architecture (`detail { reason: "arch", value }`) |
| `Failed` | the start failed and every effect is known: the Sandbox was stopped and deleted (`detail { container, removed, log }`, [`launch/local.rs`](../../../../control/crates/layerfs-sandbox/src/launch/local.rs) `abandon`) |
| `Unknown` | the worker's answer was lost, the start's effect is unknown, or it started but could not be recorded: the record keeps its `op` with `uncertain: true` and `state` is `unresolved` |

### sandbox.list

The Sandboxes this home records, or one project's, from records alone.

```text
JS      await layerfs.sandbox.list(project?, { page_size, page_pos, live })
Python  layerfs.sandbox.list(project=None, *, page_size=None, page_pos=None, live=None)
CLI     layerfs sandbox list [<project>] [--page-size <page-size>] [--page-pos <page-pos>] [--live]
```

`page_size` is clamped to 1..=1000 (default 50); a negative `page_pos`
counts as 0 ([`family.rs`](../../../../control/crates/layerfs-sandbox/src/family.rs) `page`).
Each item is the record (`View::List`). No machine, worker or daemon is
asked, so `state` is `running` for any started Sandbox even when its machine
is down, and never `lost`; `sandbox.get` asks
([`list.rs`](../../../../control/crates/layerfs-sandbox/src/list.rs)).

With `live` (layerfs-project#164) each item of the page also says whether
its daemon process runs now. `state` is unchanged, and without the option
the answer is what it was, byte for byte.

| Field | Meaning | Source |
| --- | --- | --- |
| `live` | `true`: the process the Sandbox's daemon record names is that process (pid, start time and boot) and runs. `false`: it has ended, or the side that runs the Sandbox has no record or no daemon record of it. `null`: not known | [`inspect/live.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/live.rs), [`inspect/process.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/process.rs) `live` |
| `live_reason` | only with `live: null`: `machine_stopped` (its machine has a worker and records none running), `deadline` (the request's soft budget ended before its turn), `no_pid` (the daemon record names no process), `record_unreadable`, `not_answered` (the machine's answer did not name it), or the code of the refusal the machine's request met, in snake_case (`unavailable`, `busy`, `invalid_input` from a worker of a build without the kind) | `live.rs` `Live` |

The check is one [inspect](../../control/sandboxes/inspect.md) request per
machine among the page's Sandboxes, kind `sandboxes_live`, asked at once
when there are several: in this process on `native`, else of the machine's
worker, which reads its own records (a forwarded Sandbox included). No
daemon is asked, nothing starts a machine and a machine that does not answer
is `live: null`, never an error of the list. `--live` is a `sandbox.list`
like any other in the call log.

```text
$ layerfs sandbox list --live
{"items":[{"name":"s1","project":"shop/api","machine":"native",…,"state":"running","live":true},{"name":"s2","project":"shop/api","machine":"dev",…,"state":"running","live":null,"live_reason":"machine_stopped"}],"next_page_pos":null}
```

### sandbox.get

One Sandbox's record, its live state and one page of the Workspaces its
daemon holds, starting nothing.

```text
JS      await layerfs.sandbox.get(name, { page_size, page_pos })
Python  layerfs.sandbox.get(name, *, page_size=None, page_pos=None)
CLI     layerfs sandbox get <name> [--page-size <page-size>] [--page-pos <page-pos>]
```

```text
{ ...record, state,
  workspaces: [ { ws, name, path, workspace, namespace, branch, base,
                  base_layer, root, activity, mounted, directory,
                  head_moved_since_mount, dirty?, pending_merge, revision,
                  running: [ { exec_id, started, tag, program } ] | null,
                  overlay_bytes: <int> | null } ],
  workspace_capacity,
  daemon: { pid, started_at } | null,
  usage: { cpu_usec, memory_current, memory_peak, pids } | null,
  usage_reason?, next_page_pos, down_reason?, down?, daemon_capacity? }
```

| Field | Meaning | Source |
| --- | --- | --- |
| `state` | derived with the machine's answer: `lost` when the record has an id, no close intent, and the machine runs no such Sandbox (`sandbox.close` clears it) | [`record/sandbox.rs`](../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs) |
| `workspaces[].ws` | `<sandbox>:<name>` when the mount was named, else `<sandbox>:<path>`; the rest is the daemon's read model | [`get.rs`](../../../../control/crates/layerfs-sandbox/src/get.rs), [`operator/inspect.rs`](../../../../engine/crates/layerfs-store-service/src/operator/inspect.rs) |
| `workspaces[].running` | the commands running in the Workspace now, oldest first, whichever client started them: the exec records of the side that keeps the Workspace's record (the home on `native` and for a forwarded Sandbox, the worker root otherwise) whose owner process is the one recorded and still runs (pid, start time and boot), the rule `unmount` and `close` refuse `Busy` by. `started` is ms since the epoch; `tag` is the log tag of the `workspace.exec` call that started it (`log_tag`, else `LAYERFS_LOGS_TAG`) or null; `program` is the command's first word that is not a `NAME=value` assignment, at most 64 characters, or null: never an argument or an environment value. A command that ended has no record; a stale record (its owner gone) is not listed and not touched. `null` when the records could not be read. A record written before these fields lists with `tag` and `program` null | [`exec/record.rs`](../../../../control/crates/layerfs-sandbox/src/exec/record.rs) `running`, `program`; [`dispatch.rs`](../../../../control/crates/layerfs-sandbox/src/dispatch.rs) `listed` |
| `workspaces[].overlay_bytes` | the size in bytes of the Workspace's overlay file, `<run folder>/overlay/<namespace>.sqlite`, from its `lstat` on the side that has the run folder; the file is never opened. A high-water mark: it includes the template's size and grows until unmount (the overlay is never vacuumed). `null` where that side did not answer: always for a forwarded Sandbox in this build (its run folder is in its machine), and when the file is not there | [`run_folder.rs`](../../../../control/crates/layerfs-sandbox/src/run_folder.rs) |
| `workspace_capacity` | how many Workspaces one Sandbox holds at once: control's constant `WORKSPACES` (16), which a mount into a full Sandbox is refused `Capacity` by. Not a daemon setting, so it is there whatever the Sandbox's state | [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) |
| `daemon` | the daemon process as the side that runs it sees it: `pid`, and `started_at` in ms since the epoch while that process still runs (`null` once it has ended, or when the start time could not be turned into a time). macOS records the start exactly; Linux derives it from the boot time and clock ticks, to about a second. `null` when that side did not answer (a stopped machine, a worker that was not reached) or records no process | [`inspect/process.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/process.rs) `shown`, `started_at` |
| `usage` | what the Sandbox's own cgroup (`/sys/fs/cgroup/layerfs/<id>`, a `namespaces` Sandbox) has used: `cpu_usec` (`cpu.stat` `usage_usec`), `memory_current`, `memory_peak`, `pids` (`pids.current`), each from one small file and each `null` where the kernel keeps no such file there (a controller that is not delegated to the cgroup). It covers the daemon and every command run in the Sandbox. `null` with `usage_reason` otherwise | `process.rs` `usage` |
| `usage_reason` | only with `usage: null`: `host_process` (a `native` Sandbox is a plain process with no cgroup of its own), `cgroup_v1` (the machine has no cgroup v2 tree), `no_cgroup` (the Sandbox's cgroup is not there, as after its daemon ended), `not_observed` (the side that runs it did not answer) | `process.rs` |
| `down_reason` | the worker or forwarded daemon was not reached, or the daemon's control pool had no vacant slot; `workspaces` is then `[]` | `get.rs` |
| `down` | partial counts from the daemon's pre-handshake capacity notice: `code`, `resource`, `pool_vacant`, `reserved_vacant`, `retained`; absent when the refusal carried none | [`direct.rs`](../../../../control/crates/layerfs-sandbox/src/direct.rs) `unanswered` |
| `daemon_capacity` | the daemon's `backend.daemon.connection_failures` read model, read once, best effort, within 2 s on the connection the Workspace list lent; absent when not read | [`daemon/capacity.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/capacity.rs), [`control_capacity.rs`](../../../../engine/crates/layerfs-bridge/src/control_capacity.rs) |

`running`, `overlay_bytes` and `workspace_capacity` (layerfs-project#178)
add no daemon call, lock, lease, Store open or write to a get: `running`
and `overlay_bytes` are put on each Workspace where the Workspace list is
answered ([`dispatch.rs`](../../../../control/crates/layerfs-sandbox/src/dispatch.rs)
`listed`), from files read by path. A machine whose worker predates them
answers Workspaces without `running` and `overlay_bytes`; the get shows
them without, so a client reads an absent field as "not known" and a
present `null` as "not observed".

`daemon` and `usage` (layerfs-project#164) add none either, and no round
trip: they are read with `overlay_bytes`, by the side that has the run
folder, and ride the same answer
([`run_folder.rs`](../../../../control/crates/layerfs-sandbox/src/run_folder.rs)
`observed`). For a forwarded Sandbox that is the one `overlay_sizes`
inspect request a get already sends, now sent with no Workspaces too. A
Sandbox whose Workspace list was not answered shows the home's own record of
its process on `native` (a `pid`, `started_at: null` once it has ended) and
`daemon: null`, `usage: null`, `usage_reason: "not_observed"` elsewhere. A
worker that predates the fields answers without them and the get shows none.

A machine that is down is not an error: it is `down_reason`. A Sandbox with
an uncertain `op` is shown, not refused; `get` is the call `Unresolved`
refusals name. Errors: `NoSuchSandbox`; `Busy` or `StaleSandbox` (a
forwarded daemon admits no connection in time, or a saved daemon is not
ready, [`daemon/pool.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs));
`Capacity` other than the pool's own notice; `Failed` (the daemon's list
failed); `Unknown` (the worker's answer was lost; no effect for a read).

### sandbox.close

Stops a Sandbox and frees its name; also the call that clears an uncertain
`sandbox.create` or `sandbox.close`.

```text
JS      await layerfs.sandbox.close(name, { force })
Python  layerfs.sandbox.close(name, *, force=None)
CLI     layerfs sandbox close <name> [--force]
```

`force` stops it even with Workspaces mounted or commands running: each
running command is cancelled first (5000 ms grace), and uncommitted changes
and processes end with the Sandbox
([`exec/cancel.rs`](../../../../control/crates/layerfs-sandbox/src/exec/cancel.rs) `settle`).
On `native`, a forced close works from the Sandbox's own records instead: it
opens no Store and calls no daemon, checks the runtime's identity, ends its
mounts and processes, and answers every recorded or held Workspace as
`unknown` ([`force.rs`](../../../../control/crates/layerfs-sandbox/src/force.rs)).
It does not settle commands one by one or check mounts first: the runtime's
stop ends them. It is `Busy` while a live process holds `native` (a forced
`machine.stop`, for one), but not for an unresolved intent, whether an ended
process or this one left it. If the
runtime stopped and a later step (delete, folder removal) failed, the refusal
carries `detail.runtime_stopped: true` and the record is marked stopped.

| Case | Answer |
| --- | --- |
| a started Sandbox, stopped | `{ kind: "Closed", sandbox, unknown: [path], closed }`; `unknown` lists, with `force`, the Workspaces still mounted, whose state is not confirmed |
| the machine says it is already gone | `{ closed, was_gone: true }` |
| a record of the retired docker backend | `{ closed, forgotten: true, container, remove: "docker rm -f <container>", left? }`: forgotten, not stopped |
| a record with no id (never started, or create's outcome unknown) | `{ closed, was: "gone" \| "unresolved", left? }` |

The shapes are built in
[`sandbox/close.rs`](../../../../api/crates/layerfs-api/src/sandbox/close.rs).
The intent `sandbox.close` (phase `stop`) is written before the runtime is
stopped (5 s), then deleted, each once. On `Closed` or an already-gone
Sandbox the home record is removed; on `Unknown` it is kept with its `op`
marked uncertain; on any other refusal the `op` is dropped and the Sandbox
is as it was ([`close.rs`](../../../../control/crates/layerfs-sandbox/src/close.rs)).

| Error | When |
| --- | --- |
| `Busy` | another live operation holds the Sandbox; an unmount is in progress; without `force`, commands still run (`detail.execs`) |
| `MountsActive` | without `force`, Workspaces are mounted (`detail.workspaces`) ([`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs)) |
| `Failed` | the stop or delete failed; a failed stop is not followed by a delete |
| `Unknown` | the machine's answer was lost: the Sandbox may be stopped or not; or with `force` a command's cancel was not confirmed (`detail.execs`) |

### sandbox.files

Reads what is in a Sandbox: its roots, one page of a folder, or one window
of a file, in its home and, for a Sandbox with a root of its own, in that
root. It is a read for a UI that shows a tree
(layerfs-project#178): no lock, no lease, no daemon call, nothing written,
no call-log line (the call is `quiet`), and the MCP server does not list it.

```text
JS      await layerfs.sandbox.files(sandbox, path?, { depth, cursor, limit, offset, length })
Python  layerfs.sandbox.files(sandbox, path=None, *, depth=None, cursor=None, limit=None, offset=None, length=None)
CLI     layerfs sandbox files <sandbox> [<path>] [--depth <depth>] [--cursor <cursor>] [--limit <limit>] [--offset <offset>] [--length <length>]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `path` (optional) | Text | none: the roots. Else an absolute path as the Sandbox's commands see it, with no `..`: under the Sandbox's home, or, when the roots offer `/`, anywhere in its root |
| `depth` option | Int | how many levels of a folder a page walks: 1 (default) to 32 |
| `limit` option | Int | the most entries of a page: 1 to 1000, default 500 |
| `cursor` option | Text | a page's `next`: the entries after it. An entry's path under `path`, at most `depth` levels down |
| `offset`, `length` options | Int | a file window: from byte `offset` (default 0), at most `length` bytes (1 to 1 MiB, default 256 KiB) |

An option that does not apply to what `path` is (a `limit` for a file, an
`offset` for a folder) is checked for its range and not used.

**The roots** (no `path`): a folder page that is never continued, each entry
with a `role`, in this order.

| `role` | Entry | From |
| --- | --- | --- |
| `workspace` | one per mounted Workspace: `kind: "mount"`, `path` its mount (`<run folder>/workspaces/<n>`), `workspace` its id; `size`, `mode` and `mtime` null. Ordered by path, `2` before `10` | the Workspace records on the side that keeps them (the home for `native` and for a Sandbox whose daemon the home reaches directly, which sends them with the request; else the worker's root). The mount is never `stat`ed or opened: a stopped daemon cannot hang the answer |
| `home` | `kind: "folder"`, `path` `<run folder>/tmp`: `HOME` for the Sandbox's commands | the run folder |
| `log` | `kind: "file"`, `path` `<run folder>/daemon.log`, with its `size`. It is read with [`logs.sandbox`](logs.md#logssandbox), not with this call | the run folder |
| `root` | `kind: "folder"`, `path` `/`: what the Sandbox's commands see as `/`. Only for a Sandbox with a root of its own (the `namespaces` backend: native Linux, docker, Lima, Apple Container); `size`, `mode` and `mtime` null | the Sandbox's record: nothing of the root is opened for this answer, so a stopped Sandbox lists it too |

A client offers a second view ("whole filesystem") only when the answer has
the `root` entry. A host-process Sandbox (native macOS) has none: its
commands see the owner's computer, and a path outside its home is
`InvalidInput`.

**The root** (a path outside the home, when the roots offer `/`) is read
through the root of the Sandbox's init process, on the machine that runs it
([`inspect/root.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/root.rs)):

| | In the root |
| --- | --- |
| Links | one on the way resolves inside the root (`/lib` of a merged-`/usr` image); the last name is never followed |
| `system` | `/proc`, `/sys`, `/dev`: listed, never `stat`ed or entered |
| Mounts | entered when the type is `ext4`, `xfs`, `btrfs`, `zfs`, `f2fs`, `overlay` or `tmpfs` (the private `/tmp`, `/.layerfs/runtime`, the run folder and a bound Store's folder); any other is `mount` with its `fs`. A Workspace mount is `mount` with `workspace`, its id, and is never `stat`ed or entered |
| What is opened | every file and folder: the Sandbox's files are its own, and there is no world-readable rule |
| `restricted` | the run folder and a bound Store's folder are in the root at their own paths: the run folder's `config/` and `overlay/`, `store.sqlite*`, locks and leases are listed and never read; `daemon.log` is a `log`. A path that a link leads to one of them is refused too: where the open ended is judged, not only its names |
| Hard links | read, but for a file in the run folder that the command user does not own: an image's files keep their links (busybox applets) |

A path under the home is read in the home's own view in every case, which
needs no running Sandbox. A stopped Sandbox's root is `Unavailable`.

**A folder** answers `{ kind: "folder", entries, next, has_more, reason? }`.
`entries` is a flat list in tree order: names in byte order, and a folder's
own entries right after it, down `depth` levels. `next` is the path of the
last entry (null for an empty first page); while `has_more`, the same call
with `cursor: next` answers the entries after it, whatever was added or
removed in between. `reason` is `limit` (the page is full) or `deadline`
(the read used its 4 s; the page holds what it had, which may be nothing).

| Entry field | Meaning |
| --- | --- |
| `path` | the entry's absolute path, usable as `path` or `cursor` |
| `depth` | 1 for an entry of the folder asked for, 2 for an entry of one of its folders |
| `kind` | `file`; `folder`; `symlink` (with `target`, what it names; never followed or entered); `mount` (another filesystem is mounted there; with `fs`, its type, when the mount table gave it; never `stat`ed where the table names it, never entered); `restricted` (LayerFS keeps it private: listed, never entered or read); `other` (a FIFO, socket or device). `system` and `unreadable` are kinds of a machine's `/` ([`machine.files`](machine.md#machinefiles)) |
| `size` | bytes, for a regular file (a `restricted` one included); else null |
| `mode` | the permission bits as a number (`0o644` is 420); null for a `mount` the table named |
| `mtime` | last modification, milliseconds since 1970; null as `mode` |

A name that is not UTF-8 is not listed.

**A file** answers `{ kind: "file", size, offset, content, binary, next }`.
`content` is the window as text: bytes that are not UTF-8 are U+FFFD, and
every control character but a line break and a tab is written as its escape
(`\r`, `\u{1b}`), as a log line's are. A window that would end inside a
character ends before it, so windows read from each `next` join to the
file's text. `binary` is true when the window holds a NUL byte: `content`
is then empty, and a client shows the file's `size` instead. `next` is the
offset of the following window, null at the end of the file. Only the
window is read: a sparse 10 GiB file answers a window near its end at once.

| Error | When |
| --- | --- |
| `InvalidInput` | an option outside its range; a `cursor` with no `path`, or not an entry of the folder; a path that is not absolute or holds a `..`; for a Sandbox with no root of its own, a path outside its home (its run folder's `config/`, `overlay/`, `daemon.log` and Workspace mounts included); a symbolic link at the end, or, in the home, on the way, wherever it leads; a `system` entry or a path in one; a path a link leads into a mount that is not entered, or to what LayerFS keeps private; a FIFO, socket or device; a mount point or a path in one; a `restricted` entry or a path in one (`store.sqlite*`); a regular file with several names that the Sandbox's command user (the numeric uid of its daemon record, else 501) does not own |
| `NotFound` | nothing is at the path |
| `NoSuchSandbox` | no Sandbox of that name, or its machine holds no run folder for it |
| `Unavailable` | the Sandbox's machine is stopped or its worker did not answer in 8 s; the read ran past 6 s; a path in the root of a Sandbox whose daemon is not running, or whose recorded process id now names another process. Nothing starts a machine or a Sandbox |
| `Busy` | two inspection reads already run on that machine; read again |
| `Unsupported` | a Linux kernel before 5.6 (no `openat2`), or before 5.8 for a path in the root (no mount id in `statx`); a path in the root read by a process that is not root (a native Linux home), or that may not read the init's root, each with its reason; nothing else is tried |

A Sandbox whose daemon has ended keeps its home readable while its run
folder is there. Which calls open what, and what a page costs, is in
[inspection](../../control/sandboxes/inspect.md#files).

## Family errors

`InvalidInput` (a bad name, a folder scope), `NoSuchStore`,
`NoSuchProject`, `NoSuchSandbox`
([`family.rs`](../../../../control/crates/layerfs-sandbox/src/family.rs)),
`Busy` or `Unresolved` (an operation holds the Sandbox or its Store),
`Unavailable` (the worker is not recorded or does not answer, a lock is held
too long, a PostgreSQL server is not reached,
[`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs)). New
work on a removed platform is `Unsupported` before the home is touched
([`calls/platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs)).
After `Unknown` from `create` or `close`, a line goes to
`log/uncertain.jsonl` and later calls are `Unresolved`, naming
`sandbox get` (shows it) and `sandbox close` (clears it).

## Related

- [workspace family](workspace.md), [machine family](machine.md), [logs family](logs.md).
- [Sandboxes](../../control/sandboxes/README.md) and [backends](../../control/sandboxes/backends.md).
- [Errors and events](../calls/errors-and-events.md).
