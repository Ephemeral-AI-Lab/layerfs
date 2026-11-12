# logs family

[Architecture](../../../README.md) › [API](../../README.md) › [SDK](../README.md) › logs

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `logs` calls: the home's call log (#659).

| | |
| --- | --- |
| Component | [Call table](../../call-table/README.md) |
| Source | [`api/crates/layerfs-api/src/logs`](../../../../../api/crates/layerfs-api/src/logs), [`logs.rs`](../../../../../control/crates/layerfs-home/src/logs.rs) |

## Calls

```text
 every other call ---- one line when it ends ----> <home>/trace/calls.jsonl
                                                    (calls.1.jsonl past 16 MiB)
 list  (filters, newest first, paged) <---------- reads both files
 reset (truncate calls.jsonl, remove calls.1.jsonl) -> { kind: "Reset", removed }
```

Two calls, in help order
([`logs/mod.rs`](../../../../../api/crates/layerfs-api/src/logs/mod.rs)).
Both run in the caller's process against the home, never start a machine
and write no line themselves. The line format, rotation and what is
redacted are described in
[Home and records: the call log](../../../control/management/home-and-records.md#the-call-log).

| Call | Arguments | Options | Returns |
| --- | --- | --- | --- |
| [`list`](list.md) | none | `op`, `project`, `client`, `tag`, `outcome`, `since`, `cursor`, `limit` | `{ lines, next?, has_more, skipped }` |
| [`reset`](reset.md) | none | none | `{ kind: "Reset", removed }` |

---

← [backup](../backup/README.md) · [Up: SDK](../README.md)
