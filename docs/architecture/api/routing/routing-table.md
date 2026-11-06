# Routing table

[Architecture](../../README.md) › [API](../README.md) › [Routing and workers](README.md) › Routing table

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Where each call runs.

| | |
| --- | --- |
| Component | [Routing and workers](README.md) |
| Source | [`api/crates/layerfs-control/src/calls`](../../../../api/crates/layerfs-control/src/calls), [`api/crates/layerfs-control/src/store/resolve.rs`](../../../../api/crates/layerfs-control/src/store/resolve.rs), [`api/crates/layerfs-control/src/sandbox/route.rs`](../../../../api/crates/layerfs-control/src/sandbox/route.rs), [`common/place.rs`](../../../../api/crates/layerfs-control/src/common/place.rs), [`store/call.rs`](../../../../api/crates/layerfs-control/src/store/call.rs), [`workspace/calls.rs`](../../../../api/crates/layerfs-control/src/workspace/calls.rs) |

## By Store kind

```text
 branch/commit/file call (project "<store>/<project>")
      |
      v
 resolve::target    reads store/<store>/record.json and the project record
      |             (NoSuchStore, NoSuchProject)
      v
 store_call: record.unblocked(); lease::claim under the Store's lock
      |
      +-- StoreRef::Postgres --> this process: URL from the record,
      |                          password reference resolved now,
      |                          store::open::run(op, ..., lease)
      |
      +-- StoreRef::Sqlite --> place::plan --> on_machine(store.machine)
                                 |                   |
                          Executor::Local     Executor::Worker
                                 |                   :
             store::open::run with          worker op "store.call"
             storePath = store/<name>/      {op, args + store_record};
             store.sqlite                   worker opens the Store under
                                            its own root (/layerfs/data)
```

The decision is made once per call from the Store's record, read under its
lock ([`store/call.rs`](../../../../api/crates/layerfs-control/src/store/call.rs)).
`plan` reads no worker record and dials nothing; the first worker call dials
the address in `machine/<name>/worker.json`
([`place.rs`](../../../../api/crates/layerfs-control/src/common/place.rs)).

| Store kind | Machine | Runs in | Source |
| --- | --- | --- | --- |
| PostgreSQL | any (none recorded) | the caller's process; the Store record holds the URL and a password reference, resolved per call | `place::plan`, `store/call.rs` `postgresql` |
| SQLite | a machine whose platform's executor is `Local` on this host | the caller's process, on `store/<name>/store.sqlite` | `place::plan`, `place::local` |
| SQLite | any other machine | that machine's worker; the worker holds the Store's lease under its root | `place::Worker::call`, [`worker/server.rs`](../../../../api/crates/layerfs-control/src/machine/worker/server.rs) `store.call` |

The same open-Store cache ([`store/open.rs`](../../../../api/crates/layerfs-control/src/store/open.rs))
serves both sides: a Store is opened once per process and closed after 60 s
unused (`IDLE`), never while a call runs on it.

### Other families

| Call | Caller's process does | A worker does (when the Store's machine is not local) | Source |
| --- | --- | --- | --- |
| `project.init` | checks, copies the folder to the machine | `project.stage` (each part), then `project.init` | [`project/family.rs`](../../../../api/crates/layerfs-control/src/project/family.rs), [`project/send.rs`](../../../../api/crates/layerfs-control/src/project/send.rs) |
| `project.remove` | record and lock work | `project.remove` | `project/family.rs` |
| `store.push`, `store.pull` | phases, records, locks | `store.file` steps on the source and target machine | [`store/transfer/phases.rs`](../../../../api/crates/layerfs-control/src/store/transfer/phases.rs) |
| `store.remove` | records and locks | `store.file` step `remove` | [`store/remove.rs`](../../../../api/crates/layerfs-control/src/store/remove.rs) |
| `store.status`, `store.list` | records | `store.file` stat, when reached | [`store/status.rs`](../../../../api/crates/layerfs-control/src/store/status.rs), [`store/list.rs`](../../../../api/crates/layerfs-control/src/store/list.rs) |
| `backup.create` | upload to the bucket | `store.file` (the Store's snapshot is sent back) | [`backup/create.rs`](../../../../api/crates/layerfs-control/src/backup/create.rs) |
| `machine.*`, `store.create`, `store.connect`, `backup.connect`, `backup.list`, `backup.restore` | all of it | nothing | [Families](../control/families.md) |

## By platform

`place::local` asks the machine's platform for its executor on this host
([`platform.rs`](../../../../connect/crates/layerfs-machine/src/platform.rs),
[`platforms/*/profile.rs`](../../../../connect/crates/layerfs-machine/src/platforms)).

| Platform | macOS host | Linux host | Windows host | Worker binds |
| --- | --- | --- | --- | --- |
| `native` | in process | in process | worker | every interface |
| `docker` | worker | worker | worker | every interface |
| `apple-container` | worker | worker | worker | every interface |
| `microsandbox` | worker | worker | worker | every interface |
| `firecracker` | worker | worker | worker | every interface |
| `lima` | worker | worker | worker | loopback |
| `ssh` | worker | worker | worker | loopback |

Whether a platform can exist on a host at all is the
[support matrix](../../connect/support-matrix/README.md)'s answer, not this
table's. A build without the `machine` feature (the guest worker's) has no
platform adapters: there `native` is local and every other machine is a
worker (`place::local`, `#[cfg(not(feature = "machine"))]`). The worker's
secret guards it on every bind address.

## Workspace calls

```text
 workspace.<call>(ws = "<sandbox>:<path or name>")
      |
      v
 sandbox_of(sandbox)  -> NoSuchSandbox
 record.free()        -> Busy / Unresolved   (not for status)
      |
      +-- daemon forwarded to this host (record.daemon.forward)?
      |      yes, call != exec --> direct::call: this process dials
      |                            the daemon (pool), under the
      |                            Store's lease
      |      yes, exec ----------> directory checked here, then
      |                            on_machine (runtime is in machine)
      |
      +-- no --> sandbox::route::on_machine / reach
                   |                       |
            Executor::Local         Executor::Worker
            sandbox::dispatch       worker op "sandbox.host"
            (host Sandbox code      (the worker runs the same
             dials the daemon)       host code on its native)
```

| Call | Route | Starts nothing? | Source |
| --- | --- | --- | --- |
| `workspace.mount` | direct when forwarded, else the machine | yes; a mount never starts a Sandbox | [`workspace/calls.rs`](../../../../api/crates/layerfs-control/src/workspace/calls.rs), [`workspace/serve.rs`](../../../../api/crates/layerfs-control/src/workspace/serve.rs) |
| `workspace.exec` | always the machine (`bash -c` through the Sandbox runtime) | yes | `workspace/calls.rs`, [`sandbox/direct.rs`](../../../../api/crates/layerfs-control/src/sandbox/direct.rs) |
| `workspace.commit`, `workspace.unmount` | direct when forwarded, else the machine | yes | `workspace/calls.rs` |
| `workspace.status` | direct when forwarded, else `reach` without loading an image; an unreached worker is the refusal | yes | `workspace/calls.rs` (`reach(..., false)`) |

A Sandbox is forwarded when its record's `daemon.forward` holds an address:
on `ssh` and `lima` an `ssh -L` carries the daemon's port to this host, and a
`docker` Sandbox publishes its daemon's port on the host's loopback
([`sandbox/direct.rs`](../../../../api/crates/layerfs-control/src/sandbox/direct.rs)).
The daemon itself serves no Store calls: a Branch lookup or head read for a
mount or status goes the Store's own route above; for a PostgreSQL Store,
on the caller's held read session (one light-checked connection, no Store
service, #64). How a call reaches the
daemon is in [Daemon channel](daemon-channel.md).

## Related

- [Workers](workers.md): the worker protocol used by every `Executor::Worker` route.
- [Daemon channel](daemon-channel.md): the control connections a Workspace call uses.
- [Control: Families](../control/families.md): what each family does before it routes.
- [Connect: Network](../../connect/network/README.md): forwards, reach and worker setup.
- [Connect: Machines](../../connect/machines/README.md): platforms and their profiles.
- [Engine: Store calls](../../engine/store-calls/README.md): the Store methods `store::open::run` runs.

---

← [API: Routing and workers](README.md) · [Up: Routing and workers](README.md) · [Routing and workers: Workers](workers.md) →
