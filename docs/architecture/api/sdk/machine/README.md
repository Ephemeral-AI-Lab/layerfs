# machine family

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › machine

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Firecracker and SSH are **permanently disabled for new work**. Their code
and records remain for inspection and explicit cleanup. See the
[platform restriction](../../../control/support-matrix/disabled-platforms.md) for refusals, existing-resource handling and
the source change required for re-enabling. Descriptions of their adapter
mechanics below are retained implementation details, not selectable features.

The `machine` calls.

| | |
| --- | --- |
| Component | [Call table](../../call-table/README.md) |
| Source | [`api/crates/layerfs-api/src/machine`](../../../../../api/crates/layerfs-api/src/machine), [`control/crates/layerfs-control/src/machine`](../../../../../control/crates/layerfs-machine/src) |

## Calls

```text
 reads (start nothing):  list   platforms   get   status

 built in: native, docker ---------------------- update (image, docker,
                                                  wsl, daemon)
 platforms --> create <name> <vm platform> --+
               connect <name> <ssh host> ----+--> running
                                             |     |      ^
                                             |   stop     |
                                             |     v      |
                                             |  stopped --+ start
                                             v
               remove (VM + disk)   |   disconnect (ssh; data root stays)
```

Eleven calls, in help order
([`machine/mod.rs`](../../../../../api/crates/layerfs-api/src/machine/mod.rs)).
A machine is where Sandboxes and sqlite Stores live: the built-ins `native`
and `docker`, a VM made by `create`, or an ssh host added by `connect`.
Each entry runs `layerfs_control::machine::<call>`. "Summary" is the
machine as `machine.get` shows it: `{ name, platform, backend, supports,
state, cpus, memory, disk, stores, sandboxes, … }`
([`record.rs`](../../../../../control/crates/layerfs-machine/src/record.rs)).

| Call | Arguments | Options | Returns |
| --- | --- | --- | --- |
| [`list`](list.md) | — | — | `{ machines: [summary] }`, live state probed in parallel |
| [`platforms`](platforms.md) | — | — | `{ host, platforms: [{ platform, how, backend, kernel, sqlite, postgresql, ready, missing, version, options, description }] }` |
| [`get`](get.md) | `name` (Name) | — | summary |
| [`status`](status.md) | `name` | — | `{ name, platform, state, free_disk, tool_version, down_reason, stores, … , op? }` |
| [`start`](start.md) | `name` | — | summary, once the worker answers its secret |
| [`stop`](stop.md) | `name` | — | summary with `state: "stopped"` |
| [`create`](create.md) | `name`, `platform` (Name) | `cpus` (Int), `memory`, `disk` (Size), `image` (Text), `nested` (Bool), `host` (Name) | summary |
| [`connect`](connect.md) | `name` (Name), `host` (Text) | `user` (Text), `port` (Int), `identity`, `known_hosts`, `root` (Text) | summary |
| [`remove`](remove.md) | `name` | — | `{ name, removed: true }` |
| [`disconnect`](disconnect.md) | `name` | — | `{ name, removed: true }`; the host's data root stays |
| [`update`](update.md) | `name` | `cpus` (Int), `memory` (Size), `image`, `docker`, `wsl`, `daemon` (Text) | summary |

Sources of the answers:
[`list.rs`](../../../../../control/crates/layerfs-machine/src/list.rs),
[`platforms.rs`](../../../../../control/crates/layerfs-machine/src/platforms.rs),
[`status.rs`](../../../../../control/crates/layerfs-machine/src/status.rs),
[`power.rs`](../../../../../control/crates/layerfs-machine/src/power.rs),
[`create.rs`](../../../../../control/crates/layerfs-machine/src/create.rs),
[`remove.rs`](../../../../../control/crates/layerfs-machine/src/remove.rs),
[`update.rs`](../../../../../control/crates/layerfs-machine/src/update.rs).

## Arguments and options

| Name | Kind | Rule |
| --- | --- | --- |
| `memory`, `disk` | Size | bytes as a number, or text with `K`, `M`, `G` (powers of 1024), turned into bytes by `check::size` ([`calls/check.rs`](../../../../../api/crates/layerfs-api/src/calls/check.rs)) |
| `platform` | Name | a platform of the support table; `machine.platforms` lists what this host and build support |
| `image`, `docker`, `wsl`, `daemon` | Text | `update` records them on the machine; `wsl` is a Windows host's setting; `daemon` is an absolute path |
| `cpus`, `memory` on `update` | Int, Size | resizing a VM; refused while Sandboxes run on it, and "not built yet" for some platforms |

The machine calls take no paging. `list`, `get`, `status` and `platforms`
start nothing (P4); `list` and `get` wait for their probes together and
show a probe that did not answer as `unknown`.

## Errors

| Code | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad name or option for this platform | [`create.rs`](../../../../../control/crates/layerfs-machine/src/create.rs), [`update.rs`](../../../../../control/crates/layerfs-machine/src/update.rs) |
| `NotFound` | no machine of that name | [`record.rs`](../../../../../control/crates/layerfs-machine/src/record.rs) |
| `Exists` | the name, VM, disk or host is taken | [`create.rs`](../../../../../control/crates/layerfs-machine/src/create.rs), [`connect.rs`](../../../../../control/crates/layerfs-machine/src/connect.rs) |
| `InUse` | `stop`, `remove` or a resize while Sandboxes run (`detail.related`) | [`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs) |
| `Unsupported` | a platform this host or build lacks (`detail.missing`); every machine call in a build without the `machine` feature | [`spec.rs`](../../../../../control/crates/layerfs-machine/src/spec.rs), [`api/…/machine/create.rs`](../../../../../api/crates/layerfs-api/src/machine/create.rs) |
| `Unavailable`, `Denied` | a provider or guest that does not answer; an ssh host that refuses the login or host key | [`layerfs-platform` `error.rs`](../../../../../control/crates/layerfs-platform/src/error.rs) |
| `Busy`, `Unresolved` | another operation's `op` on the machine | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs) |
| `Failed` | it ran once and failed; a create that failed after its VM existed keeps it as `failed` | [`create.rs`](../../../../../control/crates/layerfs-machine/src/create.rs) |
| `Unknown` | **uncertain:** the adapter timed out or lost its answer; everything made is kept, the `op` stays `uncertain` and is logged | [`create.rs`](../../../../../control/crates/layerfs-machine/src/create.rs), [`remove.rs`](../../../../../control/crates/layerfs-machine/src/remove.rs) |

An unresolved machine is shown by `machine status` and cleared by
`machine remove` or `disconnect`. See
[error kinds](../../call-table/errors.md#error-kinds) and
[uncertain outcomes](../../call-table/errors.md#uncertain-outcomes).

## Where it runs

```text
 machine.<call>  caller's process (host build, `machine` feature)
   |  home records under the home lock (claim, record only)
   v
 layerfs-platform adapter: probe, create, start, stop, remove a VM or host
   |
   +-- start: under start.lock, start the VM and its `layerfs worker`
   |          from the sealed Linux bundle, record it in worker.json,
   |          wait until it answers its secret (readiness, not a retry)
   +-- stop:  end the worker and VM; worker.json goes, disks stay
```

All machine calls run in the caller's process; none is sent to a worker.
The guest worker is built without the `machine` feature, so no machine
adapter runs inside a machine
([`layerfs-api/Cargo.toml`](../../../../../api/crates/layerfs-api/Cargo.toml)).
Nothing but `machine start` starts a machine (#535 D11): other calls on a
stopped machine are refused `Unavailable`
([`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs)).
See the [routing table](../../../control/routing/routing-table.md) and
[workers](../../../control/routing/workers.md).

## Example

Not run; written from the call table.

```js
await layerfs.machine.create("vm1", "lima", { cpus: 4, memory: "8G", disk: "64G" });
await layerfs.machine.start("vm1");
```

```python
layerfs.machine.create("vm1", "lima", cpus=4, memory="8G", disk="64G")
layerfs.machine.start("vm1")
```

```console
$ layerfs machine create vm1 lima --cpus 4 --memory 8G --disk 64G
$ layerfs machine start vm1
```

## Related

- [Control: Families](../../../control/management/families.md) and [Workers](../../../control/routing/workers.md).
- [Control: Machines](../../../control/machines/README.md) and [Control: Support matrix](../../../control/support-matrix/README.md).
- [store family](../store/README.md): `store push` and `pull` move sqlite Stores between machines.

---

← [workspace.status](../workspace/status.md) · [Up: SDK](../README.md) · [machine.list](list.md) →
