# machine family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `machine` calls: list, inspect, create, start, stop, update and remove the machines that hold sqlite Stores and run Sandboxes, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/machine`](../../../../api/crates/layerfs-api/src/machine), [`layerfs-control/src/machine.rs`](../../../../control/crates/layerfs-control/src/machine.rs), [`layerfs-machine/src`](../../../../control/crates/layerfs-machine/src), [`layerfs-platform/src`](../../../../control/crates/layerfs-platform/src) |
| Runs by default | every call in the caller's process, through the platform adapter (provider CLI or Docker); none is sent to a worker but the two reads `stats` and `files`, each one inspect request served where the machine's data is. The guest worker is built without the `machine` feature, so there every machine call is `Unsupported` before any effect |
| Design | [Machines](../../control/machines/README.md), [platforms](../../control/machines/platforms.md), [workers and reach](../../control/machines/workers-and-reach.md) |

## Calls

```text
 reads (start nothing):  list   platforms   get   status   stats   files   images
 Sandbox images:         load (copy from the host's Docker)   prune

 built in: native, docker ---------------------- update (image, docker,
                                                  wsl, daemon)
 platforms --> create <name> <vm platform> -----> running
                                             |     |      ^
                                             |   stop     |
                                             |     v      |
                                             |  stopped --+ start
                                             v
               remove (VM + disk; a removed platform's record: forgotten)
```

| Call | Returns |
| --- | --- |
| [`machine.list`](#machinelist) | `{ machines: [summary] }` |
| [`machine.platforms`](#machineplatforms) | `{ host, platforms: [row] }` |
| [`machine.get`](#machineget) | summary |
| [`machine.images`](#machineimages) | `{ machine, mode, architecture, default, free, images, docker, available }` |
| [`machine.load`](#machineload) | `{ id, refs, size, reused }` |
| [`machine.prune`](#machineprune) | `{ removed: [id], freed }` |
| [`machine.status`](#machinestatus) | `{ name, platform, state, free_disk, tool_version, down_reason, stores, op? }` |
| [`machine.stats`](#machinestats) | `{ machine, platform, observed_at, worker, resources, data_disk, stores, sandboxes, complete, reason? }` |
| [`machine.files`](#machinefiles) | `{ kind: "folder", entries, next, has_more, reason? }` or `{ kind: "file", size, offset, content, binary, next }` |
| [`machine.start`](#machinestart) | summary, once the worker answers |
| [`machine.stop`](#machinestop) | summary, `state: "stopped"`; with `force`, also `forced`, `stopped_sandboxes`, `unknown` |
| [`machine.create`](#machinecreate) | summary, `state: "running"` |
| [`machine.remove`](#machineremove) | `{ name, removed: true, note? }` |
| [`machine.update`](#machineupdate) | summary |

The **summary** is `MachineRecord::to_sdk`
([`record.rs`](../../../../control/crates/layerfs-machine/src/record.rs)):

```text
{ name, platform, backend, supports: { sqlite, postgresql_sandbox, reason, log },
  state, cpus, memory, disk, stores: [name], postgresql: [name],
  sandboxes: [name], image?, docker?, wsl?, daemon?, os_image?, error?,
  ssh?, host?, vm_name? }
```

| Field | Meaning |
| --- | --- |
| `platform` | `native`, `docker`, `apple-container` or `lima`; an older home's record may still name the removed `firecracker`, `microsandbox` or `ssh` |
| `backend` | the Sandbox backend on this host: `native`, `namespaces` (also on `docker`), or `-` (always for a removed platform) |
| `supports` | the platform's support row on this host; a removed platform supports nothing, with the removal as `reason` |
| `supports.log` | whether the machine has a worker log to read (layerfs-project#178): its calls go to a worker and the support table refuses it nothing on this host, so it is a worker this build starts. True for `docker`, `lima` and `apple-container` where they run; false on every `native` (its calls run in the caller's process; on Windows its WSL2 worker is not designed) and for a removed platform. Clients read this field and never test platform names (`Supports::log`, [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs)) |
| `cpus`, `memory`, `disk` | the recorded size (bytes for memory and disk); null on a built-in machine |
| `stores`, `postgresql`, `sandboxes` | sqlite Stores on it, PostgreSQL Stores attached to it, Sandboxes recorded on it ([`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs)) |
| `error` | the probe's reason when `unknown`; the removal for a removed platform; a failed create's error |

| `state` | When |
| --- | --- |
| `creating`, `removing` | a live process holds the machine's `op` |
| `unresolved` | a gone process left an uncertain `op` |
| `running`, `stopped`, `failed`, `missing` | the provider's answer to the live probe ([`setup/calls.rs`](../../../../control/crates/layerfs-platform/src/setup/calls.rs)) |
| `unknown` | the probe failed or did not answer within 10 s (`PROBE`) |
| `unsupported` | a platform removed in 2.0; never probed |
| `ready` | `native`, never probed |

Firecracker, Microsandbox and SSH machines were removed in 2.0: a record for
one is shown by `list` and `get`, `remove` only forgets it, and every other
call on it is `Unsupported` ``platform `ssh` was removed in LayerFS 2.0``
([`calls/platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs)).
Nothing but `machine.start` starts a machine; a call that needs a stopped
machine is `Unavailable`, naming `layerfs machine start`
([`place.rs`](../../../../control/crates/layerfs-store/src/place.rs)). A
folder scope refuses every machine call.

### machine.list

Every machine with its live state, Stores and Sandboxes. Starts nothing.

```text
JS      await layerfs.machine.list()
Python  layerfs.machine.list()
CLI     layerfs machine list
```

Every `machine/<name>/record.json` is read without the home lock; each machine
except `native`, one held by an `op` and one of a removed platform is probed on
its own thread, and all probes are awaited together for at most 10 s. A late or
failed probe shows `unknown` with its reason (its command killed), not an error;
nothing is written back ([`list.rs`](../../../../control/crates/layerfs-machine/src/list.rs)).
The built-in `docker` machine is probed like a VM because its worker is started
and stopped by hand. Error: `Failed` (a record names no known platform).

### machine.platforms

The support table for this host, with a live tool probe per platform.

```text
JS      await layerfs.machine.platforms()
Python  layerfs.machine.platforms()
CLI     layerfs machine platforms
```

```text
{ host: "macos" | "linux" | "windows" | "other",
  platforms: [ { platform, how, backend, kernel, sqlite,
                 postgresql: { store_calls, sandbox, reason },
                 ready, missing: [text], version,
                 options: [ { name, kind } ], description } ] }
```

| Field | Meaning | Source |
| --- | --- | --- |
| `how` | `built_in` (`native`, `docker`) or `created` | [`platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs) |
| `backend`, `kernel` | `native` or `namespaces`; `fuse` or `nfs`; null when not on this host | [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs) |
| `sqlite` | where a sqlite Store's calls run: `in_process` or `worker` | same |
| `postgresql` | `store_calls` (always `in_process`), `sandbox` (bool), `reason` | same |
| `ready`, `missing`, `version` | the live probe: `tool:<cli>` not on the path, `host:<fact>` lacking; each command bounded at 10 s | [`probe.rs`](../../../../control/crates/layerfs-platform/src/probe.rs) |
| `options` | what `create` takes for this platform | [`options.rs`](../../../../control/crates/layerfs-platform/src/options.rs) |

The rows without the live fields are
[`control/SUPPORT.json`](../../../../control/SUPPORT.json), generated from
`support.rs`; the per-host matrix is on [platforms](../../control/machines/platforms.md).
A missing tool is `ready: false`, not an error.

### machine.get

One machine's summary, after one live probe (at most 10 s).

```text
JS      await layerfs.machine.get(name)
Python  layerfs.machine.get(name)
CLI     layerfs machine get <name>
```

A machine held by an `op` is not probed (its state comes from the record);
`get` never clears an `op`. A failed probe is `state: "unknown"`, not an
error. Errors: `InvalidInput` (not a machine name), `NotFound`, `Failed` (no
known platform).

### machine.images

The Sandbox images a machine holds and the host's Docker images it could
copy, read without starting it
([`images.rs`](../../../../control/crates/layerfs-sandbox/src/images.rs),
[`docker.rs`](../../../../control/crates/layerfs-sandbox/src/docker.rs)).

```text
JS      await layerfs.machine.images(name)
Python  layerfs.machine.images(name)
CLI     layerfs machine images <name>
```

```text
{ machine, mode: "images" | "host", architecture, default, free,
  images: [{ id, refs, size, architecture, source, sandboxes, last_used }],
  docker: null | { available: true }
        | { available: false, reason: "not-installed" | "not-running" | "error", message },
  available: [{ id, refs, size, os, architecture, usable, reason?, need, fits }] }
```

`images` are the unpacked images with their sidecars
(`machines/native/run/images/<hex>.2.json`: `source: "imported"` for one
copied from Docker) and what the archives its Sandboxes read still hold
(`bundle`, `machine`), unpacked or not. `id` is the Docker image Id for an
imported image and the config digest for an archive's. `sandboxes` counts
the Sandbox records on this machine naming that `id`; `last_used` is the
last create's time (RFC 3339) or null. `default` is the record's image, else
the archive's only one. `free` is the image cache's free bytes. `available`
lists the host's tagged Docker images (one `docker image ls`, one batched
`docker image inspect`, 10 s each) through the `docker` machine's socket,
else Docker's default context, minus any whose id or refs `images` already
has (the bundle's image in Docker, `layerfs-cache/sandbox:<hex>`, included).
A row for another OS or architecture is `usable: false` with `reason {kind:
"os" | "arch", value}`. `size` is what the image's layers unpack to: the
sum of `docker history`'s layer sizes (one call per image, eight at a time,
10 s in all), never less than `docker image inspect`'s `Size`, which with
Docker's containerd image store is the compressed content, about a third of
it; an image not measured in time keeps that `Size`. `need` is `2 × size × 1.05` and `fits` is
`need <= free` (true when `free` is unknown). A Docker failure fills
`docker.available: false` and never fails the call. macOS `native` answers
`mode: "host"`, `docker: null` and no rows. Errors: `NotFound`,
`Unavailable` `{what: "machine"}` when its worker does not answer.

### machine.load

Copies an image to a machine as `sandbox.create`'s `image` does, without a
Sandbox ([`image_load.rs`](../../../../control/crates/layerfs-sandbox/src/image_load.rs)).

```text
JS      await layerfs.machine.load(name, image, { on_progress })
Python  layerfs.machine.load(name, image, *, on_progress=None)
CLI     layerfs machine load <name> <image> [--progress]
```

`image` is a digest, Docker image Id or Docker reference. The machine is
asked first: an image it holds is `reused: true` and needs neither Docker nor
a byte sent. Otherwise `docker image inspect` resolves it; the host keeps an
export container `layerfs-export-<hex>` (removed once the machine publishes
the image), streams `docker export` without the init layer's files
(`.dockerenv`, `etc/hosts`, `etc/hostname`, `etc/resolv.conf`, `etc/mtab`,
`dev/*`) and device or FIFO entries in 1 MiB `image.artifact` frames to
`.recv-<hex>.part`, and the machine checks its size and sha256, unpacks it
once into `<hex>.2/`, writes the sidecar and deletes the staged tar. A staged
tar left by a stopped copy is resumed when its bytes hash as this export's
prefix, else restarted; one still growing (under 30 s old) is another
caller's copy, which this one waits for. `NoSpace`'s `need` counts the
larger of the layers' unpacked size and the export container's root
(`SizeRootFs`); `size` is the bytes sent, else the sidecar's, else the
layers' unpacked size. Progress: `inspect`, `export`, `send` (`done`,
`total`: an estimate, never below `done`, until the last `send` event, whose
`done` and `total` are both the bytes sent), `unpack`. Errors: `Unsupported` `{reason: "host"}`
(macOS `native`), `{reason: "arch" | "os", value}`; `InvalidInput` (Docker
has no such image); `Unavailable` `{what: "docker" | "machine"}`; `NoSpace`
`{need, free}`.

### machine.prune

Removes one unpacked image, or with `unused` every one no Sandbox uses
except the machine's default
([`images.rs`](../../../../control/crates/layerfs-sandbox/src/images.rs)).

```text
JS      await layerfs.machine.prune(name, image?, { unused })
Python  layerfs.machine.prune(name, image=None, *, unused=None)
CLI     layerfs machine prune <name> [<image>] [--unused]
```

An image is in use while a Sandbox directory's `config/helper.spec`
`lower=` names it, or while a create holds its lock between its unpack and
its spec: a named one is refused `InUse` (`detail.count`), `unused` skips
it. Naming the default removes it; the next create unpacks it again from
its archive. `freed` is the free bytes after minus before. Errors:
`InvalidInput` (neither an image nor `unused`, or not an id), `NotFound`
(not unpacked here), `InUse`, `Unsupported` `{reason: "host"}`.

### machine.status

The machine's state now, its data disk's free space, the provider tool's
version and why it is down.

```text
JS      await layerfs.machine.status(name)
Python  layerfs.machine.status(name)
CLI     layerfs machine status <name>
```

```text
{ name, platform, state, free_disk, tool_version, down_reason,
  stores: [name], op?: { id, name, phase, started, uncertain } }
```

`free_disk` is the bytes free at `/layerfs/data` as the guest sees them;
`down_reason` on Linux `native` names missing host facts
(`missing: host:root, ...`,
[`platforms/native/observe.rs`](../../../../control/crates/layerfs-platform/src/platforms/native/observe.rs)).
`native` answers `running` here (`ready` in `list` and `get`). Unlike `get`, a
failed probe is the call's error: `Unavailable`, `Denied`, `Failed`, or
`Unknown` when a provider command did not end in time
([`setup/tool.rs`](../../../../control/crates/layerfs-platform/src/setup/tool.rs));
a read changes nothing. An `unresolved` machine is cleared by `machine.remove`.

### machine.stats

What a machine has and uses now: its processors, memory and data disk, each
sqlite Store's size and how many of its Sandboxes have a running daemon
(layerfs-project#164). A monitoring read for a UI that polls: it is in the
MCP server's `ui` bucket, not in an agent profile, and writes no call-log
line.

```text
JS      await layerfs.machine.stats(machine)
Python  layerfs.machine.stats(machine)
CLI     layerfs machine stats <machine>
```

```text
{ machine, platform, observed_at,
  worker: { version, pid, started_at, protocol, bundle } | null,
  resources: { cpus, memory_total, memory_used,
               disk_total, disk_used, disk_free, scope, reason? },
  data_disk: { path, fs, scope, volume, inodes_free },
  stores: [ { name, bytes, allocated } ],
  sandboxes: { recorded, live },
  complete, reason? }
```

| Field | Meaning | Source |
| --- | --- | --- |
| `observed_at` | ms since the epoch on the side that answered | [`inspect/stats.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/stats.rs) `serve` |
| `worker` | the machine's worker process: its build's `version`, `pid`, `started_at` (ms since the epoch, when it began to serve), worker `protocol` and the `bundle` it was installed from. `null` on a machine whose calls run in the caller's process (`native`) | [`worker_server/ops.rs`](../../../../control/crates/layerfs-control/src/worker_server/ops.rs) `about` |
| `resources.cpus` | the processors the answering process may run on: where the worker runs in a container with a processor quota (`apple-container`), the quota, which can be less than the processors the guest kernel lists | [`inspect/host.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/host.rs) |
| `resources.memory_total`, `memory_used` | bytes. Linux: `MemTotal` and `MemTotal - MemAvailable` of `/proc/meminfo`. macOS: `hw.memsize`, and that less the free, file-backed and purgeable pages, from one run of `/usr/sbin/sysctl` | `host.rs` |
| `resources.disk_total`, `disk_used`, `disk_free` | one `statvfs` of the filesystem that holds the machine's data. `disk_used` is the total less every free block; `disk_free` is what the answering process may use, so the two can sum to less than the total where blocks are reserved. On a machine whose data is on its own volume or disk (`lima`, `apple-container`) these are that volume's sizes, not the whole VM's | [`home/capacity.rs`](../../../../control/crates/layerfs-home/src/home/capacity.rs) `filesystem` |
| `resources.scope` | whose processors and memory they are: `host` (`native`), `vm` (`lima`, `apple-container`), `engine_host` (`docker`: the kernel the Engine runs on, a VM on macOS and Windows) | `stats.rs` `scopes` |
| `resources.reason` | which values are null and why: `memory_not_read`, `disk_not_read`, joined by `, `; absent when all were read | `stats.rs` |
| `data_disk.path`, `fs` | the folder whose filesystem was measured, as the answering side names it (the home, or the worker's root), and its type (`apfs`, `ext4`, `xfs`, `btrfs`, `overlay`, ..., else the magic number in hex) | `capacity.rs` |
| `data_disk.scope` | what that filesystem is: `host_fs` (`native`; `docker` on a Linux host, where a volume is a folder of the host's filesystem), `vm_disk` (`docker` on macOS and Windows: the Engine VM's one disk, shared with every image and volume), `volume` (`lima`, `apple-container`: the machine's own disk) | `stats.rs` `scopes` |
| `data_disk.volume` | the provider's name of the volume or disk, from the machine's record; `null` on `native` | [`adapter.rs`](../../../../control/crates/layerfs-platform/src/adapter.rs) `data_volume` |
| `data_disk.inodes_free` | file slots free; `null` where the filesystem counts none | `capacity.rs` |
| `stores[]` | the sqlite Stores the home records on the machine: `bytes` is the length of `store.sqlite` plus its `-wal`, `allocated` their blocks, each from one `lstat`; no Store file is opened. A sparse file shows in the difference; a clone counts in full. Both `null` when the file is not there, or when the deadline came first | `stats.rs` `store_bytes` |
| `sandboxes.recorded`, `live` | the Sandboxes the home records on the machine, and how many of them have a daemon process that still runs (the check of `sandbox.list`'s `live`); `live` is `null` when the deadline came first | [`inspect/live.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/live.rs) |
| `complete`, `reason` | `false` with `reason: "deadline"` when the request's soft budget (4 s) ended the Store or Sandbox part early; what was read is answered | `stats.rs` |

It is one [inspect](../../control/sandboxes/inspect.md) request, kind
`machine_stats`: in this process on `native`, else one request to the
machine's worker. It takes no lock or lease, opens no Store, asks no daemon
and no provider tool, starts nothing and writes nothing. PostgreSQL Stores
are not listed: their data is not on a machine.

```text
$ layerfs machine stats native
{"machine":"native","platform":"native","observed_at":1794776707857,"worker":null,"resources":{"cpus":14,"memory_total":38654705664,"memory_used":31340068864,"disk_total":994662584320,"disk_used":641705943040,"disk_free":352956641280,"scope":"host"},"data_disk":{"path":"/Users/me/.layerfs","fs":"apfs","scope":"host_fs","volume":null,"inodes_free":3446842200},"stores":[{"name":"shop","bytes":217088,"allocated":217088}],"sandboxes":{"recorded":12,"live":10},"complete":true}
```

Errors: `NotFound` (no such machine); `Unavailable` (the machine is stopped or its worker
did not answer within 8 s; nothing starts it); `Busy` (two inspect requests
already run in that process); `InvalidInput` (a worker of a build without
the kind, or over 4096 Stores or Sandboxes on one machine).

### machine.files

Reads what is on a machine: its LayerFS folders, one page of a folder or one
window of a file (layerfs-project#178). A read for a UI that shows a tree:
no lock, no lease, no daemon call, nothing written, no call-log line (the
call is `quiet`), and the MCP server does not list it. Its options, a folder
page, a file window and their fields are those of
[`sandbox.files`](sandbox.md#sandboxfiles); what differs is where a path may
lead.

```text
JS      await layerfs.machine.files(machine, path?, { depth, cursor, limit, offset, length })
Python  layerfs.machine.files(machine, path=None, *, depth=None, cursor=None, limit=None, offset=None, length=None)
CLI     layerfs machine files <machine> [<path>] [--depth <depth>] [--cursor <cursor>] [--limit <limit>] [--offset <offset>] [--length <length>]
```

**The roots** (no `path`): a folder page that is never continued. Each entry
is a folder with a `role`, in this order; a folder that is not there is left
out. LayerFS owns the order and the paths, so a client shows roles and never
writes a path of its own.

| `role` | Folder on docker, Lima and Apple Container | Folder on `native` |
| --- | --- | --- |
| `data` | `/layerfs/data`, the worker's root, with `free_bytes`: what its filesystem has left | — |
| `stores` | `/layerfs/data/store` | `<home>/store` |
| `sandbox_records` | `/layerfs/data/sandbox` | `<home>/sandbox` |
| `run_folders` | `/layerfs/data/machine/native/run`: one folder per Sandbox | `<home>/machine/native/run` |
| `images` | `/layerfs/data/machine/native/run/images`: the Sandbox image cache | — (inside `run_folders`) |
| `staging` | `/layerfs/data/incoming`: folder copies of a `project.init` | — |
| `logs` | `/layerfs/data/log` | `<home>/log` |
| `state` | `/layerfs/state`: the worker's pid and secret | — |
| `bundle` | `/layerfs/bundle`: the installed build | — |
| `image` | `/layerfs/image`: a custom Sandbox image copied in (never on docker) | — |
| `machine` | `/`: the whole machine | — |

A client offers a second view ("whole filesystem") only when the answer has
the `machine` root. `native` has none: that machine is its owner's own
computer, and a path outside its LayerFS folders is `InvalidInput`.

**A path** is absolute, with no `..`, and is read in one of two views
([`inspect/machine.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/machine.rs)):

| | In a LayerFS folder (`data`, `state`, `bundle`, `image`; on `native` its four) | Anywhere else, from `/` |
| --- | --- | --- |
| Links | none is followed, on the way or at the end | one on the way resolves inside the machine's root (`/lib` of a merged-`/usr` system); the last name is never followed |
| Mounts | none is entered: `mount` | entered when its type is `ext4`, `xfs`, `btrfs`, `zfs`, `f2fs`, `overlay` or `tmpfs`; any other (`fuse.*`, `nfs`, `cifs`, `9p`, `virtiofs`, `fakeowner`, `sshfs`, an unknown one) is `mount` with its `fs`, never `stat`ed or entered |
| What is opened | everything but what LayerFS keeps private | a file others may read (`o+r`) and a folder others may list and search (`o+rx`), under folders that are so too; the rest is `unreadable` |
| `system` | — | `/proc`, `/sys`, `/dev`, `/run`: listed, never `stat`ed or entered |
| `restricted` | `store.sqlite*`; a run folder's `config/` and `overlay/`; `control.json`, `worker.json`; `lock`, `*.lock`; a Store's `open/`; `/layerfs/state/secret` | `store.sqlite*` |
| LayerFS folders | listed with their `role` | listed with their `role` and not entered: each is read by its own path, whatever filesystem it is on |
| Workspace mounts | an entry of a run folder's `workspaces/` is `mount`, with `workspace` (its id) from the Workspace records; never `stat`ed | — |

`worker.log`, `worker.1.log` and a run folder's `daemon.log` are listed as
files with `role: "log"` and read with the [`logs`](logs.md) calls.

```text
$ layerfs machine files native
{"kind":"folder","entries":[{"path":"/Users/me/.layerfs/store","depth":1,"kind":"folder","size":null,"mode":493,"mtime":1794781376540,"role":"stores"},{"path":"/Users/me/.layerfs/sandbox","depth":1,"kind":"folder","size":null,"mode":493,"mtime":1794781376540,"role":"sandbox_records"},{"path":"/Users/me/.layerfs/machine/native/run","depth":1,"kind":"folder","size":null,"mode":493,"mtime":1794781376540,"role":"run_folders"},{"path":"/Users/me/.layerfs/log","depth":1,"kind":"folder","size":null,"mode":493,"mtime":1794781376540,"role":"logs"}],"next":null,"has_more":false}
```

| Error | When |
| --- | --- |
| `InvalidInput` | an option outside its range; a `cursor` with no `path`, or not an entry of the folder; a path that is not absolute or holds a `..`; on `native`, a path outside the LayerFS folders; a `restricted`, `unreadable`, `system` or `mount` entry, or a path in one; a link at the end, or, in a LayerFS folder, on the way; a FIFO, socket or device; a path a link leads into a mount that is not entered; a file or folder the system refuses to open (its permissions, or a link another user made in a shared folder such as `/tmp`) |
| `NotFound` | no machine of that name; nothing is at the path |
| `Unavailable` | the machine is stopped, or its worker did not greet in 2 s or answer in 8 s; the read ran past 6 s. Nothing starts a machine |
| `Busy` | two inspection reads already run on that machine; read again |
| `Unsupported` | a Linux kernel before 5.6 (no `openat2`), or before 5.8 for a path read from `/` (no mount id in `statx`); nothing else is tried |

It is one [inspect](../../control/sandboxes/inspect.md#files) request, kind
`machine_files`: in this process on `native`, else one request to the
machine's worker, which runs as root on Lima and Apple Container and in the
worker container on docker; the world-readable rule makes the answer the
same whoever it runs as.

### machine.start

Starts a stopped machine and its `layerfs worker`, waits until the worker
answers its secret, and answers the summary. The only call that starts a
machine.

```text
JS      await layerfs.machine.start(name, { on_progress })
Python  layerfs.machine.start(name, *, on_progress=None)
CLI     layerfs machine start <name> [--progress]
```

`on_progress` receives `{ stage, state, elapsed_ms? }` for the stages
`bundle-validation`, `sandbox-image` (default Docker image only),
`provider-and-worker` and `worker-compatibility`; each started stage ends
`completed` or `failed`.

```text
 native, or a worker that already answers --> get(name)
 take machine/<name>/start.lock (wait <= 600 s)
 validate the bundle's protocol and architecture
 provider start: boot the VM or container, copy changed bundle files,
                 start the worker; wait for its secret (readiness)
 hello: compare protocol and bundle, then write worker.json --> get(name)
```

`start.lock` is shared with `create`, `stop` and `remove`
([`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs));
the home lock is held only to write records, never while a VM boots. `docker`
runs the privileged worker container (`--pull never`) from the recorded image,
else the bundle's image loaded once; `lima` and `apple-container` boot the VM
when stopped. Guest installation copies only changed or absent bundle files and
publishes the manifest last ([`image.rs`](../../../../control/crates/layerfs-machine/src/image.rs),
[`power.rs`](../../../../control/crates/layerfs-machine/src/power.rs)).

| Error | When |
| --- | --- |
| `Unavailable` | `start.lock` held by another process for 600 s; or the worker is of another build (`detail.worker`, `expected`, `bundle`): the machine runs and is recorded ([`worker/hello.rs`](../../../../control/crates/layerfs-machine/src/worker/hello.rs)) |
| `Unsupported` | no sealed Linux bundle (`detail.missing: ["bundle:<dir>"]`); `docker` with no image and no `sandbox-image.tar` ([`bundle.rs`](../../../../control/crates/layerfs-machine/src/bundle.rs)) |
| `Unknown` | a provider command timed out or its answer was lost: the VM or worker may be up with no `worker.json`; read `machine.status` first |

### machine.stop

Stops a machine's worker and, where it has one, its VM; disks and Stores
stay.

```text
JS      await layerfs.machine.stop(name, { force })
Python  layerfs.machine.stop(name, *, force=None)
CLI     layerfs machine stop <name> [--force]
```

`native` stops nothing; `docker` ends its worker container only; `lima` and
`apple-container` end the worker and VM. `worker.json` is deleted and the
state recorded `stopped`. Nothing stops a machine because it is unused.
Errors: `InUse` while Sandboxes are recorded on it and `force` is not set
(`detail.related`, `can_stop_machine`); `InvalidInput` on `docker` when the
container named for the machine is not a LayerFS worker (its role label), which
is left alone; `Unavailable` (`start.lock` held 600 s,
or the provider did not answer: records stay as they were); `Unknown` (a
provider command timed out: the machine may be stopped while still recorded
with its worker).

`force` (Bool, default false) stops the machine while Sandboxes, mounted
Workspaces or commands are active; their uncommitted state may be lost. It goes
straight to the provider: it does not start the machine, call its worker,
commit, unmount Workspaces one by one, or delete any machine, Sandbox, Store or
disk record. On `native` it ends only this home's recorded Sandbox runtimes and
commands, without opening the Store or calling the daemon, and the machine
stays `ready`. On `docker`, ending the worker container ends its Sandboxes; the
Store volume stays. `lima` uses `limactl stop --force`, also when the host
agent reports a broken VM. It returns the recorded summary without a worker
probe, plus `forced: true`, `stopped_sandboxes: [name]` and
`unknown: [workspace]` (`sandbox:*` when a worker-side Workspace frontier could
not be listed; these are not saved Commits). Stopped Sandboxes are recorded
`gone` for explicit reclaim, or `unresolved` when the stop could not be
confirmed. A Sandbox that never started is neither stopped nor listed. On `native` a Sandbox is uncertain only once its stop has been
sent: a refusal before that (no recorded runtime, or a recorded process
identity that no longer matches) leaves that Sandbox as it was, and the stop
goes on to the next one. The call then answers the first such refusal, with
`detail.refused_sandboxes`, `stopped_sandboxes` and `unknown`, and leaves no op
on `native`. If recording the outcome fails, the stop's own refusal is still
answered, with `detail.custody_errors`. A later forced stop may settle an
uncertain stop this process left, or one whose process has ended; one a live
process still holds is `Busy`. A provider failure or uncertain outcome is
reported, never replayed or turned into success
([`machine_force.rs`](../../../../control/crates/layerfs-control/src/machine_force.rs)).

### machine.create

Makes a VM machine on this host, copies the sealed Linux bundle in and
starts its worker.

```text
JS      await layerfs.machine.create(name, platform, { cpus, memory, disk, image, os_image, on_progress })
Python  layerfs.machine.create(name, platform, *, cpus=None, memory=None, disk=None, image=None, os_image=None, on_progress=None)
CLI     layerfs machine create <name> <platform> [--cpus <cpus>] [--memory <memory>] [--disk <disk>] [--image <image>] [--os-image <os-image>] [--progress]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `name` | Name | not `native` or `docker`; not taken |
| `platform` | Name | an enabled created platform the host supports: `apple-container` or `lima` (macOS hosts) |
| `cpus` option | Int | default 2 (`DEFAULT_CPUS`, [`spec.rs`](../../../../control/crates/layerfs-machine/src/spec.rs)) |
| `memory` option | Size | default 4 GiB |
| `disk` option | Size | default 20 GiB, the data disk at `/layerfs/data` |
| `image` option | Text | the Sandbox image archive or content identity |
| `os_image` option | Text | the VM's operating-system image (an installed OCI reference for Apple Container, an image location for Lima); default an installed prepared image, else the provider's stock image; recorded and answered as `os_image` |
| `on_progress` option | Function | the `machine.start` stages, plus `{ phase: "create", machine }` |

The record keeps `vm_name` (`layerfs-<home id prefix>-<name>`, so two homes
never share a VM; every character of the name that is not a letter or digit
becomes `-`). The built-in `docker` machine's record keeps one too, from a
new home's first call. A record without `vm_name` (an existing home's
`docker`) means `lfs-<home id prefix>-<name>`, the formula before
layerfs-project#180, so it keeps its container and volume. Under the home
lock a create refuses `InvalidInput` a name longer than 46 characters (a VM
name is at most 63, the prefix takes 17), then a name whose VM name is
longer than the platform's provider takes on this host
(`layerfs_platform::name_limit`, the adapters' `name_limit`: only `lima` has
one, since `limactl create` refuses an instance whose
`<Lima home>/<VM name>/ssh.sock.<16 digits>` reaches 104 characters; the
message states the longest name that fits), and `Exists` a name whose VM name,
under either formula, is another machine's. The name is claimed with an
`op` `create` under the home lock; the boot runs under `start.lock` only.

| Adapter outcome | Record afterwards |
| --- | --- |
| running | `state: running`, `op` dropped, `worker.json` written |
| failed before the VM existed | the machine folder is removed |
| failed after (`detail.vm_exists: true`) | `state: failed` with `error`; the VM is kept for inspection |
| `Unknown` | everything kept; `op` uncertain with `error`; logged to `log/uncertain.jsonl` |

Errors: `Exists`; `Unsupported` (a removed or unsupported platform, a missing
tool in `detail.missing`, no sealed bundle). After `Unknown`, read
`machine.status` and the provider's own listing; `machine.remove` clears it
([`create.rs`](../../../../control/crates/layerfs-machine/src/create.rs)).

### machine.remove

Deletes a VM machine with its data disk; also clears an unresolved create or
remove.

```text
JS      await layerfs.machine.remove(name)
Python  layerfs.machine.remove(name)
CLI     layerfs machine remove <name>
```

Returns `{ name, removed: true }`. Built-in machines are never removed. A
running machine is removed as it is. A machine of a removed platform is only
forgotten: its record goes and the answer adds a `note` naming what was not
touched (for `microsandbox` the `msb remove --force <vm>` and
`msb volume remove <vm>-data` commands; for `firecracker` its folder and disk
in its Lima host) ([`remove.rs`](../../../../control/crates/layerfs-machine/src/remove.rs)).

| Error | When |
| --- | --- |
| `Busy` | a live process's create or remove holds it (a gone process's uncertain intent does not refuse) |
| `InUse` | sqlite Stores, attached PostgreSQL Stores, Sandboxes or old Firecracker records it hosted are on it (`detail.related`, `can_remove_machine`) |
| `Unavailable`, `Failed` | the provider refused or failed; the record is put back |
| `Unknown` | the provider timed out: `op` kept, uncertain; read `machine.status`, then remove again |

### machine.update

Changes a machine's recorded settings.

```text
JS      await layerfs.machine.update(name, { cpus, memory, image, docker, wsl, daemon })
Python  layerfs.machine.update(name, *, cpus=None, memory=None, image=None, docker=None, wsl=None, daemon=None)
CLI     layerfs machine update <name> [--cpus <cpus>] [--memory <memory>] [--image <image>] [--docker <docker>] [--wsl <wsl>] [--daemon <daemon>]
```

| Option | Applies to | Rule |
| --- | --- | --- |
| `image` | any machine | on `docker`, the worker container's image; an absolute path is the Sandbox image archive |
| `docker` | built-in only | the Docker socket this home's tools use |
| `wsl` | built-in only, Windows hosts | the WSL distribution |
| `daemon` | built-in only | the absolute path of an existing file: the daemon its Sandboxes run |
| `cpus`, `memory` | VM machines | `InvalidInput` on a built-in; on a VM `Unsupported` ("not built yet"), nothing changed |

Refused `InUse` while Sandboxes are recorded on the machine (a running Sandbox
keeps what it started with); a new setting takes effect at the next
`machine.start` or `sandbox.create`
([`update.rs`](../../../../control/crates/layerfs-machine/src/update.rs)).

## Family errors

| Error | When | Source |
| --- | --- | --- |
| `InvalidInput`, `NotFound` | a bad name or option; no machine of that name | [`record.rs`](../../../../control/crates/layerfs-machine/src/record.rs) |
| `Busy`, `Unresolved` | another operation's `op` on the machine | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs) |
| `Unsupported` | a platform this host or build lacks; a removed platform; a build without the `machine` feature | [`machine/create.rs`](../../../../api/crates/layerfs-api/src/machine/create.rs), [`spec.rs`](../../../../control/crates/layerfs-machine/src/spec.rs) |
| `Unavailable`, `Denied` | a provider or guest that does not answer, or refuses | [`error.rs`](../../../../control/crates/layerfs-platform/src/error.rs) |
| `Unknown` | the adapter timed out or lost its answer; everything made is kept and the `op` stays uncertain | [`create.rs`](../../../../control/crates/layerfs-machine/src/create.rs), [`remove.rs`](../../../../control/crates/layerfs-machine/src/remove.rs) |

## Related

- [Machines](../../control/machines/README.md), [platforms](../../control/machines/platforms.md), [workers and reach](../../control/machines/workers-and-reach.md).
- [store family](store.md) (`push` places a sqlite Store on a machine), [sandbox family](sandbox.md).
- [Doctor](../../control/doctor/README.md) for a machine's health.
