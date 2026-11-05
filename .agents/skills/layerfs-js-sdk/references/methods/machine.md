# machine calls

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-js-sdk` skill. `machine list`, `get native` and `status native`
> were run on macOS in a private home on 2026-11-02. No machine was created,
> connected, updated or removed in this pass: those rows come from the table,
> `layerfs help machine`, S §3.7 and H. An agent working on a project rarely
> needs this family; it belongs to whoever sets up the home.


Firecracker and SSH are permanently disabled for new work. Keep existing
records and data; inspection and explicit cleanup remain available. There
is no environment or feature override. `machine.start` cannot restart them.

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
`microsandbox`, `lima`) are created; Firecracker and SSH are disabled.
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
platform on this host: backend, kernel route, the isolation class it provides
(`isolation_class`) with its `exec_mode` and `kernel_scope`, where Store calls
run, whether a PostgreSQL Sandbox runs and why not, `ready`, `missing`,
`version` and the options `machine.create` takes (the rows of
`connect/SUPPORT.json`). Neither call starts a machine.

`machine.get(name)` is the record:

```json
{"name":"native","kind":"process","platform":"native","backend":"native","isolation_class":"process","exec_mode":"worker","kernel":"shared","schema_version":1,"state":"ready","cpus":null,"memory":null,"disk":null,"stores":["shop"],"sandboxes":[],"isolation":{"class":"process","requested":"process","effective":"process","kernel":"shared","strength":"weak"},"scheduling":{"worker":true,"long_lived":true,"exec_mode":"worker"},"capability":{"schema_version":1,"isolation":"process","kernel":"shared","strength":"weak","provider":"native","backend":"native","mount_driver":"loopback-nfs","idle_stop":"ours","snapshot":null,"notes":"Sandboxes run as this user's processes, served over loopback NFS","record_schema_version":1},"worker":{"state":"none"},"lease":{"held":false},"health":{"ok":true,"state":"ready","reason":null}}
```

`kind` is the isolation class only (#551), the same word as
`isolation_class`. `schema_version` is the record's contract version (1);
a record written before #551 reads 1 through the document's fallbacks and
is migrated only by a write. `capability` is the backend's typed
declaration, also `schema_version` 1: its class, kernel, strength word,
provider and backend names, `mount_driver` (`fuse`, `loopback-nfs`,
`shared-fs` or `none`), `idle_stop` (`ours`, `provider` or `none`), a
`snapshot` object or `null`, and its notes. `worker` is what `worker.json`
records (`state`: `running`, `recorded` or `none`; `address`, `started`,
`forward_pid`), read without dialing; `lease` is the machine's
`start.lock` (`held`, its `path`, `since`), probed without taking it; and
`health` is `{ ok, state, reason }`, `ok` for `ready` or `running`.

The `machine.list` and `machine.get` live probes share a **2 s** budget
(#551 B8-159), so a slow provider answers `unknown` with its reason instead
of holding the call. Neither call starts a machine, and neither writes the
home.

A machine's `isolation_class` is the boundary its platform provides on this
host (#551 B0-144): `process` (a host process or namespaces), `container`
(the host kernel, docker), `vm` (its own kernel: any VM or microVM) or
`remote` (an ssh host), weakest first. `kernel` is `shared`, `separate` or
`separate-host`; `exec_mode` is `worker`, or `remote-exec` for a provider
that runs each command through its own API. The `isolation` object repeats
the class with its `kernel` and the capability matrix's `strength` word
(`weak`, `medium`, `strong`, `network`); a machine has no separate request,
so its `requested` equals its class. `scheduling` says whether work runs
through our worker (`worker`, false for `remote-exec`), its `long_lived`
(true for a machine that keeps its state between calls, as every machine
does today) and its `exec_mode`. A Sandbox that asks for a class is satisfied
by that class or a stronger one and never by a weaker one; see
[sandbox](sandbox.md).

`machine.status(name)` is the live state; a stopped VM answers without being
started:

```json
{"name":"native","platform":"native","state":"running","free_disk":null,"tool_version":null,"down_reason":null,"stores":["shop"]}
```

While a create, connect, remove or disconnect holds the machine, its record
holds that call's `op`. `get` and `status` then probe nothing: `state` is
`creating` or `removing` while that process lives, and `unresolved` once it
is gone with the outcome unknown. `status` also answers
`op: { id, name, phase, started, uncertain }` (`name` is `machine.create`,
`machine.connect`, `machine.remove` or `machine.disconnect`).

`NotFound` for a name that isn't recorded.

## Changes (not run here)

| Call | What it does | Notes |
| --- | --- | --- |
| `start(name)` | Starts a stopped machine and its worker and waits until ready | An enabled running machine and `native` answer at once; disabled machines always refuse. Returns the machine as `get` does. `Busy` while another call holds the machine (its `op`) |
| `stop(name)` | Stops the worker and, where there is one, the VM; disks stay | `InUse` while Sandboxes run on it; `detail.related` lists them as `{ kind: "sandbox", name }`. `native` answers at once. Later calls that need the machine are refused `Unavailable` until `start` |
| `create(name, platform, …)` | Makes and starts a VM | `Unsupported` when the platform's tool is missing; `InvalidInput` for `native` or `docker`; `Unsupported` for permanently disabled `ssh` or `firecracker`. `memory`, `disk`: bytes or text with `K`, `M`, `G`. `nested` is for `lima` only; `host` (required for `firecracker`) names a nested `lima` machine |
| `connect(name, host, …)` | Permanently refused `Unsupported` | `user` defaults to `root`, `port` to 22, `root` to `/layerfs/data`; the host key must already be known. `Unavailable` or `Denied` records nothing; a second machine for the same host is `Exists` |
| `remove(name)` | Deletes a VM and its disk | `InUse` while it holds Stores, Sandboxes or Firecracker machines. While it runs, the record holds its `op` and every call that would start, update, remove or use the machine is `Busy` |
| `disconnect(name)` | Forgets an ssh machine; its data root stays | `InUse` and `Busy` as above |
| `update(name, …)` | Sets `native`/`docker`'s image, Docker socket (`docker`) and WSL distribution (`wsl`) | `InUse` while Sandboxes run on it; `wsl` only on a Windows host; resizing a VM (`cpus`, `memory`) is `Unsupported`, not built |

An `Unknown` outcome of a create, connect, remove or disconnect keeps its
`op`, marked `uncertain`, with the `error`. Once that process is gone,
every other call on the machine is `Unresolved`, naming
`layerfs machine status <name>` and the call that clears it:
`machine.remove(name)`, or `machine.disconnect(name)` for ssh.

`native` and `docker` are never created, removed or disconnected. A Sandbox
on the `docker` machine runs in a container on its image; with none named,
the bundle's Sandbox image is loaded into Docker once and used (#535 P5; see
[sandbox](sandbox.md)).
