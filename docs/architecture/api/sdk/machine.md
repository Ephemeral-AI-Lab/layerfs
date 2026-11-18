# machine family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `machine` calls: list, inspect, create, start, stop, update and remove the machines that hold sqlite Stores and run Sandboxes, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/machine`](../../../../api/crates/layerfs-api/src/machine), [`layerfs-control/src/machine.rs`](../../../../control/crates/layerfs-control/src/machine.rs), [`layerfs-machine/src`](../../../../control/crates/layerfs-machine/src), [`layerfs-platform/src`](../../../../control/crates/layerfs-platform/src) |
| Runs by default | every call in the caller's process, through the platform adapter (provider CLI or Docker); none is sent to a worker but the three reads `stats`, `space` and `files`, each one inspect request served where the machine's data is. The guest worker is built without the `machine` feature, so there every machine call is `Unsupported` before any effect |
| Design | [Machines](../../control/machines/README.md), [platforms](../../control/machines/platforms.md), [workers and reach](../../control/machines/workers-and-reach.md) |

## Calls

```text
 reads (start nothing):  list   platforms   get   status   stats   space   files   images
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
| [`machine.load`](#machineload) | `{ id, refs, size, reused, dropped?, sparse? }` |
| [`machine.prune`](#machineprune) | `{ removed: [id], freed }` |
| [`machine.status`](#machinestatus) | `{ name, platform, state, free_disk, tool_version, down_reason, stores, op? }` |
| [`machine.stats`](#machinestats) | `{ machine, platform, observed_at, worker, resources, data_disk, stores, sandboxes, complete, reason? }` |
| [`machine.space`](#machinespace) | `{ machine, platform, observed_at, disk, layerfs, outside, parts, stores, sandboxes, unreadable, complete, reason? }` |
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
  state, cpus, memory, disk, stores: [name],
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
| `stores`, `sandboxes` | sqlite Stores on it, Sandboxes recorded on it (a PostgreSQL Store is on no machine) ([`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs)) |
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

The support table for this host, with a live tool probe per platform. It
describes the host and this build, so it needs no home and never creates or
converts one (`Call.opens` false); with a home it reads the docker machine's
recorded socket.

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
                 options: [ { name, kind, default } ], description,
                 // the docker row only:
                 socket: { path, source, state },
                 candidates: [ { path, host, source, usable } ] } ] }
```

| Field | Meaning | Source |
| --- | --- | --- |
| `how` | `built_in` (`native`, `docker`) or `created` | [`platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs) |
| `backend`, `kernel` | `native` or `namespaces`; `fuse` or `nfs`; null when not on this host | [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs) |
| `sqlite` | where a sqlite Store's calls run: `in_process` or `worker` | same |
| `postgresql` | `store_calls` (always `in_process`), `sandbox` (bool), `reason` | same |
| `ready`, `missing`, `version` | the live probe: `tool:<cli>` not on the path or in the fallback folders, `host:<fact>` lacking (docker: `host:<socket>` when its socket does not answer a ping; `docker version` runs only when it does); each command bounded at 10 s. A platform that runs a worker (`sqlite: "worker"`: docker, lima, apple-container where they run) also needs `bundle:<dir>` when this build's sealed Linux bundle is absent or of another worker protocol, the folder being the one it would use (`LAYERFS_BUNDLE`, else the cache). A bundle from another commit is no need: `home.get` warns `BundleMismatch` | [`probe.rs`](../../../../control/crates/layerfs-platform/src/probe.rs), [`platforms.rs`](../../../../control/crates/layerfs-machine/src/platforms.rs) |
| `options` | what `create` takes for this platform; `default` is what it takes when the option is absent: `cpus` 2, `memory` 4 GiB and `disk` 20 GiB (bytes), null for the rest | [`options.rs`](../../../../control/crates/layerfs-platform/src/options.rs), `DEFAULT_*` in [`spec.rs`](../../../../control/crates/layerfs-machine/src/spec.rs) |
| `socket` | the docker machine's Engine socket: `path`; `source`, where it comes from (`recorded`, `legacy` for a machine that ran before sockets were recorded, else the automatic choice, named as `candidates` name the same socket: `env:DOCKER_HOST`, `context:<name>`, `desktop`, `colima`, `orbstack` or `default`); `state`, a ping (`GET /_ping`, 2 s per read and write; the probe's, when it found the Engine's version): `answering`, `absent` (no such file), `denied` (`EACCES`: the docker group fix, not Start) or `silent` (there, no answer) | [`docker_socket.rs`](../../../../control/crates/layerfs-platform/src/docker_socket.rs), `docker_socket` in [`spec.rs`](../../../../control/crates/layerfs-machine/src/spec.rs) |
| `candidates` | every endpoint the host names, most specific first, each socket once: `DOCKER_HOST`, the current Docker context (`DOCKER_CONTEXT`, else `currentContext` in `config.json` under `DOCKER_CONFIG` or `~/.docker`), the other contexts by name (`contexts/meta/*/meta.json`), Docker Desktop's, colima's and OrbStack's sockets that exist, and `/var/run/docker.sock`. `host` is the endpoint as Docker writes it; one that is not `unix://` has `path: null`, `usable: false` and is never followed. Read from files only | same |

The rows without the live fields are
[`control/SUPPORT.json`](../../../../control/SUPPORT.json), generated from
`support.rs`; the per-host matrix is on [platforms](../../control/machines/platforms.md).
A missing tool is `ready: false`, not an error. How tools and the docker
socket are found: [platforms](../../control/machines/platforms.md#tools-and-the-docker-socket).

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
  images: [{ id, refs, size, architecture, source, last_used, state, op, reason, fix, message, staged_bytes, sandboxes }],
  docker: null | { available: true }
        | { available: false, reason: "not-installed" | "not-running" | "error", message },
  available: [{ id, refs, size, os, architecture, usable, reason?, need, fits }] }
```

`images` has one item per image (spec v19): published unpacked images with
their sidecars (`machines/native/run/images/<hex>.2.json`: `source:
"imported"` for one copied from Docker), what the archives its Sandboxes read
still hold (`bundle`, `machine`), and what copies left. The machine's own
listing still reports a staged part (`.recv-<hex>.part`) and an unpack left
without a sidecar as raw rows; the host merges them into the image's item
and gives it a state: `ready` when published (a stale part beside it is its
`staged_bytes`); `busy` with `op {name: load_image, phase, started_at}` while
the `ImageLock` owner record of that image and machine names a live process
(its recorded `phase`, else `lock`; the record is read, the `flock` is never
tested); otherwise `broken` `partial` with `fix: layerfs machine load-image
<m> sha256:<hex>`, a `message`, `size` null and `staged_bytes` the part's
bytes (null for an unpack). `staged`, `stranded`, `resumable`, `age`,
`variant` and the image's `Env` stay out of the answer; `variant` and
`config` (`Env`, `WorkingDir`, `User`, `Entrypoint`, `Cmd`) stay
recorded-only in the sidecar and are not applied. `id` is the Docker image Id for an
imported image and the config digest for an archive's. `sandboxes` counts
the Sandbox records on this machine naming that `id`; `last_used` is the
last create's time (RFC 3339) or null. `default` is the record's image, else
the archive's only one. `free` is the image cache's free bytes. `available`
lists the host's tagged Docker images (one `docker image ls`, one batched
`docker image inspect`, 10 s each) through the `docker` machine's socket,
else Docker's default context, minus the id or refs of every `ready` item
(the bundle's image in Docker, `layerfs-cache/sandbox:<hex>`, included).
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
a byte sent. Otherwise `docker image inspect` resolves it; one copy of that Id
on that machine at a time holds `<home>/run/image-lock/<machine>/<hex>.lock`
(an exclusive `flock` with the owner's `pid`, `pid_start` and `pid_boot`
beside it, and the phase the copy is in, `export`, `send` or `unpack`,
written as it moves), so a second copy waits — a plain `{phase: "lock"}`
event each second, with no deadline; the copy it waits for is the `busy`
item of `machine.images` — and then reuses what the first
published; a copy whose owner died takes the lock over at once, because the
kernel released it, and a record naming a reused pid holds nothing. Holding
the lock, a copy passes the machine's prune gate (an exclusive `flock` on
`<home>/run/image-lock/<machine>/` that `machine.prune` holds while it runs)
before it creates or writes anything, so it waits for a prune under way. The
machine's free bytes are checked before anything is created on either side:
`Capacity` `{needed_bytes, free_bytes, path}` with `fix: layerfs machine
prune <m> --unused` refuses a machine too small for the copy,
with no export container, no staged byte and no cache folder, `needed_bytes`
counting the bytes a resumed copy already staged once and `path` the
filesystem `probe` measured. Only then does the host create an exclusive export
container `layerfs-export-<home8>-<machine>-<nonce>` (labeled with
`layerfs.role=image-export`, `layerfs.home`, `layerfs.machine`, `layerfs.image`
and `layerfs.export.nonce`, removed with its anonymous volumes on exit or
error; one a `SIGKILL`ed copy left is swept by the next copy of the same
image to the same machine, under its `ImageLock`; any other, and the legacy
`layerfs-export-<machine>-<hex>` containers, are `home.leftovers` entries of
kind `export_container` that `home.prune` removes with `docker rm -v`) and stream `docker export` without the init layer's
files (`.dockerenv`, `etc/hosts`, `etc/hostname`, `etc/resolv.conf`, `etc/mtab`,
`dev/*`), device or FIFO entries, refuses hardlinks whose targets were dropped,
expands GNU oldgnu (`S`) and PAX (`0.0`, `0.1`, `1.0`) sparse entries while
streaming (enforcing `image_tar::SPARSE_MAX` on per-entry `realsize` and
cumulative expanded bytes, bounding oldgnu/PAX `0.x` chunk counts by entry
`size`, requiring `payload_needed == size` after padding, refusing malformed PAX
records after `GNU.sparse.*` keys with `InvalidData`, counting expanded bytes
written to `.part` against `Capacity`, and answering the dropped paths and
expanded sparse files as `dropped` and `sparse`), and normalizes the
modification times of the directories the init layer stamps (the root, `dev/`,
`etc/`, `proc/`, `sys/`) to epoch 0 (`1970-01-01T00:00:00Z`, PAX time records
stripped and header checksum recomputed, so exports separated in time are
byte-identical for resume; every other directory keeps the image's time) in
1 MiB `image.artifact` frames to `.recv-<hex>.part`;
the machine checks its size and sha256, unpacks it
once into `<hex>.2/`, strips the newly claimed `refs` from other cached
sidecars (skipping corrupt unrelated sidecars) so moved tags cannot select stale
images, writes the sidecar (keeping
`variant` and `config` recorded-only), and deletes the staged tar.
`<hex>.2.json` is the publication commit point (Option B interrupted-publish
recovery): a read-only `probe` treats an un-sidecared `<hex>.2/` outside a
sealed archive as `present: false, stranded: true` instead of backfilling
incomplete `refs`/`config`/`variant` without a lock, while `publish` removes
any non-in-use stranded `<hex>.2/` under `ImageLock` before re-unpacking, and
`probe` refuses ambiguous `ref` lookups matching multiple cached images
(`InvalidInput`, suggesting `machine.prune` to remove one).

Known limitations of `load` ref maintenance:
- `strip_moved_refs` reads and rewrites other sidecars without a cross-file lock; in a narrow concurrent window between `publish` stripping a moved ref and a simultaneous `touch()` (Sandbox creation) on the older image, the stripped ref can reappear on the older sidecar and cause an ambiguous-ref refusal until one image is removed with `machine.prune`.
- When the target digest is already cached on the machine, `load` returns `reused: true` early without calling `publish`, so moving a tag in Docker onto an already-cached target does not strip that tag from an older cached sidecar; if two sidecars claim the same ref, use `machine.prune` on one of them.

A staged
tar left by a copy that is not running is resumed when its bytes hash as this
export's prefix (hashed once, on the probe under the lock), else restarted;
`size` is the bytes sent, or for a reused image the sidecar's, and the export container's `SizeRootFs` only
refines the progress estimate. A copy that sent the image also answers
`dropped: {fifos, devices, init}` (the paths left out) and `sparse: {files:
[{path, expanded_bytes, stored_bytes}], expanded_bytes}`; a reused one has
neither. Progress: `inspect`, `lock`, `export`, `send`
(`done`, `total`: an estimate, never below `done`, until the last `send`
event, whose `done` and `total` are both the bytes sent), `unpack`. Errors:
`Unsupported` `{reason: "host"}`
(macOS `native`), `{architecture}` or `{os}`; `InvalidInput` (Docker
has no such image, or a reference matching several cached images, detail
`{images}`); `Unavailable` `{what: "docker" | "machine"}`; `Capacity`
`{needed_bytes, free_bytes, path}`; `Failed` (frames out of order, or
published bytes whose size or sha256 do not match). A `sandbox.create` that
copied holds the `ImageLock` until the Sandbox is made.

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
`lower=` names it or a create holds `.2.lock`: a named one is refused `InUse`
`{sandbox_count}`, and `unused` keeps the image but removes a part no copy
holds beside it. While a live process holds `ImageLock`
(`<home>/run/image-lock/<machine>/<hex>.lock`) a named one is refused `Busy`
with that copy's `op` (from its owner record), and `unused` leaves its
artifacts untouched. `prune` holds the machine's prune gate (an exclusive `flock` on the
folder `<home>/run/image-lock/<machine>/`, creating no file, so disk-full
cleanups still proceed) from before it reads the `ImageLock`s until its
removals are done, on every route: a `load` passes the gate after taking its
`ImageLock` and before it writes, so it either holds its lock when `prune`
reads them or waits until `prune` is over. If probing a single hex's lock fails
(a bad `.lock` file), `unused` skips that hex, continues cleaning the rest, and
returns `skipped: [{ id, reason }]` when any were skipped (naming that image
directly returns the error; the call surface answers them as `skipped
[{image, message}]`, first 20). Otherwise `prune` removes unpacked directories (including stranded
`<hex>.2` without `<hex>.2.json`), dangling `<hex>.2.json` sidecars, unheld
`.recv-<hex>.part` staged files, and temporary `.tmp-<hex>.2-*` unpack
directories (`unused` keeps valid unpacked copies of the machine's default).
Naming the default removes it; the next create unpacks it again from
its archive. `freed` is the free bytes after minus before. Errors:
`InvalidInput` (neither an image nor `unused`, or not an id), `NotFound`
(no matching artifact here), `InUse`, `Unsupported` `{reason: "host"}`.

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
sqlite Store's size, how many of its Sandboxes have a running daemon and what
each of them uses (layerfs-project#164, #182). It answers the present only:
processor values are counters, and a client keeps its own history and turns
two reads into a rate. A monitoring read for a UI that polls: it is in the
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
               disk_total, disk_used, disk_free,
               cpu_busy_usec, cpu_total_usec, scope, reason? },
  data_disk: { path, fs, scope, volume, inodes_free },
  stores: [ { name, bytes, allocated } ],
  sandboxes: { recorded, live,
               items: [ { name, live, live_reason?, started_at,
                          usage, usage_reason? } ] },
  complete, reason? }
```

| Field | Meaning | Source |
| --- | --- | --- |
| `observed_at` | ms since the epoch on the side that answered | [`inspect/stats.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/stats.rs) `serve` |
| `worker` | the machine's worker process: its build's `version`, `pid`, `started_at` (ms since the epoch, when it began to serve), worker `protocol` and the `bundle` it was installed from. `null` on a machine whose calls run in the caller's process (`native`) | [`worker_server/ops.rs`](../../../../control/crates/layerfs-control/src/worker_server/ops.rs) `about` |
| `resources.cpus` | the processors the answering process may run on: where the worker runs in a container with a processor quota (`apple-container`), the quota, which can be less than the processors the guest kernel lists | [`inspect/host.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/host.rs) |
| `resources.memory_total`, `memory_used` | bytes. Linux: `MemTotal` and `MemTotal - MemAvailable` of `/proc/meminfo`. macOS: `hw.memsize`, and that less the free, file-backed and purgeable pages, from one run of `/usr/sbin/sysctl` | `host.rs` |
| `resources.disk_total`, `disk_used`, `disk_free` | one `statvfs` of the filesystem that holds the machine's data. `disk_used` is the total less every free block; `disk_free` is what the answering process may use, so the two can sum to less than the total where blocks are reserved. On a machine whose data is on its own volume or disk (`lima`, `apple-container`) these are that volume's sizes, not the whole VM's | [`home/capacity.rs`](../../../../control/crates/layerfs-home/src/home/capacity.rs) `filesystem` |
| `resources.cpu_busy_usec`, `cpu_total_usec` | the machine's processor time since boot, in microseconds, from the first `cpu` line of `/proc/stat` (one read, which also gives the boot time every row's `started_at` uses): busy is user + nice + system + irq + softirq + steal, total is busy + idle + iowait, each in ticks of 1/100 s × 10,000. Guest time is inside user and nice already. Steal counts as busy: on a VM it is time the host took from it. The machine's processor share between two reads is Δbusy / Δtotal, which needs neither `cpus` nor a clock; it is of the kernel `scope` names, so on `docker` the whole Engine VM even where `cpus` is a container's quota, and on `apple-container` the whole guest, whose kernel can list more processors than the worker's quota `cpus` (seen: 3 online, quota 2): a guest busy on its whole quota then reads about 2/3. `null` on macOS (`cpu_unsupported`: its only source, `host_statistics`, needs `unsafe` code this crate forbids) and when the read failed (`cpu_not_read`) | `host.rs` `cpu`, [`inspect/process.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/process.rs) `KernelStat` |
| `resources.scope` | whose processors and memory they are: `host` (`native`), `vm` (`lima`, `apple-container`), `engine_host` (`docker`: the kernel the Engine runs on, a VM on macOS and Windows) | `stats.rs` `scopes` |
| `resources.reason` | which values are null and why, joined by `, `; absent when all were read. Each word names its fields: `memory_not_read` → `memory_*`; `disk_not_read` → `disk_*`; `cpu_unsupported`, `cpu_not_read` → `cpu_*`. A client shows a word it does not know as it is | `stats.rs` |
| `data_disk.path`, `fs` | the folder whose filesystem was measured, as the answering side names it (the home, or the worker's root), and its type (`apfs`, `ext4`, `xfs`, `btrfs`, `overlay`, ..., else the magic number in hex) | `capacity.rs` |
| `data_disk.scope` | what that filesystem is: `host_fs` (`native`; `docker` on a Linux host, where a volume is a folder of the host's filesystem), `vm_disk` (`docker` on macOS and Windows: the Engine VM's one disk, shared with every image and volume), `volume` (`lima`, `apple-container`: the machine's own disk) | `stats.rs` `scopes` |
| `data_disk.volume` | the provider's name of the volume or disk, from the machine's record; `null` on `native` | [`adapter.rs`](../../../../control/crates/layerfs-platform/src/adapter.rs) `data_volume` |
| `data_disk.inodes_free` | file slots free; `null` where the filesystem counts none | `capacity.rs` |
| `stores[]` | the sqlite Stores the home records on the machine: `bytes` is the length of `store.sqlite` plus its `-wal`, `allocated` their blocks, each from one `lstat`; no Store file is opened. A sparse file shows in the difference; a clone counts in full. Both `null` when the file is not there, or when the deadline came first | `stats.rs` `store_bytes` |
| `sandboxes.recorded`, `live` | the Sandboxes the home records on the machine, and how many of them have a daemon process that still runs (the check of `sandbox.list`'s `live`); `live` is `null` when the deadline came first | [`inspect/live.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/live.rs) |
| `sandboxes.items[]` | one row per recorded Sandbox, in the home's order, read by the side that runs it from its own daemon record, `/proc` and the Sandbox's cgroup (a forwarded Lima Sandbox by the machine's worker; no daemon is dialed): `live` and `live_reason` as `sandbox.list`'s `live`; `started_at`, ms since the epoch when its daemon record's process started (`null` when it is gone); `usage` and `usage_reason` exactly as [`sandbox.get`](sandbox.md#sandboxget)'s. With no daemon record read, `started_at` and `usage` are `null` and `usage_reason` is `not_observed`, or the row's own reason (`record_unreadable`). A row whose daemon process is gone (`live: false`) still reads its cgroup while the group is there (something it started may still run in it), with no `usage_reason`; `sandbox.get` of the same Sandbox, which reads beside its daemon's Workspace list, answers `usage: null` with `not_observed` when that list was not answered. A row the soft budget did not reach stays, `live: null, live_reason: "deadline", usage: null, usage_reason: "deadline"`: an absent row would read as a closed Sandbox. A worker of an older build answers no `items`, and the field is absent | `stats.rs` `item`, `live.rs` `checked` |
| `complete`, `reason` | `false` with `reason: "deadline"` when the request's soft budget (4 s) ended the Store or Sandbox part early; what was read is answered | `stats.rs` |

It is one [inspect](../../control/sandboxes/inspect.md) request, kind
`machine_stats`: in this process on `native`, else one request to the
machine's worker. It takes no lock or lease, opens no Store, asks no daemon
and no provider tool, starts nothing and writes nothing. PostgreSQL Stores
are not listed: their data is not on a machine.

```text
$ layerfs machine stats native
{"machine":"native","platform":"native","observed_at":1794776707857,"worker":null,"resources":{"cpus":14,"memory_total":38654705664,"memory_used":31340068864,"disk_total":994662584320,"disk_used":641705943040,"disk_free":352956641280,"cpu_busy_usec":null,"cpu_total_usec":null,"scope":"host","reason":"cpu_unsupported"},"data_disk":{"path":"/Users/me/.layerfs","fs":"apfs","scope":"host_fs","volume":null,"inodes_free":3446842200},"stores":[{"name":"shop","bytes":217088,"allocated":217088}],"sandboxes":{"recorded":1,"live":1,"items":[{"name":"web","live":true,"started_at":1794776100211,"usage":null,"usage_reason":"host_process"}]},"complete":true}
```

Errors: `NotFound` (no such machine); `Unavailable` (the machine is stopped or its worker
did not answer within 8 s; nothing starts it); `Busy` (two inspect requests
already run in that process); `InvalidInput` (a worker of a build without
the kind, or over 4096 Stores or Sandboxes on one machine).

### machine.space

Where a machine's data disk goes: how much of it each part of LayerFS's
own data takes, and how much of what the filesystem uses is not
LayerFS's. One bounded walk of LayerFS's folders, read where the data is,
for a UI that warns before a disk fills.

```text
JS      await layerfs.machine.space(machine)
Python  layerfs.machine.space(machine)
CLI     layerfs machine space <machine>
```

```text
{ machine, platform, observed_at,
  disk: { total, used, free, path, fs, scope },
  layerfs, outside,
  parts: { stores, sandboxes, images, other_runs, logs, other },
  stores: [ { name, bytes } ],
  sandboxes: [ { name, bytes, overlays, writes, home, log, runtime, reason? } ],
  unreadable, complete, reason? }
```

Every size is the blocks a part takes (`st_blocks` × 512), so a sparse file
counts what it holds and a file with several names counts once; a clone
counts in full, since `stat` cannot see sharing.

| Field | Meaning | Source |
| --- | --- | --- |
| `disk` | the filesystem [`machine.stats`](#machinestats) measures (`total`, `used`, `free` from one `statvfs`, the folder measured, its type) and its `scope` (`host_fs`, `vm_disk`, `volume`, as `data_disk.scope`); `null` values with `disk_not_read` | [`inspect/space.rs`](../../../../control/crates/layerfs-sandbox/src/inspect/space.rs) `serve` |
| `layerfs` | the sum of `parts` | `space.rs` |
| `outside` | `disk.used` less `layerfs`, never below 0: what the filesystem uses that is not LayerFS's data. On a `vm_disk` (docker on macOS and Windows) that is Docker's images, build cache and other volumes; on `native` the rest of the host's disk, a VM machine's own disk image included. `null` unless the walk and the disk read are both complete | `space.rs` |
| `parts.stores` | the folders of the sqlite Stores the home records on the machine (`store/<name>/`: the Store file, its `-wal`, staged snapshots and records) | `space.rs` |
| `parts.sandboxes` | the run folders of the Sandboxes the home records on the machine, the sum of `sandboxes[].bytes` | `space.rs` `sandbox` |
| `parts.images` | the Sandbox image cache, `machine/native/run/images/` (unpacked images, archives, parts) | `space.rs` |
| `parts.other_runs` | the run root's other entries: run folders no listed Sandbox names (a create in flight, or what `layerfs doctor` reports as a leftover) | `space.rs` `other_runs` |
| `parts.logs` | `log/` (the worker log, command records) and `trace/` (the call log) | `space.rs` |
| `parts.other` | `incoming/` (folder copies of `project.init`), `backup/` and `sandbox/` (records) | `space.rs` |
| `stores[]` | one row per Store in `parts.stores`, `bytes` its folder | `space.rs` |
| `sandboxes[]` | one row per recorded Sandbox, from the run folder its daemon record names on the side that runs it: `overlays` (its Workspaces' overlay databases in `overlay/`), `writes` (its own writable layer `overlay/upper` and `work`: what its commands wrote outside Workspaces), `home` (`tmp/`, `HOME` for its commands), `log` (`daemon.log*`), `runtime` (its copies of the daemon, helper and runtime layer, its config, the folder itself). With no run folder measured the sizes are `null` and `reason` says why: `no_run_folder` (no daemon record or folder on that side), `outside_run_root` (the record names a folder elsewhere; it is not read), `record_unreadable`, `deadline`. A row the walk's bound ended inside keeps what it counted, with `reason: "deadline"` | `space.rs` `sandbox` |
| `unreadable` | folders that could not be listed; what is under them is not in the numbers | `space.rs` |
| `complete`, `reason` | `complete: false` when the walk stopped at 250,000 entries or the request's soft budget (4 s): every number is then at least what was counted, a part not reached is `null`, and `outside` is `null`. `reason` joins `deadline`, `unreadable`, `disk_not_read` | `space.rs` |

It is one [inspect](../../control/sandboxes/inspect.md) request, kind
`machine_space`: in this process on `native`, else one request to the
machine's worker. The walk takes no lock or lease and dials no daemon; it
opens no file (each size is one `lstat`, each folder one listing), follows
no link and enters no other device. A run folder's `workspaces/` (its
Workspace mount points) and a private root `<name>.root` are neither
entered nor `stat`ed, so a stopped daemon cannot hang it, and the home
itself is never listed: what an integration keeps there (`runtime/`,
`secret/`, ...) is not counted. It writes nothing, the call log included.
A Sandbox's `/tmp` and `/dev/shm` are memory, not disk: they show in
[`sandbox.get`](sandbox.md#sandboxget) `usage.memory_file`.

```text
$ layerfs machine space native
{"machine":"native","platform":"native","observed_at":1794817324800,"disk":{"total":994662584320,"used":677787914240,"free":316874670080,"path":"/Users/me/.layerfs","fs":"apfs","scope":"host_fs"},"layerfs":14168064,"outside":677773746176,"parts":{"stores":2277376,"sandboxes":11833344,"images":0,"other_runs":0,"logs":8192,"other":49152},"stores":[{"name":"shop","bytes":2277376}],"sandboxes":[{"name":"web","bytes":11833344,"overlays":8675328,"writes":0,"home":3145728,"log":4096,"runtime":8192}],"unreadable":0,"complete":true}
```

Errors: as [`machine.stats`](#machinestats): `NotFound`, `Unavailable`,
`Busy`, and `InvalidInput` (a worker of a build without the kind, or over
4096 Stores or Sandboxes on one machine).

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
| `os_image` option | Text | the VM's operating-system image (an installed OCI reference for Apple Container, an image location for Lima); absent, an installed prepared image, else the provider's stock image; `stock` (`--os-image stock`), the provider's stock image even when a prepared one exists (nothing recorded); recorded and answered as `os_image` |
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
Once removed, the machine's own PostgreSQL connections are forgotten and the
home's image copy locks of it, `<home>/run/image-lock/<machine>/`, are
deleted (`Failed` naming the folder when that deletion fails; the machine
stays removed).

| Error | When |
| --- | --- |
| `Busy` | a live process's create or remove holds it (a gone process's uncertain intent does not refuse) |
| `InUse` | sqlite Stores, Sandboxes or old Firecracker records it hosted are on it (`detail.related`, `can_remove_machine`) |
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
| `docker` | built-in only | the Docker Engine socket the docker machine and every Docker read of the home use: an absolute path (`unix://` and one is taken); `""` clears it (the record keeps `docker: null`, not shown), so the next start chooses one automatically and records it. Otherwise `InvalidInput`; `InUse` while the machine's worker is recorded running (stop it first) |
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
