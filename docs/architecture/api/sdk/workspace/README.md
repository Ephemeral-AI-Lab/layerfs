# workspace family

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › workspace

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `workspace` calls.

| | |
| --- | --- |
| Component | [Call table](../../call-table/README.md) |
| Source | [`api/crates/layerfs-api/src/workspace`](../../../../../api/crates/layerfs-api/src/workspace), [`workspace/calls.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/calls.rs), [`daemon/values.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) |

## Calls

```text
 sandbox.create (running Sandbox)      branch.fork (a Branch, not main)
            \                              /
             mount <sandbox> <branch> --> ws = <sandbox>:<path|name>
                |
                v
   +---> exec <ws> <command>   (bash -c via the Sandbox runtime)
   |        |
   |     status <ws>           (base, head moved?, activity, overlay)
   |        |
   +---- commit <ws>           (one Commit; merges a moved head)
            |
            v
         unmount <ws>          (Dirty unless committed or discard)
```

Five calls, in help order
([`workspace/mod.rs`](../../../../../api/crates/layerfs-api/src/workspace/mod.rs)).
Each runs its own function of `layerfs_sandbox::workspace::calls`. A Workspace is
named `ws`: `<sandbox>:<path>`, or `<sandbox>:<name>` when its mount named
it ([`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs)).

| Call | Arguments | Options | Returns | Host operation |
| --- | --- | --- | --- | --- |
| [`mount`](mount.md) | `sandbox` (Name), `branch` (Name) | `name` (Name) | `{ kind: "Mounted", path, workspace, namespace, branch, base, base_layer, root, sandbox, container, image, platform, runtime, name, ws }` | `workspace.mount` |
| [`exec`](exec.md) | `ws` (Text), `command` (Text) | `message` (Text), `stream` (Bool), `stdout`, `stderr` (Function) | `{ kind: "exit:<code>", exit_code, stdout, stderr, message, truncated }`; a stream over 1 MiB is spooled and sent as `{ stream, bytes }` events, the answer keeping it null with `stdout_bytes`, `stderr_bytes` and `streamed: true` | `workspace.exec` |
| [`commit`](commit.md) | `ws` | `message`, `force` (Bool) | `{ kind: "Committed", head, commit, displaced_head, merged, lww, lww_truncated }` or `{ kind: "UpToDate", head, root, merged, lww, lww_truncated }` | `workspace.commit` |
| [`unmount`](unmount.md) | `ws` | `discard`, `force` (Bool) | `{ kind: "Unmounted" }`, or with `force` `{ kind: "ForceUnmounted", abort, detach, cleanup }` | `workspace.unmount` |
| [`status`](status.md) | `ws` | — | `{ kind: "ok", base, base_layer, branch_head, head_moved_since_mount, activity, overlay: { inodes, entries, pending_merge } }` | `backend.daemon.workspace.status` |

Answers are built in
[`daemon/values.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/values.rs),
[`workspace/serve.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs),
[`exec.rs`](../../../../../control/crates/layerfs-sandbox/src/exec.rs)
and [`output.rs`](../../../../../control/crates/layerfs-sandbox/src/output.rs).
After the call, `exec` also sends its `stdout` and `stderr` as
`{ stream, bytes }` events for front ends that stream.

## Arguments and options

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `branch` | Name | a side Branch; `main` is refused (fork a Branch from it) | [`workspace/serve.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) |
| `name` | Name | unique among the Sandbox's mounted Workspaces | [`workspace/serve.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) |
| `ws` | Text | parsed by `WorkspaceRef::parse`; a name is resolved to its path on the Sandbox's side | [`workspace/calls.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/calls.rs) |
| `command` | Text | run as `bash -c` in the Workspace directory by the Sandbox runtime | [`exec.rs`](../../../../../control/crates/layerfs-sandbox/src/exec.rs) |
| `message` | Text | the shared `MESSAGE` option | [`calls/table.rs`](../../../../../api/crates/layerfs-api/src/calls/table.rs) |
| `force` (commit) | Bool | publish over a moved head instead of merging | [`workspace/calls.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/calls.rs) |
| `discard`, `force` (unmount) | Bool | drop uncommitted changes; abort the connection | [`daemon/values.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) |
| `stream`, `stdout`, `stderr` | Bool, Function | front-end options; the functions never reach the library | [`calls/check.rs`](../../../../../api/crates/layerfs-api/src/calls/check.rs) |

A Sandbox holds at most `WORKSPACES` = 16 Workspaces; inline exec output
is at most `INLINE` = 1 MiB per stream
([`lifecycle.rs`](../../../../../control/crates/layerfs-sandbox/src/lifecycle.rs),
[`output.rs`](../../../../../control/crates/layerfs-sandbox/src/output.rs)).
Under a folder scope, `mount` takes only `main`, mounted as
`<namespace>.main`, and `exec` runs `cd '<folder>' && <command>`
([`calls/scope.rs`](../../../../../api/crates/layerfs-api/src/calls/scope.rs)).

## Errors

| Code | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad `ws`, `main` as the Branch, out of scope | [`workspace/serve.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) |
| `NoSuchSandbox`, `NoSuchBranch` | a missing Sandbox or Branch | [`family.rs`](../../../../../control/crates/layerfs-sandbox/src/family.rs), [`direct.rs`](../../../../../control/crates/layerfs-sandbox/src/direct.rs) |
| `Exists`, `Capacity` | `mount` with a name in use, or past 16 Workspaces | [`workspace/serve.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) |
| `NotMounted` | a `ws` whose path or name is not mounted | [`workspace/serve.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) |
| `Conflict` | `commit` conflicts: markers are written, nothing is committed (`detail.conflicts`, `written`) | [`daemon/values.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) |
| `Dirty` | `unmount` without `discard` while changes are uncommitted (`detail.changed`) | [`daemon/values.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) |
| `Busy`, `Unresolved` | a `sandbox.create` or `close` holds the Sandbox (not for `status`); a daemon connection not lent in time | [`op.rs`](../../../../../control/crates/layerfs-home/src/op.rs), [`daemon/pool.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs) |
| `StaleSandbox` | a saved daemon that is not `ControlReady` | [`daemon/pool.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs) |
| `TooLarge` | a request over one control record | [`daemon/call.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/call.rs) |
| `Unavailable` | the machine's worker is not recorded or does not answer | [`route.rs`](../../../../../control/crates/layerfs-sandbox/src/route.rs) |
| `Unknown` | **uncertain:** the worker's answer was lost after the request was sent | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |
| `Failed` | a daemon exchange that failed; the message carries the original `OperationFailure`, whose `attempted` flag says the request may have had effects (**may be uncertain**) | [`lifecycle.rs`](../../../../../control/crates/layerfs-sandbox/src/lifecycle.rs), [`daemon/failure.rs`](../../../../../control/crates/layerfs-sandbox/src/daemon/failure.rs) |

A daemon control refusal keeps its own code (`values::refusal`); any other
daemon exchange failure becomes `Failed`. A lost answer does not undo a
mutation the daemon published: after an uncertain `commit` or `unmount`,
read `workspace.status`, `sandbox.get` or `commit.list` before acting
again. See
[error kinds](../../call-table/errors.md#error-kinds) and
[uncertain outcomes](../../call-table/errors.md#uncertain-outcomes).

## Where it runs

```text
 workspace.<call>(ws)            caller's process
   | sandbox_of(record), record.free() (not for status)
   +-- forwarded Sandbox (direct::forwarded), call != exec
   |     direct::call ------------------------> Sandbox daemon
   |                                             (control channel, pool)
   +-- exec, or not forwarded
   |     route::on_machine
   |       Executor::Local --> sandbox::dispatch here --> daemon
   |       Executor::Worker -> "sandbox.host" -> worker --> daemon
   |                                     exec: Sandbox runtime, bash -c
   +-- status: route::reach, no machine start (P4)
```

Mount, commit, unmount and status reach the Sandbox's daemon; `exec` goes
through the Sandbox's runtime on its machine and the daemon runs no
command ([`exec.rs`](../../../../../control/crates/layerfs-sandbox/src/exec.rs)).
The daemon serves no Store calls. A mount's caller reads no Store: the
daemon finds the Branch by name as it binds (#64). A status's head read
takes the Store's own route
([`workspace/direct.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/direct.rs)).
See the [routing table](../../../control/routing/routing-table.md) and the
[daemon channel](../../../control/routing/daemon-channel.md).

## Example

Not run; written from the call table.

```js
const { ws } = await layerfs.workspace.mount("tests", "agent", { name: "build" });
const run = await layerfs.workspace.exec(ws, "make test", { stdout: (b) => process.stdout.write(b) });
await layerfs.workspace.commit(ws, { message: "tests pass" });
```

```python
ws = layerfs.workspace.mount("tests", "agent", name="build")["ws"]
run = layerfs.workspace.exec(ws, "make test", stream=True)
layerfs.workspace.commit(ws, message="tests pass")
```

```console
$ layerfs workspace mount tests agent --name build
$ layerfs workspace exec tests:build 'make test' --stream
$ layerfs workspace commit tests:build -m 'tests pass'
```

## Related

- [sandbox family](../sandbox/README.md).
- [Routing table](../../../control/routing/routing-table.md) and [Daemon channel](../../../control/routing/daemon-channel.md).
- [Engine: Daemon](../../../engine/daemon/README.md), [Engine: Workspace](../../../engine/workspace/README.md), [Engine: Commit](../../../engine/commit/README.md).
- [A Workspace session](../../../flows/workspace.md) and [A Commit](../../../flows/commit.md).

---

← [sandbox.close](../sandbox/close.md) · [Up: SDK](../README.md) · [workspace.mount](mount.md) →
