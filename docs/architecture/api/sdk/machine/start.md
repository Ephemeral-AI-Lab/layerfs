# machine.start

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [machine](README.md) › start

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Firecracker and SSH are **permanently disabled for new work**. Their code
and records remain for inspection and explicit cleanup. See the
[platform restriction](../../../control/support-matrix/disabled-platforms.md) for refusals, existing-resource handling and
the source change required for re-enabling. Descriptions of their adapter
mechanics below are retained implementation details, not selectable features.

`machine.start` starts a stopped machine and its `layerfs worker`, waits
until the worker answers its secret, and answers the machine's summary. A
machine whose worker already answers is answered at once. It is the only
call that starts a machine (#535 D11).

| | |
| --- | --- |
| Family | [machine](README.md) |
| Library entry | [`machine/start.rs`](../../../../../api/crates/layerfs-api/src/machine/start.rs) |
| Implemented in | [`layerfs_control::machine::start`](../../../../../control/crates/layerfs-control/src/machine/power.rs), through [`endpoint::running`](../../../../../control/crates/layerfs-control/src/machine/endpoint.rs) |
| Runs in | the caller's process (host build with the `machine` feature); it starts the worker inside the machine |

## Signature

```js
// JavaScript
await layerfs.machine.start(name);
```

```python
# Python
layerfs.machine.start(name)
```

```bash
# CLI
layerfs machine start <name>
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | an existing machine not held by an `op` | [`machine/spec.rs`](../../../../../control/crates/layerfs-control/src/machine/spec.rs) `machine_of` |

## Options

None.

## Returns

The machine's summary, as [`machine.get`](get.md) answers it after a live
probe; `state` is normally `running` (`ready` for `native`).

```text
{ name, platform, backend, supports, state, cpus, memory, disk,
  stores, sandboxes, ... }
```

## Where it runs

```text
caller's process                                 machine
 machine_of(name)        Busy | Unresolved | NotFound
 native? --> answer get(name)   (nothing to start)
 worker.json's worker answers hello? --> answer get(name)
 firecracker? --> start its lima host first (same steps)
 take machine/<name>/start.lock  (wait <= 600 s)
 worker answers now? (another process started it) --> get
 spec + sealed Linux bundle
 docker with no image? --> load bundle's sandbox-image.tar once
 layerfs-platform start(spec, bundle) ------------> boot VM / container
                                                   copy bundle once per
                                                   hash, image archive,
                                                   start `layerfs worker`
           readiness wait (not a retry) <--------- worker answers secret
 home lock: write worker.json, lasting, state "running", drop op
 hello: worker of another build? --> Unavailable (worker stays recorded)
 answer get(name)
```

`start.lock` is taken before the home lock and is shared with `create`,
`stop` and `remove`, so a start never runs beside them on the same machine
([`machine/endpoint.rs`](../../../../../control/crates/layerfs-control/src/machine/endpoint.rs)).
The home lock is held only to write the records, never while the VM boots.

| Platform | What start does | Source |
| --- | --- | --- |
| `native` | nothing; answers at once | [`machine/power.rs`](../../../../../control/crates/layerfs-control/src/machine/power.rs) |
| `docker` | runs the worker container (`--pull never`) from the recorded image, else the bundle's image loaded once | [`machine/image.rs`](../../../../../control/crates/layerfs-control/src/machine/image.rs), [`platforms/docker/power.rs`](../../../../../control/crates/layerfs-platform/src/platforms/docker/power.rs) |
| `lima`, `apple-container`, `microsandbox` | boots the VM if stopped, then sets up and starts the worker | [`platforms/`](../../../../../control/crates/layerfs-platform/src/platforms) |
| `firecracker` | starts its Lima host first, then the microVM | `endpoint.rs` `host_first` |
| `ssh` | starts the worker and its `ssh -L` forward | [`platforms/ssh/power.rs`](../../../../../control/crates/layerfs-platform/src/platforms/ssh/power.rs) |

Every other call on a stopped machine is refused `Unavailable`, naming
`layerfs machine start` ([`common/place.rs`](../../../../../control/crates/layerfs-control/src/common/place.rs)).
See [workers](../../../control/routing/workers.md#lifecycle) and the
[routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput`, `NotFound` | bad name; no such machine | no | [`machine/record.rs`](../../../../../control/crates/layerfs-control/src/machine/record.rs) `of` |
| `Busy`, `Unresolved` | a create, connect, remove or disconnect holds it (or its firecracker host) | no | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs) `blocks` |
| `Unavailable` | `start.lock` held by another process for 600 s; nothing changed | no | [`home/entity_lock.rs`](../../../../../control/crates/layerfs-home/src/home/entity_lock.rs) |
| `Unavailable` | the started worker is of another build: the bundle is older than this build (`detail.worker`, `expected`, `bundle`) | no: the machine runs and is recorded | [`machine/worker/hello.rs`](../../../../../control/crates/layerfs-control/src/machine/worker/hello.rs) `stale_bundle` |
| `Unsupported` | no sealed Linux bundle (`detail.missing: ["bundle:<dir>"]`); docker with no image and no `sandbox-image.tar`; a build without the `machine` feature | no | [`machine/bundle.rs`](../../../../../control/crates/layerfs-control/src/machine/bundle.rs), `image.rs` |
| `NotFound` | the provider holds no VM of that name | no | [`setup/vm.rs`](../../../../../control/crates/layerfs-platform/src/setup/vm.rs) |
| `Unavailable`, `Denied`, `Failed` | the provider or guest did not answer; ssh refused the login or key; a step failed once | no | [`error.rs`](../../../../../control/crates/layerfs-platform/src/error.rs) |
| `Unknown` | **a provider command timed out or its answer was lost: the VM or worker may be up with no `worker.json` written.** Read `machine status` first; a new `machine start` is a new call, not a replay | **yes** | [`setup/tool.rs`](../../../../../control/crates/layerfs-platform/src/setup/tool.rs) |

## Examples

Not run; written from the call table.

```js
await layerfs.machine.start("vm1");
```

```python
layerfs.machine.start("vm1")
```

```console
$ layerfs machine start docker
```

## Related

- Before: [`machine.create`](create.md), [`machine.connect`](connect.md) or
  [`machine.stop`](stop.md). After: Store, project and Sandbox calls on the
  machine; [`machine.stop`](stop.md).
- [Workers](../../../control/routing/workers.md),
  [Control: Machines](../../../control/machines/README.md),
  [Control: Network](../../../control/network/README.md).

---

← [machine.status](status.md) · [Up: machine](README.md) · [machine.stop](stop.md) →
