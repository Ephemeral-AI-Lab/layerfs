# workspace.exec

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [workspace](README.md) › exec

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Runs one `bash -c` command in a mounted Workspace's directory, through the
Sandbox's runtime, and answers its exit code and output when it ends. It
does not commit. The command has an exec id, the caller's or one control
makes, sent as the call's first progress event; while it runs, its record
lets any process end it with [workspace.exec_cancel](exec_cancel.md).

| | |
| --- | --- |
| Family | [workspace](README.md) |
| Library entry | [`workspace/exec.rs`](../../../../../api/crates/layerfs-api/src/workspace/exec.rs) |
| Implemented in | the library entry parses a typed `ExecRequest` → `layerfs_sandbox::workspace::exec::exec` ([`workspace/exec.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/exec.rs)) → host operation `workspace.exec` → `sandbox::exec::exec` ([`exec/run.rs`](../../../../../control/crates/layerfs-sandbox/src/exec/run.rs)) |
| Runs in | the Sandbox's runtime on its machine (here, or through its worker); never the daemon |

## Signature

```js
// JavaScript
await layerfs.workspace.exec(ws, command, { message, stream, stdout, stderr, exec_id, on_progress });
```

```python
# Python
layerfs.workspace.exec(ws, command, message=None, stream=None, stdout=None, stderr=None,
                       exec_id=None, on_progress=None)
```

```bash
# CLI
layerfs workspace exec <ws> <command> [--stream] [--message TEXT] [--exec-id ID] [--progress]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `ws` | Text | `<sandbox>:<path>` or `<sandbox>:<name>` from [workspace.mount](mount.md) | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs) `WorkspaceRef` |
| `command` | Text | run as `bash -c <command>` as the Sandbox's command identity, `HOME` its scratch folder (on macOS `native`, `RUSTUP_HOME`, `CARGO_HOME` and `GIT_CONFIG_GLOBAL` default to the caller's when present; override in the caller's environment or the command), in the Workspace directory | [`exec/run.rs`](../../../../../control/crates/layerfs-sandbox/src/exec/run.rs) |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `message` | Text | none | echoed in the answer; no Commit is made | [`exec/run.rs`](../../../../../control/crates/layerfs-sandbox/src/exec/run.rs) |
| `stream` | Bool | false | JS and Python: pass output to this process's stdout and stderr (when no functions are given); CLI `--stream` | [`node/src/index.js`](../../../../../api/node/src/index.js) |
| `stdout`, `stderr` | Function | none | called with each stream's text (bytes when not UTF-8); never reach the library | same, [`calls/check.rs`](../../../../../api/crates/layerfs-api/src/calls/check.rs) |
| `exec_id` | Text | made by control: `exec-` and 16 hex digits | the command's id: 1 to 64 of `A-Z a-z 0-9 . _ -`, not starting with `.` or `-`; unique among the Workspace's running commands | [`exec/id.rs`](../../../../../control/crates/layerfs-sandbox/src/exec/id.rs) |
| `on_progress` | Function | none | called once, before the command starts, with `{ exec: { id } }`; CLI `--progress` prints `{"progress":{"exec":{"id":…}}}` on stderr | [`workspace/exec.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/exec.rs) |

## Returns

```text
 fits:   { kind: "exit:<code>" | "cancelled", exit_code, stdout, stderr,
           message, truncated: false, exec_id, stop }
 over 1 MiB (either stream):
         { kind, exit_code, stdout: null | text, stderr: null | text,
           message, truncated: false, stdout_bytes, stderr_bytes,
           streamed: true }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `kind`, `exit_code` | text, int | `exit:<code>` and the code; `cancelled` when a [workspace.exec_cancel](exec_cancel.md) ended it (the code is then 128 + the signal) | [`exec/run.rs`](../../../../../control/crates/layerfs-sandbox/src/exec/run.rs), [`workspace/exec.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/exec.rs) |
| `exec_id` | text | the command's exec id | same |
| `stop` | text | how a cancel ends it: `cgroup` (`namespaces`: its own cgroup) or `group` (`host_process`: its own process group) | [`stop.rs`](../../../../../control/crates/layerfs-sandbox-backend/src/stop.rs) |
| `stdout`, `stderr` | text, bytes or null | the whole stream when it fits 1 MiB (`INLINE`); text when UTF-8, else bytes; null when it was spooled | [`output.rs`](../../../../../control/crates/layerfs-sandbox/src/output.rs) |
| `truncated` | bool | always false: no output is cut | same |
| `stdout_bytes`, `stderr_bytes` | int | both streams' sizes, when one was spooled | same |
| `streamed` | bool | a spooled stream was delivered as events, not in the answer | [`route.rs`](../../../../../control/crates/layerfs-sandbox/src/route.rs) `drain` |

```text
 command stdout --> 8 KiB window --> Spool: kept in memory <= 1 MiB
                                      | more --> unlinked temp file
                                      v        (bounded by free space)
 on the Sandbox's machine, after the command ends (route::drain):
   each spooled stream --> { stream, bytes } events, 1 MiB pages, in order
   spool released; answer keeps the stream null, adds streamed: true
 in this process (workspace::calls): one { stream, bytes } event per
   non-empty inline stream
 events --> stdout / stderr functions, or this process's own streams
            with stream: true (CLI --stream)
```

Output reaches the caller after the command ends, never while it runs.
With no `stdout`/`stderr` function and no `stream`, a spooled stream's
events are dropped by the front end and only its size is answered; pass a
function or `stream: true` to receive output over 1 MiB
([`node/src/index.js`](../../../../../api/node/src/index.js)).
The two streams are delivered one after the other (all of stdout, then all
of stderr), not interleaved, and each event is text or bytes by its own
content. [Events](../../call-table/events.md#workspaceexec-output) states
the contract and marks what was not run.

## Where it runs

```text
 caller process                Sandbox's machine (here | worker)
 --------------                ---------------------------------
 WorkspaceRef::parse; sandbox_of; record.free
 exec id: the caller's, or minted; progress { exec: { id } }
 forwarded? exec_directory checked here (Workspace records);
   this home keeps the command's record (route machine:<name>)
 on_machine -----------------> dispatch "workspace.exec" (+ execId)
                                 by_name: name -> path
                                 record exec/<id>.json, create-new
                                 (not when the caller keeps it)
                                 runtime: bash -c <command>, as the
                                 command identity, in the directory,
                                 in its own cgroup | process group;
                                 the stop handle goes in the record
                                 once the command runs (forwarded:
                                 event { exec_started: { stop } })
                                 record removed when it ends
                                 drain spooled streams as events
                               <-- exit code + inline streams + events
 output events (inline streams)
```

The record is `sandbox/<name>/workspace/<workspace id>/exec/<exec id>.json`
on the side that keeps the Workspace's own record, and names its owner
process, its route and its stop handle
([`exec/record.rs`](../../../../../control/crates/layerfs-sandbox/src/exec/record.rs)).
While a record is live, [workspace.unmount](unmount.md) and
[sandbox.close](../sandbox/close.md) are refused `Busy` unless `force`
cancels the command first. A record whose owner process is gone is stale:
the next call that finds it ends its command, removes it and logs it in
`log/exec.jsonl`, and `layerfs doctor` reports it.

The daemon supervises no command: it only serves the filesystem the
command uses. There is no runtime timeout, no implicit Commit and no
automatic unmount. See the [routing table](../../../control/routing/routing-table.md)
and [Control: commands and streams](../../../control/sandboxes/commands-and-streams.md).
Under a folder scope the command runs as `cd '.<folder>' && <command>`.
That is a prefix, not confinement: a scope guards call arguments, not
processes, and the command can still read and write anywhere in the mounted
tree ([Scopes](../../call-table/table.md#scopes)).

## Errors

A non-zero exit is not an error: it is `kind: "exit:<code>"`.

| Code | When | Uncertain? | Source |
| --- | --- | --- | --- |
| `InvalidInput` | a bad `ws`, option or `exec_id`; out of scope | no | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs), [`exec/id.rs`](../../../../../control/crates/layerfs-sandbox/src/exec/id.rs) |
| `Exists` | a running command of the Workspace already has this `exec_id` | no | [`exec/record.rs`](../../../../../control/crates/layerfs-sandbox/src/exec/record.rs) `Running::start` |
| `NoSuchSandbox`, `NotMounted` | no such Sandbox; the path or name is not mounted | no | [`workspace/serve.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) |
| `Busy`, `Unresolved` | a `sandbox.create` or `close` holds the Sandbox | no | [`record/sandbox.rs`](../../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs) |
| `Unavailable` | the worker is not recorded or does not answer | no | [`route.rs`](../../../../../control/crates/layerfs-sandbox/src/route.rs) |
| `Failed` | the runtime could not run the command, or spooling or draining failed | no | [`exec/run.rs`](../../../../../control/crates/layerfs-sandbox/src/exec/run.rs) |
| `Unknown` | **the worker's answer was lost; the command may have run, in full or in part** | **yes** | [`worker/client.rs`](../../../../../control/crates/layerfs-machine/src/worker/client.rs) |

**After `Unknown`**, the command's file changes may be in the Workspace.
Read [workspace.status](status.md) before running it again. A lost reply
does not undo what the command wrote.

## Examples

Not run; written from the call table.

```js
const r = await layerfs.workspace.exec(ws, "npm ci && npm test", { stdout: (t) => process.stdout.write(t) });
if (r.exit_code !== 0) console.error(r.stderr);
```

```python
r = layerfs.workspace.exec(ws, "make build", stream=True)
print(r["exit_code"])
```

```bash
layerfs workspace exec s1:build 'make test' --stream
layerfs workspace exec s1:build 'npm run dev' --exec-id dev &
layerfs workspace exec-cancel s1:build dev
```

## Related

- Before: [workspace.mount](mount.md). After: [workspace.status](status.md),
  [workspace.commit](commit.md). To end it early: [workspace.exec_cancel](exec_cancel.md).
- [A Workspace session](../../../flows/workspace.md),
  [Control: sandboxes](../../../control/sandboxes/README.md).

---

← [workspace.mount](mount.md) · [Up: workspace](README.md) · [workspace.exec_cancel](exec_cancel.md) →

## CLI framed output

`layerfs workspace exec <ws> <command> --events` delivers every output byte
as NDJSON output events, followed by one `{result}` or `{refusal}` frame.
It includes progress and is exclusive with `--stream` and `--text`. A result
exits 0 regardless of the command's exit code; refusal exits 125 and usage
exits 2. Bytes use UTF-8 text or `{base64: ...}`. A failed stdout exits 125
with effects kept and delivery incomplete; never replay the operation.

### Cancellation before registration

A cancel with no registered command retains a pending mark for 30 seconds
while answering `ended`. `Running::start` consumes it under the Sandbox lock
and ends the command at spawn (`kind: "cancelled"`, exit code 128 + the signal).
The mark
is local to the Workspace and id, removed when consumed and ignored after
expiration. Reuse after consumption is unaffected. An unconsumed mark may
cancel reuse within that interval; use unique ids. Expired unconsumed files
are removed on access or Workspace teardown. No call is retried or replayed.

At-spawn cancellation can finish under SIGTERM before SIGKILL. The exec
reports the actual 128 + signal exit (143 or 137), with `kind: "cancelled"`.
