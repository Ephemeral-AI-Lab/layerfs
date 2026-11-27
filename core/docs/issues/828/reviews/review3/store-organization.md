# layerfs-store organization review (#828, review 3)

Scope: the planned rows of `control/crates/layerfs-store/src/` in
`nav/site/layout.json`, checked against the old source in `v2tree/` (base
`1317ebf7ca`). LOC numbers come from `prod_loc_files.txt`
(`tools/production_loc.py --files`). Every placement claim below is backed by
`crate::`/`super::` lines from the old source. This is a planning review
only. No repository or git state was touched.

## 1. Verdict

The four groups (`admin/`, `data/`, `backend/`, `backup/`) are sound and I
leave them as they are. Every folder is within the count and depth limits.
The problems are inside the groups. Four files were put where a name or a
count needed them, not where their `use` lines point:

- `unknown.rs` sits in `project/delete/`.
- `family.rs` and `restored.rs` sit in `backup/jobs/`.
- `StoreLink` sits in `bucket/record.rs`.

Also, the three per-call folders in `data/project/` (`delete/`, `export/`,
`init/`) each name their entry file differently. Fixing these five things
also leaves `family.rs` with one meaning in the crate. The `r2/` and `s3/`
provider folders are thin, but only as thin as the owner's own
`auth`/`settings`/`url` rule makes them. `checks.rs` is the one file that is
not in that rule.

## 2. Findings, ranked by value

### F1. `unknown.rs` does not belong in `project/delete/`, and moving it in only fills that folder to three files

- **Header** (`unknown.rs`): "the `Unknown` detail a GC call can name when
  the worker's answer was lost". It has no delete-specific code.
- **Callers.** Five places call `crate::unknown::named_unknown`:
  - `calls/branch.rs:352`, which becomes `data/branch.rs`
  - `calls/project.rs:73`, which becomes `data/project/hide_path.rs`
  - `project/live.rs:54`, which becomes `data/project/status/live.rs`
  - `project/family.rs:49,266`
  - `project/delete/remote.rs` (5 calls)
- **What the plan creates.** `data/branch.rs` would have to import
  `crate::data::project::delete::unknown`. A Branch call would then depend
  on the project-delete folder.
- **Why it was placed there.** The plan's note says "project/delete leaf (x.rs
  beside x/)". Without `unknown.rs`, `delete/` holds only `call.rs` and
  `remote.rs`. Two files makes it a thin folder, so it looks like
  `unknown.rs` was put there to reach three.
- **Fix, part 1.** Move it to `data/unknown.rs`. All of its callers are under
  `data/`, and `data/` then has three loose files, within the limit of four.
  If the owner would rather keep it next to `words.rs`, the crate root also
  works.
- **Fix, part 2.** Give `delete/` a real third file by splitting
  `project/delete.rs` the way the code already divides.
  - Lines 1–745 are the caller side: `Intent`, `delete_with`, `delete`, and
    the `Local` effects.
  - Lines 746–end are the worker's handlers: `admit_project`,
    `release_project`, `delete_project`, `remove_project_entry` and
    `remove_worker_entry`, called by `project.delete.admit/release/entry`
    frames.
  - Those handlers become `delete/worker.rs`. That matches `init/worker.rs`
    (the old `project/init.rs`), whose header says it holds "the worker's
    half of an Init on another machine".

### F2. `backup/jobs/` holds two files that are not jobs

The plan's own note defines `jobs/` as "backup create and restore are long
jobs with progress and an end; list and heads read them". Two of its files
don't fit that definition:

**`jobs/family.rs` (129) is about connecting buckets.**

- Its header says: "This file connects and disconnects buckets and keeps the
  links; `list`, `create` and `restore` are in their own files."
- Its own functions are `connect`, `rotate`, `disconnect`, `connected` and
  `write_link`. The rest is `pub use` re-exports and a one-line `list`
  forwarder.
- It works on `BucketRecord`, `BucketConnect` and `secret`, which the plan
  places in `backup/bucket/`.

**`jobs/restored.rs` (284) is about project records.**

- Its header says: "A restored Store's project records (#668): the record
  `project.init` writes for each of its projects".
- Only one of its callers is in backup:
  - `backup/restore.rs:6,465` (backup restore)
  - `sqlite/transfer/phases.rs:241` (`complete_existing`, used by
    `store.move`)
  - `project/family.rs:301` (`complete`, used by `project.init`)
  - `layerfs-control/src/doctor/project_target.rs:66` (the project doctor,
    planned for inspect)
  - `layerfs-sandbox/src/lifecycle.rs:172` (`repair_project`)
- It imports `crate::project::record`, `crate::project::add` and
  `crate::identity`.

**Fix:**

- `jobs/family.rs` becomes `backup/bucket/connect.rs`, with `connect` and
  `disconnect`. `bucket/` grows to 11 files.
  - `write_link` moves with `StoreLink` (see F4).
  - The `create`, `restore` and `list` re-exports become `pub use` lines in
    `backup/mod.rs`, or are dropped, because the api import paths change in
    this refactor anyway.
- `restored.rs` moves to `data/project/record/restored.rs`. `record/` grows
  to 4 files and holds all project-record code.
- `jobs/` keeps `create`, `ended`, `list`, `progress` and `restore`. Each of
  these is a job or a read of jobs.

### F3. The per-call folders in `data/project/` don't share one design

Each of the three call folders names its entry file differently:

- **`delete/`** uses `call.rs`. The plan rejects the folder names `call/` and
  `calls/`, and `call.rs` is just as generic. `admin/route/call.rs` already
  uses the same name for something else.
- **`export/`** uses `route.rs`. Its header is
  "`project.export(project, destination, { branch, at, on_progress })`", so it
  is the call itself. The name `route` collides with the entity skeleton's
  `route/` ("how a call reaches it"), which rule 13 forbids.
- **`init/`** has no entry file. The `project.init` call lives in the loose
  file `data/project/family.rs`, which defines `init`, `route`,
  `init_on_worker`, `init_postgresql`, `project_add` and `created`.
  - Outside the crate, only `layerfs_store::project::family::{init,
    ProjectInit}` is used (`api/.../project/init.rs:9`, plus two test
    supports).
  - `family::get` has no caller at this commit. That is out of scope here;
    it moves with the file or is removed as dead code.

**Fix:** name each call folder's entry file `run.rs`:

- `delete/call.rs` becomes `delete/run.rs`.
- `export/route.rs` becomes `export/run.rs`.
- `project/family.rs` becomes `init/run.rs`.

`run` is an accepted name, and persistence already uses `run.rs` the same way
(`backend/*/compact/run.rs`, `backend/*/upgrade/run.rs`). With F1, `delete/`
and `init/` each also have a `worker.rs`.

Afterwards, `data/project/` has three loose files: `frame.rs`, `hide_path.rs`
and `with_users.rs`. `frame.rs` stays loose because its frames are used by
`delete`, `export`, `init`, `status` and the staging files.

### F4. A Store copy's link and object layout are split across `bucket/` and a misnamed file in `copy/`

**`StoreLink` sits in `bucket/record.rs`, but it belongs to the Store.**

- `backup/record.rs` holds two records. One is `BucketRecord`. The other is
  `StoreLink` (lines 336–427), which the header describes as: "Also each
  Store's link to a bucket, which the Store's record holds."
- `StoreLink` records a Store's copy: its file, heads, sha256 and last
  transfer. Its readers are:
  - `record.rs:7,373–387`, which becomes `admin/record/store.rs`
  - `get.rs:14`
  - `live.rs:173`
  - `backup/ended.rs:8`
  - `backup/family.rs` (`write_link`)
- The plan leaves it in `backup/bucket/record.rs`. The Store record would
  then read "a bucket's record" just to find its own field.

**`copy/upload.rs` (7) does no uploading.**

- It holds two metadata keys and `Expected`.
- The upload itself is in `jobs/create.rs`.

**The Store's object layout goes to `bucket/registry.rs` with the provider
list.**

- `layerfs-bucket/src/backup/link.rs` holds two things:
  - `STORE_FILE` and `store_object` ("Store `NAME` is the file
    `NAME/store.sqlite`").
  - The `Provider` enum.
- The plan renames the whole file to `bucket/registry.rs`. But `registry`
  only describes the `Provider` part.

**Fix:**

- Add `copy/link.rs`, holding `StoreLink` (split out of `bucket/record.rs`)
  and `write_link` (from F2).
- Rename `copy/upload.rs` to `copy/object.rs`. Add `STORE_FILE` and
  `store_object` from `link.rs`, so it holds a copy's object name and the
  metadata stored beside it.
- `bucket/registry.rs` then holds only the provider list, which is what the
  plan says it is for.
- `copy/` grows to 5 files (`heads`, `link`, `list`, `load`, `object`) and
  now covers everything its plan note promises: "a Store copy on a bucket".

### F5. `diagnostic.rs` mixes engine entry points for both backends in a backend-neutral folder

- **Header:** "Observational Store engine entry points, without leases or
  writable opens."
- **Contents.** Apart from the re-exports, every public function belongs to
  one backend:
  - SQLite: `sqlite`, `sqlite_schema`, `sqlite_identity`, `sqlite_project`,
    `done_audit`, `mount_holders`.
  - PostgreSQL: `postgresql`, `open_admitted`, `postgresql_project`.
  - The re-exports are `pub use layerfs_persistence::diagnostic::{…}` and
    `POSTGRESQL_HANDSHAKE`.
- **Callers.** No caller chooses by kind inside the store crate:
  - `doctor/store_target.rs:501–628` calls the `sqlite_*` and `postgresql`
    functions directly.
  - `doctor/project_target.rs:239,250` chooses by `StoreRef` itself.
  - `postgresql/check.rs:8` and `connect.rs:14` take only the PostgreSQL
    `ConnectionStep`.
- **Rules.** Rule 23 says "Backend-specific code lives in its backend (each
  Store backend's `diagnostic/`)". The settled persistence layout does exactly
  this.

**Fix:**

- Split the file into `backend/sqlite/diagnostic.rs` and
  `backend/postgresql/diagnostic.rs`.
- The neutral re-exports go to `backend/mod.rs`. Callers could also import
  `layerfs_persistence::diagnostic` directly, since the layer order allows it.
- `admin/maintenance/` keeps `compact`, `compact_report`, `repair` and
  `upgrade`, which are all calls that change the Store in place.
- Both backend folders gain the same file name for the same job (rule 9).

### F6. Two `heads.rs` files in neighbouring backup folders do different jobs

- `copy/heads.rs` computes a digest of a Store file and defines the `HEADS`
  metadata key.
- `jobs/heads.rs` reads a live Store's heads where the Store is, through the
  SQLite `StoreStep::Heads` worker step.
- `jobs/heads.rs` has one caller: `backup/list.rs:3`
  (`super::heads::store_heads`).

**Fix:** merge `jobs/heads.rs` (28) into `jobs/list.rs` (219 → about 247).
That leaves one `heads.rs` in the crate.

### F7. Providers: are they padded, and does `local_folder` match the others?

**Are `r2/` and `s3/` padded?** Not beyond what the owner's rules require.

- Their 70 and 38 LOC count only the existing `provider.rs` code that moves
  in.
- The new files will add code that already exists elsewhere:
  - Each `url.rs` gets about 20–25 lines of URL parsing and field mapping
    from `backup/record.rs` (`bucket_record`, `provider`, lines 262–340).
  - Each `auth.rs` gets key handling.
  - Each `checks.rs` gets provider facts.
- On those sources I'd estimate about 80–110 LOC per provider across four
  files. That is thin, but rule 9 itself says "every bucket provider has
  `auth.rs`, `settings.rs`, `url.rs`". So I don't count it as padding.

**`checks.rs` is the one exception.**

1. The owner's rule 9 doesn't list it. It comes from the "checks or driver"
   line in the store-groups comment.
2. Its source, the provider match and TLS requirement in
   `doctor/backup_target.rs:90–115`, is about 20 lines. The plan also moves
   all 322 LOC of that file to `inspect/home/bucket.rs`, so the same lines
   are counted as going to two places.
3. On 2026-11-27 the owner made `local_folder` a real backup target. The
   backup doctor will need its facts too: the folder exists and is a
   directory with 0700 permissions. Under "checks or driver",
   `local_folder` has no place for them.

**Fix:**

- Give `local_folder/` a conditional `checks.rs`. Every provider then has
  `auth`, `checks`, `settings` and `url`, and only `local_folder` adds
  `driver.rs`, because it is the one provider that doesn't use `s3_api/`.
- Re-count the `inspect/home/bucket.rs` row minus the lines that move to the
  providers' `checks.rs` files.

If the owner prefers fewer files, fold each provider's checks into its
`settings.rs`. Either choice gives the three providers one inner design.

### F8 (optional, low). `observed.rs` is mostly PostgreSQL code

- **Header:** "What was seen when a PostgreSQL Store's database was dialled".
- **Kind check.** `record()` returns early unless `kind()` is
  `StoreKind::Postgresql`.
- **The one kind-neutral function** is `seen_at`, an 8-line write of the `at`
  time. Its only caller is `live.rs:271`.
- **Other users** are PostgreSQL code:
  - `postgresql/check.rs` and `connect.rs`
  - the attach path in `with_users`
  - the store doctor
  - `sandbox/route.rs:297`
- **Fix:** move it to `backend/postgresql/observed.rs` and put `seen_at` into
  `admin/status/live.rs`.
- **Why it's optional.** Keeping it in `status/` is also defensible, because
  it is a record of live probes. I would take this only along with F5.

### Side effect: `family.rs` ends with one meaning

At the plan as written, four files are called `family.rs`, with three
different meanings:

- `data/project/family.rs`: the init call.
- `backup/jobs/family.rs`: connecting buckets.
- `backend/sqlite/family.rs` and `backend/postgresql/family.rs`: the Store's
  exclusive `begin`/`settle` steps.

The `postgresql/family.rs` header calls itself "the counterpart of
`sqlite::family::begin` and `settle`", so those two are one meaning.

F2 and F3 remove the first two. The name then survives only in the two
backend folders, where it means the same thing in both. If the naming review
wants it, `exclusive.rs` would describe it better, but that isn't needed for
organization.

## 3. Proposed layout (changed folders only)

Baseline is the plan (`layout.json`). LOC is from
`tools/production_loc.py --files` at `1317ebf7ca`, covering
`control/crates/layerfs-store/src/` only. Tests are not listed.

How to read the listing:

- Folders that were partly shown are marked "partial".
- A `-` line means the file leaves that folder (with `→` naming where it
  goes, or `merged` if it joins another file).
- Moved and renamed files appear without a marker at their destination, with
  "from"/"was" naming the source.
- Each bracketed subtotal adds up every listed file, removed ones included,
  as AGENTS.md requires. So a file that moves inside the listing is counted
  at both ends.
- F8 is not shown.

```diff
  admin/                                [1,494]  (partial: maintenance/ only)
    maintenance/                        [1,494]
      compact.rs                            386
      compact_report.rs                      97
-     diagnostic.rs                         324  (F5: split by backend → backend/*/diagnostic.rs)
      repair.rs                             326
      upgrade.rs                            361
  backend/                              [2,840]
    postgresql/                         [1,347]
      address.rs                            254
      check.rs                              198
      connect.rs                            393
+     diagnostic.rs                           —  new (F5: postgresql, open_admitted, postgresql_project)
      family.rs                              46
      login.rs                               61
      mod.rs                                 14
      open.rs                               208
      reader.rs                             173
    sqlite/                             [1,493]
      create.rs                             162
+     diagnostic.rs                           —  new (F5: sqlite*, done_audit, mount_holders)
      family.rs                             120
      mod.rs                                  9
      move_leftover.rs                      211
      move_phases.rs                        359
      remove.rs                             163
      step_request.rs                       163
      step_run.rs                           306
  backup/                               [3,163]  (partial: s3_api/, r2/, s3/ unchanged)
    bucket/                             [1,178]
      auth.rs                                29
      connect.rs                            129  (F2: was jobs/family.rs; connect, disconnect)
      driver.rs                              98
      error.rs                               47
      key.rs                                 41
      limits.rs                              30
      mod.rs                                 24
      pieces.rs                             381
      progress.rs                            42
+     provider.rs                             —  new (as planned)
      record.rs                             326  (F4: StoreLink split out → copy/link.rs)
      registry.rs                            31  (F4: STORE_FILE, store_object split out → copy/object.rs)
    copy/                                 [290]
      heads.rs                               15
+     link.rs                                 —  new (F4: StoreLink from bucket/record.rs, write_link from jobs/family.rs)
      list.rs                                61
      load.rs                               197
      mod.rs                                 10
      object.rs                               7  (F4: was upload.rs; + STORE_FILE, store_object)
    jobs/                               [1,486]
      create.rs                             276
      ended.rs                               39
-     family.rs                             129  (F2 → bucket/connect.rs)
-     heads.rs                               28  (F6: merged into list.rs, its one caller)
      list.rs                               219  (F6: + store_heads)
      progress.rs                           119
      restore.rs                            392
-     restored.rs                           284  (F2 → data/project/record/)
    providers/                            [209]  (partial: local_folder/ only)
      local_folder/                       [209]
+       auth.rs                               —  new (as planned)
+       checks.rs                             —  cond. (F7: folder exists, is a directory, 0700)
        driver.rs                           207
        mod.rs                                2
+       settings.rs                           —  new (as planned)
+       url.rs                                —  new (as planned)
  data/                                 [4,434]  (partial: branch.rs, commit.rs, file/ unchanged)
    unknown.rs                               32  (F1: from project/delete/; callers branch.rs, project/*)
    project/                            [4,402]  (partial: status/ and loose frame, hide_path, with_users unchanged)
-     family.rs                             283  (F3 → init/run.rs)
      delete/                             [859]
        remote.rs                           186
        run.rs                              641  (F3: was call.rs; F1: worker handlers split out)
-       unknown.rs                           32  (F1 → data/unknown.rs)
+       worker.rs                             —  new (F1: admit/release/delete project, remove entries; from call.rs lines 746–end)
      export/                             [857]
        destination.rs                      178
        mod.rs                                5
        run.rs                              215  (F3: was route.rs; the project.export call)
        walk.rs                             342
        writer.rs                           117
      init/                             [1,634]
        parts.rs                            213
        postgresql.rs                       121
        run.rs                              283  (F3: was project/family.rs; the project.init call)
        send.rs                             346
        sqlite.rs                           241
        staged.rs                           242
        tree.rs                             131
        worker.rs                            57
      record/                             [769]
        discovery.rs                        259
        op.rs                               121
        project.rs                          105
        restored.rs                         284  (F2: from backup/jobs/; project records rebuilt from a Store file)
```

File counts after the changes (mod.rs not counted). All are within 3–12, and
the deepest path is still `src/backup/providers/r2/url.rs`.

| Folder | Files |
| --- | --- |
| `admin/maintenance` | 4 |
| `backend/postgresql` | 8 |
| `backend/sqlite` | 8 |
| `backup/bucket` | 11 |
| `backup/copy` | 5 |
| `backup/jobs` | 5 |
| `data/` | 3 loose files + 2 folders |
| `data/project` | 3 loose files + 5 folders |
| `data/project/delete` | 3 |
| `data/project/export` | 4 |
| `data/project/init` | 8 |
| `data/project/record` | 4 |
| `providers/*` | 4–5 each |

## 4. What I deliberately left alone

- **`admin/`, `data/` and `backend/` as groups, and `admin/with_users.rs`.**
  These are settled, and I found no defect.
  - The worry was that `with_users.rs` reaches up into the sandbox crate
    (`layerfs_sandbox::route::dialled`, `inventory::*`). The `StoreUsers`
    table in `c-control-v2.md` already lists `dialled`, `store_idle_on`,
    `sandboxes_on` and `machine_state`, so that dependency is covered.
- **`record/` and `status/` in both `admin/` and `data/project/`.** This is
  not duplication. These are two entities (the Store and the project), each
  with the shared entity skeleton. The file names line up where the jobs line
  up (`get`, `live`, `state`, `stats`).
  - `listing.rs` is not the same as `admin/status/list.rs`. It is the Store's
    own project listing, while `project.list` is in `get.rs`.
- **`admin/route/methods.rs` (`StoreOp`).**
  - It imports the data families' request types (`super::branch`, `commit`,
    `file`), and the families import it back.
  - But `admin/route/call.rs:8` and `admin/status/stats.rs:4` also take
    `StoreOp`. Its header says "Both routes send these".
  - So it is the route's wire vocabulary. Moving it to `data/` would only
    reverse the loop.
- **Store lifecycle calls inside the backend folders.** `create`, `remove`
  and `move` sit under `sqlite/`, and `connect` under `postgresql/`. Each
  call exists for one backend only, so the backends differ in a way that fits
  in one line, as rule 9 allows.
  - The `step_request`/`step_run` pair stays in `sqlite/`. It is the
    `store.file` step primitive that `list`, `live`, `stats`, backup and
    project send all use, which is why the plan didn't name it `move_`.
- **`init/{sqlite,postgresql}.rs` among the folder-copy files.** The owner
  settled this ("per-backend project init stays in
  `data/project/init/{sqlite,postgresql}.rs`").
  - A `copy/` subfolder for `parts`, `send`, `staged` and `tree` would exceed
    the four-level depth limit.
- **`bucket/` holding both driver files and the bucket's record and key.**
  The owner chose to fold the driver files into it.
  - `s3_api/` beside `providers/` follows rule 10, because it is a shared
    client and not one of the provider variants.
- **The r2/s3 file set.** `auth`, `settings` and `url` are thin but required
  by rule 9 (see F7).
- **Names that aren't organization:**
  - `key.rs` vs `auth.rs` (the old name `secret.rs` read better)
  - `compact_report.rs`
  - `family.rs` → `exclusive.rs`

  These are for the naming pass.
