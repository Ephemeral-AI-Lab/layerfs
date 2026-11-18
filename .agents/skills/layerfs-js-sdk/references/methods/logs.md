# logs calls

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-js-sdk` skill. An agent working on a project rarely needs this
> family; it belongs to whoever watches a run. A folder scope refuses every
> call on this page.

```js
home.log({ call, project, client, tag, outcome, since, limit, cursor, detail })
home.clear_log()
logs.machine(machine, { cursor, before, limit })
logs.sandbox(sandbox, { cursor, before, limit })
```

`home.log` reads the home's call log (`trace/calls.jsonl`, then
`trace/calls.1.jsonl`); `logs.machine` and `logs.sandbox` read a machine's
worker log and a Sandbox's `daemon.log`. The two call-log calls were
`logs.list` and `logs.reset` before the call surface 2.0; those names are
gone, with no alias.

## `home.log`

`home.log({ call, project, client, tag, outcome, since, limit, cursor, detail })`:
the calls made against this home, newest first. Every option given must
match: `call` is `family` or `family.call`; `project` matches the lines'
`argv`; `client` is `cli`, `node`, `python` or `rust`; `tag` is an exact tag
or a prefix ending in `:`; `outcome` is a row's `outcome` (`ok`, `refused`, a
result's word such as `committed`) or its `error` (a public refusal code);
`since` keeps calls that started at or after a time (ms). `limit` is 1 to
1000 (default 200), and `cursor` is a previous answer's `next_cursor`.

Returns `{ items, next_cursor? }`. A row is `{ call, outcome, logged_at }`,
with `project` and `error` when it has them; `detail: true` gives every key
(`duration_ms`, `client`, `argv`, `commit`, `tag` too, nulls included) and
`skipped`, the torn lines that do not parse, which are never answered.
`next_cursor` carries the filters that made it: a later call may leave them
out, a filter given again must be the same. Errors: `InvalidInput` (a bad
option, `limit` out of range, a malformed or stale cursor, a cursor given
with other filters or made by another call), `Failed` (a log file cannot be
read).

## `home.clear_log`

`home.clear_log()`: clears the call log, truncating `trace/calls.jsonl` in
place and removing `trace/calls.1.jsonl`. Returns `{ ok: true }`; with
`detail: true`, `{ ok, outcome: "cleared", line_count }`, the lines both
files held. A cursor from before the clear is stale afterwards. Error:
`Failed`.

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
