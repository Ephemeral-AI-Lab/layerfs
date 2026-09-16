# layerfs-storage (C2)

> **Status:** Implemented slice; FULL and PREFIX payload records plus an explicit
> multi-lane format matrix. Metadata value pooling is not implemented.

Content-addressed physical storage. C2 accepts already-finalized canonical
objects, performs exact CAS reuse, writes supported FULL records into framed
packs, records locators in an embedded SQLite schema, and reads objects back
through an independent authenticated wave. It runs no C1 file construction and
needs no Workspace, branch, commit, mount or daemon.

## Accepted profile (this slice)

| Item | Accepted value |
| --- | --- |
| Format profile | `1` |
| Schema identity | `application_id = 1279677261`, `user_version = 3` (v1/v2 are rejected, not migrated) |
| Persisted policy | one row, `id = 1`: cutoff, both depths, the publication watermark |
| Accepted policy | cutoff a power of two in `131072..=1048576`; each depth `0..=50`; default 128 KiB / 8 / 4 |
| Encoding | **FULL and PREFIX** payload records. One PREFIX trial per object against at most one eligible candidate; the complete framed record cost decides |
| Pack framings | v1 ordinary groups, v2 native chunk records, v4 compact whole-file records, v6 pooled metadata groups, v7 singleton packs |
| Rejected framings | v3, v5 and unknown versions fail explicitly; no trial decoding |
| Pack size | `<= 256 KiB` for v1/v2/v4/v6, `<= 16 MiB + 4096` for the single-group v7 lane |
| Group body | `<= 64 KiB` (v1/v2/v4), `<= 16 KiB` (v6) |
| Records per group | `<= 8191`; `1` in the compact, pooled and singleton lanes |
| Groups per pack | `<= 256`; `1` in the singleton lane |
| Chain budgets | `512 KiB` canonical and `256 KiB` encoded per chain, independent of the accepted depths |
| Metadata pooling | **not implemented**: `metadata_value_groups` is shipped and empty |
| Lookup page | `128` identifiers |
| Pending batch | `512` objects and `512 KiB` canonical bytes |
| Write transaction | `8191` rows and `4 MiB - 1` canonical bytes, shared across batches |
| Stored canonical object | `<= 16 MiB` envelope ceiling, `<= 8 MiB` per field; the binding limits are the lane caps below (`65,527` B ordinary, `135,169` B whole-file) |

Object roles are persisted as `1..=6` (`WHOLE_FILE`, `CHUNK`, `EXTENT_LEAF`,
`EXTENT_BRANCH`, `FILE_STATE`, `INODE_LEAF`). `INODE_LEAF` is the declared
physical-pooling input role; nothing produces such an object yet, because the
pooling lane is not implemented. `base_object_id` records the direct physical base
of a PREFIX record and is `NULL` for a FULL record; it is constrained by a direct
self-FK and by the `objects_bases` index.

## Persistence profile

`PRAGMA journal_mode = MEMORY`, `PRAGMA synchronous = OFF`,
`PRAGMA temp_store = MEMORY`, `PRAGMA foreign_keys = ON`, and a zero busy
timeout. There is no WAL, no added crash durability and no
`fsync`/`fdatasync`/`sync_all`/`sync_data` anywhere in the product path. `COMMIT`
is required and remains the acknowledgement. A lost write lock fails immediately;
a lost `COMMIT` acknowledgement is an unknown persistence outcome and is never
resent, polled or deleted.

## Two deliberate deviations

**Eager writer authority.** Writer authority is acquired by a single eager
`BEGIN IMMEDIATE` attempt, not by a later lazy one. An operation that writes
nothing releases that acquisition with `ROLLBACK`, so no `COMMIT` is ever issued
for an empty write.

**Publication watermark.** The eager lock is not what keeps an unfinished save
invisible: bounded transactions release it between commits, so a save that
commits a pack early and then fails would otherwise leave that pack readable.
Visibility is therefore carried by `store_policy.retained_pack_ceiling`, the
highest pack identifier belonging to a *completed* save. It is advanced only
inside a save's final transaction, and ordinary reads clamp to it:

- `W <= MAX(pack_id)` must hold when the store is opened (`Integrity`); a
  persisted watermark ahead of storage is rejected rather than trusted.
- A save acquires only when `W == MAX(pack_id)`; otherwise it fails with
  `UninspectedState`, because it cannot tell another attempt's live output from
  its own baseline.
- A definite failure removes only packs newer than its baseline and never moves
  `W`, so early-committed packs of a failed save stay unreadable.
- A read of an object above the captured ceiling fails with
  `VisibilityCeiling`; it is never silently visible.

## Reading inside an open save

`SaveOperation::read_batch` sees every object `accept` acknowledged. An identity still
held in the bounded batch is served from memory. An identity waiting in an unfinished
group has no row yet, so the group holding it is framed and placed first, inside the
read's own `storage.read` scope, and the row is then read through the open transaction.
The read therefore performs the owner's own write; the alternative — keeping a second
copy of every waiting payload — was rejected because a group bounds only its *framed*
bytes, so a compressible record set could retain megabytes per group. The cost here is
packing granularity for the caller that reads inside a save, and it is paid once per
identity: afterwards the row exists and the membership lookup finds it. Unrelated
readers are unaffected and still see nothing above the publication watermark.

## Public surface

```text
Store::create / open / policy / capacities / begin_save / read_batch / contains
SaveOperation::accept / read_batch / finish / abort / pending / retained_tail_bytes
SaveHandoff               the C1 -> C2 finalized-object adapter
StorageError              includes UnknownOutcome, CleanupFailed and UninspectedState
```

## Failure semantics

One terminal boundary runs per failed save. A definite failure marks the save
terminal, aborts the open transaction and removes only rows and packs newer than
the baseline pack identifier captured under exclusive ownership. A failure whose
outcome is unproven quarantines the save instead. Cleanup runs once; a cleanup
failure retains both errors and is never retried by an outer wrapper or a
destructor.

## Scope limits

- Metadata value pooling, filesystem trees, runtime adapters and cloud placement
  are later scope. Payload DELTA chains up to the accepted depth are implemented.
- Only the four tables above exist; `metadata_value_groups` is shipped and empty,
  which is not a claim that value pooling is implemented.
- A base that this same save accepted but has not stored yet is not read for a
  delta trial: the object is stored FULL by policy rather than sealing a group for
  an optional optimisation.
- The admitted-FULL winner cache is a bounded min-hash sketch. It can miss a
  genuinely similar candidate; that is a bounded candidate loss, never a
  correctness risk, and the selected bytes always decide the outcome.
- A read captures `retained_pack_ceiling` once; an object stored beyond it fails
  the read rather than being silently visible. This bounds visibility, not
  durability: the persistence profile above still provides none.
- Evidence for memory is declared-limits and live-ownership accounting, not RSS.

## Checks

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --tests
```

The aggregate `tools/preflight.sh` gate is permanently retired (ledger L32) and runs
no checks; verify this workspace with the core manifest.

External targets: `cas_roundtrip`, `cas_reuse`, `pack_locator`,
`persistence_failure`, `memory_bounds`, `visibility`, `timing`, `core_pipeline`,
`delta_payload`, `delta_chains`, `policy_capacity`, `physical_formats`,
`edit_pipeline`; runnable examples: `examples/measure_components.rs` and
`examples/measure_edits.rs` (`--mode c1|c2|pipeline --case
small|chunked|small-to-large|large-to-small|batch --threshold-bytes N --output
FRESH_DIR`).
