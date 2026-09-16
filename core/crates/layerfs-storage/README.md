# layerfs-storage (C2)

> **Status:** Implemented slice; one explicit FULL-only physical profile.

Content-addressed physical storage. C2 accepts already-finalized canonical
objects, performs exact CAS reuse, writes supported FULL records into framed
packs, records locators in an embedded SQLite schema, and reads objects back
through an independent authenticated wave. It runs no C1 file construction and
needs no Workspace, branch, commit, mount or daemon.

## Accepted profile (this slice)

| Item | Accepted value |
| --- | --- |
| Format profile | `1` |
| Schema identity | `application_id = 1279677261`, `user_version = 1` |
| Persisted policy | one row, `id = 1`, the frozen 128 KiB / 8 / 4 values |
| Encoding | **FULL only**. DELTA, value pooling and pooled metadata are not implemented |
| Pack framings | v1 ordinary groups, v2 native chunk records, v4 compact whole-file records |
| Pack size | `<= 256 KiB` for every framing |
| Group body | `<= 64 KiB`; the compact lane holds one record per group |
| Records per group | `<= 8191` (`1` in the compact lane) |
| Groups per pack | `<= 256` |
| Lookup page | `128` identifiers |
| Pending batch | `512` objects and `512 KiB` canonical bytes |
| Write transaction | `8191` rows and `4 MiB - 1` canonical bytes, shared across batches |
| Stored canonical object | `<= 16 MiB` envelope ceiling, `<= 8 MiB` per field; the binding limits are the lane caps below (`65,527` B ordinary, `135,169` B whole-file) |

Object roles are persisted as `1..=5` (`WHOLE_FILE`, `CHUNK`, `EXTENT_LEAF`,
`EXTENT_BRANCH`, `FILE_STATE`). `base_object_id` exists, is constrained by a
direct self-FK and is always `NULL` in this slice because no DELTA is produced.

## Persistence profile

`PRAGMA journal_mode = MEMORY`, `PRAGMA synchronous = OFF`,
`PRAGMA temp_store = MEMORY`, `PRAGMA foreign_keys = ON`, and a zero busy
timeout. There is no WAL, no added crash durability and no
`fsync`/`fdatasync`/`sync_all`/`sync_data` anywhere in the product path. `COMMIT`
is required and remains the acknowledgement. A lost write lock fails immediately;
a lost `COMMIT` acknowledgement is an unknown persistence outcome and is never
resent, polled or deleted.

## One deliberate deviation

Writer authority is acquired by a single eager `BEGIN IMMEDIATE` attempt, not by
a later lazy one. Holding the write lock for the whole save is what keeps
unfinished-save output invisible to unrelated readers. An operation that writes
nothing releases that acquisition with `ROLLBACK`, so no `COMMIT` is ever issued
for an empty write.

## Public surface

```text
Store::create / open / policy / capacities / begin_save / read_batch / contains
SaveOperation::accept / read_batch / finish / abort / pending / retained_tail_bytes
SaveHandoff               the C1 -> C2 finalized-object adapter
StorageError              includes UnknownOutcome and CleanupFailed
```

## Failure semantics

One terminal boundary runs per failed save. A definite failure marks the save
terminal, aborts the open transaction and removes only rows and packs newer than
the baseline pack identifier captured under exclusive ownership. A failure whose
outcome is unproven quarantines the save instead. Cleanup runs once; a cleanup
failure retains both errors and is never retried by an outer wrapper or a
destructor.

## Scope limits

- DELTA chains, arbitrary edits, metadata pooling, filesystem trees, runtime
  adapters and cloud placement are later scope.
- Only the four tables above exist; `metadata_value_groups` is shipped and empty,
  which is not a claim that value pooling is implemented.
- A read captures its retained-pack ceiling once; an object stored beyond that
  ceiling fails the read rather than being silently visible.
- Evidence for memory is declared-limits and live-ownership accounting, not RSS.

## Checks

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --tests
sh core/../tools/preflight.sh   # from the repository root
```

External targets: `cas_roundtrip`, `cas_reuse`, `pack_locator`,
`persistence_failure`, `memory_bounds`, `timing`, `core_pipeline`; runnable
example: `examples/measure_components.rs`.
