# Multi-writer reconciliation: the owner's uncommitted iteration onto main

Dated 2026-09-20. Append-only receipt for the reconciliation that landed the
pair-3 continuation, the multi-writer storage model and the bridge optimization
together on `main`.

## Why a merge, not a copy

`main` (`7b8a7d9d3`) carried later storage work (#188d/W2, #190, #194-#199:
`SaveProfile` instrumentation, `prepare_cached` catalogue reuse, the candidate
index flush, `advance_retained_pack_ceiling`) that the worktree's dirty
`layerfs-storage` never saw. Copying the dirty files over main did not even
compile: `error[E0432]: unresolved import crate::cas::owner::SaveProfile`, plus
`MutationOwner` missing `profile`/`probe`. The dirty set was an *older* iteration
plus new multi-writer semantics, so it was reconciled with a real three-way merge
against the shared base `9f35c49ad` (branch tip `fc647e91a` as the local side).

The merge produced exactly **5 conflicts**, all storage, all resolved by
composition rather than by picking a side:

| file | local (multi-writer) | main (later work) | resolution |
|---|---|---|---|
| `cas/lifecycle.rs` | `ownership` import, `advance_pack` on commit, publication flow | counter wrapper + `finish_inner`, capacity-gated commits, W2 index flush, `SaveProfile` | both: wrapper + `finish_inner`, `advance_pack` kept, publication transaction carries the W2 flush |
| `cas/placement.rs` | arbitration lock + `begin_write` | `SaveProfile::charge` timing | both |
| `cas/pool_lane.rs` | arbitration lock + `begin_write` | `SaveProfile::charge` timing | both |
| `encoding/delta/select.rs` | arbitration lock on `acquire` | `SaveProfile::charge` timing | both |
| `sqlite/pool.rs` | publication-scoped `group_for` query | `prepare_cached` | main's `prepare_cached` carrying local's scope filter |

## Defects found and fixed while reconciling

1. **`cas/collision.rs` predated main's reader API.** It called
   `Resolver::new(..., &mut packs, ...)`; main's signature takes
   `BodyCaches { packs, pool }`. Adapted with a wave-local `PoolReader`.
2. **Nested transactions.** Local's `begin_write()` calls at step boundaries
   assumed its own commit-every-step model. Guarded with
   `if !self.transaction_open` in `flush_candidates`, `placement` and `pool_lane`.
3. **The W>1 invariant, which is the real multi-writer fix.** SQLite admits one
   writer per store file. Main's capacity-gated commit keeps a write transaction
   open across the whole save, so a second concurrent writer's `INSERT` failed
   with `OwnershipUnavailable` (mapped from the SQLite busy error at
   `sqlite/connection.rs:141`) — `owned_load.rs` proved it: the second native
   writer never reached its announce point. `maybe_commit` now commits at every
   locked step, so a write transaction never outlives the arbitration lock.
   Batching stays inside a step; main's cross-step capacity gate cannot survive
   W>1 and its `SaveProfile` commit instrumentation is retained.
4. **`group_for` preparation count.** The publication-scoped query needs its
   `MAX(first_ordinal)` subquery: without it an unpublished covering group would
   silently fall back to an older visible one instead of raising `Unpublished`.
   One preparation of that shape issues two `Select` authorizer actions, so
   `catalogue_statement_reuse.rs` now measures reuse across calls
   (`PER_PREPARATION = 2`) instead of assuming one event per preparation.
5. **External readers need the read scope.** `metadata_pool.rs` opened a raw
   `rusqlite::Connection`; the scoped query needs `temp.layerfs_read_scope`,
   which the supported opener `sqlite::connection::open` initializes to
   "everything published". That test's reader now uses the supported opener.

## Verification (reconciled tree)

- `env -u RUSTFLAGS cargo +1.85.1 test --locked --workspace` — **535 passed, 0
  failed, 95 test binaries**, including `two_writers_overlap_and_capacity_is_reclaimed_after_reverse_completion`
  (service) and `two_native_writers_keep_streaming_heap_bounded_as_input_grows`.
- `cargo fmt --all -- --check` — clean.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — clean. This
  also fixes the long-standing `layerfs-daemon/examples/transport_probe.rs`
  `format!`-in-iterator lint (behaviour unchanged: same hex output).
- `python3 core/tools/check_product_boundary.py` — PASS, 175 production
  Rust/SQL files.

## Production LOC

`Production LOC: 24745 -> 25394 (delta +649)` for `core`; reference unchanged at
65417; combined 90162 -> 90811. First parent `7b8a7d9d3` vs the committed tree,
method `tools/production_loc.py`. The delta carries the pair-3 continuation, the
multi-writer storage model and the bridge optimization, net of what main already
had.

## Still open (not closed by this commit)

- D-series decisions: numeric W/C profile (the `capacities.transaction_rows/bytes`
  gate is no longer a commit trigger under W>1), verified-caller construction,
  deadline ownership, bounded-shutdown semantics.
- The registered two-stream rate discrepancy (1.460/0.764 vs 2.051/1.541 GB/s).
- T01-T16, ENV01/02/05/06 and O01-O11 still have zero receipts.
- The bridge optimization's own numbers are unchanged and re-measured on the
  reconciled tree only to the extent the workspace suite covers it; the container
  throughput figures (upload 0.2598 -> 1.043 GB/s) come from the v9 registered
  runs recorded in `registered-selection-v4.md`.
