# Call trace

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-sdk-observe` skill. **In this 2.0 build no trace is written.**
> Checked on 2026-11-02: no code under `engine/`, `api/` or `core/crates`
> reads `LAYERFS_TRACE`, and `LAYERFS_TRACE=<file> layerfs file read …` left
> no file. The line format below is 1.1.0's (#333), kept for reading old
> traces and as the target the settings are reserved for.

## Where it stands

| Piece | 1.1.0 | 2.0 build |
| --- | --- | --- |
| Settings `LAYERFS_TRACE`, `LAYERFS_TRACE_STEP`, `LAYERFS_TRACE_START` | read by `@layerfs/agent` (`api/agent/sdk/src/trace.js` at tag `v1.1.0`) | kept as the only settings besides `LAYERFS_HOME` (S §2.5), but **read by nothing** |
| The trace line writer | in that JavaScript file | deleted with `@layerfs/agent` (#533 step 14); no Rust replacement yet |
| In-process collector | `host/trace.rs` in the Node binding | `api/crates/layerfs-api/src/host/trace.rs`: takes a 64-hex scope from an `observe` argument, traces the call's daemon control exchanges under it and collects up to 64 telemetry values. **No front end calls it** (`take_scope` and `run` have no callers) |
| The home's `trace/` folder | `traces/` | reserved (`paths::trace_dir`; a 1.1.0 `traces/` folder is renamed to it); O26 makes trace files there the event feed. Nothing writes it yet |

So, today: turning tracing on costs nothing and records nothing. Use Commit
records and messages instead ([storage metrics](storage-metrics.md#one-row-per-step)),
and report timing as not collected.

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
