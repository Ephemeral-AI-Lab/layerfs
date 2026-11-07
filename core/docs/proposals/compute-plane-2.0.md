# Compute plane 2.0: machines, Sandboxes, exec and the control layer

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.
> Written against `layerfs_v2_0_0` at `8b701f42f0` (2026-11-06); repository
> links resolve on that branch. Production LOC is from that commit's
> `tools/production_loc.py --files`.
> **Amended 2026-11-06** (owner decisions D-7 to D-12): per-exec network
> isolation is deferred to v3.0.0; the docker platform moves to the
> `namespaces` backend; the Sandbox driver is `layerfs-sandbox-backend`; exec
> records belong to their Workspace. Superseded text is kept and marked.

This is our own spec for the 2.0 compute plane. It takes over from the
#545 index where the two differ. Owner decisions are marked **decided** with
their date. Everything else is a proposal until the owner accepts it.

## 1. Decisions

| Id | Decision | Owner, date |
| --- | --- | --- |
| D-1 | **No snapshot.** The capability's `snapshot: Option<SnapshotCapability>` field is removed from the capability contract (#567). A Family 2 machine admits only PostgreSQL Stores; the "SQLite snapshot → admitted" placement row, its test and the B12 snapshot Store kind are gone, not deferred (#557). The `snapshot` step inside `store push`, `store pull` and `backup create` stays. | owner, 2026-11-06 |
| D-2 | **Management code leaves `api/`**, after the trimmed #545 PRs merge. Supersedes #536 C1 ("in the api layer"). Where it goes is D-5 and D-6. | owner, 2026-11-06 |
| D-3 | **Exec and cancel are one model on `workspace.exec`** (§5). PR #569 (#555, the separate `step.*` family) is rejected. | owner, 2026-11-06 |
| D-4 | ~~**Per-exec network isolation is optional** for the exec SDK (§4).~~ **Superseded by D-7.** | owner, 2026-11-06 (requested) |
| D-5 | **One crate per concern, plain names for entities** (§6.2): `layerfs-home`, `layerfs-machine`, `layerfs-store`, `layerfs-sandbox` (with `workspace/`), `layerfs-control`; drivers renamed `layerfs-platform`, `layerfs-bucket`, `layerfs-isolation` (the last revised to `layerfs-sandbox-backend` by D-9). Refines D-2. | owner, 2026-11-06 |
| D-6 | **The `connect/` layer is renamed `control/`.** Layers: `core < engine < control < api`. | owner, 2026-11-06 |
| D-7 | **Per-exec network isolation is deferred to v3.0.0.** No `isolateNetwork` option and no `network` result field in 2.0; exec runs on the Sandbox's network. §4 is kept as the v3 starting point. O-1 and O-2 are superseded. | owner, 2026-11-06 |
| D-8 | **The docker platform runs the `namespaces` backend.** The docker machine's worker container (privileged; cgroup v2 with `cgroup.kill`, Linux 5.14+) runs namespaces Sandboxes inside it, as a VM does. Backends are `namespaces` (every Linux kernel) and `host_process` (macOS, stored `"native"`). The docker backend and the Engine client are deleted. A docker-machine Sandbox displays `backend: "namespaces"`; a Sandbox recorded under the old docker backend is refused with a message to recreate it. | owner, 2026-11-06 |
| D-9 | **The Sandbox driver is `layerfs-sandbox-backend`** (revises D-5's `layerfs-isolation`): it builds a Sandbox's box and puts processes in it; "isolation" is false for `host_process`. The type stays `Backend`; no stored or displayed value changes. | owner, 2026-11-06 |
| D-10 | **Exec records belong to their Workspace** (§5.2): `sandbox/<name>/workspace/<workspace-id>/exec/<execId>.json`, `execId` unique within its Workspace. `workspace.unmount`, like `sandbox.close`, is refused `Busy` while a live exec record exists; `force: true` cancels first. | owner, 2026-11-06 |
| D-11 | **The cancel call is `workspace.exec_cancel` everywhere**, including to a machine's worker, where it rides the existing `sandbox.host` frame. There is no `sandbox.exec_cancel` worker op. | owner, 2026-11-06 |
| D-12 | **Order:** exec records and cancel (§5) are built after #566 merges and after D-8, so they are written once, for the two remaining backends (§7). | owner, 2026-11-06 |

## 2. Model

```
 home records (the only control-plane authority)
   machine/<machine>/record.json       a machine: native, docker, lima, apple-container,
                                       microsandbox (firecracker, ssh: existing only)
   sandbox/<name>/record.json          a Sandbox: names its machine and its Store (#536 C2)
   store/<store>/record.json           a Store: sqlite on one machine, or PostgreSQL

 machine ──runs──▶ Sandbox ──through──▶ backend (namespaces | host_process, D-8)
                     │
                     └──mounts──▶ Workspace ──exec──▶ command (bash -c)
```

- Sandbox records live at `sandbox/<name>/`, not `machine/<m>/sandbox/<name>/`
  as #545 says. #536 C2 (owner, 2026-11-03) retired the nested path; the
  record names its machine.
- The machine kind decides how a Sandbox runs. Callers do not request an
  isolation class in 2.0 (§3, P-4).
- A sqlite Store's calls run on the Store's machine; PostgreSQL Store calls
  run in the caller (#533 G2). The placement guard (#557) refuses a live
  sqlite Store on a machine that cannot hold its file.

## 3. Scope: what we keep from the #545 PRs

Production LOC is each PR's own increment over the PR it is stacked on,
measured at its head on 2026-11-06. Every PR is stacked on #556.

| PR | Area | Prod LOC | Proposal |
| --- | --- | --- | --- |
| #556 | worker liveness, protocol 3 | +570 | **Keep.** A dead worker is `Unavailable`/`Unknown` in bounded time; a live command is not cut off. |
| #587 | per-exec netns, data-root modes | +6 | ~~**Keep, then make the netns optional** (§4).~~ The per-exec netns is **deferred to v3.0.0** (D-7); the data-root modes are for its assignee to keep or split out. |
| #558 | Sandbox-owned scratch | +486 | **Keep.** Sweep `Closed`/`Lost` only, re-check ownership before delete. |
| #557 | placement guard | +158 | **Keep without snapshot** (D-1). |
| #566 | lifecycle | +1,087 | **Trim** (P-1 to P-3). |
| #567 | contract and read models | +1,406 | **Trim** (P-4 to P-6). |
| #569 | step runtime | — | **Rejected** (D-3). |

Proposals:

- **P-1 States.** Keep the derived states the branch has (`creating`,
  `running`, `closing`, `unresolved`, `gone`) and add one: `lost`. A Sandbox
  is `lost` when its record has a runtime id and no close intent, and its
  machine answers that the runtime is not there. `sandbox.close` clears it.
  Drop `Provisioning`, `Starting`, `Ready`, `Busy`, `Draining` and `Failed`.
- **P-2 Close.** No generation compare-and-swap. The Sandbox lock and the
  `op` intent already serialize close; concurrent closes are rare under the
  one-writer model. Close is refused `Busy` while a live exec record exists
  (§5.2); `sandbox.close(name, { force: true })` cancels those commands first.
- **P-3 No lease heartbeat and no reclaimer.** The exec record (§5.2) replaces
  `ExecLease` and its heartbeat; liveness of a remote call is #556's beat.
  There is no automatic reclaim in 2.0, so `reclaim.enabled`, `--dry-run`
  and `--freeze` go.
- **P-4 No caller isolation request.** Drop `sandbox.create({ isolation })`,
  `requested_isolation`/`effective_isolation` and the strength order.
- **P-5 Typed capability, nothing more.** Keep the typed, `#[non_exhaustive]`
  `Capability` that replaces the flat `HashMap<String, bool>`: isolation,
  kernel, mount driver, idle stop and `direct_store_access` (which #557
  reads). `machine get` and `sandbox get` show it.
- **P-6 No capacity contract or extra read models.** Drop the per-Store quota,
  in-flight limit, queue bound and `Capacity` refusal, the Sandbox activity
  read model and the scheduling facts. Revisit after 2.0 with a measured need.

## 4. Exec network option

> **Deferred to v3.0.0 (D-7).** Not built in 2.0. Kept unchanged as the v3
> starting point; the `docker` row no longer applies after D-8.

`workspace.exec(ws, command, { spool?, isolateNetwork? })`

| Backend | omitted | `true` | `false` |
| --- | --- | --- | --- |
| `namespaces` (VM machines) | isolated (as #587) | a fresh network namespace for this command, with `lo` up | the machine's network, as before #587 |
| `docker` | the container's network | refused `Unavailable` | the container's network |
| `native` | the host's network | refused `Unavailable` | the host's network |

- A request the backend cannot meet is refused, never silently ignored.
- The result gains `network`: `"none"` (isolated) or `"sandbox"` (the network
  the Sandbox has), so a caller sees what took effect.
- Plumbing (paths as they are today; the driver crate becomes
  `layerfs-isolation` under D-5): an `isolate_network: bool` on `ExecRequest`
  (`layerfs-sandbox/src/types/request.rs`), passed to the helper as
  `--isolate-network`; the helper's exec stage
  (`layerfs-sandbox-helper/exec.rs`) runs `unshare(CLONE_NEWNET)` only with
  the flag, then brings `lo` up. The worker request carries the field.
- What isolation protects: the command cannot reach the worker's control
  port or other services on the machine's loopback. With `false` the port is
  reachable, but the worker's secret stays in `/layerfs/state/secret`
  (directory `0700`, root-owned), out of reach of the command's uid.

## 5. Exec and cancel

### 5.1 Calls

```
workspace.exec(ws, command, { spool?, execId? })
    execId: the caller's name for this command, unique among its Workspace's
            running commands (D-10); when omitted, control makes one and
            sends it in the first progress event: { exec: { id } }
    result: as today, plus execId and stop ("cgroup" | "group", §5.3); a
            cancelled command has kind "cancelled" and exitCode 128 + signal
            (143 TERM, 137 KILL)

workspace.exec_cancel(ws, execId, { graceMs? })   graceMs defaults to 5000
    → { outcome: "cancelled" | "ended" | "unknown", signal? }
    sends SIGTERM to everything the command started, waits up to graceMs,
    then SIGKILL; "ended" means it had already finished
```

One new call in the call table, with its CLI, Node and Python forms
(`layerfs workspace exec-cancel <ws> <exec-id> [--grace-ms N]`).

### 5.2 The exec record

A running command has
`sandbox/<name>/workspace/<workspace-id>/exec/<execId>.json` (D-10). The
Workspace id is the 64-hex stem of the Workspace record
`workspace/<workspace-id>.json`, which stays where it is; the
`workspace/<workspace-id>/` folder sits beside it, so the home layout needs
no migration.

The record holds:

- its owner process (pid, kernel start time, boot, as Store leases name theirs);
- its route: `local`, or `machine:<name>` for a machine's worker;
- its stop handle: `Cgroup { path }` or `Group { pgid, leader }` (§5.3).

It is created with create-new (a duplicate `execId` is refused without a
scan) under the Sandbox lock when the command starts, and removed when it
ends. The record is how a different process cancels a command, and how
`sandbox.close` and `workspace.unmount` know commands are running. A record
whose owner is dead is stale; close, unmount and cancel remove it under the
Sandbox lock and log it, ending the command first through its stop handle if
it still runs. Terminal unmount removes the Workspace's folder with its
record. `layerfs doctor` checks exec records too.

### 5.3 How each backend stops one command

After D-8 there are two backends and two stop handles. The backend fills the
handle in at spawn and acts on it at cancel.

| Backend | Stop handle | Group | Cancel |
| --- | --- | --- | --- |
| `namespaces` (every Linux kernel, including the docker machine) | `Cgroup { path }` | a child cgroup per command, `<sandbox cgroup>/ws-<workspace-id>/exec-<execId>`, mirroring the home layout; the helper joins it instead of the Sandbox's cgroup | SIGTERM to the cgroup's processes, wait up to `graceMs`, then `cgroup.kill`, then check it is empty; this also catches children that left the process group. One `cgroup.kill` on `ws-<workspace-id>` ends a Workspace's commands |
| `host_process` (macOS) | `Group { pgid, leader }` | its own process group (`process_group(0)` at spawn, as today); `leader` is pid plus start time, against pid reuse | `killpg` SIGTERM, wait up to `graceMs`, then SIGKILL. There is no cgroup, so a child that calls `setsid` itself escapes; documented |
| worker route | the record's route is `machine:<name>` | the worker holds the command; the cgroup path follows from the Sandbox and the ids, so the worker keeps no table | `workspace.exec_cancel` inside the existing `sandbox.host` frame (D-11); a worker that does not answer gives `unknown` |

The docker row of the first draft (a wrapper and a second Engine exec) is
dropped with the docker backend (D-8).

### 5.4 Rules

- Cancel is explicit only. No automatic timeout, no retry, no replay. A
  cancel whose result cannot be confirmed (a worker that does not answer) is
  `unknown`, and the exec record stays until its owner ends or close clears it.
- The daemon supervises no commands; cancel lives in control and the Sandbox
  backend, like exec.
- Exiting `bash` is not proof every descendant is gone; only the cgroup form
  (`namespaces`) can promise that, and the result's `stop` says which form
  ran.

## 6. Code layout

### 6.1 What is wrong today

`layerfs-control` holds the SDK as well as the management code, so the line
between "API" and "control" is not where the folders say it is:

- **The SDK value is the internal data type.** 16,130 of the crate's 18,202
  production LOC (89%) are in files that use `SdkValue`, the untyped value
  every SDK call carries. 86 public functions take `args: &SdkValue`, and
  records are kept as `SdkValue` maps (`SandboxRecord { fields: SdkValue }`).
- **Calls are dispatched twice.** A `layerfs-api` op file declares the call
  and forwards it in one line, passing its own name as a string
  (`layerfs_control::sandbox::family::sandbox(context, "close", args)`);
  the family file matches on that string again. Argument parsing and result
  shaping (`exitCode`, `stdoutBytes`, …) happen inside control.
- **The call machinery is in control.** `calls/` (the table types, checks,
  folder scope, dispatcher, text form), `Context` and `daemon/values.rs`
  ("answers as the JavaScript SDK shows them") are API code.
- **The entities import each other both ways** (`sandbox` ↔ `store`,
  `machine` → `sandbox`, `common` → every entity), mostly through helpers
  kept in the wrong module (§6.6).
- **Driver and entity share a noun.** `layerfs-machine` and
  `layerfs-sandbox` are drivers, while the machine and Sandbox records live
  in `layerfs-control`'s `machine/` and `sandbox/`.

### 6.2 Layers and crates

Decided (D-5, D-6): layers are `core < engine < control < api`. The
`connect/` layer is renamed `control/`. Drivers are named for what they drive
(platform, Sandbox backend, bucket; D-9 renamed `layerfs-isolation` to
`layerfs-sandbox-backend`); entities take the plain names. Production LOC
is the files each crate receives, at `8b701f42f0`; the total does not change.

```diff
  api/crates/                               [3131]
    layerfs-api/                            [2344]   (one folder per op family, one file per op; gains calls/, 917)
    layerfs-cli/                             [644]
    layerfs-node/                             [93]
    layerfs-python/                           [50]
  control/crates/                          [33353]   (renamed from connect/crates)
+   layerfs-home/                           [1499]   (management: layout, record files, locks, op intents, names, secrets, trace)
+   layerfs-machine/                        [1654]   (machine records, lifecycle, worker client; the name's old crate is now layerfs-platform)
    layerfs-platform/                       [4157]   (driver, one folder per platform; renamed from layerfs-machine)
+   layerfs-store/                          [5817]   (Store records, sqlite/, postgresql/, backup/, project/, placement, leases)
    layerfs-bucket/                         [3301]   (driver, one folder per bucket provider; renamed from layerfs-store-backup)
+   layerfs-sandbox/                        [6081]   (Sandbox records, launch, exec, daemon channel, workspace/; the name's old crate is now layerfs-sandbox-backend)
    layerfs-sandbox-backend/                [8610]   (driver: namespaces, host_process; renamed from layerfs-sandbox via layerfs-isolation, D-9; loses docker, about 3,200, under D-8)
    layerfs-control/                        [2234]   (moved from api/crates; keeps worker server, doctor, inventory, migrate)
```

- `layerfs-sandbox` holds Workspaces as its `workspace/` module (1,206):
  a Workspace exists only inside a Sandbox's daemon, and the engine already
  has a `layerfs-workspace` crate.
- One near-pair remains across layers: `layerfs-store` (control: which
  Stores exist and where) and the engine's `layerfs-store-service` (opens
  the database).
- Stored and displayed values do not follow crate renames (#535 G6): a
  Sandbox backend is still stored as `native` or `namespaces` (and `docker`
  for a Sandbox recorded before D-8, which is refused with a message to
  recreate it).

### 6.3 Dependencies (Cargo enforces them)

```
 layerfs-api ──▶ layerfs-control ──▶ layerfs-sandbox ──▶ layerfs-store ──▶ layerfs-machine ──▶ layerfs-home
                 (worker server,      │  └ workspace/     │                 │
                  doctor, inventory,  ▼                   ▼                 ▼
                  migrate)   layerfs-sandbox-backend  layerfs-bucket    layerfs-platform
                                  (driver)            (driver), engine  (driver)
                                                      store-service
```

A Workspace lives in a Sandbox, a Sandbox opens a Store, and a sqlite Store
lives on a machine; each arrow points from what lives inside to what it lives
on. Every control crate uses `layerfs-home`. `layerfs-api` may call any
control crate.

Rules:

1. **`SdkValue` stops at three edges:** SDK calls (`layerfs-api`), record
   files (the codec in `layerfs-home`) and worker frames (the protocol in
   `layerfs-machine`, the server in `layerfs-control`). Entity crates take
   typed requests and return typed results and refusals.
2. **One dispatch** (§6.5).
3. **Store isolation.** In `layerfs-store`, `sqlite/` and `postgresql/`
   never import each other; the kind-neutral root chooses by `StoreKind`.
4. **Workspace isolation.** In `layerfs-sandbox`, `workspace/` may use the
   Sandbox modules; the Sandbox modules do not import `workspace/`. A module
   test checks it.
5. **Management is `layerfs-home` only:** it knows no entity, and depends
   on no driver (its process facts moved below the drivers, into
   `layerfs_bridge::process`, by layerfs-project#112; §6.6).
6. **Drivers take every input as arguments**, know nothing of the home and
   write no records.

### 6.4 Inside each crate

```
layerfs-home/src/                   1499
  ├── json.rs  names.rs  op.rs  outcome.rs  secrets.rs  trace.rs
  └── home/                          623   layout, paths, records, lock, flock, entity_lock, guest

layerfs-machine/src/                1654
  ├── record.rs  spec.rs  create.rs  connect.rs  remove.rs  power.rs  status.rs
  │   get.rs  list.rs  update.rs  platforms.rs  endpoint.rs  bundle.rs  image.rs
  └── worker/                        360   client, frame, hello (the protocol; the server is in layerfs-control)

layerfs-store/src/                  5817
  ├── record.rs  identity.rs  resolve.rs  call.rs  calls.rs  open.rs  service.rs
  │   list.rs  status.rs  lease.rs  place.rs                  (kind-neutral)
  ├── sqlite/                        837   create, family, remove, transfer/ (push, pull)
  ├── postgresql/                    688   connect, open, reader
  ├── backup/                       1032   create, restore, list, record, progress, family
  └── project/                      1216   init, record, family, send/staged/tree (sqlite), postgres_init

layerfs-sandbox/src/                6081
  ├── record.rs  recorded.rs  create.rs  close.rs  get.rs  list.rs  route.rs
  │   dispatch.rs  serve.rs  deploy.rs  runtime.rs  exec.rs  output.rs  files.rs  registry.rs
  ├── launch/                        954   connection, create, local, provider
  ├── daemon/                        702   channel, pool, call, failure, internal, operator
  └── workspace/                    1206   serve, direct, unmount_guard, daemon/ (mount, commit, status, unmount, recovery)

layerfs-control/src/                2234
  ├── worker_server.rs  inventory.rs
  ├── doctor/                        618
  └── migrate/                      1218   v110, v2, volume, open, import_env
```

Planned in phase C (architecture review, 2026-11-06):

- **`layerfs-sandbox`:**
  - one table of Sandbox and Workspace ops replaces three separate lists
    (`route.rs`, `dispatch.rs`, `workspace/dispatch.rs`);
  - `recorded.rs` becomes `reopen.rs`;
  - `serve.rs` loses the docker start under D-8 and becomes `lifecycle.rs`
    (close, reclaim);
  - exec records and cancel get an `exec/` module.
- **`layerfs-store`:**
  - `calls.rs` becomes plain typed functions, removing its second string
    match;
  - `identity.rs` keeps only the identity codec, with project add and remove
    moving to `project/`.
- **`layerfs-control`:** the wrappers that pass `Related` become typed
  functions. `Related` itself stays, because its checks run inside the Store
  lock.

### 6.5 API: one folder per op family, one file per op

`layerfs-api` keeps its shape (`src/<family>/<op>.rs`, 61 ops today) and
becomes the only place the SDK is spoken. Each op file holds its table entry
and the whole API side of that op: parse the arguments into a typed request,
call **one** typed function, shape the typed result into the SDK answer.

```
layerfs-api/src/
  ├── lib.rs
  ├── calls/            table, check, scope, dispatch, keys, text, context, values   (from layerfs-control)
  ├── machine/          connect create disconnect get list platforms remove
  │                     start status stop update                       → layerfs-machine
  ├── store/            connect create disconnect get list pull push
  │                     remove status                                  → layerfs-store
  ├── project/          get init list remove                           → layerfs-store (project/)
  ├── branch/           diff fork get list merge publish               → layerfs-store (Store calls)
  ├── commit/           diff get list                                  → layerfs-store (Store calls)
  ├── file/             apply blame cp diff edit glob grep list mkdir
  │                     mv read rm stat write                          → layerfs-store (Store calls)
  ├── backup/           connect create disconnect list restore         → layerfs-store (backup/)
  ├── sandbox/          close create get list                          → layerfs-sandbox
  └── workspace/        commit exec exec_cancel mount status unmount   → layerfs-sandbox (workspace/)
                        (exec_cancel is new, §5)
```

An op whose check spans entities calls `layerfs-control` instead:
`machine.list`, `machine.get` and `machine.remove` (a machine's Stores and
Sandboxes), and `store.remove`, `store.push` and `store.pull` ("in use" by a
Sandbox). `layerfs doctor` is `layerfs-control` too.

The shape of one op file after phase B:

```rust
// layerfs-api/src/sandbox/close.rs
pub(crate) const CALL: Call = call("sandbox", "close", &[a("name", Name)],
                                   &[opt("force", Bool)], "stop a Sandbox", run);

fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = CloseRequest { name: args.name("name")?, force: args.flag("force") };
    let closed = layerfs_sandbox::close(context.home(), request)?;   // typed, no op string
    Ok(closed.to_sdk())
}
```

### 6.6 Edges to break

| Edge today | Cause | Fix |
| --- | --- | --- |
| `store`, `common`, `machine` → `sandbox::registry` | `random`, `text`, `lock` are general helpers in `sandbox/registry.rs` | move to `layerfs-home` |
| `machine` → `sandbox::deploy` | `bundle_dir`, `IMAGE_ARCHIVE` describe what a machine installs | move to `layerfs-machine` |
| `machine::worker::server` → `store`, `sandbox`, `project` | the worker's dispatcher runs every op | the server moves up to `layerfs-control`; frame, client and hello stay in `layerfs-machine` |
| `common::home::migrate`, `import_env` → every entity | migration reads every entity's old layout | move up to `layerfs-control` |
| `common::place`, `common::home::lease` → `store`, `machine` | placement and leases are Store concerns | move to `layerfs-store` |
| `store::identity` → `sandbox::files` | project-record discovery is in Sandbox code | move to `layerfs-store` `project/` |
| `store` → `inventory` (remove, push, pull "in use" checks) | the check reads Sandbox records | the check moves up to the `layerfs-control` op (§6.5) |
| `sandbox` → `workspace` (`direct.rs`, `dispatch.rs`, `registry.rs`, `serve.rs`) | Workspace routing sits in Sandbox code | `direct.rs` moves to `workspace/`; the rest is resolved when the crate is cut (rule 4), or moves up |
| `machine` → `inventory` (`list` shows Stores and Sandboxes) | a machine's summary includes derived counts | composed in `layerfs-control` (§6.5) |
| `layerfs-home` → `layerfs-isolation` (found after phase A; `home/process.rs`) | process identity (`LocalProcess`) lives in the Sandbox driver | move the process facts, with the one audited macOS unsafe file and its guard entry, below the drivers; own commit after D-9 (done by layerfs-project#112: `layerfs_bridge::process`, audited `process/darwin.rs`) |

The table covers every backward edge the import scan found at `8b701f42f0`.
Some may need a small body change rather than a move; each one is listed in
the commit that resolves it.

### 6.7 How we get there

| Phase | What | Production delta |
| --- | --- | --- |
| A | Rename `connect/` to `control/` and the three drivers; cut the entity crates (§6.2) and fix §6.6. Cargo rejects any edge left. | about 0 (moves, renames, import paths, helper relocation) |
| B | API out: `calls/`, `Context`, `daemon/values.rs` and the family dispatch move to `layerfs-api`; each op file calls one function (§6.5); control gets an `Events` type for progress. | small; measured |
| C | Typed operations per entity (rule 1): `layerfs-sandbox` and `layerfs-store`/`layerfs-machine` in parallel. Exec and cancel (§5) follow on the typed Sandbox code (D-12). | grows (typed requests, results and records); measured per commit |

Phase A also updates: the Cargo workspace members (`control/crates/*`);
`tools/production_loc.py` (scope `connect` → `control`; `layerfs-control`
leaves `API_CRATES`; new and renamed crate names);
`tools/check_product_boundary.py` (layer names and the new crates);
`tools/ci_tests.py` and the workflow paths; `connect/AGENTS.md`,
`connect/README.md` and `connect/SUPPORT.json` → `control/`; `api/AGENTS.md`
and the root `AGENTS.md` layer line; `docs/architecture/connect/` and
`docs/architecture/api/{control,routing}/` → `docs/architecture/control/`;
and #536 C1 marked superseded.

## 7. Order of work

1. Snapshot removal in #567 and #557 (D-1); the assignee is asked (#545).
2. Trims P-1 to P-6 in #566 and #567, if accepted.
3. Merge: `#556 → #587`, `#566 → #558`, `#567 → #557`. The two stacks
   conflict in `sandbox/{record,get,mod}.rs`; the second stack rebases.
4. Exec network option (§4) and exec cancel (§5) on the merged branch.
5. Phase A: rename `connect/` to `control/` and the drivers, cut the entity crates (§6.7).
6. Phase B: the SDK code moves to `layerfs-api`; one typed call per op file.
7. Phase C: typed operations, entity by entity.

Each commit builds and passes the same tests; phase A and B commits report
their production LOC delta against their parent.

**Amended order (2026-11-06).** Phases A and B are done on the
`control-split` branch, ahead of steps 1–4 (the owner chose to start before
the #545 PRs merged). From there:

1. This amendment (D-7 to D-12).
2. Merge the current `layerfs_v2_0_0` into the branch.
3. Rename `layerfs-isolation` to `layerfs-sandbox-backend` (D-9).
4. The docker platform onto `namespaces`; delete the docker backend (D-8).
5. Phase C: typed `layerfs-sandbox` operations, and in parallel typed
   `layerfs-store` and `layerfs-machine` operations.
6. Exec and cancel (§5) after #566 merges (D-12).
7. Move the process facts below the drivers (§6.6, last row).

## 8. Acceptance

- ~~`isolateNetwork`: on `namespaces`, `true` (and omitted) gives a command
  with only `lo` (up) that cannot reach the worker port; `false` reaches the
  machine's network; `true` on `docker` and `native` is refused
  `Unavailable`. The result's `network` names what ran.~~ Deferred to v3.0.0
  (D-7).
- `execId`: a caller-named id is used; an omitted one arrives in the first
  progress event; a duplicate among the Workspace's running commands is
  refused.
- Cancel: a `sleep 600` started by one process is cancelled by another
  through `workspace.exec_cancel`; the exec call returns `kind:
  "cancelled"`; the command's background children are gone (cgroup empty on
  `namespaces`); cancelling an ended command answers `ended`; a cancel
  against an unreachable worker answers `unknown` and changes nothing.
- Close and unmount: refused `Busy` while an exec record of theirs is live;
  `force: true` cancels and then closes or unmounts; a stale record (dead
  owner) is removed and logged.
- `lost` is reachable: kill a Sandbox's process tree behind control's back
  (after D-8 there is no per-Sandbox container to remove); `sandbox.get`
  answers `lost`; `sandbox.close` clears it.
- D-8: a docker-machine Sandbox runs as `namespaces` and shows
  `backend: "namespaces"`; a Sandbox recorded under the old docker backend is
  refused with a message to recreate it.
- Phases A and B change no behavior: the same test set passes before and
  after. After phase A, Cargo rejects every dependency edge outside §6.3,
  and no `layerfs-api` op file passes an op name as a string after phase B.

## 9. Open questions

| Id | Question | Recommendation | Answer |
| --- | --- | --- | --- |
| O-1 | Default for `isolateNetwork` on `namespaces` when omitted | isolated (D-4 asks for an opt-out, which implies isolated by default) | accepted, then superseded by D-7 (owner, 2026-11-06) |
| O-2 | Bring `lo` up in isolated mode | yes; local test servers need it | accepted, then superseded by D-7 (owner, 2026-11-06) |
| O-3 | Accept trims P-1 to P-6 | yes | open; belongs to the compute-plane assignee |
| O-5 | `execId` chosen by the caller, by control, or both | both (caller may name it; else control) | accepted (owner, 2026-11-06); unique per Workspace (D-10) |
