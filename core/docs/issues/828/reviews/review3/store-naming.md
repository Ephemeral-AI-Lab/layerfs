# layerfs-store naming review (planned layout, #828)

Scope: every folder and file under the planned `control/crates/layerfs-store/src`
(rows of `nav/site/layout.json` whose `n` starts with that path), judged
against the `//!` header and public items of the old source in
`v2tree/control/crates/{layerfs-store,layerfs-bucket,layerfs-control}`, and
against `nav/trees/tree-{control,api,engine}.txt`. Settled §5 groups (`admin/`,
`data/`, `backend/`, `backup/`, `providers/{local_folder,r2,s3}`, `s3_api/`,
`copy/`, `jobs/`, `data/{branch.rs,commit.rs,file/,project/}`) are not
reopened. LOC is production LOC from the plan.

## 1. Verdict

The groups are sound. Most leaf names are predictable. 14 renames or
single-file moves are worth making. The main problems:

- one key file named for the wrong half of the concept (`bucket/key.rs`);
- call files not named after the 2.0 call (`repair.rs`, `export/route.rs`,
  `project.init` in `family.rs`, `backup.connect`/`disconnect` in
  `jobs/family.rs`, `project.list` in `status/get.rs`);
- one concept, two words in the same crate (`intent.rs` / `op.rs`,
  `check.rs` / `checks.rs`);
- same-name files with different jobs inside `backup/` (`list.rs` ×2,
  `heads.rs` ×2).

None of the fixes changes a group or crosses a crate.

## 2. Proposed renames (ranked by value)

| # | Planned path | Proposed path | Reason (evidence) |
|---|---|---|---|
| 1 | `backup/bucket/key.rs` (41) | `backup/bucket/secret.rs` | Its public functions are `read`, `write`, `kept`, `put_back` and `remove` of the secret in the home's secret folder (`secret/bucket/<bucket>.json`). It defines no key type. Meanwhile `bucket/auth.rs` (old `credentials.rs`, header "A bucket's key") holds `Credentials`, so both files answer "where is the key?". `secret` was the old name, and it matches home `disk/secrets.rs` and the PostgreSQL login "secret kind `login`". |
| 2 | `admin/maintenance/repair.rs` (326) | `admin/maintenance/repair_mount_holders.rs` | The call is `store.repair_mount_holders` (header line 1), and the api file is `families/store/maintenance/repair_mount_holders.rs` (rule 15). `repair` alone is broader than the code: it judges mount, Init, merge and removal intents for one call. |
| 3 | `data/project/record/op.rs` (121) | `data/project/record/intent.rs` | Same concept as `admin/record/intent.rs`: `begin`/`phase`/`end`/`uncertain` here, `begin`/`advance`/`clear`/`leave` there, both on the record's `op`. Home names it `op/intent.rs` ("the one in-flight intent"), and rule 20 says "op intent". Two `record/` folders in one crate use two words (rule 19). |
| 4 | `data/project/export/route.rs` (215) | `data/project/export/call.rs` | The file holds the `project.export` call itself (`ProjectExport`, `run`, `in_worker`). The sibling call folder uses `delete/call.rs`, and workspace has `exec/call.rs` ("`workspace.exec` … as a caller makes them"). `route` is also the role folder `admin/route/` in the same crate (rule 13). |
| 5 | `data/project/family.rs` (283): `init` + `ProjectInit` | `data/project/init/call.rs` | `project.init` is the call. It sits outside `init/`, under `family.rs`, a name that predicts nothing, while its parts are in `init/`. This uses the same call-folder convention as row 4. The file's other public fn, `get`, is the project doctor's "project.listed check", not `project.get`, and it collides by name with `status/get.rs::get`. It only composes `live::projects` and `live::find`, so it joins `status/live.rs`. After that, `family.rs` is gone. |
| 6 | `backup/jobs/family.rs` (129) | `backup/connect.rs` (loose in `backup/`) | Its own code is `backup.connect`, `backup.disconnect` and `write_link`. None of that is a job (header: "This file connects and disconnects buckets"). Its other items are re-exports of `create`/`restore` and a one-line `list` forward, which belong in `backup/mod.rs`. The api has `families/backup/connect.rs` and `disconnect.rs`. The name mirrors `backend/postgresql/connect.rs`, which also holds connect plus disconnect. `backup/` is a branch folder, so one loose file is allowed (rule 3). |
| 7 | `admin/maintenance/diagnostic.rs` (324) | `admin/status/diagnostic.rs` | Header: "Observational Store engine entry points, without leases or writable opens". Its consumers are the Store and project doctors (inspect `store/doctor.rs`, `store/project.rs`) and `ConnectionStep`. It changes nothing, so it fits rule 20 `status/` ("how the thing is, without changing it"), and the api puts `store.doctor` in `families/store/status/`. `maintenance/` then matches api `maintenance/` exactly (compact, repair_mount_holders, upgrade). This is a move within the crate; the name is kept (it matches persistence `diagnostic`). |
| 8 | `admin/route/place.rs` (216) | `admin/route/executor.rs` | Its items are `Executor`, `plan`, `on_machine` and `on_machine_read`, which choose in-process or worker. Its sibling `place_view.rs` carries the engine's `PlaceView` (persistence `history/admission/place.rs`: machine incarnations and Sandbox generations for admission). Side by side, `place`/`place_view` read like a type and its view, but they are unrelated (rule 13). Keep `place_view.rs`, which is named after the engine type. |
| 9 | `data/project/delete/unknown.rs` (32) | `data/unknown.rs` | `named_unknown` is called by `data/branch.rs`, project `hide_path`, project `status/live.rs`, `family.rs` and `delete/remote.rs`. Placing it under `delete/` says delete-only. The name is kept (the `Unknown` refusal code). `data/` loose files become branch, commit and unknown (3 of 4 allowed). |
| 10 | `data/project/status/get.rs` (171) | `status/get.rs` + `status/list.rs` | It holds both `project.get` and `project.list` (`ProjectList`, `list`). Every other `status/` (store `admin/status`, sandbox, machine) and api `families/project/` has its own `list.rs` (rule 15, one call per file). With `list.rs` present, `listing.rs` (the Store-side listing a worker op serves) reads correctly as the source below the call. |
| 11 | `backup/copy/list.rs` (61) | `backup/copy/listing.rs` | It is not a call: `list_stores` and the `Copy` stat of what a bucket holds. It shares a name with `backup/jobs/list.rs` (`backup.list`) in the same `backup/`. `listing.rs` uses the crate's existing word for "the underlying listing below a list call" (project `status/listing.rs`). |
| 12 | `backup/jobs/heads.rs` (28) | fold into `backup/jobs/list.rs` (sole caller); else `jobs/store_heads.rs` | `backup/` would hold two `heads.rs` with different jobs. `copy/heads.rs` is the digest of a Store file plus the `layerfs-heads` metadata key. `jobs/heads.rs` reads that digest where the Store lives (here, or a worker `store.file` step). Its only user is `jobs/list.rs`; its function is already called `store_heads`. |
| 13 | `backend/sqlite/step_request.rs` (163), `step_run.rs` (306) | `step_frame.rs`, `steps.rs` | `step_request.rs` holds `StepFrame` and `StoreStep`, "the `store.file` frame it travels in". The crate's word for a worker wire message is *frame* (`project/frame.rs`, `route/call.rs` `CallFrame`, machine `route/frame.rs`, sandbox `route/host/frame.rs`). `step_run` is a verb phrase. The file is the set of steps (`steps::run`), and `steps.rs` was its old name (rule 18: plural for a set). |
| 14 | `backup/providers/{r2,s3}/checks.rs` (new) | `backup/providers/{r2,s3}/check.rs` | `backend/postgresql/check.rs` does the same job ("connection checked from the side that dials it"). Persistence uses `diagnostic/check/`. Use one word, singular, for "a target's connection check" (rule 19). |
| 15 | `admin/status/formats.rs` (85) | `admin/status/format.rs` | It answers the `format` section of `--with format`. Its siblings are named after their `--with` word (`live.rs`, `stats.rs`). The plural adds nothing. Low value. |

Resulting counts stay inside §1:

- `admin/status/`: 8 files.
- `admin/maintenance/`: 4.
- `data/project/` loose: 3 (frame, hide_path, with_users).
- `data/project/init/`: 8.
- `data/project/status/`: 6.
- `backup/jobs/`: 6 or 7.
- `backup/` loose: 1.

## 3. Names checked and judged fine

- **Root:** `lib.rs`. `users.rs` holds the StoreUsers trait (machine `users.rs` uses the same name). `words.rs` holds refusal plain words; one file after the bucket merge.
- **`admin/` and `admin/with_users.rs`:** settled.
- **`admin/record/`:**
  - `identity.rs`: `Identity`, `Kind`, `store_dir`, `make_empty`.
  - `store.rs`: StoreRecord. It avoids `record/record.rs` and matches sandbox `record/sandbox.rs`.
- **`admin/route/`:**
  - `call.rs`: `store_call` and `CallFrame`, matching sandbox `route/daemon/call.rs`.
  - `lease.rs`.
  - `methods.rs`: StoreOp and the engine-method translation. It is the same concept as engine store-service `methods/`, and it frees `op` for the record op.
  - `open.rs`: the open-Store cache and reaper.
  - `open_args.rs`: OpenArgs.
  - `place_view.rs`: engine PlaceView.
  - `resolve.rs`: named Store/project plus the first checks. The old sqlite `resolve.rs` became `move_leftover.rs`, which removes a collision.
  - `service.rs`: wraps the engine's StoreService.
- **`admin/status/`:** `get`, `list`, `live`, `observed`, `state`, `stats`.
- **`admin/maintenance/`:** `compact`, `compact_report`, `upgrade`.
- **`backend/postgresql/`:**
  - `address`, `connect` (connect and disconnect), `login`, `open`, `reader` (the host read session).
  - `check` holds the dial-side check plus the attach/detach record writes. The call words `attach` and `detach` are not in any file name; this is acceptable because the check is what they run.
  - `family` is the pair with sqlite `family`: the per-backend begin/settle of the exclusive op (rule 9 parallel).
- **`backend/sqlite/`:**
  - `create` and `remove` (named after the calls).
  - `family` (each call's entry and shared steps).
  - `move_phases`: `store.move` cannot be `move.rs` because `move` is a Rust keyword.
  - `move_leftover`.
- **`backup/bucket/`:** `auth` (shared credential types), `driver`, `error`, `limits`, `pieces`, `provider` (trait), `record` (the bucket's record), `registry` (provider list by scheme).
- **`backup/copy/`:** `heads` and `load`.
  - Note on `upload.rs` (7): it only holds the metadata keys `SQLITE_VERSION` and `BINDING` and the `Expected` precondition. The upload itself is in `jobs/create.rs`. `metadata.rs` would describe the keys but not `Expected`, so this is left for the owner.
- **`backup/jobs/`:** `create`, `restore` (calls), `ended`, `progress` (the public call event, like machine `lifecycle/progress.rs`), `restored`, `list` (`backup.list`).
- **`backup/providers/*`:** `auth`, `settings`, `url`; `local_folder/driver.rs` is its own driver (the one-line difference).
- **`backup/s3_api/`:** all ten files are predictable.
- **`data/`:** `branch.rs`, `commit.rs` (settled).
- **`data/file/`:** `requests.rs` is fine as a name. See §4 on `read.rs` and `write.rs`.
- **`data/project/`:**
  - `frame.rs` (project worker frames), `hide_path.rs` (named after the call), `with_users.rs`.
  - `delete/`: `call.rs` and `remote.rs` (the `Effects` implementation through a worker).
  - `export/`: `destination` and `walk`. `writer` is acceptable: inside `export/` it pairs with `walk`, although "writer" is also the Store's writer lane.
  - `init/`: `parts`, `postgresql`, `sqlite`, `send`, `staged`, `worker` (`in_worker`, Init deadline, locator).
  - `record/`: `discovery` (same word as sandbox `record/discovery.rs`) and `project`.
  - `status/`: `listing`, `live`, `state`, `stats`.

Minor, no rename proposed:

- `init/tree.rs` is also used by `export/walk.rs` and `export/writer.rs`, and the word "tree" overlaps content `tree/`. If it ever moves to serve both folders, name it after its job: the folder-copy record stream.
- `bucket/progress.rs` (driver byte counts) and `jobs/progress.rs` (call event) share a name, but they are layered and their folders disambiguate them.

## 4. Cross-crate and cross-layer inconsistencies

1. **File family, one file per call "in every layer" (rule 1 exception and §5 "files named after the 2.0 calls").**
   - api `families/file/` has 14 call files, and engine `methods/file/` has 15.
   - Control `data/file/` bundles them into `read.rs` (440, about nine reads) and `write.rs` (245, about seven writes).
   - The owner should either split `data/file/` per call or narrow the exception's wording to api and engine.
2. **The Store's lifecycle calls have no `lifecycle/`.**
   - api `families/store/lifecycle/{attach,connect,create,detach,disconnect,move,remove}`.
   - In control they live in the backend variants: sqlite `create`, `remove`, `move_phases`; postgresql `connect`, and `check` (attach/detach).
   - Justified because each is kind-specific. The effect is that `attach` and `detach` cannot be found by file name. This is recorded, not proposed.
3. **`family.rs` has four meanings across control.**
   - sandbox root `family.rs`: a 27-LOC facade.
   - store `data/project/family.rs`: the `project.init` implementation plus a doctor check.
   - store `backup/jobs/family.rs`: the connect/disconnect implementation plus a facade.
   - `backend/{sqlite,postgresql}/family.rs`: the per-backend exclusive op.
   - Renames 5 and 6 leave only the backend pair, which is a consistent rule-9 sibling pair.
4. **Op intent: `intent` versus `op`.** home `op/intent.rs` and store `admin/record/intent.rs` use `intent`; project `record/op.rs` uses `op` (rename 3).
5. **Connection checks: `check` versus `checks`.** postgresql `check.rs` against providers `checks.rs` (rename 14). The engine-entry `diagnostic` name (`admin/.../diagnostic.rs`, `s3_api/diagnostic.rs`) is consistent with persistence and is kept.
6. **Call-folder entry file.** workspace `exec/call.rs` and store `delete/call.rs` use one convention, but `export/route.rs` and `init` (in `family.rs`) break it (renames 4 and 5).
7. **Where the doctor entry sits.** api store `status/doctor.rs` against control store `maintenance/diagnostic.rs` (rename 7).
