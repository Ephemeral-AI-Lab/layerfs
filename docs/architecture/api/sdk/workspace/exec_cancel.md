# workspace.exec_cancel

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [workspace](README.md) › exec_cancel

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Ends a command a [workspace.exec](exec.md) runs, from any process: SIGTERM
to everything the command started, then SIGKILL after `grace_ms`. It is
one explicit attempt: nothing times a command out, retries or replays a
cancel (compute-plane 2.0 §5.1, §5.4; layerfs-project#113).

| | |
| --- | --- |
| Family | [workspace](README.md) |
| Library entry | [`workspace/exec_cancel.rs`](../../../../../api/crates/layerfs-api/src/workspace/exec_cancel.rs) |
| Implemented in | the library entry parses a typed `ExecCancelRequest` → `layerfs_sandbox::workspace::exec::exec_cancel` ([`workspace/exec.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/exec.rs)) → host operation `workspace.exec_cancel` → `exec::cancel::call` ([`exec/cancel.rs`](../../../../../control/crates/layerfs-sandbox/src/exec/cancel.rs)) → the backend's stop handle ([`stop.rs`](../../../../../control/crates/layerfs-sandbox-backend/src/stop.rs)) |
| Runs in | the side that keeps the command's record: the Sandbox's machine (here, or its worker), or this home for a Sandbox it reaches directly |

## Signature

```js
// JavaScript
await layerfs.workspace.exec_cancel(ws, exec_id, { grace_ms });
```

```python
# Python
layerfs.workspace.exec_cancel(ws, exec_id, grace_ms=None)
```

```bash
# CLI
layerfs workspace exec-cancel <ws> <exec-id> [--grace-ms N]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `ws` | Text | the command's Workspace, `<sandbox>:<path>` or `<sandbox>:<name>` | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs) |
| `exec_id` | Text | the id [workspace.exec](exec.md) was given or sent as its first progress event | [`exec/id.rs`](../../../../../control/crates/layerfs-sandbox/src/exec/id.rs) |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `grace_ms` | Int | 5000 | how long the command has to end after SIGTERM before SIGKILL; not negative; with 0, SIGKILL follows SIGTERM at once | [`exec/cancel.rs`](../../../../../control/crates/layerfs-sandbox/src/exec/cancel.rs) `grace` |

## Returns

```text
 { outcome: "cancelled", signal: "SIGTERM" | "SIGKILL" }
 { outcome: "ended" }      it had finished: no record, or nothing left
 { outcome: "unknown" }    not confirmed: the worker was not reached or
                           did not answer; nothing is known to have changed
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `outcome` | text | `cancelled`, `ended` or `unknown` | [`exec/cancel.rs`](../../../../../control/crates/layerfs-sandbox/src/exec/cancel.rs) `Outcome` |
| `signal` | text | with `cancelled`: the last signal sent: `SIGTERM` when the command was gone within the grace, else `SIGKILL` (with `grace_ms: 0` it is `SIGKILL` even when SIGTERM already ended the command, whose exec then reports exit code 143) | same, [`stop.rs`](../../../../../control/crates/layerfs-sandbox-backend/src/stop.rs) `ended` |

The cancelled command's own `workspace.exec` answers `kind: "cancelled"`
with exit code 128 + the signal.

## Where it runs

```text
 caller process                     side that keeps the record
 --------------                     --------------------------
 ExecId::parse; grace; WorkspaceRef
 forwarded (this home keeps it)? ---> cancel here, then the worker
 else reach -----------------------> dispatch "workspace.exec_cancel"
   not reached / no answer:            Sandbox lock: read exec/<id>.json
   { outcome: "unknown" }                none        -> ended
                                         owner gone  -> stale: ended,
                                                        removed, logged
                                         (a forwarded one: ended in its
                                          worker, zero grace; kept as it
                                          was when that is unknown)
                                         else mark cancel, unlock
                                       no stop handle yet (not running):
                                         cancelled, SIGKILL; its owner
                                         ends it once it runs
                                       stop handle (one attempt):
                                         Cgroup  SIGTERM each process,
                                                 wait grace, cgroup.kill,
                                                 check it is empty
                                         Group   SIGTERM the group (leader
                                                 checked), wait grace,
                                                 SIGKILL
                                       the exec's owner sees it end and
                                       removes the record
```

On `namespaces` (every Linux kernel, the docker machine included) each
command runs in its own cgroup,
`<sandbox cgroup>/ws-<workspace id>/exec-<exec id>`, so children that left
its process group end too. On `host_process` (macOS `native`) it is the
command's own process group; a child that calls `setsid` itself escapes
it. A cancel that comes before the command runs (its record has no stop
handle yet) answers `cancelled` with `SIGKILL` and marks the record; the
exec's owner, told the handle once the command runs, ends it at once, and
the exec answers `kind: "cancelled"` with exit code 137. A cancel never
answers `ended` for a command that then runs. For a
Sandbox this home reaches directly, a worker that answers `NoSuchSandbox`
confirms the command `ended`. A worker that takes the cancel and never
answers leaves the call waiting until its connection closes: there is no
operation timeout (§5.4). See
[Control: commands and streams](../../../control/sandboxes/commands-and-streams.md#exec-records-and-cancel)
and the [routing table](../../../control/routing/routing-table.md).

## Errors

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad `ws` or `exec_id`; a negative `grace_ms` | no | [`exec/id.rs`](../../../../../control/crates/layerfs-sandbox/src/exec/id.rs), [`exec/cancel.rs`](../../../../../control/crates/layerfs-sandbox/src/exec/cancel.rs) |
| `NoSuchSandbox`, `NotMounted` | no such Sandbox; the Workspace is not mounted | no | [`workspace/serve.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) |
| `Failed` | a record that does not read, or the signal could not be sent | no | [`exec/record.rs`](../../../../../control/crates/layerfs-sandbox/src/exec/record.rs), [`stop.rs`](../../../../../control/crates/layerfs-sandbox-backend/src/stop.rs) |

An unreached worker is not an error: it is `{ outcome: "unknown" }`.
**After `unknown`**, call `workspace.exec_cancel` again once the worker
answers; a command that ended meanwhile answers `ended`. The record keeps
the cancel's mark after `unknown`: if the command then ends by a signal,
its exec answers `kind: "cancelled"`.

## Examples

```js
const id = new Promise((resolve) => {
  layerfs.workspace.exec(ws, "npm run dev", { on_progress: (p) => resolve(p.exec.id) });
});
await layerfs.workspace.exec_cancel(ws, await id, { grace_ms: 2000 });
```

```python
layerfs.workspace.exec_cancel("s1:build", "dev", grace_ms=0)
```

```bash
layerfs workspace exec s1:build 'sleep 600' --exec-id long &
layerfs workspace exec-cancel s1:build long
# {"outcome":"cancelled","signal":"SIGTERM"}
```

The CLI form was run by `layerfs-cli/tests/exec_cancel.rs` (native),
`exec_cancel_worker.rs` (stand-in worker) and the release CLI on `native`
and the `docker` machine (Docker Desktop worker, namespaces cgroups) on
the layerfs-project#113 branch; the JS and Python lines are written from the call table.

## Related

- [workspace.exec](exec.md): the command and its exec id.
- [workspace.unmount](unmount.md) and [sandbox.close](../sandbox/close.md):
  refused `Busy` while a command runs; `force` cancels it first.
- [layerfs doctor](../../../control/management/doctor.md): reports stale
  command records.

---

← [workspace.exec](exec.md) · [Up: workspace](README.md) · [workspace.commit](commit.md) →
