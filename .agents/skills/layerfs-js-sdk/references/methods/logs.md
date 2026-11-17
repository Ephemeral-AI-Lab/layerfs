# logs calls

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-js-sdk` skill. An agent working on a project rarely needs this
> family; it belongs to whoever watches a run. A folder scope refuses every
> `logs` call.

```js
logs.list({ op, project, client, tag, outcome, since, cursor, limit })
logs.reset()
logs.machine(machine, { cursor, before, limit })
logs.sandbox(sandbox, { cursor, before, limit })
```

`logs.list` reads the home's call log (`trace/calls.jsonl`, then
`trace/calls.1.jsonl`); `logs.machine` and `logs.sandbox` read a machine's
worker log and a Sandbox's `daemon.log`. A call-log line is
`{ v, at, ms, client, argv, outcome, kind?, commit?, layer?, tag? }`, where
`at` is when the call started (ms since the epoch) and `ms` its duration.

## `logs.list`

`logs.list({ op, project, client, tag, outcome, since, cursor, limit })`: the
calls made against this home, newest first. Every option given must match:
`op` is `family` or `family.call`; `project` matches the lines' `argv`;
`client` is `cli`, `node`, `python` or `rust`; `tag` is an exact tag or a
prefix ending in `:`; `outcome` is `ok` or a refusal code; `since` keeps lines
at or after a time (ms). `limit` is 1 to 1000 (default 100), and `cursor` is a
previous answer's `next`.

Returns `{ lines, next?, has_more, skipped }`; `skipped` counts torn lines
that do not parse, which are never answered. Errors: `InvalidInput` (a bad
option, `limit` out of range, a malformed or stale cursor), `Failed` (a log
file cannot be read).

## `logs.reset`

`logs.reset()`: clears the call log, truncating `trace/calls.jsonl` in place
and removing `trace/calls.1.jsonl`. Returns `{ kind: "Reset", removed }`, the
lines both files held. A cursor from before the reset is stale afterwards.
Error: `Failed`.

## `logs.machine`

`logs.machine(machine, { cursor, before, limit })`: one window of a machine's
`log/worker.log`, with `worker.1.log`. `limit` is 1 to 1000 (default 200);
`cursor` reads the lines after another answer's `cursor`, `before` the lines
before another answer's `before` (not with `cursor`).

Returns `{ lines, cursor, before?, has_more }`: `lines` are oldest first, each
`{ text }`, or `{ text, continued: true }` for a piece of a longer line.
`cursor` is always text; pass it back to read what was written since. One call
reads at most 64 KiB, so an answer may hold fewer than `limit` lines with
`has_more` true. Errors: `InvalidInput` (both options, `limit` out of range, a
malformed or stale cursor), `Unsupported` (`native` has no worker),
`NotFound`, `Unavailable`, `Busy`, `Failed`.

## `logs.sandbox`

`logs.sandbox(sandbox, { cursor, before, limit })`: the same, over a
Sandbox's `daemon.log` in its run folder. Options, result and rules are
`logs.machine`'s. Errors add `NoSuchSandbox` and `Unsupported` (a retired
docker Sandbox record).
