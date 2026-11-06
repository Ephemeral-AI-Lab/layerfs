# workspace.mount

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [workspace](README.md) › mount

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Mounts a Branch as a new Workspace, a complete writable filesystem view of
its head, in a running Sandbox, and answers the `ws` the other Workspace
calls take.

| | |
| --- | --- |
| Family | [workspace](README.md) |
| Library entry | [`workspace/mount.rs`](../../../../../api/crates/layerfs-api/src/workspace/mount.rs) |
| Implemented in | `layerfs_control::workspace::calls::workspace` → host operation `workspace.mount` → `workspace::serve::mount` ([`workspace/calls.rs`](../../../../../control/crates/layerfs-control/src/workspace/calls.rs), [`workspace/serve.rs`](../../../../../control/crates/layerfs-control/src/workspace/serve.rs)) |
| Runs in | the caller's process, the Sandbox's machine (here or its worker) and the Sandbox's daemon |

## Signature

```js
// JavaScript
await layerfs.workspace.mount(sandbox, branch, { name });
```

```python
# Python
layerfs.workspace.mount(sandbox, branch, name=None)
```

```bash
# CLI
layerfs workspace mount <sandbox> <branch> [--name NAME]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `sandbox` | Name | a running Sandbox from [sandbox.create](../sandbox/create.md); a mount never starts one | [`workspace/serve.rs`](../../../../../control/crates/layerfs-control/src/workspace/serve.rs) |
| `branch` | Name | a Branch of the Sandbox's project; `main` is refused (fork a Branch from it) | same |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `name` | Name | none | `WorkspaceName::parse`; unique among the Sandbox's Workspaces; `ws` becomes `<sandbox>:<name>` | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs) |

## Returns

```text
 { kind: "Mounted", path, workspace, namespace, branch,
   base: <head Commit> | null, base_layer, root,
   sandbox, container, image, platform | null, runtime, name | null,
   ws: "<sandbox>:<path>" | "<sandbox>:<name>" }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `path` | text | the mount directory in the Sandbox | [`daemon/values.rs`](../../../../../control/crates/layerfs-control/src/daemon/values.rs) `mounted` |
| `workspace`, `namespace` | text, int | the daemon's Workspace id and namespace | same |
| `base`, `base_layer`, `root` | text | the Branch head and base Layer it is bound to, and its root | same |
| `sandbox`, `container`, `image`, `platform`, `runtime` | text | where it lives | [`workspace/serve.rs`](../../../../../control/crates/layerfs-control/src/workspace/serve.rs) |
| `ws` | text | the handle for exec, commit, unmount and status | [`workspace/calls.rs`](../../../../../control/crates/layerfs-control/src/workspace/calls.rs) |

The mount binds the whole root without a tree scan or copy; its only new
file is the Workspace's own overlay database, copied from the template.
The Workspace is recorded as `workspace/<id>.json` in the Sandbox's folder.

## Where it runs

```text
 caller process            Sandbox's machine (here | worker)     daemon
 --------------            ---------------------------------     ------
 sandbox_of; record.free (Busy / Unresolved)
 forwarded? --> direct::call (daemon reached from this host)
 else on_machine --------> serve::mount
                             main? --> InvalidInput
                             started? else NoSuchSandbox
                             name free? else Exists
                             guard: < 16 Workspaces? else Capacity
                             Branch id (the Store's own route)
                             workspace.mount(id, branch) -------> bind root,
                                                                 overlay file,
                                                                 kernel mount
                             record workspace/<id>.json
 add ws to the answer
```

The Branch is looked up through the Store's route, not the daemon; the
daemon binds its id. For a PostgreSQL Store it is looked up on the
process's one held read session for that Store, not a Store service (#65,
[`store/reader.rs`](../../../../../control/crates/layerfs-control/src/store/reader.rs)):
at most one connection per process, reused by later calls within 60 s. See the [routing table](../../../control/routing/routing-table.md)
and the [daemon channel](../../../control/routing/daemon-channel.md). Under a folder
scope only `main` is accepted and is mounted as the Branch `<ns>.main`
([`calls/scope.rs`](../../../../../control/crates/layerfs-control/src/calls/scope.rs)).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | no `sandbox`; `branch` is `main`; a bad `name`; out of scope | no | [`workspace/serve.rs`](../../../../../control/crates/layerfs-control/src/workspace/serve.rs) |
| `NoSuchSandbox` | no such Sandbox, or its daemon ended itself when idle | no | same |
| `NoSuchBranch` | the Branch is missing | no | [`store/service.rs`](../../../../../control/crates/layerfs-control/src/store/service.rs) |
| `Exists` | the Sandbox already has a Workspace of that name | no | [`workspace/serve.rs`](../../../../../control/crates/layerfs-control/src/workspace/serve.rs) |
| `Capacity` | the Sandbox holds 16 Workspaces (`WORKSPACES`) | no | same |
| `Busy`, `Unresolved` | a `sandbox.create` or `close` holds the Sandbox | no | [`sandbox/record.rs`](../../../../../control/crates/layerfs-control/src/sandbox/record.rs) `free` |
| `Unavailable` | the machine's worker is not recorded or does not answer | no | [`sandbox/route.rs`](../../../../../control/crates/layerfs-control/src/sandbox/route.rs) |
| daemon refusal codes | the daemon refused before effect; its own code and `detail.phase` | no | [`daemon/values.rs`](../../../../../control/crates/layerfs-control/src/daemon/values.rs) `refusal` |
| `Failed` | **a daemon exchange failed; the message carries the `OperationFailure`, whose `attempted` says whether the mount may have taken effect** | **maybe** | [`sandbox/serve.rs`](../../../../../control/crates/layerfs-control/src/sandbox/serve.rs) `operation` |
| `Unknown` | **the worker's answer was lost** | **yes** | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

**After an uncertain mount**, read [sandbox.get](../sandbox/get.md) for the
Sandbox's Workspaces before mounting again; a mount that landed takes one
of the 16 places.

## Examples

Not run; written from the call table.

```js
const { ws, path } = await layerfs.workspace.mount("s1", "agent", { name: "build" });
```

```python
ws = layerfs.workspace.mount("s1", "agent")["ws"]
```

```bash
layerfs workspace mount s1 agent --name build
```

## Related

- Before: [branch.fork](../branch/fork.md), [sandbox.create](../sandbox/create.md).
  After: [workspace.exec](exec.md), [workspace.status](status.md),
  [workspace.commit](commit.md), [workspace.unmount](unmount.md).
- [A Workspace session](../../../flows/workspace.md),
  [Engine: daemon](../../../engine/daemon/README.md),
  [Control: sandboxes](../../../control/sandboxes/README.md).

---

← [SDK: workspace](README.md) · [Up: workspace](README.md) · [workspace.exec](exec.md) →
