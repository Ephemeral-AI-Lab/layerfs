# step calls

> **Status:** New in #555. The runtime is
> `api/crates/layerfs-control/src/step/`, the four entries are
> `api/crates/layerfs-api/src/step/`, and `ProcessLauncher` runs a real
> `bash -c` command in a detached process group. It changes no daemon
> protocol and adds no core queue or actor.

```js
step.exec_async(key, command, { timeout_ms })
step.exec_wait(exec_id, { wait_ms })
step.exec_status(exec_id)
step.cancel(exec_id)
```

Start one command asynchronously, observe it by id from any later process,
wait for it or cancel it. The command is a real process with its own group;
its pid, stdout, stderr and exit receipt live under
`$LAYERFS_HOME/step/launch/<exec_id>/`. The wait is durable: the id is a
string and the home's `step/<exec_id>.json` entry holds the status, the
handle and the deadline, so a new client resumes the wait instead of starting
a second copy.

## `step.exec_async`

`step.exec_async(key, command, { timeout_ms })`

Starts `command` with `bash -c` and returns without waiting. `key` is the
caller's idempotency key: 1 to 128 bytes, no NUL. A retry with the same key
and the same command and deadline answers the first exec, `reused: true`, and
starts nothing; a different command or deadline on the same key is refused
`Conflict`. `timeout_ms` is the deadline; `0` is already due.

**Returns:**

| Field | Meaning |
| --- | --- |
| `exec_id` | `exec-<16 lowercase hex>`: pass it to the other three calls |
| `reused` | Whether an earlier request already claimed the key |
| `status` | `running` for a new exec, else the existing entry's status |
| `exit_code` | The exit code when the status already carries one |

## `step.exec_wait`

`step.exec_wait(exec_id, { wait_ms })`

Waits up to `wait_ms` (default 30000) for a terminal status. It answers as
soon as one is recorded, and `running` when the bound passes first (the
command keeps running; call it again later).

## `step.exec_status`

`step.exec_status(exec_id)`

The status now. A passed deadline is reconciled to `timed_out` and signals
the command once.

## `step.cancel`

`step.cancel(exec_id)`

Cancels by id. Before the deadline it records `cancelled`; after a deadline
that passed first it answers `timed_out` and does not signal again; no
entry or an already finished exec answers `unknown`. A cancellation signals
the launched process group `TERM` once.

## Statuses

| Status | Meaning |
| --- | --- |
| `running` | Started, no terminal outcome recorded |
| `succeeded` | Ended with exit code 0 |
| `failed` | Ended with a non-zero exit code |
| `cancelled` | A cancellation was recorded before the deadline |
| `timed_out` | The deadline passed first; not a cancellation |
| `unresolved` | An entry exists but its outcome cannot be known; terminal, never replayed |
| `unknown` | No entry names the id |

```js
const started = await layerfs.step.exec_async("build-1", "npm test", { timeout_ms: 600000 });
const waited = await layerfs.step.exec_wait(started.exec_id, { wait_ms: 60000 });
if (waited.status === "running") {
  // the command continues; ask again later
}
```

```bash
id=$(layerfs step exec_async build-1 "npm test" --timeout-ms 600000 | jq -r .exec_id)
layerfs step exec_wait "$id" --wait-ms 60000
layerfs step exec_status "$id"
layerfs step cancel "$id"
```

Refusals: `InvalidInput` (a key or command outside its shape, a bad exec id,
a negative deadline or wait), `Conflict` (the same key names another
command or deadline), `Failed` (the command could not be started; the entry
is recorded `unresolved`, so a retry does not run a second copy).
