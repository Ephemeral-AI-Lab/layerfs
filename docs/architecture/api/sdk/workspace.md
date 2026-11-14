# workspace family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `workspace` calls: mount a Branch as a writable tree in a running Sandbox, run and cancel commands in it, commit it, read its status and unmount it, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/workspace`](../../../../api/crates/layerfs-api/src/workspace), [`layerfs-sandbox/src/workspace`](../../../../control/crates/layerfs-sandbox/src/workspace), [`layerfs-sandbox/src/exec`](../../../../control/crates/layerfs-sandbox/src/exec), [`daemon/values.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) |
| Runs by default | records in the caller's process; `mount`, `commit`, `unmount`, `status` reach the Sandbox's daemon (directly when its port is forwarded, else through the machine: this process or its worker, `sandbox.host`); `exec` and `exec_cancel` run in the Sandbox's runtime on its machine, never in the daemon |
| Design | [Sandboxes: daemon channel and exec](../../control/sandboxes/daemon-channel-and-exec.md), [Engine: daemon](../../engine/daemon/README.md), [Commit](../../engine/commit/README.md), [Workspace flow](../../flows/workspace.md), [Commit flow](../../flows/commit.md) |

## Calls

```text
 sandbox.create (running Sandbox)      branch.fork (a Branch, not main)
            \                              /
             mount <sandbox> <branch> --> ws = <sandbox>:<path|name>
                |
   +---> exec <ws> <command>   (bash -c in the Sandbox runtime)
   |        |   exec_cancel <ws> <exec_id>  (from any process)
   |     status <ws>           (base, head moved?, activity, overlay)
   +---- commit <ws>           (one Commit; merges a moved head)
            v
         unmount <ws>          (Dirty unless committed or discard)
```

| Call | Host operation | Returns |
| --- | --- | --- |
| [`workspace.mount`](#workspacemount) | `workspace.mount` | `{ kind: "Mounted", …, ws }` |
| [`workspace.exec`](#workspaceexec) | `workspace.exec` | `{ kind: "exit:<code>" \| "cancelled", exit_code, stdout, stderr, … }` |
| [`workspace.exec_cancel`](#workspaceexec_cancel) | `workspace.exec_cancel` | `{ outcome: "cancelled" \| "ended" \| "unknown", signal? }` |
| [`workspace.commit`](#workspacecommit) | `workspace.commit` | `Committed` or `UpToDate` |
| [`workspace.unmount`](#workspaceunmount) | `workspace.unmount` | `Unmounted` or `ForceUnmounted` |
| [`workspace.status`](#workspacestatus) | `backend.daemon.workspace.status` | `{ kind: "ok", base, branch_head, activity, overlay, … }` |

A Workspace is named by `ws`: `<sandbox>:<path>`, or `<sandbox>:<name>`
when its mount named it (`WorkspaceRef`,
[`names.rs`](../../../../control/crates/layerfs-home/src/names.rs)). A
Sandbox holds at most 16 Workspaces (`WORKSPACES`,
[`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs)).
Under a folder scope, `mount` takes only `main`, mounted as the Branch
`<namespace>.main`; `exec` runs `cd '.<folder>' && <command>` (a prefix,
not confinement); and the Sandbox must belong to the scope's project
([`calls/scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs)).

### workspace.mount

Mounts a Branch as a new Workspace, a complete writable view of its head.

```text
JS      await layerfs.workspace.mount(sandbox, branch, { name })
Python  layerfs.workspace.mount(sandbox, branch, *, name=None)
CLI     layerfs workspace mount <sandbox> <branch> [--name <name>]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `sandbox` | Name | a running Sandbox; a mount never starts one |
| `branch` | Name | a side Branch of the Sandbox's project; `main` is `InvalidInput` (fork from it) |
| `name` option | Name | unique among the Sandbox's Workspaces; `ws` becomes `<sandbox>:<name>` |

```text
{ kind: "Mounted", path, workspace, namespace, branch,
  base: <head Commit> | null, base_layer, root,
  sandbox, container, image, platform | null, runtime, name | null, ws }
```

The caller reads no Store: it sends the project and Branch names and the
daemon finds the Branch as it binds (`Request::MountNamed`). The mount binds
the whole root without a scan or copy; its only new file is the Workspace's
overlay database, copied from the template. It is recorded as
`workspace/<id>.json` in the Sandbox's folder
([`workspace/serve.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs)).
Errors: `NoSuchProject`, `NoSuchBranch` (the daemon's lookup,
[`store/bind.rs`](../../../../engine/crates/layerfs-store-service/src/store/bind.rs));
`Exists` (name in use); `Capacity` (16 Workspaces). After an uncertain mount,
`sandbox.get` lists what is mounted; a mount that landed takes a place.

### workspace.exec

Runs one `bash -c` command in the Workspace's directory and answers its
exit code and output when it ends. It does not commit.

```text
JS      await layerfs.workspace.exec(ws, command, { message, stream, stdout, stderr, exec_id, on_progress })
Python  layerfs.workspace.exec(ws, command, *, message=None, stream=None, stdout=None, stderr=None, exec_id=None, on_progress=None)
CLI     layerfs workspace exec <ws> <command> [--message <message>] [--stream] [--exec-id <exec-id>] [--progress] [--events]
```

| Argument | Kind | Rule |
| --- | --- | --- |
| `command` | Text | `bash -c <command>` as the Sandbox's command identity, `HOME` its scratch folder (on macOS `native`, `RUSTUP_HOME`, `CARGO_HOME` and `GIT_CONFIG_GLOBAL` default to the caller's when present; override in the caller's environment or the command) ([`exec/run.rs`](../../../../control/crates/layerfs-sandbox/src/exec/run.rs)) |
| `message` option | Text | echoed in the answer; no Commit is made |
| `stream` option | Bool | pass output to this process's stdout and stderr when no functions are given (CLI `--stream`) |
| `stdout`, `stderr` options | Function | receive each output event's text, or bytes when not UTF-8; never reach the library |
| `exec_id` option | Text | 1–64 of `A-Z a-z 0-9 . _ -`, not starting with `.` or `-`, unique among the Workspace's running commands; default `exec-<16 hex>` ([`exec/id.rs`](../../../../control/crates/layerfs-sandbox/src/exec/id.rs)) |
| `on_progress` option | Function | one event before the command starts: `{ exec: { id } }` |

```text
fits:  { kind: "exit:<code>" | "cancelled", exit_code, stdout, stderr,
         message, truncated: false, exec_id, stop }
spooled (a stream over 1 MiB):
       { ..., stdout: null | text, stderr: null | text,
         stdout_bytes, stderr_bytes, streamed: true }
```

A non-zero exit is not an error. `cancelled` means `exec_cancel` ended it;
the code is then 128 + the signal. `stop` is how a cancel ends it: `cgroup`
(`namespaces`) or `group` (`host_process`,
[`stop.rs`](../../../../control/crates/layerfs-sandbox-backend/src/stop.rs)).
Output reaches the caller after the command ends; how it is spooled, paged
and ordered is in [errors and events](../calls/errors-and-events.md#workspaceexec-output).
While the command runs, its record
`sandbox/<name>/workspace/<id>/exec/<exec id>.json` lets any process cancel
it and makes `unmount` and `sandbox.close` refuse `Busy` unless `force`; a
record whose owner process is gone is stale, ended and removed by the next
call that finds it, logged in `log/exec.jsonl`
([`exec/record.rs`](../../../../control/crates/layerfs-sandbox/src/exec/record.rs)).
There is no runtime timeout, no implicit Commit, no automatic unmount.

`--events` (CLI only) writes every output byte as NDJSON output events,
then one `{result}` or `{refusal}` frame; it includes progress and excludes
`--stream` and `--text`. A result exits 0 whatever the command's code
([`framed.rs`](../../../../api/crates/layerfs-cli/src/framed.rs)).

| Error | When |
| --- | --- |
| `Exists` | a running command of the Workspace has this `exec_id` |
| `Failed` | the runtime could not run the command, or spooling or draining failed |
| `Unknown` | the worker's answer was lost: the command may have run in full or in part; read `workspace.status` before running it again |

### workspace.exec_cancel

Ends a running command from any process: SIGTERM to everything it started,
then SIGKILL after `grace_ms`. One explicit attempt.

```text
JS      await layerfs.workspace.exec_cancel(ws, exec_id, { grace_ms })
Python  layerfs.workspace.exec_cancel(ws, exec_id, *, grace_ms=None)
CLI     layerfs workspace exec-cancel <ws> <exec-id> [--grace-ms <grace-ms>]
```

`grace_ms` defaults to 5000 (`GRACE_MS`), not negative; 0 sends SIGKILL right
after SIGTERM ([`exec/cancel.rs`](../../../../control/crates/layerfs-sandbox/src/exec/cancel.rs)).

| Answer | Meaning |
| --- | --- |
| `{ outcome: "cancelled", signal }` | `SIGTERM` when the command was gone within the grace, else `SIGKILL`; a command not yet running is marked and answered `SIGKILL`, and its owner ends it once it runs |
| `{ outcome: "ended" }` | no record, or nothing left; with no record, a pending mark is kept for 30 s and cancels a command registered with that id in that time ([`exec/pending.rs`](../../../../control/crates/layerfs-sandbox/src/exec/pending.rs)) |
| `{ outcome: "unknown" }` | the worker was not reached or did not answer; call again once it answers |

On `namespaces` each command runs in its own cgroup
`<sandbox cgroup>/ws-<workspace id>/exec-<exec id>`, so children that left
its process group end too; on `host_process` (macOS `native`) it is the
command's process group, which a child calling `setsid` escapes. A worker
that takes the cancel and never answers leaves the call waiting until its
connection closes: there is no operation timeout. Error: `Failed` (a record
that does not read, or the signal could not be sent).

### workspace.commit

Commits everything published in the Workspace so far, from every process
that wrote to it, as one Commit on its Branch; a moved head is merged first.
The Workspace stays mounted.

```text
JS      await layerfs.workspace.commit(ws, { message, force })
Python  layerfs.workspace.commit(ws, *, message=None, force=None)
CLI     layerfs workspace commit <ws> [--message <message>] [--force]
```

`force` publishes over a moved head instead of merging (the head is
displaced).

```text
Committed: { kind: "Committed", head, commit, displaced_head | null,
             merged: null | { from, head }, lww, lww_truncated }
UpToDate:  { kind: "UpToDate", head | null, root, merged, lww, lww_truncated }
```

The daemon captures the Workspace's published frontier, whichever process
wrote it (not filtered by exec), saves, merges a moved head and publishes;
only the publish step holds the Store's lane, and later writes stay in the
Workspace ([`daemon/values.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) `commit`).

| Error | When |
| --- | --- |
| `Conflict` | the head moved and files conflict: markers are written into the Workspace and nothing is committed; or files still hold markers (`detail.head`, `base`, `markers`, `conflicts`, `written`, `lww`) |
| `Busy` | another control operation runs on the Workspace ([`control/registry.rs`](../../../../engine/crates/layerfs-daemon/src/control/registry.rs) `idle`) |
| `Unknown`, `detail.phase: "admission"` | an earlier Commit's install was uncertain or failed; the daemon keeps that custody and refuses new Commits |

After an uncertain Commit read `commit.list` (a new head?) and
`workspace.status` (`activity`, `base`, `pending_merge`). A lost reply does
not remove a published Commit.

### workspace.unmount

Ends a Workspace: the daemon drains its requests, detaches the mount,
closes its overlay connection and deletes its overlay file.

```text
JS      await layerfs.workspace.unmount(ws, { discard, force })
Python  layerfs.workspace.unmount(ws, *, discard=None, force=None)
CLI     layerfs workspace unmount <ws> [--discard] [--force]
```

| Option | Effect |
| --- | --- |
| `discard` | drop uncommitted changes instead of refusing `Dirty` |
| `force` | cancel each running command first (5000 ms grace), then one abort write ends the connection, also after a normal unmount stalled; uncommitted changes are dropped |

```text
normal: { kind: "Unmounted" }
force:  { kind: "ForceUnmounted", abort, detach, cleanup }
either may add cleanup_error (daemon succeeded; a local record stayed)
```

The machine writes `workspace/<id>.unmount` before it calls the daemon and
records the outcome after, so a repeated unmount answers what the first one
established without dialling again; a Workspace the daemon no longer knows
answers `Unmounted`
([`unmount_guard.rs`](../../../../control/crates/layerfs-sandbox/src/unmount_guard.rs)).

| Error | When |
| --- | --- |
| `Dirty` | uncommitted changes and no `discard` (`detail.changed`, `pending_merge`, `truncated`) |
| `Busy` | without `force`, commands run (`detail.execs`); another unmount is in progress; a process holds the mount; another control operation runs |
| `Unknown` | the worker's answer was lost; or with `force` a command's cancel was not confirmed (`detail.execs`). Call `unmount` again: a recorded outcome is answered as it was |

### workspace.status

A Workspace's base, whether its Branch head moved since the mount, what it is
doing and how many changes it holds. Changes nothing.

```text
JS      await layerfs.workspace.status(ws)
Python  layerfs.workspace.status(ws)
CLI     layerfs workspace status <ws>
```

```text
{ kind: "ok", base | null, base_layer, branch, branch_head | null,
  head_moved_since_mount | null, activity,
  overlay: { inodes, entries, pending_merge } | null }
```

| Field | Meaning |
| --- | --- |
| `branch_head`, `head_moved_since_mount` | the Branch head now, read on the Store's own route by the caller; null when that read failed |
| `activity` | `Idle`, `Committing`, `Closing`, `Attaching`, `Uncertain` (an unknown outcome keeps custody; Commit and unmount are refused) or `LocalFailure` (published, but the local install did not finish) ([`control_types.rs`](../../../../engine/crates/layerfs-bridge/src/control_types.rs)) |
| `overlay` | uncaptured changed inodes and entries; `pending_merge` when the overlay's base root is not the binding's root |

A read: answered while a `sandbox.create` or `close` holds the Sandbox, and
it starts no machine. It is the read after an uncertain `exec` or `commit`.

## Family errors

| Error | When | Source |
| --- | --- | --- |
| `InvalidInput` | a bad `ws`, option or `exec_id`; `main` as the Branch; out of scope | [`workspace/serve.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) |
| `NoSuchSandbox`, `NotMounted` | no such Sandbox (or its daemon ended itself when idle); the path or name is not mounted | same |
| `Busy`, `Unresolved` | a `sandbox.create` or `close` holds the Sandbox (not for `status`); no daemon connection lent in time | [`record/sandbox.rs`](../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs), [`daemon/pool.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs) |
| `StaleSandbox` | a saved daemon that is not `ControlReady` | `daemon/pool.rs` |
| `TooLarge` | a request over one control record | [`daemon/call.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/call.rs) |
| `Unavailable` | the machine's worker is not recorded or does not answer | [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) |
| daemon codes | a daemon control refusal keeps its own code (`Busy`, `HeadMoved`, `Missing`, `Capacity`, `Invalid`, `Failed`, `Unknown`) with `detail.phase` | `daemon/values.rs` `refusal` |
| `Failed` | a daemon exchange failed; the message carries the `OperationFailure`, whose `attempted` says the request may have had effects | [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs), [`daemon/failure.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/failure.rs) |
| `Unknown` | the worker's answer was lost after the request was sent | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) |

## Related

- [sandbox family](sandbox.md), [commit family](commit.md).
- [Daemon channel and exec](../../control/sandboxes/daemon-channel-and-exec.md), [bridge](../../engine/bridge/README.md).
- [Errors and events](../calls/errors-and-events.md).
