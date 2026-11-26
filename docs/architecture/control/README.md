# Control

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

**The question this layer answers:** Which machines, Stores, projects,
Sandboxes and buckets exist, where does each call run, and how does it
reach a machine, a daemon or a bucket?

Control holds the home's records and the management calls, and everything
that reaches a machine or the network on behalf of the engine. Layers are
`core < engine < control < api`: control depends on engine and core;
nothing in engine or core depends on it, and the API layer calls it.

## Picture

Each entity crate keeps records and calls; its driver does the work on a
kernel, a provider or a bucket, takes every input as an argument and
writes no record.

```text
                     layerfs-control
          worker server, inventory, doctor, migrate,
          machine/Store/project/backup calls + checks
                            |
                            v
  entity crates             |              drivers
  -------------             v              -------
              +----> layerfs-sandbox ----> layerfs-sandbox-backend
              |             |               (how a Sandbox runs:
              |             v                native, namespaces)
              |      layerfs-store ------> layerfs-bucket
              |             |               (how a Store is backed up:
              |             v                local_folder, minio, r2)
              +----> layerfs-machine ----> layerfs-platform
                            |   (feature    (where Sandboxes run:
                            v    machine)    native, docker,
                      layerfs-home           apple_container, lima)
                 (home, records, locks,
                  op, names, secrets, logs)
```

Arrows follow each crate's `[dependencies]`; edges may skip down the chain,
never up. `layerfs-machine` reaches `layerfs-platform` only through its
`machine` feature, which the guest worker's build leaves off
([`layerfs-machine/Cargo.toml`](../../../control/crates/layerfs-machine/Cargo.toml)).

## Topics

| Topic | Crates | What it covers | Pages |
| --- | --- | --- | --- |
| [Home](home/README.md) | `layerfs-home` | Opening a home, layout, record files, locks, the `op` intent, names, secrets, traces and the call log; the inventory of relationships | none |
| [Machines](machines/README.md) | `layerfs-machine`, `layerfs-platform` | Machine records and calls; one adapter per platform; the support table | [Platforms](machines/platforms.md), [Workers and reach](machines/workers-and-reach.md) |
| [Stores and projects](stores-and-projects/README.md) | `layerfs-store` (but `backup/`) | Store and project records, placement, leases, the open-Store cache, Store calls | [SQLite and PostgreSQL](stores-and-projects/sqlite-and-postgresql.md), [Projects](stores-and-projects/projects.md) |
| [Sandboxes](sandboxes/README.md) | `layerfs-sandbox`, `layerfs-sandbox-backend` | Sandbox records and lifecycle, Workspace calls, exec; the two backends; the inspect route | [Daemon channel and exec](sandboxes/daemon-channel-and-exec.md), [Backends](sandboxes/backends.md), [Inspection](sandboxes/inspect.md) |
| [Backups](backups/README.md) | `layerfs-store` `backup/`, `layerfs-bucket` | Whole-Store backup to a bucket and restore | [Buckets](backups/buckets.md) |
| [Doctor](doctor/README.md) | `layerfs-control` `doctor/` | Read-only home, host, machine, Sandbox, Store and backup checks | [Checks](doctor/checks.md) |

## Where each call runs

Every call goes through `layerfs-api`'s dispatcher, which hands it to a
control function. That function decides once, from records read under the
entity's lock, where the work runs: in the caller's process, in a
machine's worker, or in a Sandbox's daemon.

| Call | Caller's process | Machine's worker (when the Store's or Sandbox's machine is not local) | Sandbox daemon | Source |
| --- | --- | --- | --- | --- |
| `branch.*`, `commit.*`, `file.*` on a PostgreSQL Store | all of it: URL from the record, password resolved now, under the Store's lease | — | — | [`call.rs`](../../../control/crates/layerfs-store/src/call.rs) |
| the same on a sqlite Store, machine local | all of it, on `store/<s>/store.sqlite` | — | — | [`place.rs`](../../../control/crates/layerfs-store/src/place.rs) |
| the same on a sqlite Store, other machine | holds the lease, sends the record | `store.call`: opens `<root>/store/<s>/store.sqlite`, holds the lease there | — | `place.rs`, [`worker_server.rs`](../../../control/crates/layerfs-control/src/worker_server.rs) |
| `project.init` | checks; walks and copies the folder | `project.stage` (parts), then `project.init` | — | [`project/family.rs`](../../../control/crates/layerfs-store/src/project/family.rs), [`project/send.rs`](../../../control/crates/layerfs-store/src/project/send.rs) |
| `project.export` | destination checks, strict local writer, atomic rename | `project.export` streams pinned tree records | — | [`project/export/route.rs`](../../../control/crates/layerfs-store/src/project/export/route.rs) |
| `project.delete` | record, durable intent and lock work | `project.delete.admit`, `project.delete`, `project.delete.entry`, `project.delete.release` | — | [`project/delete.rs`](../../../control/crates/layerfs-store/src/project/delete.rs), `project/family.rs` |
| `store.move` | phases, records, locks | `store.file` steps on the source and target | — | [`sqlite/transfer/phases.rs`](../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) |
| `store.remove` | records, locks | `store.file` `remove` | — | [`sqlite/remove.rs`](../../../control/crates/layerfs-store/src/sqlite/remove.rs) |
| `store.list`, `store.get` | records only; `--with live`: leases and `store.file` `stat`, compaction record and heads, when reached; `--with stats` opens the Store; `--with format` reads the format from the file's header or one `SELECT` | `store.file` `stat`, when reached; `store.format` for `--with format` | — | [`get.rs`](../../../control/crates/layerfs-store/src/get.rs), [`live.rs`](../../../control/crates/layerfs-store/src/live.rs), [`list.rs`](../../../control/crates/layerfs-store/src/list.rs) |
| `store.compact` | the Store's lease; PostgreSQL dials its URL, a sqlite file here rebuilds in this process | `store.compact`: a sqlite file on that machine rebuilds there (spec S6, #42 D5) | — | [`compact.rs`](../../../control/crates/layerfs-store/src/compact.rs) |
| `store.upgrade` | the exclusive `op` and the Store's lease; PostgreSQL dials its URL, a sqlite file here upgrades in this process | `store.upgrade` (and `store.cancel` on a second connection): a sqlite file on that machine upgrades there (#804) | — | [`upgrade.rs`](../../../control/crates/layerfs-store/src/upgrade.rs) |
| `store.attach` | records; the Store's address and login are dialled here for a locally executing machine | `store.reach`: the machine's check | — | [`layerfs-control/src/store.rs`](../../../control/crates/layerfs-control/src/store.rs) |
| `backup.create` | uploads to the bucket | `store.file` `stage`, `read`, `unstage` | — | [`backup/create.rs`](../../../control/crates/layerfs-store/src/backup/create.rs) |
| `machine.*`, `store.create`, `store.connect`, `store.disconnect`, `store.detach`, `backup.connect`, `backup.disconnect`, `backup.list`, `backup.restore` | all of it | — | — | [Machines](machines/README.md), [Backups](backups/README.md) |
| `sandbox.create`, `sandbox.remove` | records, intent | `sandbox.host` with the internal host ops `sandbox.create` and `sandbox.close` (the worker runs the same host code on its `native`) | started, stopped | [`route.rs`](../../../control/crates/layerfs-sandbox/src/route.rs), [`remove.rs`](../../../control/crates/layerfs-sandbox/src/remove.rs), [`host/call.rs`](../../../control/crates/layerfs-sandbox/src/host/call.rs) |
| `workspace.mount`, `commit`, `unmount`, `status` on a forwarded Sandbox | dials the daemon directly | — | serves it | [`direct.rs`](../../../control/crates/layerfs-sandbox/src/direct.rs) |
| the same, not forwarded | — (local machine: dials the daemon) | `sandbox.host`; the worker dials the daemon | serves it | `route.rs` |
| `workspace.exec` | checks the directory; keeps the command's record when forwarded (route `machine:<name>`) | runs `bash -c` through the Sandbox runtime (always the machine) | — | [`workspace/exec.rs`](../../../control/crates/layerfs-sandbox/src/workspace/exec.rs) |
| `workspace.cancel` (`--exec E` for one command) | where the record is: here when forwarded (then rides `sandbox.host`) | else here; an unreached worker answers `{outcome: "unknown"}`, nothing changed | — | `workspace/exec.rs`, [`exec/cancel.rs`](../../../control/crates/layerfs-sandbox/src/exec/cancel.rs) |

| Platform | sqlite Store calls on macOS / Linux / Windows | Workspace calls skip the worker? | Source |
| --- | --- | --- | --- |
| `native` | caller / caller / worker | yes (the daemon is on this host) | [`native/profile.rs`](../../../control/crates/layerfs-platform/src/platforms/native/profile.rs) |
| `docker` | worker / worker / — | no: ports publish only at container start | [`docker/adapter.rs`](../../../control/crates/layerfs-platform/src/platforms/docker/adapter.rs) |
| `apple-container` | worker / — / — | no: ports publish only at create | [`apple_container/adapter.rs`](../../../control/crates/layerfs-platform/src/platforms/apple_container/adapter.rs) |
| `lima` | worker / — / — | yes: an `ssh -L` forward | [`lima/adapter.rs`](../../../control/crates/layerfs-platform/src/platforms/lima/adapter.rs) |

The machine whose calls run in this process
([`layerfs_machine::in_process`](../../../control/crates/layerfs-machine/src/local.rs):
the built-in machine whose platform's executor is `Local` here, `native` on
macOS and Linux, none on Windows) is where a sqlite Store goes when
`store create` names no machine, one end of every `store move`, and the
machine a `backup restore` installs on; on a host with none those refuse
`Unsupported`. This computer is never a machine: see
[Platforms](machines/platforms.md#native-platformsnative).

`plan` reads no worker record and dials nothing; the first worker call
dials `machine/<m>/worker.json` and a worker not recorded or not answering
is `Unavailable`, naming `layerfs machine start`. A build without the
`machine` feature (the guest worker's) has no adapters: there `native` is
local and every other machine a worker. The daemon serves no Store calls:
a mount's caller reads no Store (the daemon finds the Branch by name), and
a status's head read takes the Store's own route
([`place.rs`](../../../control/crates/layerfs-store/src/place.rs),
[`direct.rs`](../../../control/crates/layerfs-sandbox/src/direct.rs)).
Store calls in a process pass the engine's kernel guard
([Stores and projects](stores-and-projects/README.md#invariants)). The
worker and daemon wire records are in [Bridge](../engine/bridge/README.md)
and [Workers and reach](machines/workers-and-reach.md).

## Extension points

Each is one crate-private sealed trait with no default methods, reached
through one exhaustive `match`; there is no public provider trait,
registry or factory.

| To add | Trait and match | Touch | Source |
| --- | --- | --- | --- |
| a platform | `Adapter`, `adapter()`; facts in `Platform::profile()` | `platforms/<p>/` (`profile.rs`, `adapter.rs`), one `Platform` variant, its `support.rs` rows, conformance arms | [`adapter.rs`](../../../control/crates/layerfs-platform/src/adapter.rs), [Platforms](machines/platforms.md#adding-a-platform) |
| a Sandbox backend | `BackendAdapter`, `adapter()` | `backend/<b>/`, one `Backend` variant, one arm, one record type | [`backend/adapter.rs`](../../../control/crates/layerfs-sandbox-backend/src/backend/adapter.rs), [Backends](sandboxes/backends.md) |
| a bucket provider | `Driver` (sealed by a private `Sealed` supertrait) | `bucket/<p>/`, one `Provider` variant in [`backup/link.rs`](../../../control/crates/layerfs-bucket/src/backup/link.rs) | [`bucket/driver.rs`](../../../control/crates/layerfs-bucket/src/bucket/driver.rs), [Buckets](backups/buckets.md) |

Every refusal of a platform, host and Store kind comes from the one support
table, and `control/SUPPORT.json` is generated from it
([Platforms](machines/platforms.md#support-table-supportrs)). A new Store
kind or kernel route is an engine change.

## Rules

| Rule | Enforced by |
| --- | --- |
| sqlite Store calls run in a process on the Store's kernel (the caller on a local `native`, else that machine's worker); PostgreSQL Store calls and bucket I/O run in the caller's process; a sqlite Store is on exactly one machine; a PostgreSQL Store is on no machine: its Sandboxes run on every machine whose platform runs them | [`place.rs`](../../../control/crates/layerfs-store/src/place.rs), [`support.rs`](../../../control/crates/layerfs-platform/src/support.rs) |
| Workers listen on loopback behind a secret; one attempt per call, no retry, no replay; an uncertain outcome is reported, never resent | [`worker/client.rs`](../../../control/crates/layerfs-machine/src/worker/client.rs) |
| A PostgreSQL password is resolved only in the caller, travels only in one worker request and the Sandbox's private daemon setup file, and is never written to a record, log, trace or `Debug` output | [`secrets.rs`](../../../control/crates/layerfs-home/src/secrets.rs), [SQLite and PostgreSQL](stores-and-projects/sqlite-and-postgresql.md#credentials) |
| Drivers know nothing of the home and write no records | [`layerfs-platform/src/lib.rs`](../../../control/crates/layerfs-platform/src/lib.rs) |
| Stored and displayed values do not follow internal renames: the `Native` backend is stored and shown as `"native"` | [`native/adapter.rs`](../../../control/crates/layerfs-sandbox-backend/src/backend/native/adapter.rs) |
| sqlite Stores open with the Normal profile (WAL, synchronous=NORMAL), the only SQLite profile; their records carry the label `normal`; Durable execution is `NOT_RUN — disabled by owner until explicit reauthorization` | [`record.rs`](../../../control/crates/layerfs-store/src/record.rs) |

The binding code rules for this layer are in the
[control guide](../../../control/AGENTS.md).

## Native host worker bootstrap

A machine-enabled CLI on macOS or Linux can start the native process through its
internal `worker --host` entry. The native endpoint uses the real home's ID
and preserves its directory permissions. The process holds its lifetime lock
from endpoint setup through socket removal. It accepts ordinary and management
connections on separate Unix sockets, and checks the peer UID before exchanging
frames. Its bootstrap stays independent of guest root setup.

The accept loop lets the kernel hold ordinary connections once all 16
ordinary slots are occupied. Eight ordinary threads and two independently
reserved management threads serve the admitted connections. Admission uses a
fixed queue and a capped frame with an absolute deadline. The management
socket reports worker identity and can request a drain even when every
ordinary thread is occupied. The drain fences new operations, closes both
listeners, and joins admitted connections before releasing the lifetime lock.
The idle drain starts after 60 seconds without an admitted call, checked every
five seconds. Connections that never submit an authenticated operation leave
the idle deadline unchanged.

The CLI can route bounded local SQLite `file.read`, `file.stat`, and
`file.list` calls to the native worker after argument and scope checks.
The envelope contains the Store and project names and the allowlisted
read arguments. The worker resolves the binding and checks the current
Store record under its admission lock. An endpoint path or request that
does not fit uses the direct route, chosen before dialing. An error reading
the worker's home identity is a definite refusal before any send. If operation
bytes may have been sent and no valid reply arrives, the caller reports
`Unknown` with a request ID. The caller owns the single public audit line.
SDK calls, PostgreSQL, writes, and guest Stores use their existing routes.
See the [host exchange](machines/workers-and-reach.md#host-worker-exchange-20-implementation-plan).
