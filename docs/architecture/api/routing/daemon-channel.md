# Daemon channel

[Architecture](../../README.md) › [API](../README.md) › [Routing and workers](README.md) › Daemon channel

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How Workspace calls reach a daemon.

| | |
| --- | --- |
| Component | [Routing and workers](README.md) |
| Source | [`api/crates/layerfs-control/src/daemon`](../../../../api/crates/layerfs-control/src/daemon), [`sandbox/files.rs`](../../../../api/crates/layerfs-control/src/sandbox/files.rs), [`sandbox/registry.rs`](../../../../api/crates/layerfs-control/src/sandbox/registry.rs), [`workspace/serve.rs`](../../../../api/crates/layerfs-control/src/workspace/serve.rs), [`common/trace.rs`](../../../../api/crates/layerfs-control/src/common/trace.rs) |

The process that serves a Sandbox (the caller on `native` or for a forwarded
Sandbox, else the machine's worker) holds that Sandbox's control connections
in memory and sends each Workspace operation over one of them. The daemon's
side of the channel is in [Engine: Daemon](../../engine/daemon/README.md).

## Channel and pool

```text
 sandbox/<name>/control.json  {endpoint, secret, peer, instance}   (0600)
        |
        v
 Dial ----TCP connect (10 s)----> daemon endpoint
   |      authenticated channel (initiate_observed: secret, peer)
   |      Request::Hello{expected_instance} --> Reply::Hello(ControlReady)
   |      anything else: StaleSandbox
   v
 Controls (one per Sandbox, in SandboxState)
 +--------------------------------------------------------------+
 | idle: [Control, Control, ...]   open <= limit (6)            |
 |                                                              |
 | lend(): idle one --> else open one more (if open < limit)    |
 |         --> else wait for a free one, <= 60 s --> Busy       |
 | drop(Lease): failed Control --> closed, open -= 1            |
 |              healthy Control --> back to idle                |
 +--------------------------------------------------------------+
        |
        v  one exchange at a time per Control:
 Control.call(Request) --> send once --> await one Reply
```

| Part | What it is | Source |
| --- | --- | --- |
| `Control` | one sequential authenticated connection; numbers its calls; once an exchange fails it is quarantined and refuses every later call before sending | [`daemon/channel.rs`](../../../../api/crates/layerfs-control/src/daemon/channel.rs) |
| `Dial` | how to open another connection to the same daemon instance: endpoint, optional dial host, secret, peer key, instance | [`daemon/pool.rs`](../../../../api/crates/layerfs-control/src/daemon/pool.rs) |
| `Controls` | the Sandbox's pool: up to `CONNECTIONS` (6) per process; the daemon admits `DAEMON_CONNECTIONS` (8), leaving the rest to other processes | `daemon/pool.rs`, [`sandbox/serve.rs`](../../../../api/crates/layerfs-control/src/sandbox/serve.rs) |
| `Lease` | one connection lent for one call's exchanges, returned on drop | `daemon/pool.rs` |
| `Traced` | a lease whose requests travel as traced observations while the thread traces; its wait is the call's queue time | [`common/trace.rs`](../../../../api/crates/layerfs-control/src/common/trace.rs) |

The pool exists because the daemon serves each authenticated connection on
its own worker: one shared connection made every call wait for the one before
it (#352 F-017, `pool.rs`). When the daemon admits no more connections, the
pool lowers its limit to what it has and callers take turns. The first
connection a process opens waits up to `ADMISSION_WAIT` (60 s) to be admitted;
opening a connection sends only Hello, so it attempts no operation.

`LAYERFS_DAEMON_HOST` (or the deploy setting `daemonHost`) names the host to
dial a daemon's loopback-published port from a client in a container that
does not share the Docker host's loopback (`pool::daemon_host`).

## Workspace calls

```text
 workspace call (control)            host Sandbox code      daemon
   |                                 (serve.rs, files.rs)
   | mount(sandbox, branch, name?)        |
   |------------------------------------->| Branch id: Store's own
   |                                      | route (never the daemon)
   |                                      | lend Control
   |                                      |-- Mount{workspace,branch} -->
   |                                      |<-- Bound{token, binding} ----
   |                                      |-- Attach(token) ------------>
   |                                      |<-- Ready(mount dir, profile)-
   |<--- {ws, path, name, ...} -----------|
   | commit / unmount / status (ws)       |
   |------------------------------------->|-- Commit / Unmount /
   |                                      |   ForceUnmount / Status ---->
   |<--- value or refusal ----------------|<-- Reply or Refused ---------
```

| Call | Daemon requests | Answer shaped by | Source |
| --- | --- | --- | --- |
| `workspace.mount` | `Mount` (bind), then `Attach` (kernel Ready): two acknowledged steps; a failed attach leaves the Workspace bound | `values::mounted` | [`daemon/workspace/mount.rs`](../../../../api/crates/layerfs-control/src/daemon/workspace/mount.rs), [`recovery.rs`](../../../../api/crates/layerfs-control/src/daemon/workspace/recovery.rs) |
| `workspace.commit` | `Commit` | `values::commit`: Committed, Merged (with `merged`), or `Conflict` | [`daemon/workspace/commit.rs`](../../../../api/crates/layerfs-control/src/daemon/workspace/commit.rs), [`daemon/values.rs`](../../../../api/crates/layerfs-control/src/daemon/values.rs) |
| `workspace.unmount` | `Unmount{token, discard}`, or `ForceUnmount` with `force` | `values::unmounted` (`Dirty` refusal) or `force_unmounted` | [`daemon/workspace/unmount.rs`](../../../../api/crates/layerfs-control/src/daemon/workspace/unmount.rs) |
| `workspace.status` | `Status(token)`; the Branch head is read separately on the Store's route | `values::status` | [`daemon/workspace/status.rs`](../../../../api/crates/layerfs-control/src/daemon/workspace/status.rs) |
| `sandbox.get` Workspace list | SDK method `sandbox.list_workspaces`, the one daemon backend method sent | `sandbox/dispatch.rs` | [`daemon/operator.rs`](../../../../api/crates/layerfs-control/src/daemon/operator.rs) |
| `workspace.exec` | none on this channel: `bash -c` runs through the Sandbox's runtime | — | [`sandbox/exec.rs`](../../../../api/crates/layerfs-control/src/sandbox/exec.rs) |

`Locate` and `Cleanup` are read-only observations for a caller whose
acknowledgement was lost; they bind, attach and remove nothing
([`recovery.rs`](../../../../api/crates/layerfs-control/src/daemon/workspace/recovery.rs)).
Every Workspace record (`workspace/<id>.json`) and unmount guard lives in the
serving side's Sandbox folder ([`workspace/serve.rs`](../../../../api/crates/layerfs-control/src/workspace/serve.rs),
[`workspace/unmount_guard.rs`](../../../../api/crates/layerfs-control/src/workspace/unmount_guard.rs)).
A Sandbox holds at most 16 Workspaces (`WORKSPACES`); a mount into a full one
is `Capacity`.

## Failures

```text
 Control.call(request)
   phase: Encode --> Send --> Receive --> Validate   (Fence: session end)
                     ^
                     attempted = true from here: the request may have
                     taken effect; nothing is resent
```

| Failure | Becomes | Outcome known? | Source |
| --- | --- | --- | --- |
| Daemon refusal (`Reply::Refused`) | its `ControlCode` as the code: `Busy`, `HeadMoved`, `Missing`, `Capacity`, `Invalid`, `Failed` or `Unknown`; `detail.phase` | yes, except code `Unknown` | [`daemon/failure.rs`](../../../../api/crates/layerfs-control/src/daemon/failure.rs), [`values.rs`](../../../../api/crates/layerfs-control/src/daemon/values.rs) `refusal`, [`control_types.rs`](../../../../engine/crates/layerfs-bridge/src/control_types.rs) |
| Request larger than one control record, before sending | `TooLarge` | yes: not sent | [`daemon/call.rs`](../../../../api/crates/layerfs-control/src/daemon/call.rs) |
| Transport or protocol failure (`OperationCause::Exchange`) | `Failed`, its message the failure's debug text with `phase` and `attempted` | no when `attempted` is true | [`sandbox/serve.rs`](../../../../api/crates/layerfs-control/src/sandbox/serve.rs) `operation` |
| Terminal step stopped after effects (`Retained`) | `Failed`; the daemon keeps custody | partly | `daemon/failure.rs` |
| Reply of the wrong kind (`Unexpected`) | `Failed` | not established from source | `daemon/failure.rs` |
| No free connection in 60 s | `Busy`, "nothing was attempted" | yes | [`daemon/pool.rs`](../../../../api/crates/layerfs-control/src/daemon/pool.rs) |
| Saved daemon not `ControlReady` | `StaleSandbox` | yes | `daemon/pool.rs` |
| Commit conflict | `Conflict` with `head`, `base`, `markers`, `conflicts`, `written` | yes: nothing committed | `values.rs` `commit` |
| Unmount with uncommitted changes | `Dirty` with `changed`, `pendingMerge` | yes | `values.rs` `unmounted` |

A lost daemon reply after the request was sent reaches the caller as
`Failed`, not `Unknown`: `serve::operation` turns every non-refusal exchange
failure into `Failed`, so the uncertainty shows only in the message's
`attempted: true`. No step is retried, rebound or re-attached on the
caller's behalf; a quarantined connection is closed and a later call may
open another. Other codes are in [Call table: Errors](../call-table/errors.md).

## Related

- [Routing table](routing-table.md#workspace-calls): which process serves a Sandbox.
- [Workers](workers.md): the channel to a worker that serves Sandboxes in a machine.
- [Engine: Daemon](../../engine/daemon/README.md), [Daemon bridge](../../engine/daemon/bridge.md) and [Control calls](../../engine/daemon/control-calls.md): the daemon's side.
- [Call table: workspace family](../call-table/families/workspace.md): the calls and their answers.
- [Call table: Errors](../call-table/errors.md#uncertain-outcomes): uncertain outcomes.

---

← [Routing and workers: Workers](workers.md) · [Up: Routing and workers](README.md) · [API: Call table](../call-table/README.md) →
