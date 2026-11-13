# Call trace

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-sdk-observe` skill. **In this 2.0 build the traces are the
> worker-connection line** (#535 phase 8 item 6): with `LAYERFS_TRACE=<file>`,
> each connection the caller opens to a machine's worker appends one line,
> **and the step lines**: with `LAYERFS_TRACE_STEP` set too, each call
> appends one line per step it timed. Calls that run in the caller's process
> (`native`, any PostgreSQL Store call) write no connection line.
> **Independently, the home's call log** (#659) records one line per public
> call in `<home>/trace/calls.jsonl`, on by default. The call-line format
> further down is 1.1.0's (#333), kept for reading old traces.

## The call log

Every call made through the dispatcher (the `layerfs` command, Node,
Python, direct `layerfs_api::call`) appends one line to
`<home>/trace/calls.jsonl` when it ends; `LAYERFS_LOGS=0` turns it off,
and `LAYERFS_TRACE` / `LAYERFS_TRACE_STEP` do not affect it.

```json
{"v":1,"at":1793000004120,"ms":212,"client":"node","argv":["file","write","demo/app","main","/index.ts","<5321 bytes>"],"outcome":"ok","kind":"Committed","commit":"<66 hex>","tag":"dsh:s_4d2a:a91e"}
```

It keeps 1.1.0's redaction rule (values cut to 200 characters; contents,
JSON, commands, messages, edit strings, passwords and URLs as byte counts)
and drops the daemon telemetry: no `sizes`, `scope` or `daemon`. `argv` is
the call as CLI words (`argv[0]`/`argv[1]` are the family and call),
`outcome` is `"ok"` or the refusal code, `kind` and `commit`/`layer` come
from the result, `client` is `cli`/`node`/`python`/`rust`, and `tag` is the
caller's (`log_tag` argument, else `LAYERFS_LOGS_TAG`). Read it with
`layerfs logs list` (filters `--op`, `--project`, `--client`, `--tag`,
`--outcome`, `--since`; newest first; `--cursor` pages) and clear it with
`layerfs logs reset`. It rotates once to `calls.1.jsonl` past 16 MiB. See
[Home: traces and the call log](../../../../docs/architecture/control/home/README.md#traces-and-the-call-log).

```bash
# slowest calls in the home's log
layerfs logs list --limit 1000 | jq -c '.lines[] | [.ms, .argv[0] + "." + .argv[1]]' | sort -rn | head
# refusals
layerfs logs list --limit 1000 | jq -c '.lines[] | select(.outcome != "ok") | [.at, .argv, .outcome]'
```

## The worker-connection line

```json
{"at":1793000000000,"event":"worker.connection","op":"file.read","worker":"127.0.0.1:52011","sent":true,"ms":14}
```

| Field | Meaning |
| --- | --- |
| `at` | milliseconds since the Unix epoch, written when the connection ended |
| `op` | the request it carried; `hello` for a connection that only read the worker's hello |
| `worker` | the worker's loopback address on the caller's host |
| `sent` | whether the request went out (`false`: the worker was not reached or was of another build, and nothing was sent) |
| `ms` | the connection's whole time: connect, secret, request, events and answer |

One worker-routed call is one line (R1). Written by
`control/crates/layerfs-home/src/trace.rs` `connection`; a line that
cannot be written changes no outcome.

## The step lines

With `LAYERFS_TRACE=<file>` and `LAYERFS_TRACE_STEP` set (any non-empty
value), each call appends, when it ends, one line per step it timed:

```json
{"at":1793000000000,"event":"step","op":"workspace.exec","step":"worker.exec.helper_spawn","us":1830,"ms":1}
```

`us` is the step's time in microseconds, `ms` the same in whole
milliseconds. Steps without a prefix ran in the caller, each the time since
the one before: `dispatch` (call start to the call's own work: argument
checks, platform policy, home and built-in machine records), `route` (to
the worker connection's start: the call's records and route), `connect`
(TCP connect), `ready` (secret sent, `ready` read), `request` (request
written) and `answer` (to the answer frame: the worker's whole run). A
worker-routed call asks the worker for its own steps (`trace_step: true`
in the request); the worker times them from reading the request and sends
them back beside its answer (`steps: [{step, us}]`); the caller writes them
as `worker.<step>`. A worker without the field sends none, and a request
without it gets none back. `project.init` into a worker's Store sends its
`project.stage` parts several at once: each part's `route` ... `answer` and
`worker.*` steps are written as its part is answered, so they overlap,
and one `send` step then holds the whole copy's wall time.

Worker steps of `workspace.exec`, each the time since the one before:
`records` (Store and project records), `host_args` (deploy settings, run
root), `store_open`, `by_name` (message check, Workspace by name),
`registry` (the Sandbox), `exec_record` (directory, token, Workspace list,
exec record), `command` (the whole command run), `exec_record_end`,
`result` (the answer's value, spooled output drained). A namespaces
Sandbox breaks `command` down as `exec.endpoint`, `exec.cgroup_create`
(the command's cgroup made),
`exec.helper_spawn`, `exec.helper_wait` (helper started to helper ended,
with output copied) and `exec.cgroup_remove`. Its helper, given
`--trace <file>`, writes when each of its stages ended (wall clock, written
once the command ended) and the worker adds them, each since the one
before: `helper.start` (spawn call to the helper's own start),
`helper.cgroup_join` (the helper joins the Sandbox's group),
`helper.setns` (three `setns`, no-new-privs), `helper.bash_spawn` (the
command started; with its own cgroup it moved itself there just before
exec), `helper.ready` (the backend told it runs, so its stop handle can be
recorded; a command with its own cgroup only), `helper.bash` (the command
ran) and `helper.exit`
(to the worker's wait returning). These overlap `exec.helper_spawn` and
`exec.helper_wait`; they are a breakdown, not further time.

Untraced, a step mark is one thread-local check: nothing is timed or sent,
and the helper gets no file. A step line that cannot be written changes no
outcome. Written by `layerfs-home/src/trace.rs` (`stepped`, `step`),
`layerfs-sandbox-backend/src/steps.rs` and the helper's `exec.rs`.

## Where it stands

| Piece | 1.1.0 | 2.0 build |
| --- | --- | --- |
| Settings `LAYERFS_TRACE`, `LAYERFS_TRACE_STEP`, `LAYERFS_TRACE_START` | read by `@layerfs/agent` (`api/agent/sdk/src/trace.js` at tag `v1.1.0`) | kept as the only settings besides `LAYERFS_HOME` (S §2.5); `LAYERFS_TRACE` is read for the worker-connection line, and with `LAYERFS_TRACE_STEP` for the step lines; `LAYERFS_TRACE_START` is not read |
| The trace line writer | in that JavaScript file | deleted with `@layerfs/agent` (#533 step 14); the call log (#659, `layerfs-home/src/logs.rs`) is its per-call replacement, without daemon telemetry |
| In-process collector | `host/trace.rs` in the Node binding | `api/crates/layerfs-api/src/host/trace.rs`: takes a 64-hex scope from an `observe` argument, traces the call's daemon control exchanges under it and collects up to 64 telemetry values. **No front end calls it** (`take_scope` and `run` have no callers) |
| The home's `trace/` folder | `traces/` | `paths::trace_dir` (a 1.1.0 `traces/` folder is renamed to it); O26 makes trace files there the feed. Holds the call log, `calls.jsonl` and `calls.1.jsonl` (#659) |

So, today: the call log records every call's outcome and time; turning
tracing on records worker connections and, with `LAYERFS_TRACE_STEP`, each
call's step times. For storage work use Commit
records and messages ([storage metrics](storage-metrics.md#one-row-per-step)),
and report daemon-side timing (1.1.0's `daemon` entries) as not collected.

## The 1.1.0 settings

| Setting | Effect in 1.1.0 |
| --- | --- |
| `LAYERFS_TRACE=<file>` | Append one JSON line per call; read when the client was made |
| `LAYERFS_TRACE_STEP=<label>` | Put `step` on each line; read at every call; cut to 200 characters |
| `LAYERFS_TRACE_START=1` | Add a start line (`phase: "start"`) before each traced call |

Off meant no file, no daemon observation and byte-identical replies. A failed
trace write was dropped and never changed a result. A step label alone traced
nothing.

## Reading a 1.1.0 trace line

| Field | Meaning |
| --- | --- |
| `layerfs_trace` | Format version, `1` |
| `id` | `<run>-<n>`: per-process run prefix and sequence number. Join by `id`; lines are in completion order |
| `step` | The step label, when set |
| `method` | 1.1.0 method name, such as `fs.write`, `workspace.commit`, `backend.store.project.get_storage` (2.0 names: `file.write`, `workspace.commit`; the backend calls are gone) |
| `project`, `args` | The project, and an argument summary: paths, refs, names, Sandbox number, command, message (cut to 200 characters); contents only as byte counts |
| `start`, `ms` | Wall start and elapsed milliseconds in the calling process |
| `ok` | `true`, or `false` with `code` and `message` |
| `kind` | The result's `kind` |
| `commit`, `layer` | The new Commit (or the head when `UpToDate`); `branch.publish` adds `layer` |
| `sizes` | `logical`, `newCanonical`, `newStored`, `newMetadata` when the reply carried a Commit record |
| `scope` | 64-hex daemon observation scope; joins to daemon records |
| `daemon` | One entry per control exchange; absent if no daemon was reached |

Each `daemon` entry may hold `timing` (phase trees `{ name, ms, children }`, at
most 256 nodes, `omitted` counts the rest), `sql` (Store SQL work as `writer`
and `readers` deltas of 27 counters), `delta` (delta selection work on a Save),
`fuse` (kernel opcode counts on status and unmount), `storage` (42 Save
counters, on a committing exchange), `construction` (31 captured-namespace
counters) and `diagnosticError` (`{ kind, message }` when the daemon could not
write its records; the call was unaffected).

**Timing roots**

| Root | Where | Children |
| --- | --- | --- |
| `commit` | `workspace.commit` | `capture`, `producer`, `begin`, `construct` › `captured.namespace`, `head`, `merge`, `logical.known`, `finish`, `logical.count`, `validate`, `publish`, `install` |
| `files.bind`, `commit` | `fs` writes | as above, without storage/construction counters |
| `sandbox.mount.bind` › `store.bind` | `workspace.mount` | |
| `sandbox.mount.attach` › `kernel.attach` | `workspace.mount` | |
| `sdk.read`, `sdk.lines` | `fs.read` | `content.read` |
| `branch.merge`, `branch.merge.publish` | `branch.merge` | merge tree; the one history write that publishes a clean merge |
| `blame.compare` | `fs.blame` | |

```bash
# slowest calls
jq -c 'select(.ms) | [.ms, .method, .step]' trace.jsonl | sort -rn | head
# per-method p50 / max (jq 1.6+)
jq -s 'group_by(.method) | map({method: .[0].method, n: length, p50: (map(.ms) | sort | .[length/2|floor]), max: (map(.ms) | max)})' trace.jsonl
# phase breakdown of one call
jq -c 'select(.id == "9f3c21aa-12") | .daemon[].timing' trace.jsonl
# refusals
jq -c 'select(.ok == false) | [.id, .step, .method, .code, .message]' trace.jsonl
```

#333's one-sample diagnostic measured about 1–2 ms of tracing overhead per
single-exchange call and 3–6 ms per multi-exchange call. Compare times only
within one tracing setting.
