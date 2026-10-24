# Daemon logs and observation records

> **Status:** Current general guide. Part of the `layerfs-sdk-observe` skill. The records,
> traced SDK calls requesting them and the decoder (#333) are on `main`.

## What the daemon writes

Each Sandbox daemon writes to its stderr, which Docker keeps as the container log. The
container is `<prefix><project>-<n>` (`layerfs-<project>-<n>` by default), and its
identifier is `get_log`'s `container` (also in the plain-text file
`$LAYERFS_HOME/<project>/sandboxes/<n>/container`).

- **Failures:** a fatal startup cause, or a control-connection failure.
- **Observation records (tracing on):** for every traced control exchange,
  fixed-schema numeric records marked `"layerfs_observation":1`:
  - SQL work (statements, steps);
  - owner lock wait and hold times per job family;
  - Store reader and cache work;
  - FUSE kernel counters (Linux);
  - Workspace resources.

There is no general log level or logging framework.

## Saving and decoding

Save each Sandbox's log through the backend; it reads Docker where the SDK's binding
runs, so it also works on macOS and Windows:

```js
import { writeFileSync } from "node:fs";
const log = await backend.daemon.sandbox.get_log(ws.sandbox, { lines: 2000 });   // { sandbox, container, text, bytes }
writeFileSync(`${run}/daemon-${log.container}.log`, log.text);
```

`get_log` keeps the last 2000 lines and at most 64 KiB. For a whole log of a long run,
where your process can reach Docker, `docker logs <container> >
<run>/daemon-<container>.log 2>&1` still works. Then decode:

```bash
# from the LayerFS checkout
python3 tools/decode_observations.py --trace <run>/trace.jsonl < <run>/daemon-<container>.log > <run>/daemon-<container>.jsonl
```

Each output line is one daemon call: `daemon`, `scope`, `call`, `operation`,
`complete`, the joined trace line's `id` as `trace`, and named `sections`. Unavailable
sections are `null`, never zero. Values are **cumulative** daemon counters at that call:
subtract the previous call's values to get per-call work.

Save the logs while the Sandbox runs. A Sandbox closes itself two minutes after its
last Workspace is unmounted (its daemon writes `LAYERFS_DAEMON_IDLE_EXIT` last), and the
next mount or the runtime's `sandbox.reclaim` deletes the stopped container and its log.

## When a diagnostic write fails

If the daemon can't write its records, or the telemetry doesn't fit the reply, the trace
line's `daemon` entry carries `diagnosticError: { kind, message }`. With SDK tracing the
call's result and its connection are unaffected.
