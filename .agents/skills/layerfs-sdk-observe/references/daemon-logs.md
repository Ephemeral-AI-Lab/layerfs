# Daemon logs and observation records

> **Status:** Current general guide for the LayerFS 2.0 call surface. Part of
> the `layerfs-sdk-observe` skill. `sandbox.log`, `sandbox.files` and
> `machine.log` outputs were printed by the built `layerfs` command on a
> throwaway home on 2026-11-18 (macOS, a `native` Sandbox). What the daemon
> writes is read from source
> (`control/crates/layerfs-sandbox-backend/src/backend/native/`), not run.
> 1.x `logs.sandbox` and `logs.machine` are now `sandbox.log` and
> `machine.log`, with no alias.

## What the daemon writes

Each Sandbox runs one LayerFS daemon. It writes to its stdout and stderr,
kept as `daemon.log` in the Sandbox's run folder:

- **Startup:** its listen line (seen: `LAYERFS_DAEMON_LISTEN 127.0.0.1:62804`).
- **Failures:** a fatal startup cause, or a control-connection failure.
- **Observation records** (`"layerfs_observation":1`): fixed-schema numeric
  records of SQL work, owner lock waits and holds, Store reader and cache work,
  FUSE kernel counters (Linux) and Workspace resources. The daemon writes them
  only for **traced** control exchanges. No front end traces a call's daemon
  exchanges (see [tracing](tracing.md)), so expect none.

There is no general log level or logging framework.

## Reading it

`sandbox.log(sandbox, { limit, cursor })` reads `daemon.log`: `items`
(`[{ text, continued? }]`, oldest first; `continued` marks a piece of a
longer line), the last `limit` lines (default 200, 1 to 1000), or with
`cursor` the lines before it, at most 64 KiB a call. `next_cursor` continues
with older lines and is absent at the start of the log. Control characters
are escaped (`\u{1b}`). It takes no lock, asks no daemon, starts nothing and
writes nothing, the call log included.

```bash
$ layerfs sandbox log tests --limit 2
{"items":[{"text":"LAYERFS_DAEMON_LISTEN 127.0.0.1:62804"}]}
```

```js
// the whole log, oldest first
import layerfs from "@layerfs/node";
const pages = [];
let cursor;
do {
  const page = await layerfs.sandbox.log("tests", { limit: 1000, cursor });
  pages.unshift(page.items);          // each page is older than the one before
  cursor = page.next_cursor;
} while (cursor);
const lines = pages.flat().map((l) => l.text);
```

`sandbox.files(sandbox)` shows where it is (the file itself is refused there;
read it with `sandbox.log`). A command's output over 1 MiB is kept beside it
as `exec/<workspace id>/<exec id>.stdout` (and `.stderr`), which
`sandbox.files` reads and `workspace.unmount` deletes.

```bash
$ layerfs sandbox files tests
{"items":[{"path":"/tmp/demo/home/machine/native/run/layerfs-8a7387b2-tests/workspaces/1","type":"dir","not_read":"mount"},{"path":"/tmp/demo/home/machine/native/run/layerfs-8a7387b2-tests/tmp","type":"dir"},{"path":"/tmp/demo/home/machine/native/run/layerfs-8a7387b2-tests/daemon.log","type":"file","size":38}]}
```

**`sandbox.remove` deletes the run folder with the log.** Save what you need
first. When a Sandbox fails to start, `sandbox.create`'s refusal message
carries the cause.

## A machine's worker log

`machine.log(machine, { limit, cursor })` reads a machine's worker log the
same way (newest page first, `next_cursor` for the lines before it). A
cursor from before the log rotated is stale (`InvalidInput`). `native` has
no worker:

```bash
$ layerfs machine log native --limit 1
{"error":"Unsupported","message":"machine native has no worker (its calls run in the caller's process), so it has no worker log"}
```

## Decoding old logs

`tools/decode_observations.py` (in this checkout) still decodes 1.1.0 daemon
logs joined to a 1.1.0 trace:

```bash
python3 tools/decode_observations.py --trace run/trace.jsonl < run/daemon-tests.log > run/daemon-tests.jsonl
```

Each output line is one daemon call: `daemon`, `scope`, `call`, `operation`,
`complete`, the joined trace line's `id` as `trace`, and named `sections`.
Unavailable sections are `null`, never zero. Values are **cumulative** daemon
counters: subtract the previous call's values to get per-call work. On a 2.0
log with no observation records it has nothing to decode.
