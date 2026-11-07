# Daemon logs and observation records

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-sdk-observe` skill. **Not run in this pass** (no Sandbox was
> started). Read from `control/crates/layerfs-sandbox-backend/src/backend/native/`,
> `api/crates/layerfs-api/src/family/sandboxes.rs`. 1.1.0's `backend.daemon.sandbox.get_log` has no 2.0
> call.

## What the daemon writes

Each Sandbox runs one LayerFS daemon. It writes to its stdout and stderr:

- **Failures:** a fatal startup cause, or a control-connection failure.
- **Observation records** (`"layerfs_observation":1`): fixed-schema numeric
  records of SQL work, owner lock waits and holds, Store reader and cache work,
  FUSE kernel counters (Linux) and Workspace resources. The daemon writes them
  only for **traced** control exchanges. In this build no front end traces a
  call (see [tracing](tracing.md)), so expect none.

There is no general log level or logging framework.

## Where the log is

| Sandbox on | Log |
| --- | --- |
| `native` (macOS host processes, Linux namespaces) | The file `daemon.log` (stdout and stderr together, mode 0600) in the Sandbox's private folder under `$LAYERFS_HOME/machine/native/run/`. The folder is named `lfs-<home id, 8 digits>-<Sandbox name>`, the `container` field of `workspace.mount`'s result |
| a VM or ssh machine | Inside that machine, under its worker's root; no call reads it from the host |
| `docker` | Sandboxes on `docker` are `Unsupported` in this build |

Copy the file before `sandbox.close`, which deletes its folder with the log.
A Sandbox no longer closes itself when its last Workspace goes (#535 D11).

```bash
# from source; check the folder name against workspace.mount's `container`
ls "$LAYERFS_HOME/machine/native/run/"
cp "$LAYERFS_HOME/machine/native/run/<container>/daemon.log" run/daemon-<container>.log
```

When a Sandbox fails to start, `sandbox.create`'s `Failed` message carries
the cause, including the end of its daemon log when it could be read.

## Decoding old logs

`tools/decode_observations.py` (in this checkout) still decodes 1.1.0 daemon
logs joined to a 1.1.0 trace:

```bash
python3 tools/decode_observations.py --trace run/trace.jsonl < run/daemon-<container>.log > run/daemon-<container>.jsonl
```

Each output line is one daemon call: `daemon`, `scope`, `call`, `operation`,
`complete`, the joined trace line's `id` as `trace`, and named `sections`.
Unavailable sections are `null`, never zero. Values are **cumulative** daemon
counters: subtract the previous call's values to get per-call work. On a 2.0
log with no observation records it has nothing to decode.
