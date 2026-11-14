# machine family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `machine` calls: list, inspect, create, start, stop, update and remove the machines that hold sqlite Stores and run Sandboxes, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/machine`](../../../../api/crates/layerfs-api/src/machine), [`layerfs-control/src/machine.rs`](../../../../control/crates/layerfs-control/src/machine.rs), [`layerfs-machine/src`](../../../../control/crates/layerfs-machine/src), [`layerfs-platform/src`](../../../../control/crates/layerfs-platform/src) |
| Runs by default | every call in the caller's process, through the platform adapter (provider CLI or Docker); none is sent to a worker. The guest worker is built without the `machine` feature, so there every machine call is `Unsupported` before any effect |
| Design | [Machines](../../control/machines/README.md), [platforms](../../control/machines/platforms.md), [workers and reach](../../control/machines/workers-and-reach.md) |

## Calls

```text
 reads (start nothing):  list   platforms   get   status

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
| [`machine.status`](#machinestatus) | `{ name, platform, state, free_disk, tool_version, down_reason, stores, op? }` |
| [`machine.start`](#machinestart) | summary, once the worker answers |
| [`machine.stop`](#machinestop) | summary, `state: "stopped"`; with `force`, also `forced`, `stopped_sandboxes`, `unknown` |
| [`machine.create`](#machinecreate) | summary, `state: "running"` |
| [`machine.remove`](#machineremove) | `{ name, removed: true, note? }` |
| [`machine.update`](#machineupdate) | summary |

The **summary** is `MachineRecord::to_sdk`
([`record.rs`](../../../../control/crates/layerfs-machine/src/record.rs)):

```text
{ name, platform, backend, supports: { sqlite, postgresql_sandbox, reason },
  state, cpus, memory, disk, stores: [name], postgresql: [name],
  sandboxes: [name], image?, docker?, wsl?, daemon?, os_image?, error?,
  ssh?, host?, vm_name? }
```

| Field | Meaning |
| --- | --- |
| `platform` | `native`, `docker`, `apple-container` or `lima`; an older home's record may still name the removed `firecracker`, `microsandbox` or `ssh` |
| `backend` | the Sandbox backend on this host: `native`, `namespaces` (also on `docker`), or `-` (always for a removed platform) |
| `supports` | the platform's support row on this host; a removed platform supports nothing, with the removal as `reason` |
| `cpus`, `memory`, `disk` | the recorded size (bytes for memory and disk); null on a built-in machine |
| `stores`, `postgresql`, `sandboxes` | sqlite Stores on it, PostgreSQL Stores attached to it, Sandboxes recorded on it ([`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs)) |
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

The support table for this host, with a live tool probe per platform.

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
                 options: [ { name, kind } ], description } ] }
```

| Field | Meaning | Source |
| --- | --- | --- |
| `how` | `built_in` (`native`, `docker`) or `created` | [`platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs) |
| `backend`, `kernel` | `native` or `namespaces`; `fuse` or `nfs`; null when not on this host | [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs) |
| `sqlite` | where a sqlite Store's calls run: `in_process` or `worker` | same |
| `postgresql` | `store_calls` (always `in_process`), `sandbox` (bool), `reason` | same |
| `ready`, `missing`, `version` | the live probe: `tool:<cli>` not on the path, `host:<fact>` lacking; each command bounded at 10 s | [`probe.rs`](../../../../control/crates/layerfs-platform/src/probe.rs) |
| `options` | what `create` takes for this platform | [`options.rs`](../../../../control/crates/layerfs-platform/src/options.rs) |

The rows without the live fields are
[`control/SUPPORT.json`](../../../../control/SUPPORT.json), generated from
`support.rs`; the per-host matrix is on [platforms](../../control/machines/platforms.md).
A missing tool is `ready: false`, not an error.

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
| `os_image` option | Text | the VM's operating-system image (an installed OCI reference for Apple Container, an image location for Lima); default an installed prepared image, else the provider's stock image; recorded and answered as `os_image` |
| `on_progress` option | Function | the `machine.start` stages, plus `{ phase: "create", machine }` |

The record keeps `vm_name` (`lfs-<home id prefix>-<name>`, so two homes
never share a VM). The name is claimed with an `op` `create` under the home
lock; the boot runs under `start.lock` only.

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

| Error | When |
| --- | --- |
| `Busy` | a live process's create or remove holds it (a gone process's uncertain intent does not refuse) |
| `InUse` | sqlite Stores, attached PostgreSQL Stores, Sandboxes or old Firecracker records it hosted are on it (`detail.related`, `can_remove_machine`) |
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
| `docker` | built-in only | the Docker socket this home's tools use |
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
