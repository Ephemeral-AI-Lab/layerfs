# T9: PostgreSQL `store.compact` rebuild

> Implementation note for the frozen [`LayerFS-GC-spec.md`](LayerFS-GC-spec.md)
> §8 (PostgreSQL rebuild), §9 (failure and leftovers) and §11 (report). It adds
> no contract: the spec wins and `§N` cites it.

## Placement

`PostgresHandles::compact` is the Store-side primitive
(`engine/crates/layerfs-persistence/src/backend/postgresql/compact.rs`); the
`store.compact` control call dials the Store's recorded URL, opens its schema,
and runs it (`control/crates/layerfs-store/src/compact.rs`). Unlike SQLite the
session is kept; the caller reopens when it needs the new generation pinned
(the diagnostic guard rejects a stale incarnation by design, §10).

The mark reuses T7.2: `HistoryProvider::mark_gc` reads the §8 tuple as the
mark's first statement and then runs the shared §6.1/§6.2 closure in one write
transaction. The new SQLite-portable statement `gc_tuple_read` is registered
with the other history statements, so both engines share it.

## Run (PostgreSQL)

1. **Admission.** `settle_leftover` judges a recorded `gc_op` first (§9): `G`
   with a live `idle` owner is terminated with the two-argument
   `pg_terminate_backend(pid, backend_start)` and adopted; a live working owner
   is `Busy`; `G + 1` resumes Phase C. Then one Store-scoped `gc_intent`
   (`compaction`, exclusive) is registered: its session advisory key is held
   through Phase C, and a failure releases it.
2. **Phase M — mark.** The first statement reads `(catalog_incarnation,
   next_history_serial, next_pack_id, next_ordinal, store_totals.*,
   MAX(stamp), history rows)`; the T7.2 closure writes `kept_node` and returns
   the object/pack set and live-heads digest. The set is loaded into session
   temporary tables (`gc_visited`, `gc_live_pack`).
   `deadBytesBound` = stored bytes minus the retained packs' bytes.
   `gc_op` (phase `mark`, deterministic scratch name `lfs_gc_<op hex>`) and the
   persisted `gc_dead` membership (dead Commit/Layer/Branch/Stack ids,
   `content_signature` slots, `file_digest` roots) are written next; the
   `dead_logical` sum of dead history rows is recorded. A `none` decision
   truncates `gc_dead`, clears `gc_op`, releases the intent and returns.
3. **Phase A — build** (session advisory key, no table lock). `CREATE SCHEMA`
   `lfs_gc_<hex>`; `CREATE TABLE ... (LIKE schema.pack INCLUDING STORAGE
   INCLUDING COMPRESSION INCLUDING CONSTRAINTS)` plus the explicit primary key
   (the copy does not carry it), the same for `object_location`; owner,
   tablespace and reloptions are copied from `pg_class`. Retained rows are
   copied in pack-id batches, each its own transaction and idempotent by
   deleting its own range first; `object_location` uses an `EXISTS` semi-join.
   The prefix index, foreign key and `ANALYZE` follow; counts are checked
   against the mark.
4. **Phase B — swap** (one short transaction). Every Store base table is
   locked `ACCESS EXCLUSIVE NOWAIT`; `55P03` rolls back, drops the scratch
   schema in a new transaction and is `Busy`. The mark tuple and live-heads
   digest are re-read under the lock; any change is `Conflict` with no writes.
   Owner/ACL are applied, the two tables swapped, the `pack_immutable` trigger
   recreated, counts and the four totals checked, `dead_canonical/stored/
   metadata` persisted, `catalog_incarnation = G + 1`, the effective
   `synchronous_commit` (Store and server, never lowered) set, the standby
   predicate re-checked against `synchronous_standby_names` and `pg_stat_replication`,
   the scratch schema dropped, and `COMMIT`.
5. **Phase C — cleanup** (batched, after the swap). Only `gc_dead` members are
   deleted, child first; `binding_hint_block` is cleared; the acquisition
   working tables are emptied. `store_totals` is reduced once by the persisted
   `dead_*` totals in the settle transaction and checked against the retained
   row sums; `gc_dead`/`gc_op` are cleared and the intent released. A row
   created after the mark is not a member, so it survives.

Report (§11): `path`, `generation {before, after}`, `deadBytesBound` =
`deadBytesRefined`, `logicalDiscarded`, `backendReusable = 0`
(`postgresql-in-place`), `physicalShrink` from `pg_total_relation_size`,
`keptWaste`, `peakTempBytes` (scratch schema), empty `residue`, `estimate` and
the zero reasons; PostgreSQL reports 0 for rebuilt-table reusable space.

## Tests

`engine/crates/layerfs-persistence/tests/postgresql_compact.rs` (real
PostgreSQL; `LAYERFS_TEST_POSTGRES_URL`):

| Test | Covers |
|---|---|
| `an_empty_store_takes_the_none_path` | empty Store → `none`, `empty-store`, no artifact, cleared `gc_op`/`gc_dead`/intent |
| `a_live_store_takes_the_none_path` | live Store → `none`, bound = refined, no shrink |
| `a_wholly_dead_pack_is_rebuilt_away_and_totals_settle` | rebuild, dead pack collected, retained object kept, `G → G+1`, totals invariant, scratch dropped |
| `retained_roots_survive_a_rebuild_that_collects_a_deleted_project` | retained roots survive; a deleted project's history, markers and objects are collected; `logicalDiscarded > 0` |
| `a_lost_swap_reply_is_resumed_by_the_next_compact` | failpoint after the swap: the op stays at phase `swap`, `G+1`; the next compact finishes Phase C and reports the installed outcome |
| `a_stale_owner_backend_is_terminated_before_the_next_round` | a failpoint before Phase A leaves an idle owner; the next session terminates it with `pg_terminate_backend(pid, backend_start)` and adopts the op |
| `the_same_session_resumes_an_uncertain_round` | a failpoint before Phase A leaves the caller's own idle session as the recorded owner; the retry adopts its own op instead of terminating itself |
| `an_exclusive_table_lock_makes_the_swap_busy` | a conflicting `ACCESS EXCLUSIVE` lock at the swap is `Busy` (55P03), and the scratch schema is dropped |

Unit tests in `compact.rs` pin the standby predicate (`rank < remote_write` or
no names → no wait; names → a streaming sync/quorum standby is required) and
the `synchronous_commit` strength order.

Result at T9: `cargo test -p layerfs-persistence --features postgresql --test
postgresql_compact` — 8 passed on PostgreSQL 17.12 (Debian, server on
`127.0.0.1:55432`); `--lib` 4 passed; the SQLite suite
(`--test sqlite_compact`) passed (17 when T9 branched, 20 on the rebased main with the T12 fixtures); `sqlite_compact_crash` passed 13.

## Gaps (not contract changes)

- **AC-41/AC-42/AC-43/AC-44/AC-60 remain `open`** in the acceptance matrix:
  the tests are E1/E2 evidence, not the full #52 matrix. Not covered here:
  a reserved-pack publish racing Phase A/C, a mid-cleanup crash failpoint, a
  real synchronous standby, and a truly lost `COMMIT` reply from a killed
  backend. The swap-phase failpoint proves the resume path, not the server's
  reply loss.
- **ACL replay** is best-effort: the owner and a default table ACL are copied;
  non-default column/function ACLs are outside this note.
- **Busy holder detail** is the code only (`{entity, phase, op}` and PG
  `pid/client_addr/backend_start` are not populated yet); the runbook's lock
  query is the manual path.
- The Phase A batch size is a fixed 512 packs, not a 1 GB byte budget; the
  scratch copy is `INSERT ... SELECT`, so no pack bytes cross the client.
- `physicalShrink`/`peakTempBytes` are sampled, and a resumed cleanup reports
  them as 0 (the op does not persist them).
- `store.status` reading the Store-side `gc_op` is T10/T11; this note only
  writes the row/phases it needs.
