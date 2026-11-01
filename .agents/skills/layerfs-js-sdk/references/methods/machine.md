# machine calls

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-js-sdk` skill. `machine list`, `get native` and `status native`
> were run on macOS in a private home on 2026-11-02. No machine was created,
> connected, updated or removed in this pass: those rows come from the table,
> `layerfs help machine`, S §3.7 and H. An agent working on a project rarely
> needs this family; it belongs to whoever sets up the home.

```js
machine.list()
machine.get(name)
machine.status(name)
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
idle (10 minutes, O2) and it keeps its disk.

## Reads

```bash
layerfs machine list --text
```

```
NAME    PLATFORM  BACKEND  STATE  CPUS  MEMORY  DISK
docker  docker    docker   ready  -     -       -
native  native    native   ready  -     -       -

PLATFORM         READY  VERSION
apple-container  true   container CLI version 1.6.1 (…)
microsandbox     false  -
lima             true   limactl version 2.2.1
…
```

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
| `create(name, platform, …)` | Makes and starts a VM | `Unsupported` when the platform's tool is missing; `InvalidInput` for `native`, `docker` or `ssh`. `memory`, `disk`: bytes or text with `K`, `M`, `G`. `nested` is for `lima` only; `host` (required for `firecracker`) names a nested `lima` machine |
| `connect(name, host, …)` | Records an existing Linux host over ssh | `user` defaults to `root`, `port` to 22, `root` to `/layerfs/data`; the host key must already be known. `Unavailable` or `Denied` records nothing; a second machine for the same host is `Exists` |
| `remove(name)` | Deletes a VM and its disk | `InUse` while it holds Stores, Sandboxes or Firecracker machines |
| `disconnect(name)` | Forgets an ssh machine; its data root stays | `InUse` as above |
| `update(name, …)` | Resizes a VM, or sets `native`/`docker`'s image, Docker socket (`docker`) and WSL distribution (`wsl`) | `InUse` while Sandboxes run on it; `wsl` only on a Windows host |

`native` and `docker` are never created, removed or disconnected. In this
build a Sandbox on the `docker` machine is `Unsupported` (see
[sandbox](sandbox.md)).
