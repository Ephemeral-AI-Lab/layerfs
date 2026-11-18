# logs family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `logs` calls: read a machine's worker log and a Sandbox's daemon log by cursor, within the [SDK](README.md). The home's own call log, the one line every other call writes when it ends, is read and cleared by [`home.log` and `home.clear_log`](home.md#homelog) since the call surface 2.0 (they were `logs.list` and `logs.reset`; the old names are gone).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/logs`](../../../../api/crates/layerfs-api/src/logs), [`layerfs-home/src/logs.rs`](../../../../control/crates/layerfs-home/src/logs.rs), [`layerfs-home/src/tail.rs`](../../../../control/crates/layerfs-home/src/tail.rs), [`layerfs-sandbox/src/inspect`](../../../../control/crates/layerfs-sandbox/src/inspect), [`calls/render.rs`](../../../../api/crates/layerfs-api/src/calls/render.rs) |
| Runs by default | `machine`, `sandbox`: where the log file is, the caller's process for a `native` Sandbox and the machine's worker otherwise. No call here starts a machine or writes a call-log line |
| Design | [Home](../../control/home/README.md) (the call log's layout and locking), [Sandboxes: inspection](../../control/sandboxes/inspect.md) (the route of `machine` and `sandbox`) |

## Calls

```text
 machine (cursor | before, limit) <--- <worker root>/log/worker.log, worker.1.log
 sandbox (cursor | before, limit) <--- <run folder>/daemon.log
          read where the file is: here (native) or in the machine's worker
```

| Call | Returns |
| --- | --- |
| [`logs.machine`](#logsmachine) | `{ lines: [{ text, continued? }], cursor, before?, has_more }` |
| [`logs.sandbox`](#logssandbox) | the same shape |

A folder scope refuses every `logs` call.

### logs.machine

One window of a machine's worker log: `log/worker.log` under the worker's
root, with `worker.1.log`, the one file it rotates to when the worker starts
([Workers and reach](../../control/machines/workers-and-reach.md#the-workers-log)).

```text
JS      await layerfs.logs.machine(machine, { cursor, before, limit })
Python  layerfs.logs.machine(machine, *, cursor=None, before=None, limit=None)
CLI     layerfs logs machine <machine> [--cursor <cursor>] [--before <before>] [--limit <limit>]
```

| Option | Kind | Rule |
| --- | --- | --- |
| `cursor` | Text | an answer's `cursor`: the lines written after it |
| `before` | Text | an answer's `before`: the lines before it; not with `cursor` |
| `limit` | Int | 1 to 1000 (`MAX_LINES`); default 200 (`DEFAULT_LINES`) |

```text
 no option   the last `limit` lines         before: present when earlier lines exist
 cursor      the lines after it, oldest     has_more: more are there already; read again
             first, at most `limit`
 before      the `limit` lines before it    before: present when still earlier lines exist
```

Returns `{ lines, cursor, before?, has_more }`. `lines` are oldest first;
each is `{ text }`, or `{ text, continued: true }` for a piece of a line
longer than one read, whose next piece follows (in the same answer or the
next). `cursor` is always text: pass it back to read what was written since.
`has_more` says the direction asked for has more lines now. One call reads at
most 64 KiB (`CHUNK`) of the file, so an answer may hold fewer than `limit`
lines with `has_more` true. A last line the writer has not ended with a
newline is held back until it is. A log that does not exist yet (a machine
started by a build that wrote none) answers `lines: []`, and its cursor reads
the file from its start once it is there.

`text` is for display: every control character is written as Rust's escape
(`\t`, `\r`, `\u{1b}`), bytes that are not UTF-8 as U+FFFD, and a backslash
is left as it is. The call reads these two files and no other; in particular
never `log/uncertain.jsonl`.

It takes no lock or lease, checks no operation in progress on the machine,
starts nothing and writes nothing, on either side, so a client may poll it.
At most two inspect requests run on a machine at a time; the caller waits at
most 2 s to connect, 2 s for the hello and 8 s for the answer.

| Error | When |
| --- | --- |
| `InvalidInput` | both `cursor` and `before`; `limit` out of range; a cursor that is not one, or is stale: "the cursor is stale (the log was rotated or replaced); read again without a cursor" |
| `Unsupported` | the machine's calls run in the caller's process (`native`), so it has no worker: `detail.reason` is `no_worker` |
| `NotFound` | no machine has this name |
| `Unavailable` | the machine is stopped, its worker is not reached, or it does not answer in time; the message names `layerfs machine start` where that helps. The request changed nothing, so this is never `Unknown` |
| `Busy` | two inspect requests are running on the machine already; read again |
| `Failed` | the file cannot be read, or the request panicked in the worker |

### logs.sandbox

One window of the log of a Sandbox's daemon: `daemon.log` in the Sandbox's
run folder on its machine, which the backend makes when the daemon starts,
never rotates and removes with the folder when the Sandbox closes.

```text
JS      await layerfs.logs.sandbox(sandbox, { cursor, before, limit })
Python  layerfs.logs.sandbox(sandbox, *, cursor=None, before=None, limit=None)
CLI     layerfs logs sandbox <sandbox> [--cursor <cursor>] [--before <before>] [--limit <limit>]
```

Options, result and the rules above are [`logs.machine`](#logsmachine)'s. The
file is read where the run folder is: in the caller's process for a Sandbox
on a `native` machine, in the machine's worker for every other, a Sandbox
whose daemon the home reaches directly (`lima`) included. The daemon is not
asked, so the log of a daemon that has ended stays readable while its run
folder is there.

| Error | When |
| --- | --- |
| `InvalidInput` | as `logs.machine`; or the Sandbox's record names a run folder outside its machine's run root |
| `NoSuchSandbox` | no Sandbox has this name, or its machine holds no record or run folder for it |
| `Unsupported` | a retired docker Sandbox record |
| `Unavailable`, `Busy`, `Failed` | as `logs.machine` |

Both calls are for a UI that shows a log and follows it. The MCP server lists
neither in any profile ([MCP](../front-ends/mcp.md#profiles)).

## Related

- [Calls: the call log](../calls/README.md#the-call-log).
- [Home](../../control/home/README.md).
- [Sandboxes: inspection](../../control/sandboxes/inspect.md).
