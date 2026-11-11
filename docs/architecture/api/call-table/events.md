# Events: progress and exec output

[Architecture](../../README.md) › [API](../README.md) › [Call table](README.md) › Events

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

What a running call sends to its caller before it answers: progress events
and the output of `workspace.exec`. This page states the contract as it is
today, with what was run and what was not. It adds no event type and changes
no payload.

| | |
| --- | --- |
| Component | [Call table](README.md) |
| Source | [`common/context.rs`](../../../../api/crates/layerfs-api/src/calls/context.rs), [`calls/table.rs`](../../../../api/crates/layerfs-api/src/calls/table.rs) `PROGRESS`, [`store/family.rs`](../../../../control/crates/layerfs-store/src/sqlite/family.rs) `progress`, [`workspace/calls.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/calls.rs), [`sandbox/route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) `drain`, [`sandbox/output.rs`](../../../../control/crates/layerfs-sandbox/src/output.rs) |
| Front ends | [CLI](../front-ends/cli.md), [Node](../front-ends/node.md), [Python](../front-ends/python.md) |
| Used by | [project.init](../sdk/project/init.md), [project.export](../sdk/project/export.md), [store.push](../sdk/store/push.md), [store.pull](../sdk/store/pull.md), [backup.create](../sdk/backup/create.md), [backup.restore](../sdk/backup/restore.md), [workspace.exec](../sdk/workspace/exec.md) |

**How statements are marked.** *Run* names the test or probe that ran it on
this host (a debug build of `layerfs_v2_0_0` `716197c49`; Linux under WSL2;
Node 24.22, Python 3.14.4). **UNVERIFIED** means it was read from source and
not run; the source is quoted. A claim with neither mark is a definition, not
a measurement.

## Which calls take `on_progress`

Nine calls declare the shared `PROGRESS` option
([`calls/table.rs:134`](../../../../api/crates/layerfs-api/src/calls/table.rs)):

| Call | Declared in | Run |
| --- | --- | --- |
| `machine.start`, `machine.create` | [`machine/start.rs`](../../../../api/crates/layerfs-api/src/machine/start.rs), [`machine/create.rs`](../../../../api/crates/layerfs-api/src/machine/create.rs) | machine startup diagnostics and `progress_options.rs` |
| `project.init` | [`project/init.rs:13`](../../../../api/crates/layerfs-api/src/project/init.rs) | table check: `progress_options.rs`, `progress.mjs`, `test_progress.py` |
| `project.export` | [`project/export.rs:15`](../../../../api/crates/layerfs-api/src/project/export.rs) | same, and its own events: `project_export.mjs`, `test_project_export.py`, `project_export.rs` |
| `store.push`, `store.pull` | [`store/push.rs:13`](../../../../api/crates/layerfs-api/src/store/push.rs), [`store/pull.rs:13`](../../../../api/crates/layerfs-api/src/store/pull.rs) | same |
| `backup.create`, `backup.restore` | [`backup/create.rs:13`](../../../../api/crates/layerfs-api/src/backup/create.rs), [`backup/restore.rs:13`](../../../../api/crates/layerfs-api/src/backup/restore.rs) | same |
| `workspace.exec` (its exec id, layerfs-project#113) | [`workspace/exec.rs`](../../../../api/crates/layerfs-api/src/workspace/exec.rs) | same; the event itself: `layerfs-cli/tests/exec_cancel.rs` (live, native), `exec_cancel_worker.rs` (stand-in worker) |

`workspace.exec` is the only call that declares `stdout` and `stderr`
(Function options). No other option of any call is a function. The CLI
takes `--progress` only on the nine. `store list --progress` remains an unknown
option. Machine events report `{ stage, state, elapsed_ms? }` for validation,
image resolution, provider/worker provisioning and compatibility; completion or
failure includes elapsed milliseconds. A function never crosses into the library:
`check.rs` refuses it as a value ([`calls/check.rs:18`](../../../../api/crates/layerfs-api/src/calls/check.rs)).

## The event stream

Every call gets one event sink, [`Context`](../../../../api/crates/layerfs-api/src/calls/context.rs),
with three senders (`context.rs:21-35`):

| Sender | Event | Used by |
| --- | --- | --- |
| `progress(v)` | `{ progress: v }` | the nine calls above |
| `event(e)` | a worker's event, as it came | calls routed to a worker ([workers](../../control/routing/workers.md)) |
| `output(stream, bytes)` | `{ stream, bytes }` | `workspace.exec` |

A front end hands each event to the caller's function:

| Front end | Progress goes to | Source |
| --- | --- | --- |
| Node | `on_progress(event.progress)`: the payload only | [`index.js:62-79`](../../../../api/node/src/index.js) |
| Python | `on_progress(event["progress"])`: the payload only | [`_client.py:46-58`](../../../../api/python/src/layerfs/_client.py) |
| CLI | with `--progress`, the whole event as one JSON line on stderr: `{"progress":{"phase":"check","done":null,"total":null}}` | [`run.rs:88-95`](../../../../api/crates/layerfs-cli/src/run.rs) |

So the CLI line carries the `progress` wrapper and an SDK function does not.
Without a function (SDK) or without `--progress` (CLI), progress events are
dropped; the CLI prints nothing on stderr then (run: `progress.rs`). An
event that has neither a `progress` key nor a `stream` key (a worker event
of another shape) reaches no function in any front end (UNVERIFIED: no
worker sent one in these runs; Init, push and pull through a worker sent only
the phases below).

## Payload shapes

Five shapes exist today. None has a sequence number, timestamp, call id,
unit field or "phase finished" event. `project.export`'s `finalize` is the one
last event: it is sent after the staging folder has been renamed onto the
destination.

| Shape | Calls | Source | Run |
| --- | --- | --- | --- |
| `{ phase, done, total }` | `project.init`, `store.push`, `store.pull` | [`sqlite/family.rs:242-253`](../../../../control/crates/layerfs-store/src/sqlite/family.rs) `progress` | Init in process, Init through a worker, push, pull: `progress.mjs`; `progress.rs` |
| `{ phase, files, directories, symlinks, bytes, done, total }` | `project.export` | [`project/export/writer.rs`](../../../../control/crates/layerfs-store/src/project/export/writer.rs) `phase_event` | `project_export.mjs`, `test_project_export.py`, `project_export.rs` |
| `{ phase, done, total, pieces }` | `backup.create`, `backup.restore` | [`backup/progress.rs`](../../../../control/crates/layerfs-store/src/backup/progress.rs) `Update::to_sdk` | NOT RUN: needs a bucket. UNVERIFIED |
| `{ exec: { id } }`, once, before the command starts | `workspace.exec` | [`workspace/exec.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/exec.rs) `exec` | `exec_cancel.rs` (CLI `--progress`, library), `exec_cancel_worker.rs` |
| `{ phase, machine }` | `machine.create` | [`machine/create.rs:128-131`](../../../../control/crates/layerfs-machine/src/create.rs) | NOT RUN: needs `lima` on the host. UNVERIFIED |

In `{ phase, done, total }`, `done` and `total` are **bytes** while the phase
moves bytes and `null` in a phase that moves none; the unit is implicit in
the phase. `null` means unknown or not counted. The phases that count bytes
are `send` everywhere, and `snapshot` and `verify` in `store.push` and
`store.pull` (run: `progress.mjs`; see [Phases observed](#phases-observed)).
In `project.export`, the four counters are entry counts and logical file
bytes, `done` is `bytes` in `export` and `finalize` and `null` in `prepare`,
and `total` is always `null`: export does no whole-tree prescan. In
`backup.*`, `done`/`total` are bytes (null when a total is unknown) and
`pieces` is `{ done, of }` while a file goes up or down as pieces, else
`null` (UNVERIFIED by run: needs a bucket).

### Phases observed

For `project.init`, `store.push`, `store.pull` and `backup.*`, phase events
are **entry markers**: a phase is announced when it begins, and nothing
announces that it ended. `project.export` is the exception: its `finalize`
is sent after publication (above).

| Call and route | Phases, in order | Source | Run |
| --- | --- | --- | --- |
| `project.init`, SQLite Store in this process | `check`, `install`, `record`; counts `null`; three events for 20 files and for 2000 | [`project/family.rs:143,170,173`](../../../../control/crates/layerfs-store/src/project/family.rs) | yes: `progress.mjs`, `test_progress.py`, `progress.rs` |
| `project.init`, SQLite Store behind a worker | `check`, `send` (first `done` 0, then one event per 8 MiB of tree records, last `done == total`; a final duplicate `done == total` was observed), `install`, `record` | `project/family.rs:227-260`, [`project/send.rs:378-392`](../../../../control/crates/layerfs-store/src/project/send.rs) | yes, through a loopback stand-in worker: `progress.mjs` |
| `project.init`, PostgreSQL Store | `check`, `install`, `record` | `project/family.rs:306,309` | NOT RUN: no PostgreSQL server. UNVERIFIED |
| `store.push`, `store.pull` | `check`, `snapshot` (entry marker with null counts, then byte counts `0..=total`), `send` (as above, chunk 8 MiB), `verify` (entry marker, then byte counts), `install`, `record`, `retire` | [`store/transfer/phases.rs:293-330`](../../../../control/crates/layerfs-store/src/sqlite/transfer/phases.rs), [`sqlite/family.rs:109-150`](../../../../control/crates/layerfs-store/src/sqlite/family.rs) `step_observed`, [`steps.rs:19`](../../../../control/crates/layerfs-store/src/sqlite/transfer/steps.rs) `CHUNK` | yes, through a loopback stand-in worker: `progress.mjs` |
| `project.export` | `prepare`, `export` (first at zero, then at most one every 200 ms, last at the final counts), `finalize` (after publication) | [`project/export/writer.rs`](../../../../control/crates/layerfs-store/src/project/export/writer.rs) `LocalWriter` | yes: `project_export.mjs`, `test_project_export.py`, `project_export.rs` |
| `backup.create` | `check`, `snapshot`, `send` (a worker Store's staged copy only), `check` again, then `hash` and `upload` from inside the transfer; `{phase,done,total,pieces}` updates | [`backup/create.rs:82-133`](../../../../control/crates/layerfs-store/src/backup/create.rs), [`backup/progress.rs`](../../../../control/crates/layerfs-store/src/backup/progress.rs) | NOT RUN. UNVERIFIED |
| `backup.restore` | `check`, `download` and `verify` reported by the bucket transfer, `install`; `{phase,done,total,pieces}` updates | [`backup/restore.rs:81-104`](../../../../control/crates/layerfs-store/src/backup/restore.rs), [`backup/progress.rs`](../../../../control/crates/layerfs-store/src/backup/progress.rs) | NOT RUN. UNVERIFIED |

The older per-call pages list fewer phases for push and pull; the run above
is the list.

The numbers show:

- **Volume is bounded by structure, not by file count.** Init in process
  sends 3 events for any tree; through a worker, one per 8 MiB of tree
  records plus the phase events (a 40 MB file sent 6 `send` events in a probe
  run, not kept as a test);
  push and pull send one per 8 MiB of Store file plus the phase events, and
  their `snapshot`/`verify` byte counts come from the hashing step.
  There is no per-file event today.
- **Only the worker route of `project.init` pre-walks the tree.** It sums
  the file bytes before the first `send` so `total` is known
  ([`project/send.rs:181`](../../../../control/crates/layerfs-store/src/project/send.rs) `measure`,
  called at `project/family.rs:227`); the in-process route has no counts at
  all. A caller must not read `total` as available in every route.
  `project.export` deliberately keeps `total` null: it does no prescan.
- **`project.export` already throttles in Rust.** Its writer coalesces
  `export` events to at most one per 200 ms
  ([`project/export/writer.rs:16-19`](../../../../control/crates/layerfs-store/src/project/export/writer.rs)),
  besides the first and last. No other call shares that helper (decision 2
  below).

## Ordering and delivery

| Statement | Front end | Run |
| --- | --- | --- |
| every event reaches the function **before the call settles**, in the order sent | Node: the addon counts the events it queued and the package resolves only after delivering that many ([`call.rs:100-118`](../../../../api/crates/layerfs-node/src/call.rs), `index.js:62-91`); Python: on the calling thread, with the interpreter re-attached per event ([`layerfs-python/src/call.rs:28-35`](../../../../api/crates/layerfs-python/src/call.rs)); aio: queued on the loop, and after the thread's result the coroutine queues a barrier behind them and awaits it, so a wrapper that resumes it early (`asyncio.wait_for`) cannot drop them ([`aio.py:63-105`](../../../../api/python/src/layerfs/aio.py)) | yes, all three: `progress.mjs`, `test_progress.py` (every event seen before the call returned); aio: `test_project_export.py` (including a call that never yields) |
| events arrive **while the call runs**, not replayed at its end | Init in process spends its time between `install` and `record`; in a probe run (3000 files, debug build) `install` reached the caller's function at 3 ms and `record` at 1252 ms, the call settling at 1253 ms | yes: the tests assert a gap of more than 10 ms; the CLI test reads stderr as the child runs and sees the first line before the child exits |
| **no event after the call returned** | all three | yes, over a 200 to 300 ms window after the return; by construction otherwise (events are sent on the call's own thread before it returns) |
| the CLI writes each progress line to stderr **before** it prints the result to stdout | CLI | the write order is the code's ([`run.rs:88-111`](../../../../api/crates/layerfs-cli/src/run.rs)); two pipes have no order for a reader of both, so a script that needs it reads stderr to its end first |
| a call refused before its first phase sends no event | CLI: relative `folder` is refused (exit 125) with only the refusal on stderr | yes: `progress.rs` |
| a call that fails after some phases has already sent those events | Node/Python deliver them before throwing | UNVERIFIED for a failing call (the refusal path is the same delivery code) |

## A function that fails

The call is one attempt and the observer cannot change its outcome. What an
observer's failure looks like depends on the front end:

| Front end | A function that throws or raises | Later events | The call and its result | Run |
| --- | --- | --- | --- | --- |
| Node, a synchronous throw | caught; `process.emitWarning(..., "LayerFSHandlerWarning")` | still delivered | unchanged | yes: `progress.mjs` |
| Node, an `async` function that rejects | **not caught**: its promise is not awaited, so Node's unhandled-rejection rule applies, which ends the process by default | the process ends at the first rejection it reports | the process ends while the native call may still be running, so the caller never sees the result and the outcome is not known: read state before acting | yes, in a child process: `progress.mjs` (current behaviour, see [Open decisions](#open-decisions)) |
| Python, sync | `sys.unraisablehook` ("Exception ignored in ...") | still delivered | unchanged | yes: `test_progress.py` |
| Python, aio | the loop's exception handler | still delivered | unchanged | yes: `test_progress.py` |
| CLI | no user function; a stderr line that cannot be written is ignored (`let _ =`, [`run.rs:92`](../../../../api/crates/layerfs-cli/src/run.rs)), also with stderr closed | n/a | exit code is the outcome's | yes: `progress.rs` (closed stderr, exit 0) |

Cancelling a Python coroutine stops its functions and not the native call
(`aio.py:6-9`; the existing `test_calls.py` covers it and was not re-run for
this page). Node has no cancellation option. No event asks the call to stop.

## What completion means

**An event is never proof that anything finished.** The only completion is
the call's result.

- **Bytes moved is not validation, installation or handoff.** `send` reaches
  `done == total` before the call verifies, installs and records. In the
  push, Init and pull runs, `send` hit 100% and two to four phases still
  followed (`verify`, `install`, `record`, `retire`, as the call has them),
  each of which can fail or leave an uncertain outcome. Run: `progress.mjs` asserts that no call
  ends at its last `send`.
- **Phase events are sent on entry.** `record` is sent before the project
  record is written
  ([`project/family.rs:173`](../../../../control/crates/layerfs-store/src/project/family.rs));
  `install` before the add runs. A `record` event does not mean recorded.
- **Success is the resolved or returned value** (the CLI's exit 0 and its
  stdout JSON). A refusal after any number of events is still a refusal; its
  `Unknown` and `Unresolved` codes mean the outcome is not known (see
  [Errors](errors.md#uncertain-outcomes)). No event carries a result.
- **Events imply neither cancellation nor safe replay.** A call is one
  attempt; the library never retries or resumes. An observer that stopped
  listening (a cancelled coroutine, a closed stderr, a handler that threw)
  has learned nothing about whether the call ended. Read state with the
  family's `get`, `list` or `status` before acting again.

## Little throttle, an unbounded Node queue

- `project.export` is the one call that coalesces: its writer emits at most
  one `export` event per 200 ms, besides the first and last
  ([`project/export/writer.rs:16-19`](../../../../control/crates/layerfs-store/src/project/export/writer.rs));
  run: `project_export.rs` `export_progress_events_follow_time_not_entry_count`.
  The other calls send every event: `Context::progress` calls the sink
  synchronously on the call's thread (`context.rs:21-23`), and their counts
  are bounded by structure (above). A future per-file reporter has no shared
  helper and would send one event per file (decision 2 below).
- The Node addon makes its callback with `create_threadsafe_function(0, ...)`,
  which is an **unbounded queue**, and calls it in `Blocking` mode, which
  never blocks on an unbounded queue ([`call.rs:33,105`](../../../../api/crates/layerfs-node/src/call.rs)).
  A slow handler does not slow the call: events wait in memory.
  UNVERIFIED by run (no test floods the queue).
- Python holds the interpreter for each event, so a slow function slows the
  call; aio queues on the loop without a bound (`aio.py:50`). UNVERIFIED by run.
- The CLI writes each line synchronously to stderr; a full pipe blocks the
  call. UNVERIFIED by run.

Anything that adds per-file or per-chunk events (for example a
`project.export`) must bound the rate in Rust, before the Node queue; see
[Open decisions](#open-decisions).

## Historical machine.create event audit

Superseded on 2026-11-09: `machine.create` and `machine.start` now declare
`PROGRESS`; the statements and old receipts below describe the earlier source.


`machine.create` sends `{ progress: { phase: "create", machine } }` after its
record is claimed and before the VM is made
([`machine/create.rs:128-131`](../../../../control/crates/layerfs-machine/src/create.rs)).
Its table entry declares no `on_progress`
([`layerfs-api/src/machine/create.rs:9-23`](../../../../api/crates/layerfs-api/src/machine/create.rs)),
so no front end can pass a function for it: Node and Python refuse the option
(`unknown option "on_progress"`, `InvalidInput`), the CLI refuses
`--progress` (exit 2), and a CLI call drops the event because `--progress`
was not given. Run: `progress_options.rs`, `progress.mjs`,
`test_progress.py`, `progress.rs`. A caller of the Rust library
(`layerfs_api::call` with its own sink) would receive the event. Whether the
event is emitted is UNVERIFIED by run: the code is reached only after the
host's `lima` probe passes (`machine/create.rs:75`), and this host has no
`lima`.

## `workspace.exec` output

Output is **buffered and delivered after the command ends**. It is never
live. Statements marked UNVERIFIED were not run: the exec runs below could
not be made on this host (see [What was run](#what-was-run-and-not-run)).

| Statement | Source | Checked |
| --- | --- | --- |
| The command's two streams are read into two spools; nothing reaches the caller until it exits. Inline stream events are emitted by `exec` after it gets the answer (`workspace/exec.rs:120-129`). | [`exec/run.rs`](../../../../control/crates/layerfs-sandbox/src/exec/run.rs), [`workspace/exec.rs:120-129`](../../../../control/crates/layerfs-sandbox/src/workspace/exec.rs) | UNVERIFIED |
| With both streams inline, events are **all of stdout, then all of stderr**; with both spooled, `drain` pages stdout then stderr; with only one spooled, its pages precede the other's single inline event (open decision 7). There is no chronological interleaving, so `--stream` is not a faithful terminal view. | [`workspace/exec.rs:120-129`](../../../../control/crates/layerfs-sandbox/src/workspace/exec.rs) (inline), [`sandbox/route.rs:533-551`](../../../../control/crates/layerfs-sandbox/src/route.rs) (spooled) | UNVERIFIED |
| A non-empty stream of at most 1 MiB (`INLINE`) is kept in memory, answered in the result and sent as **one** event; an empty stream is answered but sends none. | [`sandbox/output.rs:14,22`](../../../../control/crates/layerfs-sandbox/src/output.rs), `workspace/exec.rs:127` | UNVERIFIED |
| A stream over 1 MiB goes to an unlinked temp file in the host's temp directory; the result has that stream `null` plus `stdout_bytes`, `stderr_bytes`, `streamed: true`; the bytes reach the caller **only** as 1 MiB event pages. | `output.rs:46-70`, `route.rs:527-562` `drain` | UNVERIFIED |
| With no `stdout`/`stderr` function and no `stream`, those pages are drained and **dropped**; only the sizes are answered. The spool is released, so the output cannot be fetched again (the page reader is not a table call). | `route.rs:485`, [`index.js:57-61`](../../../../api/node/src/index.js) | UNVERIFIED |
| `truncated` is always `false`; no total is capped (bounded by free space of the host temp directory); there is no timeout. | `output.rs:1-12`, [`exec.md`](../sdk/workspace/exec.md) | UNVERIFIED |
| **Text or bytes, per event.** An event's `bytes` is printed as text when that event's bytes are valid UTF-8, else as `{"base64": ...}`, which Node reads as a `Buffer` and Python as `bytes`. | [`common/json.rs:44-48`](../../../../control/crates/layerfs-home/src/json.rs) | UNVERIFIED |
| A spooled stream is cut into 1 MiB pages by byte count, not at character boundaries, so one stream can reach a handler as text in one page and bytes in the next. A caller that joins output must accept both. | `route.rs:456-475` | UNVERIFIED |
| Consumer side: nothing bounds what is queued between the drain and a Node handler (see above). | `call.rs:33` | UNVERIFIED |
| A failure of the spool (a full disk) or of the drain makes the call `Failed` **after the command ran**, with no exit code in the error. | `output.rs` `fields`, `route.rs` `drain` (`?`) | UNVERIFIED; see [Open decisions](#open-decisions) |

Staying buffered was the owner's decision for 2.0. The runtime below the
control layer does copy each stream as it arrives (8 KiB windows,
[commands and streams](../../control/sandboxes/commands-and-streams.md)); the
control layer's spools are what hide it.

## What was run and not run

| Run | Result |
| --- | --- |
| `project.init` with progress through Node, Python (sync and aio) and the CLI, local SQLite Store, Disposable temp home on ext4 | events, order, payload, liveness, no events after return, handler failures as tabled above |
| `store.push`, `project.init`, `store.pull` through a loopback `layerfs worker` on a recorded `lima` machine record (the repository's debug worker; no VM) | phase lists and byte counts above (this round: `progress.mjs`, and a probe that printed the events of all three) |
| `project.export` through Node, Python (sync and aio) and the CLI, both Branches, refusals, fidelity and progress | events and final counts above; run: `project_export.mjs` (all 9 cases, none skipped, with `LP53_EXPORT_NATIVE=1` and `LAYERFS_EXPORT_CLI`), `test_project_export.py` (`LP53_EXPORT_NATIVE=1`), `project_export.rs` |
| `workspace.exec` output, over and under 1 MiB, with and without a sink | **NOT RUN (this round).** No exec was attempted: the native Sandbox is refused on this host for lack of user namespaces (recorded in the previous round, when `sandbox.create` refused with `unsupported runtime capability: user namespaces are not blocked`). The statements in [its section](#workspaceexec-output) are read from source and marked UNVERIFIED. |
| `backup.create`, `backup.restore` | **NOT RUN:** need a bucket |
| `machine.create` event | **NOT RUN:** needs `lima` |
| `project.init` into PostgreSQL | **NOT RUN:** no server |
| Flooding the Node queue, a full disk during exec, a slow consumer | **NOT RUN** |

| Test | Command | Needs |
| --- | --- | --- |
| [`progress_options.rs`](../../../../api/crates/layerfs-api/tests/progress_options.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test progress_options` | nothing |
| [`progress.rs`](../../../../api/crates/layerfs-cli/tests/progress.rs) | `cargo +1.85.1 test --locked -p layerfs-cli --test progress` | Unix permissions |
| [`progress.mjs`](../../../../api/node/test/progress.mjs) | `node --test test/progress.mjs` in `api/node` | the built addon; the worker case needs `target/debug/layerfs` and is skipped without it |
| [`test_progress.py`](../../../../api/python/test/test_progress.py) | `python test/test_progress.py` in `api/python` | the built native module |
| [`project_export.mjs`](../../../../api/node/test/project_export.mjs) | `LP53_EXPORT_NATIVE=1 LAYERFS_EXPORT_CLI=<layerfs> node --test test/project_export.mjs` in `api/node` | the built addon and command |
| [`project_export.rs`](../../../../api/crates/layerfs-api/tests/project_export.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test project_export` | nothing beyond Unix metadata |
| [`test_project_export.py`](../../../../api/python/test/test_project_export.py) | `LP53_EXPORT_NATIVE=1 python -m unittest test_project_export` in `api/python` | the built native module |

## Open decisions

None of these is fixed here: each needs a payload, call-table or
control-plane change. Each has evidence and a proposal.

| # | Finding | Evidence | Proposal |
| --- | --- | --- | --- |
| 1 | **`machine.create` emits an event no caller can receive.** | table lacks `on_progress`; refusals in all front ends (run); emit at `machine/create.rs:128` | delete the emit (one phase, nothing to report), unless a consumer wants the event: then declare `PROGRESS` on the call and document `{ phase, machine }` here |
| 2 | **No shared throttle, unbounded Node queue.** Safe today because the only per-entry reporter, `project.export`, coalesces in its own writer; no other call reports per file. | `context.rs:21`, `layerfs-node/src/call.rs:33,105`, `project/export/writer.rs:16` | one shared coalescing helper in `layerfs-api/src/calls` used by every progress call (by bytes and time), so a per-file reporter cannot flood the queue; do it in Rust, not in the Node package |
| 3 | **An `async` progress function that rejects ends the Node process** while a synchronous throw is a warning. | run: `progress.mjs` | in `bind` (`index.js:70`), catch the returned promise and emit the same `LayerFSHandlerWarning`; a one-line change that was not made here because it changes the handler contract |
| 4 | **Phase events mark entry only.** There is no "phase finished", failure or terminal event, so a reader cannot tell the last phase's outcome from the stream. | `sqlite/family.rs:242-253` | keep it: the result is the only completion. State it in every `on_progress` page (this page is the reference). Add terminal events only with a payload extension |
| 5 | **A failure after the command ran is `Failed`.** A spool or drain error after `workspace.exec` ran the command is reported as `Failed` ("did not happen" in [Errors](errors.md)) with no exit code. | `sandbox/output.rs` `fields`, `sandbox/route.rs` `drain`; needs a disk-full test, NOT RUN | classify it as an uncertain outcome (`Unknown`) that names the exit code; belongs to the control-plane lane (#23) |
| 6 | **Exec output is buffered, two spools, mixed text and bytes, dropped without a sink.** | the exec table above, all UNVERIFIED | keep buffered (owner decision). If the typing matters to a consumer, send every page as bytes or add a `text` flag: a payload change. Live output needs an event-emitting sink in `sandbox/runtime.rs` and a thread-safe event path (`Context.events` is a single-threaded `&mut dyn FnMut`) |
| 7 | **Mixed inline/spooled output is ordered by spool, not by stream.** When exactly one of the two streams exceeds 1 MiB, the worker's `drain` sends that spooled stream's pages before the caller emits the other stream's single inline event, so stderr may precede stdout. | `route.rs` `drain` (sends only a `null`/spooled stream), `workspace/exec.rs` (emits inline streams after the worker answers); UNVERIFIED by run | either state the mixed case in [exec.md](../sdk/workspace/exec.md) instead of "all of stdout, then all of stderr", or spool both streams when either exceeds `INLINE` so one place orders both |

## Related

- [Table](table.md): `Function` kinds and the `PROGRESS` set.
- [Errors](errors.md): `Unknown`, `Unresolved` and what a refusal after events means.
- [Front ends](../front-ends/README.md): how each one shows events.
- [workspace.exec](../sdk/workspace/exec.md): the call whose output this page describes.

---

← [Call table: Errors](errors.md) · [Up: Call table](README.md) · [API: SDK](../sdk/README.md) →

## CLI framed output

`layerfs workspace exec <ws> <command> --events` delivers every output byte
as NDJSON output events, followed by one `{result}` or `{refusal}` frame.
It includes progress and is exclusive with `--stream` and `--text`. A result
exits 0 regardless of the command's exit code; refusal exits 125 and usage
exits 2. Bytes use UTF-8 text or `{base64: ...}`. A failed stdout exits 125
with effects kept and delivery incomplete; never replay the operation.
