# logs.reset

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › [logs](README.md) › reset

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Clears this home's call log: truncates `trace/calls.jsonl` in place and
removes `trace/calls.1.jsonl`.

| | |
| --- | --- |
| Family | [logs](README.md) |
| Library entry | [`api/crates/layerfs-api/src/logs/reset.rs`](../../../../../api/crates/layerfs-api/src/logs/reset.rs) |
| Implemented in | `layerfs_home::logs::reset` ([`logs.rs`](../../../../../control/crates/layerfs-home/src/logs.rs)) |
| Runs in | the caller's process, holding `trace/calls.lock` (waiting for it); starts no machine; writes no line |

## Signature

```js
// JavaScript
await layerfs.logs.reset();
```

```python
# Python
layerfs.logs.reset()
```

```bash
# CLI
layerfs logs reset
```

## Arguments

None.

## Options

None.

## Returns

```json
{"kind":"Reset","removed":1342}
```

`removed` is the number of lines both files held before the reset, torn
ones included. A line another process writes meanwhile lands before or
after the reset; no line is torn by it. A `logs.list` cursor from before
the reset is stale afterwards.

## Errors

| Code | When | Uncertain? |
| --- | --- | --- |
| `Failed` | the lock or a log file cannot be opened, truncated or removed | no |

## Related

- [logs.list](list.md); [Home and records: the call log](../../../control/management/home-and-records.md#the-call-log).

---

← [logs.list](list.md) · [Up: logs](README.md)
