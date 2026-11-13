# Daemon channel

[Architecture](../../README.md) › [Control](../README.md) › [Routing and workers](README.md) › Daemon channel

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How Workspace calls reach a daemon.

| | |
| --- | --- |
| Component | [Routing and workers](README.md) |
| Source | [`daemon`](../../../../control/crates/layerfs-sandbox/src/daemon), [`files.rs`](../../../../control/crates/layerfs-sandbox/src/files.rs), [`registry.rs`](../../../../control/crates/layerfs-sandbox/src/registry.rs), [`workspace/serve.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs), [`trace.rs`](../../../../control/crates/layerfs-home/src/trace.rs) |

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
 | drop(Lease): quarantined Control --> closed, open -= 1       |
 |              healthy Control --> back to idle                |
 +--------------------------------------------------------------+
        |
        v  one exchange at a time per Control:
 Control.call(Request) --> send once --> await one Reply
```

| Part | What it is | Source |
| --- | --- | --- |
| `Control` | one sequential authenticated connection; numbers its calls; once an exchange fails, or an applied fence (`close_handle`, `daemon/fence.rs`) closes its channel, it is quarantined and refuses every later call before sending | [`daemon/channel.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/channel.rs) |
| `Dial` | how to open another connection to the same daemon instance: endpoint, secret, peer key, instance | [`daemon/pool.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs) |
| `Controls` | the Sandbox's pool: up to `CONNECTIONS` (6) per process; the daemon admits `DAEMON_CONNECTIONS` (8), leaving the rest to other processes | `daemon/pool.rs`, [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) |
| `Lease` | one connection lent for one call's exchanges, returned on drop | `daemon/pool.rs` |
| `Traced` | a lease whose requests travel as traced observations while the thread traces; its wait is the call's queue time | [`daemon/trace.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/trace.rs) |

The pool exists because the daemon serves each authenticated connection on
its own worker: one shared connection made every call wait for the one before
it (#352 F-017, `pool.rs`). When the daemon admits no more connections, the
pool lowers its limit to what it has and callers take turns. The first
connection a process opens waits up to `ADMISSION_WAIT` (60 s) to be admitted;
opening a connection sends only Hello, so it attempts no operation.

## Workspace calls

```text
 workspace call (control)            host Sandbox code      daemon
   |                                 (lifecycle.rs, files.rs)
   | mount(sandbox, branch, name?)        |
   |------------------------------------->| reads no Store (#64)
   |                                      | lend Control
   |                                      |-- MountNamed{workspace,  --->
   |                                      |   project, branch}
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
| `workspace.mount` | `MountNamed` (bind the Branch the daemon finds by project and Branch name; `SdkRefused` `NoSuchProject` / `NoSuchBranch` when missing), then `Attach` (kernel Ready): two acknowledged steps; a failed attach leaves the Workspace bound | `values::mounted` | [`daemon/workspace/mount.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/workspace/mount.rs), [`daemon/workspace/recovery.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/workspace/recovery.rs) |
| `workspace.commit` | `Commit` | `values::commit`: Committed, Merged (with `merged`), or `Conflict` | [`daemon/workspace/commit.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/workspace/commit.rs), [`daemon/values.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) |
| `workspace.unmount` | `Unmount{token, discard}`, or `ForceUnmount` with `force` | `values::unmounted` (`Dirty` refusal) or `force_unmounted` | [`daemon/workspace/unmount.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/workspace/unmount.rs) |
| `workspace.status` | `Status(token)`; the Branch head is read separately on the Store's route | `values::status` | [`daemon/workspace/status.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/workspace/status.rs) |
| `sandbox.get` | SDK method `sandbox.list_workspaces`, then, on the same pooled connection and for a running Sandbox its machine still reaches, `backend.daemon.connection_failures` (`daemon_capacity`, bounded to 2 s: admission and the exchange share one deadline, fenced by [`daemon/fence.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/fence.rs); a window admission used up refuses the read, and a connection the fence ends is not lent again); a full pool's pre-handshake `LAYERFS-CAPACITY` notice survives the failed open as the partial `down` counts (#649), on the forwarded route and on the worker/local one, where the same notice arrives as the refusal the call returned | `sandbox/dispatch.rs` (both through `daemon/operator.rs`) | [`daemon/operator.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/operator.rs), [`daemon/capacity.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/capacity.rs) |
| `workspace.exec` | none on this channel: `bash -c` runs through the Sandbox's runtime | — | [`exec/run.rs`](../../../../control/crates/layerfs-sandbox/src/exec/run.rs) |

`Locate` and `Cleanup` are read-only observations for a caller whose
acknowledgement was lost; they bind, attach and remove nothing
([`daemon/workspace/recovery.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/workspace/recovery.rs)).
Every Workspace record (`workspace/<id>.json`) and unmount guard lives in the
serving side's Sandbox folder ([`workspace/serve.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs),
[`unmount_guard.rs`](../../../../control/crates/layerfs-sandbox/src/unmount_guard.rs)).
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
| Daemon refusal (`Reply::Refused`) | its `ControlCode` as the code: `Busy`, `HeadMoved`, `Missing`, `Capacity`, `Invalid`, `Failed` or `Unknown`; `detail.phase` | yes, except code `Unknown` | [`daemon/failure.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/failure.rs), [`daemon/values.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) `refusal`, [`control_types.rs`](../../../../engine/crates/layerfs-bridge/src/control_types.rs) |
| Request larger than one control record, before sending | `TooLarge` | yes: not sent | [`daemon/call.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/call.rs) |
| Transport or protocol failure (`OperationCause::Exchange`) | `Failed`, its message the failure's debug text with `phase` and `attempted` | no when `attempted` is true | [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) `operation` |
| Terminal step stopped after effects (`Retained`) | `Failed`; the daemon keeps custody | partly | `daemon/failure.rs` |
| Reply of the wrong kind (`Unexpected`) | `Failed` | not established from source | `daemon/failure.rs` |
| No free connection in 60 s | `Busy`, "nothing was attempted" | yes | [`daemon/pool.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs) |
| Saved daemon not `ControlReady` | `StaleSandbox` | yes | `daemon/pool.rs` |
| Commit conflict | `Conflict` with `head`, `base`, `markers`, `conflicts`, `written` | yes: nothing committed | `values.rs` `commit` |
| Unmount with uncommitted changes | `Dirty` with `changed`, `pendingMerge` | yes | `values.rs` `unmounted` |

A lost daemon reply after the request was sent reaches the caller as
`Failed`, not `Unknown`: `lifecycle::operation` turns every non-refusal exchange
failure into `Failed`, so the uncertainty shows only in the message's
`attempted: true`. No step is retried, rebound or re-attached on the
caller's behalf; a quarantined connection is closed and a later call may
open another. Other codes are in [Call table: Errors](../../api/call-table/errors.md).

## Related

- [Routing table](routing-table.md#workspace-calls): which process serves a Sandbox.
- [Workers](workers.md): the channel to a worker that serves Sandboxes in a machine.
- [Engine: Daemon](../../engine/daemon/README.md), [Daemon bridge](../../engine/daemon/bridge.md) and [Control calls](../../engine/daemon/control-calls.md): the daemon's side.
- [Call table: workspace family](../../api/sdk/workspace/README.md): the calls and their answers.
- [Call table: Errors](../../api/call-table/errors.md#uncertain-outcomes): uncertain outcomes.

---

← [Routing and workers: Workers](workers.md) · [Up: Routing and workers](README.md) · [API](../../api/README.md) →
