# machine family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `machine` calls: list and read the machines that hold sqlite Stores and run Sandboxes, manage their Sandbox images, read what they have and use, their files and their worker's log, and create, start, stop, update, remove and check them, within the [SDK](README.md). `machine.doctor` is one of [the doctors](../../control/doctor/README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/machine`](../../../../api/crates/layerfs-api/src/machine) (one file per call), [`layerfs-control/src/machine.rs`](../../../../control/crates/layerfs-control/src/machine.rs), [`layerfs-machine/src`](../../../../control/crates/layerfs-machine/src), [`layerfs-platform/src`](../../../../control/crates/layerfs-platform/src), [`layerfs-sandbox/src/images.rs`](../../../../control/crates/layerfs-sandbox/src/images.rs), [`layerfs-sandbox/src/inspect`](../../../../control/crates/layerfs-sandbox/src/inspect) |
| Runs by default | every call in the caller's process, through the platform adapter (provider CLI or Docker); `stats`, `files` and `log` are each one inspection request served where the machine's data is, and `images`, `load_image` and `prune` reach the machine's image cache the same way. The guest worker is built without the `machine` feature, so there every machine call is `Unsupported` before any effect |
| Design | [Machines](../../control/machines/README.md), [platforms](../../control/machines/platforms.md), [workers and reach](../../control/machines/workers-and-reach.md) |

## Calls

```text
 reads (start nothing):  list   get   images   stats   files   log   doctor
                         (--with live probes; --with space walks; --with docker)
 Sandbox images:         load-image (copy from the host's Docker)   prune

 built in: native, docker ---------------------- update (image, docker,
                                                  wsl, daemon)
 create <name> <lima | apple-container> -----> ready
                                          |     |      ^
                                          |   stop     |
                                          |     v      |
                                          |  stopped --+ start
                                          v
            remove (VM + disk; a removed platform's: its record only)
```

| Call | Short answer | MCP |
| --- | --- | --- |
| [`machine.list`](#machinelist) | `{ items: [{ machine, platform }] }` | `admin` |
| [`machine.get`](#machineget) | `{ platform, state }` | `admin` |
| [`machine.images`](#machineimages) | `{ items: [{ image }] }` | `admin` |
| [`machine.load_image`](#machineload_image) | `{ image }` | `admin` |
| [`machine.prune`](#machineprune) | `{ freed_bytes }` | `admin` |
| [`machine.stats`](#machinestats) | `{ cpus, memory_used_bytes, memory_total_bytes, disk_free_bytes, disk_total_bytes }` | `admin` |
| [`machine.files`](#machinefiles) | `{ items }`, or a file window `{ text }` | `admin` |
| [`machine.log`](#machinelog) | `{ items: [{ text }] }` | `admin` |
| [`machine.start`](#machinestart) | `{ ok }`, or `{ unchanged }` | `admin` |
| [`machine.stop`](#machinestop) | `{ ok }`, or `{ unchanged }` | `admin` |
| [`machine.create`](#machinecreate) | `{ ok }` | `admin` |
| [`machine.remove`](#machineremove) | `{ ok }` | `admin` |
| [`machine.update`](#machineupdate) | `{ ok }` | `admin` |
| [`machine.doctor`](#machinedoctor) | the doctor report | `admin` |

Every call names the machine by the positional `machine`, except `create`,
which takes `name` and `platform`. A folder scope refuses every machine
call.

**A machine's state** comes from its record: `ready`, `busy` (with `op {
name, phase, started_at }`: a `create`, `start`, `stop` or `remove`
running), `stopped`, or `broken` with `reason` and `fix`; `unknown` is never
recorded. Reasons: `engine_down` (Docker is not running; `fix` starts it:
`open -a Docker` on macOS, `sudo systemctl start docker` on Linux),
`not_answering`, `outdated`, `missing`, `failed`, `unresolved` and
`unsupported`. A records-only read answers the last state word and reason a
start or a live read recorded, and `observed_at` from the last live read.

Firecracker, Microsandbox and SSH machines were removed in 2.0: a record for
one is shown by `list` and `get` as `broken` `unsupported`, `remove`
removes only its record, and every other call on it is `Unsupported`
([`calls/platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs)).
Nothing but `machine start` starts a machine; a call that needs a stopped
machine is `Unavailable` with `fix: layerfs machine start <machine>`
([`place.rs`](../../../../control/crates/layerfs-store/src/place.rs)).

### machine.list

Every machine, from the home's records.

```text
CLI     layerfs machine list [--with live]
Node    await layerfs.machine.list({ with })
Python  layerfs.machine.list(*, with_=None)
```

| Answer | Keys |
| --- | --- |
| short | `items` |
| long | `items` |
| `items[]` short | `machine`, `platform`; when set: `state`, `op`, `reason`, `fix`, `observed_at` |
| `items[]` long | `machine, platform, state, op, reason, fix, observed_at, backend, supports, cpus, memory_bytes, disk_bytes, image, os_image, docker, wsl, daemon, message, stores, stores_count, sandboxes, sandboxes_count, created_at` |
| `items[].supports[]` short | each a `support` |
| `items[].stores[]` short | each a `store` |
| `items[].sandboxes[]` short | each a `sandbox` |
| `--with live` | adds `observed_at` |
| call | writes one call-log line; MCP `admin` |

Reads every `machine/<name>/record.json` without the home lock and probes
nothing; each row has `machine` and `platform`, and its state with its
companion when not ready. `--with live` probes each machine's provider and
says hello to its worker, all at the same time, within 10 s, starting
nothing; the state, its companion, `message` and `observed_at` then come
from the probe, and `unknown` means the probe did not answer
([`list.rs`](../../../../control/crates/layerfs-machine/src/list.rs)). The
record is written only when the state word or reason changed (which moves
the home's `generation`); `observed_at` is kept in `observed.json` beside
the record, which does not.

The long row: `backend` is the Sandbox backend on this host (`native`,
`namespaces`, also on `docker`, or `-` for a removed platform); `supports`
is a list of words: `sqlite`, `postgresql_sandbox`, and `log` when the
machine has a worker whose log can be read (clients read this and never test
platform names; [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs));
`cpus`, `memory_bytes`, `disk_bytes` are the recorded size (null on a
built-in machine); `stores` and `sandboxes` list the first 20 names, each
count only past those
([`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs)).

Run on macOS native with a debug build:

```console
$ layerfs machine list
{"items":[{"machine":"docker","platform":"docker","state":"stopped"},{"machine":"native","platform":"native"}]}
```

### machine.get

A machine's record and state.

```text
CLI     layerfs machine get <machine> [--with live]
Node    await layerfs.machine.get(machine, { with })
Python  layerfs.machine.get(machine, *, with_=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `machine` | name | a machine name |

| Answer | Keys |
| --- | --- |
| short | `platform`, `state`; when set: `op`, `reason`, `fix`, `observed_at` |
| long | `platform, state, op, reason, fix, observed_at, machine, backend, supports, cpus, memory_bytes, disk_bytes, image, os_image, docker, wsl, daemon, message, stores, stores_count, sandboxes, sandboxes_count, created_at` |
| `supports[]` short | each a `support` |
| `stores[]` short | each a `store` |
| `sandboxes[]` short | each a `sandbox` |
| `--with live` | adds `disk_free_bytes`, `tool_version`, `worker` |
| call | writes one call-log line; MCP `admin` |

A machine's record and state, as a `list` row with `platform` and `state`
first. `--with live` probes it once, as `list --with live` does, and adds
the `live` section: `disk_free_bytes`, `tool_version` (the provider tool's
first line of `--version`) and `worker` (`{protocol, bundle}` as it
answered), each null when not read. A machine held by an `op` is not
probed; `get` never clears an `op`. It is the read that settles a lost
reply of `machine start` or `stop`.

```console
$ layerfs machine get docker
{"platform":"docker","state":"stopped"}
$ layerfs machine get native --with live
{"platform":"native","state":"ready"}
```

### machine.images

The Sandbox images a machine holds.

```text
CLI     layerfs machine images <machine> [--with docker]
Node    await layerfs.machine.images(machine, { with })
Python  layerfs.machine.images(machine, *, with_=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `machine` | name | a machine name |

| Answer | Keys |
| --- | --- |
| short | `items` |
| long | `items, machine, mode, architecture, disk_free_bytes` |
| `items[]` short | `image`; when set: `refs`, `size`, `default`, `state`, `op`, `reason`, `fix` |
| `items[]` long | `image, refs, size, default, state, op, reason, fix, message, staged_bytes, architecture, source, sandboxes, sandboxes_count, last_used_at` |
| `items[].refs[]` short | each a `ref` |
| `items[].sandboxes[]` short | each a `sandbox` |
| `--with docker` | adds `images`, `reason`, `message` |
| call | writes one call-log line; MCP `admin` |

The Sandbox images a machine holds, read without starting it
([`images.rs`](../../../../control/crates/layerfs-sandbox/src/images.rs)):
one item per image, published unpacked images with their sidecars
(`machine/native/run/images/<hex>.2.json`), what the archives its Sandboxes
read hold, and what copies left. A published image is `ready` (a stale part
beside it is its `staged_bytes`); a part a live copy holds is `busy` with
`op {name: load_image, phase, started_at}` (the phase the copy recorded in
its lock's owner record); a part no copy holds, or an unpack that stopped
before it was recorded, is `broken` `partial` with `fix: layerfs machine
load-image <machine> sha256:<hex>`, which resumes or rebuilds it, and
`size` null. `image` is the Docker image Id for an imported image and the
config digest for an archive's; `sandboxes` names the Sandbox records on
this machine using it; `last_used_at` is the last create's time; `default`
marks the record's image, else the archive's only one. macOS `native`
answers `mode: "host"` with no items.

`--with docker` reads the host's Docker (one `docker image ls`, one batched
`docker image inspect`, 10 s each) through the `docker` machine's socket,
else Docker's default context, and adds `images` it could copy (an image
the machine holds `ready` is not offered), each `{image, refs, size, os,
architecture, usable, needed_bytes, fits, unusable}`. `size` is what the
image's layers unpack to (the sum of `docker history`'s layer sizes);
`needed_bytes` is `2 × size × 1.05` and `fits` is `needed_bytes <=` the
cache's free bytes. A Docker that does not answer fills `reason` and
`message` and never fails the call
([`docker.rs`](../../../../control/crates/layerfs-sandbox/src/docker.rs)).

```console
$ layerfs machine images native
{"items":[]}
```

### machine.load_image

Copy an image from the host's Docker to a machine without creating a Sandbox.

```text
CLI     layerfs machine load-image <machine> <image> [--progress]
Node    await layerfs.machine.load_image(machine, image, { on_progress })
Python  layerfs.machine.load_image(machine, image, *, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `machine` | name | a machine name |
| `image` | image | a Docker reference, image Id or `sha256:` digest in the host's Docker |
| `on_progress` option | function |  |

| Answer | Keys |
| --- | --- |
| short | `image`; when set: `reused`, `dropped` |
| long | `image, reused, dropped, outcome, machine, refs, size, dropped_count, expanded_bytes` |
| `refs[]` short | each a `ref` |
| `dropped[]` short | each a `path` |
| call | sends progress; writes one call-log line; MCP `admin` |

Copies an image to a machine as `sandbox create --image` does, without a
Sandbox ([`image_load.rs`](../../../../control/crates/layerfs-sandbox/src/image_load.rs)).
The machine is asked first: an image it holds answers `reused` and needs
neither Docker nor a byte sent. Otherwise `docker image inspect` resolves
it, and one copy of that Id on that machine at a time holds
`<home>/run/image-lock/<machine>/<hex>.lock` (an exclusive `flock` with its
owner's process identity and the phase the copy is in): a second copy waits,
with a `{phase: "lock"}` event each second and no deadline, then reuses what
the first published. Holding the lock, a copy passes the machine's prune
gate before it writes anything, so it waits for a prune under way.

The machine's free bytes are checked before anything is created on either
side. Only then does the host create an exclusive export container
(labelled with this home, removed with its anonymous volumes on exit or
error; one a killed copy left is swept by the next copy of the same image to
the same machine, and any other by the next copy of an image no copy holds
([Checks: leftovers](../../control/doctor/checks.md#leftovers))) and stream `docker export` without the init layer's
files and device or FIFO entries, with the modification times of the folders
the init layer stamps normalized to epoch 0, so separate exports are
byte-identical for resume. Sparse entries are expanded while streaming, and
the expanded bytes count against `Capacity`. The bytes go in 1 MiB frames to
a staged part, which the machine checks (size and sha256), unpacks once into
`<hex>.2/` and publishes with its sidecar `<hex>.2.json`, the commit point;
the staged tar is deleted. A staged part a stopped copy left is resumed when
its bytes hash as this export's prefix, else restarted. `dropped` lists the
FIFO and device paths the copy left out (the first 20, `dropped_count` only
past those); `expanded_bytes` (long form) counts sparse expansion.

Progress: `inspect`, `lock`, `export`, `send` (`done`, `total`: an
estimate, never below `done`, until the last `send` event, whose `done` and
`total` are the bytes sent), `unpack`.

| Refusal | When | Next |
| --- | --- | --- |
| `Capacity` | the machine has no room for the image, checked before the first effect (`detail { needed_bytes, free_bytes, path }`) | `fix: layerfs machine prune <machine> --unused` |
| `Unsupported` | the image is for another architecture or OS (`detail.architecture` or `os`); macOS `native` runs no image (`{reason: "host"}`) | |
| `InvalidInput` | Docker has no such image, or a reference more than one held image claims (`detail.images`) | `machine prune` one of them |
| `Unavailable` | Docker or the machine's worker cannot be reached (`detail.what`: `docker` or `machine`) | |
| `InUse` | a stranded unpack of the image is in use | |
| `Failed` | the export failed or changed while it was sent; a staged part does not match what was sent; a frame arrived out of order | |

It was `machine.load`, removed with no alias.

### machine.prune

Remove a Sandbox image from a machine, or every unused one.

```text
CLI     layerfs machine prune <machine> [<image>] [--unused]
Node    await layerfs.machine.prune(machine, image?, { unused })
Python  layerfs.machine.prune(machine, image=None, *, unused=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `machine` | name | a machine name |
| `image` | image | an image the machine holds; left off with `--unused`; optional |
| `unused` option | bool | remove every image no Sandbox uses, except the machine's default |

| Answer | Keys |
| --- | --- |
| short | `freed_bytes`; when set: `skipped` |
| long | `freed_bytes, skipped, outcome, machine, removed, removed_count, skipped_count` |
| `removed[]` short | each a `image` |
| `skipped[]` short | `image`, `message` |
| `skipped[]` long | `image, message` |
| call | writes one call-log line; MCP `admin` |

Removes one image, or with `--unused` every one no Sandbox uses except the
machine's default, and what copies left: parts no copy holds (even beside an
image a Sandbox uses), unpacks that were not recorded, sidecars with no
image and `.tmp-*` folders. An image is in use while a Sandbox's run folder
names it or a create holds it. `prune` holds the machine's prune gate (an
exclusive `flock` on `<home>/run/image-lock/<machine>/`, creating no file)
from before it reads the copy locks until its removals are done, so a copy
either holds its lock when `prune` reads them or waits until `prune` is
over. `--unused` leaves alone what a live copy holds and lists what it
could not remove in `skipped` (`[{image, message}]`, the first 20). Naming
the default removes it; the next create unpacks it again from its archive.
`freed_bytes` is the free bytes after less before; the long form adds
`outcome`, `machine` and `removed` (the first 20 images, `removed_count`
only past those).

| Refusal | When |
| --- | --- |
| `Busy` | the named image is held by a live copy (`machine load-image` or `sandbox create --image`); it carries that copy's `op` |
| `InUse` | a Sandbox uses the named image (`detail.sandbox_count`) |
| `Failed` | the named image's copy lock cannot be checked |
| `InvalidInput` | neither an image nor `--unused`, or not an image id |
| `NotFound` | no such image on the machine |
| `Unsupported` | macOS `native` (`{reason: "host"}`) |

### machine.stats

A machine's observed processors, memory and data disk, its Stores' sizes and live Sandboxes.

```text
CLI     layerfs machine stats <machine> [--with space]
Node    await layerfs.machine.stats(machine, { with })
Python  layerfs.machine.stats(machine, *, with_=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `machine` | name | a machine name |

| Answer | Keys |
| --- | --- |
| short | each when set: `cpus`, `memory_used_bytes`, `memory_total_bytes`, `disk_free_bytes`, `disk_total_bytes`, `incomplete` |
| long | `cpus, memory_used_bytes, memory_total_bytes, disk_free_bytes, disk_total_bytes, incomplete, disk_used_bytes, reasons, machine, platform, observed_at, scope, cpu_busy_ms, cpu_total_ms, worker, data_disk, stores, sandboxes` |
| `--with space` | adds `observed_at`, `layerfs_bytes`, `outside_bytes`, `parts`, `stores`, `sandboxes`, `unreadable_count`, `incomplete`, `reasons` |
| call | writes no call-log line; MCP `admin` |

What a machine has and uses now, for a UI that polls: its processors,
memory and data disk, each sqlite Store's size, and its Sandboxes' daemons
and usage. It answers the present only: processor values are counters, and
a client keeps its own history and turns two reads into a rate. It is one
[inspection](../../control/sandboxes/inspect.md) request: in this process
on `native`, else one request to the machine's worker. It takes no lock or
lease, opens no Store, asks no daemon and no provider tool, starts nothing
and writes nothing, the call log included
([`inspect/stats.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/stats.rs)).

| Key | Meaning | Source |
| --- | --- | --- |
| `cpus` | the processors the answering process may run on: on `apple-container`, the worker's quota, which can be less than the guest kernel lists | [`inspect/host.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/host.rs) |
| `memory_total_bytes`, `memory_used_bytes` | Linux: `MemTotal` and `MemTotal - MemAvailable` of `/proc/meminfo`. macOS: `hw.memsize`, and that less the free, file-backed and purgeable pages | `host.rs` |
| `disk_total_bytes`, `disk_used_bytes`, `disk_free_bytes` | one `statvfs` of the filesystem that holds the machine's data; `disk_free_bytes` is what the answering process may use, so used and free can sum to less than the total. On `lima` and `apple-container` that is the machine's own data volume | [`home/capacity.rs`](../../../../control/crates/layerfs-home/src/home/capacity.rs) |
| `cpu_busy_ms`, `cpu_total_ms` | the machine's processor time since boot, from the first `cpu` line of `/proc/stat` (busy is user + nice + system + irq + softirq + steal; total adds idle and iowait). The share between two reads is Δbusy / Δtotal, of the kernel `scope` names. Null on macOS (`cpu_unsupported`) and when the read failed (`cpu_not_read`) | `host.rs`, [`inspect/process.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/process.rs) |
| `scope` | whose processors and memory they are: `host` (`native`), `vm` (`lima`, `apple-container`), `engine_host` (`docker`: the kernel the Engine runs on) | `stats.rs` |
| `reasons` | which values are null and why: `memory_not_read`, `disk_not_read`, `cpu_unsupported`, `cpu_not_read`, `deadline` | `stats.rs` |
| `incomplete` | the request's soft budget (4 s) ended the Store or Sandbox part early; what was read is answered | `stats.rs` |
| `worker` | the machine's worker process (`version`, `pid`, `started_at`, `protocol`, `bundle`); null on `native` | [`worker_server/ops.rs`](../../../../control/crates/layerfs-control/src/worker_server/ops.rs) |
| `data_disk` | the folder measured, its filesystem type, its `scope` (`host_fs`, `vm_disk`, `volume`), the provider's `volume` name and `inodes_free` | `capacity.rs` |
| `stores` | `[{store, size, allocated_bytes}]`: the sqlite Stores the home records on the machine, from one `lstat` of `store.sqlite` and its `-wal`; no Store file is opened | `stats.rs` |
| `sandboxes` | `[{sandbox, live, live_reason, started_at, usage, usage_reason}]`: one row per recorded Sandbox, read by the side that runs it from its daemon record, `/proc` and its cgroup; `usage` as [`sandbox get --with stats`](sandbox.md#sandboxget) answers it | [`inspect/live.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/live.rs) |

`--with space` adds `space`, one bounded walk of LayerFS's folders where the
machine's data is ([`inspect/space.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/space.rs)).
Every size is the blocks a part takes, so a sparse file counts what it holds
and a file with several names counts once. `layerfs_bytes` is the sum of
`parts` (`[{part, bytes}]`, the parts `stores`, `sandboxes`, `images`,
`other_runs`, `logs`, `other`); `outside_bytes` is the disk's used bytes
less that, never below 0 (on a Docker VM disk, Docker's images and other
volumes; on `native` the rest of the host's disk), null unless the walk is
complete. `stores` is `[{store, allocated_bytes}]`; `sandboxes` is
`[{sandbox, allocated_bytes, overlays_bytes, writes_bytes, home_bytes,
log_bytes, runtime_bytes, reason}]` from each run folder. `unreadable_count`
counts folders that could not be listed. The walk stops at 250,000 entries
or the 4 s budget (`incomplete`, `reasons`). It opens no file, follows no
link, enters no other device and never enters a Workspace mount point, so a
stopped daemon cannot hang it.

```console
$ layerfs machine stats native
{"cpus":14,"memory_used_bytes":25904775168,"memory_total_bytes":38654705664,"disk_free_bytes":129668911104,"disk_total_bytes":994662584320}
```

Refusals: `NotFound` (no such machine); `Unavailable` (the machine is
stopped or its worker did not answer within 8 s; nothing starts it);
`Busy` (two inspection requests already run there); `InvalidInput` (over
4096 Stores or Sandboxes on one machine). `--with space` was
`machine.space`, removed with no alias.

### machine.files

A machine's files: its LayerFS folders by role, or a folder or file of the machine.

```text
CLI     layerfs machine files <machine> [<path>] [--depth 1] [--limit 100] [--cursor C] [--byte-offset 0] [--byte-length 262144]
Node    await layerfs.machine.files(machine, path?, { depth, limit, cursor, byte_offset, byte_length })
Python  layerfs.machine.files(machine, path=None, *, depth=None, limit=None, cursor=None, byte_offset=None, byte_length=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `machine` | name | a machine name |
| `path` | text | an absolute path of the machine; left off, LayerFS's folders by role; optional |
| `depth` option | int | how many levels of a folder a page walks; default `1` |
| `limit` option | int | at most this many entries; default `100` |
| `cursor` option | text | a page's next_cursor: the entries after it |
| `byte_offset` option | int | where a file window starts; default `0` |
| `byte_length` option | int | at most this many bytes of a file; default `262144` |

| Answer | Keys |
| --- | --- |
| short | each when set: `items`, `next_cursor`, `text`, `binary`, `size`, `next_byte_offset` |
| long | `items, next_cursor, text, binary, size, next_byte_offset, type` |
| `items[]` short | `path`, `type`; when set: `size`, `not_read` |
| `items[]` long | `path, type, size, not_read, role, depth, mode, modified_at, target, fs, workspace, disk_free_bytes` |
| call | pages by `limit` and `cursor`; writes no call-log line; MCP `admin` |

Reads what is on a machine: its LayerFS folders, one page of a folder or one
byte window of a file. Its options, folder page and file window are those of
[`sandbox files`](sandbox.md#sandboxfiles); what differs is where a path may
lead ([`inspect/machine.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/machine.rs)).
It is one inspection request: in this process on `native`, else one request
to the machine's worker; no lock, no lease, no daemon call, nothing written.

**The roots** (no `path`): a page that is never continued. Each entry is a
folder with a `role`, in this order; a folder that is not there is left out.
A client shows roles and never writes a path of its own.

| `role` | Folder on docker, Lima and Apple Container | Folder on `native` |
| --- | --- | --- |
| `data` | `/layerfs/data`, the worker's root, with `disk_free_bytes` | — |
| `stores` | `/layerfs/data/store` | `<home>/store` |
| `sandbox_records` | `/layerfs/data/sandbox` | `<home>/sandbox` |
| `run_folders` | `/layerfs/data/machine/native/run`: one folder per Sandbox | `<home>/machine/native/run` |
| `images` | `/layerfs/data/machine/native/run/images`: the image cache | — (inside `run_folders`) |
| `staging` | `/layerfs/data/incoming`: folder copies of a `project init` | — |
| `logs` | `/layerfs/data/log` | `<home>/log` |
| `state` | `/layerfs/state`: the worker's pid and secret | — |
| `bundle` | `/layerfs/bundle`: the installed build | — |
| `image` | `/layerfs/image`: a custom Sandbox image copied in (never on docker) | — |
| `machine` | `/`: the whole machine | — |

`native` has no `machine` root: that machine is its owner's own computer,
and a path outside its LayerFS folders is `InvalidInput`.

| | In a LayerFS folder | Anywhere else, from `/` |
| --- | --- | --- |
| Links | none is followed, on the way or at the end | one on the way resolves inside the machine's root; the last name is never followed |
| Mounts | none is entered: `not_read: "mount"` | entered when its type is `ext4`, `xfs`, `btrfs`, `zfs`, `f2fs`, `overlay` or `tmpfs`; any other is `not_read: "mount"` with its `fs` |
| What is opened | everything but what LayerFS keeps private | a file others may read (`o+r`) and a folder others may list and search (`o+rx`), under folders that are so too; the rest is `not_read: "unreadable"` |
| `system` | — | `/proc`, `/sys`, `/dev`, `/run`: listed, never `stat`ed or entered |
| `restricted` | `store.sqlite*`; a run folder's `config/` and `overlay/`; `control.json`, `worker.json`; `lock`, `*.lock`; a Store's `open/`; `/layerfs/state/secret` | `store.sqlite*` |
| Workspace mounts | an entry of a run folder's `workspaces/` is `not_read: "mount"` with `workspace` (its handle) | — |

`worker.log`, `worker.1.log` and a run folder's `daemon.log` are listed as
files with `role: "log"` and read with `machine log` and `sandbox log`.
Refusals are `sandbox files`'s.

```console
$ layerfs machine files native
{"items":[{"path":"/tmp/demo/home/store","type":"dir"},{"path":"/tmp/demo/home/sandbox","type":"dir"},{"path":"/tmp/demo/home/machine/native/run","type":"dir"}]}
```

### machine.log

A machine's worker log: its last lines, then the lines before a cursor.

```text
CLI     layerfs machine log <machine> [--limit 200] [--cursor C]
Node    await layerfs.machine.log(machine, { limit, cursor })
Python  layerfs.machine.log(machine, *, limit=None, cursor=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `machine` | name | a machine name |
| `limit` option | int | at most this many lines; default `200` |
| `cursor` option | text | a page's next_cursor: the lines before it |

| Answer | Keys |
| --- | --- |
| short | `items`; when set: `next_cursor` |
| long | `items, next_cursor` |
| `items[]` short | `text`; when set: `continued` |
| `items[]` long | `text, continued` |
| call | pages by `limit` and `cursor`; writes no call-log line; MCP `admin` |

One page of a machine's worker log: `log/worker.log` under the worker's
root, with `worker.1.log`, the one file it rotates to when the worker starts
([Workers and reach](../../control/machines/workers-and-reach.md#the-workers-log)).
`items` are the last `limit` lines (200 by default), oldest first, or with
`cursor` the lines before it; `next_cursor` continues with older lines and
is left out at the start of the log. One call reads at most 64 KiB, so a
page may hold fewer than `limit` lines. A line longer than one read is
answered in pieces, each but the last with `continued: true`; a last line
the writer has not ended is held back until it is. `text` is for display:
control characters are escaped (`\t`, `\r`, `\u{1b}`), bytes that are not
UTF-8 are U+FFFD. It reads these two files and no other
([`inspect/log.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/log.rs)).
It takes no lock or lease, starts nothing and writes nothing.

| Refusal | When |
| --- | --- |
| `Unsupported` | the machine's calls run in the caller's process (`native`), so it has no worker |
| `InvalidInput` | `limit` out of range; a cursor that is not one, or is stale (the log rotated) |
| `NotFound` | no machine has this name |
| `Unavailable` | the machine is stopped, its worker is not reached, or it does not answer in time |
| `Busy` | two inspection requests already run on the machine |

It was `logs.machine`, removed with no alias (its `--before` is `--cursor`).

```console
$ layerfs machine log native --limit 1
{"error":"Unsupported","message":"machine native has no worker (its calls run in the caller's process), so it has no worker log"}
```

### machine.start

Start a stopped machine and its worker; a running one answers at once.

```text
CLI     layerfs machine start <machine> [--progress]
Node    await layerfs.machine.start(machine, { on_progress })
Python  layerfs.machine.start(machine, *, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `machine` | name | a machine name |
| `on_progress` option | function |  |

| Answer | Keys |
| --- | --- |
| short | each when set: `ok`, `unchanged` |
| long | `ok, unchanged, outcome, machine` |
| call | sends progress; writes one call-log line; MCP `admin` |

Starts a stopped machine and its worker, waits until the worker answers its
secret, and records it `ready`; a running machine answers
`{"unchanged":true}` at once. It is the only call that starts a machine. It
records its `op` before its first effect and runs under
`machine/<name>/start.lock`, shared with `create`, `stop` and `remove`
([`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs));
the home lock is held only to write records, never while a VM boots.
`docker` runs the privileged worker container from the recorded image, else
the bundle's image loaded once; `lima` and `apple-container` boot the VM.
Guest installation copies only changed or absent bundle files and publishes
the manifest last ([`power.rs`](../../../../control/crates/layerfs-machine/src/power.rs)).
Progress phases: `bundle`, `image` (the docker machine loading the bundle's
image), `boot`, `worker`.

| Refusal | When | Next |
| --- | --- | --- |
| `Unavailable` | the worker is of another build: the machine runs and is recorded `broken` `outdated` ([`worker/hello.rs`](../../../../control/crates/layerfs-machine/src/worker/hello.rs)) | `fix: layerfs machine stop <machine> && layerfs machine start <machine>`, `--force` on the stop when Sandboxes are recorded on it |
| `Busy` | another operation on the machine runs; it names the `op` | |
| `Unsupported` | no sealed Linux bundle (`detail.missing`); `docker` with no image ([`bundle.rs`](../../../../control/crates/layerfs-machine/src/bundle.rs)) | |
| `Unknown` | a provider command timed out or its reply was lost: the machine is `broken` `unresolved` | `fix: layerfs machine get <machine> --with live`, which settles it by observing |

```console
$ layerfs machine start native
{"unchanged":true}
```

### machine.stop

Stop a machine's worker and VM; its disks stay.

```text
CLI     layerfs machine stop <machine> [--force] [--progress]
Node    await layerfs.machine.stop(machine, { force, on_progress })
Python  layerfs.machine.stop(machine, *, force=None, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `machine` | name | a machine name |
| `force` option | bool | stop it and this home's Sandboxes on it without asking its worker; data stays |
| `on_progress` option | function |  |

| Answer | Keys |
| --- | --- |
| short | each when set: `ok`, `unchanged`, `stopped_sandboxes`, `stopped_sandboxes_count`, `unknown_workspaces`, `unknown_workspaces_count` |
| long | `ok, unchanged, stopped_sandboxes, stopped_sandboxes_count, unknown_workspaces, unknown_workspaces_count, outcome, machine, forced` |
| call | sends progress; writes one call-log line; MCP `admin` |

Stops a machine's worker and, where it has one, its VM; disks and Stores
stay. `native` stops nothing; `docker` ends its worker container only;
`lima` and `apple-container` end the worker and VM. It runs under
`start.lock` and removes `worker.json`: a later call that needs the machine
is `Unavailable` until `machine start`. A stopped machine answers
`{"unchanged":true}`. Progress phases: `check`, `stop`.

Without `--force` it is refused `InUse` while Sandboxes are recorded on the
machine (`detail.related` lists them): the message names them and `--force`
as the other way, the next field `sandboxes` lists them, and the `fix`
removes each, then stops the machine
(`layerfs sandbox remove a && layerfs sandbox remove b && layerfs machine stop <m>`;
`sandbox remove` refuses while a Workspace is mounted or a command runs, so
the chain loses nothing). `--force` is not offered as the fix: its
Workspaces become `unknown_workspaces` ([`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) `can_stop_machine`). `--force` stops the machine anyway:
it goes straight to the provider, does not call the worker, commit or
unmount Workspaces one by one, and deletes no record; it answers
`stopped_sandboxes` and `unknown_workspaces` (Workspaces whose state could
not be confirmed; these are not saved Commits), each the first 20 with its
count past those. On `native` it ends only this home's recorded Sandbox
runtimes and commands, without opening the Store or calling the daemon, and
the machine stays `ready`. Stopped Sandboxes are recorded `broken`
`missing`, or `unresolved` when the stop could not be confirmed. A provider
failure or uncertain outcome is reported, never replayed or turned into
success ([`machine_force.rs`](../../../../control/crates/layerfs-control/src/machine_force.rs)).
A lost reply leaves the machine `broken` `unresolved` with `fix: layerfs
machine get <machine> --with live`.

```console
$ layerfs machine stop native
{"unchanged":true}
```

### machine.create

Create a VM machine (lima, apple-container) and start it.

```text
CLI     layerfs machine create <name> <platform> [--cpus 2] [--memory 4G] [--disk 20G] [--image I] [--os-image I] [--progress]
Node    await layerfs.machine.create(name, platform, { cpus, memory, disk, image, os_image, on_progress })
Python  layerfs.machine.create(name, platform, *, cpus=None, memory=None, disk=None, image=None, os_image=None, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `name` | name | the new machine's name |
| `platform` | name | `lima` or `apple-container` |
| `cpus` option | int | its processors; default `2` |
| `memory` option | size | its memory; default `4G` |
| `disk` option | size | its data disk; default `20G` |
| `image` option | text | the Sandbox image its Sandboxes run: a digest or a file |
| `os_image` option | text | the VM's OS image or prepared machine image; stock for the provider's own |
| `on_progress` option | function |  |

| Answer | Keys |
| --- | --- |
| short | `ok` |
| long | `ok, outcome, machine, platform, backend, cpus, memory_bytes, disk_bytes, image, os_image, created_at` |
| call | sends progress; writes one call-log line; MCP `admin` |

Makes a VM machine on this host, copies the sealed Linux bundle in and
starts its worker ([`create.rs`](../../../../control/crates/layerfs-machine/src/create.rs)).
`--os-image` is the VM's operating-system image (an installed OCI reference
for Apple Container, an image location for Lima); absent, an installed
prepared image, else the provider's stock image; `stock` asks for the
provider's stock image even when a prepared one exists. The record keeps
`vm_name` (`layerfs-<home id prefix>-<name>`, so two homes never share a
VM). A name longer than 46 characters, or whose VM name is longer than the
platform's provider takes on this host, is `InvalidInput` (the message
states the longest name that fits); a name whose VM name is another
machine's is `Exists`. The name is claimed with an `op` `create` under the
home lock; the boot runs under `start.lock` only. Progress phases:
`bundle`, `boot`, `worker`.

| Adapter outcome | Record afterwards |
| --- | --- |
| ready | `op` dropped, `worker.json` written |
| failed before the VM existed | the machine folder is removed |
| failed after the VM existed | `broken` `failed`; the VM is kept for inspection |
| `Unknown` | everything kept; `op` uncertain; logged to `log/uncertain.jsonl`; `machine remove` clears it |

Refusals: `Exists`; `InvalidInput` (`native` or `docker` as the name, a
name too long); `Unsupported` (a removed or unsupported platform, a missing
tool in `detail.missing`, no sealed bundle).

### machine.remove

Delete a VM machine and its disk.

```text
CLI     layerfs machine remove <machine> [--progress]
Node    await layerfs.machine.remove(machine, { on_progress })
Python  layerfs.machine.remove(machine, *, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `machine` | name | a machine name |
| `on_progress` option | function |  |

| Answer | Keys |
| --- | --- |
| short | `ok` |
| long | `ok, outcome, machine, message` |
| call | sends progress; writes one call-log line; MCP `admin` |

Deletes a VM machine with its data disk; also clears an unresolved create
or remove. Built-in machines are never removed. A running machine is removed
as it is. Its own PostgreSQL addresses in Store records are forgotten and the
home's image copy locks of it (`run/image-lock/<machine>/`) are deleted.
The long form's `outcome` is `removed`. Of a machine of a removed platform
(`firecracker`, `microsandbox`, `ssh`) only its record is removed: `{"ok":true}`,
the long form's `message` "only its record was removed (<reason>); nothing
on its host was touched"
([`remove.rs`](../../../../control/crates/layerfs-machine/src/remove.rs)).
Progress phases: `check`, `delete`.

| Refusal | When |
| --- | --- |
| `Busy` | a live process's create or remove holds it |
| `InUse` | sqlite Stores, Sandboxes or old Firecracker records are on it (`detail.related`); the next fields `stores`, `sandboxes`, `machines` name each kind. Held by Sandboxes alone, the `fix` removes them, then the machine (`layerfs sandbox remove a && layerfs machine remove <m>`); a machine its record says is stopped gets no fix, since `sandbox remove` cannot reach a daemon on it, and the message says so ([`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) `can_remove_machine`) |
| `Unavailable`, `Failed` | the provider refused or failed; the record is put back |
| `Unknown` | the provider timed out: `op` kept, uncertain; read `machine get --with live`, then remove again |

### machine.update

Set a machine's image, or a built-in machine's Docker socket, WSL distribution and daemon.

```text
CLI     layerfs machine update <machine> [--cpus N] [--memory M] [--image I] [--docker S] [--wsl D] [--daemon P]
Node    await layerfs.machine.update(machine, { cpus, memory, image, docker, wsl, daemon })
Python  layerfs.machine.update(machine, *, cpus=None, memory=None, image=None, docker=None, wsl=None, daemon=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `machine` | name | a machine name |
| `cpus` option | int | a VM's processors; refused Unsupported in 2.0 |
| `memory` option | size | a VM's memory; refused Unsupported in 2.0 |
| `image` option | text | the Sandbox image its Sandboxes run |
| `docker` option | text | the docker machine's Engine socket; "" chooses it again at the next start |
| `wsl` option | text | a Windows host's WSL distribution |
| `daemon` option | text | the absolute path of the daemon a built-in machine's Sandboxes run |

| Answer | Keys |
| --- | --- |
| short | `ok` |
| long | `ok, outcome, machine, platform, cpus, memory_bytes, disk_bytes, image, docker, wsl, daemon` |
| call | writes one call-log line; MCP `admin` |

Changes a machine's recorded settings; a new setting takes effect at the
next `machine start` or `sandbox create`
([`update.rs`](../../../../control/crates/layerfs-machine/src/update.rs)).
Refused `InUse` while Sandboxes run on the machine: the next field
`sandboxes` names them and the `fix` removes each
(`layerfs sandbox remove a && layerfs sandbox remove b`;
[`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) `can_change_machine`). There is no `--disk`:
resizing is not in 2.0.

| Option | Applies to | Rule |
| --- | --- | --- |
| `image` | any machine | on `docker`, the worker container's image; an absolute path is the Sandbox image archive |
| `docker` | built-in only | the Docker Engine socket the docker machine and every Docker read of the home use: an absolute path (`unix://` and one is taken); `""` clears it. `InUse` while the machine's worker is recorded running |
| `wsl` | built-in only, Windows hosts | the WSL distribution |
| `daemon` | built-in only | the absolute path of an existing file: the daemon its Sandboxes run |
| `cpus`, `memory` | VM machines | `InvalidInput` on a built-in; on a VM `Unsupported` ("not built yet"), nothing changed |

```console
$ layerfs machine update native --daemon /opt/layerfs/layerfs-daemon
{"ok":true}
```

### machine.doctor

Check a machine, then each Store and Sandbox on it.

```text
CLI     layerfs machine doctor <machine> [--progress]
Node    await layerfs.machine.doctor(machine, { on_progress })
Python  layerfs.machine.doctor(machine, *, on_progress=None)
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `machine` | name | a machine name |
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

Checks a machine (`machine.record`, `machine.operation`,
`machine.configuration`, `machine.provider`, `machine.state`, then its
worker: `machine.worker.endpoint`, `machine.worker.auth`,
`machine.worker.compatibility`, `machine.worker.ping` and the rest of its
kind's list), then gives each sqlite Store and each Sandbox on it a shallow
check; a check that does not apply is left out. A stopped machine gets no
worker checks, and under a stopped machine, or one whose provider or worker
failed its check, nothing on it is tried (`not_checked` with `blocked_by`).
`machine.capacity`'s evidence adds `reclaimable_image_bytes` (what `machine
prune --unused` frees); that prune is its fix only when there are such
bytes. It changes nothing and exits 0 once its report is made. It replaces
the shell's `layerfs doctor machine`, removed with no alias.

```console
$ layerfs machine doctor native
{"ok":true}
```

## Family errors

| Error | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad name or option; a folder scope | [`calls/check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs) |
| `NotFound` | no machine of that name | [`record.rs`](../../../../control/crates/layerfs-machine/src/record.rs) |
| `Busy` | a `create`, `start`, `stop` or `remove` of the machine runs; it names the `op` | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs) |
| `Unsupported` | a removed platform; a build without the `machine` feature; a platform this host does not support | [`calls/platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs) |
| `Unavailable` | the machine is stopped or its worker does not answer; nothing starts it | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs) |
| `Unknown` | a provider command or a worker's answer was lost; the `op` stays uncertain | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |

## Related

- [Machines](../../control/machines/README.md), [platforms](../../control/machines/platforms.md), [workers and reach](../../control/machines/workers-and-reach.md).
- [sandbox family](sandbox.md), [store family](store.md), [home family](home.md) (`home get --with platforms`).
- [Doctors](../../control/doctor/README.md).
- [Errors and events](../calls/errors-and-events.md).
