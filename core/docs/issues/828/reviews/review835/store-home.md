# Review 835: `layerfs-store` and `layerfs-home` on the #835 base

Scope: the planned `control/crates/layerfs-store` and `control/crates/layerfs-home`, recursively.
Sources: `S/v835tree` (merged #835 tree), `S/sim835/site/layout.json` (plan rows), LOC from `S/loc_835.txt`.
Nothing in the repository or any worktree was touched.

## 1. Verdict

**Yes, but only inside the two crates. No new crate, no crate merge and no cross-crate move is needed.**

#835 added a third Store route (the host worker) and a second consumer of the open-Store cache. The plan
packs both into the wrong places:

- The five host-worker files go to `data/`. But `data/` holds the api families (branch, commit, file,
  project), and these files are a route.
- `admin/route/` becomes a 9-file mix of two concerns:
  - choosing a call's route;
  - the Stores this process holds open: the cache, its idle checkpoint, the reaper, the service
    registry and the leases.

The fix is to split `admin/route/` into `route/` and a new `admin/open/`, and to move the host worker to
its own `admin/host_worker/`. The 674-line `open.rs` splits along four seams it already has. Home needs
one small regroup: a `secret/` folder. The settled store skeleton (`admin/`, `data/`, `backend/`,
`backup/`) and home's `disk/ json/ lock/ log/ record/ reply/` all stand. No §5 decision is reopened.

## 2. Findings (ranked by value)

### F1. The host-worker files are a route, not data: `data/host_worker/` → `admin/host_worker/`

**What is wrong.** The plan puts these files in `data/host_worker/{read,write,stream,postgresql}.rs`:
`host_read.rs` 211, `host_write.rs` 424, `host_stream.rs` 501 and `host_postgresql.rs` 276.

But `data/` is "what a Store holds": the api families `branch.rs`, `commit.rs`, `file/` and `project/`,
by the settled rule "Store calls match the api families". These four files implement no family. They
are both ends of one route: the CLI side (`routed`, `send`, `spool`, `likely`, deferred input) and the
worker side (`serve`, `serve_write`, `retire_request`, admission under the record lock).

**Evidence.**

- Headers:
  - `host_read.rs`: "The native CLI worker's bounded, read-only Store protocol."
  - `host_stream.rs`: "Both ends of a streamed host write".
  - `host_postgresql.rs`: "The CLI chooses this route after its argument and scope checks … any other
    call stays on the direct route (P-A), chosen before dialing."
- Route selection. The route chooser `call.rs` (planned `admin/route/call.rs`) tries them first:
  `call.rs:31 crate::host_read::routed(...)`, `:34 crate::host_postgresql::routed(...)` and
  `:37 crate::host_write::routed(...)`.
- Imports. They use route and open code, never a family:
  - `host_read.rs:8 use crate::resolve::{self, Target}` and `:232 crate::open::run_host(`;
  - `host_stream.rs:610 crate::resolve::target`, `:613 crate::open::run_host`;
  - `host_postgresql.rs:31-34 use crate::{lease, open_args, record, resolve}`.
- Callers:
  - `layerfs-control/src/worker_server/{host.rs,connection.rs,host/{postgres,write}.rs}`;
  - `layerfs-control/src/doctor/host_worker.rs`;
  - `layerfs-api/src/calls/{dispatch,render}.rs`.

  All of them are route or worker-server code.
- Rule 20 says `route/` is "how a call reaches it (worker client, host dispatch, daemon channel)".

**Fix.**

- Move `control/crates/layerfs-store/src/data/host_worker/` to `control/crates/layerfs-store/src/admin/host_worker/`.
- Use a sibling folder, not `admin/route/host/`: `route/` keeps 6 loose files, so a subfolder would break
  rule 3. Four `host_*` files would also trigger rule 2.
- Name. "Host worker" is #835's own word everywhere: the lease role `"host-worker"` in `lease.rs:27`,
  `doctor/host_worker.rs` and machine `worker/host*.rs`. It names a specific route, so it does not fall
  under rule 12.
- Split `write.rs`. Its second half, lines 383–539, is a separate concern: `retained_dir`,
  `RETAINED_CAP`, `keep`, `make_room`, `records` and `retained`. This is the on-disk store of write
  outcomes the caller may not know, under `machine/host/retained/`. The worker writes it (`keep`, called
  from `worker_server/host/write.rs:329`) and the doctor reads it (`retained*`, called from
  `doctor/host_worker.rs:169,172`). It becomes `admin/host_worker/retained.rs`, and `write.rs` stays the
  route alone.

**Cost / gain.**

- Cost: 5 files, plus the import paths in the 8 caller files above. The plan rewrites those paths anyway.
- Gain: `data/` again means exactly the api families, and all of the host worker's Store side is in one
  folder that a reader can predict.

### F2. `admin/route/` mixes two concerns: split out `admin/open/`, the Stores this process holds open

**What is wrong.** The planned `admin/route/` holds 9 files: `call`, `executor`, `lease`, `methods`,
`open`, `open_args`, `place_view`, `resolve` and `service`. Two concerns are mixed:

1. **Route:** which path a call takes, and the frame it carries.
   - `call.rs`: "The route of one Store call, chosen from records".
   - `executor.rs` (`place.rs`): "Where a call runs … [`plan`] decides it once".
   - `methods.rs`: the method translation.
   - `resolve.rs`: "The Store and project a call names".
   - `place_view.rs`: the admission view sent with a call.
2. **Held open:** what this process keeps open, and who holds it.
   - `open.rs`: "The Stores this process holds open: a call opens its Store once and later calls reuse
     it; a Store unused for IDLE is closed by a reaper thread".
   - `service.rs`: "One `StoreService` per Store path per process, opened by the first client … closed
     when the last one releases it".
   - `lease.rs`: "every process that uses a Store holds `store/<name>/open/<pid>`".
   - `open_args.rs`: "What a Store call opens its Store with".

The two groups are coupled tightly among themselves and loosely to each other:

- Held-open files use each other:
  - `open.rs:16-19 use crate::{lease, open_args, record, service::{Completion, Owner, Slot}}`;
  - `service.rs:18 use crate::open_args`;
  - `pool.rs` uses `crate::service::open_postgres_{read,write}` and `crate::open::IDLE`.
- Route files use held-open files only through one entry: `call.rs:10 use crate::open` and
  `:9 use crate::lease`.

**The name is proven by the home layout.** The lease folder on disk is literally `open/`:
`layerfs-home/src/home/paths.rs:25-26 fn lease_dir(...) { store_dir(home, store).join("open") }`.
Alternatives:

- `session/` would collide with each persistence backend's engine `session/` (rule 13), which is a
  database session, not a held `StoreService`.
- `lease/` names only one of the files.

**Fix.**

- `admin/route/lease.rs` → `admin/open/lease.rs`.
- `admin/route/service.rs` → `admin/open/service.rs`. Part of it splits out (F4).
- `admin/route/open_args.rs` → `admin/open/args.rs`. Rule 16: drop the word the folder already says.
- `admin/route/open.rs` splits into `admin/open/` (F3).
- `admin/maintenance/worker_lease.rs` (`host_maintenance.rs`) → `admin/open/host_handoff.rs`:
  - Its header: "Hand off one native worker's Store lease before a local maintenance step."
  - It is not a maintenance call. It is the fence over the host worker's hold. It uses
    `crate::lease::Owner`, the record lock and `open::close_host` (`host_maintenance.rs:203 close_at`).
  - Its callers include non-maintenance code: `sqlite/family.rs:21` and `sqlite/transfer/phases.rs:9`
    (store move, push and pull), plus `layerfs-control/src/worker_server/host.rs`.
  - "worker_lease" also misnames it: it takes no lease. It records the `close_worker` op fence and
    closes the worker's cache entry.
  - If the owner prefers it beside its main callers, keeping it in `maintenance/` as
    `worker_handoff.rs` is the acceptable fallback.
- `admin/route/` keeps `call`, `executor`, `methods`, `place_view` and `resolve`, plus the new
  `mount_intents` (F4).

**Minor rename that goes with it.** Once `lease.rs` and `service.rs` sit side by side, two different
`Owner` types are neighbours:

- `lease::Owner` is a process identity: pid, start time and boot (`lease.rs:138`).
- `service::Owner { Direct, Host }` is the route that borrows the service (`service.rs:30`). Its refusal
  text already says "the Store service belongs to another route".

Rename `service::Owner` → `Route`. It is `pub(crate)`, so no other crate changes (rule 19).

**Cost / gain.**

- Cost: 5 moved files and 1 rename. Callers outside the crate that already change paths under the plan:
  - `api/.../dispatch.rs`;
  - `layerfs-control/src/{host_power.rs, store.rs, worker_server/{host,ops}.rs, doctor/*}`;
  - `layerfs-sandbox/src/{route.rs, host/{call,frame}.rs, mount_intents.rs}`.
- Gain: `route/` (6) and `open/` (8) are each one concern, named by what they hold. The doctor and the
  CLI find "who holds this Store" in one folder.

### F3. Split the 674-line `open.rs` along its four existing seams

**What is wrong.** `open.rs` grew from 184 to 674 LOC (797 physical). It now holds four concerns, each
a contiguous block with its own doc comments:

| Block | Lines | What it holds |
| --- | --- | --- |
| cache core | 30–220, 298–394 | `Entry`, `Cached`, `OPEN`, `CHANGED`, `with_one_shot`, `run`, `call`, `Running`, `hold`, `enter`, `reserve`, `opened` |
| idle WAL checkpoint (#86) | 221–297 | `holds`, `settle`, `settled`, `has_log`, `wal_remaining`, `checkpoint`, `released` |
| host-worker admission | 114–162, 395–527, 756–790 | `run_host`, `run_host_streamed`, `HOST_STORES`, `host_store_count`, `enter_host`, `check_identity`, `close_host` |
| close and reaper | 528–755, 791–797 | `reaper`, `CloseReport`, `start_close`, `finish_close`, `release_lease`, `finish_idle`, `close_idle*`, `close_if_idle`, `close`, `close_report`, `in_use` |

**Fix.**

- `admin/open/cache.rs`: the cache core. It keeps `IDLE`, `run`, `hold` and `with_one_shot`.
- `admin/open/checkpoint.rs`: the idle checkpoint, with `released`.
- `admin/open/close.rs`: the reaper and every close.
- `admin/open/host.rs`: the host worker's entries in the cache, with its own slot cap and its fenced
  close.

The host block stays inside `open/` rather than moving to `host_worker/` because it reaches into
private cache state (`cache()`, `Slot`, `Entry.calls/settling`, `start_close`, `finish_close`). Inside
one folder that needs only `pub(super)`. A move would need `pub(crate)` on cache internals.

**Cost / gain.**

- Cost: 1 file becomes 4, plus `pub(super)` on about 8 items.
- Gain: no 674-line mixed file. The #86 checkpoint rule, the reaper and host admission can each be read
  and reviewed alone. The store `lib.rs` re-export `pub use open::close_idle` becomes
  `admin::open::close::close_idle`.

### F4. `service.rs` (229 → 432) carries Sandbox and machine helpers that are not the registry

**What is wrong.** `service.rs` lines 414–537 hold four public helpers. They use the registry but are
not part of it:

- Their only callers are other crates:
  - `head` from `layerfs-sandbox/src/workspace/serve.rs`;
  - `branch_named` and `end_mount` from `layerfs-sandbox/src/mount_intents.rs`;
  - `reconcile_machine` from `layerfs-control/src/machine.rs`.
- `reconcile_machine` is itself a route decision: `match crate::place::plan(...) { Executor::Local =>
  …, Executor::Worker(w) => w.call("store.reconcile", …) }`, and it reads `place_view::scoped` /
  `encoded_from_home`.
- `end_mount` releases leftover mount intents.

**Fix.**

- `end_mount` and `reconcile_machine` → new `admin/route/mount_intents.rs`, beside `place_view.rs`.
  Their concern is placed mount intents, the same concern as `place_view.rs`'s header ("a call whose
  admission compares placed mount intents"). The name also matches the caller's file,
  `layerfs-sandbox/src/mount_intents.rs`.
- `head` and `branch_named` merge into `data/branch.rs`. They are Branch reads, and `branch_named`
  literally runs `project.list_branches`.
- `open/service.rs` keeps the registry: open, acquire, release, dispatch and the PostgreSQL openers.

**Cost / gain.**

- Cost: 1 split, 1 new file, a merge of about 45 LOC into `branch.rs`, and 3 caller paths.
- Gain: `service.rs` is only the registry its header describes, and the Store-side mount-intent logic
  sits in one file next to the view it reads.
- Lower value than F1–F3. It can be dropped without harming them.

### F5. Home: a `secret/` folder; `wipe.rs` and two secret files are scattered

**What is wrong.** The plan places three secret-related files apart:

- `wipe.rs` (51) at the root: "Overwriting a secret before its memory is freed (#819 P-B)";
- `disk/keys.rs` (root `secrets.rs`, 40): "worker secrets in `worker.json` and Sandbox daemon keys in
  `control.json`";
- `disk/secrets.rs` (`home/secrets.rs`, 60): "The home's secret folder, `secret/` … so a record names a
  secret and never holds it".

They are one concept. `keys.rs` was renamed only to avoid two `secrets.rs` (plan note), which is a sign
that a folder is missing. `wipe` also does not belong to `disk/` or the root. Its callers are:

- the secret holders: store `postgresql/{address,login,open,connect,pool}.rs`, `host_postgresql.rs` and
  `host_write.rs`;
- machine `worker/{host,host_frame}.rs`;
- `layerfs-control/src/worker_server/connection.rs`;
- `home/{records,secrets}.rs`.

The root also reaches the 4-loose-file limit (`cancel`, `names`, `socket`, `wipe`).

**Fix.** New `control/crates/layerfs-home/src/secret/`:

- `keys.rs`: was planned `disk/keys.rs`.
- `saved.rs`: was planned `disk/secrets.rs`. Rule 14 forbids `secret/secrets.rs`. "Saved" is the word
  `postgresql/connect.rs` uses: "the password is a secret of the home … saved".
- `wipe.rs`: was the root `wipe.rs`.

The singular `secret/` follows rule 18, and it is also the home's on-disk folder name
(`home/secrets.rs: dir(home) = home.join("secret")`). After the move, `disk/` is `capacity`, `guest`,
`layout`, `paths` and `native_names` (5), and the root has 3 loose files.

**Cost / gain.**

- Cost: 3 moves and 1 new `mod.rs`. Caller paths:
  - `layerfs_home::wipe`: about 12 files;
  - `layerfs_home::secrets` and `home::secrets`: 13 files.

  The plan already rewrites the `home::` paths.
- Gain: one place for everything that holds or erases a secret. That matters for a P-B security review.

### F6. Home: `legacy_host.rs` is a layout check, not a record kind: `record/legacy.rs` → `disk/native_names.rs`

**What is wrong.**

- Header: "Read-only compatibility checks for development homes made before host naming."
- Its only caller is `home/layout.rs:59 super::legacy_host::check(home)?`, called from the home's
  open/detect.
- It imports `super::{layout::foreign_fix, paths, ...}`, both of which are in `disk/`.
- In `record/` it would sit among the record field types (`fields`, `generation`, `observed`, `intent`)
  and need cross-folder access to `layout::foreign_fix`.
- "legacy" fits anything old (rule 12). What the file actually detects is records that still use the
  machine name `native`.

**Fix.** `record/legacy.rs` → `disk/native_names.rs`, a `pub(super)` sibling of `layout.rs`.

**Cost / gain.**

- Cost: 1 file.
- Gain: the file sits beside its only caller with private visibility, and its name says what it refuses.

### F7. Map the unplanned #835 test files

The plan has no rows for these four test files:

| File | Maps to |
| --- | --- |
| store `tests/host_store_cache.rs` | `tests/route/host_cache.rs` |
| store `tests/lease_release.rs` | `tests/route/lease_release.rs` |
| store `tests/postgresql_held_sessions.rs` | `tests/postgresql/held_sessions.rs` |
| home `tests/json_stream.rs` | `tests/call/json_stream.rs` |

The test area `tests/route/` then has 6 modules: `lease_race`, `lease_release`, `lease_stale`,
`host_cache`, `open_args` and `place_view_scope`. A separate `tests/open/` area would leave `route/`
with 2 modules, which is thin. So the area keeps the name `route/`. The cost is the test-area word
differing from the source split; the gain is one cheap binary.

## 3. Proposed layout

Scope: tree `S/v835tree` (#835 merged onto `layerfs_v2_0_0`), production LOC from `S/loc_835.txt`
(`tools/production_loc.py --files`). Conventions:

- Markers are relative to the plan.
- A file with a space marker exists in the tree today; its annotation names where the plan had it.
- `+` folders are folders the plan does not have.
- A moved file counts only at its destination. The `-` row is a split source, counted where the plan
  placed it.
- `mod.rs` files the plan creates show `—`.

### `layerfs-store` (only `admin/` and `data/` change; `backend/` and `backup/` stay as planned)

```diff
  src/                                  [partial]  (backend/, backup/, data/file/, data/project/ unchanged, not shown)
    lib.rs                                53
    admin/                            [7194]
      mod.rs                               —   (plan)
      with_users.rs                      515
+     host_worker/                    [1412]  (plan: data/host_worker/; F1)
+       mod.rs                             —   new
        postgresql.rs                    276   (host_postgresql.rs)
        read.rs                          211   (host_read.rs)
        stream.rs                        501   (host_stream.rs)
        write.rs                         424   (host_write.rs; retained outcomes split out)
+       retained.rs                        —   new  (keep/retained/RETAINED_CAP from write.rs)
      maintenance/                    [1254]
        mod.rs                             —   (plan)
        compact.rs                       451
        compact_report.rs                 97
        repair_mount_holders.rs          326
        upgrade.rs                       380
+     open/                           [1006]  (Stores held open here; = home lease dir store/<name>/open/; F2)
+       mod.rs                             —   new
        args.rs                          118   (open_args.rs; plan admin/route/open_args.rs)
        host_handoff.rs                  208   (host_maintenance.rs; plan admin/maintenance/worker_lease.rs)
        lease.rs                         248   (plan admin/route/lease.rs)
        service.rs                       432   (plan admin/route/service.rs; registry only, F4; Owner → Route)
+       cache.rs                           —   new  (from open.rs: entries, run, hold, IDLE)
+       checkpoint.rs                      —   new  (from open.rs: idle WAL checkpoint #86, released)
+       close.rs                           —   new  (from open.rs: reaper, close*, CloseReport)
+       host.rs                            —   new  (from open.rs: host admission, slot cap, close_host)
      record/                          [706]
        mod.rs                             —   (plan)
        identity.rs                      193
        intent.rs                        120
        store.rs                         393
      route/                          [1366]  (which path a call takes; now 6 files)
        mod.rs                             —   (plan)
        call.rs                          161
        executor.rs                      216   (place.rs)
        methods.rs                       206   (calls/op.rs)
        place_view.rs                     79
        resolve.rs                        30
-       open.rs                          674   (split into open/{cache,checkpoint,close,host}.rs)
+       mount_intents.rs                   —   new  (end_mount, reconcile_machine from service.rs)
      status/                          [935]  (unchanged)
        mod.rs                             —   (plan)
        formats.rs                        85
        get.rs                           204
        list.rs                           76
        live.rs                          238
        observed.rs                      146
        state.rs                          91
        stats.rs                          95
    data/                              [548]  (partial: file/ and project/ unchanged, not shown)
      mod.rs                               7   (calls/mod.rs)
      branch.rs                          297   (gains head, branch_named from service.rs)
      commit.rs                          212
      unknown.rs                          32
```

The planned `data/host_worker/` no longer exists: it is replaced by `admin/host_worker/`.

### `layerfs-home`

```diff
  src/                                [4286]
    lib.rs                                21
    cancel.rs                             14
    names.rs                             158
    socket.rs                            139   (home/host_socket.rs; as planned)
    disk/                              [464]
      mod.rs                              21   (home/mod.rs)
      capacity.rs                         60
      guest.rs                            60
      layout.rs                          151
      paths.rs                           114
      native_names.rs                     58   (home/legacy_host.rs; plan record/legacy.rs; F6)
+   secret/                            [151]  (F5)
+     mod.rs                               —   new
      keys.rs                             40   (root secrets.rs; plan disk/keys.rs)
      saved.rs                            60   (home/secrets.rs; plan disk/secrets.rs)
      wipe.rs                             51   (root wipe.rs)
    json/                             [1068]  (unchanged from plan)
      mod.rs                               4
      read.rs                            232
      stream.rs                          277
      strings.rs                         162
      value.rs                           393
    lock/                              [175]  (unchanged from plan)
      mod.rs                               —
      entity.rs                           37
      flock.rs                            31
      home.rs                            107
      mutex.rs                             —   (plan, split from helpers.rs)
    log/                               [966]  (unchanged from plan)
      mod.rs                               —
      calls.rs                           377
      lines.rs                             —   (plan)
      rotate.rs                           35
      tail.rs                            434
      trace.rs                           120
    record/                            [669]  (legacy.rs leaves)
      mod.rs                               —   (plan)
      fields.rs                          104
      file.rs                            198   (home/records.rs, split as planned)
      generation.rs                       65
      intent.rs                          188
      observed.rs                         30
      process.rs                          84
      token.rs                             —   (plan)
    reply/                             [461]  (unchanged from plan)
      mod.rs                               —
      cursor.rs                           67
      events.rs                           43
      outcome.rs                         113
      state.rs                           151
      time.rs                             24
      values.rs                           63   (helpers.rs, split as planned)
```

## 4. Checked and left alone

- **The store skeleton:** `admin/{record,status,maintenance}`, `backend/{sqlite,postgresql}`,
  `backup/…`, and `data/{file,project}`, `branch.rs`, `commit.rs`. #835 does not touch their roles, so
  the settled decisions stand.
- **`backend/postgresql/pool.rs` (447), as planned.** It holds the host worker's held PostgreSQL
  sessions, which is PostgreSQL-specific (rule 23). It sits beside `reader.rs`, the CLI host's held
  sessions. The folder then has 9 files.
  - `host_postgresql::retire` is called from `backend/postgresql/{connect,family}.rs`. That is a
    backend → admin import inside one crate, the same direction as `pool.rs` → `service.rs` today, so
    it is fine.
- **`compact.rs` (386 → 451).** It is one call with a worker side, `in_worker`, that mirrors
  `upgrade.rs`'s `in_worker`. It is cohesive and well under 999, so no split.
- **`lease.rs` (248) stays whole.** The host-worker role (`hold_host_locked`,
  `unmatched_host_workers`) is part of the lease file format.
- **`record.rs` (`lock_for_host_close`) and `intent.rs` (`WORKER_CLOSE`)** stay in `admin/record/`.
- **Two layered caches.** `open.rs`'s `OPEN` and `service.rs`'s `STORES` share `Slot`/`Completion`. One
  could argue they should merge. That is a code change, not a layout one, and is out of scope.
- **Home `socket.rs` at the root, as planned.** Its one non-socket function, `private_file`, opens
  owner-only files in the home and could go to `disk/` later. It shares `flock::euid` checks with the
  socket code, so moving it now buys little.
- **Home `json/stream.rs`.** It is planned correctly. Its `use super::{base64_object, JsonError,
  MAX_DEPTH}` becomes `super::value::…`.
- **Cross-crate notes for the other reviewer** (no move proposed here):
  - `host_postgresql::{harden, limit_core_dumps}` is worker-process setup. Its only external callers
    are `layerfs-control/src/worker_server/{host,connection}.rs`.
  - `home::host_socket` is used only by machine `worker/host*` and store `host_write.rs`
    (`private_file`).

  If those move, `admin/host_worker/postgresql.rs` and `socket.rs` shrink, and the layout above does
  not otherwise change.
- **Limits.**
  - All proposed leaves hold 3–8 files.
  - `admin/` is a branch with 6 subfolders and 1 loose file.
  - The home root has 3 loose files.
  - The maximum depth is `src/admin/open/cache.rs`, 3 levels.
  - There is no `x/x.rs` repeat.

## 5. Machine-readable list

```json
[
  {"from": "control/crates/layerfs-store/src/data/host_worker", "to": "control/crates/layerfs-store/src/admin/host_worker", "op": "move", "why": "host worker is a Store route chosen by route/call.rs, not an api family"},
  {"from": "control/crates/layerfs-store/src/data/host_worker/write.rs", "to": "control/crates/layerfs-store/src/admin/host_worker/write.rs", "op": "split", "why": "retained write outcomes (worker writes, doctor reads) are a separate concern"},
  {"from": "control/crates/layerfs-store/src/data/host_worker/write.rs", "to": "control/crates/layerfs-store/src/admin/host_worker/retained.rs", "op": "new", "why": "keep/retained/retained_dir/RETAINED_CAP from write.rs"},
  {"from": "control/crates/layerfs-store/src/admin/route/open.rs", "to": "control/crates/layerfs-store/src/admin/open/cache.rs", "op": "split", "why": "674 LOC holding four concerns; cache core lands in cache.rs"},
  {"from": "control/crates/layerfs-store/src/admin/route/open.rs", "to": "control/crates/layerfs-store/src/admin/open/checkpoint.rs", "op": "new", "why": "idle WAL checkpoint (#86) and released"},
  {"from": "control/crates/layerfs-store/src/admin/route/open.rs", "to": "control/crates/layerfs-store/src/admin/open/close.rs", "op": "new", "why": "reaper, close_idle*, close, close_report, CloseReport"},
  {"from": "control/crates/layerfs-store/src/admin/route/open.rs", "to": "control/crates/layerfs-store/src/admin/open/host.rs", "op": "new", "why": "host worker cache admission, slot cap and close_host; needs cache internals via pub(super)"},
  {"from": "control/crates/layerfs-store/src/admin/route/lease.rs", "to": "control/crates/layerfs-store/src/admin/open/lease.rs", "op": "move", "why": "leases live in store/<name>/open/; held-open concern, not route choice"},
  {"from": "control/crates/layerfs-store/src/admin/route/open_args.rs", "to": "control/crates/layerfs-store/src/admin/open/args.rs", "op": "rename", "why": "what a call opens its Store with; folder already says open (rule 16)"},
  {"from": "control/crates/layerfs-store/src/admin/route/service.rs", "to": "control/crates/layerfs-store/src/admin/open/service.rs", "op": "split", "why": "StoreService registry is held-open state; Sandbox/machine helpers leave; Owner renamed Route"},
  {"from": "control/crates/layerfs-store/src/admin/route/service.rs", "to": "control/crates/layerfs-store/src/admin/route/mount_intents.rs", "op": "new", "why": "end_mount and reconcile_machine: placed mount intents beside place_view.rs"},
  {"from": "control/crates/layerfs-store/src/admin/route/service.rs", "to": "control/crates/layerfs-store/src/data/branch.rs", "op": "merge", "why": "head and branch_named are Branch reads"},
  {"from": "control/crates/layerfs-store/src/admin/maintenance/worker_lease.rs", "to": "control/crates/layerfs-store/src/admin/open/host_handoff.rs", "op": "move", "why": "fence over the host worker's hold; used by sqlite move too; takes no lease"},
  {"from": "control/crates/layerfs-home/src/disk/keys.rs", "to": "control/crates/layerfs-home/src/secret/keys.rs", "op": "move", "why": "one secret/ concern folder"},
  {"from": "control/crates/layerfs-home/src/disk/secrets.rs", "to": "control/crates/layerfs-home/src/secret/saved.rs", "op": "rename", "why": "the home's saved named secrets; avoids secret/secrets.rs"},
  {"from": "control/crates/layerfs-home/src/wipe.rs", "to": "control/crates/layerfs-home/src/secret/wipe.rs", "op": "move", "why": "erasing a secret from memory belongs with the secrets"},
  {"from": "control/crates/layerfs-home/src/secret/mod.rs", "to": "control/crates/layerfs-home/src/secret/mod.rs", "op": "new", "why": "declarations for the new folder"},
  {"from": "control/crates/layerfs-home/src/record/legacy.rs", "to": "control/crates/layerfs-home/src/disk/native_names.rs", "op": "move", "why": "only caller is disk/layout.rs; refuses records still naming native"},
  {"from": "control/crates/layerfs-store/tests/host_store_cache.rs", "to": "control/crates/layerfs-store/tests/route/host_cache.rs", "op": "move", "why": "unplanned #835 test; route test area"},
  {"from": "control/crates/layerfs-store/tests/lease_release.rs", "to": "control/crates/layerfs-store/tests/route/lease_release.rs", "op": "move", "why": "unplanned #835 test; beside lease_race and lease_stale"},
  {"from": "control/crates/layerfs-store/tests/postgresql_held_sessions.rs", "to": "control/crates/layerfs-store/tests/postgresql/held_sessions.rs", "op": "move", "why": "unplanned #835 test; postgresql test area"},
  {"from": "control/crates/layerfs-home/tests/json_stream.rs", "to": "control/crates/layerfs-home/tests/call/json_stream.rs", "op": "move", "why": "unplanned #835 test; beside json_codec"}
]
```
