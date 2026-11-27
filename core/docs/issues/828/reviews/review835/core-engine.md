# Review 835: core and engine areas #818 touched

Scope: core `layerfs-storage` (`read/` and its ripple), core `layerfs-content` `compare/`,
engine `layerfs-persistence` (both `acquisition/` folders, `backend/acquisition.rs`,
PostgreSQL `history/` and what it reaches), engine `layerfs-store-service` (`history/`,
`read/` → `filesystem/`, `write/` → `filesystem/change/`). Source: `S/v835tree`. LOC:
`S/loc_835.txt` (`prod` column). Plan: `S/sim835/site/layout.json`.

## 1. Verdict

**No crate-level change is needed. Two folders do need a more destructive regroup than
the plan makes.** Neither adds a crate, merges crates or moves code across crates.

- **Storage `read/`.** It splits along the type boundary that already exists in the
  code. Ten files are `impl Fetch` blocks over `Fetch`'s `pub(super)` fields. They become
  `read/source/`. The demand path stays loose (provider, objects, prefetch, length).
- **Persistence.** #818 added a fifth piece of code both backends share: the placement
  algorithm in `sqlite/acquisition/placement.rs`, imported by PostgreSQL. With it, the
  settled `backend/acquisition.rs` would mix five topics in about 260 LOC. **I reopen that
  decision.** The neutral acquisition code becomes a root `acquisition/` folder, beside
  the root `history/` and `storage/` the crate already has for the other two ports.
  SQLite's `child_windows.rs` then takes the name `placement.rs`, so both backends'
  `placement.rs` do the same job.

Everything else is either right as planned or needs only one file moved or renamed.

## 2. Findings, ranked by value

### F1. Storage `read/`: 14 files, split by the `Fetch` type (rules 1, 9)

**What is wrong.** There are 14 files in a leaf folder, over the 12 limit. The proposed
`locate/` + `demand/` split cuts through one type.

**Evidence: `impl` blocks.** Ten files are one type, `Fetch`, declared in `read/fetch.rs:100`:

| File | `impl Fetch` | `Fetch` field it serves |
| --- | --- | --- |
| `captured.rs` | `:228` | `captured_locators` |
| `held.rs` | `:32` | `held` |
| `hints.rs` | `:118` | `hints` |
| `joined.rs` | `:14` | — |
| `per_object.rs` | `:37` | `refused` |
| `values.rs` | `:11` | — |

- `locator_cache.rs` is the `locators: RefCell<LocatorCache>` field.
- `counters.rs` is the `counters: Cell<Diagnostics>` field (`Fetch::note`).
- `units.rs` (`GroupPlan`) is used only by `fetch.rs:744` and `hints.rs:10`.

**Evidence: visibility.** `Fetch` has 14 `pub(super)` fields:

- locators, absent, descriptors, published, held, serve_published;
- compact_inode_packs, paired_inode_values, captured_locators, hints;
- bind_hints, bind_pages, trust_hints, refused.

It also has `pub(super)` helpers: `check_info`, `validate_frame`, `bad_position`,
`keep_locator`, `published_body`, `remember_pack`. All their users are inside those ten
files.

**Evidence: the demand files.** The other four use only `Fetch`'s `pub(crate)` API:

- `objects.rs`: `ReadState`, `Decoder`, `call_failure`. It calls `begin_demand`, `locate`,
  `page`, `cohort`, `whole_packs`, `drop_hints`.
- `prefetch.rs`: `chains`. It calls `prefetch_groups`, `refused_pack`, `refuse`,
  `has_hint`, `chain_ahead`, `unpacked`.
- `provider.rs`: `Reader`, `ReadCache`, `KeptReads`, `impl Storage`.
- `length.rs`: called only by `provider.rs`.

**Why the proposed split fails.** It would put `per_object.rs`, `values.rs` and
`captured.rs` (all `impl Fetch`) in `demand/`, away from the struct in `locate/`. About
15 `pub(super)` items would then have to widen to `pub(crate)`. It would also put
`objects.rs`, which uses only public `Fetch` calls, beside them.

**Why `Fetch` stays in `read/`.** `Fetch` is `Storage.source` (`store/handle.rs:17`). Save
uses it too: `save/source.rs` wraps `&Fetch`, `save/wave.rs` and `save/provider.rs` call
`read::prefetch_chains`, and `encoding/pool/read.rs:451` takes `&read::Fetch`. That is
still "reading committed objects" whoever asks, so `read/` stays one domain. No lift to
the crate root.

**Fix.**

- `read/{fetch,locator_cache,captured,held,hints,per_object,values,joined,units,counters}.rs`
  → `read/source/<same name>.rs`.
- `read/` keeps `provider.rs`, `objects.rs`, `prefetch.rs` and `length.rs` loose. That is
  4, the branch limit.

**Name.** `source/` is the crate's own word for this object:

- the field is `Storage.source`;
- `fetch.rs:696` holds `impl Source for Fetch`;
- Save's counterpart is `save/source.rs` (`WaveSource`, the Save's `Source`).

`read/fetch/` would force `fetch/fetch.rs` (rule 14). `cache/` would collide with the
public `ReadCache`.

**Cost / gain.** 10 file moves, one new `source/mod.rs`, and import edits in `read/mod.rs`
and in `save/source.rs` (`read::Fetch` → `read::source::Fetch`, or keep the `read/mod.rs`
re-export). No visibility changes. The gain is a 4 + 10 layout that follows the type
boundary, with room to grow.

**Fallback if no subfolder is wanted.** Merge `values.rs` into `per_object.rs` (both
"#818 H5 per-object demand", 159 LOC) and `joined.rs` into `held.rs` (both Save-side pack
state, 161 LOC). That gives exactly 12 files, so the next file breaks it again. Not
recommended.

**Ripple on the rest of storage: none.**

- The plan's `store/source.rs` → `location.rs` merge still holds. The trait lives in
  `location.rs`, its implementations in `read/source/fetch.rs` and `save/source.rs`.
- `pack/units.rs` and `read/source/units.rs` keep one word for complete record groups, as
  review 2 ruled.
- Root keeps 4 loose files: error, handle, location, policy.
- **The plan's LOC for `read/` rows is stale.** For example fetch 873 → 810, hints
  684 → 776, objects 199 → 438, length 66 → 124, prefetch 85 → 219, provider 98 → 160.
  Refresh from `loc_835.txt`.

### F2. Persistence: shared acquisition code becomes a root `acquisition/` (reopens settled `backend/acquisition.rs`)

**What is wrong.**

- **#818 left neutral code inside `sqlite/` again.** `sqlite/acquisition/placement.rs`
  says it is "the algorithm both backends run, with each backend's statements in its own
  module" (header, lines 1–2). PostgreSQL imports it at
  `postgresql/acquisition/placement.rs:16`
  (`sqlite::acquisition::placement::{plan, Placement}`) and at `provider.rs:191`.
- **#818 also added `flights.rs:32`** importing `sqlite::acquisition::accounting::ROOT_BYTES`.
- **The plan's target would hold five topics in one file.** Adding these to
  `backend/acquisition.rs` gives about 260 LOC:
  - the session epoch (`backend/epoch.rs`, 39);
  - the unit `Failure` type with `signed`/`unsigned` (from `statements.rs`, about 40);
  - row payload charges (`ROOT_BYTES`, `HOST_BYTES`, `entry_bytes`, `host_bytes`, about 18);
  - stored-row encodings (`metadata_blob`/`metadata_parse` 41, `packed`/`well_formed`
    about 20);
  - the placement window algorithm (103).
- **That fails rule 11.** The name no longer predicts the contents, and the
  algorithm is the largest part.
- **A folder in `backend/` is not the answer either.** `backend/acquisition/` would sit
  among the `postgresql/` and `sqlite/` variants (rule 10). `backend/` is also already at
  4 loose files: admission, acquisition, error, seam.

**Evidence that a root folder is the crate's existing pattern.** The crate implements
three core ports:

- storage `PackPersistence`: neutral parts in root `storage/` (`blocks/`, `metadata/`,
  `merge_records.rs`);
- history provider: neutral code in root `history/`, which the backends import, for
  example `postgresql/compact/op.rs` uses `crate::history::{GcTuple, Reach}`;
- storage `port::acquisition`: no neutral home today.

The engine-specific mechanics of each sit under `backend/<engine>/<domain>/`. A root
`acquisition/` completes the pattern (rules 21, 23).

**Fix.**

| Planned | Proposed | What it holds |
| --- | --- | --- |
| `backend/acquisition.rs` | `src/acquisition/epoch.rs` | the `Epoch`, from `backend/epoch.rs` |
| `backend/acquisition.rs` | `src/acquisition/failure.rs` (new) | `Failure`, `signed`, `unsigned`, from `sqlite/acquisition/statements.rs` |
| `backend/acquisition.rs` | `src/acquisition/payload.rs` | `metadata.rs` plus the byte charges from `accounting.rs` and `packed`/`well_formed` from `writes.rs` |
| `backend/sqlite/acquisition/placement.rs` | `src/acquisition/placement.rs` | the shared window plan |

- **Why `payload.rs`.** "A row's payload is its variable bytes", from the `accounting.rs`
  header. `metadata.rs` clashed with `sqlite/metadata/`; `rows.rs` would echo storage
  `port/acquisition/rows.rs`.
- **Callers.** `diagnostic/check/acquisition.rs` imports `metadata_parse` from
  `crate::acquisition::payload`, and `lib.rs` is unchanged.
- **No name clash.** After the plan's `gc/epoch.rs` → `gc/fence.rs` rename,
  `acquisition/epoch.rs` clashes with nothing.

**Cost / gain.** The plan already splits the same five sources. Only the target changes:
4 files in one folder instead of one file, plus `mod.rs`. Gains:

- `backend/` drops to 3 loose files;
- the placement algorithm is findable by name;
- PostgreSQL stops importing anything from `sqlite::`. Today 7 PostgreSQL acquisition
  files do.

### F3. SQLite `child_windows.rs` → `placement.rs`: one job, one name in both backends (rule 9)

**What is wrong.** Today `placement.rs` means two different jobs:

- in SQLite, the shared algorithm;
- in PostgreSQL, its statements: "Wide-directory placement on PostgreSQL: one statement
  per set window (#818 I2a), cut and decoded by the shared algorithm".

SQLite's statements for the same job are in `child_windows.rs`: "SQLite's wide-directory
placement statements (#818 I2b): the shared window rule and outcome decoding
(`placement.rs`) over fixed 32-row inputs".

**Fix.** After F2 moves the algorithm out, rename `backend/sqlite/acquisition/child_windows.rs`
→ `backend/sqlite/acquisition/placement.rs`.

**Result.** Both acquisition folders share one design:

- the same names: accounting, cleanup, placement, provider, reads, writes;
- the difference fits in one line: "SQLite binds fixed 32-row inputs (`entry_windows`,
  `root_windows`) and keeps a closed `statements` set; PostgreSQL pipelines units into
  round trips (`flights`, `unit`)";
- counts: SQLite 9, PostgreSQL 8, both inside 3–12.

**Cost / gain.** One rename and two `use` lines (`writes.rs`, `provider.rs`). It removes
the only same-name-different-job pair in the two folders.

### F4. PostgreSQL attempt token lives in a Commit file: `nonce()` → `session/attempt.rs` (rule 23)

**What is wrong.** `history/fused.rs:132` holds `nonce()`, the per-attempt token behind
`set_config('layerfs.attempt', …)` and the token-checked `COMMIT`. Its hash domain is still
`"layerfs/fused-commit/v1"`. Six files outside the Commit import it from there.

**Evidence.**

- `history/{mount,release,reserve,binding_append}.rs`. `reserve.rs` is new in #818.
- `objects/publish_flight.rs:3`.
- `acquisition/flights.rs:24`, new in #818: the acquisition domain imports from the
  history domain's Commit file.

It is session mechanics, like `session/profile.rs::barrier_leftover`, which the same
files import.

**Fix.** Split `nonce` out of `backend/postgresql/history/fused.rs` into a new
`backend/postgresql/session/attempt.rs`. `session/` becomes 10 files after the plan's
`query.rs` merge. The name follows the SQL setting `layerfs.attempt`.

**Cost / gain.** One new file and seven `use` edits. One concept then has one place.
**Not fixed here:** the duplicated `BEGIN … set_config … token check` text in
fused/mount/flights/publish_flight. That is a code change, not layout.

### F5. Pack statements live in the Commit file: → `objects/publish_flight.rs` (rule 23; pre-#818, in a touched folder)

**What is wrong.** `objects/publish_flight.rs:3` imports `pack_statements`, `row_values`,
`PACK` and `ROWS` from `history/fused.rs`. `row_values` itself reaches back for
`objects::publish::prefix` (`fused.rs:425`). The pack and location statements belong to
objects; the Commit only sends them in its message.

**Fix.** Move `PACK`, `ROWS`, `pack_statements` and `row_values` (about 80 LOC) from
`backend/postgresql/history/fused.rs` into `backend/postgresql/objects/publish_flight.rs`.
`fused.rs` then imports them from objects.

**Cost / gain.** Two files edited. The dependency then runs one way, history → objects.
The `.sql` files stay under `sql/postgresql/fused/`, outside the `src/` layout. Rank it
below F1–F4: it predates #818 and is a cross-folder edge, not a count violation.

### F6. Store-service `filesystem/` has 5 loose files: move `reply.rs` into `paged/`, keep `versions.rs` loose (rules 3, 22)

**What is wrong.** The plan puts `cache`, `facts`, `reply`, `root` and `versions` loose in
`filesystem/`, one over the branch limit.

**Evidence for `reply.rs`** (was `read/read_reply.rs`):

- its only caller is `read/paged_reads.rs:42` (→ `filesystem/paged/results.rs`);
- it imports `query` (→ `paged/query.rs`), `file_facts` and `window`;
- header: "Reserve saved-reference/header space before returning a bounded text window".

So it belongs to the paged reads.

**Evidence against `history/versions.rs`.**

- **Every user of `versions.rs` is a filesystem read:**
  - `read/blame.rs` (→ `text/blame.rs`);
  - `read/attribution.rs` (→ `text/`);
  - `read/revision.rs` (→ `paged/revision.rs`);
  - `read/file_compare.rs` (→ `compare/file_rows.rs`);
  - the file-family calls `blame`, `diff_files` and `revision`.
- **It imports `super::cached::{batches, Cached, ROOTS}`** (→ `filesystem/cache.rs`) and
  `layerfs_content::compare::PathAcross`. It does path lookups in many roots, with a
  directory memo.
- **`history/rows.rs` is only its input.** `rows.rs` is the per-call Commit/Layer row memo.
- **`history/versions.rs` would point the wrong way.** `history/` would then import
  filesystem's cache. It would sit away from all four of its users, under the domain
  whose job is the Commit graph.
- **Its users span three subfolders** (`text/`, `paged/`, `compare/`). By rule 3, a file
  shared that way is a loose file of the parent.

**Fix.** `engine/.../store-service/src/filesystem/reply.rs` →
`filesystem/paged/reply.rs`. `filesystem/` then has 4 loose files (cache, facts, root,
versions) and `paged/` 7.

**Cost / gain.** One move and two `use` lines. It meets rule 3 without inventing a group.

## 3. Proposed layouts of changed folders

Scope: v835 tree (`S/v835tree`, branch `layerfs_v2_0_0` with #818, plus #835).

- **LOC.** Production LOC from `S/loc_835.txt` (the `tools/production_loc.py --files`
  `prod` column).
- **Paths.** Planned names from `S/sim835/site/layout.json` where the plan already
  renames.
- **Splits.** A split file keeps its current LOC; the files it feeds show `—`.
- **Partial folders.** Where only part of a folder is shown, the annotation says so.

`core/crates/layerfs-storage/src/`:

```diff
  read/                                       [3500]
    mod.rs                                       22   (declares source/; re-exports unchanged)
    length.rs                                   124   (file-length facts; only provider.rs calls it)
    objects.rs                                  438   (ReadState, Decoder, call_failure: one demand)
    prefetch.rs                                 219   (chains; save/ calls it too)
    provider.rs                                 160   (Reader, ReadCache, KeptReads, impl Storage)
-   captured.rs                                 293   (moves to source/)
-   counters.rs                                  59   (moves to source/)
-   fetch.rs                                    810   (moves to source/)
-   held.rs                                      87   (moves to source/)
-   hints.rs                                    776   (moves to source/)
-   joined.rs                                    74   (moves to source/)
-   locator_cache.rs                            160   (moves to source/)
-   per_object.rs                                72   (moves to source/)
-   units.rs                                    119   (moves to source/)
-   values.rs                                    87   (moves to source/)
+   source/                                       —   new (Fetch, Storage.source: struct and its impl blocks)
+     mod.rs                                      —   new
+     captured.rs                                 —   (from read/, 293: impl Fetch + CapturedLocators)
+     counters.rs                                 —   (from read/, 59: Diagnostics, Fetch.counters)
+     fetch.rs                                    —   (from read/, 810: struct Fetch, impl Source for Fetch)
+     held.rs                                     —   (from read/, 87: impl Fetch, field held)
+     hints.rs                                    —   (from read/, 776: impl Fetch, field hints)
+     joined.rs                                   —   (from read/, 74: impl Fetch)
+     locator_cache.rs                            —   (from read/, 160: field locators)
+     per_object.rs                               —   (from read/, 72: impl Fetch, field refused)
+     units.rs                                    —   (from read/, 119: GroupPlan, used by fetch + hints)
+     values.rs                                   —   (from read/, 87: impl Fetch)
```

(`read/` subtotal counts each moved file once, at its current path; `source/` will hold
2537.)

`engine/crates/layerfs-persistence/src/` (partial: `acquisition/`, `backend/` loose files,
both acquisition folders, PostgreSQL `history/` and the touched files of `session/` and
`objects/`):

```diff
+ acquisition/                                [183]  new (neutral rules both acquisition providers share; partial: split parts not counted)
+   mod.rs                                        —   new
    epoch.rs                                     39   (moved in from backend/epoch.rs; the plan had backend/acquisition.rs)
+   failure.rs                                    —   new (Failure, signed, unsigned out of sqlite/acquisition/statements.rs)
    payload.rs                                   41   (was sqlite/acquisition/metadata.rs; gains byte charges from accounting.rs, packed/well_formed from writes.rs)
    placement.rs                                103   (moved in from sqlite/acquisition/placement.rs: the window plan both backends run)
  backend/                                    [523]  (partial: loose files only)
    admission.rs                                121
    error.rs                                    214   (plan: was records.rs)
    seam.rs                                     188
-   acquisition.rs                                —   (plan target, never created: becomes src/acquisition/)
    sqlite/
      acquisition/                           [1772]
        mod.rs                                   13
        accounting.rs                            74   (split: byte charges leave for src/acquisition/payload.rs; owner/charge/credit stay)
        cleanup.rs                              190
        entry_windows.rs                        236
        placement.rs                            192   (renamed from child_windows.rs: SQLite's placement statements)
-       child_windows.rs                          —   (renamed to placement.rs, above)
-       metadata.rs                              41   (moves to src/acquisition/payload.rs)
-       placement.rs                            103   (the shared plan: moves to src/acquisition/placement.rs; its name passes to child_windows.rs)
        provider.rs                             231
        reads.rs                                364
        root_windows.rs                          56
        statements.rs                            89   (split: Failure/signed/unsigned leave for src/acquisition/failure.rs)
        writes.rs                               183   (split: packed/well_formed leave for src/acquisition/payload.rs)
    postgresql/
      acquisition/                           [2012]
        mod.rs                                    8
+       accounting.rs                             —   new (plan: owner check and charges out of writes.rs)
        cleanup.rs                              151
        flights.rs                              295   (ROOT_BYTES from src/acquisition/payload.rs; nonce from session/attempt.rs)
        placement.rs                            177   (plan from src/acquisition/placement.rs)
        provider.rs                             293
        reads.rs                                311
        unit.rs                                 296
        writes.rs                               481   (split per plan: accounting.rs)
      history/                               [1267]
        mod.rs                                    6
        binding_append.rs                        96
        fused.rs                                447   (split: nonce → session/attempt.rs; PACK/ROWS/pack_statements/row_values → objects/publish_flight.rs)
        mount.rs                                309
        release.rs                              132
        reserve.rs                              142   (#818 W4: inode reservation in one message; stays)
        statements.rs                           135
      objects/                                [141]  (partial: one file shown)
        publish_flight.rs                       141   (gains the pack/row statements from history/fused.rs)
      session/                                  [0]  (partial: new file only)
+       attempt.rs                                —   new (nonce: the layerfs.attempt token every pipelined write unit sets)
```

`engine/crates/layerfs-store-service/src/filesystem/` (partial: loose files and `paged/`):

```diff
  filesystem/                                [2053]  (partial)
    cache.rs                                    300   (was read/cached.rs; +212 since plan, still one call's caches + memo-bound lookups)
    facts.rs                                    129   (was read/file_facts.rs)
    root.rs                                     362   (was read/snapshot.rs; +150 since plan, still roots + path lookups)
    versions.rs                                 217   (was read/versions.rs; stays loose: users in text/, paged/, compare/)
-   reply.rs                                     78   (moves to paged/)
    paged/                                    [967]
      boundaries.rs                              42
      continuation.rs                            72
      query.rs                                   62
+     reply.rs                                    —   (moved in from filesystem/reply.rs, 78; only caller results.rs)
      results.rs                                413
      revision.rs                               307
      revision_boundaries.rs                     71
```

`core/crates/layerfs-content/src/` (optional rename only):

```diff
  compare/                                   [2317]
    mod.rs                                       23
    bytes.rs                                    127
    changes.rs                                  334
    memo.rs                                      95
    named.rs                                    153
    pages.rs                                    251   (plan: was sorted.rs)
    text.rs                                     207
    tree.rs                                     626
    versions.rs                                 501   (renamed from across.rs, optional: VersionPath, one path in many versions; same word as its only caller)
-   across.rs                                     —   (renamed to versions.rs)
```

## 4. Checked and left alone

- **Content `compare/across.rs` stays in `compare/`. A more destructive move is not
  justified.**
  - **Its job is resolution, not comparison.** Its header: "One path resolved across
    many versions' roots, level by level".
  - **Its imports tie it to `compare/tree.rs`:** `super::tree::{EntryFacts, TreeTable,
    DIRECTORY_WINDOW}`.
  - **`compare/` already hosts the shared tree-reading primitives:** `TreeTable`,
    `locate`, `list_directory`, `subtree`, `EntryFacts`.
  - **Moving it beside `filesystem/read.rs` would mean splitting those primitives out of
    `compare/tree.rs` first.** About 40 importers across core, engine and control use
    them (merge, statistics, store-service, daemon, project, control store export). That
    is too much churn for a 501-LOC file.
  - **`compare/` holds 8 files after the plan.** The only change I propose is the
    optional rename to `versions.rs`: "across" is a preposition (rule 16). The caveat is
    that the content crate also uses "version" for format versions (`file/mapping/codec.rs`).
  - **`DirectoryMemo` could join `memo.rs`.** That would be a small follow-up, not needed.
- **Store-service: the five-folder layout holds.** The root folders are `families/`,
  `filesystem/`, `history/`, `store/` and `workspace/`, with `lookup.rs`, `service.rs` and
  `telemetry.rs` loose. #818 added no file outside `history/`, `read/` and `write/`.
  - **`history/rows.rs` stays in `history/`.** It is the per-call Commit/Layer/Branch-name
    row memo. Its users are `history/saved.rs`, blame, revision, attribution and
    versions. `history/` has 10 files.
  - **`write/plan.rs` → `filesystem/change/plan.rs` is right.** It uses `files::Change`,
    `mutation` and `recursive`, and only `write/files.rs:395` calls it. "plan" also
    appears in `compare/plans.rs` with another meaning. The code's own word is
    `Mutation::plan` / `PLAN_PATHS`, so I keep it.
  - **`cache.rs` (+212) and `root.rs` (+150) still match their headers.** `Cached::lookup`
    delegates to `root.rs::lookup_bound`. That is one boundary, not a scattered concern.
- **Persistence PostgreSQL `history/reserve.rs` stays.** It is the inode reservation,
  called from `history/catalog.rs:342`, with its text prepared in `mount.rs:162`.
  `history/` has 6 files.
- **`flights.rs` and `unit.rs` stay.** They are PostgreSQL-only pipelining, with
  acquisition SQL in both, so the difference from SQLite is explainable. `unit.rs` is a
  vague word in a folder where every operation is a "unit", but I found no better domain
  noun.
- **Follow-up, code not layout:** `ENTRY_ROW_BYTES`, `BINDING_ROW_BYTES`,
  `UNPLACED_ROW_BYTES` and `FILE_ROOT_ROW_BYTES` are duplicated in both `reads.rs` files.
  They could join `src/acquisition/payload.rs`.
- **Noticed outside the #818 scope:** the plan's `backend/sqlite/compact/` holds 14 files,
  over the 12 aim and under the 20 ceiling.

## 5. Machine-readable moves

```json
[
  {"from": "core/crates/layerfs-storage/src/read/fetch.rs", "to": "core/crates/layerfs-storage/src/read/source/fetch.rs", "op": "move", "why": "struct Fetch (Storage.source); read/ had 14 files"},
  {"from": "core/crates/layerfs-storage/src/read/locator_cache.rs", "to": "core/crates/layerfs-storage/src/read/source/locator_cache.rs", "op": "move", "why": "Fetch's locators field"},
  {"from": "core/crates/layerfs-storage/src/read/captured.rs", "to": "core/crates/layerfs-storage/src/read/source/captured.rs", "op": "move", "why": "impl Fetch over pub(super) captured_locators"},
  {"from": "core/crates/layerfs-storage/src/read/held.rs", "to": "core/crates/layerfs-storage/src/read/source/held.rs", "op": "move", "why": "impl Fetch over pub(super) held"},
  {"from": "core/crates/layerfs-storage/src/read/hints.rs", "to": "core/crates/layerfs-storage/src/read/source/hints.rs", "op": "move", "why": "impl Fetch over pub(super) hints"},
  {"from": "core/crates/layerfs-storage/src/read/per_object.rs", "to": "core/crates/layerfs-storage/src/read/source/per_object.rs", "op": "move", "why": "impl Fetch over pub(super) refused"},
  {"from": "core/crates/layerfs-storage/src/read/values.rs", "to": "core/crates/layerfs-storage/src/read/source/values.rs", "op": "move", "why": "impl Fetch"},
  {"from": "core/crates/layerfs-storage/src/read/joined.rs", "to": "core/crates/layerfs-storage/src/read/source/joined.rs", "op": "move", "why": "impl Fetch"},
  {"from": "core/crates/layerfs-storage/src/read/units.rs", "to": "core/crates/layerfs-storage/src/read/source/units.rs", "op": "move", "why": "GroupPlan used only by fetch.rs and hints.rs"},
  {"from": "core/crates/layerfs-storage/src/read/counters.rs", "to": "core/crates/layerfs-storage/src/read/source/counters.rs", "op": "move", "why": "Diagnostics is Fetch's counters field"},
  {"from": "", "to": "core/crates/layerfs-storage/src/read/source/mod.rs", "op": "new", "why": "declarations for the Fetch group"},
  {"from": "engine/crates/layerfs-persistence/src/backend/acquisition.rs", "to": "engine/crates/layerfs-persistence/src/acquisition", "op": "split", "why": "reopened: five shared topics (~260 LOC incl. #818 placement) get a neutral root folder like history/ and storage/"},
  {"from": "", "to": "engine/crates/layerfs-persistence/src/acquisition/mod.rs", "op": "new", "why": "declarations"},
  {"from": "", "to": "engine/crates/layerfs-persistence/src/acquisition/epoch.rs", "op": "new", "why": "session acquisition Epoch (was backend/epoch.rs)"},
  {"from": "", "to": "engine/crates/layerfs-persistence/src/acquisition/failure.rs", "op": "new", "why": "unit Failure, signed, unsigned shared by both backends"},
  {"from": "", "to": "engine/crates/layerfs-persistence/src/acquisition/payload.rs", "op": "new", "why": "metadata blob, packed host identity, well_formed, row payload charges"},
  {"from": "engine/crates/layerfs-persistence/src/backend/sqlite/acquisition/placement.rs", "to": "engine/crates/layerfs-persistence/src/acquisition/placement.rs", "op": "move", "why": "the window plan both backends run; PostgreSQL imports it"},
  {"from": "engine/crates/layerfs-persistence/src/backend/sqlite/acquisition/child_windows.rs", "to": "engine/crates/layerfs-persistence/src/backend/sqlite/acquisition/placement.rs", "op": "rename", "why": "SQLite's placement statements; same name and job as postgresql/acquisition/placement.rs"},
  {"from": "engine/crates/layerfs-persistence/src/backend/sqlite/acquisition/statements.rs", "to": "engine/crates/layerfs-persistence/src/acquisition/failure.rs", "op": "split", "why": "Failure/signed/unsigned leave; closed SQLite statement set stays"},
  {"from": "engine/crates/layerfs-persistence/src/backend/sqlite/acquisition/accounting.rs", "to": "engine/crates/layerfs-persistence/src/acquisition/payload.rs", "op": "split", "why": "ROOT_BYTES/HOST_BYTES/entry_bytes/host_bytes leave; owner/charge/credit stay"},
  {"from": "engine/crates/layerfs-persistence/src/backend/sqlite/acquisition/writes.rs", "to": "engine/crates/layerfs-persistence/src/acquisition/payload.rs", "op": "split", "why": "packed/well_formed leave"},
  {"from": "engine/crates/layerfs-persistence/src/backend/postgresql/history/fused.rs", "to": "engine/crates/layerfs-persistence/src/backend/postgresql/session/attempt.rs", "op": "split", "why": "nonce (layerfs.attempt token) used by history, objects and acquisition"},
  {"from": "", "to": "engine/crates/layerfs-persistence/src/backend/postgresql/session/attempt.rs", "op": "new", "why": "attempt token of every pipelined write unit"},
  {"from": "engine/crates/layerfs-persistence/src/backend/postgresql/history/fused.rs", "to": "engine/crates/layerfs-persistence/src/backend/postgresql/objects/publish_flight.rs", "op": "split", "why": "PACK/ROWS/pack_statements/row_values are objects statements; publish_flight imports them today"},
  {"from": "engine/crates/layerfs-store-service/src/filesystem/reply.rs", "to": "engine/crates/layerfs-store-service/src/filesystem/paged/reply.rs", "op": "move", "why": "only caller paged/results.rs, imports paged/query; filesystem/ back to 4 loose"},
  {"from": "core/crates/layerfs-content/src/compare/across.rs", "to": "core/crates/layerfs-content/src/compare/versions.rs", "op": "rename", "why": "optional: noun not preposition; VersionPath; same word as its caller filesystem/versions.rs"}
]
```
