# sandbox.get

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [sandbox](README.md) › get

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Reads one Sandbox's record, its live state and one page of the Workspaces
its daemon holds, without starting anything.

| | |
| --- | --- |
| Family | [sandbox](README.md) |
| Library entry | [`sandbox/get.rs`](../../../../../api/crates/layerfs-api/src/sandbox/get.rs) |
| Implemented in | the library entry parses a typed `GetRequest` → `layerfs_sandbox::family::get` → `get::get`, whose typed `Got` (its `daemon_capacity` included) it shapes ([`get.rs`](../../../../../control/crates/layerfs-sandbox/src/get.rs)) |
| Runs in | the caller's process; the Workspace list and the daemon's capacity read model come from the Sandbox daemon, dialed from here (forwarded or local) or from the machine's worker |

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
   state,                       creating|closing|unresolved|gone|lost|running
   op?: { id, name, phase, started, uncertain },
   workspaces: [ { ws, name, path,
                   workspace, namespace, branch, base, base_layer,
                   root, activity, mounted, directory,
                   head_moved_since_mount, dirty: {inodes, entries}?,
                   pending_merge, revision } ],
   next_page_pos: int | null,
   down_reason?: text,
   daemon_capacity?: { admission: { pool_vacant, pool_running,
                                    reserved_vacant, reserved_running },
                        retained: { limit, ttl_ms, count, dropped, recorded,
                                    failures: [ { slot, class, generation, owner,
                                                  received_bytes, request, reply_sent,
                                                  cause, fence_error,
                                                  diagnostic_error, age_ms,
                                                  expires_in_ms } ] } } }
```

| Field | Kind | Meaning |
| --- | --- | --- |
| record fields | | as [`sandbox.list`](list.md) shows them |
| `state` | text | derived again with the machine's answer: `lost` when the record has an id and no close intent and the machine says no such Sandbox runs (it ended behind control's back; `sandbox.close` clears it, compute-plane 2.0 P-1), `gone` when the record has no id ([`record/sandbox.rs`](../../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs) `state`) |
| `workspaces[].ws` | text | `<sandbox>:<name>` when the mount named it, else `<sandbox>:<path>` ([`get.rs`](../../../../../control/crates/layerfs-sandbox/src/get.rs) `listed_workspace`) |
| `workspaces[].name`, `path` | text | the mount's name or null; the mount directory |
| `workspaces[].*` | | the daemon's read model per Workspace: id, namespace, Branch, base Commit and Layer, root, `activity`, dirty counts ([`operator/inspect.rs`](../../../../../engine/crates/layerfs-store-service/src/operator/inspect.rs) `workspace_value`) |
| `next_page_pos` | int or null | next page of `workspaces` |
| `down_reason` | text | the machine's worker or the forwarded daemon was not reached; `workspaces` is then `[]` |
| `daemon_capacity` | record | the daemon's `backend.daemon.connection_failures` read model ([#540](https://github.com/Ephemeral-AI-Lab/layerfs/issues/540)): control admission by slot class and the bounded retained connection failures that used to hold a slot ([`control_capacity.rs`](../../../../../engine/crates/layerfs-bridge/src/control_capacity.rs)). It is read once, best effort, and only for a Sandbox that is `running` and whose machine and forward were reached; nothing is started and no second control slot is taken, so the field is simply absent when the daemon refuses, does not answer within 2 s, or was not asked ([`daemon/capacity.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/capacity.rs)) |

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
 NoSuchSandbox           --> state "lost", workspaces []

 running and not down, same route and the same pooled connection:
   backend.daemon.connection_failures --> daemon_capacity (2 s, best effort)
```

`get` starts no machine and loads no image (#535 P4); reading never clears
an intent. The capacity read is a second host operation on the connection
the Workspace list lent from the Sandbox's pool
([`daemon/pool.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs)),
bounded to 2 s for admission and for
the exchange and fenced at that deadline ([`daemon/fence.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/fence.rs)),
so a daemon that trickles cannot hold the slot; a forwarded daemon's forward
and its machine are checked before it is dialed (`direct::check`), so a
stopped machine is `down_reason` and never a 2 s wait on a reused port. See
the [routing table](../../../control/routing/routing-table.md) and the
[daemon channel](../../../control/routing/daemon-channel.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | bad name; folder scope | no | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs), [`calls/scope.rs`](../../../../../api/crates/layerfs-api/src/calls/scope.rs) |
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
