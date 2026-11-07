# sandbox.close

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [sandbox](README.md) › close

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Stops a Sandbox and frees its name: the runtime is stopped and deleted and
its record removed; it is also the call that clears an uncertain
`sandbox.create` or `sandbox.close`.

| | |
| --- | --- |
| Family | [sandbox](README.md) |
| Library entry | [`sandbox/close.rs`](../../../../../api/crates/layerfs-api/src/sandbox/close.rs) |
| Implemented in | the library entry parses a typed `CloseRequest` → `layerfs_sandbox::family::close` → `close::close`, whose typed `Closed` it shapes ([`close.rs`](../../../../../control/crates/layerfs-sandbox/src/close.rs)), then `lifecycle::close` on the machine ([`lifecycle.rs`](../../../../../control/crates/layerfs-sandbox/src/lifecycle.rs)) |
| Runs in | the caller's process (records, locks), then the caller's process for a local machine or the machine's worker (`sandbox.host`) |

## Signature

```js
// JavaScript
await layerfs.sandbox.close(name, { force? })
await layerfs.client({ home }).sandbox.close(name, { force: true })
```

```python
# Python
layerfs.sandbox.close(name, force=None)
```

```bash
# CLI
layerfs sandbox close <name> [--force]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `name` | Name | a recorded Sandbox, else `NoSuchSandbox` | [`family.rs`](../../../../../control/crates/layerfs-sandbox/src/family.rs) `sandbox_of` |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `force` | Bool | false | stop even with Workspaces mounted; their uncommitted changes and processes end with the Sandbox | [`lifecycle.rs`](../../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) `close` |

Under a folder scope the call is refused `InvalidInput`
([`calls/scope.rs`](../../../../../api/crates/layerfs-api/src/calls/scope.rs)).

## Returns

```text
 started Sandbox, stopped here      { kind: "Closed", sandbox, unknown: [path],
                                      closed }
 machine says it is already gone    { closed, was_gone: true }
 record with no id (never started,  { closed, was: "gone" | "unresolved",
 or create's outcome unknown)         left?: text }
```

| Field | Kind | Meaning |
| --- | --- | --- |
| `kind` | text | `Closed` |
| `sandbox` | text | the Sandbox's id (its name) |
| `unknown` | list of text | with `force`: the Workspace paths it could not drain; empty otherwise |
| `closed` | text | the name now free |
| `was_gone` | bool | the runtime had already ended; its record was removed |
| `was` | text | the derived state of a record with no id |
| `left` | text | for `unresolved`: a Sandbox its create may have started is not recorded here; or what a layout 2 conversion found ended |

## Where it runs

```text
 caller's process
  1 sandbox_of(name)
  2 home + Sandbox lock: clearable? (a live op -> Busy; an uncertain
    op of a gone owner is what close clears)
    no id --> close_unstarted: remove the folder, answer { closed, was }
    else  --> write op sandbox.close (phase stop)
  3 forwarded? direct::held: Workspaces its daemon still holds -> held
  4 route::on_machine(record, "sandbox.close", [force, held?])
       Executor::Local --> lifecycle::close here
       Executor::Worker -> "sandbox.host" -> worker -> lifecycle::close
         guard; MountsActive unless force; stop (5 s), then delete,
         each once; remove the machine-side folder
  5 home lock: release the forward; remove sandbox/<name>/
```

| Outcome on the machine | Home record | Source |
| --- | --- | --- |
| `Closed` | removed; forward ended | [`close.rs`](../../../../../control/crates/layerfs-sandbox/src/close.rs) |
| `NoSuchSandbox` (already gone) | removed; answer `was_gone` | `close.rs` |
| `Unknown` | kept, `op` marked uncertain, logged | `close.rs` |
| any other refusal | kept, `op` dropped: the Sandbox is as it was | `close.rs` |

See the [routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | bad name; folder scope | no | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs) |
| `NoSuchSandbox` | no record of that name | no | [`family.rs`](../../../../../control/crates/layerfs-sandbox/src/family.rs) |
| `Busy` | another live operation's `op` holds the Sandbox (`detail {entity, phase, op}`); an unmount is in progress | no | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs), [`unmount_guard.rs`](../../../../../control/crates/layerfs-sandbox/src/unmount_guard.rs) |
| `MountsActive` | without `force`, Workspaces are mounted; `detail.workspaces` lists their paths | no | [`lifecycle.rs`](../../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) |
| `Unavailable` | the machine's worker is not recorded or does not answer (nothing sent); a lock held too long | no | [`place.rs`](../../../../../control/crates/layerfs-store/src/place.rs) |
| `Failed` | the stop or delete failed (a failed stop is not followed by a delete); the Sandbox stays as it was | no | `lifecycle.rs` |
| `Unknown` | **the machine's answer was lost after the request was sent: the Sandbox may be stopped or not** | **yes** | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs), `close.rs` |

After `Unknown`, read [`sandbox.get`](get.md) (its `state` is
`unresolved`), then call `close` again: it clears the uncertain intent.
Nothing is retried for you.

## Examples

Not run; written from the call table.

```js
await layerfs.sandbox.close("tests");
```

```python
layerfs.sandbox.close("tests", force=True)
```

```console
$ layerfs sandbox close tests --force
```

## Related

- Before: [`workspace.commit`](../workspace/commit.md), [`workspace.unmount`](../workspace/unmount.md), [`sandbox.get`](get.md).
- After: [`sandbox.create`](create.md) with the same name.
- [Control: Sandboxes](../../../control/sandboxes/README.md), [Engine: Forced teardown](../../../engine/daemon/forced-teardown.md).
- [A Workspace session](../../../flows/workspace.md).

---

← [sandbox.get](get.md) · [Up: sandbox](README.md) · [SDK: workspace](../workspace/README.md) →
