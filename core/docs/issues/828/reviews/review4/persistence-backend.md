# Review 4: `layerfs-persistence/src/backend/` (planned layout)

Scope: every planned path under `engine/crates/layerfs-persistence/src/backend/`
(145 plan rows, 24,321 production LOC). Old source read at `S/v2tree` (branch
`layerfs_v2_0_0` @ 1317ebf7ca); LOC are the plan's `loc` values (the
`tools/production_loc.py --files` counts in `S/prod_loc_files.txt`). Crate-root
folders (`storage/`, `gc/`, `history/`, `diagnostic/`, `upgrade/`, `store/`)
belong to the other reviewer; I name them only where a backend file has to
land there or a name must match.

## 1. Verdict

**backend/: shape is good, and most of rule 9 holds.** Every folder is within
3–12 files (SQLite `compact/` has 14 under its granted exception), depth is ≤ 4,
and loose-file counts are within limits (backend root 4, PostgreSQL 4, SQLite 3).
The differences named in the brief are real and each fits one line:

| Difference | Real? | One-line reason (evidence) |
| --- | --- | --- |
| SQLite `lane/`, PostgreSQL `lane.rs` | real | SQLite adds a cross-process lock on the Store file (`writer_admission.rs`, `Writer` used by seal, compact install, upgrade, session) and evidence of other open handles (`exclusion.rs`). PostgreSQL's lane is a single row held `FOR UPDATE`. |
| SQLite `lifecycle/` | real | Only SQLite has a Store file: seal, macOS extents, `(dev, ino)` identity, headroom, page reclamation, `VACUUM INTO` copy, heads read on a connection it opens and closes itself. |
| PostgreSQL `server/` | real | Only a server reports version, latency, schema sizes and role privileges (`status.rs`, `access.rs`). |
| PostgreSQL `history/` | real | One-message pipelined forms of Commit, mount, owner release and binding append (#768), plus statement-text overrides. SQLite runs these statement by statement through the crate's `history/` (`history/fused.rs` composes `hints::record` + `commit_branch` in one `BEGIN IMMEDIATE`). |

The problem is that the PostgreSQL code depends on modules inside `sqlite/`.
The plan keeps several pieces that both backends use inside `sqlite/`, so
PostgreSQL imports `crate::backend::sqlite::…` in 13 places, and `sqlite/`
calls back into `postgresql/` in 7 places. In other places a file with the same
job has a different name in each backend. Findings 1–7 fix this. Findings 8–12
are small renames.

## 2. Findings (ranked by value)

### 1. The backend-neutral compaction API sits in `sqlite/compact/types.rs` → move it to `gc/compact.rs`
- **Evidence.** `sqlite/compact/types.rs` (210) defines the public, backend-neutral
  `store.compact` types: `CompactRequest`, `CompactReceipt`, `CompactPath`,
  `CompactResidue`, `CompactZeroReason`, `CompactKeptWaste`, `CompactFailpoint`,
  `OwnerLiveness`, `DELTA_BASE_REFERENCES_CAP` and `cap_delta_references`. These
  files import them from SQLite:
  - PostgreSQL `compact/{op,run,cleanup,report}.rs`, 17 `use crate::backend::sqlite::compact::Compact…` lines;
  - PostgreSQL `compact/types.rs:51`: `kept_waste: crate::backend::sqlite::compact::CompactKeptWaste`;
  - `compact_metrics.rs:16` (planned `gc/metrics.rs`);
  - `lib.rs:26`, which publishes them as `backend::sqlite::compact::{…}`.
- **Fix.** Split `sqlite/compact/types.rs`. The neutral types go to a new
  `src/gc/compact.rs`, which holds the request and receipt of the `store.compact`
  call, so the file is named after the call (rule 15). It sits beside
  `gc/metrics.rs` and `gc/phases.rs`, which already hold the neutral compaction
  metrics and phase names. SQLite `types.rs` keeps its own items: the artifact
  suffixes, `MarkTuple` and `io_error`/`db_error`/`seal_error`. This puts 4 loose
  files in `gc/`, which is within the limit.
- The two `receipt.rs` files (SQLite `receipt()` and PostgreSQL `report()`) are
  about 85% identical. Their only differences are the zero-reason arms and
  `dead_refined`. Merging them is a code change, not a layout change, so it is
  only noted here.

### 2. The neutral merge-records front sits in `sqlite/merge_records.rs` → `storage/merge_records.rs`, with the same functions in both backends
- **Evidence.** `sqlite/merge_records.rs` (561) holds the engine-neutral merge
  scratch. That covers `MergeScratch`, `MergeRecords`, `MergeRecordsWork`, the
  resident buffer and the write-back buffer, `RESIDENT_BYTES`, `BUFFER_BYTES`,
  `MERGE_LEASE` and the `IndexedEditBacking` impl. It also holds an
  `enum Scratch { Sqlite(..), Postgres(Arc<postgresql::session::connection::Session>) }`
  whose PostgreSQL arms call `crate::backend::postgresql::merge_records::{register,remove,refresh,keys,stored,flush}`
  (lines 125 and 380–534). In the other direction, PostgreSQL imports
  `crate::backend::sqlite::storage::merge_records::Window` (`postgresql/merge_records.rs:17`),
  and `PostgresStorage::merge_scratch` builds `sqlite::…::Scratch::Postgres`
  (`postgresql/storage/provider.rs:52`). The SQLite file's own header points at
  the PostgreSQL file ("only the statements differ").
- **Fix.** Split it:
  - the neutral front (types, buffers, `Window`, the `Scratch` dispatch and the
    content-backing impl) goes to a new `src/storage/merge_records.rs`;
  - `sqlite/merge_records.rs` keeps SQLite's statements as
    `register/remove/refresh/keys/stored/flush`, plus its `sweep`.

  Both backend files then have one shape, and neither imports the other. This
  follows the existing pattern of `storage/metadata/allocation.rs` (neutral)
  beside `sqlite/metadata/allocation.rs`. Both providers expose `merge_scratch()`,
  so the front belongs with the storage port.

### 3. SQLite's `StorageProvider` sits at the crate root, PostgreSQL's in `objects/` → `sqlite/objects/provider.rs`
- **Evidence.** The plan keeps `src/storage/provider.rs` (121) at the crate root
  and moves PostgreSQL's provider to `postgresql/objects/provider.rs` ("pack
  publication and reads"). The crate-root file is SQLite-only:
  - it uses `crate::backend::Session`, and `backend/mod.rs:14` re-exports that
    name from `sqlite::session::connection::Session`;
  - it builds `Scratch::Sqlite`;
  - its `snapshot()` is SQLite's `VACUUM INTO`;
  - it calls `sqlite::objects::{selection,unit_layout}` (lines 82–125).

  So both providers do the same job, but one is at the crate root and the other
  is in a backend folder (rules 9 and 23).
- **Fix.** Move `src/storage/provider.rs` → `src/backend/sqlite/objects/provider.rs`,
  which gives SQLite `objects/` 8 files.
- For the other reviewer:
  - `storage/publication.rs::publish` is a 3-line wrapper over SQLite's
    `publish::run` with SQLite's `Transaction`. Once the provider calls
    `super::publish::run` directly, that wrapper can go, and `publication.rs`
    keeps only the neutral `validate`.
  - `backend/mod.rs` gives SQLite's `Session`, `Transaction`, `publish` and
    `metadata_locations` unqualified `backend::` names. That hides SQLite-only
    uses in `history/fused.rs`, `diagnostic/transaction.rs`, `store/open.rs` and
    `history/lineage_backfill.rs`. This is a code note, not a layout operation.

### 4. PostgreSQL `metadata/` splits by read and write, while SQLite and `storage/metadata/` split by table → split PostgreSQL by table
- **Evidence.** The `Mechanics` trait methods map like this:
  - **SQLite:** `metadata::{policy::read, allocation::{cursor,advance}, locations::{read,after,one,ranked}, values::{read,read_blocks}, signatures::read}` (`sqlite/mechanics.rs:25–86`).
  - **PostgreSQL:** `metadata::{reads::{policy,cursor,locate,locations_after,inode_values,inode_values_ahead,signatures}, writes::{advance,allocate}}` (`postgresql/mechanics.rs:61–163`).

  The neutral `storage/metadata/` uses SQLite's five names. These are the same
  jobs under different file names, which rule 9 forbids.
- **Fix.**
  - Rename `postgresql/metadata/writes.rs` (33) → `allocation.rs`, and move
    `cursor` into it from `reads.rs`.
  - Split `reads.rs` (374):
    - locate and held locations, lines 41–212 → `locations.rs` (the rename target);
    - `inode_values*`, `HeldBlocks` and `push_block` → new `values.rs`;
    - `policy` → new `policy.rs`;
    - `signatures` → new `signatures.rs`.
  - `chains.rs` stays as the one-line difference: PostgreSQL locates delta
    chains in two statements (#758).

  The result is 6 files against SQLite's 5.

### 5. PostgreSQL borrows the acquisition row rules from `sqlite/acquisition/` → a neutral `backend/acquisition.rs`, and a matching `accounting.rs`
- **Evidence.** PostgreSQL `acquisition/{cleanup,provider,reads,writes}.rs` import
  these items from `sqlite::acquisition`:
  - `statements::{Failure, signed, unsigned}`;
  - `metadata_blob`, `metadata_parse` (from `metadata.rs`);
  - `accounting::{entry_bytes, host_bytes, HOST_BYTES, ROOT_BYTES}`;
  - `writes::{packed, well_formed}`.

  These are the rules for the acquisition rows that both backends share ("the
  same three tables", per `postgresql/acquisition/mod.rs`). Two names also clash:
  - SQLite `acquisition/metadata.rs` (the init_entry blob) has the same name as
    its sibling folder `sqlite/metadata/`, which is Store Metadata;
  - `backend/epoch.rs` (a session's acquisition epoch, used only by the two
    acquisition providers) has the same name as the crate's `gc/epoch.rs`
    (`project_epoch` writer fence), which is a different concept.
- **Fix.**
  - Rename `backend/epoch.rs` → `backend/acquisition.rs` ("acquisition state and
    row rules shared by both engines").
  - Merge `sqlite/acquisition/metadata.rs` into it.
  - Move into it from SQLite: `Failure`, `signed` and `unsigned` from
    `statements.rs`; the byte constants and charges from `accounting.rs`; and
    `packed` and `well_formed` from `writes.rs`.

  SQLite `accounting.rs` then holds owner, charge and credit only. To match it,
  split PostgreSQL `acquisition/writes.rs` (499): `identity`, `owner`, `adjust`,
  `charge` and `credit` (lines 49–114, "the owner check and exact charges every
  unit shares") go to a new PostgreSQL `accounting.rs`. Both backends then have
  `accounting, cleanup, provider, reads, writes`. SQLite also keeps
  `statements.rs` (its closed SQL set) and `entry_windows.rs`/`root_windows.rs`
  (fixed-input windows); PostgreSQL uses `unnest` arrays instead.

### 6. SQLite's history statement table is hidden in `session/query.rs` → `sqlite/history.rs`
- **Evidence.** Lines 7–360 of `sqlite/session/query.rs` (425) are `history(key)`,
  which maps every shipped history statement name to its `include_str!` text.
  Only `run`, `borrowed`, `mapped` and `batch` (about 60 lines) are execution.
  PostgreSQL has the same job in `postgresql/history/statements.rs`, and falls
  back to SQLite's table (`statements.rs:43`). The crate's
  `diagnostic/check/mounts.rs:79` also calls it. A reader cannot predict that
  `session/query.rs` holds the statement table.
- **Fix.** Split it. `history()` goes to a new loose file `sqlite/history.rs`
  ("shipped history statement texts by key"), which gives SQLite 4 loose files.
  `session/query.rs` keeps measured execution. The one-line difference then
  reads: SQLite has `history.rs`, the shipped texts, which are portable;
  PostgreSQL has `history/`, which holds its overrides and the one-message forms.

### 7. `diagnostic/` and `server/`: same name for the same job, and probe/step in the right place
- **Evidence.**
  - **Same job, different names.** SQLite `diagnostic/engine.rs` (from
    `diagnostic/sqlite.rs`) and PostgreSQL `diagnostic/audit.rs` (from
    `diagnostic/postgresql.rs`) do the same job: each is
    `impl {Sqlite,Postgres}Handles { diagnose, diagnose_project }`. Each
    backend's `compact/run.rs` and `upgrade/run.rs` already name that kind of
    "orchestration on the Handles" `run.rs`.
  - **`audit.rs` holds two things.** Lines 14–211 are the doctor's connection
    check (`connection`, `connection_queries`, published as
    `postgresql_connection`), separate from `diagnose`.
  - **`server/probe.rs` is in the wrong folder.** It drives
    `objects::read::{expect,scoped,ranges}` and
    `metadata::reads::inode_values_ahead`. Nothing in it reports anything about
    the server; the plan itself calls server/ the "least-wrong product home".
  - **`diagnostic/step.rs` is about server reachability, not a check.** It
    defines `ConnectionStep`, the variant inside `PostgresStatusError`
    (`server/status.rs:28–65`). `session/connection.rs:297–312` uses it when
    classifying refused dials. That makes it part of reaching the server.
- **Fix.**
  - Rename SQLite `diagnostic/engine.rs` → `run.rs`.
  - Split PostgreSQL `diagnostic/audit.rs`: `run.rs` keeps `diagnose` and
    `with_cancellation`; a new `connection.rs` takes the connection check.
  - Move PostgreSQL `diagnostic/step.rs` → `server/reach.rs`. The new name avoids
    a clash with the `upgrade/step.rs` files in each backend, and the doc already
    says "a reach refusal names them".
  - Move `server/probe.rs` → `objects/probe.rs`.

  Result:
  - `server/`: access, reach, status (3). It still exists, as settled.
  - PostgreSQL `diagnostic/`: connection, contract, expected, run, schema.
  - SQLite `diagnostic/`: run, schema, structure.

  Both backends share `run.rs` and `schema.rs`. The public
  `diagnostic::ConnectionStep` path can stay as a re-export.

### 8. `backend/records.rs` is a vague name that clashes with other "records" → `error.rs`, with the value types moved to `seam.rs`
- **Evidence.** The header says "Owned bounded scalar inputs/results". The file
  holds `Param`, `Value`, `Record`, `FromValue` and `pack_info` (the values that
  cross the seam) as well as `BackendError`, its refusal texts,
  `outdated_store`/`upgradable_store`, the `From` impls and `OutcomeError`. The
  name "records" clashes with:
  - `merge_records.rs` in both backends;
  - `gc/spill/records.rs` and `history/lineage/records.rs`;
  - `layerfs_history::records`, which PostgreSQL `compact/*.rs` import in the
    same files as `backend::records::BackendError`.
- **Fix.**
  - Move `Param`, `Value`, `Record`, `FromValue` and `pack_info` into `seam.rs`.
    `seam.rs` already imports them, and they are the seam's own values.
  - Rename what is left to `backend/error.rs`.

  The backend root stays at 4 loose files: acquisition, admission, error, seam.
  This changes import paths in about 81 files mechanically, so it ranks low.

### 9. PostgreSQL `session/query.rs` binds parameters, which is not what SQLite's `query.rs` does → merge it into `rows.rs`
- **Evidence.** PostgreSQL `session/query.rs` (31) binds `Param` to declared types
  ("Engine-neutral parameters bound…"). SQLite does the same job in
  `session/rows.rs:9–13`. SQLite `query.rs` is measured execution. This is the
  same file name for different jobs.
- **Fix.** Merge PostgreSQL `query.rs` into `session/rows.rs` (68 → about 99 LOC;
  "rows and parameters in and out"). After that, both backends share
  `connection`, `profile`, `rows` and `transaction`.

### 10. PostgreSQL `compact/receipt.rs` also holds the gc_op row's wire form → move that part into `op.rs`
- `encode_kept_waste`/`decode_kept_waste` (about 50 lines) are used only by
  `postgresql/compact/op.rs:3–4, 144, 398`, for the `gc_op` row. Move them into
  `op.rs`. `receipt.rs` then holds only `receipt()`, like SQLite's, which
  completes the planned rename from `report.rs`.

### 11. (Optional) SQLite `objects/` `unit_` family: either keep the exception, or merge to drop it
- **Confirmed.** The exception is valid. A `units/` folder would be 5 levels
  deep, and the four files are only loosely coupled.
- **Improvement, if the owner wants the exception gone.** Each move below is a
  whole-file move, and each file has a single caller:
  - `units_publish.rs` (35) is called only from `publish.rs:11` → merge into
    `publish.rs`;
  - `unit_io.rs` (70) uses `units_read::Unit` and is called only from
    `units_read.rs:96` → merge into `units_read.rs`, then rename that file
    `units.rs` ("pack offsets over unit BLOBs");
  - `unit_layout.rs` → `layout.rs` (the `SqlitePackLayout` views, costs and batch
    check).

  The folder then has layout, provider, publish, read, selection and units:
  6 files, no prefix family, no exception. If the owner prefers to keep
  4 files, the smallest fix is the consistent prefix `unit_publish.rs` and
  `unit_read.rs`. Today the folder mixes `unit_` and `units_`.

### 12. SQLite `session/background.rs` is a word that fits anything → `checkpointer.rs`
- The header says it is "A Normal Store's WAL checkpoints, off the commit path …
  wakes this session's checkpoint thread". It sits beside `checkpoint.rs`, the
  idle checkpoint. `checkpointer.rs` says what the file is: the checkpoint
  thread.

## 3. Proposed layout (changed folders)

Scope: `engine/crates/layerfs-persistence/src/`, plan LOC at 1317ebf7ca
(`tools/production_loc.py --files`). Folders I don't change are collapsed to
one line, and their subtotals cover only those lines. Crate-root folders are
partial: they show only the files this review touches.

```diff
  backend/                                [24,442]
    admission.rs                             121
    acquisition.rs                            39   (was epoch.rs; gains sqlite acquisition/metadata.rs + Failure/signed/unsigned, byte charges, packed/well_formed)
-   epoch.rs                                  —   (renamed acquisition.rs)
    error.rs                                 214   (was records.rs; Param/Value/Record/FromValue/pack_info → seam.rs)
    mod.rs                                    12
    seam.rs                                  188   (gains the seam's value types)
    postgresql/                           [11,830]
      lane.rs                                199
      mechanics.rs                           160
      merge_records.rs                       194   (no sqlite import: Window from storage/merge_records.rs)
      mod.rs                                  15
      schema.rs                              304
      acquisition/                        [1,135]
+       accounting.rs                          —   new (owner/charge/credit from writes.rs; same name as SQLite)
        cleanup.rs                           151
        mod.rs                                 5
        provider.rs                          212
        reads.rs                             268
        writes.rs                            499   (split: accounting.rs)
      compact/                            [2,469]
        build.rs                             310
        cleanup.rs                           583
        mod.rs                                11
        op.rs                                373   (gains the kept-waste wire form)
        receipt.rs                           151   (split: encode/decode_kept_waste → op.rs)
        run.rs                               273
        swap.rs                              537
        sync.rs                              190
        types.rs                              41
      diagnostic/                         [1,013]
+       connection.rs                          —   new (doctor's connection check, from audit.rs)
        contract.rs                           82
        expected.rs                          219
        mod.rs                                 —
        run.rs                               439   (was audit.rs; same name as SQLite run.rs)
        schema.rs                            273
-       step.rs                               41   (moved: server/reach.rs)
      history/                            [1,051]  (unchanged, not expanded)
      metadata/                             [693]
        allocation.rs                         33   (was writes.rs; gains cursor)
        chains.rs                            283
        locations.rs                         374   (was reads.rs; keeps locate/held locations)
        mod.rs                                 3
+       policy.rs                              —   new (from reads.rs)
+       signatures.rs                          —   new (from reads.rs)
+       values.rs                              —   new (inode values, from reads.rs)
      objects/                            [1,119]
        mod.rs                                 3
        probe.rs                              56   (moved from server/probe.rs)
        provider.rs                          332
        publish.rs                           151
        publish_flight.rs                    141
        read.rs                              436
      server/                               [376]
        access.rs                            109
        mod.rs                                 —
        reach.rs                              41   (moved from diagnostic/step.rs; ConnectionStep)
        status.rs                            226
      session/                            [2,561]
        connection.rs                        432
        driver.rs                            648
        mod.rs                                 9
        pipeline.rs                          312
        prepare.rs                            35
        profile.rs                           112
-       query.rs                              31   (merged into rows.rs)
        reopen.rs                            305
        rows.rs                               68   (gains Param binding)
        transaction.rs                       609
      upgrade/                              [541]  (unchanged, not expanded)
    sqlite/                               [12,038]
+     history.rs                               —   new (history statement texts, from session/query.rs)
      mechanics.rs                           125
      merge_records.rs                       561   (split: neutral front → storage/merge_records.rs)
      mod.rs                                  16
      schema.rs                              509
      acquisition/                        [1,512]
        accounting.rs                         74   (split: byte charges → backend/acquisition.rs)
        cleanup.rs                           190
        entry_windows.rs                     236
-       metadata.rs                           41   (merged into backend/acquisition.rs)
        mod.rs                                11
        provider.rs                          231
        reads.rs                             364
        root_windows.rs                       56
        statements.rs                         86   (split: Failure/signed/unsigned → backend/acquisition.rs)
        writes.rs                            223   (split: packed/well_formed → backend/acquisition.rs)
      compact/                            [3,661]  (14 files, exception confirmed)
        types.rs                             210   (split: neutral API → gc/compact.rs; other 13 files unchanged, partial)
      diagnostic/                           [522]
        mod.rs                                 —
        run.rs                               156   (was engine.rs)
        schema.rs                            255
        structure.rs                         111
      lane/                                 [250]  (unchanged, not expanded)
      lifecycle/                            [678]  (unchanged, not expanded)
      metadata/                             [415]  (unchanged, not expanded)
      objects/                              [857]
        mod.rs                                 7
        provider.rs                          121   (moved from src/storage/provider.rs)
        publish.rs                           136
        read.rs                              238
        selection.rs                          83
        unit_io.rs                            70   (optional: merge into units_read.rs)
        unit_layout.rs                        37   (optional: rename layout.rs)
        units_publish.rs                      35   (optional: merge into publish.rs)
        units_read.rs                        130   (optional: rename units.rs)
      session/                            [1,971]
        checkpointer.rs                      115   (was background.rs)
        checkpoint.rs                         88
        close.rs                              74
        connection.rs                        354
        mod.rs                                11
        prepared.rs                          196
        profile.rs                           113
        query.rs                             425   (split: history() → sqlite/history.rs)
        rows.rs                               57
        statement_work.rs                     33
        thread_work.rs                       134
        transaction.rs                       371
      upgrade/                              [961]  (unchanged, not expanded)
  gc/                                         [0]  (partial: crate root, other reviewer)
+   compact.rs                                 —   new (store.compact request/receipt types, from sqlite/compact/types.rs)
  storage/                                  [121]  (partial: crate root, other reviewer)
+   merge_records.rs                           —   new (neutral merge scratch, from sqlite/merge_records.rs)
-   provider.rs                              121   (moved: backend/sqlite/objects/provider.rs)
```

## 4. Checked and left alone

- **Loose root files.**
  - Both backends keep `mechanics.rs` (the `Mechanics` impl), `schema.rs` and
    `merge_records.rs` under the same names.
  - The settled loose PostgreSQL `lane.rs` corresponds to SQLite's
    `lane/publication.rs`.
  - `exclusion.rs` stays in `lane/`. Its only caller is `compact/run.rs:133`,
    and it is evidence about writer exclusion.
- **SQLite `lifecycle/` (8 files).** `heads.rs` is a read, but its whole concern
  is holding the writer lane around its own open and close (#776 C3/C4), which
  makes it file lifecycle. The owner's rule 9 uses the word "lifecycle".
- **SQLite `compact/` at 14 files.** The exception is confirmed. Any sub-split
  would be 5 levels deep. `visit.rs` belongs in `compact/`: it is the
  `.compact-visited` artifact the build attaches. It is the mark's sink at
  `history/objects.rs:270,312`.
- **`compact/` names.** `build`, `op`, `receipt`, `run` and `types` are shared.
  PostgreSQL `swap`/`cleanup`/`sync` and SQLite
  `install`/`verify`/`staged`/`repack`/`values`/`window`/`lengths`/`space` are
  different algorithms: an in-place swap against a rebuild-and-rename.
  `types.rs` is a generic name, but both backends use it, so I left it.
- **`upgrade/`.** `frozen`, `run` and `step` are shared. PostgreSQL `check.rs`
  is message 1, the "check" phase. SQLite `format.rs` reads the header and
  `verify.rs` holds the checks before COMMIT. The phase lists in
  `upgrade/types.rs` are different, so the names are justified.
- **PostgreSQL `session/`.** `driver`, `pipeline` and `reopen` are PostgreSQL
  only. `prepare.rs` is named after `Session::prepare`. It is close to SQLite's
  `prepared.rs` (a cached statement lease), but the jobs differ and both names
  are accurate.
- **SQLite `session/`.** `statement_work` and `thread_work` are two `_work`
  files, below the prefix rule. `close.rs` (the planned rename) is fine.
- **PostgreSQL `history/`.** The name matches the seam trait
  `HistoryStatements`. `mount.rs` and `release.rs` serve the crate's
  `gc/admission/`, but two files are not enough for their own folder.
- **`backend/admission.rs`.** AGENTS.md calls this "the in-process admission
  queue", so the name is the project's own word. The crate-root `gc/admission/`
  (GC intents) is the one that overloads it; that is the other reviewer's call.
- **Crate-root name matches the other reviewer should know about.**
  - Backend `diagnostic/`, `upgrade/` and `metadata/` match the crate's neutral
    `diagnostic/`, `upgrade/` and `storage/metadata/`. Finding 4 makes the
    PostgreSQL `metadata/` file names match too.
  - Findings 1–3 put `gc/compact.rs` and `storage/merge_records.rs` at the crate
    root and remove `storage/provider.rs` from it.
  - Finding 5 removes the clash between `backend/epoch.rs` and `gc/epoch.rs`.
- **Tests, examples and `sql/`.** These are outside `src/backend/` and belong to
  the other reviewer. `sql/postgresql/history/` already matches
  `backend/postgresql/history/`.

## 5. Machine-readable list

```json
[
{"from":"engine/crates/layerfs-persistence/src/backend/sqlite/compact/types.rs","to":"engine/crates/layerfs-persistence/src/backend/sqlite/compact/types.rs","op":"split","why":"neutral store.compact API (Request/Receipt/Path/Residue/ZeroReason/KeptWaste/Failpoint) leaves sqlite; PostgreSQL imports 17 of them"},
{"from":"engine/crates/layerfs-persistence/src/backend/sqlite/compact/types.rs","to":"engine/crates/layerfs-persistence/src/gc/compact.rs","op":"new","why":"store.compact request/receipt types shared by both backends, beside gc/metrics.rs and gc/phases.rs"},
{"from":"engine/crates/layerfs-persistence/src/backend/sqlite/merge_records.rs","to":"engine/crates/layerfs-persistence/src/backend/sqlite/merge_records.rs","op":"split","why":"keep SQLite statements as register/remove/refresh/keys/stored/flush like PostgreSQL's; drop the sqlite<->postgresql cross-imports"},
{"from":"engine/crates/layerfs-persistence/src/backend/sqlite/merge_records.rs","to":"engine/crates/layerfs-persistence/src/storage/merge_records.rs","op":"new","why":"neutral MergeScratch/MergeRecords, buffers, Window and engine dispatch used by both providers"},
{"from":"engine/crates/layerfs-persistence/src/storage/provider.rs","to":"engine/crates/layerfs-persistence/src/backend/sqlite/objects/provider.rs","op":"move","why":"SQLite-only StorageProvider (sqlite Session, VACUUM INTO, unit_layout); pairs with postgresql/objects/provider.rs"},
{"from":"engine/crates/layerfs-persistence/src/backend/postgresql/metadata/writes.rs","to":"engine/crates/layerfs-persistence/src/backend/postgresql/metadata/allocation.rs","op":"rename","why":"allocation cursor; same file name as sqlite/metadata and storage/metadata (gains cursor from reads.rs)"},
{"from":"engine/crates/layerfs-persistence/src/backend/postgresql/metadata/reads.rs","to":"engine/crates/layerfs-persistence/src/backend/postgresql/metadata/locations.rs","op":"split","why":"split by table like SQLite; locate and held locations stay"},
{"from":"engine/crates/layerfs-persistence/src/backend/postgresql/metadata/reads.rs","to":"engine/crates/layerfs-persistence/src/backend/postgresql/metadata/values.rs","op":"new","why":"inode value reads and read-ahead, as sqlite/metadata/values.rs"},
{"from":"engine/crates/layerfs-persistence/src/backend/postgresql/metadata/reads.rs","to":"engine/crates/layerfs-persistence/src/backend/postgresql/metadata/policy.rs","op":"new","why":"policy read, as sqlite/metadata/policy.rs"},
{"from":"engine/crates/layerfs-persistence/src/backend/postgresql/metadata/reads.rs","to":"engine/crates/layerfs-persistence/src/backend/postgresql/metadata/signatures.rs","op":"new","why":"signature ring read, as sqlite/metadata/signatures.rs"},
{"from":"engine/crates/layerfs-persistence/src/backend/epoch.rs","to":"engine/crates/layerfs-persistence/src/backend/acquisition.rs","op":"rename","why":"acquisition epoch plus the shared acquisition row rules; ends the clash with gc/epoch.rs"},
{"from":"engine/crates/layerfs-persistence/src/backend/sqlite/acquisition/metadata.rs","to":"engine/crates/layerfs-persistence/src/backend/acquisition.rs","op":"merge","why":"init_entry blob used by both backends; name clashed with sqlite/metadata/"},
{"from":"engine/crates/layerfs-persistence/src/backend/sqlite/acquisition/statements.rs","to":"engine/crates/layerfs-persistence/src/backend/sqlite/acquisition/statements.rs","op":"split","why":"Failure/signed/unsigned used by PostgreSQL move to backend/acquisition.rs; closed SQLite statement set stays"},
{"from":"engine/crates/layerfs-persistence/src/backend/sqlite/acquisition/accounting.rs","to":"engine/crates/layerfs-persistence/src/backend/sqlite/acquisition/accounting.rs","op":"split","why":"ROOT_BYTES/HOST_BYTES/entry_bytes/host_bytes move to backend/acquisition.rs; owner/charge/credit stay"},
{"from":"engine/crates/layerfs-persistence/src/backend/sqlite/acquisition/writes.rs","to":"engine/crates/layerfs-persistence/src/backend/sqlite/acquisition/writes.rs","op":"split","why":"packed/well_formed used by PostgreSQL move to backend/acquisition.rs"},
{"from":"engine/crates/layerfs-persistence/src/backend/postgresql/acquisition/writes.rs","to":"engine/crates/layerfs-persistence/src/backend/postgresql/acquisition/writes.rs","op":"split","why":"owner check and charges get their own file, as in SQLite"},
{"from":"engine/crates/layerfs-persistence/src/backend/postgresql/acquisition/writes.rs","to":"engine/crates/layerfs-persistence/src/backend/postgresql/acquisition/accounting.rs","op":"new","why":"identity/owner/adjust/charge/credit; same name and job as sqlite/acquisition/accounting.rs"},
{"from":"engine/crates/layerfs-persistence/src/backend/sqlite/session/query.rs","to":"engine/crates/layerfs-persistence/src/backend/sqlite/session/query.rs","op":"split","why":"history statement table (lines 7-360) leaves; measured execution stays"},
{"from":"engine/crates/layerfs-persistence/src/backend/sqlite/session/query.rs","to":"engine/crates/layerfs-persistence/src/backend/sqlite/history.rs","op":"new","why":"shipped history statement texts by key; PostgreSQL history/statements.rs falls back to it"},
{"from":"engine/crates/layerfs-persistence/src/backend/sqlite/diagnostic/engine.rs","to":"engine/crates/layerfs-persistence/src/backend/sqlite/diagnostic/run.rs","op":"rename","why":"SqliteHandles diagnose run; same name as PostgreSQL and as compact/run.rs, upgrade/run.rs"},
{"from":"engine/crates/layerfs-persistence/src/backend/postgresql/diagnostic/audit.rs","to":"engine/crates/layerfs-persistence/src/backend/postgresql/diagnostic/run.rs","op":"split","why":"PostgresHandles diagnose run; same name as SQLite"},
{"from":"engine/crates/layerfs-persistence/src/backend/postgresql/diagnostic/audit.rs","to":"engine/crates/layerfs-persistence/src/backend/postgresql/diagnostic/connection.rs","op":"new","why":"the doctor's connection check (postgresql_connection), separate from diagnose"},
{"from":"engine/crates/layerfs-persistence/src/backend/postgresql/diagnostic/step.rs","to":"engine/crates/layerfs-persistence/src/backend/postgresql/server/reach.rs","op":"move","why":"ConnectionStep is part of PostgresStatusError and of dial classification; avoids clash with upgrade/step.rs"},
{"from":"engine/crates/layerfs-persistence/src/backend/postgresql/server/probe.rs","to":"engine/crates/layerfs-persistence/src/backend/postgresql/objects/probe.rs","op":"move","why":"test driver over objects::read and inode value reads; reports nothing about the server"},
{"from":"engine/crates/layerfs-persistence/src/backend/records.rs","to":"engine/crates/layerfs-persistence/src/backend/error.rs","op":"split","why":"BackendError and its refusal texts; 'records' clashes with merge_records, spill/lineage records, layerfs_history::records"},
{"from":"engine/crates/layerfs-persistence/src/backend/records.rs","to":"engine/crates/layerfs-persistence/src/backend/seam.rs","op":"merge","why":"Param/Value/Record/FromValue/pack_info are the seam's values; keeps backend root at 4 loose files"},
{"from":"engine/crates/layerfs-persistence/src/backend/postgresql/session/query.rs","to":"engine/crates/layerfs-persistence/src/backend/postgresql/session/rows.rs","op":"merge","why":"Param binding lives in rows.rs on SQLite; SQLite query.rs is a different job"},
{"from":"engine/crates/layerfs-persistence/src/backend/postgresql/compact/receipt.rs","to":"engine/crates/layerfs-persistence/src/backend/postgresql/compact/op.rs","op":"split","why":"encode/decode_kept_waste are the gc_op row's wire form, used only by op.rs; receipt.rs then matches SQLite"},
{"from":"engine/crates/layerfs-persistence/src/backend/sqlite/session/background.rs","to":"engine/crates/layerfs-persistence/src/backend/sqlite/session/checkpointer.rs","op":"rename","why":"the WAL checkpoint thread; 'background' fits anything"},
{"from":"engine/crates/layerfs-persistence/src/backend/sqlite/objects/units_publish.rs","to":"engine/crates/layerfs-persistence/src/backend/sqlite/objects/publish.rs","op":"merge","why":"optional: only caller publish.rs:11; removes the unit_ exception"},
{"from":"engine/crates/layerfs-persistence/src/backend/sqlite/objects/unit_io.rs","to":"engine/crates/layerfs-persistence/src/backend/sqlite/objects/units.rs","op":"merge","why":"optional: only caller units_read.rs, uses its Unit type"},
{"from":"engine/crates/layerfs-persistence/src/backend/sqlite/objects/units_read.rs","to":"engine/crates/layerfs-persistence/src/backend/sqlite/objects/units.rs","op":"rename","why":"optional: pack offsets over unit BLOBs, without the mixed unit_/units_ prefix"},
{"from":"engine/crates/layerfs-persistence/src/backend/sqlite/objects/unit_layout.rs","to":"engine/crates/layerfs-persistence/src/backend/sqlite/objects/layout.rs","op":"rename","why":"optional: SqlitePackLayout views, costs and batch check"}
]
```
