# logs family

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `logs` calls: read and clear the home's call log, the one line every other call writes when it ends, within the [SDK](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-api/src/logs`](../../../../api/crates/layerfs-api/src/logs), [`layerfs-home/src/logs.rs`](../../../../control/crates/layerfs-home/src/logs.rs), [`calls/render.rs`](../../../../api/crates/layerfs-api/src/calls/render.rs) |
| Runs by default | the caller's process, against two files of the home; neither call starts a machine or writes a line |
| Design | [Home](../../control/home/README.md) (the call log's layout and locking) |

## Calls

```text
 every other call ---- one line when it ends ----> <home>/trace/calls.jsonl
                                                    (calls.1.jsonl past 16 MiB)
 list  (filters, newest first, paged) <---------- reads both files
 reset (truncate calls.jsonl, remove calls.1.jsonl) -> { kind: "Reset" }
```

| Call | Returns |
| --- | --- |
| [`logs.list`](#logslist) | `{ lines, next?, has_more, skipped }` |
| [`logs.reset`](#logsreset) | `{ kind: "Reset", removed }` |

A line is `{ v, at, ms, client, argv, outcome, kind?, commit?, layer?, tag? }`:
`at` is when the call started (ms since the epoch), `ms` its duration,
`client` `cli`, `node`, `python` or `rust`, `argv` the CLI words of the same
call, `outcome` `ok` or the refusal code, and for a call that saved a version
its result `kind` and `commit` or `layer` id. How `argv` is rendered and what
is redacted is in [calls](../calls/README.md#the-call-log). Past 16 MiB
(`ROTATE_AT`) `calls.jsonl` rotates once to `calls.1.jsonl` under a
non-blocking lock on `trace/calls.lock`
([`logs.rs`](../../../../control/crates/layerfs-home/src/logs.rs)). A folder
scope refuses both calls.

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

## Related

- [Calls: the call log](../calls/README.md#the-call-log).
- [Home](../../control/home/README.md).
