# machine.start

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [machine](README.md) › start

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Firecracker, Microsandbox and SSH machines were removed in 2.0; a record
an older home still holds for one is shown, refused and only forgotten.
See [removed platforms](../../../control/support-matrix/disabled-platforms.md).

`machine.start` starts a stopped machine and its `layerfs worker`, waits
until the worker answers its secret, and answers the machine's summary. A
machine whose worker already answers is answered at once. It is the only
call that starts a machine (#535 D11).

| | |
| --- | --- |
| Family | [machine](README.md) |
| Library entry | [`machine/start.rs`](../../../../../api/crates/layerfs-api/src/machine/start.rs) |
| Implemented in | [`layerfs_control::machine::start`](../../../../../control/crates/layerfs-machine/src/power.rs), through [`endpoint::running`](../../../../../control/crates/layerfs-machine/src/endpoint.rs) |
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
| `name` | Name | an existing machine not held by an `op` | [`spec.rs`](../../../../../control/crates/layerfs-machine/src/spec.rs) `machine_of` |

## Options

`on_progress` receives stage events. The CLI exposes it as `--progress`, emitting
JSON lines on stderr while stdout retains the final result. Each started stage
has a `completed` or `failed` event with `elapsed_ms`. Stages are
`bundle-validation`, `sandbox-image` (default Docker image only),
`provider-and-worker` (includes guest provisioning and readiness), and
`worker-compatibility`. An already answering worker takes no provisioning stages and is still checked
for protocol/bundle compatibility.

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
 take machine/<name>/start.lock  (wait <= 600 s)
 worker answers now? (another process started it) --> get
 spec + sealed Linux bundle
 validate manifest protocol/architecture when supplied
 docker with no image? --> inspect Engine content tag; import only if missing
 layerfs-platform start(spec, bundle) ------------> boot VM / container
                                                   copy changed/missing
                                                   bundle files,
                                                   start `layerfs worker`
           readiness wait (not a retry) <--------- worker answers secret
 authenticated hello: compare protocol and bundle
 home lock: write worker.json and lasting; running only after compatibility
 incompatible hello? --> retain endpoint and failed state; Unavailable
 answer get(name)
```

`start.lock` is taken before the home lock and is shared with `create`,
`stop` and `remove`, so a start never runs beside them on the same machine
([`endpoint.rs`](../../../../../control/crates/layerfs-machine/src/endpoint.rs)).
The home lock is held only to write the records, never while the VM boots.

| Platform | What start does | Source |
| --- | --- | --- |
| `native` | nothing; answers at once | [`power.rs`](../../../../../control/crates/layerfs-machine/src/power.rs) |
| `docker` | runs the privileged worker container (`--pull never`) from the recorded image, else the bundle's image loaded once; the worker runs namespaces Sandboxes inside it, rooted in the bundle's `sandbox-image.tar` (D-8) | [`image.rs`](../../../../../control/crates/layerfs-machine/src/image.rs), [`platforms/docker/power.rs`](../../../../../control/crates/layerfs-platform/src/platforms/docker/power.rs) |
| `lima`, `apple-container` | boots the VM if stopped, then sets up and starts the worker | [`platforms/`](../../../../../control/crates/layerfs-platform/src/platforms) |

Every other call on a stopped machine is refused `Unavailable`, naming
`layerfs machine start` ([`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs)).
See [workers](../../../control/routing/workers.md#lifecycle) and the
[routing table](../../../control/routing/routing-table.md).

## Artifact reuse

The existing flat bundle and guest paths remain compatible. `bundle.json` now
also records `protocol` and a `runtime` identity for the three binaries. Its
existing `image` config digest identifies Sandbox content independently of runtime
builds. Image archives have deterministic tar metadata so binary-only rebuilds
leave their file digest unchanged.

Docker owns the shared mapping in the Engine: `layerfs-cache/sandbox:<config-hex>`
points to the actual ID returned by `docker load`. That ID can differ from the
archive config digest. `image inspect` checks presence on the selected Engine;
only a missing image is a cache miss. Engine failures are returned, not treated
as a miss. A valid matching per-home record seeds the tag without another import.
A private cache lock serializes imports for the same content across homes. These
tags are reusable dependencies, not disposable per-machine test state.

VM installation compares the host and guest manifests' per-file identities and
checks file presence. It transfers changed or absent artifacts into `.install`,
renames each file, and publishes the new manifest and `.sealed` only after all
copies succeed. A runtime update preserves an identical Sandbox archive. Prepared
Lima images supplied through the machine `os_image` option benefit from the
same check; no warm pool is introduced and no guest private state is shared.

An explicitly incompatible manifest protocol or architecture is refused before
image import or VM boot. Legacy manifests lacking protocol metadata remain
readable and retain the final worker handshake check; legacy manifests without
file identities retain whole-folder installation through a staging directory,
with publication only after the complete copy succeeds. This is compatibility support,
not proof that an unlabelled worker is compatible before boot.

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput`, `NotFound` | bad name; no such machine | no | [`record.rs`](../../../../../control/crates/layerfs-machine/src/record.rs) `of` |
| `Busy`, `Unresolved` | a create or remove holds it | no | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs) `blocks` |
| `Unavailable` | `start.lock` held by another process for 600 s; nothing changed | no | [`home/entity_lock.rs`](../../../../../control/crates/layerfs-home/src/home/entity_lock.rs) |
| `Unavailable` | the started worker is of another build: the bundle is older than this build (`detail.worker`, `expected`, `bundle`) | no: the machine runs and is recorded | [`worker/hello.rs`](../../../../../control/crates/layerfs-machine/src/worker/hello.rs) `stale_bundle` |
| `Unsupported` | a record of a removed platform (`firecracker`, `microsandbox`, `ssh`: ``platform `ssh` was removed in LayerFS 2.0``); no sealed Linux bundle (`detail.missing: ["bundle:<dir>"]`); docker with no image and no `sandbox-image.tar`; a build without the `machine` feature | no | [`bundle.rs`](../../../../../control/crates/layerfs-machine/src/bundle.rs), `image.rs` |
| `NotFound` | the provider holds no VM of that name | no | [`setup/vm.rs`](../../../../../control/crates/layerfs-platform/src/setup/vm.rs) |
| `Unavailable`, `Denied`, `Failed` | the provider or guest did not answer or refused; a step failed once | no | [`error.rs`](../../../../../control/crates/layerfs-platform/src/error.rs) |
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

- Before: [`machine.create`](create.md) or [`machine.stop`](stop.md). After: Store, project and Sandbox calls on the
  machine; [`machine.stop`](stop.md).
- [Workers](../../../control/routing/workers.md),
  [Control: Machines](../../../control/machines/README.md),
  [Control: Network](../../../control/network/README.md).

---

← [machine.status](status.md) · [Up: machine](README.md) · [machine.stop](stop.md) →
