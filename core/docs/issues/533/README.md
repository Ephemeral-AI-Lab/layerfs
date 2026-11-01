# #533 LayerFS 2.0: one home, machines and Stores, one SDK

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.
> Written 2026-11-02 from the owner's review of two proposals:
> [one control center for every platform](../../proposals/host-platform-connections.md)
> (called **H**: the model, records, workers, code layout) and
> [the `layerfs` command and SDK](../../proposals/layerfs-sdk-cli.md)
> (called **S**: every call). Those two documents are the design; this page
> is the work order. Both were reviewed by four independent passes (SDK and
> command, machines, layout, architecture) and corrected before this page
> was written. Every code fact was read at `ddc7c5696b`, the head of
> #530 (#529), stacked on #528 (#527) and `layerfs_v2_0_0` `81f8758bcf`,
> which contains `origin/main` `35d36575c1`. Re-check anything those
> branches have changed since.

Part of the [#446](https://github.com/Ephemeral-AI-Lab/layerfs/issues/446)
roadmap, areas **A** (Store backends), **R** (remote storage), **B**
(Sandbox) and **P** (API). Builds on #527 (placement and backend), #529
(`sandbox.create({ platform })`), #506 and #509 (Stores, remotes,
PostgreSQL) and #463 (the layer split).

## 1. Outcome

A host service (Connect, dsh, the command, any script) reaches every Sandbox
backend from **one process and one home**, with **one setting**,
`LAYERFS_HOME`.

- **Compute is machines.** `native` and `docker` are built in, one each. VM
  kinds (`apple-container`, `microsandbox`, `lima`, `firecracker`) are
  created by name, many per host. ssh hosts are connected. A machine starts
  on the first call and its worker stops it when idle. "Platform" means only
  a machine's kind; #529's eight Sandbox platforms become the machine
  argument.
- **Storage is Stores.** A sqlite Store is one file on one machine at a
  time, created on `native` and moved with `store move`. A postgresql Store
  is one database in a fixed `layerfs` schema. There is no conversion
  between the kinds.
- **Backups** of sqlite Stores go to r2 or minio buckets through the
  `backup` family.
- **Projects** are `<store>/<project>`. Init writes into an existing Store.
- **Sandboxes** are named, bound to one project, and run on a named machine.
- **One package, `layerfs`:** one Rust library with three thin front ends
  (the `layerfs` binary, Node, Python), nine families and 54 calls, the same
  in code and in the shell.
- **Guests never read the home.** The caller reads the records, sends each
  worker call what it needs, and writes every record itself.

Done means:

- every call in S §3 works from the binary, Node and Python, with the same
  results and errors;
- every 1.1.0 call has the fate S §5 states, and nothing else is removed;
- the home has the layout of H §7.8 and nothing else; the platform settings
  are gone;
- the terminal pass (step 19) passes on macOS and Linux;
- every commit reports Production LOC, and no step leaves a Durable Store, a
  retry, or a host server in the data path.

## 2. Settled owner decisions

Recorded in H §12 (2026-11-02). Do not reopen them.

| # | Decision |
| --- | --- |
| D1 | Storage is separate from compute. |
| D2 | sqlite and postgresql are separate Store kinds; no import or conversion either way. s3 backups belong to sqlite. |
| D3 | Init names an existing Store and never creates one: `project.init(project, folder, options)`. |
| D4 | Projects are `<store>/<project>`; `LAYERFS_PROJECT` and `--project` are removed. |
| D5 | `LayerFS.init` and `LayerFS.open` are removed with no alias. |
| D6 | One package, `layerfs`; nine families; no split between agents, operators and the command. |
| D7 | Arguments: positional for what and where (owner first, optional Commit last); options for how. |
| D8 | `branch.fork` keeps its name. |
| D9 | Backups are their own family, apart from `store`. |
| D10 | `store create` makes a sqlite Store on `native`; `store connect` attaches a postgresql database with no schema option. `remove` takes no confirmation. |
| D11 | Machines: `native` and `docker` one each; VM kinds many; agents may create and remove machines; start on demand, stop when idle. |
| D12 | Sandboxes are named, bound to one project at creation; the machine is a required argument. |
| D13 | The home has three parts, `machine/`, `store/`, `backup/`; Sandboxes nest under machines, projects under Stores; no `platform/` folder; Firecracker's host is a `lima` machine. |
| D14 | One Rust implementation; the binary, Node and Python are thin front ends. |
| D15 | Machines and Sandbox backends are two layers of `layerfs-sandbox`: `machine/` (host only, behind a feature) and `backend/` (renamed from `platform/`). |

## 3. Owner gates

A step whose gate is open does not start. When a gate is open, stop and ask;
do not pick an answer. H §13 holds each question in full with its
recommended answer.

| Gate | Question | Recommended | Blocks |
| --- | --- | --- | --- |
| G0 | The base: #528 and #530 merge into `layerfs_v2_0_0` first, and this work branches from there | yes | every step |
| G1 | H and S are accepted as the 2.0 direction | — | every step |
| G2 | Step 1: the AGENTS.md amendments in H §9.1: PostgreSQL and s3 calls in the caller's process on every OS; a VM machine holds many Stores (replaces V1, one VM per Store); routes by records (replaces `useWorker`, `LAYERFS_WORKER_ENDPOINT`, `@layerfs/runtime`); the `native` machine (replaces `LAYERFS_SANDBOX=native`) | approve the wording | every step after 1 |
| G3 | #531 (one VM per PostgreSQL Store, on `layerfs_v2_0_0`) conflicts with G2's machine rule | hold #531; this issue supersedes it | 6, 10a |
| G4 | #521 (Area P: a PyO3 Python SDK, an MCP server, folder scope) overlaps steps 2, 14 and 16 | merge it first and port its Python binding and folder scope into the library; decide MCP's fate (O32) | 2, 14, 16 |
| G5 | O21a: grow `layerfs-api` into the library, not a new `layerfs-sdk` crate; O21b: keep the crates in `engine/crates/` for 2.0 (the LOC counter and layer guard scan only `core/crates` and `engine/crates`) | yes, both | 2 |
| G6 | New dependency: PyO3, and maturin to build wheels (unless G4 brings them) | yes; nothing in the tree supplies them | 16 |
| G7 | O22: progress for long calls: one `on_progress` stream, no job registry | stream | 8, 9, 10b, 11, 14 |
| G8 | O23: backups: one link per Store per bucket (reverses #509's one link per Store; replaces `send` and `store.unlink`) | yes | 11 |
| G9 | Removed 1.1.0 calls: O24 `project.remove` as a 55th call; O25 the 14 operator and diagnostic calls of `@layerfs/backend`, including background processes; O26 the trace feed (`events.subscribe`) | O24 add; O25 drop from the 54 and keep what Connect needs behind trace files; O26 trace files are the feed | 9, 14 |
| G10 | O11 widened: `machine update`, also for `native` and `docker` (image, Docker socket, WSL distribution) | yes | 6, 17 |
| G11 | O16: where the sealed Linux bundle and the built-in machines' binaries come from | a per-build user cache, pinned by the package version | 6 |
| G12 | O17: who supplies Firecracker's kernel and rootfs (not in the bundle today) | `machine create --nested` adds them to the bundle | 6 (firecracker only) |
| G13 | O3: PostgreSQL Sandboxes beyond `docker`, and reaching a loopback PostgreSQL from a guest | `native` now; VMs and ssh later | 10a |
| G14 | O4 widened: old settings, and old Connect and dsh homes | removed, with `layerfs import-env` once | 17 |
| G15 | O13 `fs` or `file`; O14 `workspace.status`; O15 package names; O19 Python style; O20 prebuilt targets; O29 `fs.diff` argument form | see H §13 | 14–16 |
| G16 | O27: the Linux `native` backend (`native` on Linux uses namespaces in 2.0) | remove it | 5 |

The other open decisions are named in the step they touch. They default to
H's recommendation unless the owner says otherwise:

- O1 secret references;
- O2 idle time;
- O5 Windows;
- O6 old PostgreSQL schemas;
- O7, a prerequisite for Init on macOS;
- O8 and O10;
- O12 moves between machines through `native`;
- O18 ssh filesystems;
- O28, the default home;
- O31 forced Sandbox close;
- O33 container names per home;
- O34 machine facts;
- O35 a lease left by a crashed process.

## 4. Rules that apply throughout

Read the root [AGENTS.md](../../../../AGENTS.md), `engine/AGENTS.md`,
`api/AGENTS.md` and `core/AGENTS.md` at the base first.

- **Worktree.** Work in your own worktree from the G0 base through
  [the parallel worktree launcher](../../../../docs/general/parallel-worktrees.md),
  on a branch named `issue533-<step>`. Build and test with that worktree's
  Cargo home and target. Never touch another worktree.
- **Disk.** Run `python3 tools/cleanup.py check` at the start, before large
  builds, after build batches, every 30 minutes and at handoff.
- **Toolchain.** Host cargo is `cargo +1.85.1`. Keep `.cargo/config.toml`'s
  ARM64 flags.
- **Store profile.** Disposable only. Durable is `NOT_RUN — disabled by
  owner until explicit reauthorization`.
- **One attempt.** There is no retry, busy handler or replay anywhere:
  transfers, machine tool commands, worker calls, the home lock. A readiness
  wait is not a retry. An outcome that is not known is `Unknown`, keeps what
  exists, and is written to `log/`.
- **Routes.** A sqlite Store's calls run in a process on the Store's kernel.
  postgresql calls and bucket I/O run in the caller's process (after G2).
  Workers listen on loopback where the forward allows, behind a secret. There
  is no host server in the data path.
- **Guests.** No guest mounts or reads the home. The caller sends a worker
  the settings each call needs and writes every record.
- **Tests.** Every test command has a wall timeout of at most 120 s. Build
  first with `--no-run`. A test that reaches the ceiling has failed as a
  hang.
- **Refactors delete tests rather than nurse them.** A test that drives a
  removed route or type is deleted and listed in the commit message, and its
  surviving behaviour is covered on the new code. Never weaken an assertion.
- **Commits.** Moves and behaviour changes never share a commit. Every
  commit message has `Production LOC: <before> -> <after> (delta <signed>)`
  with scope and method (`tools/production_loc.py`). Moves report delta 0.
- **Dependencies.** No new crate unless G6 or another owner gate allows it.
  The command's argument parser is generated from the call table, not a new
  crate.
- **Layer guard.** `core/tools/check_product_boundary.py` must pass after
  each step. Update its layer map and the LOC counter's crate lists when a
  crate is added.
- **No push.** Do not push, open a PR or merge unless the owner says so. CI
  reports and never gates. Verify each step yourself and report checks and
  gaps.
- **Docs.** Update affected architecture and API docs with the code. Keep
  proposal, implemented and measured claims apart.
- **Connect is another repository.** Never edit it from here. Its follow-up
  is Appendix A.

## 5. Target layout

Full listings with per-file production LOC are in H §10. Summary at
`ddc7c5696b` (production LOC from `tools/production_loc.py --files`):

```diff
  api/
+   node/                                     —   new (npm `layerfs`: the napi front end's JS shim and types)
+   python/                                   —   new (PyPI `layerfs`)
-   agent/                                 [981]  (ported into the library)
-   runtime/                                      (ported into the library)
-   backend/                                      (ported into the library)
-   cli/                                          (replaced by the binary)
  engine/crates/
    layerfs-api/                          [2103]  (grows into the one implementation, G5)
    layerfs-node/                         [6083]  (becomes the Node front end; registry, copies and postgres code move into layerfs-api)
+   layerfs-cli/                              —   new (the binary, also `layerfs worker`)
+   layerfs-python/                           —   new (the PyO3 front end; G4, G6)
    layerfs-sandbox/                      [8290]  (machine/ added; platform/ becomes backend/; the Docker client becomes docker_engine/)
    layerfs-persistence/                          (postgresql import.rs removed; Init's acquisition over PostgreSQL added)
    layerfs-project/                              (live add opens either Store kind)
    layerfs-store-service/                        (remote/ keeps transfers; link records move to layerfs-api)
```

The home (H §7.8). Guests never see it.

```
<home>/
  layerfs.json                 layout version, home id
  layerfs.lock                 record writes and Sandbox names (host only)
  machine/<name>/
    record.json                kind, size, lasting state
    worker.json                endpoint, secret, live forward
    known_hosts                ssh only
    sandbox/<name>/
      record.json
      work/
  store/<name>/
    record.json                kind, machine, backups
    store.sqlite               native sqlite Stores only
    open/<pid>                 leases of processes that hold it open
    project/<name>/
      record.json              rebuilt from the Store on connect or restore
  backup/<bucket>/record.json
  trace/
  log/                         init-failures.jsonl, uncertain.jsonl
```

A machine's own disk (a VM's data disk, or an ssh host's root) mounts at
`/layerfs/data` and holds `store/<name>/store.sqlite` and `sandbox/<name>/`.

## 6. Steps

Each step is one or more small commits. Each commit builds and is checked on
its own. "Checks" says what the step must show before the next starts. The
library comes first, so nothing new is written in JavaScript. The
numbering is H §11's.

### Step 1. AGENTS.md amendments (G2)
Write the H §9.1 wording into AGENTS.md once the owner approves it (G2):
PostgreSQL and bucket calls in the caller's process on every OS; a VM
machine holds many Stores; routes by records; the `native` machine; the
stale Init sentence. Docs only; Production LOC delta 0.

### Step 2. The Rust library and call table (G2, G4, G5)

1. The LOC counter and the layer guard learn `layerfs-cli` and
   `layerfs-python` (and `layerfs-nfs`, which the counter files under core
   today).
2. `layerfs-api` gets `calls.rs`: one table of the 54 calls (name,
   arguments, options, help), the one dispatcher `call(name, args) ->
   result`, and one progress stream (G7).
3. Port today's runtime, agent and backend calls family by family into the
   library, with no behaviour change. `layerfs-node` keeps only the napi
   conversion; its `registry.rs`, `copies.rs`, `postgres.rs` and
   `local_stores.rs` move into the library. The Node package calls the
   table.

Checks: today's JS and Node tests pass against the ported calls, or are
deleted and listed per §4. The dispatcher has one test per family.

### Step 3. `layerfs.json` and the layout version

A home without `layerfs.json` is today's layout; with it, the new one. Write
the home id. No behaviour change. Checks: a unit test per layout version.

### Step 4. The home layout: Stores, backups and records

Move today's Store and remote records into `store/`, `backup/` and
`record.json` once: `stores.json`, `stores.v1.json`, `remote-state/`,
`.stores/`, `<home>/<project>/`, `.runtime/projects/`, `remotes`. Sandboxes
move in step 12; machine folders in step 6. An unknown file is left alone
and reported. `layerfs.lock` guards record writes; taking it waits (bounded),
never refuses `Busy`. Checks: a 1.1.0 home with every kind of record
converts; a second run changes nothing.

### Step 5. `layerfs-sandbox`: machine and backend layers (D15, G16)

1. One pure-move commit, delta 0: `platform/` becomes `backend/`, and the
   Docker client becomes `docker_engine/`. `layerfs_sandbox::docker::Docker`
   is re-exported so callers do not change.
2. `machine/` behind a `machine` cargo feature, which the library's own
   `machine` feature turns on. It holds the Machine trait, one adapter per
   kind wrapping today's behaviour, and the kind-to-backend table.
   `machine/` uses only `docker_engine/`, the shared types, and
   `backend/`'s published types. It runs setup (VM and disk, bundle,
   worker, forward, readiness) and returns the endpoint. The library writes
   the records.
3. Host builds turn on `machine`, `tls` and `postgresql`. The guest worker is
   a second Linux build without `machine`, in its own `cargo build
   --no-default-features` invocation. The Windows front end needs its own
   cfg and dependency plan (`backend/` is `cfg(unix)` and uses `nix`).
4. If G16 says so, remove the Linux `native` backend.

Checks: the guest build has no `machine` symbols. Each adapter's probe
reports what is missing.

### Step 6. Machines (G3, G10, G11, G12)

- `machine.list/get/status/create/connect/remove/disconnect`, plus
  `update` if G10 is accepted. Each VM machine has one VM named
  `<prefix><machine>`, one data disk at `/layerfs/data` holding many Stores,
  and one worker. The worker binds loopback where the forward allows.
- `lima --nested`. `firecracker --host <lima machine>`, at most 250 per
  host. Lima and Firecracker are macOS only today (`vmType: vz`).
- ssh is connected, never created. It runs as a user that can run root
  commands; the default is root, port 22. Its host key must already be in
  `--known-hosts`. Until step 15 it needs Node.js 22.
- A create that fails before the VM exists removes what it made. Once the VM
  exists, a failure keeps it as `failed`. `Unknown` keeps everything and
  logs.
- Records in `machine/<name>/`. No `/layerfs/home` mount.

Checks: two Lima machines of different sizes; an ssh host; the refusals
`Exists`, `InUse`, and `Unsupported` with `missing`. No guest can read the
home.

Machine facts: `machine.status` reports state, free disk, the provider tool's version and
why a machine is down (O34). There is no change event; callers poll status.
Checks: each kind reports each field or says it can't.

### Step 7. Routes by records, and leases (G2)

- A call's route comes from its Store's and machine's records. `useWorker`,
  `LAYERFS_WORKER_ENDPOINT` and per-call `deploy` go away, and one process
  holds Stores on several machines.
- Every process that opens a sqlite Store holds `store/<name>/open/<pid>`.
- A lease whose process is gone is handled per O35.

Checks:
- one process uses Stores on `native`, `docker` and a VM at once;
- a stale lease is handled as O35 says.

### Step 8. sqlite Stores (G7)

`store.list/get/status/create/remove/move`. `move` and `remove` check leases
under the home lock and refuse `InUse`. A move runs its phases once each
(check, snapshot, send, verify, install, record, retire). The guest-to-host
send is new work. A move between two non-native machines goes through
`native` if O12 is accepted. The five `start_*` calls and the `vm:` refusals
go. Checks: native → docker (macOS) → VM → native, with history heads equal;
a failed phase keeps what exists.

### Step 9. Projects and Init (O7)

Projects are `<store>/<project>` everywhere. Init is a live add through the
Store's writer (`layerfs-project` live `add`); it does not seal. It runs in
the caller's process, into a sqlite Store on `native`, or on `docker` on
Linux. The macOS Init container and the VM folder copy go. Project records
in the home are rebuilt from the Store. If G9's O24 is accepted, add
`project.remove`. Checks: Init then move to `docker` and a VM. Init on
macOS's disk records the same owners, modes and name case as Init through
Docker (H §14).

### Step 10. PostgreSQL Stores (G13)

- **10a.** `store.connect/disconnect/status`. The fixed `layerfs` schema
  (O6 for old schemas). The password is a reference resolved per call; the
  full URL is handed to the daemon at Sandbox creation, as today. Remove
  `attach_postgresql`.
- **10b.** Init into PostgreSQL: a writable PostgreSQL open for live `add`,
  and Init's acquisition over PostgreSQL (`backend/postgresql/acquisition.rs`,
  new; today's PostgreSQL `schema.sql` has no acquisition tables). Switch
  tests that need a populated PostgreSQL Store to Init.
- **10c.** Only after 10b: delete `backend/postgresql/import.rs` (399),
  `PostgresHandles::import`, the binding's import and
  `start_import_postgresql`. List the deleted tests.

Checks: Init into PostgreSQL, then `docker` Sandboxes. Init time and size
against a sqlite Init, one sample each, recorded as diagnostic.

### Step 11. Backups (G7, G8)

- Calls: `backup.connect/disconnect/list/create/restore`, with
  `create(store, bucket, file?)` and `restore(store, bucket, file?)`.
- Data path:
  - snapshot on the Store's machine;
  - bytes through the caller's process (moved out of the worker);
  - machines never hold bucket keys.
- Objects are `<prefix>/<file>/store.sqlite` (whole, or a manifest) plus
  `store.sqlite.piece-NNNNNN`, with `layerfs-exported-from` metadata.
- `restore` over a Store that changed since its last backup is refused
  `Conflict`.
- `disconnect` forgets the bucket and this home's links; objects stay.
- The `local_folder` provider is not in 2.0.
- Code moves:
  - `store-service` `remote/` keeps push, pull and object naming;
  - `state.rs` and the remotes parsing move to the library's `backup/`.

Checks:
- create and restore against MinIO and R2;
- `backup list` states for all four cases;
- a Store in a VM backs up (refused today).

### Step 12. Named Sandboxes

- Calls: `sandbox.create(project, name, machine, { image })`, `list`, `get`
  and `close` (`force` if O31 is accepted).
- Sandbox names are unique across the home, under `layerfs.lock`.
- Records live under `machine/<name>/sandbox/<name>/`.
- `image` is the Sandbox image; copying its archive in at create is new
  work.
- `workspace.mount(sandbox, branch)`.
- Container names carry the home id (O33).

Checks: a sqlite project's Sandbox on another machine is `Unsupported`,
naming the Store's machine.

### Step 13. Machines on demand and idle stop
- A machine starts on the first call that needs it.
- The worker closes a Store after a quiet minute. With no open Store and no
  Sandbox for N minutes (O2), it exits and powers its guest off where the
  provider allows.
- The host notices at the next call; that is a readiness wait, not a retry.
  The ssh forward and the macOS worker container outlive a one-shot command.

Checks: none running while the home is idle; one started by the first call.

### Step 14. One package, nine families (G4, G9, G15)

The nine families and their argument order from S, with paging, `mode`,
exec sinks and snake_case results as S §2 states. They replace
`@layerfs/agent`, `@layerfs/runtime`, `@layerfs/backend` and `@layerfs/cli`.
Every 1.1.0 call gets S §5's fate and error codes keep 1.1.0's meanings.
Checks: every call in S §3 has a test.

Folder scope and MCP: Port #521's folder scope into the library if G4 keeps it. MCP follows O32.
Checks: as #521's P13 contract.

### Step 15. The binary and `layerfs worker`

The command is a native binary. The worker runs as `layerfs worker`, so VMs
and ssh hosts install no Node.js. Exit codes are 0, 2 and 125, as S §2.6
divides them. Checks: the shell and Node give identical JSON for every
call.

### Step 16. Python (G4, G6, G15)

The PyO3 module and wheels per target, with synchronous calls (asyncio per
O19) and the same errors. Checks: the Python results match Node's for every
call.

### Step 17. Settings removed (G10, G14)

Every `LAYERFS_*` platform setting is removed; `LAYERFS_HOME` and the trace
settings stay. `layerfs import-env` writes records from the current
environment once. Checks: a home made from a 1.1.0 environment works with
no settings.

### Step 18. Docs

Update the versioned manual, the SDK skill, `api/docs/` and the Python
reference. Apply the H §9.1 AGENTS.md wording that G2 approved.

### Step 19. Terminal pass

- one home, one process, the 12 cells of #516 on each machine kind;
- two Lima machines of different sizes, each with two Stores, used at once;
- Init on `native`, moves to `docker` (macOS) and one VM, and back;
- backup to and restore from MinIO and R2, and `backup list`;
- Init into PostgreSQL, then `docker` Sandboxes on it;
- a Mac with Docker stopped: `native` and the VM kinds still work;
- machines: none running while idle; one started by the first call; an agent
  creating and removing a machine;
- the same calls from the binary, Node and Python, compared.

The macOS host covers `native`, `docker`, `apple-container`, `microsandbox`,
`lima`, `firecracker` and `ssh`. A Linux host covers `native` and `docker`.
Windows is not run (O5).

## 7. Handoff

At each step's end, report:
- the commits, with their Production LOC lines;
- the checks run, with exact commands and outcomes;
- the tests deleted;
- what was not verified;
- the cache releases for the cleanup script.

## 8. Not verified at writing

H §14 lists them:
- the VM kinds on a Linux host;
- one worker serving several Stores;
- Init into PostgreSQL's cost;
- machine boot and worker start times;
- the guest-to-host send speed;
- anything on Windows.

## Appendix A. LayerFS Connect follow-up

Connect is a separate repository and changes there, not here. Most items
wait for the 2.0 call named in brackets. The rest can start now ("now") or
are pure deletion once 2.0 lands ("2.0").

**Settings**
- [ ] Drop every LayerFS setting Connect keeps (image, bundle, daemon,
  Docker socket, container prefix, placement, Sandbox kind, local folder,
  image archive, VM and ssh blocks) and their environment mirrors. Keep
  Connect's own state folder and the home path. (2.0)
- [ ] Drop the Compute settings fold and the "settings are missing" banner.
  Show the home path read-only under Settings › General. (now for the read-only
  line, 2.0 for the rest)

**Compute**
- [ ] Rebuild the page as a list of machines (name, kind, state, Stores,
  Sandboxes), then what each kind can create and what it is missing.
  [`machine.list`]
- [ ] Drop the tool probes, the health derivation, VM size defaults, worker
  start and ensure, the start route, the "starting" state, the "other places"
  guidance and the per-project VM row. Machines start on demand. (2.0; the
  dead start route now)
- [ ] Add create, connect and remove forms for VM and ssh machines.
  [`machine.create/connect/remove/disconnect`]
- [ ] Drop the stale "later" footer and the unused cloud provider ids. (now)
- [ ] Drop the simulation fixtures in the old shape, or rewrite them on
  `machine.list` results. (2.0)

**Storage**
- [ ] The overview is one row per Store: kind, machine, projects, size,
  backup state. [`store.list`]
- [ ] Stop measuring files by `docker exec stat` and local `stat`. (2.0)
- [ ] Replace the three-way sync model (ten states), the HEAD cache and its
  timers with `backup.list`'s four states. [`backup.list`]
- [ ] Replace the 500 ms job polling with one progress view.
  [`on_progress`, G7]
- [ ] Remove the PostgreSQL import dialog and route. (2.0)
- [ ] PostgreSQL: URL plus a password reference, no full URL at rest; drop
  the bridge-IP rewriting and health checks. [`store.connect`, `store.status`]
- [ ] Add "Move to machine…" on a sqlite Store. [`store.move`]
- [ ] Drop the "This machine" page; its facts move to Compute. (2.0)
- [ ] Drop the branches for builds older than #506 and #509. (now, on a
  1.1.0 or later pin)

**Backups**
- [ ] Remotes become buckets, r2 and minio only; drop the "coming"
  providers. (now for the providers) [`backup.connect/disconnect`]
- [ ] Publish and Push become `backup.create`; Pull and Load become
  `backup.restore`. Send becomes a create into a second bucket; Unlink
  becomes disconnect. [`backup.create/restore`, G8]
- [ ] A bucket page lists `backup.list(bucket)` with "pulled here" and
  "exported from". [`backup.list`]

**Projects**
- [ ] New project inits into an existing Store, then offers "Move to…". The
  "new Store" choice inside the dialog becomes a link to Storage.
  [`project.init`, `store.move`]
- [ ] Offer postgresql Stores in the picker. [step 10b]
- [ ] Drop Init job polling. [`project.init` with `on_progress`]
- [ ] Every key is `<store>/<project>`; drop bare-name and "shared"
  handling. (2.0)
- [ ] Delete projects through `project.remove`. [G9, O24]

**Sandboxes**
- [ ] New Sandbox is a name plus the machines the project's Store allows;
  drop the eight-platform menu. Offer it on the project's Sandboxes tab too.
  [`sandbox.create(project, name, machine)`]
- [ ] Use `sandbox.list` and `sandbox.get`; drop provider guessing and the
  container join. [`sandbox.list/get`]
- [ ] Replace the `docker stop` fallback for stuck Sandboxes.
  [`sandbox.close` with `force`, O31]

**Agents and sessions**
- [ ] Agents' shells get `LAYERFS_HOME` and trace settings only. (2.0)
- [ ] Replace `LayerFS.open` and `LayerFS.init` with `import … from
  "layerfs"`, in the host and in the SDK reference given to agents. (2.0)
- [ ] Chat Sandboxes get a name and a machine. [`sandbox.create`]
- [ ] Chat commands that run in the background need a replacement for the
  process calls. [G9, O25]
- [ ] Run scoping hooks today's JS client; a native package leaves nothing
  to hook. Use the library's folder scope. [G4, step 14]

**Scenarios and the platform pass**
- [ ] `-platform` becomes `-machine <name>`, checked against `machine.list`;
  drop the hard-coded platform names and the platform module hook.
  [`machine.list`]
- [ ] Port the scenario scripts off `LayerFS.open(LAYERFS_PROJECT)` and
  unnamed Sandboxes; give each run a unique Sandbox name (no retry on
  `Exists`). (2.0)
- [ ] The real platform pass uses one home with one machine per kind. (2.0)

**Host service and docs**
- [ ] One `layerfs` package; drop the runtime, backend, agent and command
  links. The storage host becomes thin `store.*` and `backup.*` calls plus
  secret-reference handling; keys are written as `file:` references (O1).
  (2.0)
- [ ] Rewrite the compute and storage UI docs; retire the sync-model and
  storage-redesign docs as history; update the new-project, Sandboxes,
  deployment and scenario docs. (2.0)

**Answered here for Connect** (each is now a gate or open decision above):
- project removal (O24);
- history and storage reads, and background processes (O25);
- Workspace status (O14);
- forced Sandbox close (O31);
- run scoping (G4);
- backup Send, Unlink and push-over-conflict (O23);
- progress, and a UI host restarting mid-move: another process sees a move
  through `store.status` (G7);
- secrets at rest (O1);
- two homes on one Docker engine (O33);
- the Linux bundle and default image (O16, G10);
- machine facts and change events (O34);
- reaching a loopback PostgreSQL (O3);
- migrating an old Connect home (O4).
