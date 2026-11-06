# workspace.unmount

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [workspace](README.md) › unmount

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Ends a Workspace: the daemon drains its requests, detaches the mount,
closes its overlay connection and deletes its overlay file. Uncommitted
changes refuse the call unless they are discarded.

| | |
| --- | --- |
| Family | [workspace](README.md) |
| Library entry | [`workspace/unmount.rs`](../../../../../api/crates/layerfs-api/src/workspace/unmount.rs) |
| Implemented in | `layerfs_sandbox::workspace::calls::unmount` → host operation `workspace.unmount` → `workspace::serve::unmount` ([`workspace/serve.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs), [`unmount_guard.rs`](../../../../../control/crates/layerfs-sandbox/src/unmount_guard.rs)) |
| Runs in | the Sandbox's machine (records, guard) and the Sandbox's daemon (teardown) |

## Signature

```js
// JavaScript
await layerfs.workspace.unmount(ws, { discard, force });
```

```python
# Python
layerfs.workspace.unmount(ws, discard=None, force=None)
```

```bash
# CLI
layerfs workspace unmount <ws> [--discard] [--force]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `ws` | Text | a mounted Workspace | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs) |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `discard` | Bool | false | drop uncommitted changes instead of refusing `Dirty` | [`workspace/serve.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) |
| `force` | Bool | false | one abort write ends the connection, also after a normal unmount stalled at Join; uncommitted changes are dropped | same |

## Returns

```text
 normal:  { kind: "Unmounted" }
 force:   { kind: "ForceUnmounted", abort, detach, cleanup }
 either may add cleanup_error: <text>  (the daemon succeeded; a local
                                        record could not be removed)
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `kind` | text | `Unmounted` or `ForceUnmounted` | [`daemon/values.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) |
| `abort`, `detach`, `cleanup` | text | what the forced teardown established | same, `force_unmounted` |
| `cleanup_error` | text | a local cleanup failure after a known success | [`unmount_guard.rs`](../../../../../control/crates/layerfs-sandbox/src/unmount_guard.rs) |

A Workspace the daemon no longer knows (`Missing`) answers `Unmounted`.

## Where it runs

```text
 Sandbox's machine (serve::unmount)                     daemon
 ----------------------------------                     ------
 recorded outcome of an earlier unmount? --> answer it again
 Sandbox guard (short):
   again? token; refresh; begin: write workspace/<id>.unmount
   ("Unmounting"; another in progress --> Busy)
 guard released; only this Workspace's lease held
 force?   unmount(token, abort) ----------------------> abort, drain,
 else     unmount_checked(token, discard) ------------> detach, close,
                                                        delete overlay
 finish: record the outcome (or cancel if nothing was sent)
 success: remove workspace/<id>.json, forget the mount
```

The recorded outcome makes a repeated unmount answer what the first one
established, without dialling the daemon again. The daemon's kernel probe
answers `Busy` while a process still holds the mount. See the
[routing table](../../../control/routing/routing-table.md),
[Engine: daemon](../../../engine/daemon/README.md) and the
[engine teardown pages](../../../engine/kernel-routes/README.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad `ws` | no | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs) |
| `NoSuchSandbox`, `NotMounted` | no such Sandbox; the Workspace is not mounted or already unmounted | no | [`unmount_guard.rs`](../../../../../control/crates/layerfs-sandbox/src/unmount_guard.rs) |
| `Dirty` | uncommitted changes and no `discard` (`detail.changed`, `pending_merge`, `truncated`) | no | [`daemon/values.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) `unmounted` |
| `Busy` | another unmount of it is in progress; a process holds the mount; another control operation runs; a `sandbox.create`/`close` holds the Sandbox | no | [`unmount_guard.rs`](../../../../../control/crates/layerfs-sandbox/src/unmount_guard.rs), [`control/registry.rs`](../../../../../engine/crates/layerfs-daemon/src/control/registry.rs) |
| other daemon refusal codes | refused with `detail.phase` | no | [`daemon/values.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) |
| `Unavailable` | the worker is not recorded or does not answer | no | [`route.rs`](../../../../../control/crates/layerfs-sandbox/src/route.rs) |
| `Failed` | **a daemon exchange failed; `attempted` says whether teardown may have begun** | **maybe** | [`serve.rs`](../../../../../control/crates/layerfs-sandbox/src/serve.rs) |
| `Unknown` | **the worker's answer was lost** | **yes** | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

**After an uncertain unmount**, call `workspace.unmount` again: a recorded
outcome is answered as it was, and a Workspace already gone is
`NotMounted` or `Unmounted`. [sandbox.get](../sandbox/get.md) lists what is
still mounted. A stalled normal unmount can be ended with `force`.

## Examples

Not run; written from the call table.

```js
try { await layerfs.workspace.unmount(ws); }
catch (e) { if (e.code === "Dirty") await layerfs.workspace.unmount(ws, { discard: true }); }
```

```python
layerfs.workspace.unmount(ws, discard=True)
```

```bash
layerfs workspace unmount s1:build --force
```

## Related

- Before: [workspace.commit](commit.md) to keep the changes.
  After: [sandbox.close](../sandbox/close.md).
- [A Workspace session](../../../flows/workspace.md),
  [Engine: daemon](../../../engine/daemon/README.md).

---

← [workspace.commit](commit.md) · [Up: workspace](README.md) · [workspace.status](status.md) →
