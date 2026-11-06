# sandbox.get

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [sandbox](README.md) › get

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Reads one Sandbox's record, its live state and one page of the Workspaces
its daemon holds, without starting anything.

| | |
| --- | --- |
| Family | [sandbox](README.md) |
| Library entry | [`sandbox/get.rs`](../../../../../api/crates/layerfs-api/src/sandbox/get.rs) |
| Implemented in | `layerfs_sandbox::family::sandbox` → `get::get` ([`get.rs`](../../../../../control/crates/layerfs-sandbox/src/get.rs)) |
| Runs in | the caller's process; the Workspace list comes from the Sandbox daemon, dialed from here (forwarded or local) or from the machine's worker |

## Signature

```js
// JavaScript
await layerfs.sandbox.get(name, { page_size?, page_pos? })
await layerfs.client({ home }).sandbox.get(name)
```

```python
# Python
layerfs.sandbox.get(name, page_size=None, page_pos=None)
```

```bash
# CLI
layerfs sandbox get <name> [--page-size <n>] [--page-pos <n>]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a Sandbox name (name syntax); recorded in this home, else `NoSuchSandbox` | [`family.rs`](../../../../../control/crates/layerfs-sandbox/src/family.rs) `sandbox_of` |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `page_size` | Int | 50 | clamped to 1..=1000; pages the `workspaces` list | [`family.rs`](../../../../../control/crates/layerfs-sandbox/src/family.rs) `page` |
| `page_pos` | Int | 0 | a negative value counts as 0 | `family.rs` `page` |

## Returns

```text
 { name, project, machine, created, id?, platform?, runtime?, image?,
   backend?, route?,
   state,                       creating|closing|unresolved|gone|running
   op?: { id, name, phase, started, uncertain },
   workspaces: [ { ws, name, path,
                   workspace, namespace, branch, base, base_layer,
                   root, activity, mounted, directory,
                   head_moved_since_mount, dirty: {inodes, entries}?,
                   pending_merge, revision } ],
   next_page_pos: int | null,
   down_reason?: text }
```

| Field | Kind | Meaning |
| --- | --- | --- |
| record fields | | as [`sandbox.list`](list.md) shows them |
| `state` | text | derived again with the machine's answer: `gone` when it says no such Sandbox runs ([`record.rs`](../../../../../control/crates/layerfs-sandbox/src/record.rs) `state`) |
| `workspaces[].ws` | text | `<sandbox>:<name>` when the mount named it, else `<sandbox>:<path>` ([`get.rs`](../../../../../control/crates/layerfs-sandbox/src/get.rs) `with_ws`) |
| `workspaces[].name`, `path` | text | the mount's name or null; the mount directory |
| `workspaces[].*` | | the daemon's read model per Workspace: id, namespace, Branch, base Commit and Layer, root, `activity`, dirty counts ([`operator/inspect.rs`](../../../../../engine/crates/layerfs-store-service/src/operator/inspect.rs) `workspace_value`) |
| `next_page_pos` | int or null | next page of `workspaces` |
| `down_reason` | text | the machine's worker or the forwarded daemon was not reached; `workspaces` is then `[]` |

## Where it runs

```text
 caller's process: sandbox_of(name)
   |
   +-- no id (create never recorded one) --> no daemon asked
   +-- forwarded (record.daemon.forward) --> direct::list
   |        this process dials the daemon: sandbox.list_workspaces
   +-- otherwise --> route::reach(..., load_image = false)
            Executor::Local  --> dispatch here --> daemon
            Executor::Worker --> "sandbox.host" --> worker --> daemon
   |
   v
 Unreached / Unavailable --> record + workspaces [] + down_reason
 NoSuchSandbox           --> state "gone", workspaces []
```

`get` starts no machine and loads no image (#535 P4); reading never clears
an intent. See the [routing table](../../../control/routing/routing-table.md) and the
[daemon channel](../../../control/routing/daemon-channel.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | bad name; folder scope | no | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs), [`calls/scope.rs`](../../../../../control/crates/layerfs-control/src/calls/scope.rs) |
| `NoSuchSandbox` | no record of that name | no | [`family.rs`](../../../../../control/crates/layerfs-sandbox/src/family.rs) |
| `Busy`, `StaleSandbox` | a forwarded daemon admits no connection in time; a saved daemon is not `ControlReady` | no | [`daemon/pool.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs) |
| `Unknown` | **the worker's answer was lost after the request was sent; harmless for a read, but the answer is missing** | **yes (no effect expected)** | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |
| `Failed` | the daemon's list failed or was not a record | no effect (a read) | [`dispatch.rs`](../../../../../control/crates/layerfs-sandbox/src/dispatch.rs) |

A machine that is down is not an error here: it is `down_reason`. A
Sandbox with an uncertain `op` is shown, not refused: `get` is the call
that `Unresolved` refusals name.

## Examples

Not run; written from the call table.

```js
const sb = await layerfs.sandbox.get("tests");
for (const w of sb.workspaces) console.log(w.ws, w.activity);
```

```python
sb = layerfs.sandbox.get("tests")
print(sb["state"], sb.get("down_reason"))
```

```console
$ layerfs sandbox get tests
```

## Related

- Before: [`sandbox.create`](create.md), [`workspace.mount`](../workspace/mount.md).
- After: [`workspace.status`](../workspace/status.md), [`workspace.unmount`](../workspace/unmount.md), [`sandbox.close`](close.md).
- [Engine: Daemon operator](../../../engine/daemon/operator.md), [Control: Sandboxes](../../../control/sandboxes/README.md).

---

← [sandbox.list](list.md) · [Up: sandbox](README.md) · [sandbox.close](close.md) →
