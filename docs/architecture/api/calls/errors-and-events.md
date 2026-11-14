# Errors and events

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

What a call answers when it does not succeed, and what a running call sends its caller before it answers: progress events and `workspace.exec` output, within [Calls](README.md).

| | |
| --- | --- |
| Source | [`outcome.rs`](../../../../control/crates/layerfs-home/src/outcome.rs), [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs), [`events.rs`](../../../../control/crates/layerfs-home/src/events.rs), [`calls/dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs), [`calls/context.rs`](../../../../api/crates/layerfs-api/src/calls/context.rs), [`sandbox/route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs), [`sandbox/output.rs`](../../../../control/crates/layerfs-sandbox/src/output.rs) |

## Refusals

Every call answers `Outcome = Result<SdkValue, SdkRefusal>`
([`outcome.rs`](../../../../control/crates/layerfs-home/src/outcome.rs)). A
refusal is `{ code, message, detail }`: `code` is a stable text, `message` is
for people and names the next call where there is one, `detail` is null or a
map (`{entity, phase, op}`, `{missing}`, `{needed, free}`, `{path}`, ...). The
front ends carry it unchanged.

```text
 SdkRefusal { code, message, detail }
   Node:    throws LayerFSError (code, message, detail)
   Python:  raises layerfs.LayerFSError (.code, .message, .detail)
   CLI:     JSON on stderr, exit 125
   text:    {"refused": {"code", "message", "detail"}}   (call_json)
```

| Code | Meaning | Raised in (examples) |
| --- | --- | --- |
| `InvalidInput` | the request is wrong for this call or name; nothing happened. Also every folder-scope refusal (`out of scope: …`) | [`calls/check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs), [`calls/scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs), [`calls/dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs) |
| `NotFound` | no machine, bucket or Store of that name, for a call that names it directly | [`record.rs`](../../../../control/crates/layerfs-machine/src/record.rs), [`backup/record.rs`](../../../../control/crates/layerfs-store/src/backup/record.rs), [`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) |
| `NoSuchStore`, `NoSuchProject` | the Store or project of a `<store>/<project>` argument is missing | [`resolve.rs`](../../../../control/crates/layerfs-store/src/resolve.rs) |
| `NoSuchBranch`, `NoSuchCommit` | no such Branch, or no such Commit on it | [`calls/branch.rs`](../../../../control/crates/layerfs-store/src/calls/branch.rs), [`history/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `NoSuchSandbox`, `NotMounted` | no Sandbox of that name; no Workspace of that name or path | [`family.rs`](../../../../control/crates/layerfs-sandbox/src/family.rs), [`workspace/serve.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) |
| `NoSuchPath`, `IsDirectory`, `NotDirectory`, `NotFile`, `NotEmpty`, `Binary` | path refusals of the `file` calls | [`write/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/write/mutation.rs), [`read/`](../../../../engine/crates/layerfs-store-service/src/read) |
| `Exists`, `NameTaken`, `ReservedName` | a name is taken or reserved; a database already connected | [`postgresql/connect.rs`](../../../../control/crates/layerfs-store/src/postgresql/connect.rs), [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `InUse` | held: running Sandboxes, processes with the Store open, Stores on a machine | [`open.rs`](../../../../control/crates/layerfs-store/src/open.rs), [`inventory.rs`](../../../../control/crates/layerfs-control/src/inventory.rs) |
| `Busy` | another operation runs on the entity (`detail {entity, phase, op}`), no daemon connection came free, or a Store's writable open met SQLite's busy lock before any effect (the open wrote nothing, so the caller may repeat the call); nothing was attempted | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs), [`daemon/pool.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs), [`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs) |
| `Unresolved` | an earlier operation on the entity ended `Unknown` and its process is gone | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs) |
| `Conflict`, `HeadMoved` | a merge or Commit conflict, or the Branch head moved; for `backup.restore`, the Store changed since its last transfer | [`daemon/values.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/values.rs), [`history/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/history/lookup.rs) |
| `Dirty`, `MountsActive` | unmount with uncommitted changes; `sandbox.close` while Workspaces are mounted | [`daemon/values.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/values.rs), [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) |
| `Capacity` | no room: a full Sandbox (16 Workspaces), a full disk (`detail {needed, free}`), a bounded engine limit | [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs), [`sqlite/transfer/phases.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs) |
| `TooLarge` | a worker frame or daemon control record would be too large; nothing was sent | [`worker/frame.rs`](../../../../control/crates/layerfs-machine/src/worker/frame.rs), [`daemon/call.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/call.rs) |
| `Unsupported` | not possible on this host, not built (`detail.missing` names tools), or new work on a removed platform | [`bundle.rs`](../../../../control/crates/layerfs-machine/src/bundle.rs), [`calls/platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs) |
| `Unavailable`, `Denied` | a server, bucket, worker or host cannot be reached, or refused the login or secret | [`place.rs`](../../../../control/crates/layerfs-store/src/place.rs), [`worker_server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs) |
| `StaleSandbox`, `NoStore`, `Unauthorized`, `UnknownMethod` | a saved daemon not ready; a Store file missing where it should be; a Sandbox record not owned by this user or not private; an operation no one serves | [`daemon/pool.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs), [`service.rs`](../../../../control/crates/layerfs-store/src/service.rs), [`files.rs`](../../../../control/crates/layerfs-sandbox/src/files.rs) |
| `Failed` | tried and did not happen, or no code of its own (`outcome::failed`); whatever exists is kept | [`outcome.rs`](../../../../control/crates/layerfs-home/src/outcome.rs) |
| `Unknown` | the outcome is not known | [Uncertain outcomes](#uncertain-outcomes) |

A daemon refusal of a Workspace call carries the daemon's `ControlCode` as
its code (`Busy`, `HeadMoved`, `Missing`, `Capacity`, `Invalid`, `Failed`,
`Unknown`) with `detail.phase`
([`daemon/values.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/values.rs) `refusal`).

### Refusal order

Refusals that come before any effect, in the order a call meets them
([`calls/dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs)):

| Step | Refusal | Source |
| --- | --- | --- |
| 1. text front ends parse arguments and scope | `InvalidInput` (bad JSON) | [`calls/text.rs`](../../../../api/crates/layerfs-api/src/calls/text.rs) |
| 2. find `family.call` | `InvalidInput` "no call named …" | [`calls/table.rs`](../../../../api/crates/layerfs-api/src/calls/table.rs) `find` |
| 3. check arguments | `InvalidInput`: not named fields, unknown argument, a function crossing in, missing positional, wrong kind, bad size | [`calls/check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs) |
| 4. apply folder scope | `InvalidInput` `out of scope: history`, `path`, `branch`, `project` or the call's name | [`calls/scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs) `apply` |
| 5. resolve the home | `InvalidInput`: not absolute, or neither `LAYERFS_HOME` nor `HOME` set | `dispatch::home` |
| 6. platform policy | `Unsupported`: new work on a removed platform's machine | [`calls/platform_policy.rs`](../../../../api/crates/layerfs-api/src/calls/platform_policy.rs) |
| 7. open the home | the home's refusal (layout, migration, lock), also noted by `note_failure` | [`migrate/open.rs`](../../../../control/crates/layerfs-control/src/migrate/open.rs) |
| 8. scope owns the Sandbox | `InvalidInput` `out of scope: sandbox` | [`calls/scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs) `owns` |
| 9. family checks | `NoSuch*`, `NotFound`, `Busy`, `Unresolved`, `InUse`, worker `Unavailable` | family code |

The CLI refuses a wrong command line itself, with exit 2, before any call
([`parse.rs`](../../../../api/crates/layerfs-cli/src/parse.rs)).

## Uncertain outcomes

```text
 call --> work starts --> request sent / effect may have happened
                               |
                  answer lost, or engine cannot tell
                               v
                       refusal "Unknown"
                               |
     +-------------------------+---------------------------+
     | operation with an intent (op)                       | Store call
     | store push/pull/remove, machine create/remove,      | (file write,
     | sandbox create/close                                |  merge, ...)
     v                                                     v
 record keeps op {uncertain: true}               engine keeps the private
 + line in log/uncertain.jsonl                   Workspace; detail names it
     |
     v  its process gone, later calls on the entity:
 "Unresolved"  --> show: store status | machine status | sandbox get
                   clear: store remove | machine remove | sandbox close
```

| Code | Outcome | Source |
| --- | --- | --- |
| `Unknown` | may have happened. Every call is one attempt; nothing is resent. Read state with `list`, `get` or `status` before acting again | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) `lost`, `answer`; [`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs) `retain`; [`backup/record.rs`](../../../../control/crates/layerfs-store/src/backup/record.rs) |
| `Unresolved` | an earlier `Unknown` on this entity stands; this call did nothing | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs) `blocks` |
| daemon `Unknown` | a Workspace operation's outcome is uncertain at the daemon | [`control_types.rs`](../../../../engine/crates/layerfs-bridge/src/control_types.rs) `ControlCode::Unknown` |
| `Failed` with `attempted: true` after a daemon exchange | uncertain, though the code is `Failed`: a lost daemon reply is not mapped to `Unknown` | [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) `operation` |
| `Busy` | known: nothing was attempted, including a `Busy` Store open (the open changed nothing) | [`op.rs`](../../../../control/crates/layerfs-home/src/op.rs), [`daemon/pool.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs), [`service.rs`](../../../../control/crates/layerfs-store/src/service.rs) |
| `Unavailable`, `Denied`, `TooLarge` from a worker | known: the request was not sent | [`worker/client.rs`](../../../../control/crates/layerfs-machine/src/worker/client.rs) (`Unreached`) |

Intents are written by
[`sqlite/family.rs`](../../../../control/crates/layerfs-store/src/sqlite/family.rs),
[`sqlite/remove.rs`](../../../../control/crates/layerfs-store/src/sqlite/remove.rs),
[`sqlite/transfer/phases.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs),
machine [`create.rs`](../../../../control/crates/layerfs-machine/src/create.rs) and
[`remove.rs`](../../../../control/crates/layerfs-machine/src/remove.rs),
and Sandbox [`create.rs`](../../../../control/crates/layerfs-sandbox/src/create.rs) and
[`close.rs`](../../../../control/crates/layerfs-sandbox/src/close.rs).
A read never clears an intent. An engine read that fails is a known `Failed`,
never `Unknown` (`read_failure` in
[`write/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/write/mutation.rs)).
After an `Unknown` from `project.init` on a worker, the worker removes its
staged copy when its Init ends
([`project/family.rs`](../../../../control/crates/layerfs-store/src/project/family.rs)).

## Progress events

Every call gets one event sink, [`Context`](../../../../api/crates/layerfs-api/src/calls/context.rs),
over [`Events`](../../../../control/crates/layerfs-home/src/events.rs):
`progress(v)` sends `{ progress: v }`, `event(e)` passes a worker's event as
it came, `output(stream, bytes)` sends `{ stream, bytes }`. Events are sent on
the call's own thread, before it returns.

Nine calls declare the shared `PROGRESS` option (`on_progress`; CLI
`--progress`) in [`calls/table.rs`](../../../../api/crates/layerfs-api/src/calls/table.rs):
`project.init`, `project.export`, `store.push`, `store.pull`,
`backup.create`, `backup.restore`, `machine.start`, `machine.create` and
`workspace.exec`. `workspace.exec` alone also declares the `stdout` and
`stderr` functions. A function never crosses into the library:
[`calls/check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs)
refuses one as a value.

| Shape | Calls | Source |
| --- | --- | --- |
| `{ phase, done, total }` | `project.init`, `store.push`, `store.pull` | [`sqlite/family.rs`](../../../../control/crates/layerfs-store/src/sqlite/family.rs) `progress` |
| `{ phase, files, directories, symlinks, bytes, done, total }` | `project.export` | [`project/export/writer.rs`](../../../../control/crates/layerfs-store/src/project/export/writer.rs) `phase_event` |
| `{ phase, done, total, pieces }` | `backup.create`, `backup.restore` | [`backup/progress.rs`](../../../../control/crates/layerfs-store/src/backup/progress.rs) |
| `{ exec: { id } }`, once, before the command starts | `workspace.exec` | [`workspace/exec.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/exec.rs) |
| `{ stage, state, elapsed_ms? }`: `started`, then `completed` or `failed` with `elapsed_ms` | `machine.start`, `machine.create` | [`layerfs-machine/src/progress.rs`](../../../../control/crates/layerfs-machine/src/progress.rs) `stage` |
| `{ phase: "create", machine }` | `machine.create`, after its record is claimed | [`create.rs`](../../../../control/crates/layerfs-machine/src/create.rs) |

| Call | Phases or stages, in order |
| --- | --- |
| `project.init` | `check`, `send` (worker Stores only: first `done` 0, then one per 8 MiB of tree records, last `done == total`), `install`, `record` |
| `store.push`, `store.pull` | `check`, `snapshot` (marker, then byte counts), `send` (8 MiB chunks, [`steps.rs`](../../../../control/crates/layerfs-store/src/sqlite/transfer/steps.rs) `CHUNK`), `verify` (marker, then byte counts), `install`, `record`, `retire` |
| `project.export` | `prepare`, `export` (first at zero, then at most one per 200 ms, last at the final counts), `finalize` (after the staging folder is renamed onto the destination) |
| `backup.create` | `check`, `snapshot`, `send` (worker Stores only), `check`, `hash`, `upload` |
| `backup.restore` | `check`, `download`, `verify`, `install` |
| `machine.start` | `bundle-validation`, `sandbox-image` (a `docker` machine with no image yet), `provider-and-worker`, `worker-compatibility` ([`endpoint.rs`](../../../../control/crates/layerfs-machine/src/endpoint.rs)) |
| `machine.create` | `bundle-validation`, `{phase: "create"}`, `provider-and-worker`, `worker-compatibility` |

`done` and `total` are bytes while a phase moves bytes and null otherwise;
null means unknown or not counted. Only the worker route of `project.init`
measures the tree first, so `total` is not available on every route;
`project.export` does no prescan and keeps `total` null, counting entries and
logical file bytes. `pieces` is `{ done, of }` while a backup file moves as
pieces. There is no sequence number, timestamp, call id or "phase finished"
event.

### Delivery

| Front end | Progress reaches | A handler that fails | Source |
| --- | --- | --- | --- |
| Node | `on_progress(event.progress)`, the payload only. The addon counts queued events and the call settles only after all are delivered | a synchronous throw becomes `LayerFSHandlerWarning` and later events still arrive; an `async` handler's rejected promise is not awaited, so Node's unhandled-rejection rule applies | [`index.js`](../../../../api/node/src/index.js), [`layerfs-node/src/call.rs`](../../../../api/crates/layerfs-node/src/call.rs) |
| Python | `on_progress(event["progress"])` on the calling thread; aio queues on the loop and awaits a barrier behind the events before returning | `sys.unraisablehook`; aio: the loop's exception handler; later events still arrive | [`_client.py`](../../../../api/python/src/layerfs/_client.py), [`aio.py`](../../../../api/python/src/layerfs/aio.py), [`layerfs-python/src/call.rs`](../../../../api/crates/layerfs-python/src/call.rs) |
| CLI | with `--progress`: a status line on a terminal stderr, else (or with `LAYERFS_PROGRESS_JSON` set) the whole event as one JSON line on stderr; written before the result | a write that fails is ignored; the exit code is the outcome's | [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs) |

Without a function (SDK) or `--progress` (CLI), progress events are dropped.
An event with neither a `progress` nor a `stream` key reaches no function in
any front end. Rate: `project.export` coalesces to at most one `export`
event per 200 ms (`PROGRESS_INTERVAL`); backups send at most one byte update
per 250 ms ([`bucket/progress.rs`](../../../../control/crates/layerfs-bucket/src/bucket/progress.rs)
`INTERVAL`); the other calls send every event, bounded by structure (phases
and 8 MiB chunks), never per file. The Node addon queues events in an
unbounded thread-safe function (`create_threadsafe_function(0, …)`,
`Blocking`), so a slow Node handler does not slow the call; a slow Python
function or a full CLI stderr pipe does. Cancelling a Python coroutine stops
its functions, not the call; Node has no cancellation.

### What completion means

An event is never proof that anything finished; the only completion is the
call's result. Phase events mark entry: `record` is sent before the record
is written, and `send` reaches `done == total` before the call verifies,
installs and records, each of which can fail or be uncertain. A refusal after
any number of events is still a refusal. An observer that stopped listening
has learned nothing about whether the call ended; read state before acting
again.

## workspace.exec output

Output is buffered and delivered after the command ends; it is never live.

```text
 command --> stdout spool, stderr spool  (output.rs)
                  |
       stream <= 1 MiB (INLINE)            stream > 1 MiB
       kept in memory, answered,           unlinked temp file; answer has
       one event per non-empty stream      it null + stdout_bytes,
       (sent after the answer)             stderr_bytes, streamed: true
                  |                                 |
                  |                        drain: 1 MiB pages, stdout then
                  |                        stderr, then the spool is released
                  v                                 v
          stdout / stderr functions, or --stream / --events; else dropped
```

| Rule | Source |
| --- | --- |
| Both streams are read into spools; nothing reaches the caller until the command exits | [`exec/run.rs`](../../../../control/crates/layerfs-sandbox/src/exec/run.rs), [`output.rs`](../../../../control/crates/layerfs-sandbox/src/output.rs) |
| A non-empty stream of at most 1 MiB (`INLINE`) is answered in the result and sent as one event; an empty one sends none | [`output.rs`](../../../../control/crates/layerfs-sandbox/src/output.rs), [`workspace/exec.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/exec.rs) |
| A larger stream reaches the caller only as 1 MiB pages; the result carries its size and `streamed: true` | [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) `drain` |
| Order: inline, all of stdout then all of stderr; spooled, stdout pages then stderr pages; mixed, the spooled stream's pages precede the inline event. There is no chronological interleaving | [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs), [`workspace/exec.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/exec.rs) |
| Without a function or `--stream`, pages are drained and dropped; the spool is released, so the output cannot be fetched again | [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) |
| `truncated` is always `false`; no total is capped (bounded by the temp directory's free space); there is no timeout | [`output.rs`](../../../../control/crates/layerfs-sandbox/src/output.rs) |
| An event's `bytes` is text when valid UTF-8, else `{ base64 }` (Node `Buffer`, Python `bytes`); pages cut at byte counts, so one stream may mix both | [`json.rs`](../../../../control/crates/layerfs-home/src/json.rs) |
| A spool or drain failure makes the call `Failed` after the command ran, with no exit code in the error | [`output.rs`](../../../../control/crates/layerfs-sandbox/src/output.rs), [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) |

`layerfs workspace exec … --events` writes every progress and output event
to stdout as one JSON line each, then one `{result}` or `{refusal}` line. It
is exclusive with `--stream` and `--text`. A result exits 0 whatever the
command's exit code; a refusal exits 125, a usage error 2. A stdout write that
fails exits 125 with the call's effects kept and delivery incomplete; the call
is not repeated ([`framed.rs`](../../../../api/crates/layerfs-cli/src/framed.rs),
[`parse.rs`](../../../../api/crates/layerfs-cli/src/parse.rs)).

## Related

- [Calls](README.md): dispatch, argument kinds and the `PROGRESS` set.
- [SDK](../sdk/README.md): each call's own errors.
- [Front ends](../front-ends/README.md), [bindings](../front-ends/bindings.md): how each shows a refusal and events.
- [Control](../../control/README.md): routing, workers and the daemon channel.
