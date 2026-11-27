# Review 4: `engine/crates/layerfs-store-service`

Scope: planned layout rows for `engine/crates/layerfs-store-service/` in
`nav/site/layout.json` (353 rows: src, tests), old source at
`v2tree/engine/crates/layerfs-store-service` (archive of `layerfs_v2_0_0` at
1317ebf7ca), production LOC from the plan rows (`prod_loc_files.txt`). There is
no `examples/`. Settled and not reopened: `methods/` (families, call-named
files, flat `file/`, loose `store.rs`), `read/` → `snapshot/`, the `workspace/`
group.

## 1. Verdict

**Good overall, with a handful of real defects.** The planned top level is
right: `history/`, `methods/`, `operator/`, `snapshot/`, `store/`,
`workspace/`, `write/` are distinct roles, the counts and depth are within
limits (deepest: `workspace/owner/roster/x.rs`, 4 levels), and root keeps 2
loose files. The defects are inside folders:

- `snapshot/`'s central type (`Snap`, the snapshot a read names, used by 28
  files in every folder) is buried as `snapshot/tree/lookup.rs`, a name that
  also collides with `history/lookup.rs`.
- Mechanism lives in facade folders: `snapshot/` imports `operator/files.rs`
  (`revision`, `entry`) and `write/files.rs` (`path`), against the crate's own
  rule that read/write/history know no call tables.
- `history/lookup.rs` is not history: its refusals and lookups are used by 30
  files across every folder.
- Same job, different names: three page-boundary caches (`boundaries.rs`,
  `revision_pages.rs`, `pages.rs`). Inside `store/`, `bootstrap.rs` opens the
  Store while `open.rs` holds the opened handle, which reverses persistence's
  `store/open.rs`.
- Two `tests/` areas use rejected words (`surface/`, `read/`), and `surface/`
  mixes four topics.

**Is `store/` a rule-14 repeat? No, keep it.** The crate is `store-service`,
and its central type is `Store`, defined in `store/`. Rule 14 forbids the
crate's own name as a folder (`layerfs-inspect/src/inspect/`). `store/` is the
noun for one part of the crate: opening, guarding and installing the Store,
plus the opened handle and its ports. Every other folder uses it
(`crate::store::Store`), and no folder depends upward from it (no
`crate::{history,snapshot,write,operator,methods}` in its files). Rule 19 also
argues for the word: engine `layerfs-persistence/src/store/` (`open.rs`,
`config.rs`, `seal.rs`, `selection.rs`) and `layerfs-project/src/store/`
(`init.rs`, `install.rs`) use `store/` for the same concern. Every alternative
collides with something: `open/` with overlay `open/`, `session/` with the
persistence backends' `session/`, and `handle/` is too thin. What needs fixing
is the file names inside it (finding 5).

**Is `operator/` predictable? Yes, keep it.** It is the `backend.store.*` call
table (`operator/dispatch.rs`: "The backend Store call router (#428:
`backend.store.*`)"). The same word names the same table in three crates:
daemon `operator/` (`backend.daemon.workspace.*`), control Sandbox
`channel/operator.rs` ("Daemon backend method admission"), and here. The call
prefix `backend.` cannot become the folder name, because rule 19 reserves
`backend/` for drivers (persistence and control Store `backend/{sqlite,postgresql}`).
`operator/` mirrors `methods/`: both have `args.rs` plus `dispatch.rs`, which
is a good sibling design. Its file names have two problems (findings 3 and 8).

## 2. Findings (ranked by value)

### F1. The snapshot type is hidden as `snapshot/tree/lookup.rs` → `snapshot/named.rs`

- **What:** old `read/snapshot.rs` (212) is planned as `snapshot/tree/lookup.rs`.
- **Evidence:** Header: "Saved snapshots the read calls name, and path lookups
  in them." It defines `pub enum Kind`, `pub struct Snap`, `resolve`, `open`,
  `entry`, `find`, `answer` and `header`: the snapshot itself, not a tree
  helper. Callers (`read::snapshot` / `super::snapshot`) span 28 files:
  `history/listing.rs`, `methods/args.rs`, `methods/branch/*`,
  `methods/file/{glob,grep,list,read,revision,stat}.rs`, `methods/project.rs`,
  `operator/{args,dispatch,files}.rs`, `write/files.rs` and 12 `read/*` files.
  Under the plan, every one of them reaches the crate's central type through
  `crate::snapshot::tree::lookup::Snap`. `lookup.rs` also already names
  `history/lookup.rs` ("Names, identities, lookups and refusals"), so one crate
  would have two unrelated `lookup.rs` files. That breaks rule 13 and makes
  `use` lines ambiguous.
- **Fix:** move to `snapshot/named.rs`, loose in `snapshot/`, as "the
  snapshot a read names". The name `snapshot.rs` would repeat the folder
  (rule 14), and `resolve.rs` is taken twice already (`history/resolve.rs`,
  `workspace/commit/resolve.rs`). `snapshot/tree/` then holds
  `positional`, `stats`, `visibility`, `walk` (4 files, within the limits), and
  `snapshot/` has 4 loose files (the limit).

### F2. `tests/surface/` and `tests/read/` use rejected words; `surface/` mixes four topics

- **Evidence:** Rule 12 rejects `surface/` and `read/`, and src renamed
  `read/` → `snapshot/`. The planned `tests/surface/` holds:
  - method-table checks: `method_refusal_order`, `method_golden` plus
    `sdk_methods.txt`, `branch_count`, `project_count`, `fixed_costs`,
    `timing_coverage`;
  - `admission.rs`: "#41 T6: the Store service's `branch.remove`,
    `project.delete` and `project.remove_file` admit the Store's intents", a
    removal topic;
  - `epoch_fence.rs`: "the writer-side `project_epoch` fence on the Workspace
    Commit path", a Commit topic;
  - `kernel_guard.rs`: "Direct Store access is refused `Unsupported` …",
    Store opening.

  Two Store-opening tests sit elsewhere: `write/install_directory.rs` (tests
  `install::prepare`, not a write) and `read/object_demand_errors.rs` ("An
  object demand's Store failure keeps its kind").
- **Fix:**
  - `tests/surface/` becomes `tests/methods/`, mirroring src `methods/`,
    with the six method-table topics and `sdk_methods.txt`.
  - `admission.rs` → `tests/removal/`.
  - `epoch_fence.rs` → `tests/commit/`.
  - New area `tests/store/`, mirroring src `store/`, with `kernel_guard`,
    `install_directory` and `object_demand_errors`.
  - `tests/read/` → `tests/snapshot/`.

### F3. Read mechanism lives in facade folders: `operator/files.rs` and `write/files.rs`

- **Evidence:** `lib.rs`: "`read`, `write` and `history` know no SDK method
  names; `methods/` is the SDK facade". Yet:
  - `read/revision.rs`: `use crate::operator::files::{entry, revision};`
  - `read/blame.rs`: `operator::files::{entry, revision}`. These two functions
    (`operator/files.rs` lines 17–57, about 40 LOC) are snapshot mechanism
    (a node's root, parent and record, and an entry at a root). The file's own
    calls are `history` (`backend.store.fs.list_revisions`) and `stats`.
  - `read/branch_compare.rs:68`: `crate::write::files::path(...)`. So do
    `operator/files.rs` (`write::files::path as checked`), `methods/args.rs`
    and `methods/branch/diff.rs`. `path()` is an argument checker ("expected
    an absolute path without empty, dot or parent components"), not a write.
- **Fix:**
  - Split `operator/files.rs`: `revision` and `entry` merge into
    `snapshot/paged/revision.rs`, the revision engine and their main user.
    `operator/files.rs` keeps the two backend calls and imports from
    `snapshot`, so the facade depends on the mechanism, not the reverse.
  - Move `write/files.rs` `path()` into the shared lookup module (F4), beside
    its siblings `node(text)` and `name(text)`.

### F4. `history/lookup.rs` is crate-wide → `src/lookup.rs`

- **Evidence:** Header: "Names, identities, lookups and refusals shared by the
  Store calls." It holds `Answer`, `Arg`, `refuse`, `invalid`, `history`,
  `content`, `published`, `failed`, `node`, `name`, `project`, `branch`,
  `commit`, `layer`, `root` and `next_page`. 30 files import it (usually
  `use crate::history::lookup as args`):
  - 14 of the `read/` files;
  - all 4 `write/` files;
  - `methods/{args,project,dispatch,store}.rs` and
    `methods/branch/diff_branches.rs`;
  - `operator/{args,files,store,inspect}.rs`;
  - `removal.rs` and `store_service.rs`.

  It is the crate's shared refusal and lookup module. Inside `history/` it
  creates a two-way tangle: `history/` → `snapshot` (`listing.rs` uses
  `read::snapshot::answer`) and `snapshot` → `history::lookup`.
- **Fix:** move to `src/lookup.rs` (root loose files: `lookup.rs`,
  `service.rs`, `telemetry.rs`, so 3 of 4). `history/` keeps 9 files. Merge
  `write/files.rs::path` into it (F3).

### F5. `store/` file names reverse persistence's: `bootstrap.rs` opens, `open.rs` is the handle

- **Evidence:**
  - `bootstrap.rs` (moving into `store/`): "Concrete Store composition and
    mount-time reader opening". It defines `open_store`,
    `open_store_observed`, `open_postgres` and `OpenedStore`.
  - `store/open.rs`: "Opened provider ownership and one reader per mounted
    Workspace". It defines `pub struct Store`, `StoreWork`, `ReuseWork`,
    `SaveCounters` and `MergeCounters`.
  - Persistence `store/open.rs` is "Explicit creation/open …", and its
    `store/handles.rs` is "Application-owned physical and history handles".

  Rule 19 (same idea, same word): the file that opens a Store is `open.rs`.
  In this crate, `open.rs` would be the opened handle and the opener would be
  `bootstrap.rs`.
- **Fix:** `store/bootstrap.rs` → `store/open.rs`, and `store/open.rs` →
  `store/handle.rs` (the opened Store handle, its readers and counters).

### F6. `store/statistics.rs` is half Workspace Commit state; its only Workspace caller is `workspace/commit/run.rs`

- **Evidence:** It has two blocks:
  - `impl Store { pub fn logical_size }`, used by `history/branch.rs:447`;
  - `impl BoundWorkspace { pub(super) fn known_logical, remember_logical }`,
    which reads the `BoundWorkspace.logical` field declared in old
    `store/operation.rs` ("`statistics.rs`" is named in that field's doc).

  Its only callers are old `store/commit.rs:350,721`. The plan moves
  `operation.rs` → `workspace/bind/` and `commit.rs` → `workspace/commit/run.rs`
  but leaves this file in `store/`, so its `pub(super)` methods can no longer
  reach their caller or their field. `workspace/commit/` already has 12 files
  (the ceiling).
- **Fix:** split the file and delete it:
  - the `BoundWorkspace` cache (about 23 LOC) merges into
    `workspace/bind/operation.rs`, next to the `logical` field;
  - `Store::logical_size` (11 LOC) merges into `store/handle.rs`
    (`impl Store`).

### F7. Three page-boundary caches, three names → one word, `boundaries`

- **Evidence:** All three files have the same shape: a `Key`, a `Boundary`
  or path value, a `static` recent list, and `find`/`keep` functions.
  - `paged/boundaries.rs` (old `paged_boundaries.rs`): "bounded boundaries
    for immutable namespace result pages".
  - `paged/revision_pages.rs`: "Bounded immutable revision-page boundaries".
  - `compare/pages.rs` (old `comparison_pages.rs`): "bounded boundaries for
    pinned numbered tree comparisons".

  Rule 9 (same job, same file name) and rule 19 apply.
- **Fix:** `snapshot/compare/pages.rs` → `snapshot/compare/boundaries.rs`, and
  `snapshot/paged/revision_pages.rs` → `snapshot/paged/revision_boundaries.rs`.

### F8. `operator/store.rs` → `operator/storage.rs`

- **Evidence:** Header: "Publication counters and Init's recorded totals". Its
  functions are `stats` (`backend.store.project.get_storage`), `commits`
  (`backend.store.branch.list_commit_storage`) and `overwritten`
  (`project.list_overwritten`). In a crate where everything is the Store,
  `store.rs` predicts nothing. `crate::operator::store` beside `crate::store`
  also reads as the same module; `methods/store.rs` calls
  `crate::operator::store::stats`.
- **Fix:** rename to `operator/storage.rs`. The call names say "storage"
  (`get_storage`, `list_commit_storage`).

### F9. `workspace/bind/snapshot.rs` → `workspace/bind/run.rs` (file name only; the `workspace/` group stays)

- **Evidence:** Old `store/bind.rs`: "One coherent snapshot and bounded root
  demand before local namespace creation". It holds the entry
  `impl Store { pub fn bind, bind_placed, bind_named, bind_named_placed,
  bind_private }`. Its sibling `workspace/commit/` names its entry `run.rs`
  (old `store/commit.rs`), and Store/machine/home review 3 settled "A call
  folder's entry file is `run.rs`". `snapshot.rs` also collides with the
  crate's `snapshot/` folder, which means saved-snapshot reads (rule 13).
- **Fix:** rename to `workspace/bind/run.rs`. This does not reopen the settled
  `workspace/` group; it only names one file inside it.

### F10. `snapshot/compare/` names files by subject, but the real split is by mode

- **Evidence:**
  - `compare/snapshots.rs` (old `compare.rs`, 488): "Differences between
    saved snapshots: two files, two Commits, two Branches". It holds
    `diff_files`, `diff_commits` and `diff_branches`, the default mode.
  - `compare/branches.rs` (old `branch_compare.rs`): "Pinned mixed-kind
    comparisons", the numbered page mode that `methods/branch/diff_branches.rs:48`
    takes only for pinned pages.
  - `compare/files.rs` (old `file_compare.rs`): "numbered aligned detail rows".

  A reader looking for the Branch diff opens `branches.rs` and misses
  `snapshots.rs::diff_branches`. `snapshot/compare/snapshots.rs` also repeats
  its ancestor folder's word.
- **Fix:** `compare/snapshots.rs` → `compare/diff.rs`, `compare/branches.rs` →
  `compare/tree_pages.rs`, `compare/files.rs` → `compare/file_rows.rs`.

### F11. `history/units.rs` → `history/merge_units.rs`

- **Evidence:** Header: "Merge units of a Branch merge (#352 F-057)". Alone,
  `units` predicts nothing in `history/`. "Merge units" is the established
  term (F-057, `layerfs_content::merge::MergeUnit`).

### F12. `write/files.rs` → `write/changes.rs`

- **Evidence:** It defines `pub enum Change` and `pub fn commit`, which binds
  a private Workspace, applies the change list, commits once and closes. In
  `write/`, everything is about files, so `files.rs` says nothing. Once
  `path()` leaves (F3), what remains is the change list.

### F13. Test file names that repeat their folder or keep campaign words

- `tests/compare/compare.rs` → `tests/compare/wide_namespaces.rs`. Header:
  "SDK diff correctness and Store-demand regressions for wide immutable
  namespaces".
- `tests/history/surface.rs` (rejected word) → `tests/history/listing.rs`.
  Header: "Branch pages by name with prefixes, a Commit list pinned …";
  matches src `history/listing.rs`.
- `tests/history/operator_history.rs` repeats its folder →
  `tests/history/get_history.rs`, after the `backend.store.project.get_history`
  call it tests.
- `tests/allocators/unknown_allocators*.rs`: 9 files share one prefix
  (rule 2). Drop it: `unknown.rs`, `cold_a.rs` … `cold_d.rs`,
  `raced_a.rs` … `raced_c.rs`, `warm_b.rs`.
- `tests/postgresql/postgresql_open_cost.rs` repeats its folder, and
  `tests/postgresql/tester768_open_cost.rs` keeps its campaign name. These
  are two "open_cost" tests. Rename them `session_open_cost.rs` ("connections
  and round trips of PostgreSQL session opens") and `service_open_cost.rs`
  ("what a Store service open alone costs").
- `tests/standalone/writer_reopen_reopen_sweep.rs` → `writer_reopen_sweep.rs`;
  `tests/standalone/read_fs_reads_replay.rs` → `snapshot_reads_replay.rs`.

### F14. Small cleanups (low value; take them only with the moves above)

- `snapshot/compare/line_alignment.rs` (4 LOC) wraps a single function,
  `layerfs_content::compare::diff_exact`. Its three callers sit in two
  sibling folders (`compare/files.rs`, `text/blame.rs`,
  `text/attribution.rs`). Delete the file and call `diff_exact` directly.
- `snapshot/cached.rs` → `snapshot/cache.rs`: a noun (rule 16), and the same
  word as vfs `base/cache.rs`.

## 3. Proposed layout of changed folders

Scope: planned tree (`nav/trees/tree-engine.txt`) for
`engine/crates/layerfs-store-service/src/`. LOC comes from the plan rows
(production counter at 1317ebf7ca). Split or merged files keep their current
LOC with an annotation; no new LOC is estimated.

```diff
  engine/crates/layerfs-store-service/src/   [partial: changed folders only]
    lib.rs                                23
    service.rs                           203
    telemetry.rs                         382
+   lookup.rs                            298   (moved from history/lookup.rs; gains write/files.rs path(), F4)
    history/                         [2,823]
      branch.rs                          696
      branch_remove.rs                    61
      gc_admission.rs                    361
      listing.rs                         458
-     lookup.rs                          298   (moved to src/lookup.rs, F4)
      merge_units.rs                      88   (was units.rs, F11)
      mod.rs                              10
      project_delete.rs                   68
      resolve.rs                         173
      saved.rs                           210
      whiteout.rs                        400
    operator/                          [628]
      args.rs                             48
      dispatch.rs                         18
      files.rs                           174   (split: revision, entry → snapshot/paged/revision.rs, F3)
      list_workspaces.rs                 280
      mod.rs                               7
      storage.rs                         101   (was store.rs, F8)
    snapshot/                        [4,790]
      cache.rs                            88   (was cached.rs, F14)
      file_facts.rs                      129
      mod.rs                              24
      named.rs                           212   (was tree/lookup.rs; the Snap type, F1)
      reply.rs                            78
      compare/                       [1,188]
        boundaries.rs                     44   (was pages.rs, F7)
        diff.rs                          488   (was snapshots.rs, F10)
        file_rows.rs                     296   (was files.rs, F10)
-       line_alignment.rs                  4   (callers call diff_exact directly, F14)
        mod.rs                             —
        plans.rs                          27
        tree_pages.rs                    329   (was branches.rs, F10)
      paged/                           [942]
        boundaries.rs                     42
        continuation.rs                   72
        mod.rs                             —
        query.rs                          62
        results.rs                       406
        revision.rs                      289   (gains revision, entry from operator/files.rs, F3)
        revision_boundaries.rs            71   (was revision_pages.rs, F7)
      text/                          [1,235]   (unchanged)
      tree/                            [894]
        mod.rs                             —
        positional.rs                    356
        stats.rs                          70
        visibility.rs                    219
        walk.rs                          249
    store/                           [1,384]
      handle.rs                          323   (was open.rs: the opened Store; gains Store::logical_size, F5/F6)
      install.rs                         206
      kernel_guard.rs                    114
      mod.rs                              43
      open.rs                            291   (was bootstrap.rs: opens the Store, F5)
      ports.rs                           279
      recognizer.rs                       94
-     statistics.rs                       34   (split into bind/operation.rs + store/handle.rs, F6)
    workspace/                         [partial]
      bind/                          [1,187]
        mod.rs                             —
        operation.rs                     125   (gains known_logical/remember_logical, F6)
        reader.rs                        248
        readers.rs                       408
        request.rs                        80
        run.rs                           326   (was snapshot.rs: the bind entry, F9)
    write/                           [1,021]
      changes.rs                         274   (was files.rs; path() → src/lookup.rs, F3/F12)
      edit.rs                            136
      mod.rs                               4
      mutation.rs                        311
      recursive.rs                       296
```

Tests (`engine/crates/layerfs-store-service/tests/`, no production LOC; only
changed areas shown):

```diff
  tests/                                   [partial]
-   surface/                           [0]   (rejected word; dissolved, F2)
+   methods/                           [0]   (was surface/: the method table)
+     main.rs                           —   new
      method_refusal_order.rs            0
      method_golden.rs                   0
      sdk_methods.txt                    0
      branch_count.rs                    0
      project_count.rs                   0
      fixed_costs.rs                     0
      timing_coverage.rs                 0
+   store/                             [0]   (mirrors src store/)
+     main.rs                           —   new
      kernel_guard.rs                    0   (from surface/)
      install_directory.rs               0   (from write/)
      object_demand_errors.rs            0   (from read/)
    removal/                           [0]   (partial)
      admission.rs                       0   (from surface/)
    commit/                            [0]   (partial)
      epoch_fence.rs                     0   (from surface/)
-   read/                              [0]   (rejected word)
+   snapshot/                          [0]   (was read/: 10 topics + main.rs)
    history/                           [0]   (partial)
      listing.rs                         0   (was surface.rs)
      get_history.rs                     0   (was operator_history.rs)
    compare/                           [0]   (partial)
      wide_namespaces.rs                 0   (was compare.rs)
    allocators/                        [0]
      main.rs                            —   new
      unknown.rs                         0   (was unknown_allocators.rs)
      cold_a.rs                          0
      cold_b.rs                          0
      cold_c.rs                          0
      cold_d.rs                          0
      raced_a.rs                         0
      raced_b.rs                         0
      raced_c.rs                         0
      warm_b.rs                          0
    postgresql/                        [0]   (partial)
      session_open_cost.rs               0   (was postgresql_open_cost.rs)
      service_open_cost.rs               0   (was tester768_open_cost.rs)
    standalone/                        [0]   (partial)
      writer_reopen_sweep.rs             0   (was writer_reopen_reopen_sweep.rs)
      snapshot_reads_replay.rs           0   (was read_fs_reads_replay.rs)
```

What this does not fix:

- `history/` ↔ `snapshot/` still reference each other.
  `history/listing.rs` uses `snapshot::answer`, and `history/branch.rs` uses
  `REPLY_BUDGET`. Both are reply helpers. Moving them would mean a new reply
  module for two call sites, which is not worth it now.
- `lib.rs`'s `//!` text names `read`, `overlay`, `store_service`. It must be
  updated with the move (documentation, not layout).

## 4. Checked and left alone

- **Root:** `service.rs` (StoreService; the old `service/` folder is gone,
  so there is no collision) and `telemetry.rs` (an adapter over the
  `layerfs-telemetry` crate, so the same idea under the same word, unlike
  persistence `inspect/`). With `lookup.rs`, the root has 3 loose files.
- **`store/recognizer.rs`:** a daemon-wide write-recognizer table, owned as a
  field of `Store` (`open.rs:87`) and reached through
  `StorePorts::recognizer()` (daemon `filesystem_port.rs:304`). Its placement
  follows its owner. A move to `workspace/` would separate it from the struct
  that holds it.
- **`store/install.rs`, `kernel_guard.rs`, `ports.rs`:** in the right place.
  `kernel_guard` is imported as a module by 10 external callers, and the name
  is specific.
- **`history/whiteout.rs`:** the name matches the code, which writes overlay
  whiteouts (`materialize_removals`). It is shared by `write/`, the merge in
  `history/branch.rs` and `workspace/commit/`, so `history/` is a fair home.
  `gc_admission.rs` mirrors persistence `gc/admission/`. `branch_remove.rs`
  (61) could merge into `branch.rs` (696 → 757); left as is.
- **`history/` as a name:** this crate's history operations sit over
  persistence's `history/` catalog, so the layered use of one word is
  consistent.
- **`operator/list_workspaces.rs`:** it serves
  `backend.store.project.get_history` here and the read model behind the
  daemon's `sandbox.list_workspaces`. The daemon's call file has the same
  name, which is the across-layers pattern for one call. Left as planned.
- **`operator/args.rs` vs `methods/args.rs`:** two tables, one shared design,
  where methods builds on operator's base readers. The daemon imports
  `operator::args::{paging, text}`. Fine.
- **`snapshot/paged/`, `text/`, `tree/`:** `paged/` is "matching-result
  pages" (`results.rs`, `revision.rs`) plus their paging machinery (`query`,
  `continuation`, `boundaries`), which is coherent. Its `query.rs` is used by
  every snapshot subfolder and by `methods/file/*`, which is acceptable for
  the paging contract.
- **Test areas `commit_*`:** 7 sibling areas share a prefix. They cannot nest
  (`tests/<area>/main.rs` is Cargo's auto-discovered shape) and are split for
  the 2-minute ceiling, so they stay. The `support/` grouping
  (`fault/`, `verify/`) is fine.
- **Counts and depth:** every planned leaf is 3–12 files except settled
  `methods/file/`. Branch folders have ≤4 loose files after these changes.
  The maximum depth is 4.

## 5. Machine-readable list

```json
[
  {"from": "engine/crates/layerfs-store-service/src/snapshot/tree/lookup.rs", "to": "engine/crates/layerfs-store-service/src/snapshot/named.rs", "op": "move", "why": "defines Snap/Kind/resolve used by 28 files; lookup.rs hides it and collides with history/lookup.rs"},
  {"from": "engine/crates/layerfs-store-service/src/operator/files.rs", "to": "engine/crates/layerfs-store-service/src/operator/files.rs", "op": "split", "why": "revision() and entry() are snapshot mechanism used by snapshot/text/blame.rs and snapshot/paged/revision.rs"},
  {"from": "engine/crates/layerfs-store-service/src/operator/files.rs", "to": "engine/crates/layerfs-store-service/src/snapshot/paged/revision.rs", "op": "merge", "why": "revision() and entry() join the revision engine so snapshot no longer imports operator/"},
  {"from": "engine/crates/layerfs-store-service/src/history/lookup.rs", "to": "engine/crates/layerfs-store-service/src/lookup.rs", "op": "move", "why": "refusals and lookups imported by 30 files in every folder; not history"},
  {"from": "engine/crates/layerfs-store-service/src/write/files.rs", "to": "engine/crates/layerfs-store-service/src/write/changes.rs", "op": "split", "why": "path() argument checker leaves (used by snapshot, methods, operator); the rest is the change list commit"},
  {"from": "engine/crates/layerfs-store-service/src/write/files.rs", "to": "engine/crates/layerfs-store-service/src/lookup.rs", "op": "merge", "why": "path() joins node()/name() text parsers; removes snapshot -> write dependency"},
  {"from": "engine/crates/layerfs-store-service/src/store/open.rs", "to": "engine/crates/layerfs-store-service/src/store/handle.rs", "op": "rename", "why": "holds the opened Store handle and counters; open.rs is the opener in persistence store/"},
  {"from": "engine/crates/layerfs-store-service/src/store/bootstrap.rs", "to": "engine/crates/layerfs-store-service/src/store/open.rs", "op": "rename", "why": "defines open_store/OpenedStore; same idea, same word as persistence store/open.rs"},
  {"from": "engine/crates/layerfs-store-service/src/store/statistics.rs", "to": "engine/crates/layerfs-store-service/src/workspace/bind/operation.rs", "op": "split", "why": "BoundWorkspace logical-size cache joins its field; only caller is workspace/commit/run.rs; pub(super) breaks in store/"},
  {"from": "engine/crates/layerfs-store-service/src/store/statistics.rs", "to": "engine/crates/layerfs-store-service/src/store/handle.rs", "op": "merge", "why": "Store::logical_size (11 LOC) joins impl Store"},
  {"from": "engine/crates/layerfs-store-service/src/snapshot/compare/pages.rs", "to": "engine/crates/layerfs-store-service/src/snapshot/compare/boundaries.rs", "op": "rename", "why": "same page-boundary cache shape as paged/boundaries.rs; one job, one name"},
  {"from": "engine/crates/layerfs-store-service/src/snapshot/paged/revision_pages.rs", "to": "engine/crates/layerfs-store-service/src/snapshot/paged/revision_boundaries.rs", "op": "rename", "why": "same page-boundary cache shape as paged/boundaries.rs; one job, one name"},
  {"from": "engine/crates/layerfs-store-service/src/operator/store.rs", "to": "engine/crates/layerfs-store-service/src/operator/storage.rs", "op": "rename", "why": "get_storage/list_commit_storage figures; store.rs is vague here and reads like crate::store"},
  {"from": "engine/crates/layerfs-store-service/src/workspace/bind/snapshot.rs", "to": "engine/crates/layerfs-store-service/src/workspace/bind/run.rs", "op": "rename", "why": "bind entry (Store::bind*) like sibling commit/run.rs; avoids collision with snapshot/"},
  {"from": "engine/crates/layerfs-store-service/src/snapshot/compare/snapshots.rs", "to": "engine/crates/layerfs-store-service/src/snapshot/compare/diff.rs", "op": "rename", "why": "default diff_files/diff_commits/diff_branches; snapshots repeats the ancestor folder"},
  {"from": "engine/crates/layerfs-store-service/src/snapshot/compare/branches.rs", "to": "engine/crates/layerfs-store-service/src/snapshot/compare/tree_pages.rs", "op": "rename", "why": "pinned numbered tree comparison pages, not the Branch diff (that is diff.rs)"},
  {"from": "engine/crates/layerfs-store-service/src/snapshot/compare/files.rs", "to": "engine/crates/layerfs-store-service/src/snapshot/compare/file_rows.rs", "op": "rename", "why": "numbered aligned file rows, the file counterpart of tree_pages"},
  {"from": "engine/crates/layerfs-store-service/src/history/units.rs", "to": "engine/crates/layerfs-store-service/src/history/merge_units.rs", "op": "rename", "why": "merge units (F-057); units alone predicts nothing"},
  {"from": "engine/crates/layerfs-store-service/src/snapshot/compare/line_alignment.rs", "to": "engine/crates/layerfs-store-service/src/snapshot/compare/file_rows.rs", "op": "merge", "why": "4-LOC wrapper of layerfs_content::compare::diff_exact; callers (file_rows, text/blame, text/attribution) call it directly"},
  {"from": "engine/crates/layerfs-store-service/src/snapshot/cached.rs", "to": "engine/crates/layerfs-store-service/src/snapshot/cache.rs", "op": "rename", "why": "noun, same word as vfs base/cache.rs"},
  {"from": "engine/crates/layerfs-store-service/tests/surface/method_refusal_order.rs", "to": "engine/crates/layerfs-store-service/tests/methods/method_refusal_order.rs", "op": "move", "why": "surface/ is a rejected word; method-table area mirrors src methods/"},
  {"from": "engine/crates/layerfs-store-service/tests/surface/method_golden.rs", "to": "engine/crates/layerfs-store-service/tests/methods/method_golden.rs", "op": "move", "why": "method-table area"},
  {"from": "engine/crates/layerfs-store-service/tests/surface/sdk_methods.txt", "to": "engine/crates/layerfs-store-service/tests/methods/sdk_methods.txt", "op": "move", "why": "golden of method_golden"},
  {"from": "engine/crates/layerfs-store-service/tests/surface/branch_count.rs", "to": "engine/crates/layerfs-store-service/tests/methods/branch_count.rs", "op": "move", "why": "method-table area"},
  {"from": "engine/crates/layerfs-store-service/tests/surface/project_count.rs", "to": "engine/crates/layerfs-store-service/tests/methods/project_count.rs", "op": "move", "why": "method-table area"},
  {"from": "engine/crates/layerfs-store-service/tests/surface/fixed_costs.rs", "to": "engine/crates/layerfs-store-service/tests/methods/fixed_costs.rs", "op": "move", "why": "method-table area"},
  {"from": "engine/crates/layerfs-store-service/tests/surface/timing_coverage.rs", "to": "engine/crates/layerfs-store-service/tests/methods/timing_coverage.rs", "op": "move", "why": "method-table area"},
  {"from": "engine/crates/layerfs-store-service/tests/surface/main.rs", "to": "engine/crates/layerfs-store-service/tests/methods/main.rs", "op": "move", "why": "area root follows the area"},
  {"from": "engine/crates/layerfs-store-service/tests/surface/admission.rs", "to": "engine/crates/layerfs-store-service/tests/removal/admission.rs", "op": "move", "why": "admission of branch.remove/project.delete/remove_file: a removal topic"},
  {"from": "engine/crates/layerfs-store-service/tests/surface/epoch_fence.rs", "to": "engine/crates/layerfs-store-service/tests/commit/epoch_fence.rs", "op": "move", "why": "writer-side fence on the Workspace Commit path"},
  {"from": "engine/crates/layerfs-store-service/tests/surface/kernel_guard.rs", "to": "engine/crates/layerfs-store-service/tests/store/kernel_guard.rs", "op": "move", "why": "Store opening; new store/ area mirrors src store/"},
  {"from": "engine/crates/layerfs-store-service/tests/write/install_directory.rs", "to": "engine/crates/layerfs-store-service/tests/store/install_directory.rs", "op": "move", "why": "tests install::prepare, not a write"},
  {"from": "engine/crates/layerfs-store-service/tests/read/object_demand_errors.rs", "to": "engine/crates/layerfs-store-service/tests/store/object_demand_errors.rs", "op": "move", "why": "Store object-demand failure kinds"},
  {"from": "", "to": "engine/crates/layerfs-store-service/tests/store/main.rs", "op": "new", "why": "area binary root for tests/store/"},
  {"from": "engine/crates/layerfs-store-service/tests/read", "to": "engine/crates/layerfs-store-service/tests/snapshot", "op": "rename", "why": "read/ is a rejected word; mirrors src snapshot/"},
  {"from": "engine/crates/layerfs-store-service/tests/history/surface.rs", "to": "engine/crates/layerfs-store-service/tests/history/listing.rs", "op": "rename", "why": "rejected word; tests history listing pages"},
  {"from": "engine/crates/layerfs-store-service/tests/history/operator_history.rs", "to": "engine/crates/layerfs-store-service/tests/history/get_history.rs", "op": "rename", "why": "repeats folder; named after backend.store.project.get_history"},
  {"from": "engine/crates/layerfs-store-service/tests/compare/compare.rs", "to": "engine/crates/layerfs-store-service/tests/compare/wide_namespaces.rs", "op": "rename", "why": "file repeats folder name"},
  {"from": "engine/crates/layerfs-store-service/tests/allocators/unknown_allocators.rs", "to": "engine/crates/layerfs-store-service/tests/allocators/unknown.rs", "op": "rename", "why": "9 files share the prefix; drop it"},
  {"from": "engine/crates/layerfs-store-service/tests/allocators/unknown_allocators_cold_a.rs", "to": "engine/crates/layerfs-store-service/tests/allocators/cold_a.rs", "op": "rename", "why": "drop shared prefix"},
  {"from": "engine/crates/layerfs-store-service/tests/allocators/unknown_allocators_cold_b.rs", "to": "engine/crates/layerfs-store-service/tests/allocators/cold_b.rs", "op": "rename", "why": "drop shared prefix"},
  {"from": "engine/crates/layerfs-store-service/tests/allocators/unknown_allocators_cold_c.rs", "to": "engine/crates/layerfs-store-service/tests/allocators/cold_c.rs", "op": "rename", "why": "drop shared prefix"},
  {"from": "engine/crates/layerfs-store-service/tests/allocators/unknown_allocators_cold_d.rs", "to": "engine/crates/layerfs-store-service/tests/allocators/cold_d.rs", "op": "rename", "why": "drop shared prefix"},
  {"from": "engine/crates/layerfs-store-service/tests/allocators/unknown_allocators_raced_a.rs", "to": "engine/crates/layerfs-store-service/tests/allocators/raced_a.rs", "op": "rename", "why": "drop shared prefix"},
  {"from": "engine/crates/layerfs-store-service/tests/allocators/unknown_allocators_raced_b.rs", "to": "engine/crates/layerfs-store-service/tests/allocators/raced_b.rs", "op": "rename", "why": "drop shared prefix"},
  {"from": "engine/crates/layerfs-store-service/tests/allocators/unknown_allocators_raced_c.rs", "to": "engine/crates/layerfs-store-service/tests/allocators/raced_c.rs", "op": "rename", "why": "drop shared prefix"},
  {"from": "engine/crates/layerfs-store-service/tests/allocators/unknown_allocators_warm_b.rs", "to": "engine/crates/layerfs-store-service/tests/allocators/warm_b.rs", "op": "rename", "why": "drop shared prefix"},
  {"from": "engine/crates/layerfs-store-service/tests/postgresql/postgresql_open_cost.rs", "to": "engine/crates/layerfs-store-service/tests/postgresql/session_open_cost.rs", "op": "rename", "why": "repeats folder; PostgreSQL session open round trips"},
  {"from": "engine/crates/layerfs-store-service/tests/postgresql/tester768_open_cost.rs", "to": "engine/crates/layerfs-store-service/tests/postgresql/service_open_cost.rs", "op": "rename", "why": "campaign name; Store service open cost"},
  {"from": "engine/crates/layerfs-store-service/tests/standalone/writer_reopen_reopen_sweep.rs", "to": "engine/crates/layerfs-store-service/tests/standalone/writer_reopen_sweep.rs", "op": "rename", "why": "doubled word"},
  {"from": "engine/crates/layerfs-store-service/tests/standalone/read_fs_reads_replay.rs", "to": "engine/crates/layerfs-store-service/tests/standalone/snapshot_reads_replay.rs", "op": "rename", "why": "area read/ becomes snapshot/"}
]
```
