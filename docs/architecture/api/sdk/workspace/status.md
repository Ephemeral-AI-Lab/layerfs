# workspace.status

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [workspace](README.md) › status

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Shows a Workspace's base, whether its Branch's head has moved since the
mount, what it is doing, and how many changes it holds, changing nothing.

| | |
| --- | --- |
| Family | [workspace](README.md) |
| Library entry | [`workspace/status.rs`](../../../../../api/crates/layerfs-api/src/workspace/status.rs) |
| Implemented in | `layerfs_sandbox::workspace::calls::status` → host operation `backend.daemon.workspace.status` → `workspace::serve::status` ([`workspace/serve.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs)) |
| Runs in | the Sandbox's daemon (status, reads no Store) and one head read on the Store's own route |

## Signature

```js
// JavaScript
await layerfs.workspace.status(ws);
```

```python
# Python
layerfs.workspace.status(ws)
```

```bash
# CLI
layerfs workspace status <ws>
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `ws` | Text | a mounted Workspace | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs) |

## Options

None.

## Returns

```text
 { kind: "ok", base | null, base_layer, branch,
   branch_head | null, head_moved_since_mount | null,
   activity,
   overlay: { inodes, entries, pending_merge } | null }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `base`, `base_layer` | text | the Commit and Layer the Workspace is bound to now | [`daemon/values.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) `status` |
| `branch` | text or null | the Branch it was mounted from | [`workspace/serve.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) |
| `branch_head` | text or null | the Branch's head now; null when that read failed | same, `status` |
| `head_moved_since_mount` | bool or null | the head differs from `base`; null when unread | [`daemon/values.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) |
| `activity` | text | `Idle`, `Committing`, `Closing`, `Attaching`, `Uncertain` (an unknown outcome keeps custody; Commit and unmount are refused) or `LocalFailure` (published, but the local install did not finish) | [`control_types.rs`](../../../../../engine/crates/layerfs-bridge/src/control_types.rs) `Activity` |
| `overlay.inodes`, `overlay.entries` | int | uncaptured changed inodes and directory entries | [`daemon/values.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) |
| `overlay.pending_merge` | bool | the overlay's base root is not the binding's root (a merge is pending) | same |

## Where it runs

```text
 caller process            machine (here | worker)        daemon / Store
 --------------            -----------------------        --------------
 ws, sandbox_of            (no record.free: a read)
 forwarded? direct::call ------------------------------>  status
 else reach (no machine start) --> serve::status ------>  workspace.status
                                   head_of(branch) ----->  Store route:
                                                           newest head
                                   values::status
```

A read: it is answered while a `sandbox.create` or `close` holds the
Sandbox, and it starts no machine (P4). See the
[routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad `ws` | no | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs) |
| `NoSuchSandbox`, `NotMounted` | no such Sandbox or Workspace | no | [`workspace/serve.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) |
| `Unavailable` | the machine is stopped or its worker gone | no | [`route.rs`](../../../../../control/crates/layerfs-sandbox/src/route.rs) `reach` |
| daemon refusal codes | refused with `detail.phase` | no | [`daemon/values.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) |
| `Failed` | a daemon exchange failed | read-only | [`serve.rs`](../../../../../control/crates/layerfs-sandbox/src/serve.rs) |
| `Unknown` | the worker's answer was lost | read-only; call again | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

`workspace.status` is the read to make after an uncertain
[exec](exec.md) or [commit](commit.md): `activity` other than `Idle`, a new
`base`, or zero overlay counts tell what happened.

## Examples

Not run; written from the call table.

```js
const s = await layerfs.workspace.status(ws);
if (s.head_moved_since_mount) console.log("the Branch moved; the next commit merges");
```

```python
s = layerfs.workspace.status(ws)
print(s["activity"], s["overlay"])
```

```bash
layerfs workspace status s1:build
```

## Related

- Before and after: [workspace.exec](exec.md), [workspace.commit](commit.md),
  [workspace.unmount](unmount.md).
- [Engine: daemon](../../../engine/daemon/README.md),
  [Engine: overlay](../../../engine/overlay/README.md).

---

← [workspace.unmount](unmount.md) · [Up: workspace](README.md) · [SDK: machine](../machine/README.md) →
