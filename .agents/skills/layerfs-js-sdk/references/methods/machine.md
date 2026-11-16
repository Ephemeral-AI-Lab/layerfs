# machine calls

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-js-sdk` skill. `machine list`, `get native` and `status native`
> were run on macOS in a private home on 2026-11-02. No machine was created,
> updated or removed in this pass: those rows come from the table,
> `layerfs help machine`, S §3.7 and H. An agent working on a project rarely
> needs this family; it belongs to whoever sets up the home.


Firecracker, Microsandbox and SSH machines were removed in 2.0; there is
no `machine.connect` or `machine.disconnect`. See
[removed platforms](#removed-platforms-firecracker-microsandbox-and-ssh).

```js
machine.list()
machine.platforms()
machine.get(name)
machine.images(name)
machine.load(name, image, { on_progress })
machine.prune(name, image?, { unused })
machine.status(name)
machine.stats(machine)
machine.space(machine)
machine.files(machine, path?, { depth, cursor, limit, offset, length })
machine.start(name, { on_progress })
machine.stop(name, { force })
machine.create(name, platform, { cpus, memory, disk, image, os_image, on_progress })
machine.remove(name)
machine.update(name, { cpus, memory, image, docker, wsl, daemon })
```

A machine is one kernel where Sandboxes run. Two are built in and always
recorded: `native` (the host itself) and `docker`. VMs (`apple-container`,
`lima`) are created.
A machine runs only between `machine.start(name)` and `machine.stop(name)`
(#535 D11): nothing starts it on demand and nothing stops it when unused. A
call that needs a stopped machine is refused `Unavailable`, its message
naming `layerfs machine start <name>`; call `machine.start(name)` first.

## Reads

```bash
layerfs machine list --text
```

```
NAME    PLATFORM  BACKEND  STATE  CPUS  MEMORY  DISK
docker  docker    docker   stopped  -     -       -
native  native    native   ready    -     -       -
```

`native` is always `ready`. Every other machine, the built-in `docker`
included, shows its live state as `machine status` reads it (`running`,
`stopped`, …): its worker is started and stopped by hand (`machine start`).

Each machine also carries `supports` (`sqlite`, `postgresql_sandbox`,
`reason`). `machine.platforms()` (`layerfs machine platforms`) answers each
platform on this host: backend, kernel route, where Store calls run, whether
a PostgreSQL Sandbox runs and why not, `ready`, `missing`, `version` and the
options `machine.create` takes (the rows of `control/SUPPORT.json`). Neither
call starts a machine.

`machine.get(name)` is the record:

```json
{"name":"native","platform":"native","backend":"native","state":"ready","cpus":null,"memory":null,"disk":null,"stores":["shop"],"sandboxes":[]}
```

`machine.status(name)` is the live state; a stopped VM answers without being
started:

```json
{"name":"native","platform":"native","state":"running","free_disk":null,"tool_version":null,"down_reason":null,"stores":["shop"]}
```

While a create or remove holds the machine, its record
holds that call's `op`. `get` and `status` then probe nothing: `state` is
`creating` or `removing` while that process lives, and `unresolved` once it
is gone with the outcome unknown. `status` also answers
`op: { id, name, phase, started, uncertain }` (`name` is `machine.create`
or `machine.remove`).

`NotFound` for a name that isn't recorded.

### `machine.images`

`machine.images(name)`: the Sandbox images the machine holds and the host's
Docker images it could copy, read without starting it. `mode` is `images`, or
`host` on macOS `native` (no rows, `docker: null`). `available` lists the
host's tagged Docker images not already unpacked; a row for another OS or
architecture is `usable: false` with a `reason`. A Docker failure fills
`docker.available: false` and never fails the call.

```text
{ machine, mode, architecture, default, free,
  images: [{ id, refs, size, architecture, source, sandboxes, last_used }],
  docker: null | { available: true } | { available: false, reason, message },
  available: [{ id, refs, size, os, architecture, usable, reason?, need, fits }] }
```

Errors: `NotFound`; `Unavailable` when the machine's worker does not answer.

### `machine.stats`

`machine.stats(machine)`: what the machine has and uses now, for a UI that
polls. It answers the present only: processor values are counters, and a
client keeps its own history and turns two reads into a rate.

```text
{ machine, platform, observed_at, worker, resources, data_disk, stores,
  sandboxes: { recorded, live, items: [{ name, live, live_reason?, started_at, usage, usage_reason? }] },
  complete, reason? }
```

`worker` is the machine's worker process (`null` on `native`); `resources`
carries `scope`: `host`, `vm` or `engine_host`. `sandboxes.items` has one row
per recorded Sandbox; a worker of an older build answers none. `complete:
false` with `reason: "deadline"` means the 4 s soft budget ended the Store or
Sandbox part early. Nothing is locked, opened or started, and PostgreSQL
Stores are not listed. Errors: `NotFound`, `Unavailable`, `Busy`,
`InvalidInput`.

### `machine.space`

`machine.space(machine)`: where the data disk goes, for a UI that warns
before it fills. Every size is the blocks a part takes.

```text
{ machine, platform, observed_at, disk, layerfs, outside,
  parts: { stores, sandboxes, images, other_runs, logs, other },
  stores: [{ name, bytes }],
  sandboxes: [{ name, bytes, overlays, writes, home, log, runtime, reason? }],
  unreadable, complete, reason? }
```

`outside` is `disk.used` less `layerfs`, never below 0. `complete: false`
when the walk stopped at 250,000 entries or its 4 s budget (`reason` joins
`deadline`, `unreadable`, `disk_not_read`); a part not reached is `null`. It
takes no lock, dials no daemon and writes nothing, the call log included.
Errors as `machine.stats`.

### `machine.files`

`machine.files(machine, path?, { depth, cursor, limit, offset, length })`:
reads what is on a machine, for a UI that shows a tree. Its options, a folder
page, a file window and their fields are [`sandbox.files`](sandbox.md)'s;
what differs is where a path may lead.

With no `path` it answers the roots that exist, each with a `role` (`data`,
`stores`, `sandbox_records`, `run_folders`, `images`, `staging`, `logs`,
`state`, `bundle`, `image`, `machine`); a client offers a "whole filesystem"
view only when the answer has the `machine` root. On `native` that root is
absent: a path outside the LayerFS folders is `InvalidInput`. It takes no
lock, lease or daemon call, and writes nothing, the call log included.
Errors: `InvalidInput`, `NotFound`, `Unavailable`, `Busy`, `Unsupported`.

## Changes (not run here)

| Call | What it does | Notes |
| --- | --- | --- |
| `start(name)` | Starts a stopped machine and its worker and waits until ready | A running machine and `native` answer at once. Returns the machine as `get` does. `Busy` while another call holds the machine (its `op`) |
| `stop(name)` | Stops the worker and, where there is one, the VM; disks stay | `InUse` while Sandboxes run on it; `detail.related` lists them as `{ kind: "sandbox", name }`. `native` answers at once. Later calls that need the machine are refused `Unavailable` until `start` |
| `load(name, image, …)` | Copies an image to the machine as `sandbox.create`'s `image` does, without a Sandbox | `image` is a digest, Docker image Id or reference. An image the machine holds is `reused: true` and needs neither Docker nor a byte sent. Progress `inspect`, `export`, `send`, `unpack`. `Unsupported` (`host` on macOS `native`; `arch`/`os`), `InvalidInput` (Docker has no such image), `Unavailable`, `NoSpace` |
| `prune(name, image?, …)` | Removes one unpacked image, or with `unused` every one no Sandbox uses except the default | A named in-use image is `InUse` (`detail.count`); `unused` skips it. Naming the default removes it and the next create unpacks it again. `freed` is free bytes after minus before. `InvalidInput`, `NotFound`, `InUse`, `Unsupported` |
| `create(name, platform, …)` | Makes and starts an enabled VM (`apple-container`, `lima`) | `Unsupported` when the platform's tool is missing; `InvalidInput` for `native` or `docker`; `Unsupported` for the removed `firecracker`, `ssh` and `microsandbox`. `memory`, `disk`: bytes or text with `K`, `M`, `G`; `os_image` sets the VM's operating-system image; `on_progress` reports the start stages plus `{ phase: "create", machine }` |
| `remove(name)` | Deletes a VM and its disk; a removed platform's record is only forgotten | `InUse` while it holds Stores or Sandboxes. While it runs, the record holds its `op` and every call that would start, update, remove or use the machine is `Busy` |
| `update(name, …)` | Sets `native`/`docker`'s image, Docker socket (`docker`), WSL distribution (`wsl`) and the daemon path (`daemon`) | `InUse` while Sandboxes run on it; `wsl` only on a Windows host; resizing a VM (`cpus`, `memory`) is `Unsupported`, not built |

An `Unknown` outcome of a create or remove keeps its
`op`, marked `uncertain`, with the `error`. Once that process is gone,
every other call on the machine is `Unresolved`, naming
`layerfs machine status <name>` and the call that clears it:
`machine.remove(name)`.

`native` and `docker` are never created or removed. A Sandbox
on the `docker` machine runs in a container on its image; with none named,
the bundle's Sandbox image is loaded into Docker once and used (#535 P5; see
[sandbox](sandbox.md)).

## Removed platforms: Firecracker, Microsandbox and SSH

A home may still hold a machine record whose platform is `firecracker`,
`ssh` or `microsandbox`:

- `machine.list()` and `machine.get(name)` answer it: `platform` as
  recorded, `backend: "-"`, `supports: { sqlite: false,
  postgresql_sandbox: false, reason }`, `state: "unsupported"` and `error`
  the reason (``platform `ssh` was removed in LayerFS 2.0``, or the same for
  `firecracker` or `microsandbox`), plus its recorded `ssh` settings, `host`
  or `vm_name`.
- Every other call on it (`status`, `start`, `stop`, `update`,
  `sandbox.create` on it, Workspace calls on its Sandboxes, Store and
  project calls on a sqlite Store on it) is refused `Unsupported` with
  that reason.
- `machine.remove(name)` only forgets the record (`InUse` as for any
  remove) and answers `{ name, removed: true, note }`; the note says
  nothing on its host was touched. For microsandbox it names
  `msb remove --force <vm>` and `msb volume remove <vm>-data`, which you
  run yourself to delete the VM and its volume. For firecracker it names
  the microVM's folder and data disk left under `/layerfs/data/firecracker`
  in its Lima host.
