# machine calls

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-js-sdk` skill. `machine list`, `get native` and `status native`
> were run on macOS in a private home on 2026-11-02. No machine was created,
> connected, updated or removed in this pass: those rows come from the table,
> `layerfs help machine`, S §3.7 and H. An agent working on a project rarely
> needs this family; it belongs to whoever sets up the home.

```js
machine.list()
machine.platforms()
machine.get(name)
machine.status(name)
machine.start(name)
machine.stop(name)
machine.create(name, platform, { cpus, memory, disk, image, nested, host })
machine.connect(name, host, { user, port, identity, known_hosts, root })
machine.remove(name)
machine.disconnect(name)
machine.update(name, { cpus, memory, image, docker, wsl })
```

A machine is one kernel where Sandboxes run. Two are built in and always
recorded: `native` (the host itself) and `docker`. VMs (`apple-container`,
`microsandbox`, `lima`, `firecracker`) are created; an ssh host is connected.
A machine starts on the first call that needs it; its worker stops it when
idle (10 minutes, O2) and it keeps its disk. No list, get or status call
starts one (#535 P4): a UI calls `machine.start(name)` first.

## Reads

```bash
layerfs machine list --text
```

```
NAME    PLATFORM  BACKEND  STATE  CPUS  MEMORY  DISK
docker  docker    docker   ready  -     -       -
native  native    native   ready  -     -       -
```

Each machine also carries `supports` (`sqlite`, `postgresql_sandbox`,
`reason`). `machine.platforms()` (`layerfs machine platforms`) answers each
platform on this host: backend, kernel route, where Store calls run, whether
a PostgreSQL Sandbox runs and why not, `ready`, `missing`, `version` and the
options `machine.create` takes (the rows of `connect/SUPPORT.json`). Neither
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

`NotFound` for a name that isn't recorded.

## Changes (not run here)

| Call | What it does | Notes |
| --- | --- | --- |
| `start(name)` | Starts a stopped machine and its worker and waits until ready | A running machine and `native` answer at once. Returns the machine as `get` does. `InUse` while the machine is being removed |
| `stop(name)` | Stops the worker and, where there is one, the VM; disks stay | `InUse` while Sandboxes run on it; `detail.related` lists them as `{ kind: "sandbox", name }`. `native` answers at once. The next call that needs the machine starts it again |
| `create(name, platform, …)` | Makes and starts a VM | `Unsupported` when the platform's tool is missing; `InvalidInput` for `native`, `docker` or `ssh`. `memory`, `disk`: bytes or text with `K`, `M`, `G`. `nested` is for `lima` only; `host` (required for `firecracker`) names a nested `lima` machine |
| `connect(name, host, …)` | Records an existing Linux host over ssh | `user` defaults to `root`, `port` to 22, `root` to `/layerfs/data`; the host key must already be known. `Unavailable` or `Denied` records nothing; a second machine for the same host is `Exists` |
| `remove(name)` | Deletes a VM and its disk | `InUse` while it holds Stores, Sandboxes or Firecracker machines. While it runs, the record is marked `removing` and every call that would start, update, remove or use the machine is `InUse` |
| `disconnect(name)` | Forgets an ssh machine; its data root stays | `InUse` as above |
| `update(name, …)` | Sets `native`/`docker`'s image, Docker socket (`docker`) and WSL distribution (`wsl`) | `InUse` while Sandboxes run on it; `wsl` only on a Windows host; resizing a VM (`cpus`, `memory`) is `Unsupported`, not built |

`native` and `docker` are never created, removed or disconnected. A Sandbox
on the `docker` machine runs in a container on its image (see
[sandbox](sandbox.md)).
