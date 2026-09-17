# W5 evidence — bounded owners (G9 input, G10)

## What the packet changes

| Item | Before | After |
| --- | --- | --- |
| W5.1 cold-start collector | `index.rs::retained_start` called `pool::catalogue(connection, None)`, which materialised **every** catalogue row into a `Vec`, on a cold Store with more ordinals than the window. | `sqlite/pool.rs` now exposes `for_each_group`, which streams the statement one row at a time (the same shape the reference uses at `objects/metadata.rs:252-281`); `retained_start` keeps three scalars and walks the catalogue through it. The point query shares the same row decoder. |
| W5.2 per-object clone | `cas/save.rs:50` cloned the whole `FinalizedObject` (up to 8 MiB of canonical bytes plus predecessors) for every wave member. | `MutationOwner::offer` takes `&FinalizedObject`; the wave keeps ownership and no canonical copy is made. |
| W5.3 placement clone | `pack/placement.rs:84-86` cloned the open pack's groups and the incoming group to measure a fit, once per placed group, against `GROUP_COUNT_LIMIT`/`PACK_LIMIT`. | `layout::append_fits` computes `assembled_length(open) + directory_entry_len(lane) + body_size(incoming)` against `lane.pack_limit()`, and `lane.group_count_limit()`; no candidate vector exists. The dead `layout::fits` predicate is deleted and `directory_entry_len` is shared with `assembled_length`. |
| W5.4 builder level growth | `flush_streaming` flushed only the level it was called with, and its only call site was level 0: level 1 and above accumulated entries until `finish`. | The same flush rule now cascades: every level over the bound flushes, lowest first, until none is. `MappingBuild` reports `peak_pending`. |
| W5.5 pack cache | `cas/owner.rs`'s dependency pack cache and `encoding/pool/read.rs`'s pooled pack cache had no byte bound and no release event; the `select.rs` comment called the first one "inside this wave". | Both are bounded by `DEPENDENCY_PACK_CACHE_BYTES` (4 MiB) and released wholesale when the next body would cross it — the discipline the pooled value cache and the index window already use; the comment now states the bound and the release. |
| W5.6 cleanup transaction | `sqlite/cleanup.rs` ran one transaction for up to ~128 M row deletes under a MEMORY journal. | The single cleanup attempt now commits and reopens when the writer's `TRANSACTION_ROW_LIMIT`/`TRANSACTION_CANONICAL_BYTES_LIMIT` would be crossed, keeping the newest-locator-first order, the one-attempt rule and the page budget. |
| W5.7 dead budgets and counters | `METADATA_INDEX_BYTES` (32 MiB) had no reader; `Candidates::live_bytes` reported the size of a fat pointer; `delta/read.rs` reset the shared `ChainCounters` per resolution, so `SaveOutcome.chain` was the last chain's. | The dead constant is gone and the real bound is stated where it is reported; `live_bytes` returns the declared slot and reference bytes; the owner keeps `chain_total` and reports it, while the per-chain value stays what the budget check compares against. `ReadCounters::packs_read` is now counted where a body is fetched, because the cache can release itself wholesale and a length difference would underflow. |
| W5.8 pooled row loop | `pool/read.rs::leaf_canonical` issued one SQL point query per pooled row (up to 100 per leaf) and cloned the whole cached group per row. | The covering catalogue row is cached across the ordinal-ordered rows, and `PoolReader::group_value` copies one 73-byte value out of the cached group. |

## Owner ledger (the G9/W7.3 input)

| Owner | Bound | Live multiplicity | Lifetime | Release |
| --- | --- | --- | --- | --- |
| COW edit frontier `EditObjects::drafts` | `EDIT_DEFERRED_LIMIT` = 8 MiB − 1 charged (canonical page bytes or decoded entries) | one per edit operation | the operation | a superseded draft is released (`release`), the replaced range is released (`discard`), the rest is dropped with the operation |
| `ExtentBuilder` levels | `(height + 2) × stream_flush_entries` retained entries | one builder per construction/edit scan | the scan | dropped at `finish`; measured as `MappingBuild::peak_pending` |
| Dependency pack cache (`cas/owner.rs`) | `DEPENDENCY_PACK_CACHE_BYTES` = 4 MiB of pack bodies | one per save, one copy per distinct pack | the save | wholesale clear when the next body crosses the bound; dropped with the operation |
| Pooled pack cache (`encoding/pool/read.rs`) | same 4 MiB bound | one per wave/chain | the wave | wholesale clear; dropped with the reader |
| Pooled decoded-value cache | `VALUE_CACHE_BYTES` = 512 KiB | one per reader | the wave | wholesale clear, documented as surviving `begin_chain` |
| Pool index window | `METADATA_INDEX_VALUES` = 131 072 entries × (key + ordinal) | one per Store, shared across saves | the Store | whole-window reset, or `invalidate()` on a failed save |
| Admitted-FULL candidate index | `INDEX_BYTES` = 128 KiB fixed | one per save | the save | dropped with the operation |
| Dependency depth cache | `DEPTH_CACHE_ENTRIES` = 4 096 entries | one per save | the save | cleared wholesale when full |
| SQLite MEMORY journal / BLOBs | one bounded transaction per write path (writer `maybe_commit`, cleanup now bounded) | one journal per open transaction | the transaction | `COMMIT`/`ROLLBACK` |
| Telemetry report | 1 024 nodes / 32 levels | one per timing scope | the scope | with the report |

## Commands, exits and raw output

`w5-verify.log` (22 focused targets + the workspace suite + clippy/fmt/boundary/tools,
every command exit 0):

* storage: `metadata_pool` 13, `metadata_pool_index` 5, `metadata_chain` 2,
  `metadata_window` 2 (8.5 s), `delta_chains` 9, `delta_payload` 13,
  `pack_locator` 8, `physical_formats` 5, `policy_capacity` 8,
  `persistence_failure` 7, `cas_reuse` 8, `cas_roundtrip` 6, `visibility` 7,
  `edit_pipeline` 5, `core_pipeline` 5, `memory_bounds` 7.
* content: `edit_bounds` 9 (17.2 s), `streaming` 9, `edit_reference` 2 (56.5 s,
  nine sealed cases), `object_identity` 10, `file_complete` 14, `file_read` 8.
* `cargo +1.85.1 test --workspace --locked --no-fail-fast` — **43 targets,
  261 tests, 0 failed**, 98.1 s.
* `clippy --all-targets -D warnings`, `fmt --all --check`, the product-boundary
  check, both tool suites and `git diff --check` — exit 0.
* production LOC pair — core `11053 -> 11166` (delta +113); C1 4500 → 4508,
  C2 5821 → 5926, telemetry unchanged.

## New oracles

* `pack_locator.rs::the_placement_fit_probe_agrees_with_the_canonical_assembled_length`
  — compares `append_fits` with `assembled_length(groups + [group]) <= pack_limit`
  at every step across the boundary, for the Ordinary, WholeFile and Native lanes,
  and checks the lane's group-count limit still binds.
* `streaming.rs::the_builder_retains_a_bounded_number_of_entries` — builds 1 MiB,
  8 MiB and 24 MiB streams, requires `MappingBuild::peak_pending` inside
  `(height + 2) × flush threshold` and requires it not to grow with the file
  (measured `[58, 194, 201]` entries for `[58, 432, 1293]` chunks).
* `delta_chains.rs::the_save_reports_every_chain_it_acquired_not_only_the_last`
  — two objects against two distinct bases in one save must report two objects and
  the sum of both envelopes; a single-base save reports one.

## Control runs (`w5-fails-without-fix.log`)

* The reported save total reverted to the per-chain counter (the reviewed
  semantics): the new chain case fails with
  `the save reports both acquired bases, not the last one: ChainCounters { objects: 1, ... }`.

## What this artifact does not prove

* W5.1, W5.2, W5.5, W5.6 and W5.8 are **source-derived**: no oracle measures the
  transient they remove. W7's instrumentation is where a phase-local ledger turns
  those claims into numbers; until then they are structural.
* W5.4's cascade only changes `peak_pending` once level one itself exceeds the
  flush threshold (192 level-zero flushes, ≈590 MB of chunked input), which no
  case inside the command budget reaches; the bound asserted is the structural one
  and the cascade is verified by reading the loop. The gap is stated in the test.
* The pack-cache bound is 4 MiB per operation; a dependency chain longer than the
  bound re-reads a pack rather than retaining it. That trade is stated, not
  measured.
