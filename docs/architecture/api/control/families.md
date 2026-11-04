# Families

[Architecture](../../README.md) › [API](../README.md) › [Control](README.md) › Families

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How control implements each call family.

| | |
| --- | --- |
| Component | [Control](README.md) |
| Source | [`api/crates/layerfs-control/src/machine`](../../../../api/crates/layerfs-control/src/machine), [`api/crates/layerfs-control/src/store`](../../../../api/crates/layerfs-control/src/store), [`api/crates/layerfs-control/src/project`](../../../../api/crates/layerfs-control/src/project), [`api/crates/layerfs-control/src/sandbox`](../../../../api/crates/layerfs-control/src/sandbox), [`api/crates/layerfs-control/src/workspace`](../../../../api/crates/layerfs-control/src/workspace), [`api/crates/layerfs-control/src/backup`](../../../../api/crates/layerfs-control/src/backup) |

Every family function takes the running call's `Context` (its home and
its event stream) and the checked arguments, and answers one `Outcome`
([`common/context.rs`](../../../../api/crates/layerfs-control/src/common/context.rs),
[`common/outcome.rs`](../../../../api/crates/layerfs-control/src/common/outcome.rs)). The call table's entries
name these functions ([Table](../call-table/table.md)).

```text
 table entry          control function                       folder
 machine.*        --> machine::{list,get,create,...}         machine/
 store.*          --> store::family::run / connect::run      store/
 branch|commit|file.* --> store::calls::run                  store/
 project.*        --> project::family::run                   project/
 sandbox.*        --> sandbox::family::sandbox               sandbox/
 workspace.*      --> workspace::calls::workspace            workspace/
 backup.*         --> backup::family::run                    backup/
```

The multi-step operations share one shape, the `op` intent
([Home and records](home-and-records.md#records)):

```text
 home lock + entity lock:  check, write op (phase 1)        short hold
 no lock:                  the work (VM boot, transfer,     long
                           worker call, runtime start)
 home lock + entity lock:  write result, drop op             short hold
 lost answer:              keep op uncertain, log, Unknown
```

## machine

Records in `machine/<name>/`, adapters from `layerfs-machine`
([Machines](../../connect/machines/README.md)). Built without the
`machine` feature, every call is `Unsupported`
([`machine/mod.rs`](../../../../api/crates/layerfs-control/src/machine/mod.rs)).

| Call | Does | Intent / lock | Source |
| --- | --- | --- | --- |
| `list`, `get` | records with live probes and `supports`; start nothing | none | [`list.rs`](../../../../api/crates/layerfs-control/src/machine/list.rs), [`get.rs`](../../../../api/crates/layerfs-control/src/machine/get.rs) |
| `platforms` | the support table rows for this host and build, each with its probe | none | [`platforms.rs`](../../../../api/crates/layerfs-control/src/machine/platforms.rs) |
| `status` | live state, free disk, tool version | none | [`status.rs`](../../../../api/crates/layerfs-control/src/machine/status.rs) |
| `start` | the only call that starts a machine: dials `worker.json`'s worker, else starts the VM and worker, waits for readiness, rewrites `worker.json` | `start.lock` | [`endpoint.rs`](../../../../api/crates/layerfs-control/src/machine/endpoint.rs), [`power.rs`](../../../../api/crates/layerfs-control/src/machine/power.rs) |
| `stop` | stops worker and VM; disks and Stores stay; `InUse` while Sandboxes are recorded on it | `start.lock` | [`power.rs`](../../../../api/crates/layerfs-control/src/machine/power.rs) |
| `create` | claims the name with `op` `machine.create`, makes the VM outside the home lock, records `running`, `failed` or an uncertain `op` | `op`, `start.lock` | [`create.rs`](../../../../api/crates/layerfs-control/src/machine/create.rs) |
| `connect` | an ssh host; host key checked strictly; `op` `machine.connect` | `op` | [`connect.rs`](../../../../api/crates/layerfs-control/src/machine/connect.rs) |
| `remove`, `disconnect` | delete a VM and its disk, or forget an ssh host (its root stays); `InUse` while it holds Stores, Sandboxes or hosted machines | `op`, `start.lock` | [`remove.rs`](../../../../api/crates/layerfs-control/src/machine/remove.rs), [`inventory.rs`](../../../../api/crates/layerfs-control/src/inventory.rs) |
| `update` | resize a VM, or set a built-in machine's image, Docker socket, `wsl` setting or daemon; `InUse` while Sandboxes run | home lock | [`update.rs`](../../../../api/crates/layerfs-control/src/machine/update.rs) |

`machine/worker/` holds the worker client and server; see
[Workers](../routing/workers.md).

## store

| Call | Kind | Runs | Source |
| --- | --- | --- | --- |
| `list`, `get`, `status` | both | reads records and leases in this process; take no Store lock | [`list.rs`](../../../../api/crates/layerfs-control/src/store/list.rs), [`status.rs`](../../../../api/crates/layerfs-control/src/store/status.rs) |
| `create` | sqlite | an empty Store on `native` | [`create.rs`](../../../../api/crates/layerfs-control/src/store/create.rs) |
| `connect`, `disconnect` | PostgreSQL | record the database in schema `layerfs` with a password reference; forget it (data stays); `disconnect` is `InUse` while leased | [`connect.rs`](../../../../api/crates/layerfs-control/src/store/connect.rs) |
| `push`, `pull` | sqlite | move the file `native` to a machine, or back; phases `check`, `snapshot`, `send`, `verify`, `install`, `record`, `retire` | [`transfer/phases.rs`](../../../../api/crates/layerfs-control/src/store/transfer/phases.rs) |
| `remove` | sqlite | delete the file wherever it is, with its projects; a PostgreSQL Store is refused `InvalidInput` (use `disconnect`) | [`remove.rs`](../../../../api/crates/layerfs-control/src/store/remove.rs) |

```text
 push / pull / remove
   home lock + store lock: leases idle? (InUse) -> write op
   per phase: op.at(phase) -> step on its side:
       native: transfer::steps::run in this process
       other : worker "store.file" request
   record phase or lost answer -> op uncertain, both copies kept
   success: record names the new machine, op dropped
```

The `branch`, `commit` and `file` families are Store calls too
([`store/calls.rs`](../../../../api/crates/layerfs-control/src/store/calls.rs)): each translates to one Store
method (`fs.read`, `branch.fork`, …) and runs through `store_call`
([`store/call.rs`](../../../../api/crates/layerfs-control/src/store/call.rs)) under the Store's lease, in this
process or in the machine's worker
([Routing table](../routing/routing-table.md)). Open Stores are cached
per process and closed after 60 s idle
([`store/open.rs`](../../../../api/crates/layerfs-control/src/store/open.rs)).

## project

| Call | Does | Source |
| --- | --- | --- |
| `list`, `get` | project records, by Store then name | [`family.rs`](../../../../api/crates/layerfs-control/src/project/family.rs) |
| `init` | adds a host folder as a new project to an existing Store, a live add through the Store's writer | [`family.rs`](../../../../api/crates/layerfs-control/src/project/family.rs) |
| `remove` | drops the project's Layers, Branches and Commits in one write, then its record; its Sandboxes must be closed first | [`family.rs`](../../../../api/crates/layerfs-control/src/project/family.rs) |

```text
 project.init(project, folder)
   PostgreSQL Store ---------> live add in this process   postgres_init.rs
   sqlite on a Local machine -> live add in this process   init.rs
   sqlite behind a worker:
     walk folder on this host, refuse what Init refuses    send.rs
     room check on the machine (Capacity)
     project.stage x N  (chunks of tree records)           tree.rs
     project.init  -> worker adds <root>/incoming/init-<id> staged.rs
     record the project in the home
 progress: {phase: check | send | install | record, done, total}
```

A lost answer is `Unknown` and the home gains no project record.

## sandbox

| Call | Does | Source |
| --- | --- | --- |
| `create` | claims the name under the home lock with `op` `sandbox.create`, starts the Sandbox on its machine (its sqlite Store's machine, else `native`), records the result and drops the `op` in one write | [`create.rs`](../../../../api/crates/layerfs-control/src/sandbox/create.rs) |
| `list`, `get` | Sandbox records with derived state; `get` adds one page of Workspaces | [`list.rs`](../../../../api/crates/layerfs-control/src/sandbox/list.rs), [`get.rs`](../../../../api/crates/layerfs-control/src/sandbox/get.rs) |
| `close` | records `op` `sandbox.close`, stops the runtime, removes the record; `MountsActive` without `force`; clears an unresolved create | [`close.rs`](../../../../api/crates/layerfs-control/src/sandbox/close.rs) |

Host operations run through `route::on_machine`: in this process when the
machine is local, else as a worker `sandbox.host` request carrying the
Store and project records ([`route.rs`](../../../../api/crates/layerfs-control/src/sandbox/route.rs)). For a
PostgreSQL Store this process resolves the password and the create checks
the server is reachable from the machine first. Where this host reaches a
Sandbox's daemon (`ssh`, `lima`, `docker`), the keys go to
`sandbox/<name>/control.json` ([`direct.rs`](../../../../api/crates/layerfs-control/src/sandbox/direct.rs)).

## workspace

```text
 workspace.mount(sandbox, branch, {name})
   record.free()? (Busy / Unresolved)
   direct::forwarded? -- yes --> daemon from this process
                     \- no ---> on_machine (Local or worker sandbox.host)
   answer + ws = "<sandbox>:<path>" or "<sandbox>:<name>"

 exec | commit | unmount | status (ws)
   forwarded and not exec --> daemon from this process
   status                 --> reach() without starting anything
   otherwise              --> on_machine; exec output also as events
```

| Call | Daemon or runtime | Source |
| --- | --- | --- |
| `mount`, `commit`, `unmount`, `status` | the Sandbox's daemon | [`workspace/calls.rs`](../../../../api/crates/layerfs-control/src/workspace/calls.rs), [`workspace/serve.rs`](../../../../api/crates/layerfs-control/src/workspace/serve.rs) |
| `exec` | the Sandbox runtime runs `bash -c` in the Workspace; no daemon | [`sandbox/exec.rs`](../../../../api/crates/layerfs-control/src/sandbox/exec.rs) |

The daemon serves no Store calls. See
[Daemon channel](../routing/daemon-channel.md).

## backup

| Call | Does | Source |
| --- | --- | --- |
| `connect` | one listing proves the bucket answers with the key, then `backup/<bucket>/record.json` (a key reference) | [`family.rs`](../../../../api/crates/layerfs-control/src/backup/family.rs), [`record.rs`](../../../../api/crates/layerfs-control/src/backup/record.rs) |
| `disconnect` | forgets the bucket and the Stores' links to it | [`family.rs`](../../../../api/crates/layerfs-control/src/backup/family.rs) |
| `list` | every Store file on the connected buckets | [`list.rs`](../../../../api/crates/layerfs-control/src/backup/list.rs) |
| `create` | the Store's machine stages a snapshot (`store.file`); this process uploads it; `Exists` if another home wrote the object | [`create.rs`](../../../../api/crates/layerfs-control/src/backup/create.rs) |
| `restore` | downloads into `store/.restore-<random>/`, installs as a sqlite Store on `native`; `Conflict` if the Store changed since its last backup or restore, `InUse` while held | [`restore.rs`](../../../../api/crates/layerfs-control/src/backup/restore.rs) |

Bytes go through the caller's process: machines never hold bucket keys,
and LayerFS never deletes a bucket object. See
[Backups](../../connect/backups/README.md).

## Related

- [Home and records](home-and-records.md), [Doctor](doctor.md)
- [Routing table](../routing/routing-table.md), [Workers](../routing/workers.md), [Daemon channel](../routing/daemon-channel.md)
- [Call table](../call-table/README.md) and its family pages, such as [store family](../sdk/store/README.md)
- [Engine: Store calls](../../engine/store-calls/README.md), [Engine: Project Init](../../engine/project-init/README.md), [Connect: Sandboxes](../../connect/sandboxes/README.md)

---

← [Control: Home and records](home-and-records.md) · [Up: Control](README.md) · [Control: Doctor](doctor.md) →
