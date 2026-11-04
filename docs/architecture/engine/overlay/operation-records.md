# Operation records

[Architecture](../../README.md) › [Engine](../README.md) › [Overlay](README.md) › Operation records

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Indexed operation records.

| | |
| --- | --- |
| Component | [Overlay](README.md) |
| Source | [`engine/crates/layerfs-overlay/src/contract/indexed_operation_record.rs`](../../../../engine/crates/layerfs-overlay/src/contract/indexed_operation_record.rs), [`engine/crates/layerfs-overlay/src/lifetime/indexed_operation_record.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/indexed_operation_record.rs), [`engine/crates/layerfs-overlay/src/maintenance/indexed_operation_record.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/indexed_operation_record.rs), [`engine/crates/layerfs-overlay/src/diagnostics`](../../../../engine/crates/layerfs-overlay/src/diagnostics) |

Operation records are raw key/value rows that a long operation (for
example a Commit's construction of one file) keeps in the Workspace's own
file instead of in memory. The overlay gives them no meaning; Content and
Workspace own the meaning of every value
([`ports/operation_record.rs`](../../../../engine/crates/layerfs-workspace/src/ports/operation_record.rs)).

## Indexed operation records

```text
commit_record row
+-----------+------------+--------+----------+-------------------+
| operation | file_scope | kind   | key      | value             |
| owner id  | 8 bytes BE | u32    | 32 bytes | 0..65536 bytes    |
+-----------+------------+--------+----------+-------------------+
 PRIMARY KEY(operation, file_scope, kind, key)    WITHOUT ROWID

acquire_operation(request) --> OperationOwner (memory, never reused)
  get / get_many / contains / keys / entries   (fenced reads)
  apply(scope, [change...])                    (one atomic job)
release_operation(owner)   --> enqueue OPERATION_RECORD(owner)
  maintenance deletes the rows, one envelope per step
```

| Type or call | Rule | Source |
| --- | --- | --- |
| `IndexedOperationRecordScope` | `OperationOwner` plus an opaque `file_scope`; all identities keep full width | [`contract/indexed_operation_record.rs`](../../../../engine/crates/layerfs-overlay/src/contract/indexed_operation_record.rs) |
| `IndexedOperationRecordKey` | `kind: u32` then a 32-byte `key`; kind orders first | [`contract/indexed_operation_record.rs`](../../../../engine/crates/layerfs-overlay/src/contract/indexed_operation_record.rs) |
| `indexed_operation_record_apply` | Checks every `OperationRecordExpectedValue` (`Missing` or `ExactBytes`) before changing any row; a mismatch returns `NotApplied { index, key, actual }` and changes nothing; `value: None` deletes | [`lifetime/indexed_operation_record.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/indexed_operation_record.rs) |
| Batch envelope | Changes must be strictly ordered and unique; their descriptors and values together stay within `OPERATION_RECORD_BYTES` (65 536) | [`contract/indexed_operation_record.rs`](../../../../engine/crates/layerfs-overlay/src/contract/indexed_operation_record.rs) |
| Windows | `get_many` at most `PAGE_ROWS` (64) keys; key and entry pages at most 64 rows; an entry window stops before it would pass one envelope but always returns its first row | [`lifetime/indexed_operation_record.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/indexed_operation_record.rs), [`statements.rs`](../../../../engine/crates/layerfs-overlay/src/database/statements.rs) |
| `first_keys` | First eligible keys excluding one retained root; no continuation cursor, so keys inserted below consumed ones are found again | [`lifetime/indexed_operation_record.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/indexed_operation_record.rs) |
| Fence | A stale route first, then a closed Workspace when the job writes, then a released or foreign owner; existing custody stays readable after logical close | [`lifetime/operation.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/operation.rs) |
| Cleanup | After release, each `OPERATION_RECORD` step deletes the records of one operation up to one envelope in one statement | [`maintenance/indexed_operation_record.rs`](../../../../engine/crates/layerfs-overlay/src/maintenance/indexed_operation_record.rs) |

The windows bound one job; they are not a total operation or edit limit.

## Plans

`explain_indexed_operation_record(scope, key, excluded_root)` returns
`EXPLAIN QUERY PLAN` text for each statement shape and the VM programs of
the write statements. It is an operator diagnostic, not a parsed contract.

| Label | Statement |
| --- | --- |
| `contains`, `get`, `delete` | Full-key point statements |
| `keys`, `all-keys`, `keys-after` | Key windows, with and without the excluded root |
| `entries`, `entries-after` | Key/value windows |
| `operation-cleanup`, `operation-cleanup-delete` | The two cleanup prefixes |
| `put-vm`, `keys-vm`, `all-keys-vm`, `keys-after-vm`, `delete-vm` | VM programs (`EXPLAIN`) |

Source: [`diagnostics/indexed_operation_record_plan.rs`](../../../../engine/crates/layerfs-overlay/src/diagnostics/indexed_operation_record_plan.rs).
The example [`indexed_operation_record_profile.rs`](../../../../engine/crates/layerfs-overlay/examples/indexed_operation_record_profile.rs)
prints these plans with scoped SQLite work for a fresh database; its own
header says it is no timing proof.

## Diagnostics

| Item | What it counts | Source |
| --- | --- | --- |
| `StatementKind` | Twelve fixed families; aggregation keeps no input-sized trace | [`metrics.rs`](../../../../engine/crates/layerfs-overlay/src/diagnostics/metrics.rs) |
| `StatementWork` | Attempts, executions, rows returned and changed, returned BLOB and value bytes, VM steps, full-scan steps, sorts, auto-index rows, re-prepares, bound and SQL bytes, one-use statement memory samples, elapsed wall | [`metrics.rs`](../../../../engine/crates/layerfs-overlay/src/diagnostics/metrics.rs) |
| `DatabaseWork` | One `StatementWork` per family; `since` and `accumulate` for per-job deltas | [`metrics.rs`](../../../../engine/crates/layerfs-overlay/src/diagnostics/metrics.rs) |
| `PayloadWork` | Write input bytes, whole and partial write cells, cell copy and zeroed bytes, read-window zeroed and local copy bytes | [`diagnostics/payload.rs`](../../../../engine/crates/layerfs-overlay/src/diagnostics/payload.rs) |
| Startup batches | Schema and profile batches counted under `Startup` | [`diagnostics/startup.rs`](../../../../engine/crates/layerfs-overlay/src/diagnostics/startup.rs) |

Elapsed nanoseconds are diagnostic wall time, not exclusive CPU or a
latency gate; statement memory samples are not pager or process residency
([`metrics.rs`](../../../../engine/crates/layerfs-overlay/src/diagnostics/metrics.rs)).

## Related

- [Database](database.md): the `commit_record` table and the job transaction.
- [Lifetimes and reclamation](lifetimes-and-reclamation.md): the `OPERATION_RECORD` cleanup kind.
- [Workspace: Ports](../workspace/ports.md): the adapter that gives records meaning.
- [Commit: Construction](../commit/construction.md): the main user.
- [Glossary](../../glossary.md).

---

← [Overlay: Lifetimes and reclamation](lifetimes-and-reclamation.md) · [Up: Overlay](README.md) · [Engine: Workspace](../workspace/README.md) →
