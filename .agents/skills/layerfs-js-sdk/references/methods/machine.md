# machine calls

> **Status:** Current general guide for the LayerFS 2.0 call surface. Part of
> the `layerfs-js-sdk` skill. Outputs marked "seen" were printed by the built
> `layerfs` command on a throwaway home on 2026-11-18 (macOS); no machine was
> created, removed or started from stopped in that run, so those rows come
> from the table, the spec rows and the manual. Removed with no alias:
> `machine.status` (now `get` with `with: ["live"]`), `machine.space` (now
> `stats` with `with: ["space"]`), `machine.load` (now `load_image`),
> `machine.platforms` (now `home.get({ with: ["platforms"] })`) and
> `logs.machine` (now `machine.log`). An agent working on a project rarely
> needs this family; it belongs to whoever sets up the home.

```js
machine.list({ with: ["live"] })
machine.get(machine, { with: ["live"] })
machine.images(machine, { with: ["docker"] })
machine.load_image(machine, image, { on_progress })
machine.prune(machine, image?, { unused })
machine.stats(machine, { with: ["space"] })
machine.files(machine, path?, { depth, limit, cursor, byte_offset, byte_length })
machine.log(machine, { limit, cursor })
machine.start(machine, { on_progress })
machine.stop(machine, { force, on_progress })
machine.create(name, platform, { cpus, memory, disk, image, os_image, on_progress })
machine.remove(machine, { on_progress })
machine.update(machine, { cpus, memory, image, docker, wsl, daemon })
machine.doctor(machine, { on_progress })
```

A machine is one kernel where Sandboxes run. Two are built in and always
recorded: `native` (the host itself) and `docker`. VMs (`apple-container`,
`lima`) are created. Firecracker, Microsandbox and SSH machines were removed
in 2.0: a record of one is `broken` `unsupported`, and `machine.remove` only
forgets it. A machine runs only between `machine.start` and `machine.stop`:
nothing starts it on demand. A call that needs a stopped machine is refused
`Unavailable` with `fix: layerfs machine start <machine>`.

## `machine.list` and `machine.get`

From the home's records; they probe nothing. A machine's `state` is `ready`,
`busy` (`op`: a `create`, `start`, `stop` or `remove` running), `stopped` or
`broken` (`reason`, such as `engine_down` when Docker is not running, with a
`fix`). `list` rows are short `{ machine, platform }` plus the state when not
ready; `get` answers `{ platform, state }`. The long form has `machine,
platform, state, op, reason, fix, observed_at, backend, supports, cpus,
memory_bytes, disk_bytes, image, os_image, docker, wsl, daemon, message,
stores, stores_count, sandboxes, sandboxes_count, created_at`.

```bash
$ layerfs machine list                                                         # seen
{"items":[{"machine":"docker","platform":"docker","state":"stopped"},{"machine":"native","platform":"native"}]}
$ layerfs machine get native                                                   # seen
{"platform":"native","state":"ready"}
```

`with: ["live"]` probes the provider and says hello to the worker once
(within 10 s, starting nothing); the state then comes from the probe, and
`unknown` means it did not answer. `get`'s `live` section adds
`disk_free_bytes`, `tool_version` and `worker`.

## `machine.stats`

What the machine has and uses now. Short: `cpus`, `memory_used_bytes`,
`memory_total_bytes`, `disk_free_bytes`, `disk_total_bytes`, and
`incomplete` when its 4 s budget ended first. Long adds `disk_used_bytes`,
`reasons`, `machine`, `platform`, `observed_at`, `scope`, `cpu_busy_ms`,
`cpu_total_ms`, `worker`, `data_disk`, `stores` and `sandboxes`.
`with: ["space"]` walks LayerFS's folders: `space { observed_at,
layerfs_bytes, outside_bytes, parts: [{ part, bytes }], stores, sandboxes,
unreadable_count, incomplete, reasons }`. It starts nothing and takes no
lock.

```bash
$ layerfs machine stats native                                                 # seen
{"cpus":14,"memory_used_bytes":25904775168,"memory_total_bytes":38654705664,"disk_free_bytes":129668911104,"disk_total_bytes":994662584320}
```

## `machine.images`, `machine.load_image`, `machine.prune`

- **`images`**: the Sandbox images the machine holds, one row per image:
  `{ image }` with `refs`, `size`, `default`, and a state when not ready
  (`busy` while a copy holds it; `broken` `partial` with a `fix` that
  resumes it). macOS `native` holds none (`mode: "host"`). `with:
  ["docker"]` adds the host's Docker images it could copy.
- **`load_image(machine, image)`**: copies an image (a Docker reference,
  image id or `sha256:` digest) from the host's Docker to the machine without
  creating a Sandbox. Short `{ image }` with `reused` and `dropped` when they
  apply. Progress `inspect`, `lock`, `export`, `send`, `unpack`. Refusals:
  `Capacity` (with `fix: layerfs machine prune <m> --unused`),
  `Unsupported` (another architecture or OS; macOS `native` runs no image),
  `InvalidInput` (an ambiguous reference, or Docker has no such image),
  `Unavailable` (Docker or the worker cannot be reached), `InUse`, `Failed`.
- **`prune(machine, image?, { unused })`**: removes one image, or with
  `unused` every one no Sandbox uses and what copies left behind. Short
  `{ freed_bytes }` with `skipped` (`[{ image, message }]`) when some could
  not be removed. A named image a live copy holds is `Busy`; one a Sandbox
  uses is `InUse`.

```bash
$ layerfs machine images native                                                # seen
{"items":[]}
```

## `machine.files` and `machine.log`

`files` reads what is on a machine, for a tree view: with no `path`, its
LayerFS folders by role; a folder answers a page of `items` (`{ path, type,
size, not_read }`) with `next_cursor`; a file one byte window (`text`,
`next_byte_offset`, or `binary: true` with `size`). Options as
[`sandbox.files`](sandbox.md#sandboxfiles). On `native` a path outside the
LayerFS folders is `InvalidInput`.

```bash
$ layerfs machine files native                                                 # seen
{"items":[{"path":"/tmp/demo/home/store","type":"dir"},{"path":"/tmp/demo/home/sandbox","type":"dir"},{"path":"/tmp/demo/home/machine/macos-host/run","type":"dir"}]}
```

`log` reads the machine's worker log: `items` (`[{ text, continued? }]`),
the last `limit` lines (default 200, 1 to 1000), and `next_cursor` for the
lines before them (left out at the start of the log). A cursor from before
the log rotated is stale (`InvalidInput`). `native` has no worker:

```bash
$ layerfs machine log native --limit 1                                         # seen, exit 125
{"error":"Unsupported","message":"machine native has no worker (its calls run in the caller's process), so it has no worker log"}
```

`stats`, `files` and `log` write no call-log line, and none of the reads
starts a machine.

## Changes

| Call | What it does | Notes |
| --- | --- | --- |
| `start(machine)` | Starts a stopped machine and its worker | A running machine answers `{"unchanged":true}` (seen on `native`), else `{"ok":true}`. Progress `bundle`, `image`, `boot`, `worker`. A worker of another build leaves the machine `broken` `outdated`, `fix: layerfs machine stop <m> && layerfs machine start <m>` (`--force` on the stop when Sandboxes are on it) |
| `stop(machine)` | Stops the worker and, where there is one, the VM; disks stay | A stopped machine answers `{"unchanged":true}` (seen on `native`). `InUse` while Sandboxes are recorded on it; `force` stops them first and answers `stopped_sandboxes`, `unknown_workspaces` |
| `create(name, platform)` | Makes and starts a VM (`lima`, `apple-container`) | `cpus` (2), `memory` (4G), `disk` (20G) as bytes or text with `K`, `M`, `G`; `image`, `os_image`. `{"ok":true}`. `native` and `docker` names are `InvalidInput` |
| `remove(machine)` | Deletes a VM and its disk | `{"ok":true}`; long `outcome` `removed`. A removed platform's record, or one with no 2.0 name (`vm_name`), goes alone, nothing on its host touched, and long `message` says so. `InUse` while Stores, Sandboxes or old Firecracker records are on it; for a record with no 2.0 name the `fix` removes each of them by its records, then the machine |
| `update(machine, …)` | Sets `cpus`, `memory`, `image`, or a built-in machine's Docker socket (`docker`), WSL distribution (`wsl`) and daemon (`daemon`) | `{"ok":true}` (seen: `machine update native --daemon /opt/layerfs/layerfs-daemon`). `InUse` while Sandboxes run on it; no `disk` |
| `doctor(machine)` | Checks the machine, then each Store and Sandbox on it | See [doctors](doctor.md); seen on `native`: `{"ok":true}` |

A `start` or `stop` whose provider reply was lost leaves the machine `broken`
`unresolved` with `fix: layerfs machine get <m> --with live`, which settles
it by observing; an uncertain `create` or `remove` is cleared by
`machine.remove`. Another call on a busy machine is `Busy`, naming the op.
