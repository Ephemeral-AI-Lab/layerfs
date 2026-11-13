# Daemon channel and exec

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The caller side of [Sandboxes](README.md) in `layerfs-sandbox`: how a
Workspace call reaches the daemon, how a command runs and is cancelled, and
how a host operation is routed to the side that serves the Sandbox.

| | |
| --- | --- |
| Source | [`daemon/`](../../../../control/crates/layerfs-sandbox/src/daemon), [`exec/`](../../../../control/crates/layerfs-sandbox/src/exec), [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs), [`host/`](../../../../control/crates/layerfs-sandbox/src/host), [`direct.rs`](../../../../control/crates/layerfs-sandbox/src/direct.rs) |

The request and reply records, their size limits, the handshake and the
daemon's admission notice are the [Bridge](../../engine/bridge/README.md)'s;
what the daemon does with each request is in
[Daemon: control calls](../../engine/daemon/control-calls.md). This page names
requests only.

## `daemon/`: the control channel

```text
 sandbox/<name>/control.json  {endpoint, secret, peer, instance}   0600
        |
        v
 Dial.open: TCP connect, authenticated handshake, Hello{expected_instance}
        |   (10 s connect/read/write while opening; cleared once ready)
        |   anything but ControlReady -> StaleSandbox
        v
 Controls (one per Sandbox, in this process)
   lend(): an idle Control --> else open one more (open < limit)
           --> else wait for a free one, <= 60 s --> Busy
   Lease dropped: failed or fenced Control closed, open -= 1;
                  healthy Control back to idle
        |
        v  one exchange at a time per Control
 Control.call(request): Encode -> Send -> Receive -> Validate, sent once
```

| Part | What it is | Source |
| --- | --- | --- |
| `Control` | One sequential authenticated connection; numbers its calls. Once an exchange fails, its session ends or a fence closes its channel, it is quarantined and refuses every later call before sending. While its thread is traced, requests travel as traced observations. | [`daemon/channel.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/channel.rs) |
| `Dial` | Endpoint, secret, peer key and instance of one daemon. `open_waiting` (a process's first connection) re-opens while the daemon's refusal is marked `retryable`, pausing 20 ms doubling to 500 ms, for up to 60 s (`ADMISSION_WAIT`): a readiness wait, nothing is sent before admission. | [`daemon/pool.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs) |
| `Controls`, `Lease` | Up to 6 connections per process (`CONNECTIONS`); the daemon admits 8 (`DAEMON_CONNECTIONS`), leaving the rest to other processes. When the daemon admits no more, the limit drops to the connections open and callers take turns. | `daemon/pool.rs`, [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) |
| `Traced` | A lease whose wait is the call's queue time, and whose replies' telemetry is collected into the call trace. | [`daemon/trace.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/trace.rs) |
| `Fence` | One absolute deadline a connection cannot outlive: a thread shuts the socket (or the channel's close handle) at it, which quarantines the connection. | [`daemon/fence.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/fence.rs) |
| `WorkspaceApi` | The Workspace calls on one borrowed `Control`; owns no process. | [`daemon/workspace/`](../../../../control/crates/layerfs-sandbox/src/daemon/workspace) |
| `operator::call` | Admits exactly `sandbox.list_workspaces`, `backend.daemon.connection_failures` and `backend.daemon.reclaim`; any other method is `UnknownMethod` before sending. | [`daemon/operator.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/operator.rs) |

The pool exists because the daemon serves each authenticated connection on
its own worker: one shared connection would make every call wait behind the
one before it.

| Call | Requests sent | Answer | Source |
| --- | --- | --- | --- |
| `workspace.mount` | `MountNamed` (the daemon finds the Branch by project and Branch name; the caller reads no Store), then `Attach`: two acknowledged steps. A failed attach leaves the Workspace bound (`MountFailure.bound`). | `Mounted` | [`daemon/workspace/mount.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/workspace/mount.rs), [`recovery.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/workspace/recovery.rs) |
| `workspace.commit` | `Commit` (`force`, message) | `Committed` or `UpToDate`, merged (`from`, `head`), or `Conflict` | [`daemon/workspace/commit.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/workspace/commit.rs), [`daemon/values.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) |
| `workspace.unmount` | `Unmount{discard}`, or `ForceUnmount` with `force` | `Unmounted`, `Dirty`, or `ForceUnmounted` (`abort`, `detach`, `cleanup`) | [`daemon/workspace/unmount.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/workspace/unmount.rs) |
| `workspace.status` | `Status(token)`; the Branch head is read on the Store's own route | status map | [`daemon/workspace/status.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/workspace/status.rs) |
| lost-reply recovery | `Locate`, `Cleanup`: read-only observations; bind, attach and remove nothing | observation | `recovery.rs` |
| `sandbox.get` | `sandbox.list_workspaces`, then on the same lent connection `backend.daemon.connection_failures`, bounded to 2 s and fenced; a window admission used up refuses the read unsent | `daemon_capacity` | [`daemon/capacity.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/capacity.rs) |

| Failure | Becomes | Outcome known? | Source |
| --- | --- | --- | --- |
| Daemon refusal (`Reply::Refused`) | its control code (`Busy`, `HeadMoved`, `Missing`, `Capacity`, `Invalid`, `Failed`, `Unknown`), `detail.phase` | yes, except `Unknown` | `refusal` in [`daemon/values.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) |
| Sent, connection ended before the reply (phase `Receive`) | `Unavailable`, `outcome: "unknown"`, `retryable: false` | no | same |
| Other transport or protocol failure, a stopped terminal step (`Retained`), a reply of the wrong kind | `Failed`, with phase and `attempted` in the message | no once `attempted` | [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) `operation`, [`daemon/failure.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/failure.rs) |
| SDK method over one control record, before sending | `TooLarge` | yes: not sent | [`daemon/call.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/call.rs) |
| Open refused: capacity notice, closed before admission, reset | `Capacity`, `Unknown` or `Unavailable`, each `retryable: true`; nothing attempted | yes | `daemon/pool.rs` |
| No free connection in 60 s | `Busy`, "nothing was attempted" | yes | `daemon/pool.rs` |
| Daemon not `ControlReady`, or another instance | `StaleSandbox` | yes | `daemon/pool.rs` |
| Commit conflict | `Conflict` with `head`, `base`, `markers`, `conflicts`, `written` | yes: nothing committed | `commit` in `daemon/values.rs` |
| Unmount with uncommitted changes | `Dirty` with `changed`, `pendingMerge` | yes | `unmounted` in `daemon/values.rs` |

No step is retried, rebound or re-attached on the caller's behalf.

## `exec/`: commands and cancel

```text
 workspace.exec(ws, command, { exec_id? })
   ExecId: caller's, or "exec-<16 hex>"; first progress event {exec:{id}}
   Running::start: exec record created new under the Sandbox lock
   ExecRequest { identity, ["bash","-c",command], ["HOME=<scratch>"],
                 directory: the Workspace path, stdin: false }
        |
        v
 Sandbox::run_in(request, ExecSlot{workspace, exec, spawned}, sinks)
   spawned(stop) --> Running::spawned: stop handle written to the record
   stdout/stderr --> Spool (1 MiB inline each, then a file)
        |
        v
 Running::end: record removed; kind "cancelled" if marked and code > 128,
               else "exit:<code>"; stdout/stderr also sent as events
```

| Item | Rule | Source |
| --- | --- | --- |
| Exec record | `<sandbox folder>/workspace/<workspace id>/exec/<exec id>.json`: `{ v, id, workspace, path, name, owner, route, stop, started, cancel? }`. `owner` is pid, start time and boot; `route` is `local` or `machine:<name>`; `stop` is null until spawn. Kept on the side that keeps the Workspace's record. | [`exec/record.rs`](../../../../control/crates/layerfs-sandbox/src/exec/record.rs) |
| Duplicate id | A live record of the id is `Exists`, found by path, with no scan. | `Running::start` |
| Stop handle | `{ cgroup: { path } }` (namespaces: `<sandbox cgroup>/ws-<workspace>/exec-<exec>`) or `{ group: { pgid, pid, pid_start, pid_boot } }` (native); written only once the command runs in it. | `stop_value` in `exec/record.rs`, [Backends](backends.md#stoprs-stop-handles) |
| Output | Each stream kept in memory up to 1 MiB (`INLINE`), then spilled to an unlinked file; read back in pages of at most 1 MiB until released. The backend reads the pipe only as fast as the spool takes it (8 KiB windows). `truncated` is always false. | [`output.rs`](../../../../control/crates/layerfs-sandbox/src/output.rs), [`exec/run.rs`](../../../../control/crates/layerfs-sandbox/src/exec/run.rs) |
| Exit | The root process's: its code, or 128 + the signal. Descendants, descriptors and filesystem drain are separate facts. | [Backends](backends.md#exit-codes) |

| Cancel case | What happens | Source |
| --- | --- | --- |
| Live record, handle known | Marked (`cancel`) under the Sandbox lock, then signalled once outside it: SIGTERM, the grace (`grace_ms`, default 5000), then the kill. Answer `{ outcome: "cancelled", signal }`. | `cancel` in [`exec/cancel.rs`](../../../../control/crates/layerfs-sandbox/src/exec/cancel.rs), [`stop.rs`](../../../../control/crates/layerfs-sandbox-backend/src/stop.rs) |
| Live record, no handle yet | Marked only; answered `cancelled` with `SIGKILL`. Told the handle, the owner finds the mark (`Running::spawned` refuses as interrupted) and the backend ends the command at once; its exec answers `kind: "cancelled"` with 128 + the signal. | `exec/cancel.rs`, `Running::spawned` |
| No record yet | A pending mark beside the record path, answered `ended`. The next `Running::start` of that id within 30 s consumes it and the command is ended at spawn; an expired or clock-skewed mark is ignored. Use unique ids. | [`exec/pending.rs`](../../../../control/crates/layerfs-sandbox/src/exec/pending.rs) |
| Nothing left running | `ended`, no signal. | `stop.rs` |
| Stale record (owner gone), route `local` | Ended through its handle with zero grace, removed, logged to `log/exec.jsonl`, by the next start, cancel, unmount or close that finds it. | `stale` |
| Stale record, route `machine:<name>` | Claimed under the lock (owner set to this process, marked), then `workspace.exec_cancel` sent to the worker with zero grace and without the lock. `NoSuchSandbox` there confirms `ended`; unreached or another refusal is `unknown`, the record is put back and the id stays taken (`Exists`). | `claim`, `stale_remote`, [`exec/remote.rs`](../../../../control/crates/layerfs-sandbox/src/exec/remote.rs) |
| Unmount or close with records | Live ones refuse `Busy` (`detail.execs`); `force` cancels each with the default grace; any not confirmed refuses `Unknown`. A terminal unmount then removes `workspace/<workspace id>/`. | `settle`, [`workspace/serve.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) |
| Worker not reached | `{ outcome: "unknown" }`: nothing is known to have changed. | [`workspace/exec.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/exec.rs) `exec_cancel` |

Cancel is one explicit attempt: nothing times a command out, retries or
replays. A cancel at spawn may end the command under SIGTERM (143) or SIGKILL
(137); the exec reports the actual 128 + signal.

## `route.rs` and `host/`: the host operation

```text
 caller: on_machine / reach(record, Call)
   resolve project -> Store kind; PostgreSQL URL resolved here (password
   included, loopback host rewritten to the machine's alias), never written
   place::on_machine(machine)
     |                                 |
 Executor::Local                  Executor::Worker
   Store lease held                 frame Sent{call, message, sandbox,
   host_args -> Host                  store/project records, choice,
   local(Host, Call)                  postgresql} --"sandbox.host"-->
     |                                in_worker: thin Store record and
     |                                project record under its root,
     |                                then the same local(Host, Call)
     v                                 v
 drain: spooled exec output sent as {stream, bytes} events, spool released
```

| Item | Rule | Source |
| --- | --- | --- |
| Operations | Ten, one table (`OPS`): `sandbox.create`, `sandbox.close`, `sandbox.list_workspaces`, `backend.daemon.connection_failures`, `workspace.mount`, `exec`, `exec_cancel`, `commit`, `unmount`, `backend.daemon.workspace.status`. A worker runs exactly these. | [`host/call.rs`](../../../../control/crates/layerfs-sandbox/src/host/call.rs) |
| Frame | Built from the typed `Call` in one field order and read back to the same request; a frame that names no operation, or a non-text message, is refused before any effect. | [`host/frame.rs`](../../../../control/crates/layerfs-sandbox/src/host/frame.rs) |
| Reach | A worker not reached is `Unreached`, nothing sent; nothing starts the machine. `on_machine` turns it into the refusal, which names `layerfs machine start`. | `reach`, `on_machine` in [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) |
| PostgreSQL create | A reach check on the side that launches: one session reads the schema's binding (same Store) and finds the project; else `Unavailable` naming the host (never the URL) or `NoSuchProject`. | `reach_check` |
| Exec with a caller-kept record | The worker runs it on its own thread and relays `{ exec_started: { stop } }` as an event as soon as the command runs, so the caller records the handle or, if a cancel came first, ends it in the worker with zero grace. | `reporting` in `route.rs`, [`workspace/exec.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/exec.rs) |
| Spooled output | Over 1 MiB, each stream goes to the caller as `{ stream, bytes }` events in order, then the spool is released; the reply keeps the stream null with its size and `streamed: true`. | `drain` |
| Close | A closed Sandbox's sqlite Store held open on this side is checkpointed and released before the answer. | `closed` |

## `direct.rs`: the direct route

| Item | Rule | Source |
| --- | --- | --- |
| When | A `sandbox.create` through a worker answers the daemon's endpoint and keys (`control`). Where the machine's adapter forwards that port to this host (`lima`: `ssh -L`), the home saves the keys in `sandbox/<name>/control.json` (0600) and the forward in the record's `daemon.forward`. Elsewhere (`docker`, `apple-container`) the worker route stays. | `open`, `save`, `carried` in [`direct.rs`](../../../../control/crates/layerfs-sandbox/src/direct.rs) |
| What runs here | `workspace.mount`, `commit`, `unmount`, `status` and `sandbox.get`'s Workspace list dial the daemon from the home under the Store's lease, with the Workspace records and unmount guards in the home's folder. `workspace.exec` still runs in the worker (the runtime is there), with its directory checked and its record (route `machine:<name>`) kept here. | `served`, [`workspace/exec.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/exec.rs) |
| Liveness | Refused `Unavailable` while the machine records no worker, or when the forward process (matched by pid, start time and boot) has ended. | `check` |
| Unanswered daemon | `Unavailable` naming `layerfs sandbox get` and `close`; a `Capacity` refusal's counts survive as the detail, tagged `code: "Capacity"`; `Busy` and `StaleSandbox` pass through. | `unanswered`, `down_detail` |
| Close | Sends the worker the Workspaces this home holds (`held`); afterwards the forward is ended if it is still that process. | `held`, `release` |

## Related

- [Sandboxes](README.md): the topic, its limits and tests.
- [Backends](backends.md): `Sandbox::run_in`, stop handles and exit codes behind exec.
- [Engine: Bridge](../../engine/bridge/README.md): the control records and handshake this channel carries.
- [Engine: Daemon: control calls](../../engine/daemon/control-calls.md): the daemon's side of each request.
- [Machines: workers and reach](../machines/workers-and-reach.md): the worker that runs `sandbox.host`, and forwards.
- [Control](../README.md): where each call runs.
- [API: `workspace` calls](../../api/sdk/workspace.md) and [API: `sandbox` calls](../../api/sdk/sandbox.md): the calls and their answers.
