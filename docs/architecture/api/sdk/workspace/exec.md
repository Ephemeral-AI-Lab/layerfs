# workspace.exec

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [workspace](README.md) › exec

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Runs one `bash -c` command in a mounted Workspace's directory, through the
Sandbox's runtime, and answers its exit code and output when it ends. It
does not commit.

| | |
| --- | --- |
| Family | [workspace](README.md) |
| Library entry | [`workspace/exec.rs`](../../../../../api/crates/layerfs-api/src/workspace/exec.rs) |
| Implemented in | the library entry parses a typed `ExecRequest` → `layerfs_sandbox::workspace::calls::exec` → host operation `workspace.exec` → `sandbox::exec::exec` ([`exec.rs`](../../../../../control/crates/layerfs-sandbox/src/exec.rs)) |
| Runs in | the Sandbox's runtime on its machine (here, or through its worker); never the daemon |

## Signature

```js
// JavaScript
await layerfs.workspace.exec(ws, command, { message, stream, stdout, stderr });
```

```python
# Python
layerfs.workspace.exec(ws, command, message=None, stream=None, stdout=None, stderr=None)
```

```bash
# CLI
layerfs workspace exec <ws> <command> [--stream] [--message TEXT]
```

## Arguments

| Name | Kind | Rule | Source |
| --- | --- | --- | --- |
| `ws` | Text | `<sandbox>:<path>` or `<sandbox>:<name>` from [workspace.mount](mount.md) | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs) `WorkspaceRef` |
| `command` | Text | run as `bash -c <command>` as the Sandbox's command identity, `HOME` its scratch folder, in the Workspace directory | [`exec.rs`](../../../../../control/crates/layerfs-sandbox/src/exec.rs) |

## Options

| Name | Kind | Default | Rule | Source |
| --- | --- | --- | --- | --- |
| `message` | Text | none | echoed in the answer; no Commit is made | [`exec.rs`](../../../../../control/crates/layerfs-sandbox/src/exec.rs) |
| `stream` | Bool | false | JS and Python: pass output to this process's stdout and stderr (when no functions are given); CLI `--stream` | [`node/src/index.js`](../../../../../api/node/src/index.js) |
| `stdout`, `stderr` | Function | none | called with each stream's text (bytes when not UTF-8); never reach the library | same, [`calls/check.rs`](../../../../../api/crates/layerfs-api/src/calls/check.rs) |

## Returns

```text
 fits:   { kind: "exit:<code>", exit_code, stdout, stderr, message,
           truncated: false }
 over 1 MiB (either stream):
         { kind, exit_code, stdout: null | text, stderr: null | text,
           message, truncated: false, stdout_bytes, stderr_bytes,
           streamed: true }
```

| Field | Kind | Meaning | Source |
| --- | --- | --- | --- |
| `kind`, `exit_code` | text, int | `exit:<code>` and the code | [`exec.rs`](../../../../../control/crates/layerfs-sandbox/src/exec.rs) |
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

## Where it runs

```text
 caller process                Sandbox's machine (here | worker)
 --------------                ---------------------------------
 WorkspaceRef::parse; sandbox_of; record.free
 forwarded? exec_directory checked here (Workspace records)
 on_machine -----------------> dispatch "workspace.exec"
                                 by_name: name -> path
                                 runtime: bash -c <command>, as the
                                 command identity, in the directory
                                 drain spooled streams as events
                               <-- exit code + inline streams + events
 output events (inline streams)
```

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
| `InvalidInput` | a bad `ws` or option; out of scope | no | [`names.rs`](../../../../../control/crates/layerfs-home/src/names.rs) |
| `NoSuchSandbox`, `NotMounted` | no such Sandbox; the path or name is not mounted | no | [`workspace/serve.rs`](../../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) |
| `Busy`, `Unresolved` | a `sandbox.create` or `close` holds the Sandbox | no | [`record/sandbox.rs`](../../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs) |
| `Unavailable` | the worker is not recorded or does not answer | no | [`route.rs`](../../../../../control/crates/layerfs-sandbox/src/route.rs) |
| `Failed` | the runtime could not run the command, or spooling or draining failed | no | [`exec.rs`](../../../../../control/crates/layerfs-sandbox/src/exec.rs) |
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
```

## Related

- Before: [workspace.mount](mount.md). After: [workspace.status](status.md),
  [workspace.commit](commit.md).
- [A Workspace session](../../../flows/workspace.md),
  [Control: sandboxes](../../../control/sandboxes/README.md).

---

← [workspace.mount](mount.md) · [Up: workspace](README.md) · [workspace.commit](commit.md) →
