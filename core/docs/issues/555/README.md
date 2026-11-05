# 555: step runtime — `exec_async` / `exec_wait` / `exec_status` outside the Sandbox compute plane

> **Status:** implemented (2026-11-05), except the Store-level receipt
> trailer, which is deliberately not done (see below). The runtime, the real
> process launcher and the four call-table entries exist and are covered by
> contract tests.

## Where it lives

`api/crates/layerfs-control/src/step/`, the launcher's management library
under the SDK call surface. The layer order is `core < engine < connect <
api`, so this module may call a Sandbox, but it does not: it drives one
`Launcher` port and writes the home's record files. The real adapter
(`ProcessLauncher`) starts a detached process group under the home's
`step/launch/` and changes no daemon protocol; there is no core queue, actor
or agent loop. `api/crates/layerfs-api/src/step/` holds the four entries of
the `step` family, so the `layerfs` command, Node and Python build them from
the one call table.

## Contract

| Call | Rust | Answers |
| --- | --- | --- |
| `exec_async(call)` | `StepRuntime::exec_async(ExecRequest)` | `Started { exec_id, reused, status }` without waiting |
| `exec_wait(execId, wait)` | `StepRuntime::exec_wait(&ExecId, Duration)` | a terminal `ExecStatus`, or `Running` when the wait bound passes first |
| `exec_status(execId)` | `StepRuntime::exec_status(&ExecId)` | `ExecStatus` now, reconciling a passed deadline |
| `cancel(execId)` | `StepRuntime::cancel(&ExecId)` | `Cancelled`, `TimedOut` or `Unknown` |
| receipt | `StepRuntime::receipt(&ExecId)` | the durable `ExecReceipt`, or `None` |

`ExecStatus` is `Running`, `Succeeded { exit_code }`, `Failed { exit_code }`,
`Cancelled`, `TimedOut`, `Unresolved` or `Unknown`. `ExecId` is the string
`exec-<16 lowercase hex>`: a string crosses a process boundary where an
`AbortSignal` does not, so a cancel can name the exec from any later launcher.
`ExecRequest` carries the caller's idempotency `key`, the `command` and an
optional `timeout_ms`; `Some(0)` is already due.

## Call table

The `step` family has four entries in the one table:

| Call | Arguments | Result |
| --- | --- | --- |
| `step.exec_async(key, command, { timeout_ms })` | a key (1..=128 bytes), a command, an optional deadline | `{ exec_id, reused, status, exit_code? }` |
| `step.exec_wait(exec_id, { wait_ms })` | an exec id, an optional wait bound (default 30000) | `{ exec_id, status, exit_code? }` |
| `step.exec_status(exec_id)` | an exec id | `{ exec_id, status, exit_code? }` |
| `step.cancel(exec_id)` | an exec id | `{ exec_id, outcome }` |

`status` is `running`, `succeeded`, `failed`, `cancelled`, `timed_out`,
`unresolved` or `unknown`; `outcome` is `cancelled`, `timed_out` or
`unknown`. The command and its deadline are the launcher's, not the daemon's;
`exec_wait` answers `running` when its wait bound passes first and a later
call resumes from the durable entry.

## Real launcher adapter

`ProcessLauncher` runs one `bash -c` command in a new process group with its
stdin closed and stdout/stderr redirected to files under
`step/launch/<exec>/` (private, 0600). A small wrapper writes the exit code
there, and the pid separately, so the receipt lives outside the launcher
process. The durable entry stores the handle (pid and file paths), so
`exec_status`, `exec_wait` and `cancel` on a **new** launcher instance read
the same files; a command started before a restart is still observed.
`cancel` signals the process group once with `TERM`, and the wrapper records
143. The adapter never blocks: it starts the process, writes the pid and
returns. It is the host-process compute path; a Sandbox-backed launcher can
implement the same port later without touching the runtime.

## Durable wait store

One backend, no new database: the home's record files.

- `step/<exec>.json` is the durable entry: key, command, deadline, status,
  exit code, the launcher's opaque handle, `started` and `updated`. It is
  replaced whole by the home's atomic write-and-rename, so a reader sees the
  old entry or the new one.
- `step/index/<digest>.json` is the idempotency unique index. It is written
  to a private temporary file, synced, and linked under its final name; the
  exclusive link means exactly one process can claim a key across processes.
  The file keeps the whole key, the command and the deadline, so a retry is
  compared against the first request: the same request answers the first
  receipt and never calls `start` again; a different command or deadline is
  refused `Conflict`.
- A per-entry `step/<exec>.lock` serializes the terminal transition, so a
  cancel and a passed deadline cannot both win. The first terminal writer
  fixes the outcome.

## Store-level receipt trailer (not done)

The remaining #555 box asked for the receipt trailer and unique index at the
Store level. That is **not implemented**, and no Store change is faked: a
trailer and unique index inside a Store commit receipt would change the
Store's schema and commit path for a wait that is launcher-owned and lives
entirely under the home. That is larger than this slice. The `step/index`
file above therefore remains the unique index, and `step/<exec>.json` remains
the receipt; both are written whole by the home's atomic write-and-rename,
and a retry compares the request trailer kept in the index. A Store-level
trailer can replace the file index later without changing this contract.

## Outcomes that must not be conflated

- `cancel` records `Cancelled` only before the deadline; a deadline that
  passed first records `TimedOut`, and a later `cancel` keeps answering
  `TimedOut`.
- A timeout signals the launcher once; a `cancel` that finds a terminal entry
  signals nothing again. `Cancelled` and `TimedOut` are distinct values and
  distinct recorded states.
- No entry is `Unknown`. An entry whose outcome cannot be known is
  `Unresolved`: it is recorded once and is terminal, so a later observation
  never replays it. A failed `start` is recorded `Unresolved` too, so a retry
  does not run a second copy.

## Restart

The wait is not an in-memory promise: the entry holds the status, the handle
and the deadline, and `exec_wait` / `exec_status` re-read it. A new launcher
instance therefore resumes the wait from the entry
(`a_new_launcher_resumes_a_live_command`), and the process adapter's pid,
output and exit receipt live under `step/launch/`, outside the launcher, so
a command that is still running is observed after the restart instead of
being replayed.

## Tests

`api/crates/layerfs-control/tests/step_runtime.rs` uses a launcher double and
a manual clock, so every state is deterministic and no command is run.
`api/crates/layerfs-control/tests/step_launcher.rs` runs real commands
through `ProcessLauncher` (start/observe, restart resume, cancel, timeout,
retry). `api/crates/layerfs-api/tests/step_calls.rs` runs the four calls
through the one table and the CLI's dispatcher, and
`api/crates/layerfs-api/tests/calls_table.rs` checks the family's entries and
counts.

```bash
CARGO_HOME=/tmp/cargo-home cargo test -p layerfs-control --test step_runtime --test step_launcher
CARGO_HOME=/tmp/cargo-home cargo test -p layerfs-api --test calls_table --test step_calls
```

The deterministic file covers: `exec_async` returns before the command ends;
status resolves success/failure; a wait observes until the command ends;
cancel before the deadline is `Cancelled`; a passed deadline is `TimedOut`; a
retry with the same key starts no second copy and answers the same receipt; a
different command on the same key is `Conflict`; a new launcher resumes from
the durable entry; a gone process is `Unresolved` and is never replayed; a
failed start is recorded and not retried; an unknown id is `Unknown` with no
cancellation; the contract checks its inputs. The real-command tests add:
start/observe through the receipt files; a still-running command observed by
a new launcher; cancel ends the process group and never becomes a success; a
timeout stays a timeout; a retry runs exactly one process.

## #555 boxes

- [x] `exec_async` / `exec_wait` / `exec_status` contract.
- [x] launcher-owned helper: the launcher owns the wait; the daemon is not an
      exec supervisor.
- [x] durable wait: a wait has a persistent entry and survives a launcher
      restart; no Durable Object dependency.
- [x] cancel by id.
- [x] `TimedOut` is not `Cancelled`.
- [x] idempotent exec receipt.
- [x] no retry / no replay; an unknown outcome is `Unknown` / `Unresolved`.
- [x] no core queue or actor; no agent loop or LLM in this layer.
- [x] the thin launcher adapter that runs a real command outside the daemon
      (the host-process `ProcessLauncher`) and can still observe a live
      command after a launcher restart.
- [x] the `layerfs-api` call entries and the JS/Python package for the four
      calls.
- [ ] the Store-level receipt trailer and unique index; the `step/index` file
      above stands in for this slice, and the Store was deliberately not
      changed (see above).
