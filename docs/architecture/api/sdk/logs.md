# logs family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `logs` calls: read and clear the home's call log, the one line every other call writes when it ends, and read a machine's worker log and a Sandbox's daemon log by cursor, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/logs`](../../../../api/crates/layerfs-api/src/logs), [`layerfs-home/src/logs.rs`](../../../../control/crates/layerfs-home/src/logs.rs), [`layerfs-home/src/tail.rs`](../../../../control/crates/layerfs-home/src/tail.rs), [`layerfs-sandbox/src/inspect`](../../../../control/crates/layerfs-sandbox/src/inspect), [`calls/render.rs`](../../../../api/crates/layerfs-api/src/calls/render.rs) |
| Runs by default | `list`, `reset`: the caller's process, against two files of the home. `machine`, `sandbox`: where the log file is, the caller's process for a `native` Sandbox and the machine's worker otherwise. No call here starts a machine or writes a call-log line |
| Design | [Home](../../control/home/README.md) (the call log's layout and locking), [Sandboxes: inspection](../../control/sandboxes/inspect.md) (the route of `machine` and `sandbox`) |

## Calls

```text
 every other call ---- one line when it ends ----> <home>/trace/calls.jsonl
                                                    (calls.1.jsonl past 16 MiB)
 list  (filters, newest first, paged) <---------- reads both files
 reset (truncate calls.jsonl, remove calls.1.jsonl) -> { kind: "Reset" }

 machine (cursor | before, limit) <--- <worker root>/log/worker.log, worker.1.log
 sandbox (cursor | before, limit) <--- <run folder>/daemon.log
          read where the file is: here (native) or in the machine's worker
```

| Call | Returns |
| --- | --- |
| [`logs.list`](#logslist) | `{ lines, next?, has_more, skipped }` |
| [`logs.reset`](#logsreset) | `{ kind: "Reset", removed }` |
| [`logs.machine`](#logsmachine) | `{ lines: [{ text, continued? }], cursor, before?, has_more }` |
| [`logs.sandbox`](#logssandbox) | the same shape |

A line is `{ v, at, ms, client, argv, outcome, kind?, commit?, layer?, tag? }`:
`at` is when the call started (ms since the epoch), `ms` its duration,
`client` `cli`, `node`, `python` or `rust`, `argv` the CLI words of the same
call, `outcome` `ok` or the refusal code, and for a call that saved a version
its result `kind` and `commit` or `layer` id. How `argv` is rendered and what
is redacted is in [calls](../calls/README.md#the-call-log). Past 16 MiB
(`ROTATE_AT`) `calls.jsonl` rotates once to `calls.1.jsonl` under a
non-blocking lock on `trace/calls.lock`
([`logs.rs`](../../../../control/crates/layerfs-home/src/logs.rs)). A folder
scope refuses every `logs` call.

### logs.list

The calls made against this home, newest first, from `trace/calls.jsonl`
then `trace/calls.1.jsonl`.

```text
JS      await layerfs.logs.list({ op, project, client, tag, outcome, since, cursor, limit })
Python  layerfs.logs.list(*, op=None, project=None, client=None, tag=None, outcome=None, since=None, cursor=None, limit=None)
CLI     layerfs logs list [--op <op>] [--project <project>] [--client <client>] [--tag <tag>] [--outcome <outcome>] [--since <since>] [--cursor <cursor>] [--limit <limit>]
```

Every option given must match.

| Option | Kind | Rule |
| --- | --- | --- |
| `op` | Text | `family` or `family.call` (`file`, `file.write`; `-` and `_` are the same) |
| `project` | Project | lines whose `argv` names this project |
| `client` | Name | `cli`, `node`, `python` or `rust` |
| `tag` | Text | an exact tag, or a prefix ending in `:` |
| `outcome` | Text | `ok`, or a refusal code |
| `since` | Int | lines whose `at` is at or after this time (ms) |
| `cursor` | Text | `next` from a previous page |
| `limit` | Int | 1 to 1000 (`MAX_LIMIT`); default 100 (`DEFAULT_LIMIT`) |

`lines` are in completion order, newest first; `next` is present when
`has_more`; `skipped` counts lines that do not parse (a torn tail after a
crash), which are never answered. Errors: `InvalidInput` (a bad option,
`limit` out of range, a malformed cursor, or a stale one: "the cursor is stale
(the call log was rotated or reset); list from the start"); `Failed` (a log
file cannot be read).

### logs.reset

Clears the call log: truncates `trace/calls.jsonl` in place and removes
`trace/calls.1.jsonl`.

```text
JS      await layerfs.logs.reset()
Python  layerfs.logs.reset()
CLI     layerfs logs reset
```

Returns `{ kind: "Reset", removed }`, the lines both files held, torn ones
included. It holds `trace/calls.lock`, waiting for it, so a line another
process writes meanwhile lands before or after the reset and is never torn. A
`logs.list` cursor from before the reset is stale afterwards. Error: `Failed`
(the lock or a file cannot be opened, truncated or removed).

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
