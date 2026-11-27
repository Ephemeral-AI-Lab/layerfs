# Review 4: `layerfs-persistence` outside `src/backend/`

Scope: `engine/crates/layerfs-persistence/` `src/` root files, `diagnostic/` (with `check/`), `gc/`
(`admission/`, `mark/`, `spill/`), `history/` (`counters/`, `graph/`, `index/`, `lineage/`, `query/`),
`storage/` (`blocks/`, `metadata/`), `store/`, `upgrade/`, the `sql/` tree, `tests/` and `examples/`.
Evidence is from the archive of `layerfs_v2_0_0` at 1317ebf7ca (`S/v2tree/...`); production LOC from
`S/prod_loc_files.txt` (same commit, `tools/production_loc.py --files`). The plan is `S/nav/site/layout.json`.

Where a fix lands in `src/backend/` (another reviewer's scope) it is marked **cross-scope** and is
proposed only because a file in my scope is the evidence.

## 1. Verdict

**layerfs-persistence (root, sql, tests, examples): mostly sound.** The row of root folders
(`backend/`, `diagnostic/`, `gc/`, `history/`, `storage/`, `store/`, `upgrade/`) reads as drivers,
shared semantics and Store-wide work, and the counts and depth limits are met everywhere. The
neutral/backend split is clean for `upgrade/`, `diagnostic/`, `storage/metadata/` and
`storage/objects.rs`, but three things in the plan still put engine-specific code in neutral
folders, or neutral code in an engine, and one planned name does not compile:

- `history/query/` collides with `layerfs_history::query`. `history/mod.rs` imports that module
  under the same name, and the graph files use it.
- The shared compaction types live in the SQLite backend. PostgreSQL's compaction and the neutral
  `gc/metrics.rs` import them from `crate::backend::sqlite::compact`.
- `storage/provider.rs` is SQLite's provider. PostgreSQL's provider is in its backend.

The `sql/` tree keeps the GC statements under `history/` after the source moved them to `gc/`.
Several test topics sit in the wrong area or repeat their folder's name.

Answers to the focus questions:

- **Neutral vs backend split (rule 23): mostly clean.** `diagnostic/`, `upgrade/` and
  `storage/metadata/` use the same word as the backend folders for the neutral half of the same job.
  Exceptions are findings 2, 3, 5 and 7.
- **`store/`: keep the name.** The engine uses `store/` for the same job (opening and installing a
  Store) in `layerfs-project` (`store/init.rs`, `install.rs`, `live.rs`) and `layerfs-store-service`
  (`store/open.rs`, `install.rs`, `bootstrap.rs`). Renaming only the persistence folder would break
  rule 19. The `inspect/` → `server/` precedent renamed a folder that took another crate's name;
  here "Store" is the domain word for what is opened. Its inside needs one fix (finding 5).
- **`gc/admission/`: no collision.** Content `admission/` became `verify/` so that "admission"
  means GC admission. Core `layerfs-history` `admission/` (intent, authority, holder) and
  store-service `history/gc_admission.rs` use the same meaning.
  - The one second meaning is inside this crate's backend: `backend/admission.rs` is the FIFO
    connection-turn queue (AGENTS.md calls it "the in-process admission queue"), and
    `backend/sqlite/lane/writer_admission.rs` is the cross-process flock.
  - That is the backend reviewer's call. A file name such as `backend/admission_queue.rs` would keep
    the AGENTS.md term and separate the two meanings. I did not put it in the change list.
- **`storage/metadata/` is not thin.** It has 5 files plus `mod.rs`, and rule 4 means 1–2 files. Each
  file is the shared half of one Metadata port group (allocation, locations, policy, signatures,
  values). `backend/sqlite/metadata/` uses the same five file names. That mirror is the reason for
  the files, so they stay. Folding them into one `storage/metadata.rs` (114 LOC) would reopen a
  settled point for no clear gain.

## 2. Findings, ranked

### 1. `history/query/` collides with `layerfs_history::query` (rules 13, 12)

- **What.** The plan creates `history/query/{bindings,rows,transaction}.rs`.
- **Evidence.**
  - `history/mod.rs` line 42 is `use layerfs_history::query;`.
  - `history/branch.rs:20`, `commit.rs:19` and `layerstack.rs:26` (all moving to `graph/`) each do
    `use super::query::{self, Context, Cursor, Range};`. That `query` is core's paged-query contract
    (`core/crates/layerfs-history/src/contract/query.rs`: cursors, capacity, `page`, `encode`).
  - A `mod query;` in `history/mod.rs` conflicts with that import, so it does not compile as planned
    without renaming the import. It also gives "query" two meanings inside `history/`.
- **What the files are.** Each file's header says what it holds:
  - `bindings.rs`: "Owned scalar bindings for history statements".
  - `rows.rs`: "Typed history record decoding from shared backend columns".
  - `transaction.rs`: "Shared history transaction facade".

  This is the statement plumbing: parameters in, rows out, and the transaction that runs them.
- **Fix.** Rename `history/query/` → `history/statement/`, with the files unchanged. "Statement" is
  already the word for this job: `HistoryStatements` in the seam, `history/statements.rs` on
  PostgreSQL, and `acquisition/statements.rs` on SQLite.

### 2. The shared compaction types live in the SQLite backend (rules 23, 9). Cross-scope.

- **Evidence.**
  - PostgreSQL's compaction imports its §11 types from SQLite:
    - `backend/postgresql/compact/run.rs:22-27`: `use crate::backend::sqlite::compact::{CompactFailpoint, CompactKeptWaste, CompactPath, CompactReceipt, CompactRequest, CompactResidue}`.
    - The same pattern is in `op.rs:13-15`, `cleanup.rs:17-19`, `report.rs` (→ `receipt.rs`) `:3-7`
      and `types.rs:51`.
  - The neutral `compact_metrics.rs` (→ `gc/metrics.rs`) has
    `use crate::backend::sqlite::compact::{CompactKeptWaste, CompactPath, CompactReceipt};`.
  - The types are defined in `backend/sqlite/compact/types.rs` (210 LOC). Its header is "§7/§11
    compaction types, sidecar constants and the error classification", so it mixes:
    - SQLite-only items: the `.compact*` sidecar suffixes, `MarkTuple`, `db_error` over rusqlite,
      `seal_error`;
    - shared types: `CompactRequest`, `CompactPath`, `CompactFailpoint`, `OwnerLiveness`,
      `CompactKeptWaste`, `CompactResidue`, `CompactZeroReason`, `CompactReceipt`,
      `DELTA_BASE_REFERENCES_CAP`, `cap_delta_references`.
  - `report.rs` (→ planned `gc/phases.rs`, 12 LOC) already says the receipt is one type that "both
    backends return".
- **Fix.**
  - Split `backend/sqlite/compact/types.rs`. The shared types go to a new `gc/compact.rs`, and the
    SQLite-only items stay in `types.rs`.
  - Merge `gc/phases.rs` (`CompactStatus`, `SQLITE_COMPACT_PHASES`, `POSTGRES_COMPACT_PHASES`) into
    `gc/compact.rs`. The result is one file for the compaction call's neutral vocabulary: request,
    receipt, status and phase lists.
  - The backends' `compact/` folders keep the engine work. That is rule 23's "same word, neutral
    half / engine half".
- **Public paths.** None change; `lib.rs` re-exports these at the root.
- **Reach.** The PostgreSQL backend then no longer imports from the SQLite backend.

### 3. `storage/provider.rs` is SQLite's provider (rules 23, 9). Cross-scope.

- **Evidence.**
  - `storage/provider.rs` defines `StorageProvider { session: Arc<Session> }`. `backend/mod.rs` has
    `pub(crate) use sqlite::session::connection::Session;`.
  - Its methods call SQLite code directly: `backend::sqlite::storage::merge_records::Scratch::Sqlite`,
    `backend::sqlite::objects::selection::read`, `objects::unit_layout::cost` and
    `objects::unit_layout::validate_batch`, and `backend::sqlite::lifecycle::headroom::admit`. Its
    snapshot is `VACUUM INTO`.
  - PostgreSQL's counterpart, `PostgresStorage`, is planned at `backend/postgresql/objects/provider.rs`.
- **Fix.** Move `storage/provider.rs` → `backend/sqlite/objects/provider.rs`.
  - Both backends then have `objects/provider.rs`.
  - `storage/` keeps only neutral code: `objects.rs`, `publication.rs` (`validate` is used by both
    providers and `history/fused.rs`), `blocks/` and `metadata/`.
  - SQLite `objects/` goes from 7 to 8 files. The depth is `src/backend/sqlite/objects/provider.rs`,
    which is the limit.
  - The module is private and `lib.rs` re-exports `StorageProvider`, so no public path changes.
- **Not changed.** The plan's settled point that `storage/` holds `blocks/` and `metadata/` still
  holds.

### 4. `diagnostic/`: the whole-Store check sits outside `check/`, and infrastructure sits inside it (rules 9, 10, 11)

- **Evidence.**
  - `scan.rs` runs the whole-Store audit. Its `run()` calls `catalog`, `acquisition`, `history` and
    `auxiliary`, plus its own `packs`, `chains`, `objects` and `blocks` checks, which emit
    `store.packs`, `store.objects` and `store.namespace`. Both backend audits call it
    (`diagnostic/sqlite.rs:165`, `diagnostic/postgresql.rs:330`).
  - `project.rs` (in `check/`) says: "The checks are the whole-Store scan's ([`super::scan`]), on a
    smaller" scope. So the two are siblings: the project audit and the Store audit. The plan
    separates them, with `project.rs` in `check/` and `scan.rs` loose.
  - `scratch.rs` ("Private SQLite temporary backing, outside the diagnosed Store") is audit
    infrastructure, like `reader.rs`, `observation.rs` and `transaction.rs`. The plan put it in
    `check/` only because of the 4-loose-file cap.
  - `transaction.rs` defines only the `View` trait ("Borrowed database mechanics"). The trait is
    implemented by both engines' transactions and imported everywhere as `transaction::View`.
    The name `transaction.rs` also belongs to `backend/*/session/transaction.rs` and the history
    transaction facade.
- **Fix.**
  - `diagnostic/scan.rs` → `diagnostic/check/store.rs`, named after the scope it checks and the
    `store.*` names it emits, beside `check/project.rs`.
  - `diagnostic/check/scratch.rs` → `diagnostic/scratch.rs`.
  - `diagnostic/transaction.rs` → `diagnostic/view.rs`.
- **Result.** The root holds the audit framework (`observation`, `reader`, `scratch`, `view`: 4
  loose files), and `check/` holds the 9 things checked.

### 5. `store/`: SQLite's handles are spread over three files with generic names (rules 9, 11)

- **Evidence.**
  - `store/open.rs` is `impl SqliteHandles { create, open_writable, open_read_only,
    open_diagnostic, diagnostic_identity, open, validated }`, so it is SQLite only.
  - `store/seal.rs` is `SealedStore` plus `impl SqliteHandles { seal }`, also SQLite only.
  - `store/handles.rs` (→ `sqlite.rs`) is the `SqliteHandles` struct.
  - `store/postgresql.rs` holds all of `PostgresHandles`: struct, create and open.
  - The neutral open is `Handles::open_writable` and `Handles::open_read_only` in `selection.rs`.
- **Problem.** A reader who opens `store/open.rs` to see "how a Store opens" finds only SQLite.
  The plan's reason for `handles.rs` → `sqlite.rs`, "pairs with store/postgresql.rs", is true only
  once SQLite's open and seal join it.
- **Fix.** Merge `store/open.rs` and `store/seal.rs` into `store/sqlite.rs` (38 + 144 + 24 = 206 LOC,
  against PostgreSQL's 192).
- **Result.** `store/` is `config.rs`, `selection.rs` (neutral), `sqlite.rs` and `postgresql.rs`
  (the two variants): a 4-file leaf.

### 6. `sql/`: GC statements stay under `history/` after the source moved to `gc/` (rules 19, 23, 8)

- **Evidence.** Key → user, from `grep` of the statement keys:
  - `gc_intent_*` and `gc_epoch_lock` → `history/admission.rs` (→ `gc/admission/`).
  - `gc_epoch_read` and `gc_epoch_share` → `history/epoch.rs` (→ `gc/epoch.rs`).
  - `gc_inode_blocks`, `gc_live_*` and `gc_tuple_read` → `history/objects.rs` (→ `gc/mark/objects.rs`).
  - `kept_node_*` and `history_remove_unreachable_*` → `history/kept.rs` (→ `gc/mark/history.rs`).
  - `gc_pack_lengths` → `backend/sqlite/compact/run.rs`.
- **Problem.** The plan puts all of these in `sql/sqlite/history/{gc,gc_intent,gc_intent_scope,kept_node,unreachable_removal}/`,
  and PostgreSQL's in `sql/postgresql/history/gc/`. That leaves `sql/sqlite/history/` with 16
  subfolders, and the SQL no longer follows its code.
- **Fix.** Add `sql/sqlite/gc/`:
  - `epoch/` (`lock`, `read`, `share`, plus `bump` and `insert` from finding 11);
  - `intent/`;
  - `intent_scope/`;
  - `kept_node/`;
  - `mark/` (`inode_blocks`, `live_branches`, `live_stacks`, `pack_lengths`, `tuple_read`);
  - `unreachable_removal/`.

  Also `sql/postgresql/history/gc/` → `sql/postgresql/gc/`, a flat leaf of 10 files.
- **Cost.** All these files already move in the plan (`queries/history/*` → new paths); only the
  destination changes. The `include_str!` paths in `backend/sqlite/session/query.rs` and
  `backend/postgresql/history/statements.rs` change either way. The `history(key)` loader names stay.
- **Result.** `sql/sqlite/history/` goes from 16 to 10 subfolders after finding 11.
- **Limits.** Depth stays at `sql/sqlite/gc/intent/x.sql`, the same as planned.
  `history/statistics/unreachable_count.sql` stays put: it is a count, and moving it would leave
  `statistics/` with 2 files.

### 7. `gc/epoch.rs` → `gc/fence.rs` (rules 13, 11)

- **Evidence.**
  - `gc/epoch.rs` (from `history/epoch.rs`) holds `pub(crate) struct Fence` and `open`/`observed`;
    its header is "The `project_epoch` writer fence".
  - `backend/epoch.rs` holds `pub(crate) struct Epoch`, "A Store session's acquisition epoch", which
    is a different concept.
  - The crate also has `claim_epoch` and `owner_epoch` (acquisition).
- **Problem.** Two `epoch.rs` files in one crate with unrelated meanings.
- **Fix.** Rename `gc/epoch.rs` → `gc/fence.rs`, named after its type and its job, and the test
  `tests/gc/epoch_fence.rs` → `tests/gc/fence.rs`.

### 8. Tests: topics in the wrong area, and names that repeat the folder (rules 7, 14, 9)

**Moves:**

- `tests/diagnostic/diagnostics_during_lane.rs` → `tests/sqlite_session/counters_during_lane.rs`.
  It reads `handles.storage.diagnostics()` (the `SqlWork` counters; lines 45 and 98) and has no
  `diagnostic::` or `Observation` use. Its header: "Reading a session's SQL counters does not wait
  for the publication lane". That is the topic of `thread_work.rs`, not of a Store check.
- `tests/store/diagnostic.rs` (`store_diagnostic.rs`) → `tests/diagnostic/store.rs`. It is
  `Observation`-based Store diagnosis ("Integrity diagnosis against real Normal databases") and
  mirrors `check/store.rs`.
- `tests/session/connection_step.rs` → `tests/postgresql_diagnostic/connection_step.rs`. It tests
  `layerfs_persistence::diagnostic::ConnectionStep`, the PostgreSQL doctor step whose source moves to
  `backend/postgresql/diagnostic/step.rs`.
- `tests/session/` → `tests/sqlite_session/`. The remaining 7 topics are all SQLite:
  `background_checkpoint`, `idle_checkpoint` (WAL), `writer_admission` (flock), `thread_work`,
  `interleaved_saves` and the `concurrent_*` pair. Each file has 0 PostgreSQL mentions. The new name
  pairs with `tests/postgresql_session/`.
- The four helpers in `tests/postgresql_diagnostic/` move to `tests/support/pg/`. They were the
  shared `pg_doctor/` module, used by 4 test files, and rule 7 puts helpers in `tests/support/`:
  - `doctor_rig.rs` → `support/pg/doctor.rs`;
  - `conninfo.rs` → `support/pg/conninfo.rs`;
  - `namespace.rs` → `support/pg/namespace.rs`;
  - `proxy.rs` ("Independent wire-byte oracle") → `support/pg/oracle_proxy.rs`, so it is not
    confused with `fault_proxy.rs`.

**Renames:**

- `tests/diagnostic/sqlite_structure_diagnostic.rs` → `sqlite_structure.rs`.
- `tests/postgresql_diagnostic/diagnostic.rs` → `audit.rs`. Its header is "Real PostgreSQL audits";
  the source is `backend/postgresql/diagnostic/audit.rs`.
- `tests/store/serverless_store.rs` → `serverless.rs`.
- `tests/upgrade/store_upgrade_{chain,guards,postgresql,postgresql_wire,sqlite,sqlite_crash}.rs` →
  `{chain,guards,postgresql,postgresql_wire,sqlite,sqlite_crash}.rs`.
- `tests/gc/compact_metrics.rs` → `metrics.rs`, to follow `src/gc/metrics.rs`.

**Resulting counts:**

| Area | Topics |
| --- | --- |
| `diagnostic/` | 12 |
| `postgresql_diagnostic/` | 6 |
| `sqlite_session/` | 8 |
| `store/` | 7 |
| `support/pg/` | 9 helpers |

### 9. `gc/admission/session.rs` → `gc/admission/advisory.rs` (rule 13)

- **Evidence.** The header is "PostgreSQL session-scoped advisory keys" (`session_lock`,
  `session_unlock`, `session_unlock_op`). In both backends `session/` means the connection session
  (`backend/*/session/`), and this file imports
  `backend::postgresql::session::connection::{Session, SessionKey}`. So `admission/session.rs` reads
  as the admission side of a connection session.
- **Fix.** Rename it to `advisory.rs`.
- **Not changed.** `gc/admission/mount.rs` and `advisory.rs` are PostgreSQL-only halves of the
  admission module (`use super::*`). Moving them into `backend/postgresql/` would cut the module's
  private sharing for little gain, so they stay.

### 10. Small `sql/` placements (rules 9, 14)

- `sql/sqlite/acquisition/schema.sql` → `sql/sqlite/schema/acquisition.sql`.
  - SQLite's other DDL is in `sql/sqlite/schema/`.
  - PostgreSQL's acquisition DDL is `sql/postgresql/schema/acquisition.sql`.
  - The frozen copy is `sql/upgrade/sqlite/v2/acquisition.sql`.
  - `sql/sqlite/acquisition/` then holds only its statement folders.
- `sql/postgresql/metadata/diagnostic_objects.sql` → `sql/postgresql/schema/live_objects.sql`.
  - It is a `pg_catalog` query of live constraints and indexes, loaded by the diagnostic contract
    check (`include_str!` in `diagnostic/postgresql_contract.rs:9`).
  - It is not a Metadata statement. `metadata/` keeps `locate.sql` (`metadata/reads.rs`),
    `publish_rows.sql` and `chain/`.
- `sql/postgresql/history/node/commit_commit_by_id.sql` → `commit_by_id.sql`, dropping the doubled
  word. Its siblings `paths_node_markers` and `position_node_serial` keep their prefixes, because
  they come from three different key groups.

### 11. SQLite `history/layerstack/` vs `history/stack/`: split by table, not by key text (rule 11)

- **Problem.** The plan sends `layerstack_stack_*` keys to `stack/` and every other `layerstack_*`
  key to `layerstack/`. `layerstack/` (11 files) then mixes Layer reads and writes, Stack writes
  (`advance_stack`, `insert_stack`, `insert_retired`, `deleted_ms`) and the `project_epoch` fence
  (`bump_project_epoch`, `insert_project_epoch`). A reader who looks in `stack/` for the Stack
  insert does not find it.
- **Fix.** Split by the table each statement touches (checked in the SQL):
  - `history/layer/`: `by_id`, `by_source`, `identity_range`, `insert` and `root` (5 files, table
    `layer`);
  - `history/stack/`: its 6 reads plus `advance`, `insert`, `insert_retired` and `deleted_ms`
    (10 files, `layer_stack` and `layer_stack_retired`);
  - `sql/sqlite/gc/epoch/`: `bump.sql` and `insert.sql` (`project_epoch`, the fence).
- **Not changed.** PostgreSQL's `history/layerstack/` overrides only 3 statements and stays as
  planned. Mirroring it would make 1–2-file folders.

### 12. `examples/format_counters/` (rules 10, 14). Low value; the owner's pending item.

- **Evidence.** The four `counters_vN.rs` files are "Frozen vN matched component screen for
  publication accounting". In `diff v4 v5`, `vN` is the screen's version (a label and one extra
  field), not a Store format. In this crate "format" means the Store format (`upgrade/`), so the
  folder name misleads.
- **Problems.**
  - `counters_v2.rs` repeats the folder's name.
  - `readonly_diagnostic.rs` ("One labeled diagnostic of sealed WAL read-only setup", 26 lines) is
    not one of the variants.
- **Fix.**
  - `examples/publication_counters/{main,v2,v3,v4,v5}.rs`.
  - `readonly_diagnostic.rs` → loose `examples/sealed_readonly.rs`, which makes 8 loose example
    targets, against the exception's 7.

## 3. Proposed layouts of changed folders

Scope: archive commit 1317ebf7ca. Production LOC is from `tools/production_loc.py --files` and
includes the shipped runtime SQL. Test files count 0, and so do example files, which are outside
the production scope. Listings show only the changed folders.

```diff
  engine/crates/layerfs-persistence/src/
    diagnostic/                          [2,138]
      mod.rs                                  27
      observation.rs                          68
      reader.rs                              129
-     scan.rs                                288   (→ check/store.rs; finding 4)
+     scratch.rs                             151   (from check/; audit infrastructure)
-     transaction.rs                          60   (→ view.rs)
+     view.rs                                 60   (was transaction.rs; holds only View)
      check/                             [1,854]   (subtotal includes scratch.rs leaving)
        acquisition.rs                       121
        auxiliary.rs                         139
        catalog.rs                            67
        facts.rs                             119
        history.rs                           242
        lineage.rs                           263
        mod.rs                                 —
        mounts.rs                             99
        project.rs                           365
-       scratch.rs                           151   (→ diagnostic/scratch.rs)
+       store.rs                             288   (was diagnostic/scan.rs: whole-Store check)
    gc/                                  [5,973]   (admission/, mark/, spill/ partly shown)
+     compact.rs                               —   new (shared §11 types from backend/sqlite/compact/types.rs + phases.rs)
-     epoch.rs                                32   (→ fence.rs)
+     fence.rs                                32   (was epoch.rs: holds Fence)
      metrics.rs                             559   (imports gc::compact, not backend::sqlite)
      mod.rs                                   —
-     phases.rs                               12   (merged into compact.rs)
      admission/                         [1,502]   (partial: one file shown)
-       session.rs                           196   (→ advisory.rs)
+       advisory.rs                          196   (PostgreSQL session-scoped advisory keys)
    history/                             [3,452]   (counters/, graph/, index/, lineage/ unchanged)
      catalog.rs                             378
      mod.rs                                  44
      provider.rs                             20
-     query/                               [312]   (name collides with layerfs_history::query)
-       bindings.rs                           85
-       mod.rs                                 —
-       rows.rs                              158
-       transaction.rs                        69
+     statement/                           [312]   (same files)
+       bindings.rs                           85
+       mod.rs                                 —
+       rows.rs                              158
+       transaction.rs                        69
    storage/                               [831]
      mod.rs                                   2
      objects.rs                              16
-     provider.rs                            121   (→ backend/sqlite/objects/provider.rs)
      publication.rs                          96
      blocks/                              [482]   (unchanged)
      metadata/                            [114]   (unchanged)
    store/                                 [658]
      config.rs                              143
      mod.rs                                   6
-     open.rs                                144   (merged into sqlite.rs)
      postgresql.rs                          192
-     seal.rs                                 24   (merged into sqlite.rs)
      selection.rs                           111
      sqlite.rs                               38   (+ open.rs + seal.rs = 206: all of SqliteHandles)
    backend/                                       (cross-scope, partial)
      sqlite/
        compact/                           [210]   (partial: one file shown)
          types.rs                           210   (split: shared types → gc/compact.rs; SQLite sidecars/errors stay)
        objects/                           [857]   (736 + provider.rs)
          mod.rs                               7
+         provider.rs                        121   (from storage/; pairs with postgresql/objects/provider.rs)
          publish.rs                         136
          read.rs                            238
          selection.rs                        83
          unit_io.rs                          70
          unit_layout.rs                      37
          units_publish.rs                    35
          units_read.rs                      130
```

```diff
  engine/crates/layerfs-persistence/sql/sqlite/            (partial)
    acquisition/                            [41]   (partial: statement folders unchanged)
-     schema.sql                              41   (→ schema/acquisition.sql)
+   gc/                                    [299]   new (GC statements follow src gc/)
+     epoch/                                 [5]
+       bump.sql                               1   (was history/layerstack/bump_project_epoch.sql)
+       insert.sql                             1   (was history/layerstack/insert_project_epoch.sql)
+       lock.sql                               1   (was history/gc/epoch_lock.sql)
+       read.sql                               1   (was history/gc/epoch_read.sql)
+       share.sql                              1   (was history/gc/epoch_share.sql)
+     intent/                               [64]   (was history/gc_intent/, same 10 files)
+     intent_scope/                         [65]   (was history/gc_intent_scope/, same 6 files)
+     kept_node/                           [111]   (was history/kept_node/, same 6 files)
+     mark/                                 [17]
+       inode_blocks.sql                       1
+       live_branches.sql                      5
+       live_stacks.sql                        4
+       pack_lengths.sql                       1   (read by sqlite compact/run.rs)
+       tuple_read.sql                         6
+     unreachable_removal/                  [37]   (was history/unreachable_removal/, same 9 files)
    history/                               [314]   (partial: allocation, branch, commit, index, lineage, paths, position, removal, statistics unchanged)
-     gc/                                   [20]   (→ gc/epoch/ and gc/mark/)
-     gc_intent/                            [64]   (→ gc/intent/)
-     gc_intent_scope/                      [65]   (→ gc/intent_scope/)
-     kept_node/                           [111]   (→ gc/kept_node/)
+     layer/                                 [5]   new (table layer)
+       by_id.sql                              1
+       by_source.sql                          1
+       identity_range.sql                     1
+       insert.sql                             1
+       root.sql                               1
-     layerstack/                           [11]   (split by table: layer/, stack/, gc/epoch/)
      stack/                                 [6]   (6 → 10 files)
        by_id.sql                              1
        by_name.sql                            1
        first_page.sql                         1
        head.sql                               1
        name_taken.sql                         1
        next_page.sql                          1
+       advance.sql                            1   (was layerstack/advance_stack.sql)
+       deleted_ms.sql                         1   (was layerstack/deleted_ms.sql)
+       insert.sql                             1   (was layerstack/insert_stack.sql)
+       insert_retired.sql                     1   (was layerstack/insert_retired.sql)
-     unreachable_removal/                  [37]   (→ gc/unreachable_removal/)
    schema/                                [392]
+     acquisition.sql                         41   (from acquisition/schema.sql)
      file_digest.sql                          6
      gc.sql                                  95
      history.sql                            164
      lineage.sql                             31
      merge_scratch.sql                       12
      metadata.sql                            46
      objects.sql                             12
      objects_units.sql                       25
      objects_units_index.sql                  1
```

```diff
  engine/crates/layerfs-persistence/sql/postgresql/        (partial: fused/, mount/ unchanged)
+   gc/                                    [111]   new (was history/gc/, same 10 files)
    history/                               [111]   (partial: branch/, index/, layerstack/ unchanged)
-     gc/                                  [111]   (→ sql/postgresql/gc/)
      node/                                  [3]
-       commit_commit_by_id.sql                1   (→ commit_by_id.sql)
+       commit_by_id.sql                       1
        paths_node_markers.sql                 1
        position_node_serial.sql               1
    metadata/                               [63]   (chain/ unchanged)
-     diagnostic_objects.sql                   5   (→ schema/live_objects.sql)
      locate.sql                               1
      publish_rows.sql                        32
    schema/                                [414]
      acquisition.sql                         41
      gc.sql                                  97
+     live_objects.sql                         5   (was metadata/diagnostic_objects.sql)
      store.sql                              276
```

```diff
  engine/crates/layerfs-persistence/tests/                 (partial; test code counts 0 production LOC)
    diagnostic/                              [0]
      content.rs                               0
      dependencies.rs                          0
-     diagnostics_during_lane.rs               0   (→ sqlite_session/counters_during_lane.rs)
      facts.rs                                 0
      framing.rs                               0
      history.rs                               0
      inventory.rs                             0
      main.rs                                  —
      namespace.rs                             0
      overwrite.rs                             0
      schema.rs                                0
      snapshot.rs                              0
-     sqlite_structure_diagnostic.rs           0   (→ sqlite_structure.rs)
+     sqlite_structure.rs                      0
+     store.rs                                 0   (from store/diagnostic.rs)
    gc/                                      [0]   (partial)
-     compact_metrics.rs                       0   (→ metrics.rs)
-     epoch_fence.rs                           0   (→ fence.rs)
+     fence.rs                                 0
+     metrics.rs                               0
    postgresql_diagnostic/                   [0]
+     audit.rs                                 0   (was diagnostic.rs)
+     connection_step.rs                       0   (from session/)
-     conninfo.rs                              0   (→ support/pg/)
-     diagnostic.rs                            0   (→ audit.rs)
      doctor_access.rs                         0
      doctor_cancellation.rs                   0
      doctor_physical.rs                       0
-     doctor_rig.rs                            0   (→ support/pg/doctor.rs)
      main.rs                                  —
-     namespace.rs                             0   (→ support/pg/)
-     proxy.rs                                 0   (→ support/pg/oracle_proxy.rs)
      status.rs                                0
-   session/                                 [0]   (→ sqlite_session/; connection_step.rs → postgresql_diagnostic/)
+   sqlite_session/                          [0]   (pairs with postgresql_session/)
+     background_checkpoint.rs                 0
+     concurrent_first_installs.rs             0
+     concurrent_pooled_saves.rs               0
+     counters_during_lane.rs                  0   (from diagnostic/diagnostics_during_lane.rs)
+     idle_checkpoint.rs                       0
+     interleaved_saves.rs                     0
+     main.rs                                  —
+     thread_work.rs                           0
+     writer_admission.rs                      0
    store/                                   [0]
-     diagnostic.rs                            0   (→ diagnostic/store.rs)
      headroom.rs                              0
      immutable_reads_under_reclamation.rs     0
      main.rs                                  —
      seal_allocation.rs                       0
      sealed_open.rs                           0
-     serverless_store.rs                      0   (→ serverless.rs)
+     serverless.rs                            0
      snapshot.rs                              0
      space_reclamation.rs                     0
    support/                                 [0]   (partial)
      pg/                                    [0]
        chains.rs                              0
        connection.rs                          0
+       conninfo.rs                            0   (from postgresql_diagnostic/)
+       doctor.rs                              0   (was postgresql_diagnostic/doctor_rig.rs)
        fault_proxy.rs                         0
        fixture.rs                             0
        mod.rs                                 —
+       namespace.rs                           0   (from postgresql_diagnostic/)
+       oracle_proxy.rs                        0   (was postgresql_diagnostic/proxy.rs)
        wire_tap.rs                            0
    upgrade/                                 [0]   (partial: the store_upgrade_* files)
-     store_upgrade_chain.rs                   0   (→ chain.rs)
-     store_upgrade_guards.rs                  0   (→ guards.rs)
-     store_upgrade_postgresql.rs              0   (→ postgresql.rs)
-     store_upgrade_postgresql_wire.rs         0   (→ postgresql_wire.rs)
-     store_upgrade_sqlite.rs                  0   (→ sqlite.rs)
-     store_upgrade_sqlite_crash.rs            0   (→ sqlite_crash.rs)
+     chain.rs                                 0
+     guards.rs                                0
+     postgresql.rs                            0
+     postgresql_wire.rs                       0
+     sqlite.rs                                0
+     sqlite_crash.rs                          0
```

```diff
  engine/crates/layerfs-persistence/examples/              (partial; examples count 0 production LOC)
-   format_counters/                         [0]   (→ publication_counters/)
+   publication_counters/                    [0]
+     main.rs                                  —
+     v2.rs                                    0
+     v3.rs                                    0
+     v4.rs                                    0
+     v5.rs                                    0
+   sealed_readonly.rs                         0   (was format_counters/readonly_diagnostic.rs)
```

## 4. Checked and left alone

- **Root row:** `backend/`, `diagnostic/`, `gc/`, `history/`, `storage/`, `store/` and `upgrade/`,
  plus `internals.rs` and `lib.rs`. These are 7 distinct roles with 1 loose file. Fine.
- **`gc/` vs the backends' `compact/`.** Different scopes: GC is admission, fence, mark and
  compaction. Finding 2 makes `gc/compact.rs` the neutral half of the backends' `compact/`, which is
  consistent with that.
- **`gc/mark/`** (11 files: `accessors`, `edges`, `history`, `ids`, `objects`, `paged`, `plan`,
  `table`, `values`, `walk`, `waste`) and **`gc/spill/`** (`lists`, `records`, `sort`): one role
  each.
  - `history.rs` and `objects.rs` name the two closures (§6.1 and §6.2).
  - `values.rs` means inode values, as it does in `storage/blocks/` and `storage/metadata/`.
- **`gc/admission/intents.rs`** (748 LOC, the old module root): it operates on `gc_intent` rows, so
  the name is fine. Core's `admission/intent.rs` is the record vocabulary, a different job.
- **`gc/admission/mount.rs`:** PostgreSQL-only, but it shares the parent's private items. See
  finding 9.
- **`history/graph/`** (`branch`, `commit`, `fused`, `layerstack`, `merge`, `removal`). `removal.rs`
  holds `remove_layerstack` and `mark_branch_removed`, the history-row half of removal, and matches
  `sql/sqlite/history/removal/`. Distinct from `gc/admission/removal.rs`, the admission half.
- **`history/index/`** (`digests`, `hints`, `identity_range`, `paths`). `paths.rs` is the
  `path_removed` marker table, read per node at mount (`paths_node_markers`), so it serves as a
  lookup index. Acceptable.
- **`history/counters/facts.rs`** holds the merge-facts blob codec plus Store statistics: a slight
  mix. Not worth a split.
- **`history/lineage/`** and **`history/catalog.rs` + `provider.rs`:** fine.
- **`storage/blocks/`** (`frame`, `hints`, `values`): fine.
- **`storage/objects.rs`** (16 LOC): the neutral read contract, used by both providers. Fine.
- **`upgrade/`** (`chain`, `dispatch`, `types`): a clean neutral half beside each backend's
  `upgrade/`. `types.rs` is a broad name (prior review tolerated it). `format.rs` would collide with
  `backend/sqlite/upgrade/format.rs`, so it stays.
- **`store/config.rs`:** both engines' selection types in one file. Fine.
- **`sql/sqlite/acquisition/{discard,operation,page,put,read,write}/`.**
  - `read/` is an owner-rejected word (rule 12), and `read/` + `page/` and `put/` + `write/` overlap
    (window reads, and inserts/updates).
  - Splitting by table (`init_operation`, `init_entry`, `init_host_file`) gives a 14-file
    `entry/`, or mixed axes. Neither is clearly better.
  - Left as a question for the owner. If changed, `read/` → `job/` + `tree/` reads are the obvious
    seams.
- **`sql/postgresql/history/node/`:** a small grab-bag of three per-node overrides from three key
  groups. Mirroring the SQLite folder names would make 1-file folders. Kept, with only the stutter
  fixed (finding 10).
- **`sql/postgresql/{fused,mount}/`** at the root: one-message pipelines, cross-cutting. Kept.
- **`sql/upgrade/*`:** frozen snapshots, under granted exceptions.
- **`sql/sqlite/history/lineage/`:** the granted 14-file exception.
- **Tests: the other areas** (`acquisition`, `gc_mark`, `history`, `metadata`, `postgresql`,
  `postgresql_gc`, `postgresql_metadata`, `postgresql_session`, `sqlite`, `sqlite_compact`) are fine.
  - `history/inode_branch_delta_custody.rs` is storage-level, but it fits `history/` well enough.
- **Examples: the other targets** (`gc_bench/`, `storage_gate_bench/`, the loose targets) were not
  restructured, per the exception.
- **Public paths.** Only `pub mod compact_metrics` was already a planned public path change (11
  callers). Findings 1–7 touch private modules; `lib.rs` re-exports keep the root names.

## 5. Machine-readable list

```json
[
 {"from": "engine/crates/layerfs-persistence/src/history/query/bindings.rs", "to": "engine/crates/layerfs-persistence/src/history/statement/bindings.rs", "op": "move", "why": "query/ collides with layerfs_history::query imported in history/mod.rs and used as super::query by graph files"},
 {"from": "engine/crates/layerfs-persistence/src/history/query/rows.rs", "to": "engine/crates/layerfs-persistence/src/history/statement/rows.rs", "op": "move", "why": "query/ collides with layerfs_history::query imported in history/mod.rs and used as super::query by graph files"},
 {"from": "engine/crates/layerfs-persistence/src/history/query/transaction.rs", "to": "engine/crates/layerfs-persistence/src/history/statement/transaction.rs", "op": "move", "why": "query/ collides with layerfs_history::query imported in history/mod.rs and used as super::query by graph files"},
 {"from": "engine/crates/layerfs-persistence/src/history/query/mod.rs", "to": "engine/crates/layerfs-persistence/src/history/statement/mod.rs", "op": "move", "why": "query/ collides with layerfs_history::query imported in history/mod.rs and used as super::query by graph files"},
 {"from": "engine/crates/layerfs-persistence/src/backend/sqlite/compact/types.rs", "to": "engine/crates/layerfs-persistence/src/backend/sqlite/compact/types.rs", "op": "split", "why": "cross-scope: shared section-11 request/receipt types used by PostgreSQL compact and gc/metrics.rs leave the SQLite backend"},
 {"from": "engine/crates/layerfs-persistence/src/backend/sqlite/compact/types.rs", "to": "engine/crates/layerfs-persistence/src/gc/compact.rs", "op": "new", "why": "neutral compaction request/receipt types (CompactRequest, CompactPath, CompactFailpoint, OwnerLiveness, CompactKeptWaste, CompactResidue, CompactZeroReason, CompactReceipt, DELTA_BASE_REFERENCES_CAP)"},
 {"from": "engine/crates/layerfs-persistence/src/gc/phases.rs", "to": "engine/crates/layerfs-persistence/src/gc/compact.rs", "op": "merge", "why": "CompactStatus and the phase lists are the same neutral compaction vocabulary"},
 {"from": "engine/crates/layerfs-persistence/src/storage/provider.rs", "to": "engine/crates/layerfs-persistence/src/backend/sqlite/objects/provider.rs", "op": "move", "why": "cross-scope: StorageProvider wraps the SQLite Session and calls sqlite objects/lifecycle code; PostgreSQL provider already sits at backend/postgresql/objects/provider.rs"},
 {"from": "engine/crates/layerfs-persistence/src/diagnostic/scan.rs", "to": "engine/crates/layerfs-persistence/src/diagnostic/check/store.rs", "op": "rename", "why": "the whole-Store check (store.packs/objects/namespace), sibling of check/project.rs"},
 {"from": "engine/crates/layerfs-persistence/src/diagnostic/check/scratch.rs", "to": "engine/crates/layerfs-persistence/src/diagnostic/scratch.rs", "op": "move", "why": "audit infrastructure like reader.rs, not a check"},
 {"from": "engine/crates/layerfs-persistence/src/diagnostic/transaction.rs", "to": "engine/crates/layerfs-persistence/src/diagnostic/view.rs", "op": "rename", "why": "holds only the View trait; transaction.rs collides with backend session/transaction.rs and history transaction facade"},
 {"from": "engine/crates/layerfs-persistence/src/store/open.rs", "to": "engine/crates/layerfs-persistence/src/store/sqlite.rs", "op": "merge", "why": "impl SqliteHandles create/open; SQLite-only, the counterpart of store/postgresql.rs"},
 {"from": "engine/crates/layerfs-persistence/src/store/seal.rs", "to": "engine/crates/layerfs-persistence/src/store/sqlite.rs", "op": "merge", "why": "impl SqliteHandles::seal and SealedStore; SQLite-only"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc/epoch_lock.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/epoch/lock.sql", "op": "move", "why": "project_epoch fence statements follow src gc/fence.rs"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc/epoch_read.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/epoch/read.sql", "op": "move", "why": "project_epoch fence statements follow src gc/fence.rs"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc/epoch_share.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/epoch/share.sql", "op": "move", "why": "project_epoch fence statements follow src gc/fence.rs"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc/inode_blocks.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/mark/inode_blocks.sql", "op": "move", "why": "mark statements follow src gc/mark/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc/live_branches.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/mark/live_branches.sql", "op": "move", "why": "mark statements follow src gc/mark/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc/live_stacks.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/mark/live_stacks.sql", "op": "move", "why": "mark statements follow src gc/mark/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc/pack_lengths.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/mark/pack_lengths.sql", "op": "move", "why": "mark statements follow src gc/mark/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc/tuple_read.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/mark/tuple_read.sql", "op": "move", "why": "mark statements follow src gc/mark/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc_intent/done.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/intent/done.sql", "op": "move", "why": "GC admission statements follow src gc/admission/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc_intent/done_branch.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/intent/done_branch.sql", "op": "move", "why": "GC admission statements follow src gc/admission/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc_intent/done_cap.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/intent/done_cap.sql", "op": "move", "why": "GC admission statements follow src gc/admission/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc_intent/done_newest.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/intent/done_newest.sql", "op": "move", "why": "GC admission statements follow src gc/admission/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc_intent/insert.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/intent/insert.sql", "op": "move", "why": "GC admission statements follow src gc/admission/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc_intent/list.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/intent/list.sql", "op": "move", "why": "GC admission statements follow src gc/admission/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc_intent/list_machine.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/intent/list_machine.sql", "op": "move", "why": "GC admission statements follow src gc/admission/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc_intent/release.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/intent/release.sql", "op": "move", "why": "GC admission statements follow src gc/admission/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc_intent/release_done.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/intent/release_done.sql", "op": "move", "why": "GC admission statements follow src gc/admission/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc_intent/uncertain.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/intent/uncertain.sql", "op": "move", "why": "GC admission statements follow src gc/admission/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc_intent_scope/all_branch.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/intent_scope/all_branch.sql", "op": "move", "why": "GC admission statements follow src gc/admission/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc_intent_scope/all_project.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/intent_scope/all_project.sql", "op": "move", "why": "GC admission statements follow src gc/admission/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc_intent_scope/all_store.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/intent_scope/all_store.sql", "op": "move", "why": "GC admission statements follow src gc/admission/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc_intent_scope/ops_branch.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/intent_scope/ops_branch.sql", "op": "move", "why": "GC admission statements follow src gc/admission/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc_intent_scope/ops_project.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/intent_scope/ops_project.sql", "op": "move", "why": "GC admission statements follow src gc/admission/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/gc_intent_scope/ops_store.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/intent_scope/ops_store.sql", "op": "move", "why": "GC admission statements follow src gc/admission/"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/kept_node/clear.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/kept_node/clear.sql", "op": "move", "why": "history closure of the mark (gc/mark/history.rs)"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/kept_node/expand.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/kept_node/expand.sql", "op": "move", "why": "history closure of the mark (gc/mark/history.rs)"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/kept_node/kept_roots.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/kept_node/kept_roots.sql", "op": "move", "why": "history closure of the mark (gc/mark/history.rs)"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/kept_node/page.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/kept_node/page.sql", "op": "move", "why": "history closure of the mark (gc/mark/history.rs)"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/kept_node/roots.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/kept_node/roots.sql", "op": "move", "why": "history closure of the mark (gc/mark/history.rs)"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/kept_node/seed.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/kept_node/seed.sql", "op": "move", "why": "history closure of the mark (gc/mark/history.rs)"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/unreachable_removal/absorbed.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/unreachable_removal/absorbed.sql", "op": "move", "why": "used only by gc/mark/history.rs"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/unreachable_removal/branch.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/unreachable_removal/branch.sql", "op": "move", "why": "used only by gc/mark/history.rs"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/unreachable_removal/commit.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/unreachable_removal/commit.sql", "op": "move", "why": "used only by gc/mark/history.rs"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/unreachable_removal/commit_lineage.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/unreachable_removal/commit_lineage.sql", "op": "move", "why": "used only by gc/mark/history.rs"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/unreachable_removal/layer.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/unreachable_removal/layer.sql", "op": "move", "why": "used only by gc/mark/history.rs"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/unreachable_removal/lineage.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/unreachable_removal/lineage.sql", "op": "move", "why": "used only by gc/mark/history.rs"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/unreachable_removal/reach.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/unreachable_removal/reach.sql", "op": "move", "why": "used only by gc/mark/history.rs"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/unreachable_removal/stack.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/unreachable_removal/stack.sql", "op": "move", "why": "used only by gc/mark/history.rs"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/unreachable_removal/totals.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/unreachable_removal/totals.sql", "op": "move", "why": "used only by gc/mark/history.rs"},
 {"from": "engine/crates/layerfs-persistence/sql/postgresql/history/gc/advisory_holder.sql", "to": "engine/crates/layerfs-persistence/sql/postgresql/gc/advisory_holder.sql", "op": "move", "why": "GC statements follow src gc/, as on SQLite"},
 {"from": "engine/crates/layerfs-persistence/sql/postgresql/history/gc/epoch_lock.sql", "to": "engine/crates/layerfs-persistence/sql/postgresql/gc/epoch_lock.sql", "op": "move", "why": "GC statements follow src gc/, as on SQLite"},
 {"from": "engine/crates/layerfs-persistence/sql/postgresql/history/gc/epoch_read.sql", "to": "engine/crates/layerfs-persistence/sql/postgresql/gc/epoch_read.sql", "op": "move", "why": "GC statements follow src gc/, as on SQLite"},
 {"from": "engine/crates/layerfs-persistence/sql/postgresql/history/gc/epoch_share.sql", "to": "engine/crates/layerfs-persistence/sql/postgresql/gc/epoch_share.sql", "op": "move", "why": "GC statements follow src gc/, as on SQLite"},
 {"from": "engine/crates/layerfs-persistence/sql/postgresql/history/gc/intent_release_owner.sql", "to": "engine/crates/layerfs-persistence/sql/postgresql/gc/intent_release_owner.sql", "op": "move", "why": "GC statements follow src gc/, as on SQLite"},
 {"from": "engine/crates/layerfs-persistence/sql/postgresql/history/gc/kept_node_mark.sql", "to": "engine/crates/layerfs-persistence/sql/postgresql/gc/kept_node_mark.sql", "op": "move", "why": "GC statements follow src gc/, as on SQLite"},
 {"from": "engine/crates/layerfs-persistence/sql/postgresql/history/gc/lock_session.sql", "to": "engine/crates/layerfs-persistence/sql/postgresql/gc/lock_session.sql", "op": "move", "why": "GC statements follow src gc/, as on SQLite"},
 {"from": "engine/crates/layerfs-persistence/sql/postgresql/history/gc/lock_xact_exclusive.sql", "to": "engine/crates/layerfs-persistence/sql/postgresql/gc/lock_xact_exclusive.sql", "op": "move", "why": "GC statements follow src gc/, as on SQLite"},
 {"from": "engine/crates/layerfs-persistence/sql/postgresql/history/gc/lock_xact_shared.sql", "to": "engine/crates/layerfs-persistence/sql/postgresql/gc/lock_xact_shared.sql", "op": "move", "why": "GC statements follow src gc/, as on SQLite"},
 {"from": "engine/crates/layerfs-persistence/sql/postgresql/history/gc/unlock_session.sql", "to": "engine/crates/layerfs-persistence/sql/postgresql/gc/unlock_session.sql", "op": "move", "why": "GC statements follow src gc/, as on SQLite"},
 {"from": "engine/crates/layerfs-persistence/src/gc/epoch.rs", "to": "engine/crates/layerfs-persistence/src/gc/fence.rs", "op": "rename", "why": "holds Fence (project_epoch writer fence); backend/epoch.rs is the unrelated session acquisition Epoch"},
 {"from": "engine/crates/layerfs-persistence/tests/gc/epoch_fence.rs", "to": "engine/crates/layerfs-persistence/tests/gc/fence.rs", "op": "rename", "why": "follows src gc/fence.rs; drops the folder word"},
 {"from": "engine/crates/layerfs-persistence/src/gc/admission/session.rs", "to": "engine/crates/layerfs-persistence/src/gc/admission/advisory.rs", "op": "rename", "why": "PostgreSQL session-scoped advisory keys; session/ means the connection session in both backends"},
 {"from": "engine/crates/layerfs-persistence/tests/diagnostic/diagnostics_during_lane.rs", "to": "engine/crates/layerfs-persistence/tests/sqlite_session/counters_during_lane.rs", "op": "move", "why": "tests SqlWork counters (storage.diagnostics()), not Store checks; sits with thread_work.rs"},
 {"from": "engine/crates/layerfs-persistence/tests/diagnostic/sqlite_structure_diagnostic.rs", "to": "engine/crates/layerfs-persistence/tests/diagnostic/sqlite_structure.rs", "op": "rename", "why": "file repeats its folder name"},
 {"from": "engine/crates/layerfs-persistence/tests/store/diagnostic.rs", "to": "engine/crates/layerfs-persistence/tests/diagnostic/store.rs", "op": "move", "why": "Observation-based Store diagnosis; mirrors src diagnostic/check/store.rs"},
 {"from": "engine/crates/layerfs-persistence/tests/store/serverless_store.rs", "to": "engine/crates/layerfs-persistence/tests/store/serverless.rs", "op": "rename", "why": "file repeats its folder name"},
 {"from": "engine/crates/layerfs-persistence/tests/session/background_checkpoint.rs", "to": "engine/crates/layerfs-persistence/tests/sqlite_session/background_checkpoint.rs", "op": "move", "why": "all SQLite session topics; pairs with tests/postgresql_session/"},
 {"from": "engine/crates/layerfs-persistence/tests/session/concurrent_first_installs.rs", "to": "engine/crates/layerfs-persistence/tests/sqlite_session/concurrent_first_installs.rs", "op": "move", "why": "all SQLite session topics; pairs with tests/postgresql_session/"},
 {"from": "engine/crates/layerfs-persistence/tests/session/concurrent_pooled_saves.rs", "to": "engine/crates/layerfs-persistence/tests/sqlite_session/concurrent_pooled_saves.rs", "op": "move", "why": "all SQLite session topics; pairs with tests/postgresql_session/"},
 {"from": "engine/crates/layerfs-persistence/tests/session/idle_checkpoint.rs", "to": "engine/crates/layerfs-persistence/tests/sqlite_session/idle_checkpoint.rs", "op": "move", "why": "all SQLite session topics; pairs with tests/postgresql_session/"},
 {"from": "engine/crates/layerfs-persistence/tests/session/interleaved_saves.rs", "to": "engine/crates/layerfs-persistence/tests/sqlite_session/interleaved_saves.rs", "op": "move", "why": "all SQLite session topics; pairs with tests/postgresql_session/"},
 {"from": "engine/crates/layerfs-persistence/tests/session/main.rs", "to": "engine/crates/layerfs-persistence/tests/sqlite_session/main.rs", "op": "move", "why": "all SQLite session topics; pairs with tests/postgresql_session/"},
 {"from": "engine/crates/layerfs-persistence/tests/session/thread_work.rs", "to": "engine/crates/layerfs-persistence/tests/sqlite_session/thread_work.rs", "op": "move", "why": "all SQLite session topics; pairs with tests/postgresql_session/"},
 {"from": "engine/crates/layerfs-persistence/tests/session/writer_admission.rs", "to": "engine/crates/layerfs-persistence/tests/sqlite_session/writer_admission.rs", "op": "move", "why": "all SQLite session topics; pairs with tests/postgresql_session/"},
 {"from": "engine/crates/layerfs-persistence/tests/session/connection_step.rs", "to": "engine/crates/layerfs-persistence/tests/postgresql_diagnostic/connection_step.rs", "op": "move", "why": "tests diagnostic::ConnectionStep, the PostgreSQL doctor step"},
 {"from": "engine/crates/layerfs-persistence/tests/postgresql_diagnostic/diagnostic.rs", "to": "engine/crates/layerfs-persistence/tests/postgresql_diagnostic/audit.rs", "op": "rename", "why": "file repeats its folder; matches backend/postgresql/diagnostic/audit.rs"},
 {"from": "engine/crates/layerfs-persistence/tests/postgresql_diagnostic/doctor_rig.rs", "to": "engine/crates/layerfs-persistence/tests/support/pg/doctor.rs", "op": "move", "why": "shared helper (was pg_doctor/mod.rs); helpers live in tests/support/"},
 {"from": "engine/crates/layerfs-persistence/tests/postgresql_diagnostic/conninfo.rs", "to": "engine/crates/layerfs-persistence/tests/support/pg/conninfo.rs", "op": "move", "why": "shared helper; helpers live in tests/support/"},
 {"from": "engine/crates/layerfs-persistence/tests/postgresql_diagnostic/namespace.rs", "to": "engine/crates/layerfs-persistence/tests/support/pg/namespace.rs", "op": "move", "why": "shared helper; helpers live in tests/support/"},
 {"from": "engine/crates/layerfs-persistence/tests/postgresql_diagnostic/proxy.rs", "to": "engine/crates/layerfs-persistence/tests/support/pg/oracle_proxy.rs", "op": "move", "why": "shared helper (wire-byte oracle); distinct from fault_proxy.rs"},
 {"from": "engine/crates/layerfs-persistence/tests/upgrade/store_upgrade_chain.rs", "to": "engine/crates/layerfs-persistence/tests/upgrade/chain.rs", "op": "rename", "why": "file repeats its folder name"},
 {"from": "engine/crates/layerfs-persistence/tests/upgrade/store_upgrade_guards.rs", "to": "engine/crates/layerfs-persistence/tests/upgrade/guards.rs", "op": "rename", "why": "file repeats its folder name"},
 {"from": "engine/crates/layerfs-persistence/tests/upgrade/store_upgrade_postgresql.rs", "to": "engine/crates/layerfs-persistence/tests/upgrade/postgresql.rs", "op": "rename", "why": "file repeats its folder name"},
 {"from": "engine/crates/layerfs-persistence/tests/upgrade/store_upgrade_postgresql_wire.rs", "to": "engine/crates/layerfs-persistence/tests/upgrade/postgresql_wire.rs", "op": "rename", "why": "file repeats its folder name"},
 {"from": "engine/crates/layerfs-persistence/tests/upgrade/store_upgrade_sqlite.rs", "to": "engine/crates/layerfs-persistence/tests/upgrade/sqlite.rs", "op": "rename", "why": "file repeats its folder name"},
 {"from": "engine/crates/layerfs-persistence/tests/upgrade/store_upgrade_sqlite_crash.rs", "to": "engine/crates/layerfs-persistence/tests/upgrade/sqlite_crash.rs", "op": "rename", "why": "file repeats its folder name"},
 {"from": "engine/crates/layerfs-persistence/tests/gc/compact_metrics.rs", "to": "engine/crates/layerfs-persistence/tests/gc/metrics.rs", "op": "rename", "why": "follows src gc/metrics.rs"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/acquisition/schema.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/schema/acquisition.sql", "op": "move", "why": "DDL beside the other SQLite DDL; matches sql/postgresql/schema/acquisition.sql and sql/upgrade/sqlite/v2/acquisition.sql"},
 {"from": "engine/crates/layerfs-persistence/sql/postgresql/metadata/diagnostic_objects.sql", "to": "engine/crates/layerfs-persistence/sql/postgresql/schema/live_objects.sql", "op": "move", "why": "catalog query of live schema objects for the diagnostic contract check, not a Metadata statement"},
 {"from": "engine/crates/layerfs-persistence/sql/postgresql/history/node/commit_commit_by_id.sql", "to": "engine/crates/layerfs-persistence/sql/postgresql/history/node/commit_by_id.sql", "op": "rename", "why": "drops the doubled key word"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/layerstack/layer_by_id.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/history/layer/by_id.sql", "op": "move", "why": "layer table statements in one folder"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/layerstack/layer_by_source.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/history/layer/by_source.sql", "op": "move", "why": "layer table statements in one folder"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/layerstack/identity_range.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/history/layer/identity_range.sql", "op": "move", "why": "layer table statements in one folder"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/layerstack/insert_layer.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/history/layer/insert.sql", "op": "move", "why": "layer table statements in one folder"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/layerstack/layer_root.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/history/layer/root.sql", "op": "move", "why": "layer table statements in one folder"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/layerstack/advance_stack.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/history/stack/advance.sql", "op": "move", "why": "layer_stack table statements join the stack reads"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/layerstack/insert_stack.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/history/stack/insert.sql", "op": "move", "why": "layer_stack table statements join the stack reads"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/layerstack/insert_retired.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/history/stack/insert_retired.sql", "op": "move", "why": "layer_stack table statements join the stack reads"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/layerstack/deleted_ms.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/history/stack/deleted_ms.sql", "op": "move", "why": "layer_stack table statements join the stack reads"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/layerstack/bump_project_epoch.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/epoch/bump.sql", "op": "move", "why": "project_epoch fence table"},
 {"from": "engine/crates/layerfs-persistence/sql/sqlite/history/layerstack/insert_project_epoch.sql", "to": "engine/crates/layerfs-persistence/sql/sqlite/gc/epoch/insert.sql", "op": "move", "why": "project_epoch fence table"},
 {"from": "engine/crates/layerfs-persistence/examples/format_counters/counters_v2.rs", "to": "engine/crates/layerfs-persistence/examples/publication_counters/v2.rs", "op": "move", "why": "publication-accounting screens; vN is the screen version, not a Store format"},
 {"from": "engine/crates/layerfs-persistence/examples/format_counters/counters_v3.rs", "to": "engine/crates/layerfs-persistence/examples/publication_counters/v3.rs", "op": "move", "why": "publication-accounting screens; vN is the screen version, not a Store format"},
 {"from": "engine/crates/layerfs-persistence/examples/format_counters/counters_v4.rs", "to": "engine/crates/layerfs-persistence/examples/publication_counters/v4.rs", "op": "move", "why": "publication-accounting screens; vN is the screen version, not a Store format"},
 {"from": "engine/crates/layerfs-persistence/examples/format_counters/counters_v5.rs", "to": "engine/crates/layerfs-persistence/examples/publication_counters/v5.rs", "op": "move", "why": "publication-accounting screens; vN is the screen version, not a Store format"},
 {"from": "engine/crates/layerfs-persistence/examples/format_counters/main.rs", "to": "engine/crates/layerfs-persistence/examples/publication_counters/main.rs", "op": "move", "why": "folder renamed"},
 {"from": "engine/crates/layerfs-persistence/examples/format_counters/readonly_diagnostic.rs", "to": "engine/crates/layerfs-persistence/examples/sealed_readonly.rs", "op": "move", "why": "unrelated sealed-WAL read-only probe; not a counters variant"}
]
```
