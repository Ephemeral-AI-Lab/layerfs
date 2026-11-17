# #42 decisions: GC 2.0 contract amendments and SQLite implementation deviations

> **Status:** Proposal for layerfs-project#42 owner sign-off; not a released
> contract. The frozen [`LayerFS-GC-spec.md`](LayerFS-GC-spec.md) (frozen for
> layerfs-project#41 on 2026-11-15, baseline `ed14ed53b`) is **byte-identical**
> on this branch: nothing here is written into it. On sign-off the accepted
> entries become spec text (or a new #42 decision entry), the spec's decision
> table indexes them, and this file is retired.

**Why.** The #41 implementation and the #711 fix-branch integration need a
different reading than the frozen sentences in sixteen places: eleven contract
amendments (A1–A11, mostly refusal detail shapes, one additive sidecar field,
seven `gc_op` columns, the snake_case rendering rule, the one-valued
`deadBytesRefined` and the §10 in-process `Busy` mapping) and five SQLite
implementation deviations (D1–D5) that the
frozen text either forbids or does not describe. `gc-spec-gaps.md` (one of the implementation notes moved out of
the tree; its decisions are folded into these entries where they carry contract
text) already flagged two of the earlier seven as "#42 correction"; this note is the
complete list, and it changes no product behaviour.

The seven A entries before A8 were written directly into the frozen file
while #41 was implemented and were backed out before review (commit
`122a168502`); A8–A12 are added by this note for the same sign-off.
The five D entries are present in the shipped SQLite code and are recorded here
so the contract and the implementation do not drift apart.

G7 records one **deferred design decision** (the install-only lane). It
continues the retired G1–G6 series, changes no contract text and keeps the
frozen whole-lane exclusion; its measurements live under
`tools/bench/installlane/`.

---

## A. Contract amendments (12) and one reading

### A1. §4.1 `project.delete` sidecar gains `op_id` (additive)

- **Frozen:** `<home>/store/<store>/delete/<project>.json` is
  `{store, project, stack_id, deleted_ms, attempt_ms, state}`.
- **Proposed:** add `op_id`, the project op the admit/mark attempt ran under.
  An older sidecar without `op_id` stays valid. The retry then settles a lost
  admit/mark reply instead of leaving it `Unknown` (§5.1, §10).
- **Reason:** the retry cannot otherwise name the op whose reply was lost.

### A2. §8 `gc_op` gains the `usename` column

- **Frozen:** `gc_op` records `artifact, pid, backend_start, incarnation`.
- **Proposed:** also record `usename`, so the lost-COMMIT recovery statement
  can terminate the owner's backend by `(pid, backend_start, usename, datname)`
  and not merely by pid/backend_start.
- **Reason:** `pg_terminate_backend` recovery in §8 matches all four fields.

### A3. §4.3 merge whiteout refusal detail is explicit

- **Frozen:** a merge that cannot materialize the whiteout is refused
  `Removed` with `detail {paths: [...]}`.
- **Proposed:** `detail {paths: [{path, frontier_serial}]}`, listing **every**
  unmaterializable path as a full element, deduplicated and sorted by
  normalized path bytes.
- **Reason:** a merge can find several paths at once; the client needs the
  exact shape and a stable order.

### A4. §4.3 `Removed` refusal detail is one shape on every call

- **Frozen:** "Every refusal carries one detail shape: `{paths: [{path,
  frontier_serial}]}` (one element for read/stat; the governing maximum
  frontier and the request path)."
- **Proposed:** a `Removed` refusal always carries that one shape. For
  `file.read`/`file.stat` it is one element: the request path with its
  governing maximum frontier. A merge lists every unmaterializable path,
  sorted by normalized path bytes.
- **Reason:** makes the one-element vs many-element cases explicit instead of
  parenthetical.

### A5. §4.3 hidden-path read surface: `cp`/`edit` vs `mv`/`rm`

- **Frozen:** "`project.export` omits it; a cp-from source is `Removed`;
  `file.edit` is `Removed`; `file.mv` from a hidden path is `NotFound` and to a
  hidden path is a visible re-creation; `mkdir`/`cp`/`apply` follow the same
  construction rule; `file.rm` of a hidden path is `NotFound`."
- **Proposed:** "`project.export` omits it; `file.cp` from a hidden source and
  `file.edit` are `Removed`; `file.mv`/`file.rm` from a hidden path return the
  `NotFound`/`NoSuchPath` wire code, not `Removed`; `file.mv` to a hidden path
  is a visible re-creation; `mkdir`/`apply` follow the same construction rule."
- **Reason:** the wire code for a hidden `mv`/`rm` source is `NotFound` /
  `NoSuchPath`; naming `cp` in the construction list contradicted its own
  `Removed` sentence.

### A6. §10 code table gains `Unknown`

- **Frozen:** §10's table has no `Unknown` row, although §4/§4.2 use it.
- **Proposed:** `Unknown` is a service/control-plane outcome, not a
  `HistoryError` code: a lost reply, or a Store listing that cannot be
  determined. Detail `{entity: {kind, name}, phase, op}`, with `phase`/`op`
  `null` when no durable op is known.
- **Reason:** callers must be able to distinguish a lost reply from a refusal;
  the row makes the existing §4 use explicit.

### A7. §10/§11: §11 names are logical; wire/SDK/CLI keys are snake_case

- **Frozen:** "The SDK/CLI render every key snake_case."
- **Proposed:** the §11 names (e.g. `deadBytesBound`,
  `keptWaste.deadInRetainedPacksBytes`) are logical Rust names only; the wire,
  SDK and CLI keys are snake_case (`dead_bytes_bound`,
  `kept_waste.dead_in_retained_packs_bytes`), per §10.
- **Reason:** the single sentence did not say which names are logical and
  which are rendered; examples remove the ambiguity.

---

### A8. §11 report: canonical `logicalDiscarded`, source-Store `backendReusable`, two more zero reasons, and the pre-release `gc_op` columns

- **Frozen:** §11 defines `logicalDiscarded` as "the collected history rows'
  `logical` bytes (the `store_totals.logical` delta)"; `backendReusable` as
  "SQLite: rebuild = the new artifact's `freelist_count * page_size` before
  the rename (normally 0), none = the live Store's; PostgreSQL: 0"; and the
  zero vocabulary as `empty-store`, `none-path`, `no-history`,
  `no-waste`, `postgresql-in-place`.
- **Proposed:**
  - `logicalDiscarded` is the **canonical bytes of the collected objects**,
    deduplicated by object id: a tree shared by several collected roots counts
    once, and a collected object that no history row reaches (an orphan)
    still counts. SQLite reports `tuple.canonical - built.canonical` after
    the install; PostgreSQL reports `swapped.dead.canonical`. It is not the
    `store_totals.logical` delta and not a sum over history rows, which
    double-counts shared trees and misses an orphan.
  - `backendReusable` is the **source Store's pre-mark free-page bytes**:
    `freelist_count * page_size`, sampled before the mark, on a rebuild. That
    is the space the run judges and reclaims; the freshly vacuumed target is
    empty by construction and names nothing. The `none` path keeps the live
    Store's count, and PostgreSQL stays 0 (`postgresql-in-place`).
  - the §11 zero vocabulary gains two words: `no-free-pages` (a SQLite rebuild
    reports `backendReusable = 0`, so the source Store had no reusable free
    pages) and `no-shrink` (a rebuild collected dead bytes but
    `physicalShrink = 0`, as a resumed `G + 1` run can). Neither is
    explained as `no-waste`.
- **Schema (pre-release):** PostgreSQL's `gc_op` also carries seven columns.
  The mark (Phase M, `persist_mark`) writes `op_id BYTEA` (the Store
  compaction op the round runs under — `GcIntentKind::Compaction` with
  `GcScopeKind::Store` in `.../postgresql/compact/run.rs` — so a retry that
  loses its reply can name that op),
  `mark_fingerprint TEXT` (the `pack`/`object_location`
  `pg_attribute`/storage/tablespace fingerprint Phase B re-checks against the
  scratch tables), `mark_visited BIGINT` (the mark's visited-object count, the
  exact retained set Phase C asserts) and `kept_waste BYTEA` (the retained
  waste the mark counts, encoded as below). The swap writes
  `physical_before BIGINT NOT NULL DEFAULT 0`,
  `physical_after BIGINT NOT NULL DEFAULT 0` and
  `peak_temp BIGINT NOT NULL DEFAULT 0`, so a resumed rebuild reports the
  swap's measured shrink (the before size sampled before the swap
  transaction opens, the after size sampled inside it), peak temp and the
  mark's retained waste instead of zero placeholders; a save between the swap
  and Phase C cannot inflate the resumed shrink. 2.0.0 is
  unreleased, so the columns go into the 2.0 schema creation in place: there
  is **no additive open-time step** (`add_*`) and no compatibility arm for a
  2.0 `gc_op` created before them (the Disposable profile, §2, and item 42).
  `kept_waste` is a versioned
  encoding of the §11 `keptWaste`: one version byte (1), three big-endian
  `u64` byte totals, the capped flag, a big-endian `u32` reference count
  and the identity bytes.
- **Reason:** the frozen sentences describe history-row accounting as object
  accounting (a shared tree is counted twice and an orphan is missed) and
  name a target artifact that is empty by construction, so the report never
  names the free space a rebuild reclaimed. The two zero words keep a zero
  honest where `no-waste` conflated "nothing was collected" with "the size
  did not change"; the `gc_op` columns are what let a resumed rebuild finish
  the report from the swap's own measurements.
- **Implementation:** the #711 report fix (`fix711-report2`, head
  `129c4b59c7`) carries the code: the canonical `logicalDiscarded` on both
  backends, the source-Store `backendReusable` sample, the two zero words and
  the `gc_op` columns.
- **Where:** `engine/crates/layerfs-persistence/src/report.rs` (the shared
  `CompactReport`), `.../backend/sqlite/compact/{run,receipt}.rs`,
  `.../backend/postgresql/compact/{run,report}.rs` and
  `engine/crates/layerfs-persistence/sql/postgresql/gc.sql`. The wire keys
  stay snake_case (`logical_discarded`, `backend_reusable`, `zero_reasons`)
  under A7.

### A9. §4.2: the retry's Stack read is the marker plus the filtered by-id lookup

- **Frozen:** §4.2 allow-lists `layerstack_stack_by_id_retired` as "the retry's
  live/absent read by the caller's recorded stack id", and keeps the ordinary
  `layerstack_stack_by_id` filtered.
- **Proposed:** no statement of that name is shipped. The `project.delete`
  same-home retry resolves the caller's recorded Stack id with two statements
  that already exist: `layerstack_deleted_ms` (the marker's `deleted_ms`, or
  none) and the filtered `layerstack_stack_by_id` (the live row, or absent). A
  retired Stack reuses its marker's `deleted_ms`, a live Stack redoes the
  marker, and an absent one finishes the entry. No unfiltered by-id statement
  is needed: the filtered read is the live/absent half the frozen sentence
  names, and a retired or deleted project must not resolve there.
- **Reason:** the frozen name describes one statement that the shipped code
  splits into two. The split keeps every ordinary Stack lookup filtered and
  returns the same retired/live/absent classification the frozen sentence
  describes, so no behaviour changes and no resurrection path is added.
- **Alternative recorded, not adopted:** add a dedicated
  `layerstack_stack_by_id_retired` statement. It duplicates
  `layerstack_stack_by_id`'s row shape only to return the same absence the
  filtered read already returns, and it would need its own allow-list
  justification.
- **Where:** `engine/crates/layerfs-persistence/src/history/admission/removal.rs`
  (`delete_stack`), `.../src/history/layerstack.rs` (`stack_deleted_ms`,
  `layer_stack`), and `control/crates/layerfs-store/src/project/delete.rs`
  (the retry path).
  `engine/crates/layerfs-persistence/tests/visibility_guard.rs` pins the two
  statements and the statement set.

### A10. §6.4/§11: `deadBytesRefined` equals `deadBytesBound`; there is no second payload sweep

- **Frozen:** §6.4 chooses the path "using the bytes of packs that hold no live
  object after the metadata walk (an upper bound that §7 step 4 refines)", and
  §11 samples `deadBytesRefined` as "the same bytes after the payload sweep
  (§7 step 4)". §11 already states that the value equals `deadBytesBound` on
  the `none` path and on PostgreSQL, because neither runs a payload sweep there.
- **Proposed:** the field and its wire key stay; the value is the bound on
  **every** path and on both backends. The mark resolves every logical reference
  and delta base, so `visited` is the whole retained object set and the build
  copies exactly those, with no second payload sweep; §8 Phase M resolves the
  same closure before the swap. No live object can be discovered after the
  mark, so no refinement exists to report (`deadBytesRefined ==
  deadBytesBound`, including a SQLite `rebuild`).
- **Reason:** §6.4's parenthetical and §11's "after the payload sweep" describe
  a second pass over the live payload that the implementation does not run (the
  walk that refined the bound is the mark itself, so the two numbers cannot
  differ). Keeping a separate field that promises a value the
  implementation cannot produce is the drift this note exists to stop; keeping
  the field at its documented value keeps the §11 shape, the snake_case wire
  key (A7) and the `bound >= refined` invariant that clients read.
- **Where:** `engine/crates/layerfs-persistence/src/backend/sqlite/compact/receipt.rs`
  (`dead_bytes_refined: dead_bytes`, and the `deadBytesRefined` zero reason),
  `.../backend/postgresql/compact/{run,cleanup}.rs` (both `report(..)` calls pass
  the bound as `dead_refined`), `.../backend/sqlite/compact/types.rs` (the
  field's doc comment) and `.../backend/sqlite/compact/mod.rs` (the module doc
  states the equality). `.../compact_metrics.rs` keeps recording the field, and
  the capacity receipt
  `engine/crates/layerfs-persistence/tests/fixtures/sqlite_compact_capacity_measure.json`
  records `deadBytesRefined == deadBytesBound` on both arms.
- **Alternative recorded, not adopted:** restore a refinement pass, i.e. a second
  payload scan after the mark that re-derives each pack's dead share from the
  admitted references. It would need its own op phase and artifact-set entry, it
  reads up to the live payload a second time under the Disposable profile (§2),
  and it cannot change the path choice (§3 takes the bound), so it is not built
  for 2.0.0.
- **Effect on #52:** AC-52's golden pins both numbers and AC-57's receipts carry
  them, so the reports stay valid; no acceptance case asks for a value that
  differs from the bound. The frozen spec file stays byte-identical until
  sign-off: this entry, like A8 and A9, is the amendment #42 records.

### A11. §10: `store.compact`'s in-process call refusal is `Busy`, not `InUse`

- **Frozen:** §10 says `Busy` carries `{entity, phase, op}` plus the
  PostgreSQL holders and that "`InUse` stays for non-GC calls only". It does
  not name the code `store.compact` reports when the open cache refuses to
  close a Store because a call still runs on it in this process.
- **Proposed:** that refusal is `Busy`, naming the Store as `entity` with
  `phase` and `op` null: no operation holds the Store, a call does. The
  cache's own `InUse` is reported by the non-GC calls that close a Store:
  `store.push` and `store.pull` (through the shared transfer's `settle`),
  `store.remove`, `store.disconnect` and `backup.restore`. The list is
  illustrative, not exhaustive.
- **Reason:** `store.compact` is a GC call, so its refusals speak the GC
  vocabulary; reporting the open cache's `InUse` from the compaction would put
  a non-GC code on the call that §5's `Busy` and §10's code table govern.
- **Where:** `control/crates/layerfs-store/src/compact.rs` (`gc_refusal`, the
  open-cache mapping), `control/crates/layerfs-store/tests/compact_in_process_call.rs`
  (`a_running_call_is_busy_for_the_store_not_in_use` and
  `exactly_the_open_cache_refusal_is_mapped`), the `store.compact` error row
  in `docs/architecture/api/sdk/store.md`, and the frozen spec file, which
  stays byte-identical until sign-off: this entry is the amendment #42
  records, like A8–A10.

### A12. §4.1/§5.1 reading: a rebuild carries the run's own `gc_intent` row

- **Frozen:** §4.1 says `gc_intent` "is the shared Store-side admission table
  (created empty, never copied)"; §5.1 says compaction "writes its `gc_intent`
  operation row before its first checkpoint; the row lives in `<store>.compact`
  during build/verify, moves to the live Store after install, and is cleared
  at settle."
- **Reading:** this is a reading, not a twelfth contract amendment; it changes
  no contract text and the eleven amendments A1–A11 stand. The two sentences
  agree when "never copied" means no admission row of the sealed Store is
  carried across a rebuild: the fresh file starts with `gc_intent` empty. The
  run's own row is not a copy of an existing intent — the compaction writes
  it, and §5.1 requires it to live in the build artifact. So the rebuild
  stages the table empty and then inserts exactly the row whose `op_id` is
  this run's.
- **Where:** `engine/crates/layerfs-persistence/src/backend/sqlite/compact/build.rs`
  (`copy_rule` returns `Copy::Empty` for `gc_intent`, and the `copy_rows`
  special case inserts only `op_id=x'<this run>'`; the `copy_rule` doc comment
  says it "keeps only this run's admission intent").
- **No contract change:** neither frozen sentence is edited; the spec stays
  byte-identical until sign-off, like A8–A11.

### A13. §11: `store.compact` takes `on_progress` and sends one event per phase

- **Frozen:** §11 says compaction writes its phase into the op, that
  `store.status` shows it, and "there is no `PROGRESS` option (#42 item 40)".
- **Proposed (owner, 2026-11-18):** `store.compact` declares the shared
  `on_progress` function, the twelfth call to do so. The run sends
  `{ phase, done, total }` as each phase starts, once per phase and with null
  counts: `mark`, `build`, `verify`, `install` on SQLite and `mark`, `build`,
  `swap`, `cleanup` on PostgreSQL. A `none` run sends `mark` alone. The SQLite
  settle (D4's `cleanup` in `store.status`) is inside `install` and sends no
  event of its own. The op record and `store.status` are unchanged and stay
  the only record another process, or a call after a crash, reads.
- **Reason:** the caller of a long rebuild saw nothing until the report; a
  client that drives its progress display from `on_progress`, as it does for
  `store.push` and `backup.create`, had to poll `store.status` for this one
  call. The four GC calls still take ids only: `on_progress` is a function,
  not a data option, and a function never crosses into the library.
- **Not in this entry:** byte counts inside a phase, an estimate before the
  run, and cancellation.
- **Where:** `api/crates/layerfs-api/src/store/compact.rs` (`PROGRESS`),
  `control/crates/layerfs-store/src/compact.rs` (the events),
  `engine/crates/layerfs-persistence/src/compact_metrics.rs`
  (`RunMetrics::begin_phase`, `CompactPhase::step`) and `compact_observed` on
  both backends' handles; tests `an_observed_run_is_told_each_phase_once_in_order`
  (`sqlite_compact.rs`, `postgresql_compact.rs`) and
  `store_compact_sends_each_phase_as_a_progress_event` (`gc_calls.rs`). The
  frozen spec file stays byte-identical until sign-off, like A8–A11.

---

## D. SQLite implementation notes: four deviations and the D2 build decision

This section keeps six records. D1, D3, D4 and D5 are deviations from the
frozen text that the shipped code still carries; D6 is a prototype that
awaits an owner decision. D2 is the build decision that
replaced the whole-Store copy: the fresh-copy rebuild follows S5's "fresh
Store" sentence but deviates from §7 step 4's payload sweep and rollback
journal and from §11's peak-temp line, and the earlier `VACUUM INTO` + delete +
`VACUUM` build is historical. Each paragraph states the frozen sentence, the
behavior and the decision. The code in
`engine/crates/layerfs-persistence/src/backend/sqlite/compact/` is the
measured implementation of this branch.

### D1. §7 step 4: the fresh load runs with `foreign_keys=OFF`

- **Frozen:** "`PRAGMA foreign_keys=ON` outside the transaction (inside it the
  pragma is a no-op), then one rollback-journal transaction."
- **Implemented:** the fresh file's bulk load runs with `foreign_keys=OFF` and
  copies only the retained rows, in foreign-key order. The final state is
  proven by the §7 step 6 `foreign_key_check` on an `immutable=1` open before
  the rename, and the load never deletes from an enforced Store because there
  is no prune: the old whole-Store copy's delete order (which the deferred
  counter refused even with a clean final row set) is gone. The pragma stays
  off for the load so the ordering cannot turn a clean retained set into a
  `SQLITE_CONSTRAINT_FOREIGNKEY` commit.
- **Decision requested:** record the deviation (the pragma is off for the load,
  the post-load check is the guard) or require enforcement during the copy.
  The current code is the tested, reproduced behavior.

### D2. S5/§3/§7: the rebuild is the fresh-copy build; peak is old + the new Store + the visited scratch

- **Frozen:** S5 says "A rebuild copies only retained rows into a fresh Store";
  §7 step 4 describes the payload sweep and one rollback-journal transaction
  with `foreign_keys=ON`; §3 says a rebuild copies at least nine live bytes per
  byte freed below the 10% threshold; §7's cost sentence bounds peak disk as
  old Store + the new Store + the rollback journal/WAL + the visited scratch
  file.
- **Implemented (superseding the earlier whole-Store copy):** the build
  creates `<store>.compact` with the sealed Store's schema, copies only the
  rows the mark retained (pack-as-unit), builds the secondary indexes and
  triggers after the bulk load, verifies the fresh file and renames it. It
  stages no whole-Store copy, writes no prune journal and runs no `VACUUM`.
  The earlier `VACUUM INTO` + delete + `VACUUM` build wrote the retained bytes
  about four times over and needed a ~2.9× peak; §11's `peakTempBytes` for
  SQLite is now the new Store plus the visited scratch, because the fresh path
  stages no whole-Store copy and writes no rollback journal (a deviation from
  the frozen §11 line, recorded here).
- **Measured:** the validated prototype (`tools/bench/freshcopy/`, branch
  `val-freshcopy`) on a 10 GiB store-shaped fixture: fresh 32.5 s, 1.007× live
  written, peak 1.003× `(old + live)`; the old build 120.8 s, 4.03× written,
  1.577× peak. The 50 GB `VACUUM`-build numbers in `gc-runbook.md` are
  historical and are replaced by this cost model.
- **Decision (owner, this branch): the fresh-copy build is the default
  rebuild.** The old build's 50 GB measurement (5,131.835 s, 143.1 GB sampled
  peak, 147.609 GB bound) stays the record for that algorithm only.
- **Alternative recorded, not adopted:** "clone + delete + incremental
  vacuum" — `fs::copy`/`copy_file_range` (reflink on xfs/btrfs) the sealed
  file, delete dead rows in the clone with `journal_mode=OFF`, run
  `incremental_vacuum` until the free list is empty, verify, rename. It follows
  dead bytes instead of live bytes and can hold peak near one Store copy. It is
  not adopted: the fresh copy already writes ~1.0× live and needs no reflink
  size threshold.
- **Runbook consequence:** a rebuild needs old Store + about one live Store
  (+ margin) free, not 2.9× the Store; re-check free space before every
  rebuild. The runbook carries this (see its "Disk headroom" section).

### D3. §3/§7 step 1.5: the `none` path writes a transient visited scratch

- **Frozen:** §3's `none` row is "Mark only; nothing is written"; §7 step 1.5
  says "On `none`, delete the scratch and stop: no `.compact`, no
  `.compact-visited`, no residue."
- **Implemented:** both paths spill the closure into `<store>.compact-visited`
  (the metadata walk needs a bounded-memory visited set). The §6.1 closure and
  the §5.1 admission intent are one write transaction: on `none` it is rolled
  back, so no `kept_node` row, no `gc_intent` row and no WAL frame is
  committed, and the live Store keeps its pre-call `(size, mtime, change
  counter)`. Only the scratch is written; it is deleted before the call
  replies and counted in `peakTempBytes`, so no `.compact`, no residue and no
  generation change survive the call.
- **Decision requested:** say "nothing is written **that survives the call**":
  the Store file is untouched (the mark and its intent are a rolled-back
  transaction), and only the transient visited scratch is written, counted in
  `peakTempBytes` and deleted before the reply (the §11 table already names the
  visited scratch as part of `peakTempBytes`). Alternative is an in-memory
  visited set, which the measurement rules out for large Stores.

### D4. §11: SQLite records a fifth observable phase, `cleanup`, during settle

- **Frozen:** "`mark`, `build`, `verify`, `install` on SQLite; `mark`, `build`,
  `swap`, `cleanup` on PostgreSQL."
- **Implemented:** the SQLite settle writes phase `cleanup` to the home op
  while it removes the artifact set and clears the intent, so a crash in the
  settle reads `cleanup` (the next `store.compact` adopts the op). The four
  build phases are unchanged; `cleanup` is the settle phase, not a build step.
- **Decision requested:** add `cleanup` to §11's SQLite phase list (it already
  exists on the PostgreSQL list), or restrict SQLite to four phases and lose
  the settle-phase diagnosis.
- **Runbook consequence:** `store.status` shows `cleanup` on SQLite after the
  install point; the runbook's phase list includes it.

### D5. S6/§7: `store.compact` is local-only; a remote sqlite Store is `Unsupported`

- **Frozen:** S6 "A SQLite Store compacts on the machine where it lives
  (`place::plan`, `control/crates/layerfs-store/src/place.rs:40-49`)"; §7
  "Runs as one operation on the Store's machine (S6)."
- **Implemented:** the call consults `place::plan` but runs compaction only on
  the `Local` branch, in the caller's process. Its `Worker` branch is refused
  `Unsupported` before the Store is opened or the machine is dialled, and the
  worker has no `store.compact` operation at all. A sqlite Store pushed to a
  machine (`docker`, `lima`, `apple-container`) therefore cannot be compacted
  from the home that owns the record; the owner pushes it back to `native`,
  compacts there, and pushes it again. `store.compact` of a PostgreSQL Store
  is unchanged: the call dials the Store's recorded URL itself.
- **Reason:** the worker route needs a worker-side compaction operation, the
  Store record and lease read under the worker's root, and a crash/resume
  answer for a call whose run can be hours long; 2.0 ships the local engine
  and the transfer route (`store.push`/`store.pull`) instead. The refusal is
  named, never a silent local run on the wrong file.
- **Decision requested:** accept local-only for 2.0 as a deviation from S6
  (and say so in the API and control docs, which now name #42 D5), or require
  the worker `store.compact` route before the contract closes.
- **Consequence:** `store.compact` of a Store whose record names another
  machine is `Unsupported` with "local-only: run it on that Store's machine
  (S6 deviation #42 D5; the worker has no compaction route)"; the operation is
  refused before any effect. `api/crates/layerfs-api/tests/store_family.rs`
  `compact_of_a_remote_sqlite_store_is_a_documented_local_only_refusal` pins
  it.

### D6. S5/§2/§3/§6.4: the SQLite rebuild writes partly dead packs again, and the 10% counts their dead records

- **Status:** prototype on branch `claude/gc-opt-repack` (2026-11-18), not a
  frozen-text change. It needs an owner decision before it is contract.
- **Frozen:** S5 "The unit that frees bytes is the pack"; §2 lists
  "repacking partly dead packs (reported, not performed)" as out of the
  release; §3 takes the numerator of the 10% test over wholly dead packs only;
  §7 step 4 copies a retained pack whole.
- **Implemented (SQLite only):** the mark names every retained pack in which
  it found a record no reached object owns (`KeptObjects::partly_dead_packs`).
  The rebuild takes those packs out of the whole-pack copy and writes each one
  again from the records the visited set keeps, which includes records kept
  only as delta bases. The bytes are built by `layerfs-storage`
  (`pack::repack::retain_records`): a group that keeps every record is
  carried over as stored, a group that keeps none is left out, a group that
  keeps some is framed again, and a compressed ordinary group is decoded and
  compressed again. The pack keeps its `pack_id`; it gets a new SHA-256 and
  length; the kept objects' `object_location` rows move to the new group and
  record numbers. Singleton packs are never rewritten. Everything is written
  to `<store>.compact`, so the old Store is unchanged until the rename, as
  before.
- **Threshold:** `rebuild iff total > 0 AND (dead + deadInRetained) * 10 >=
  total`, where `dead` is the bytes of wholly dead packs and `deadInRetained`
  is the mark's `keptWaste.deadInRetainedPacksBytes`. The 10% value and the
  denominator are unchanged. A compressed group that mixes live and dead
  records still counts 0 here (its bytes cannot be attributed without
  decoding it), although the rebuild does rewrite it.
- **Report:** `deadBytesBound` and `deadBytesRefined` stay the bytes of wholly
  dead packs. After a rebuild `keptWaste.deadInRetainedPacksBytes` is the
  mark's value minus the share of the packs written again (0 when all were).
  A rebuild chosen only by in-pack dead bytes reports `deadBytesBound = 0`
  with `zeroReason: "no-waste"` and a non-zero `physicalShrink`; a field for
  the in-pack bytes freed is not added here.
- **Verification (§7 step 6):** besides the existing counts, each rewritten
  pack's row must carry the digest and length the build computed, and its
  locators must name each of its records exactly once. The rebuilt bytes are
  checked when they are built: the pack must parse and validate like any
  stored pack and every kept record must be byte-identical to its source,
  whose own SHA-256 is checked first. Payload is not read back from the
  staged file; the full doctor still owns that.
- **Not changed:** PostgreSQL (it still copies packs whole and decides on
  wholly dead packs only), `remove_file` (visibility-only, §6.2), the
  `inode_value_block` rows (#42 item 37) and delta-base-only records, which
  stay because a live record needs them.
- **Measured (one sample each, macOS host, load about 5):** on the scenario
  matrix the nine rows with dead records in retained packs end within eight
  4 KiB pages of their control Store's used pages (for example S8c 1,990,656
  → 495,616 bytes, control 479,232; S9a 2,490,368 → 1,421,312, control
  1,404,928; S8e 573,440 → 372,736, control 339,968), and four of them
  previously took `none`. Their pack bytes end within 500 bytes of control;
  the rest is `inode_value_block` rows of the removed history and page
  rounding over more, smaller packs. A 1.16 GB Store with half of every pack dead
  rebuilds to 580,763,648 bytes in 2.05 s (build 1.68 s); copying the same
  packs whole takes 1.91 s (build 1.52 s) and frees 0.9 MB.
- **Decision requested:** accept repacking and the widened numerator for
  SQLite in 2.0, or keep the frozen pack-as-unit rebuild.

## G. Deferred design decision (1)

G1–G6 were the retired implementation-note entries (their contract-carrying
decisions are folded into A/D above). G7 continues the numbering as a
**deferred design decision**: it recommends no contract text change and no
code change, and it records the measurements that settle the question.

### G7. §7/§10: the install-only lane stays deferred (#42)

- **Frozen:** §7 step 1 takes the persistence writer guard before the
  checkpoint and holds it through step 7's rename; §10 repeats that compaction
  holds `lane::writer_admission` from the seal through the rename, and install
  requires the sealed `(size, mtime, db-header change counter)` and the catalog
  tuple unchanged, otherwise `Conflict` with no writes. §8's Phase M records a
  catalog tuple and Phase B re-checks it for equality, so PostgreSQL has the
  same fence.
- **Proposed and measured (not adopted):** mark on the build's read snapshot,
  release the lane, let writers commit during the build, and take the lane
  again only for install. `tools/bench/installlane/` implements the contract
  that would need: an open high-water mark, an incremental re-mark of roots
  committed after it, a delta copy of newly-live packs/objects/metadata, a
  bounded WAL, and a replacement for the sealed-tuple equality fence. It is a
  store-shaped model (10 GiB / 10,240 packs, 25% dead, content-addressed
  dedupe references onto skipped packs), not product code; the 50 GB arm was
  not run. Raw numbers and the method are in
  `tools/bench/installlane/RESULTS.md`.
- **Measured (10 GiB model, fresh-copy build):** build 18.96 s for 8.04 GB.
  With the build snapshot open, 6,348 commits wrote 6.35 GiB (348 MiB/s) and
  the WAL reached **7.36 GB**. The re-mark scan took 0.88 s; the closure delta
  was 6,380 packs / 6.69 GB (6,348 new + 32 skipped packs reached by dedupe)
  and copied in 40.78 s (164 MiB/s). The checkpoint took **38.43 s for
  7.38 GB (183 MiB/s)**; verify 0.57 s. The install-only fence (checkpoint +
  delta + verify) is **80.65 s — 4.25× the fastest measured build** and about
  2.5× the 32.5 s premise. Checkpoint throughput across the small probes was
  156–183 MiB/s against a 176–420 MiB/s writer rate, and
  `wal_checkpoint(TRUNCATE)` returned `busy` on every call while writers
  committed (0/430 and 0/3,105 succeeded); the backlog grew +366 MiB/s.
  The prototype ran on the sandbox machine filesystem (Python 3.11.2,
  SQLite 3.40.1); these rates are host-specific and do not replace the R730
  evidence basis below.
- **Why deferred, three independent reasons:**
  1. **No bounded WAL.** The backlog cannot drain while writers commit
     (checkpoint slower than writers; `TRUNCATE` always `busy`). A cap with no
     checkpointer is a hard pause: at a 256 MiB cap the writers committed
     256 MiB and then stalled 17.33 s of a 10 s build. That is the frozen
     whole-build exclusion again, with a WAL on disk.
  2. **No bounded delta.** Content-addressed dedupe lets a post-mark commit
     reference a pre-mark object whose pack the mark skipped. A "new packs
     only" delta lost 8 reachable objects in a 64 MiB probe; the closure
     re-mark recovered them but copied the skipped pack too. A post-mark
     commit re-referencing the whole skipped set produced a delta of **1.00×
     the dead set with zero new bytes written**. The delta is bounded by what
     the rebuild exists to free, not by post-mark writes.
  3. **No shorter outage.** The fence is longer than the build it replaces and
     still needs a final quiesce: a delta watermark is always behind the newest
     commit (verifying at w2 missed 1,984 objects; the catch-up after writers
     stopped verified clean). The frozen tuple fence also always fails once
     writers continue (mark tuple `(7665, 10241, 81920, 10.74 GB)` vs current
     `(14013, 16589, 81920, 17.39 GB)`).
- **Correctness is reachable, but not cheaply:** the closure re-mark verified
  (112,353/112,353 objects, `missing=0`, `extra=0`), so the mechanism works.
  What it needs is a new state machine: the open mark, its high-water tuple,
  the delta watermark and the delta set must be durable and crash-resumable,
  and writers must be quiesced for the final catch-up + checkpoint + swap.
- **Exact contract changes it would require** (none made here):
  1. §7 step 1 / §10: seal, then *release* the writer lane; keep the sealed
     file uncheckpointable (the `immutable=1` attachment already guarantees
     it) and admit writers for the build; re-acquire for install.
  2. §6.1 / §7 / §8 Phase M: record an **open** mark tuple
     `(catalog_incarnation, live_heads_digest, next_history_serial,
     next_pack_id, next_ordinal, store_totals canonical/stored/metadata/
     logical)`; a later publication no longer makes the run `Conflict`.
  3. §7 steps 4/6/7 and §8 Phase B: replace the tuple-equality fence with an
     incremental re-mark from the tuple, a delta copy (a pack reached only by
     a base or a dedupe reference is copied whole), a re-verify over the
     post-delta frame, then a final quiesce, catch-up delta and swap.
  4. §9 / §10: add the WAL cap/backpressure contract (what a commit above the
     cap does, the maximum backlog, the latency/refusal shape) and the final
     catch-up fence; the SQLite seal check becomes a delta-watermark check.
  5. §4.1 / §9: persist the open mark tuple, the delta watermark and the delta
     set in `gc_op`/the sidecar so a crash during the delta is adoptable, and
     add that crash/recovery row to §9's matrix.
  6. §8 PostgreSQL: Phase M/B's equality re-check must accept the same delta
     path (or the entry must say PostgreSQL is out of scope); PG's Phase C
     already allows commits after the swap, but not during Phase M→B.
  7. §11: report `remarkSeconds`, `deltaPacks`, `deltaBytes`,
     `walBytesAtInstall` and `backpressureSeconds`, and make `peakTempBytes`
     path-dependent (target + WAL/delta temp + visited) instead of the
     `VACUUM INTO` formula.
  8. §5 / S5: state the dedupe rule — a post-mark commit may reference a
     pre-mark object whose pack the mark skipped, so "newly written packs" is
     not a delta bound; the bound is the dead set.
- **Decision:** defer install-only for 2.0.0 and keep the frozen whole-lane
  exclusion. The prototype and its measurements stay under
  `tools/bench/installlane/`; no product code, schema or spec text changes.

---

## Evidence basis used by D2 and the runbook

Measured on the host class **Dell PowerEdge R730, PERC H730 Mini controller,
rotational (HDD) backing — not NVMe** (88 vCPU across two Xeon E5-2699C v4,
125–135 GB visible RAM, ext4, 4.0 TB free). The rotational device explains the
~33 MB/s build copy rate; do not read the 50 GB rates onto an NVMe host.

| Step (GB = 10^9 B) | 10 GB arm [M] | 50 GB arm [M] |
|---|---|---|
| Mark (`none`) | 14.9 s/stored GB, 16.4 s/live GB | 35.9 s/live GB (32.7 s/stored GB) |
| Mark (`rebuild`) | 16.4 s/live GB | 17.2 s/live GB |
| Build (`VACUUM INTO` + prune + `VACUUM`) | 10.3 s/new GB | 93.0 s/new GB (moves ~141 GB to keep 45.8 GB, ~33 MB/s) |
| Verify | 0.034 s/new GB | 1.87 s/new GB |
| Install | ~1.9 s fixed | 7.311 s |

- **10 GB is the medium arm** every agent can qualify: three runs per path
  (9.987 GB pack bytes; `none` median 149.318 s, `rebuild` median 246.371 s;
  one `Durable` rebuild 284.078 s as a control). The whole-Store build's
  medians are in
  `engine/crates/layerfs-persistence/tests/fixtures/sqlite_compact_capacity_measure_whole_store.json`
  (a transcribed summary of the T-G61 run: the raw per-run samples were not
  retained); the fresh-copy re-measurement on the same fixture is
  `.../sqlite_compact_capacity_measure.json` (`rebuild` median 118.439584 s,
  `none` 88.439 s). That receipt predates #49 round 4: its build path has no
  256 MiB `src.mmap_size` source window and its verify still runs the
  `quick_check` page scan, so it is the pre-head8 build/verify path, not a
  head8 re-measurement.
- **50 GB is the owner-final arm** (#42 item 44): fixtures 20–50 GB, run once
  at the final identity, one run per path (49.999 GB, 384,850 packs, 8.876%
  dead; `none` 1,633.585 s, `rebuild` 5,131.835 s); receipt
  `engine/crates/layerfs-persistence/tests/fixtures/sqlite_compact_capacity_measure_50gb.json`.
  It is not part of a normal PR run.
- The 50 GB numbers do **not** scale linearly from the 10 GB ones (the 10 GB
  Store and its artifacts fitted the page cache; the 50 GB one did not), so a
  capacity claim states per-GB rates with its arm and host.
- **Fixture basis (2026-11-16, #711 review).** Every number above was measured
  on the earlier whole-file-only fixture: one whole-file object per pack and one
  genesis Layer per project. The harness now models real Layer history depth,
  content shared across projects and delta chains kept only as bases, and it
  records the realized fixture shape, the cold-cache method and the 3-run
  variance (`tools/ci/gc-bench.md`, `examples/gc_bench/gc_fixture.rs`). These
  numbers are evidence for the earlier shape: the gated 50 GB owner-final arm
  has **not** been re-run on the current fixture. The 10 GB arm **was**
  re-measured on the current shape on 2026-11-16 (`rebuild` median 133.076 s,
  `none` 82.683 s, three runs per arm, warm overlayfs cache); its receipt is
  `engine/crates/layerfs-persistence/tests/fixtures/sqlite_compact_capacity_measure_gc_bench.json`.
  It is a different generated fixture from the `capacity_measure` 9.987 GB run,
  so the two 10 GB numbers are not interchangeable. That receipt too predates
  #49 round 4's source mmap window and `quick_check` removal, so its build and
  verify phases are the pre-head8 path, not a head8 re-measurement; the ~4 s
  `quick_check` cost is derived, not measured. Treat both 10 GB receipts as
  pre-head8 until a head8 re-run replaces them.

## Request

Please record A1–A11 and A13 as #42 items (or amend the frozen text) and note the A12
reading (it changes no contract text), and accept or reject D1–D5 individually
when the owner signs off, and accept G7 as the record of a deferred design (it
changes no contract text). Until then the implementation follows the frozen
text except where A8–A13 and D1–D5 are named, and the spec file in this branch
is byte-identical to the frozen version.
