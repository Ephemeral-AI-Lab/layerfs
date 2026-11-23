# sandbox family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `sandbox` calls: start, list, read, remove and check a named Sandbox, one project's Workspace daemon on a machine, and read its files and its daemon's log, within the [SDK](README.md). `sandbox.doctor` is one of [the doctors](../../control/doctor/README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/sandbox`](../../../../api/crates/layerfs-api/src/sandbox) (one file per call), [`layerfs-sandbox/src`](../../../../control/crates/layerfs-sandbox/src) (`family.rs`, `create.rs`, `list.rs`, `get.rs`, `remove.rs`, `state.rs`, `lifecycle.rs`, `force.rs`, `inspect/files.rs`, `inspect/roots.rs`, `inspect/root.rs`, `inspect/log.rs`) |
| Runs by default | `list`: the caller's process, home records only. `create`, `get`, `remove`: records and locks in the caller's process, then the Sandbox's machine: this process for a local machine, else the machine's worker; `get` may dial a forwarded daemon directly. `files`, `log`: where the Sandbox's run folder is, on the [inspection route](../../control/sandboxes/inspect.md), with no lock, lease or daemon. `doctor`: [the doctors](../../control/doctor/README.md) |
| Design | [Sandboxes](../../control/sandboxes/README.md), [inspection](../../control/sandboxes/inspect.md), [daemon channel and exec](../../control/sandboxes/daemon-channel-and-exec.md), [backends](../../control/sandboxes/backends.md) |

## Calls

```text
 project init --> create <project> <name> [--machine M] [--mount B]  (starts the daemon)
                     |
                     +--> list / get [--with stats]   (records; get asks the daemon)
                     +--> files [path] / log           (its roots, home, root; its log)
                     +--> doctor                       (record, runtime, daemon, Workspaces)
                     +--> workspace mount ... unmount
                     v
                  remove <sandbox> [--force]   (ends the daemon; clears an
                                                unresolved create or remove)
```

| Call | Short answer | MCP |
| --- | --- | --- |
| [`sandbox.create`](#sandboxcreate) | `{ ok }`, or with `--mount` `{ workspace, path }` | `work` |
| [`sandbox.list`](#sandboxlist) | `{ items: [{ sandbox, project }] }` | `admin` |
| [`sandbox.get`](#sandboxget) | `{ state }` + `workspaces` when any | `admin` |
| [`sandbox.remove`](#sandboxremove) | `{ ok }` | `admin` |
| [`sandbox.files`](#sandboxfiles) | `{ items }`, or a file window `{ text }` | `admin` |
| [`sandbox.log`](#sandboxlog) | `{ items: [{ text }] }` | `admin` |
| [`sandbox.doctor`](#sandboxdoctor) | the doctor report | `admin` |

Every call but `create` names the Sandbox by the positional `sandbox`. A
folder scope refuses every `sandbox` call (`out of scope: sandbox.<call>`).

**A Sandbox's state** ([`state.rs`](../../../../control/crates/layerfs-sandbox/src/state.rs))
is derived from its record and its machine's record, with no probe, in this
order: `busy` with `op { name, phase, started_at }` while a `create` or
`remove` runs; `broken` `unresolved` once one lost its answer and its process
ended (or a machine stop's outcome was not known); `broken` `missing` with no
runtime id, or a runtime a confirmed machine stop ended; `blocked` with
`blocked_by { machine, reason }` while its machine is not ready; `broken`
`unsupported` for the retired docker backend; `broken` `lost` or
`not_answering` when a `get` recorded it; else `ready`. Every broken
Sandbox's `fix` is `layerfs sandbox remove <sandbox>`; the long form's
`message` says why. `list` and `get` start no machine.

### sandbox.create

Start a named Sandbox of a project on a machine; with --mount, also mount a Branch in it.

```text
CLI     layerfs sandbox create <project> <name> [--machine M] [--image I] [--mount B] [--progress]
Node    await layerfs.sandbox.create(project, name, { machine, image, mount, on_progress })
Python  layerfs.sandbox.create(project, name, *, machine=None, image=None, mount=None, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` | project | `<store>/<project>` |
| `name` | name | the new Sandbox's name |
| `machine` option | name | the machine to run on; default: the machine of the project's sqlite Store; this computer (native) for a project of a PostgreSQL Store |
| `image` option | image | the image the Sandbox runs; default: the machine's own |
| `mount` option | name | also mount this Branch; the Workspace gets the default name, the Branch's name made valid, and the answer is its handle and path |
| `on_progress` option | function |  |

| Answer | Keys |
| --- | --- |
| short | each when set: `ok`, `workspace`, `path` |
| long | `ok, workspace, path, outcome, sandbox, project, machine, platform, image, backend, route, state, created_at, forward_failed` |
| call | sends progress; writes one call-log line; MCP `work` |

The name is checked with the Store's machine and a running `store move`
under one home lock, where it is claimed with the `op` `create`; a name taken
anywhere in the home is `Exists`. The machine starts the daemon, and the
record is written once with the answer, dropping the `op`; a refused start
removes the record. With no `--machine` the Sandbox runs on its sqlite
Store's machine, else on `native`; another machine than a sqlite Store's is
`Unsupported`, and the support table must accept the platform and Store
kind. For a PostgreSQL Store the caller resolves the saved login and sends
the URL only in this request; the launching side checks that it reaches the
server and holds the project first. Nothing starts a stopped machine.

`--image` is a `sha256:` digest, a Docker image Id or a Docker reference:
one the machine holds (unpacked, or in its archive) starts at once; any other
is copied from the host's Docker first, as
[`machine load-image`](machine.md#machineload_image) does, with its
refusals. macOS `native` runs no image (`Unsupported` `{reason: "host"}`).
Without it the root is the machine record's image (not on `docker`), else
the sealed bundle's archive
([`deploy.rs`](../../../../control/crates/layerfs-sandbox/src/deploy.rs),
[`image_load.rs`](../../../../control/crates/layerfs-sandbox/src/image_load.rs)).
Progress: with `--image`, the copy's `inspect`, `lock`, `export`, `send`,
`unpack`; then `start` and, with `--mount`, `mount`.

`--mount B` mounts Branch `B` in the started Sandbox under its default name
(the Branch's name lowercased, each other character `-`) and answers the
Workspace's `workspace` and `path`. A refused mount keeps the Sandbox and
answers the mount's own refusal with `sandbox` and a `fix`: the mount's own
when it has one (an `Exists` names `--name`); for a Branch that is not there
(raised `NoSuchBranch`, answered `NotFound`)
`layerfs branch fork <project> <B> && layerfs workspace mount <sandbox> <B>`;
else `layerfs workspace mount <sandbox> <B>`. `<B>` is one shell word
(single-quoted when a shell would not take it literally)
([`sandbox/create.rs`](../../../../api/crates/layerfs-api/src/sandbox/create.rs)).
A create whose outcome is not known is `Unknown`; its record keeps the
`op`, uncertain, until `sandbox remove`.

A start that fails is `Failed` in words: "Sandbox `<name>` did not start:
it stopped `<where>`: `<cause>`", then "Its daemon's last error: …" from
its log, except when a PostgreSQL Sandbox daemon could not reach its
Store's server at startup (`Unavailable`: "Sandbox `<name>` did not start:
its daemon cannot reach PostgreSQL Store `<store>`'s server at `<host>`",
`<host>` being every host its URL names; on a machine that does not share
this computer's loopback, when that URL names a host by name, it adds the
same advice as the loopback refusal: the platform's name for this computer
or `layerfs store connect <store> <url> --machine <machine>`). The
Sandbox resolves names as its machine does. Its `detail` holds `daemon`
(the program that was started), `log` (its last lines), `container` and
`removed`
([`launch/local.rs`](../../../../control/crates/layerfs-sandbox/src/launch/local.rs) `abandon`).
Its `fix` is `layerfs machine doctor <machine>` unless the refusal names a
closer one. On `native`, when `detail.daemon` is the native machine's
daemon and it is of another build than this one (or names no build), the
message says so and the `fix` is
`layerfs machine update native --daemon <this build's daemon>` when one is
found beside the program or the bundle, else `layerfs home doctor`
([`home.rs`](../../../../control/crates/layerfs-control/src/home.rs) `native_start`;
a refusal with no `detail.daemon` is not looked at). A start whose cleanup
failed is `Unknown` and says what is left.

| Refusal | When |
| --- | --- |
| `Exists` | the name is taken in the home |
| `InvalidInput` | `native` or `docker` as the name (they name the built-in machines), or a name ending in `.root` |
| `Unsupported` | another machine than its sqlite Store's; an `--image` on macOS `native`; an image for another architecture or OS |
| `Capacity` | copying the image needs `2 × size × 1.05` bytes, less what a resumed copy staged, and the machine has fewer (`detail { needed_bytes, free_bytes, path }`, `fix: layerfs machine prune <machine> --unused`) |
| `Unavailable` | the machine is stopped (`detail { what: "machine" }`) or Docker cannot be read (`detail { what: "docker" }`) |
| `Failed` | the start failed: what stopped it in words, `detail.daemon` and `log`, `fix` as above |
| the mount's code | `--mount` was refused; the Sandbox is kept (`sandbox`, `fix`: fork first only for a missing Branch) |
| `Unknown` | the start's answer was lost; the record keeps the `op` |

Run on macOS native with a debug build:

```console
$ layerfs sandbox create shop/api tests --progress
{"phase":"start","done":0,"total":1}
{"phase":"start","done":1,"total":1}
{"ok":true}
$ layerfs sandbox create shop/api native
{"error":"InvalidInput","message":"a Sandbox is not named native: that names a built-in machine (H §7.6)"}
```

### sandbox.list

Every Sandbox in the home, or one project's or one machine's.

```text
CLI     layerfs sandbox list [--project P] [--machine M] [--limit 100] [--cursor C]
Node    await layerfs.sandbox.list({ project, machine, limit, cursor })
Python  layerfs.sandbox.list(*, project=None, machine=None, limit=None, cursor=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `project` option | project | list only this project's Sandboxes |
| `machine` option | name | list only the Sandboxes on this machine |
| `limit` option | int | the most rows in one page, 1 to 1000; default `100` |
| `cursor` option | text | the `next_cursor` of the page before; it carries the filters |

| Answer | Keys |
| --- | --- |
| short | `items`; when set: `next_cursor` |
| long | `items, next_cursor` |
| `items[]` short | `sandbox`, `project`; when set: `workspaces`, `workspaces_not_listed`, `state`, `op`, `blocked_by`, `reason`, `fix` |
| `items[]` long | `sandbox, project, workspaces, workspaces_not_listed, state, op, blocked_by, reason, fix, message, machine, platform, image, created_at` |
| `items[].workspaces[]` short | `workspace`, `branch` |
| `items[].workspaces[]` long | `workspace, branch` |
| call | pages by `limit` and `cursor`; writes one call-log line; MCP `admin` |

Reads records only and asks no machine or daemon. A row is the Sandbox's
name, its project, the Workspaces this home records (`[{workspace,
branch}]`) and its state when not ready; `workspaces_not_listed` is true for
a Sandbox whose Workspace records live in its machine. A page ends after
`limit` rows (1 to 1000) or early at 64 KiB, with a `next_cursor` that
carries the filters; a cursor of other filters is `InvalidInput`.

```console
$ layerfs sandbox list
{"items":[{"sandbox":"tests","project":"shop/api"}]}
```

### sandbox.get

One Sandbox: its state and its Workspaces.

```text
CLI     layerfs sandbox get <sandbox> [--with stats]
Node    await layerfs.sandbox.get(sandbox, { with })
Python  layerfs.sandbox.get(sandbox, *, with_=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `sandbox` | name | a Sandbox name |

| Answer | Keys |
| --- | --- |
| short | `state`; when set: `workspaces`, `op`, `blocked_by`, `reason`, `fix`, `observed_at` |
| long | `state, workspaces, op, blocked_by, reason, fix, observed_at, message, sandbox, project, machine, platform, image, backend, route, created_at, workspace_capacity, network` |
| `workspaces[]` short | `workspace`, `branch`; when set: `running`, `state`, `op`, `reason`, `fix` |
| `workspaces[]` long | `workspace, branch, running, state, op, reason, fix, path, commit, base_layer, mounted_at, branch_moved, pending_merge, commands, id` |
| `workspaces[].running[]` short | each a `exec` |
| `workspaces[].commands[]` short | `exec` |
| `workspaces[].commands[]` long | `exec, started_at, program, tag` |
| `--with stats` | adds `daemon`, `usage`, `usage_reason`, `daemon_capacity`, `workspaces` |
| call | writes one call-log line; MCP `admin` |

Starts from the records and, for a Sandbox they say is ready (or `lost`, or
`not_answering`), asks its daemon for its Workspaces once, on the route its
other calls take. A list is `ready`; a machine that says the Sandbox does
not run is `broken` `lost`; a daemon that admits no control connection is
`broken` `not_answering`, its `message` carrying the refusal's counts; a
machine or forward that does not answer is `unknown`, with `observed_at`
(when its state was last known). The record is written only when that
changes its `reason` (which moves the home's `generation`); when it was
observed is kept in `observed.json` beside the record, which does not.
Reading never clears an `op`.

Each Workspace is named by its handle `workspace` (`<sandbox>:<name>`), its
`branch`, the exec ids `running` in it now and, when not ready, its `state`
(`busy` with `op.name` `mount`, `commit` or `unmount`; `broken` with
`fix: layerfs workspace unmount <workspace> --discard --force`). The long
row adds `path`, `commit`, `base_layer`, `mounted_at`, `branch_moved`,
`pending_merge`, `commands` (`[{exec, started_at, program, tag}]`; no
argument or environment value is shown) and `id`. The Sandbox's long form
adds `sandbox, project, machine, platform, image, backend, route,
created_at`, `workspace_capacity` (16) and `network`. `platform` is `docker`, `mac` or
`linux`; `backend` `native` (a macOS host process) or `namespaces`; `route`
`nfs` for a macOS host process, else `fuse`
([`runtime.rs`](../../../../control/crates/layerfs-sandbox/src/runtime.rs)).
`network` is a `namespaces` Sandbox's own network as its record keeps it
from the create: its host end on the machine and its commands' address,
`lfs<id12> <address>/31` (null for a native Sandbox;
[Backends: own network](../../control/sandboxes/backends.md#every-namespaces-sandboxs-own-network)).

`--with stats` adds `stats`, null unless the Sandbox is ready: `daemon`
(`{pid, started_at}`), `usage` (`{cpu_ms, memory_bytes, memory_peak_bytes,
memory_file_bytes, pid_count, oom_kill_count}` of its cgroup, or null with
`usage_reason`: `host_process`, `not_supported`, `cgroup_v1`, `no_cgroup`,
`not_observed`, `no_controller`), `daemon_capacity` (the daemon's
control-capacity model, one more bounded call) and `workspaces`
(`[{workspace, overlay_bytes, usage, usage_reason}]`).

Run on macOS native with a debug build:

```console
$ layerfs sandbox get tests
{"state":"ready"}
$ layerfs sandbox get tests --with stats
{"state":"ready","workspaces":[{"workspace":"tests:agent","branch":"agent"}],"stats":{"daemon":{"pid":75696,"started_at":"2026-11-18T22:05:16Z"},"usage_reason":"host_process","daemon_capacity":{"admission":{"pool_vacant":7,"pool_running":1,"reserved_vacant":1,"reserved_running":0},"retained":{"limit":64,"ttl_ms":3600000,"count":0,"dropped":0,"recorded":0}},"workspaces":[{"workspace":"tests:agent","overlay_bytes":77824,"usage_reason":"host_process"}]}}
```

### sandbox.remove

Stop a Sandbox and remove it.

```text
CLI     layerfs sandbox remove <sandbox> [--force] [--progress]
Node    await layerfs.sandbox.remove(sandbox, { force, on_progress })
Python  layerfs.sandbox.remove(sandbox, *, force=None, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `sandbox` | name | a Sandbox name |
| `force` option | bool | remove it even while Workspaces are mounted or commands run; uncommitted changes are lost |
| `on_progress` option | function |  |

| Answer | Keys |
| --- | --- |
| short | each when set: `ok`, `unmounted`, `left` |
| long | `ok, unmounted, left, outcome, sandbox` |
| call | sends progress; writes one call-log line; MCP `admin` |

Records its `op` (`remove`) before it stops the Sandbox and removes the
record at the end; progress is its phases `cancel` (with `--force`, one per
command this home records), `stop` and `delete`. With `--force`, each
running command is cancelled first (SIGTERM, then SIGKILL after 5 s), then
the Sandbox is stopped anyway: `unmounted` lists the Workspaces it ended,
whose uncommitted changes end with it. On `native` a forced remove works
from the Sandbox's own records: it calls no daemon, checks the runtime's
identity and ends its mounts and processes
([`force.rs`](../../../../control/crates/layerfs-sandbox/src/force.rs)).
Once the runtime is confirmed stopped, either forced path ends the mount
intent each of its Workspaces held in the Store, once each (by the Branch
name and Workspace id their records keep on `native`; by the bound ids its
worker holds on a machine, which hands them back to the caller to end with
the Store's own URL when it could not dial a PostgreSQL Store's), and each one it could not end is a `left`
sentence naming why and what ends it there
([`mount_intents.rs`](../../../../control/crates/layerfs-sandbox/src/mount_intents.rs)).
Nothing else ends a mount intent: `store compact` and the other GC calls
settle only an intent the Store's admission proves unowned.
`outcome` (long form) is `removed`, `already_gone` (its runtime was gone, or
it never recorded one; `left` says what may remain). A Sandbox of the
retired docker backend is also `already_gone`: its records go, and `left`
says its container of the retired docker backend was not touched, with
"`docker rm -f <container>` removes its container" when its daemon record
keeps the container's name, else "its record does not name its container
(`docker ps -a` lists them)". A Sandbox whose record names a run folder this
home does not give it (an earlier build's `lfs-` name) is also `already_gone`:
its records go, and `left` names that run folder and its daemon process,
neither touched, with the `rm -rf` that removes the folder (one shell word)
and a `kill` of the process only while the process its record identifies
(id, start time and boot) still runs; otherwise it says the process is gone
(RV3 FAIL 4). Two more cases go by their records alone, with no signal sent,
and answer `already_gone` with `left`: a local record this build does not
read (a field a 2.0 record always has missing or of another shape, or a
native record on Linux; every other call refuses it with the fix
`layerfs sandbox remove <sandbox>`; RV3 FAIL 2, 3), and a Sandbox on a
machine whose record has no 2.0 name, whose worker is not asked either
(RV3 FAIL 1). A lost answer keeps the record with the `op` uncertain
(`broken` `unresolved`) until another `remove`; any other refusal leaves the
Sandbox as it was ([`remove.rs`](../../../../control/crates/layerfs-sandbox/src/remove.rs)).
It was `sandbox.close`, removed with no alias.

| Refusal | When |
| --- | --- |
| `Busy` | without `--force`, a command runs in one of its Workspaces (`running`, the exec ids); another live operation holds the Sandbox |
| `InUse` | without `--force`, a Workspace is mounted (`workspaces`, the handles) |
| `Failed` | the stop or delete failed; a failed stop is not followed by a delete |
| `Unknown` | the machine's answer was lost; or with `--force` a command's cancel was not confirmed (`running`) |

Run on macOS native with a debug build:

```console
$ layerfs sandbox remove tests
{"error":"InUse","message":"Sandbox tests still holds 1 Workspace(s); commit or unmount them first","workspaces":["tests:agent"]}
$ layerfs workspace unmount tests:agent
{"ok":true}
$ layerfs sandbox remove tests --progress
{"phase":"stop","done":1,"total":3}
{"phase":"delete","done":2,"total":3}
{"phase":"delete","done":3,"total":3}
{"ok":true}
```

### sandbox.files

A Sandbox's files: its roots, one page of a folder, or one byte window of a file.

```text
CLI     layerfs sandbox files <sandbox> [path] [--depth 1] [--limit 100] [--cursor C] [--byte-offset 0] [--byte-length 262144]
Node    await layerfs.sandbox.files(sandbox, path?, { depth, limit, cursor, byte_offset, byte_length })
Python  layerfs.sandbox.files(sandbox, path=None, *, depth=None, limit=None, cursor=None, byte_offset=None, byte_length=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `sandbox` | name | a Sandbox name |
| `path` | text | an absolute path as the Sandbox's commands see it; left off, the Sandbox's roots; optional |
| `depth` option | int | how many levels of a folder a page walks, 1 to 32; default `1` |
| `limit` option | int | the most rows in one page, 1 to 1000; default `100` |
| `cursor` option | text | the `next_cursor` of the page before |
| `byte_offset` option | int | where a file's window starts; default `0` |
| `byte_length` option | int | the window's length in bytes, at most 1048576; default `262144` |

| Answer | Keys |
| --- | --- |
| short | each when set: `items`, `next_cursor`, `text`, `binary`, `size`, `next_byte_offset` |
| long | `items, next_cursor, text, binary, size, next_byte_offset, type` |
| `items[]` short | `path`, `type`; when set: `size`, `not_read` |
| `items[]` long | `path, type, size, not_read, role, depth, mode, modified_at, target, fs, workspace` |
| call | pages by `limit` and `cursor`; writes no call-log line; MCP `admin` |

Reads what is in a Sandbox: its roots, one page of a folder, or one byte
window of a file, in its home and, for a Sandbox with a root of its own, in
that root. It is a read for a UI that shows a tree: no lock, no lease, no
daemon call, nothing written and no call-log line. An option that does not
apply to what `path` is (a `limit` for a file, a `byte_offset` for a folder)
is checked for its range and not used. `depth` is 1 to 32; `byte_length` is
1 to 1 MiB.

**The roots** (no `path`): a page that is never continued, each entry with a
`role` (long form), in this order.

| `role` | Entry | From |
| --- | --- | --- |
| `workspace` | one per mounted Workspace: `type: "dir"`, `not_read: "mount"`, `path` its mount (`<run folder>/workspaces/<n>`), `workspace` its handle. Ordered by path, `2` before `10` | the Workspace records on the side that keeps them. The mount is never `stat`ed or opened: a stopped daemon cannot hang the answer |
| `home` | `type: "dir"`, `path` `<run folder>/tmp`: `HOME` for the Sandbox's commands | the run folder |
| `log` | `type: "file"`, `path` `<run folder>/daemon.log`, with its `size`. It is read with [`sandbox log`](#sandboxlog), not with this call | the run folder |
| `root` | `type: "dir"`, `path` `/`: what the Sandbox's commands see as `/`. Only for a Sandbox with a root of its own (the `namespaces` backend: native Linux, docker, Lima, Apple Container) | the Sandbox's record: nothing of the root is opened for this answer, so a stopped Sandbox lists it too |

A host-process Sandbox (native macOS) has no `root` entry: its commands see
the owner's computer, and a path outside its home is `InvalidInput`.

**The root** (a path outside the home, when the roots offer `/`) is read
through the root of the Sandbox's init process, on the machine that runs it
([`inspect/root.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/root.rs)):

| | In the root |
| --- | --- |
| Links | one on the way resolves inside the root (`/lib` of a merged-`/usr` image); the last name is never followed. Where the kernel's `fs.protected_symlinks` is on, a path right under a link a command made in a sticky shared folder (`/tmp`) is `InvalidInput` |
| `system` | `/proc`, `/sys`, `/dev`: listed with `not_read: "system"`, never `stat`ed or entered |
| Mounts | entered when the type is `ext4`, `xfs`, `btrfs`, `zfs`, `f2fs`, `overlay` or `tmpfs`; any other is `not_read: "mount"` with its `fs`. A Workspace mount is `not_read: "mount"` with `workspace`, and is never `stat`ed or entered |
| `restricted` | the run folder's `config/` and `overlay/`, `store.sqlite*`, locks and leases are listed with `not_read: "restricted"` and never read; `daemon.log` is a `log`. A path that a link leads to one of them is refused too |
| Hard links | read, but for a file in the run folder that the command user does not own |

**A folder** answers `items` in tree order: names in byte order, and a
folder's own entries right after it, down `depth` levels; `next_cursor` is
the path of the last entry while more remain, and the same call with
`cursor` answers the entries after it, whatever was added or removed in
between. A page also ends at the read's 4 s deadline, holding what it had. An
entry is `{path, type, size, not_read}`: `type` is `file`, `dir`, `symlink`
or `other` (a FIFO, socket or device); `not_read` says why an entry is listed
and not read (`system`, `mount`, `unreadable`, `restricted`). The long entry
adds `role`, `depth` (1 for an entry of the folder asked for), `mode`
(octal text), `modified_at`, `target` (a symlink's target; never followed),
`fs` and `workspace`. A name that is not UTF-8 is not listed.

**A file** answers a window: `text` from `byte_offset`, with
`next_byte_offset` while more remains, or `binary: true` with its `size`
when the window holds a NUL byte. Bytes that are not UTF-8 are U+FFFD and
every control character but a line break and a tab is written as its escape
(`\r`, `\u{1b}`). A window that would end inside a character ends before it,
so windows read from each `next_byte_offset` join to the file's text. Only
the window is read.

| Refusal | When |
| --- | --- |
| `InvalidInput` | an option outside its range; a `cursor` with no `path`, or not an entry of the folder; a path that is not absolute or holds a `..`; for a Sandbox with no root of its own, a path outside its home; a link at the end, or, in the home, on the way; a `system`, `mount` or `restricted` entry or a path in one; a FIFO, socket or device; a file the system refuses to open; the log (read with `sandbox log`) |
| `NotFound` | nothing is at the path; no Sandbox of that name, or its machine holds no run folder for it |
| `Unavailable` | the Sandbox's machine is stopped, or its worker did not greet in 2 s or answer in 8 s; a path in the root of a Sandbox whose daemon is not running. Nothing starts a machine or a Sandbox |
| `Busy` | two inspection reads already run on that machine |
| `Unsupported` | a Linux kernel before 5.6 (no `openat2`), or before 5.8 for a path in the root; a path in the root read by a process that may not read the init's root |

A Sandbox whose daemon has ended keeps its home readable while its run
folder is there. Which reads open what is in
[inspection](../../control/sandboxes/inspect.md#files).

Run on macOS native with a debug build:

```console
$ layerfs sandbox files tests
{"items":[{"path":"/tmp/demo/home/machine/native/run/layerfs-8a7387b2-tests/workspaces/1","type":"dir","not_read":"mount"},{"path":"/tmp/demo/home/machine/native/run/layerfs-8a7387b2-tests/tmp","type":"dir"},{"path":"/tmp/demo/home/machine/native/run/layerfs-8a7387b2-tests/daemon.log","type":"file","size":38}]}
```

### sandbox.log

A Sandbox's daemon log: its last lines, or the lines before a cursor.

```text
CLI     layerfs sandbox log <sandbox> [--limit 200] [--cursor C]
Node    await layerfs.sandbox.log(sandbox, { limit, cursor })
Python  layerfs.sandbox.log(sandbox, *, limit=None, cursor=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `sandbox` | name | a Sandbox name |
| `limit` option | int | the most lines in one page, 1 to 1000; default `200` |
| `cursor` option | text | the `next_cursor` of the page before: the lines before it |

| Answer | Keys |
| --- | --- |
| short | `items`; when set: `next_cursor` |
| long | `items, next_cursor` |
| `items[]` short | `text`; when set: `continued` |
| `items[]` long | `text, continued` |
| call | pages by `limit` and `cursor`; writes no call-log line; MCP `admin` |

One page of the log of a Sandbox's daemon: `daemon.log` in its run folder,
which the backend makes when the daemon starts, never rotates and removes
with the folder when the Sandbox is removed. `items` are the last `limit`
lines (200 by default), oldest first, or with `cursor` the lines before it,
at most 64 KiB a call; `next_cursor` continues with older lines and is left
out at the start of the log. A line longer than one read is answered in
pieces, each but the last with `continued: true`. Control characters in
`text` are escaped (`\t`, `\r`, `\u{1b}`), bytes that are not UTF-8 are
U+FFFD.

The file is read where the run folder is: in the caller's process for a
Sandbox on `native`, in the machine's worker for every other
([`inspect/log.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/log.rs)).
The daemon is not asked, so the log of a daemon that has ended stays
readable while its run folder is there. It takes no lock or lease and writes
nothing; refusals are `sandbox files`'s (`NotFound`, `Unavailable`, `Busy`),
`InvalidInput` for a stale or foreign cursor, and `Unsupported` for a
retired docker Sandbox record. It was `logs.sandbox`, removed with no alias
(its `--before` is `--cursor`).

```console
$ layerfs sandbox log tests --limit 2
{"items":[{"text":"LAYERFS_DAEMON_LISTEN 127.0.0.1:62804"}]}
```

### sandbox.doctor

Check one Sandbox: its record, runtime and daemon, then each of its Workspaces.

```text
CLI     layerfs sandbox doctor <sandbox> [--progress]
Node    await layerfs.sandbox.doctor(sandbox, { on_progress })
Python  layerfs.sandbox.doctor(sandbox, *, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `sandbox` | name | a Sandbox name |
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

Checks one Sandbox: its record, its runtime and its daemon, then each of its
Workspaces. A Workspace has no doctor of its own: the Sandbox's daemon list
is read once and each Workspace it lists or the home records is one node,
`<sandbox>:<name>`, with `workspace.mounted` and `workspace.activity`; a
failed or uncertain one is an error fixed by `layerfs workspace unmount
<workspace> --discard --force`. Under a machine that is not ready nothing is
tried: the Sandbox is `not_checked` with `blocked_by`. A daemon that is not
ready leaves its Workspaces `not_checked`, blocked by the Sandbox. It
changes nothing and exits 0 once its report is made. It replaces the shell's
`layerfs doctor sandbox`, removed with no alias.

```console
$ layerfs sandbox doctor tests
{"ok":true}
```

## Family errors

`InvalidInput` (a bad name, a folder scope), `NotFound` (no Store, project
or Sandbox of that name;
[`family.rs`](../../../../control/crates/layerfs-sandbox/src/family.rs)),
`Busy` or `Unresolved` (an operation holds the Sandbox or its Store),
`Unavailable` (the worker is not recorded or does not answer, a PostgreSQL
server is not reached;
[`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs)), each
with the `fix` that clears it where there is one. New work on a removed
platform is `Unsupported` before the home is touched
([`calls/platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs)).
After `Unknown` from `create` or `remove`, a line goes to
`log/uncertain.jsonl` and later calls are `Unresolved`, naming
`sandbox get` (shows it) and `sandbox remove` (clears it).

## Related

- [workspace family](workspace.md), [machine family](machine.md).
- [Sandboxes](../../control/sandboxes/README.md) and [backends](../../control/sandboxes/backends.md).
- [Doctors](../../control/doctor/README.md).
- [Errors and events](../calls/errors-and-events.md).
