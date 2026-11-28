# Errors and events

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

What a call answers when it does not succeed, and what a running call sends its caller before it answers: progress events and `workspace.exec` output, within [Calls](README.md).

| | |
| --- | --- |
| Source | [`outcome.rs`](../../../../control/crates/layerfs-home/src/reply/outcome.rs), [`intent.rs`](../../../../control/crates/layerfs-home/src/record/intent.rs), [`events.rs`](../../../../control/crates/layerfs-home/src/reply/events.rs), [`run/call.rs`](../../../../api/crates/layerfs-api/src/dispatch/run/call.rs), [`dispatch/context.rs`](../../../../api/crates/layerfs-api/src/dispatch/context.rs), [`sandbox/route.rs`](../../../../control/crates/layerfs-sandbox/src/route/machine/send.rs), [`sandbox/output.rs`](../../../../control/crates/layerfs-sandbox/src/exec/output.rs) |

## Refusals

Every call answers `Outcome = Result<SdkValue, SdkRefusal>`
([`outcome.rs`](../../../../control/crates/layerfs-home/src/reply/outcome.rs)). A
refusal is `{ code, message, detail }` inside the library: `code` is a stable
text, `message` is for people and names the next call where there is one,
`detail` is null or a map (`{entity, phase, op}`, `{missing}`,
`{needed, free}`, `{path}`, ...).

At the call boundary (call surface 2.0) the dispatcher folds the code into
one of **thirteen public codes** (`outcome::CODES`, `FOLDS`, `public_code`;
[`answer/refusal.rs`](../../../../api/crates/layerfs-api/src/dispatch/answer/refusal.rs)
`public`) and every front end prints or raises one map
(`refusal::value`): `{ error, message }`, then `fix` when there is one, then
the fields the next call needs, flat. The long form (the call asked
`detail`) always has `fix`, and adds `detail`: what the refusing code
recorded, with the raised code as `detail.code` when it was folded. The
call log keeps the raised code.

```text
 SdkRefusal { code, message, detail }        detail.fix, detail.next: conventions
   dispatcher:  code -> public code (the raised one kept as detail.code)
   short:       {"error", "message", "fix"?, ...next fields}
   long:        {"error", "message", "fix", ...next fields, "detail"}
   Node:    throws LayerFSError (error = code, message, fix, fields, detail)
   Python:  raises layerfs.LayerFSError (.error = .code, .message, .fix, .fields, .detail)
   CLI:     one JSON line on stderr, exit 125 (2 for a wrong command line)
   MCP:     isError, the same map as text
   text:    {"refused": <the map>}   (call_json)
```

Control code attaches the two extras with `outcome::with_fix(refusal, "one
shell line")` and `outcome::with_next(refusal, "commit", head)`; they travel
in `detail` under the keys `fix` and `next`, so a refusal that crosses a
worker frame keeps them, and `refusal::value` lifts them out.

| Public code | Raised codes it holds (`FOLDS`) |
| --- | --- |
| `NotFound` | `NoSuchStore`, `NoSuchProject`, `NoSuchBranch`, `NoSuchCommit`, `NoSuchPath`, `NoSuchSandbox`, `NoStore`, `NoCommit`, `NotInHistory`, `NotMounted`, `Removed`, `NoMatch` |
| `Exists` | `NameTaken` |
| `InUse` | `MountsActive` |
| `Conflict` | `HeadMoved` |
| `InvalidInput` | `NotFile`, `IsDirectory`, `NotDirectory`, `NotEmpty`, `Binary`, `ReservedName`, `ReadOnly`, `NoMergeBase`, `TooLarge`, `AmbiguousMatch` |
| `Capacity` | `NoSpace` |
| `Unsupported` | `OldStore` |
| `Unavailable` | `Denied`, `Unauthorized`, `StaleSandbox`, `StaleStore`, `Timeout` |
| `Failed` | `UnknownMethod`, `Cancelled`, and any code neither list names |

`Busy`, `Unresolved`, `Dirty` and `Unknown` hold only themselves. The table below lists the codes as they are raised.

| Code | Meaning | Raised in (examples) |
| --- | --- | --- |
| `InvalidInput` | the request is wrong for this call or name; nothing happened. Also every folder-scope refusal (`out of scope: …`) | [`guard/arguments.rs`](../../../../api/crates/layerfs-api/src/dispatch/guard/arguments.rs), [`guard/scope.rs`](../../../../api/crates/layerfs-api/src/dispatch/guard/scope.rs), [`run/call.rs`](../../../../api/crates/layerfs-api/src/dispatch/run/call.rs) |
| `NotFound` | no machine, bucket or Store of that name, for a call that names it directly | [`machine.rs`](../../../../control/crates/layerfs-machine/src/record/machine.rs), [`bucket/record.rs`](../../../../control/crates/layerfs-store/src/backup/bucket/record.rs), [`inventory.rs`](../../../../control/crates/layerfs-sandbox/src/status/inventory.rs) |
| `NoSuchStore`, `NoSuchProject` | the Store or project of a `<store>/<project>` argument is missing | [`resolve.rs`](../../../../control/crates/layerfs-store/src/admin/route/resolve.rs) |
| `NoSuchBranch`, `NoSuchCommit` | no such Branch, or no such Commit on it | [`data/branch.rs`](../../../../control/crates/layerfs-store/src/data/branch.rs), [`src/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/lookup.rs) |
| `NoSuchSandbox`, `NotMounted` | no Sandbox of that name; no Workspace of that name or path | [`family.rs`](../../../../control/crates/layerfs-sandbox/src/family.rs), [`machine/serve.rs`](../../../../control/crates/layerfs-workspace/src/machine/serve.rs) |
| `NoSuchPath`, `IsDirectory`, `NotDirectory`, `NotFile`, `NotEmpty`, `Binary` | path refusals of the `file` calls | [`change/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/filesystem/change/mutation.rs), [`filesystem/`](../../../../engine/crates/layerfs-store-service/src/filesystem) |
| `Removed` | a path is hidden by `project.hide_path`; `detail.paths[]` names each hidden path and the frontier serial that governs it | [`src/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/lookup.rs), [`history/error.rs`](../../../../core/crates/layerfs-history/src/error.rs) |
| `StaleStore` | the Store was replaced or moved under this handle; nothing on it is read or written | [`src/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/lookup.rs), [`sqlite/lifecycle/identity.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/identity.rs) |
| `Exists`, `NameTaken`, `ReservedName` | a name is taken or reserved; a database already connected | [`postgresql/connect.rs`](../../../../control/crates/layerfs-store/src/backend/postgresql/connect.rs), [`history/branch.rs`](../../../../engine/crates/layerfs-store-service/src/history/branch.rs) |
| `InUse` | held: running Sandboxes, processes with the Store open, Stores on a machine | [`cache.rs`](../../../../control/crates/layerfs-store/src/admin/open/cache.rs), [`inventory.rs`](../../../../control/crates/layerfs-sandbox/src/status/inventory.rs) |
| `Busy` | another operation runs on the entity (`detail {entity, phase, op}`), no daemon connection came free, or a Store's writable open met SQLite's busy lock before any effect (the open wrote nothing, so the caller may repeat the call); nothing was attempted | [`intent.rs`](../../../../control/crates/layerfs-home/src/record/intent.rs), [`channel/pool.rs`](../../../../control/crates/layerfs-sandbox/src/route/daemon/channel/pool.rs), [`service.rs`](../../../../engine/crates/layerfs-store-service/src/service.rs) |
| `Unresolved` | an earlier operation on the entity ended `Unknown` and its process is gone | [`intent.rs`](../../../../control/crates/layerfs-home/src/record/intent.rs) |
| `Conflict`, `HeadMoved` | a merge or Commit conflict, or the Branch head moved; for `backup.restore`, the Store changed since its last transfer | [`daemon/refusal.rs`](../../../../control/crates/layerfs-sandbox/src/route/daemon/refusal.rs), [`src/lookup.rs`](../../../../engine/crates/layerfs-store-service/src/lookup.rs) |
| `Dirty`, `MountsActive` | unmount with uncommitted changes; `project.delete` while a Workspace of the project is mounted (`sandbox.remove` without `--force` while one is mounted is `InUse` itself) | [`daemon/refusal.rs`](../../../../control/crates/layerfs-sandbox/src/route/daemon/refusal.rs), [`delete/run.rs`](../../../../control/crates/layerfs-store/src/data/project/delete/run.rs), [`remove.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle/remove.rs) |
| `Capacity` | no room: a full Sandbox (16 Workspaces), a full disk (`detail {needed, free}`), a bounded engine limit | [`serve.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle/serve.rs), [`backend/sqlite/move_phases.rs`](../../../../control/crates/layerfs-store/src/backend/sqlite/move_phases.rs) |
| `TooLarge` | a worker frame or daemon control record would be too large; nothing was sent | [`guest/frame.rs`](../../../../control/crates/layerfs-machine/src/route/guest/frame.rs), [`daemon/call.rs`](../../../../control/crates/layerfs-sandbox/src/route/daemon/call.rs) |
| `Unsupported` | not possible on this host, not built (`detail.missing` names tools), or new work on a removed platform | [`bundle.rs`](../../../../control/crates/layerfs-machine/src/setup/bundle.rs), [`guard/platform_policy.rs`](../../../../api/crates/layerfs-api/src/dispatch/guard/platform_policy.rs) |
| `OldStore` | the Store is of an older schema version this release neither reads nor migrates (#776): a SQLite Store of schema version 2, a PostgreSQL Store of schema version 2 or 3. Its message is the version refusal itself ("SQLite Store schema version 2 (…): not supported by this release and not migrated; create a new Store"); its `fix` is `layerfs store upgrade <store>` when this build has a step from its version (SQLite 2, PostgreSQL 3; #804), else `layerfs store remove <store>` (sqlite) or `layerfs store disconnect <store>` (PostgreSQL, which keeps the data). Folded into `Unsupported`: this build cannot open it, and nothing was opened or changed. A project of such a Store is `blocked` by it with the reason `outdated`; `sandbox.remove` of a Sandbox whose Store it is answers it with the fix `layerfs sandbox remove <sandbox> --force`, except on a machine with a worker, whose close goes on without the Store (each mount intent it cannot end there is named in its `left`) | [`service.rs`](../../../../engine/crates/layerfs-store-service/src/service.rs) `OpenRefusal::Outdated`, [`service.rs`](../../../../control/crates/layerfs-store/src/admin/open/service.rs), [`state.rs`](../../../../control/crates/layerfs-store/src/admin/status/state.rs) `OLD_STORE`, `outdated_fix` |
| `Unavailable`, `Denied` | a server, bucket, worker or host cannot be reached, or refused the login or secret | [`executor.rs`](../../../../control/crates/layerfs-store/src/admin/route/executor.rs), [`serve.rs`](../../../../api/crates/layerfs-api/src/worker/serve.rs) |
| `StaleSandbox`, `NoStore`, `Unauthorized`, `UnknownMethod` | a saved daemon not ready; a Store file missing where it should be; a Sandbox record not owned by this user or not private; an operation no one serves | [`channel/pool.rs`](../../../../control/crates/layerfs-sandbox/src/route/daemon/channel/pool.rs), [`service.rs`](../../../../control/crates/layerfs-store/src/admin/open/service.rs), [`discovery.rs`](../../../../control/crates/layerfs-sandbox/src/record/discovery.rs) |
| `Failed` | tried and did not happen, or no code of its own (`outcome::failed`); whatever exists is kept | [`outcome.rs`](../../../../control/crates/layerfs-home/src/reply/outcome.rs) |
| `Cancelled` | `store.upgrade` stopped by its caller before its `commit` phase (Ctrl-C or SIGTERM to the `layerfs` command, a worker's `store.cancel`, a PostgreSQL cancel request): "cancelled before COMMIT; nothing changed". Folded into `Failed`, `detail.code` `Cancelled`. A second signal exits the command at once with `Unknown` and the fix `layerfs store upgrade <store>` (#804 §6) | [`upgrade.rs`](../../../../control/crates/layerfs-store/src/admin/maintenance/upgrade.rs), [`signals.rs`](../../../../api/crates/layerfs-cli/src/signals.rs) |
| `Unknown` | the outcome is not known; a GC call names its durable operation (`detail {entity, phase, op}`, with `phase`/`op` null when none is known) | [Uncertain outcomes](#uncertain-outcomes) |

A daemon refusal of a Workspace call carries the daemon's `ControlCode` as
its code (`Busy`, `HeadMoved`, `Missing`, `Capacity`, `Invalid`, `Failed`,
`Unknown`) with `detail.phase`
([`daemon/refusal.rs`](../../../../control/crates/layerfs-sandbox/src/route/daemon/refusal.rs) `refusal`).

### Refusal order

Refusals that come before any effect, in the order a call meets them
([`run/call.rs`](../../../../api/crates/layerfs-api/src/dispatch/run/call.rs)):

| Step | Refusal | Source |
| --- | --- | --- |
| 1. text front ends parse arguments and scope | `InvalidInput` (bad JSON) | [`run/call_json.rs`](../../../../api/crates/layerfs-api/src/dispatch/run/call_json.rs) |
| 2. find `family.call` | `InvalidInput` "no call named …" | [`dispatch/table.rs`](../../../../api/crates/layerfs-api/src/dispatch/table.rs) `find` |
| 3. check arguments | `InvalidInput`: not named fields, unknown argument, a function crossing in, missing positional, wrong kind, bad size | [`guard/arguments.rs`](../../../../api/crates/layerfs-api/src/dispatch/guard/arguments.rs) |
| 4. apply folder scope | `InvalidInput` `out of scope: history`, `path`, `branch`, `project` or the call's name | [`guard/scope.rs`](../../../../api/crates/layerfs-api/src/dispatch/guard/scope.rs) `apply` |
| 5. find the home in the root | `InvalidInput`: the root not absolute, or neither `LAYERFS_ROOT` nor `HOME` set | [`disk/root.rs`](../../../../control/crates/layerfs-home/src/disk/root.rs) `home` |
| 6. platform policy | `Unsupported`: new work on a removed platform's machine | [`guard/platform_policy.rs`](../../../../api/crates/layerfs-api/src/dispatch/guard/platform_policy.rs) |
| 7. open the home | the home's refusal: a folder that is not a LayerFS 2.0 home (a `layerfs.json` of another layout, or anything but `layerfs.lock`, the integration folders and `.DS_Store` with no `layerfs.json`) `Unsupported` with the fix `mv <home> <home>.old && layerfs home init`, an unreadable `layerfs.json` `Failed`, the home lock `Unavailable` | [`disk/layout.rs`](../../../../control/crates/layerfs-home/src/disk/layout.rs) `open` |
| 8. scope owns the Sandbox | `InvalidInput` `out of scope: sandbox` | [`guard/scope.rs`](../../../../api/crates/layerfs-api/src/dispatch/guard/scope.rs) `owns` |
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
     | store move/remove, machine create/remove,           | (file write,
     | sandbox create/remove                               |  merge, ...)
     v                                                     v
 record keeps op {uncertain: true}               engine keeps the private
 + line in log/uncertain.jsonl                   Workspace; detail names it
     |
     v  its process gone, later calls on the entity:
 "Unresolved"  --> show: store get --with live | machine get --with live
                         | sandbox get
                   clear: store remove | machine remove | sandbox remove
```

| Code | Outcome | Source |
| --- | --- | --- |
| `Unknown` | may have happened. Every call is one attempt; nothing is resent. A GC call's detail names its entity and, when one is known, its durable op. Read state with `list`, `get` or `status` before acting again | [`guest/client.rs`](../../../../control/crates/layerfs-machine/src/route/guest/client.rs) `lost`, `answer`; [`service.rs`](../../../../engine/crates/layerfs-store-service/src/service.rs) `retain`; [`bucket/record.rs`](../../../../control/crates/layerfs-store/src/backup/bucket/record.rs) |
| `Unresolved` | an earlier `Unknown` on this entity stands; this call did nothing | [`intent.rs`](../../../../control/crates/layerfs-home/src/record/intent.rs) `blocks` |
| daemon `Unknown` | a Workspace operation's outcome is uncertain at the daemon | [`request.rs`](../../../../engine/crates/layerfs-protocol/src/daemon/request.rs) `ControlCode::Unknown` |
| `attempted: true` (`Failed`, or `Unavailable` with `outcome: "unknown"`) after a daemon exchange or a runtime stop, delete or reopen | uncertain, though the code is not `Unknown`: the request was sent and no answer settled it. The message says so in words ("the request was sent; its outcome is not known"; a failed connection fence is named too), and `attempted: true` is flat beside `error` in both forms and in the long form's `detail` | [`daemon/refusal.rs`](../../../../control/crates/layerfs-sandbox/src/route/daemon/refusal.rs) `failure`, `mount_failure`, `wire`, `uncertain` |
| `Busy` | known: nothing was attempted, including a `Busy` Store open (the open changed nothing) | [`intent.rs`](../../../../control/crates/layerfs-home/src/record/intent.rs), [`channel/pool.rs`](../../../../control/crates/layerfs-sandbox/src/route/daemon/channel/pool.rs), [`service.rs`](../../../../control/crates/layerfs-store/src/admin/open/service.rs) |
| `Unavailable`, `Denied`, `TooLarge` from a worker | known: the request was not sent | [`guest/client.rs`](../../../../control/crates/layerfs-machine/src/route/guest/client.rs) (`Unreached`) |

Intents are written by
[`sqlite/family.rs`](../../../../control/crates/layerfs-store/src/backend/sqlite/family.rs),
[`sqlite/remove.rs`](../../../../control/crates/layerfs-store/src/backend/sqlite/remove.rs),
[`backend/sqlite/move_phases.rs`](../../../../control/crates/layerfs-store/src/backend/sqlite/move_phases.rs),
machine [`create.rs`](../../../../control/crates/layerfs-machine/src/lifecycle/create.rs) and
[`remove.rs`](../../../../control/crates/layerfs-machine/src/lifecycle/remove.rs),
and Sandbox [`create.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle/create.rs) and
[`remove.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle/remove.rs).
A read never clears an intent. An engine read that fails is a known `Failed`,
never `Unknown` (`read_failure` in
[`change/mutation.rs`](../../../../engine/crates/layerfs-store-service/src/filesystem/change/mutation.rs)).
After an `Unknown` from `project.init` on a worker, the worker removes its
staged copy when its Init ends
([`init/run.rs`](../../../../control/crates/layerfs-store/src/data/project/init/run.rs)).

## Progress events

Every call gets one event sink, [`Context`](../../../../api/crates/layerfs-api/src/dispatch/context.rs),
over [`Events`](../../../../control/crates/layerfs-home/src/reply/events.rs):
`progress(v)` sends `{ progress: v }`, `phase(phase, done, total, extra)`
sends the call surface's event `{ progress: { phase, done, total, … } }`
(`done` and `total` null where the phase has no count), `event(e)` passes a
worker's event as it came, `output(stream, bytes)` sends `{ stream, bytes }`. Events are sent on
the call's own thread, before it returns.

Twenty calls declare the shared `PROGRESS` option (`on_progress`; CLI
`--progress`) in their entries; `help --json` shows them as `progress: true`.
`workspace.exec` alone also declares the `stdout` and `stderr` functions. A
function never crosses into the library:
[`guard/arguments.rs`](../../../../api/crates/layerfs-api/src/dispatch/guard/arguments.rs)
refuses one as a value.

Every progress event is `{ phase, done, total }`; two kinds of call add a
key. What `done` and `total` count depends on the call:

| Calls | `done`, `total` | Source |
| --- | --- | --- |
| `project.init`, `store.move`, `backup.create`, `backup.restore` | bytes while a phase moves bytes, else null | [`sqlite/family.rs`](../../../../control/crates/layerfs-store/src/backend/sqlite/family.rs), [`jobs/progress.rs`](../../../../control/crates/layerfs-store/src/backup/jobs/progress.rs) |
| `project.export` | `done` the bytes written so far, `total` always null (no prescan) | [`project/export/writer.rs`](../../../../control/crates/layerfs-store/src/data/project/export/writer.rs) `phase_event` |
| `store.compact`, `store.upgrade` | null: one event as each phase starts | [`sqlite/family.rs`](../../../../control/crates/layerfs-store/src/backend/sqlite/family.rs) |
| `machine.start`, `machine.stop`, `machine.create`, `machine.remove`, `machine.load_image`, `sandbox.create`, `sandbox.remove` | the call's phases: `done` those finished, `total` how many there are (a copied image's `send` counts bytes) | [`src/lifecycle/progress.rs`](../../../../control/crates/layerfs-machine/src/lifecycle/progress.rs) `Phases` |
| `home.doctor`, `project.doctor`, `sandbox.doctor`, `machine.doctor`, `store.doctor` | things checked; each event adds `node`, the finished check | [`doctor/runner.rs`](../../../../control/crates/layerfs-inspect/src/doctor/runner.rs) `finished` |
| `workspace.exec` | `{ phase: "start", done: 0, total: 1, exec }`, once, before the command starts; `exec` is the id `workspace.cancel --exec` takes | [`exec/call.rs`](../../../../control/crates/layerfs-workspace/src/exec/call.rs) |

| Call | Phases, in order |
| --- | --- |
| `project.init` | `check`, `send` (worker Stores only: first `done` 0, then one per 8 MiB of tree records, last `done == total`), `install`, `record` |
| `project.export` | `prepare`, `export` (at most one per 200 ms, last at the final count), `finalize` (after the staging folder is renamed onto the destination) |
| `store.move` | `check`, `snapshot`, `send` (8 MiB chunks, [`step_run.rs`](../../../../control/crates/layerfs-store/src/backend/sqlite/step_run.rs) `CHUNK`), `verify`, `install`, `record`, `retire` |
| `store.compact` | SQLite: `mark`, `build`, `verify`, `install` (a run with nothing to rebuild sends `mark` alone); PostgreSQL: `mark`, `build`, `swap`, `cleanup` |
| `store.upgrade` | SQLite: `check`, `convert`, `verify`, `commit`; PostgreSQL: `check`, `convert` (the server runs the rest in one message). `commit` is the point of no return: a cancel is no longer read, and a front end disables Cancel there; a Store already current sends `check` alone |
| `backup.create` | `check`, `snapshot`, `send` (from a machine to this computer), `hash`, `upload` |
| `backup.restore` | `check`, `download`, `verify`, `install` |
| `machine.load_image` | `inspect`, `lock`, `export`, `send`, `unpack` |
| `machine.start` | `bundle`, `image` (a docker machine loading the bundle's image), `boot`, `worker` |
| `machine.stop` | `check`, `stop` |
| `machine.create` | `bundle`, `boot`, `worker` |
| `machine.remove` | `check`, `delete` |
| `sandbox.create` | with `--image`, the copy's `inspect`, `lock`, `export`, `send`, `unpack`; then `start` and, with `--mount`, `mount` |
| `sandbox.remove` | `cancel` (with `--force`), `stop`, `delete` |
| `home.doctor` | `self`, then `machines`, `stores`, `buckets` |

Null means unknown or not counted. Only the worker route of `project.init`
measures the tree first, so `total` is not available on every route. There
is no sequence number, timestamp, call id or "phase finished" event. Each
family page lists its calls' phases; the doctors' report is in
[the doctors](../../control/doctor/README.md).

### Delivery

| Front end | Progress reaches | A handler that fails | Source |
| --- | --- | --- | --- |
| Node | `on_progress(event.progress)`, the payload only. The addon counts queued events and the call settles only after all are delivered | a synchronous throw becomes `LayerFSHandlerWarning` and later events still arrive; an `async` handler's rejected promise is not awaited, so Node's unhandled-rejection rule applies | [`index.js`](../../../../api/node/src/index.js), [`layerfs-node/src/call.rs`](../../../../api/crates/layerfs-node/src/call.rs) |
| Python | `on_progress(event["progress"])` on the calling thread; aio queues on the loop and awaits a barrier behind the events before returning | `sys.unraisablehook`; aio: the loop's exception handler; later events still arrive | [`_client.py`](../../../../api/python/src/layerfs/_client.py), [`aio.py`](../../../../api/python/src/layerfs/aio.py), [`layerfs-python/src/call.rs`](../../../../api/crates/layerfs-python/src/call.rs) |
| CLI | with `--progress`: a status line on a terminal stderr, else (or with `LAYERFS_PROGRESS_JSON` set) the event itself as one JSON line on stderr (`{"phase","done","total",…}`, without the `progress` wrapper); written before the result | a write that fails is ignored; the exit code is the outcome's | [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs) |

Without a function (SDK) or `--progress` (CLI), progress events are dropped.
An event with neither a `progress` nor a `stream` key reaches no function in
any front end. Rate: `project.export` coalesces to at most one `export`
event per 200 ms (`PROGRESS_INTERVAL`); backups send at most one byte update
per 250 ms ([`bucket/progress.rs`](../../../../control/crates/layerfs-store/src/backup/bucket/progress.rs)
`INTERVAL`); the other calls send every event, bounded by structure (phases,
8 MiB chunks, checked things), never per file. The Node addon queues events in an
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

Output is answered after the command ends; it is never live.

```text
 command --> stdout, stderr  (output.rs, each kept in memory up to 1 MiB)
                  |
       stream <= 1 MiB (INLINE)            stream > 1 MiB
       answered whole                      written, kept bytes first, to
                                           <run folder>/exec/<workspace id>/
                                           <exec id>.stdout | .stderr;
                                           answered: its last 1 MiB and
                                           stdout_file | stderr_file
                  |                                 |
                  |                        drain: the whole file as 1 MiB
                  |                        pages, stdout then stderr
                  v                                 v
          one { stream, bytes } event per non-empty inline stream, after
          the pages: stdout / stderr functions, or --stream; else dropped
```

| Rule | Source |
| --- | --- |
| Each stream is kept in memory up to 1 MiB (`INLINE`); a stream that fits is answered whole | [`output.rs`](../../../../control/crates/layerfs-sandbox/src/exec/output.rs) |
| A longer stream is written to a file beside the Sandbox's daemon log, `<run folder>/exec/<workspace id>/<exec id>.stdout` (or `.stderr`); the answer carries its last 1 MiB in `stdout`/`stderr` and the file in `stdout_file`/`stderr_file`. `sandbox files` reads the file and the Workspace's unmount deletes it | [`output.rs`](../../../../control/crates/layerfs-sandbox/src/exec/output.rs), [`exec/call.rs`](../../../../control/crates/layerfs-workspace/src/exec/call.rs) `answer` |
| `stdout_bytes` and `stderr_bytes` (long form) give each stream's full size | [`output.rs`](../../../../control/crates/layerfs-sandbox/src/exec/output.rs) `fields` |
| A spilled stream is also sent to the caller whole, as `{ stream, bytes }` events of at most 1 MiB, stdout then stderr, before the call answers | [`send.rs`](../../../../control/crates/layerfs-sandbox/src/route/machine/send.rs) `drain` |
| A non-empty inline stream is sent as one event after the answer is built; an empty one sends none. There is no chronological interleaving | [`exec/call.rs`](../../../../control/crates/layerfs-workspace/src/exec/call.rs) |
| Without a function or `--stream`, events are dropped; a spilled stream's file stays until the unmount | [`send.rs`](../../../../control/crates/layerfs-sandbox/src/route/machine/send.rs) |
| No total is capped (a spilled stream is bounded by free space); the Sandbox backend reads the command's output only as fast as this side takes it; there is no timeout | [`output.rs`](../../../../control/crates/layerfs-sandbox/src/exec/output.rs) |
| An event's `bytes` is text when valid UTF-8, else `{ base64 }` (Node `Buffer`, Python `bytes`); pages cut at byte counts, so one stream may mix both | [`value.rs`](../../../../control/crates/layerfs-home/src/json/value.rs) |
| A spill or drain failure makes the call `Failed` after the command ran, with no exit code in the error | [`output.rs`](../../../../control/crates/layerfs-sandbox/src/exec/output.rs), [`send.rs`](../../../../control/crates/layerfs-sandbox/src/route/machine/send.rs) |

`layerfs workspace exec` exits with the command's own code (0 when it has
none, 1 for a code outside 0 to 255) and prints the result JSON; with
`--stream` it writes the output to this process's stdout and stderr instead
and prints no result. A refusal
exits 125, a usage error 2 ([`parse.rs`](../../../../api/crates/layerfs-cli/src/parse.rs),
[`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs)). A lost reply is
`Unknown` with `exec` beside it and `fix: layerfs workspace cancel
<workspace> --exec <id>`.

## Related

- [Calls](README.md): dispatch, argument kinds and the `PROGRESS` set.
- [SDK](../sdk/README.md): each call's own errors.
- [Front ends](../front-ends/README.md), [bindings](../front-ends/bindings.md): how each shows a refusal and events.
- [Control](../../control/README.md): routing, workers and the daemon channel.
