# Review 835: the host worker and the guest worker, across crates

Scope: every #835 host-worker file plus the guest worker pieces they share code
with. I read headers and `use` lines in S/v835tree and checked callers and each
crate's Cargo.toml. LOC figures come from S/loc_835.txt; split sizes are
nonblank, non-comment counts of the line ranges named.

## 1. Verdict

**Yes, but only one destructive change is needed, plus two placement fixes. No
new crate.**

- **Move the host worker's server out of api into layerfs-store.** The plan puts
  it in api with the reason "it serves store, sandbox, workspace and doctor ops".
  That is true of the guest server and false of the host server. The host server
  `use`s only `layerfs_store`, `layerfs_machine`, `layerfs_home` and
  `layerfs_bridge`, and dispatches only Store ops. Once it moves, the host
  worker's whole Store side (both ends of every call kind, the accept loop and
  its queues) sits in one Store folder. api's `worker/` then holds only the
  guest server.
- **Two first-pass placements would not compile under the layer order.**
  - `machine/lifecycle/fence.rs` `use`s inventory, the Sandbox teardown and
    `layerfs_store`.
  - `inspect/machine/worker.rs` is called from `machine.get`, which the plan
    moves into layerfs-machine.
- **A dedicated `layerfs-worker` crate is not better.** The client must sit at
  or below machine, because machine's own boot and status code call it. The
  guest server must sit above inspect, because it serves inspect and Sandbox
  ops. One crate cannot hold both. A protocol-only crate below machine would
  also have to cut the client loose from `MachineRecord::admit_host` and
  `bundle::bundle_dir`. It would gain nothing that machine `route/` does not
  already give.

## 2. Findings, ranked by value

### F1. The host worker server belongs in layerfs-store, not api (cross-crate move)

**What is wrong**
- The planned `api/.../worker/host/{serve,postgresql,write}.rs` is a server
  whose every operation is a Store operation.
- Its handlers sit in store (`host_read::serve`, `host_postgresql::serve`,
  `host_stream::serve`, `host_maintenance::close_at`, `host_write::keep`), and
  so does its admission state (`postgresql::pool`).
- To reach them, store exports about 20 items that only this server uses, such
  as `host_write::header`, `host_postgresql::{checks, wipe_request, harden,
  limit_core_dumps, RETIRE}` and `pool::{reserve_write, hold, release_held,
  Reservation, PER_PROCESS}`.

**Evidence**
- Every external path in `worker_server/host.rs`, `host/postgres.rs` and
  `host/write.rs` is one of `layerfs_store::{host_read, host_write,
  host_stream, host_postgresql, host_maintenance, postgresql::pool, open}`,
  `layerfs_machine::worker::{HostPaths, HostHello, StreamReader,
  HOST_HELD_WRITES}`, `layerfs_machine::record::MachineRecord`,
  `layerfs_home::*` or `layerfs_bridge::control`. Nothing from sandbox or
  inspect.
- The dispatch table in `host.rs` (414–417) is `store.close`, `store.call`,
  `store.postgresql_read` and `store.postgresql_retire`, plus `store.write` via
  `connection.rs`.
- The only sibling coupling is:
  - `Served`, a struct with one field;
  - `fault::write_outcome`, a 20-line, host-only switch at fault.rs:196–218;
  - about 240 host-only LOC inside `worker_server/connection.rs`, at lines
    226–517, all `cfg(feature = "machine", unix)`: `serve_host_connection`,
    `read_host_request`, `answer_host_request`, `held_write`, `reply`, `go`,
    `is_postgres` and `refuse_host_request`.
- The plan moves `connection.rs` to api whole, so api's guest connection file
  would still carry the host's Unix-socket path (167 guest LOC + 241 host LOC).
- Callers: `layerfs-cli/src/worker.rs:69` (`layerfs_api::worker::serve_host`)
  is the only entry point, through api's re-export at lib.rs:60.

**Fix**
- Move all host-server code to the new
  `control/crates/layerfs-store/src/admin/host_worker/` (see F2):
  - `worker/host/serve.rs` → `server.rs`
  - `worker/host/write.rs` → `write_queue.rs` (taking `fault::write_outcome`
    under `cfg(feature = "worker-fault")`)
  - `worker/host/postgresql.rs` → `postgresql_queue.rs`
  - the host half of `worker/connection.rs` → `connection.rs`
- api keeps `pub use layerfs_store::admin::host_worker::serve_host` in
  `worker/mod.rs`.

**Cargo edges.** All edges already exist. store → machine, home and bridge are
in store's Cargo.toml, and api → store is in api's. Three edits are needed, none
a new dependency:
- store `nix` gains the `poll` and `socket` features (same pinned `=0.31.3`);
- store gains the features `machine = ["layerfs-machine/machine"]` and
  `worker-fault = []`;
- api forwards both features.

**Cost:** 3 files move to a different crate than planned; `connection.rs` and
`fault.rs` are split; 2 Cargo.toml files are edited; 1 re-export line changes.
All of these files move in the plan anyway.

**Gain:**
- One concern sits in one folder.
- About 20 store items go from `pub` to `pub(crate)`.
- api `worker/` becomes a 4-file leaf of one design.
- The false "serves sandbox, workspace and doctor ops" reason leaves the plan.

**Alternative.** If the owner wants both servers side by side in api, keep api
`worker/host/`. Splitting `connection.rs` and applying F2–F6 still hold.

### F2. The Store side of the host worker is split over `data/` and `admin/maintenance/`; it is a route

**What is wrong**
- The planned `data/host_worker/` puts route code under `data/`, whose role is
  what a Store holds (branch, commit, file, project).
- The planned `admin/maintenance/worker_lease.rs` separates `store.close`
  handoff from the other call kinds.

**Evidence**
- The headers describe protocols, not Store data:
  - `host_read.rs`: "The native CLI worker's bounded, read-only Store protocol."
  - `host_stream.rs`: "Both ends of a streamed host write".
  - `host_write.rs`: "The host worker's file writes … streamed in bounded
    frames".
- The only caller of their `routed` functions is `call.rs:31–37`, planned as
  `admin/route/call.rs` ("The route of one Store call").
- `host_maintenance.rs` ("Hand off one native worker's Store lease before a
  local maintenance step") is called by:
  - `compact.rs` and `upgrade.rs`, which are maintenance;
  - `sqlite/remove.rs` and `sqlite/transfer/phases.rs`, which are backend
    lifecycle, not maintenance.
- Its op is `CLOSE = "store.close"`, and its serve side, `close_at`, is
  dispatched by the host server.

**Fix**
- Create one leaf, `control/crates/layerfs-store/src/admin/host_worker/`,
  beside `admin/route/`.
- Content files (one file per wire call kind, both ends, by rule 15):
  `read.rs`, `write.rs`, `stream.rs`, `postgresql.rs`, and `close.rs` (was the
  planned `worker_lease.rs`).
- Server files (from F1): `server.rs`, `connection.rs`, `write_queue.rs`,
  `postgresql_queue.rs`.
- That makes 9 files (3–12) at depth 3. `admin/` keeps 1 loose file
  (`with_users.rs`).
- Why not `admin/route/host_worker/`: `admin/route/` would turn into a branch
  with 9 loose files, breaking rule 3.

**Cost:** 5 paths change from the plan (no extra file moves).

**Gain:**
- The host worker's Store side reads as one unit.
- `data/` again holds only call families.
- `maintenance/` holds only maintenance steps.

### F3. `machine/lifecycle/fence.rs` would be an upward dependency, and "fence" collides

**What is wrong.** `host_power.rs` cannot move into layerfs-machine as one file.

**Evidence.** In `host_power.rs`, beside its legitimate machine calls
(`layerfs_machine::{force_stop_lock, machine_of, done, with_stopped}` and
`worker::HostPaths`), these lines reach above machine:
- `crate::inventory::sandboxes` (117) and `stores_on` (122): the plan puts
  inventory in sandbox `status/`;
- `crate::machine_force::host_locked` (103): the Sandbox-teardown half of
  `machine_force`;
- `layerfs_store::open::close` (128);
- `layerfs_store::record::{lock, StoreRecord, StoreRef}` (180–182);
- `layerfs_store::lease::{held_here, holders}` (186–189).

The name collides as well. "fence" already means:
- persistence `gc/fence.rs`, the epoch fence (settled in review 4);
- sandbox `route/daemon/channel/fence.rs`.

The file is the host machine's own start and stop.

**Fix**
- Keep the record-state and worker part in
  `control/crates/layerfs-machine/src/lifecycle/host.rs`:
  - `set`, `start`, the `drain` call through `HostPaths::for_management()` and
    `Drained`;
  - about 170 LOC.
- Rule 15 lets `host.rs` stand beside the per-call files: work that two calls
  share gets a plain noun, as `boot.rs` does.
- Send the holder scan (the `blocking` collection, the Store close loop and
  `store_holders`, about 40 LOC) through the planned `MachineUsers` trait:
  - one method, for example `host_holders(home, machine) -> Result<Vec<String>>`;
  - implemented in the planned `control/crates/layerfs-sandbox/src/status/users.rs`,
    which may use store.
- The forced teardown reaches `host_locked` the same way the plan routes
  `machine.stop --force`.

**Cost:** 1 split, 1 trait method, 1 rename.

**Gain:** the placement compiles, and one more collision is gone.

### F4. Doctor `host_worker.rs` serves `machine.get`; its live part must sit in machine

**What is wrong.** All of `doctor/host_worker.rs` is planned for
`inspect/machine/worker.rs`. But `machine.rs:58` (`get`, with `live` on the host
machine) calls `crate::doctor::host_worker::live`, and the plan moves
`machine.rs` to `layerfs-machine/src/with_users.rs`. That would be machine
calling inspect.

**Evidence**
- `live`, `observe` and `evidence` (lines 17–31, 33–93, 226–236; about 85 LOC)
  use only `layerfs_machine::worker::{HostPaths, HostObservation, HostProbe}`
  and `layerfs_home::home::with_field`.
- `holder` (`layerfs_store::lease::Owner`), and `diagnose`, `retained` and
  `directory` (`target::{Budget, Checks}`, `layerfs_store::host_write::retained`)
  need doctor and store. Those stay in inspect.

**Fix**
- Split off `control/crates/layerfs-machine/src/status/worker.rs` (new): the
  host worker's live evidence for `machine.get`.
- The inspect `machine/worker.rs` keeps the doctor checks and calls
  `layerfs_machine` for `evidence`.

**Cost:** 1 split.

**Gain:** removes an upward call; `status/` holds the live probe, as rule 20
says it should.

### F5. Machine `route/host/client.rs` (687) mixes endpoint custody with the client

**What is wrong.** `worker/host.rs` ("The host's endpoint custody and
single-attempt client") holds two things:
- lines 1–330 (263 LOC): `HostPaths` and `HostGuard`, private directories, the
  lifetime lock, `bind` / `bind_writes` / `remove_endpoints` and the peer check;
- lines 331–830 (424 LOC): `request_fits`, `start`, `call*`, `exchange`,
  `greet`, `acquire`, `spawn`, `decode_answer`.

**Evidence**
- `bind*`, `try_hold` and `remove_endpoints` are server-side. They are used by
  `worker_server/host.rs:736` (`HostPaths::new`, then bind).
- `host_check_peer` is used by both ends.
- "client.rs" names only half of the file.

**Fix**
- Split into `route/host/endpoint.rs` (new, 263 LOC: the three Unix endpoints,
  their lock and bind) and `route/host/client.rs` (424 LOC).
- `route/host/` becomes 6 files. Its sibling `route/guest/` has client,
  diagnostic, frame and hello. The host has the same four plus `endpoint` and
  `stream`, a difference that fits in one line.

**Cost:** 1 split.

**Gain:** the name predicts the contents, and the server reads only
`endpoint.rs`.

### F6. Vocabulary: "host" now has three meanings in the route folders, and "lane" collides

**"host"**
- After #835, "host" names a machine:
  - `platform/host/`, `HOST_MACHINE`;
  - the host worker's client, planned as machine `route/host/`.
- It still also means the side of a Sandbox call that runs where the Sandbox
  lives:
  - sandbox `host/{call,frame}.rs` ("A host operation, typed … the
    `sandbox.host` frame"), planned as `layerfs-sandbox/src/route/host/`;
  - `workspace/serve.rs`, planned as `layerfs-workspace/src/host/` ("host side
    of Workspace operations").
- As planned, machine `route/host/` (the host machine's worker client) sits
  beside sandbox `route/host/` (a call sent to a guest worker). Same path,
  opposite meanings: rule 13.
- I reopen nothing settled: the Sandbox `route/` folder stays. Only the inner
  name changes, and the evidence for it is new (#835's rename).
- **Proposal (owner picks the word):**
  - sandbox `route/host/` → `route/on_machine/`;
  - workspace `host/` → `on_machine/`.
- The wire op `sandbox.host` keeps its name, as the engine's method strings
  did.
- The machine and store side keep "host" (`route/host/`, `admin/host_worker/`,
  `lifecycle/host.rs`), because there it really means the host machine.

**"lane"**
- The host server calls its four admission classes `Lane` (`ORDINARY`,
  `CONTROL`, `WRITES`, `POSTGRES`; 68 uses across `host.rs`, `postgres.rs`,
  `write.rs` and `connection.rs`).
- Its header says "PostgreSQL lane".
- Elsewhere "lane" is the publication lane: AGENTS.md, persistence PostgreSQL
  `lane.rs`, and `pool.rs`'s own "its lane connection" a few lines away.
- File names must not carry the word, hence `write_queue.rs` and
  `postgresql_queue.rs` rather than `*_lane.rs`.
- Renaming the `Lane` type to `Queue` or `Admission` is a code follow-up, not
  a layout change.

**Consistent already**
- "worker" means the machine's worker process. The guest worker is on TCP
  (`layerfs worker`); the host worker is on Unix sockets
  (`layerfs worker --host`).
- "guest" and "host" in machine `route/` name the two worker kinds, which
  matches `setup/guest.rs` and `platform/host/`.
- "lease" is used only for `store/<name>/open/<pid>`, with no second meaning.
  The host server's "slot" and "reservation" are distinct, which is fine.

**Stale wording**
- These say "native" for the host worker: `host_read.rs:1,10,200`,
  `host_write.rs:26`, `host_maintenance.rs:1,15`, `host_power.rs:1`,
  `machine/worker/host.rs:214`, and the machine Cargo.toml description.
- Fix the headers when the files move; there is no layout cost.

**Cost of the renames:** 2 folders (sandbox, workspace) plus header lines.

**Gain:** one word, one meaning, in every `route/`.

### F7. `home/legacy_host.rs` is a layout check, not a record

**What is wrong.** The plan sends it to `record/legacy.rs`.

**Evidence**
- Its only caller is `home/layout.rs:59` (`detect`, for layout version 3).
- It `use`s `layout::foreign_fix`, `paths`, `read_record` and
  `records::unreadable`.
- It is `pub(super)`.
- Header: "Read-only compatibility checks for development homes made before
  host naming". It refuses homes whose records still say `native`.

**Fix.** `layerfs-home/src/disk/native_names.rs`, beside `disk/layout.rs`:
- the visibility stays as it is;
- `legacy` fits any old thing (rule 12), while `native_names` says what it
  detects;
- `disk/` grows to 7 files.

**Cost:** 1 path.

**Gain:** placement backed by `use` lines, and a predictable name.

### F8. The #835 tests have no plan rows

**What is wrong.** These have no rows in `layout.json`:
- `layerfs-cli/tests/host_worker_*.rs` (18 files) and
  `tests/common/host_worker{,_proof}.rs`;
- `host_machine_stop.rs`;
- `layerfs-machine/tests/host_client.rs`;
- `layerfs-store/tests/postgresql_held_sessions.rs`.

**Fix**
- By rules 2 and 7, the cli tests become
  `api/crates/layerfs-cli/tests/host_worker/main.rs` with one module per topic
  (`calls`, `crash`, `doctor`, `drain`, `idle_ping`, `idle_queue`, `load`,
  `maintenance`, `mixed_load`, `peer`, `postgresql`, `postgresql_writes`,
  `start`, `uncertain`, `write_bounds`, `write_load`, `write_queue`, `writes`).
- The helpers go to `tests/support/`.
- None of them calls `set_var`, so none needs `standalone/`.

**Cost:** about 20 test paths.

**Gain:** the plan covers the whole #835 merge.

## 3. Proposed layout of changed folders

Scope: S/v835tree (the #835 merge), production LOC from S/loc_835.txt. Paths
are the planned new paths. Folders shown only in part say so.

layerfs-store (partial: `admin/`, and `data/` only where it changes):

```diff
  admin/                              [6871]  (partial: record/ and status/ not shown; route/ files not shown)
    with_users.rs                       512
+   host_worker/                      [2733]  (new leaf: both ends of every host-worker Store call, and its server)
+     mod.rs                              —   new
      read.rs                           211   (was planned data/host_worker/; store.call reads)
      write.rs                          424   (was planned data/host_worker/; store.write, retained outcomes)
      stream.rs                         501   (was planned data/host_worker/; streamed write, both ends)
      postgresql.rs                     276   (was planned data/host_worker/; PostgreSQL reads/writes, retire)
      close.rs                          208   (was planned admin/maintenance/worker_lease.rs; store.close handoff)
      server.rs                         661   (from control worker_server/host.rs; plan had api worker/host/serve.rs)
+     connection.rs                       —   new (split: host half of worker_server/connection.rs, 241 of 408)
      write_queue.rs                    252   (from worker_server/host/write.rs; + fault::write_outcome)
      postgresql_queue.rs               200   (from worker_server/host/postgres.rs; not "lane")
    maintenance/                      [1462]
      compact.rs                        451
      compact_report.rs                  97
      repair_mount_holders.rs           326
      upgrade.rs                        380
-     worker_lease.rs                   208   (planned; goes to host_worker/close.rs)
    route/                            [2164]  (unchanged, 9 files; files not shown)
  data/                               [1412]  (partial: only the removed folder)
-   host_worker/                      [1412]  (planned; goes to admin/host_worker/)
-     postgresql.rs                     276
-     read.rs                           211
-     stream.rs                         501
-     write.rs                          424
```

layerfs-machine (partial):

```diff
  route/                              [2387]
    mod.rs                               37
    runs_here.rs                         43
    guest/                             [745]
      client.rs                         283
      diagnostic.rs                     200
      frame.rs                          157
      hello.rs                          105
    host/                             [1562]
      client.rs                         687   (split: keeps 424 of the call side)
+     endpoint.rs                         —   new (split from client.rs: paths, lock, bind, peer check; 263)
      diagnostic.rs                     155
      frame.rs                          255
      hello.rs                          152
      stream.rs                         313
  lifecycle/                          [1358]
+   mod.rs                                —   new
    boot.rs                             339
    create.rs                           265
    force_stop.rs                       286   (planned split, unchanged here)
    host.rs                             211   (from control host_power.rs; plan named it fence.rs; holder scan, about 40 LOC, leaves)
    progress.rs                          23
    remove.rs                           108
    start.rs                            126
+   stop.rs                               —   new (planned)
  status/                              [695]  (partial)
+   get.rs                                —   new (planned)
    list.rs                             212
    live.rs                             273
    state.rs                            210
+   worker.rs                             —   new (split from control doctor/host_worker.rs: observe, evidence, live; about 85)
```

layerfs-inspect, layerfs-sandbox and layerfs-workspace (partial):

```diff
  layerfs-inspect/src/machine/         [221]  (partial)
    worker.rs                           221   (split: live part, about 85, goes to machine status/worker.rs)
  layerfs-sandbox/src/status/            [0]  (partial)
+   users.rs                              —   new (planned; also gains host_holders, about 40, from host_power.rs)
  layerfs-sandbox/src/route/           [1265]  (partial)
-   host/                             [1265]  (rename candidate: host now names the host machine)
-     call.rs                           208
-     dispatch.rs                       109
-     frame.rs                          357
-     mod.rs                              4
-     send.rs                           587
+   on_machine/                           —   new (same five files; owner picks the word)
  layerfs-workspace/src/                [515]  (partial)
-   host/                              [515]  (rename candidate, as above)
-     serve.rs                          515
+   on_machine/                           —   new (serve.rs, drain.rs, values.rs, mod.rs as planned)
```

layerfs-api:

```diff
  worker/                             [2147]
+   mod.rs                                —   new (planned; re-exports layerfs_store::admin::host_worker::serve_host)
    serve.rs                            224   (guest accept loop)
    connection.rs                       408   (split: keeps the 167 guest LOC; host half goes to store)
    fault.rs                            154   (split: write_outcome, 20, goes to store write_queue.rs)
    ops.rs                              248
-   host/                             [1113]  (planned; goes to store admin/host_worker/)
-     serve.rs                          661
-     postgresql.rs                     200
-     write.rs                          252
```

layerfs-home (partial):

```diff
  disk/                                [564]
    mod.rs                               21
    capacity.rs                          60
    guest.rs                             60
    keys.rs                              40
    layout.rs                           151
    native_names.rs                      58   (from home/legacy_host.rs; plan had record/legacy.rs)
    paths.rs                            114
    secrets.rs                           60
  record/                              [669]  (partial: does not gain legacy.rs)
    fields.rs                           104
    file.rs                             198
    generation.rs                        65
    intent.rs                           188
    observed.rs                          30
    process.rs                           84
+   token.rs                              —   new (planned)
```

## 4. Checked and left alone

- **Machine `route/guest/` + `route/host/` + `runs_here.rs`.** Right. Both are
  worker clients ("how a call reaches it", rule 20), and the siblings share one
  file set (client, diagnostic, frame, hello). `host/frame.rs` borrowing
  `guest/frame.rs`'s `ATTACHED` and `MAX_HELLO` is acceptable.
- **api `worker/` for the guest server.** Right. `ops.rs` `use`s
  `layerfs_sandbox::inspect` and store project, and `connection.rs` uses
  `ops`, so the server must sit above inspect.
- **Store `backend/postgresql/pool.rs`.** Right by rule 23. It is
  PostgreSQL-only, and it is also wired into `open.rs` (reaper) and
  `service.rs`, not only the host worker. "pool" is the standard term.
- **Store `admin/route/lease.rs`.** Right. Leases are taken on every route.
  `hold_host_locked`, `Owner` and `unmatched_host_workers` belong with them.
- **Home `socket.rs`.** Right. It is the only peer-credential code in the tree;
  its users are machine `route/host/*` and store `host_write::keep`. Home root
  stays at 4 loose files.
- **Home `wipe.rs`.** Right. It is used by store backend/postgresql, store
  host_worker and machine route/host.
- **Home `json/stream.rs`.** Right. It is a JSON reader used by store
  `stream.rs` and machine `host/frame.rs`. Moving primitives down into engine
  `layerfs-protocol` was considered and rejected: they `use` home outcomes and
  paths.
- **Inspect `machine/worker.rs` for the doctor checks.** Right. The store and
  home doctors call it for the host machine's worker; one file is enough.
- **Machine `setup/worker.rs` and `lifecycle/boot.rs`.** Right; these are the
  guest worker's install and boot.
- **CLI `worker.rs`.** Right. It is both `layerfs worker` entry points, 55 LOC.
- **A `layerfs-worker` crate.** Rejected; see the verdict.

## 5. Machine-readable list

```json
[
{"from":"control/crates/layerfs-store/src/data/host_worker/read.rs","to":"control/crates/layerfs-store/src/admin/host_worker/read.rs","op":"move","why":"route code, called only by admin/route/call.rs; data/ holds call families"},
{"from":"control/crates/layerfs-store/src/data/host_worker/write.rs","to":"control/crates/layerfs-store/src/admin/host_worker/write.rs","op":"move","why":"host-worker route for store.write, both ends"},
{"from":"control/crates/layerfs-store/src/data/host_worker/stream.rs","to":"control/crates/layerfs-store/src/admin/host_worker/stream.rs","op":"move","why":"both ends of a streamed host write"},
{"from":"control/crates/layerfs-store/src/data/host_worker/postgresql.rs","to":"control/crates/layerfs-store/src/admin/host_worker/postgresql.rs","op":"move","why":"host-worker route for PostgreSQL reads/writes and retire"},
{"from":"control/crates/layerfs-store/src/admin/maintenance/worker_lease.rs","to":"control/crates/layerfs-store/src/admin/host_worker/close.rs","op":"rename","why":"store.close handoff; also used by sqlite remove and move, not only maintenance; named after the call"},
{"from":"api/crates/layerfs-api/src/worker/host/serve.rs","to":"control/crates/layerfs-store/src/admin/host_worker/server.rs","op":"move","why":"host server uses only store/machine/home/bridge and dispatches only Store ops"},
{"from":"api/crates/layerfs-api/src/worker/host/write.rs","to":"control/crates/layerfs-store/src/admin/host_worker/write_queue.rs","op":"move","why":"write admission and threads of the host server; avoids 'lane' (publication lane)"},
{"from":"api/crates/layerfs-api/src/worker/host/postgresql.rs","to":"control/crates/layerfs-store/src/admin/host_worker/postgresql_queue.rs","op":"move","why":"PostgreSQL queue of the host server on store pool slots; avoids 'lane'"},
{"from":"api/crates/layerfs-api/src/worker/connection.rs","to":"control/crates/layerfs-store/src/admin/host_worker/connection.rs","op":"split","why":"lines 226-517 (241 LOC) are the host-only Unix connection; guest part stays in api"},
{"from":"","to":"control/crates/layerfs-store/src/admin/host_worker/connection.rs","op":"new","why":"host half of worker_server/connection.rs"},
{"from":"api/crates/layerfs-api/src/worker/fault.rs","to":"control/crates/layerfs-store/src/admin/host_worker/write_queue.rs","op":"split","why":"write_outcome is a host-only write fault switch (fault.rs:196-218)"},
{"from":"","to":"control/crates/layerfs-store/src/admin/host_worker/mod.rs","op":"new","why":"declarations for the new leaf; exports serve_host"},
{"from":"control/crates/layerfs-machine/src/route/host/client.rs","to":"control/crates/layerfs-machine/src/route/host/client.rs","op":"split","why":"lines 1-330 are endpoint custody and server bind, not the client"},
{"from":"","to":"control/crates/layerfs-machine/src/route/host/endpoint.rs","op":"new","why":"HostPaths, HostGuard, lock, bind, peer check (263 LOC from client.rs)"},
{"from":"control/crates/layerfs-machine/src/lifecycle/fence.rs","to":"control/crates/layerfs-machine/src/lifecycle/host.rs","op":"rename","why":"the host machine's start/stop; 'fence' collides with persistence gc/fence.rs and sandbox channel/fence.rs"},
{"from":"control/crates/layerfs-machine/src/lifecycle/host.rs","to":"control/crates/layerfs-sandbox/src/status/users.rs","op":"split","why":"holder scan uses inventory and layerfs_store open/record/lease (above machine); via a MachineUsers::host_holders method"},
{"from":"control/crates/layerfs-inspect/src/machine/worker.rs","to":"control/crates/layerfs-machine/src/status/worker.rs","op":"split","why":"live/observe/evidence serve machine.get in machine with_users.rs; machine cannot call inspect"},
{"from":"","to":"control/crates/layerfs-machine/src/status/worker.rs","op":"new","why":"the host worker's live evidence for machine.get (about 85 LOC)"},
{"from":"control/crates/layerfs-home/src/record/legacy.rs","to":"control/crates/layerfs-home/src/disk/native_names.rs","op":"move","why":"only caller is disk/layout.rs detect; checks records still naming native"},
{"from":"control/crates/layerfs-sandbox/src/route/host","to":"control/crates/layerfs-sandbox/src/route/on_machine","op":"rename","why":"candidate, owner picks the word: 'host' now names the host machine (#835); collides with machine route/host"},
{"from":"control/crates/layerfs-workspace/src/host","to":"control/crates/layerfs-workspace/src/on_machine","op":"rename","why":"candidate, same collision as sandbox route/host"},
{"from":"api/crates/layerfs-cli/tests/host_worker_*.rs","to":"api/crates/layerfs-cli/tests/host_worker","op":"move","why":"18 unplanned #835 tests sharing a prefix: one area binary host_worker/main.rs, helpers to tests/support/"}
]
```
