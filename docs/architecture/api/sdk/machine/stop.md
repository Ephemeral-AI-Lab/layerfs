# machine.stop

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [machine](README.md) › stop

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`machine.stop` stops a machine's worker and, where it has one, its VM, and
records it `stopped`. Its disks and Stores stay. It is refused while
Sandboxes run on the machine unless `force` is selected.

| | |
| --- | --- |
| Family | [machine](README.md) |
| Library entry | [`machine/stop.rs`](../../../../../api/crates/layerfs-api/src/machine/stop.rs) |
| Implemented in | [`layerfs_control::machine::stop`](../../../../../control/crates/layerfs-machine/src/power.rs) |
| Runs in | the caller's process (host build with the `machine` feature) |

## Signature

```js
// JavaScript
await layerfs.machine.stop(name, { force: true });
```

```python
# Python
layerfs.machine.stop(name, force=True)
```

```bash
# CLI
layerfs machine stop <name> [--force]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | an existing machine; without `force`, no Sandbox may be recorded on it | [`power.rs`](../../../../../control/crates/layerfs-machine/src/power.rs) |

## Options

| Name | Kind | Default | Rule |
| --- | --- | --- | --- |
| `force` | Bool | false | stop while Sandboxes, mounted Workspaces or commands are active; their uncommitted state may be lost |

Force stops the platform directly without starting the machine, contacting its
worker, committing changes, unmounting Workspaces one by one, or deleting any
registered machine, Sandbox, Store or disk. On `native` it terminates only this
home’s recorded Sandbox runtimes and commands; the host stays running and its
machine remains `ready`. On Docker, stopping this home’s worker container ends
its namespaces Sandboxes; Docker Desktop and the Store volume remain. Lima uses
`limactl stop --force`, including when the host agent reports a broken VM.

The cross-entity orchestration lives in
[`machine_force.rs`](../../../../../control/crates/layerfs-control/src/machine_force.rs).
A provider failure or partial/uncertain outcome is reported; it is never replayed
or converted into a successful close.

## Returns

The machine's summary, as [`machine.get`](get.md) answers it after a live
probe; normally `state: "stopped"` (`ready` for `native`). Forced stop returns
the recorded summary without a worker probe, plus `forced: true`,
`stopped_sandboxes: [name]` and `unknown: [workspace]`. `sandbox:*` means the
worker-side uncommitted Workspace frontier could not be enumerated. These entries
are not saved Commits. The retained Sandbox records are marked gone for explicit
reclaim, with unknown custody retained if the stop could not be confirmed.

## Where it runs

The sequence and table below describe normal Stop. Explicit force uses the
provider-only path above and bypasses the normal active-Sandbox refusal.

```text
caller's process                                   machine
 machine_of(name)          Busy | Unresolved | NotFound
 native? --> answer get(name)       (nothing to stop)
 take machine/<name>/start.lock     (no start meanwhile)
 home lock: re-read record; can_stop_machine --> InUse
            spec(record)
 layerfs-platform stop(spec) ---------------------> stop worker, VM
 home lock: delete worker.json; state "stopped"
 answer get(name)
```

| Platform | What stop ends | Source |
| --- | --- | --- |
| `native` | nothing | [`power.rs`](../../../../../control/crates/layerfs-machine/src/power.rs) |
| `docker` | its worker container only | [`setup/calls.rs`](../../../../../control/crates/layerfs-platform/src/setup/calls.rs) `stop` |
| `lima`, `apple-container` | the worker and the VM; disks stay | `setup/calls.rs` |

Nothing stops a machine because it is unused. After a stop, Store,
project, Sandbox and Workspace calls that need the machine are refused
`Unavailable`, naming `layerfs machine start`
([`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs)).
All in the caller ([routing table](../../../control/routing/routing-table.md)).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput`, `NotFound` | bad name; no such machine | no | [`record.rs`](../../../../../control/crates/layerfs-machine/src/record.rs) `of` |
| `Busy`, `Unresolved` | an `op` holds the machine | no | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs) |
| `InUse` | without `force`, Sandboxes are recorded on it; `detail.related` lists `{kind: "sandbox", name}` | no | [`inventory.rs`](../../../../../control/crates/layerfs-control/src/inventory.rs) `can_stop_machine` |
| `Unavailable` | `start.lock` held for 600 s; nothing changed | no | [`home/entity_lock.rs`](../../../../../control/crates/layerfs-home/src/home/entity_lock.rs) |
| `Unavailable`, `Denied`, `Failed` | the provider or guest did not answer or refused; the stop failed once. `worker.json` and the record stay as they were | no | [`error.rs`](../../../../../control/crates/layerfs-platform/src/error.rs) |
| `Unknown` | **a provider command timed out: the machine may be stopped while still recorded with its worker.** Read `machine status` first | **yes** | [`setup/tool.rs`](../../../../../control/crates/layerfs-platform/src/setup/tool.rs) |
| `Unsupported` | a record of a removed platform (`firecracker`, `microsandbox`, `ssh`: ``platform `ssh` was removed in LayerFS 2.0``); a build without the `machine` feature | no | [`machine/stop.rs`](../../../../../api/crates/layerfs-api/src/machine/stop.rs) |

## Examples

Not run; written from the call table.

```js
await layerfs.sandbox.close("agent-1");
await layerfs.machine.stop("vm1");
```

```python
layerfs.machine.stop("vm1")
```

```console
$ layerfs machine stop vm1
```

## Related

- Before: [`sandbox.close`](../sandbox/close.md) for each Sandbox on the
  machine. After: [`machine.start`](start.md), or
  [`machine.remove`](remove.md).
- [Workers](../../../control/routing/workers.md#lifecycle),
  [Control: Machines](../../../control/machines/README.md).

---

← [machine.start](start.md) · [Up: machine](README.md) · [machine.create](create.md) →
