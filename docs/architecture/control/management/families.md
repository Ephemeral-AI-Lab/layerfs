# Families

[Architecture](../../README.md) › [Control](../README.md) › [Management](README.md) › Families

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How control implements each call family.

| | |
| --- | --- |
| Component | [Control](README.md) |
| Source | [`control/crates/layerfs-control/src/machine`](../../../../control/crates/layerfs-machine/src), [`control/crates/layerfs-control/src/store`](../../../../control/crates/layerfs-store/src), [`project`](../../../../control/crates/layerfs-store/src/project), [`control/crates/layerfs-control/src/sandbox`](../../../../control/crates/layerfs-sandbox/src), [`control/crates/layerfs-control/src/workspace`](../../../../control/crates/layerfs-sandbox/src/workspace), [`backup`](../../../../control/crates/layerfs-store/src/backup) |

Every family function takes the home, the call's `Events` when it emits
(its event stream; layerfs-project#107) and the checked arguments, and
answers one `Outcome`
([`events.rs`](../../../../control/crates/layerfs-home/src/events.rs),
[`outcome.rs`](../../../../control/crates/layerfs-home/src/outcome.rs)). The call table's entries
name these functions ([Table](../../api/call-table/table.md)).

```text
 table entry          control function                       folder
 machine.*        --> machine::{list,get,create,...}         machine/
 store.*          --> sqlite::family::<call> / connect::<call> store/
 branch|commit|file.* --> store::calls::<family>_<call>      store/
 project.*        --> project::family::<call>                project/
 sandbox.*        --> sandbox::family::<call>                sandbox/
 workspace.*      --> workspace::calls::<call>               workspace/
 backup.*         --> backup::family::<call>                 backup/
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

Records in `machine/<name>/`, adapters from `layerfs-platform`
([Machines](../machines/README.md)). Built without the
`machine` feature, every call is `Unsupported`
([`machine/mod.rs`](../../../../control/crates/layerfs-machine/src/lib.rs)).

| Call | Does | Intent / lock | Source |
| --- | --- | --- | --- |
| `list`, `get` | records with live probes and `supports`; start nothing | none | [`list.rs`](../../../../control/crates/layerfs-machine/src/list.rs), [`get.rs`](../../../../control/crates/layerfs-machine/src/get.rs) |
| `platforms` | the support table rows for this host and build, each with its probe | none | [`platforms.rs`](../../../../control/crates/layerfs-machine/src/platforms.rs) |
| `status` | live state, free disk, tool version | none | [`status.rs`](../../../../control/crates/layerfs-machine/src/status.rs) |
| `start` | the only call that starts a machine: dials `worker.json`'s worker, else starts the VM and worker, waits for readiness, rewrites `worker.json` | `start.lock` | [`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs), [`power.rs`](../../../../control/crates/layerfs-machine/src/power.rs) |
| `stop` | stops worker and VM; disks and Stores stay; `InUse` while Sandboxes are recorded on it | `start.lock` | [`power.rs`](../../../../control/crates/layerfs-machine/src/power.rs) |
| `create` | claims the name with `op` `machine.create`, makes the VM outside the home lock, records `running`, `failed` or an uncertain `op` | `op`, `start.lock` | [`create.rs`](../../../../control/crates/layerfs-machine/src/create.rs) |
| `remove` | delete a VM and its disk; a machine whose platform was removed in 2.0 (`firecracker`, `ssh`, `microsandbox`) is only forgotten, its record removed and nothing outside the home touched ([Removed platforms](../support-matrix/disabled-platforms.md)); `InUse` while it holds Stores or Sandboxes | `op`, `start.lock` (forget: home lock only) | [`remove.rs`](../../../../control/crates/layerfs-machine/src/remove.rs), [`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) |
| `update` | resize a VM, or set a built-in machine's image (on `docker`, its worker container's image), Docker socket, `wsl` setting or daemon; `InUse` while Sandboxes run | home lock | [`update.rs`](../../../../control/crates/layerfs-machine/src/update.rs) |

`machine/worker/` holds the worker client and server; see
[Workers](../routing/workers.md).

## store

| Call | Kind | Runs | Source |
| --- | --- | --- | --- |
| `list`, `get`, `status` | both | reads records and leases in this process; take no Store lock | [`list.rs`](../../../../control/crates/layerfs-store/src/list.rs), [`status.rs`](../../../../control/crates/layerfs-store/src/status.rs) |
| `create` | sqlite | an empty Store on `native` | [`sqlite/create.rs`](../../../../control/crates/layerfs-store/src/sqlite/create.rs) |
| `connect`, `disconnect` | PostgreSQL | record the database in schema `layerfs` with a password reference; forget it (data stays); `disconnect` is `InUse` while leased | [`postgresql/connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs) |
| `push`, `pull` | sqlite | move the file `native` to a machine, or back; phases `check`, `snapshot`, `send`, `verify`, `install`, `record`, `retire` | [`sqlite/transfer/phases.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) |
| `remove` | sqlite | delete the file wherever it is, with its projects; a PostgreSQL Store is refused `InvalidInput` (use `disconnect`) | [`sqlite/remove.rs`](../../../../control/crates/layerfs-store/src/sqlite/remove.rs) |

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
([`calls/`](../../../../control/crates/layerfs-store/src/calls/mod.rs), one translation each in [`calls/op.rs`](../../../../control/crates/layerfs-store/src/calls/op.rs)): each translates to one Store
method (`fs.read`, `branch.fork`, …) and runs through `store_call`
([`call.rs`](../../../../control/crates/layerfs-store/src/call.rs)) under the Store's lease, in this
process or in the machine's worker
([Routing table](../routing/routing-table.md)). Open Stores are cached
per process and closed after 60 s idle
([`open.rs`](../../../../control/crates/layerfs-store/src/open.rs)). When the last call on an open
SQLite Store returns, or a Sandbox on it closes, its write-ahead log is
checkpointed and truncated once while it stays open (#86), so a quiesced
Store's size is its file.

## project

| Call | Does | Source |
| --- | --- | --- |
| `list`, `get` | project records, by Store then name | [`project/family.rs`](../../../../control/crates/layerfs-store/src/project/family.rs) |
| `init` | adds a host folder as a new project to an existing Store, a live add through the Store's writer | [`project/family.rs`](../../../../control/crates/layerfs-store/src/project/family.rs) |
| `export` | exports a project's committed Branch snapshot from its Store to a local folder on the caller's host | [`project/export/`](../../../../control/crates/layerfs-store/src/project/export/route.rs) |
| `remove` | drops the project's Layers, Branches and Commits in one write, then its record; its Sandboxes must be closed first | [`project/family.rs`](../../../../control/crates/layerfs-store/src/project/family.rs) |

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
| `create` | claims the name under the home lock with `op` `sandbox.create`, starts the Sandbox on its machine (its sqlite Store's machine, else `native`), records the result and drops the `op` in one write | [`create.rs`](../../../../control/crates/layerfs-sandbox/src/create.rs) |
| `list`, `get` | Sandbox records with derived state; `get` adds one page of Workspaces | [`list.rs`](../../../../control/crates/layerfs-sandbox/src/list.rs), [`get.rs`](../../../../control/crates/layerfs-sandbox/src/get.rs) |
| `close` | records `op` `sandbox.close`, stops the runtime, removes the record; `MountsActive` without `force`; clears an unresolved create. A Sandbox recorded under the retired docker backend is only forgotten: its record is removed and the answer names its container for `docker rm <name>` (every other call on it is `Unsupported`, D-8) | [`close.rs`](../../../../control/crates/layerfs-sandbox/src/close.rs) |

Host operations run through `route::on_machine`: in this process when the
machine is local, else as a worker `sandbox.host` request carrying the
Store and project records ([`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs)). For a
PostgreSQL Store this process resolves the password and the create checks
the server is reachable from the machine first. Where this host reaches a
Sandbox's daemon (`lima`), the keys go to
`sandbox/<name>/control.json` ([`direct.rs`](../../../../control/crates/layerfs-sandbox/src/direct.rs)).

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
| `mount`, `commit`, `unmount`, `status` | the Sandbox's daemon | [`workspace/calls.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/calls.rs), [`workspace/serve.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) |
| `exec` | the Sandbox runtime runs `bash -c` in the Workspace; no daemon | [`exec/run.rs`](../../../../control/crates/layerfs-sandbox/src/exec/run.rs) |
| `exec_cancel` | the command's stop handle (its cgroup or process group) on the side that keeps its record; no daemon | [`exec/cancel.rs`](../../../../control/crates/layerfs-sandbox/src/exec/cancel.rs) |

The daemon serves no Store calls. See
[Daemon channel](../routing/daemon-channel.md).

## backup

| Call | Does | Source |
| --- | --- | --- |
| `connect` | one listing proves the bucket answers with the key, then `backup/<bucket>/record.json` (a key reference) | [`backup/family.rs`](../../../../control/crates/layerfs-store/src/backup/family.rs), [`backup/record.rs`](../../../../control/crates/layerfs-store/src/backup/record.rs) |
| `disconnect` | forgets the bucket and the Stores' links to it | [`backup/family.rs`](../../../../control/crates/layerfs-store/src/backup/family.rs) |
| `list` | every Store file on the connected buckets | [`backup/list.rs`](../../../../control/crates/layerfs-store/src/backup/list.rs) |
| `create` | the Store's machine stages a snapshot (`store.file`); this process uploads it; `Exists` if another home wrote the object | [`backup/create.rs`](../../../../control/crates/layerfs-store/src/backup/create.rs) |
| `restore` | downloads into `store/.restore-<random>/`, installs as a sqlite Store on `native`; `Conflict` if the Store changed since its last backup or restore, `InUse` while held | [`backup/restore.rs`](../../../../control/crates/layerfs-store/src/backup/restore.rs) |

Bytes go through the caller's process: machines never hold bucket keys,
and LayerFS never deletes a bucket object. See
[Backups](../backups/README.md).

## Related

- [Home and records](home-and-records.md), [Doctor](doctor.md)
- [Routing table](../routing/routing-table.md), [Workers](../routing/workers.md), [Daemon channel](../routing/daemon-channel.md)
- [Call table](../../api/call-table/README.md) and its family pages, such as [store family](../../api/sdk/store/README.md)
- [Engine: Store calls](../../engine/store-calls/README.md), [Engine: Project Init](../../engine/project-init/README.md), [Control: Sandboxes](../sandboxes/README.md)

---

← [Management: Home and records](home-and-records.md) · [Up: Management](README.md) · [Management: Doctor](doctor.md) →
