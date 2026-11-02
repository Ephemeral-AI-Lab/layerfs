# #535 The connect layer: machines, Sandboxes and Stores as typed, extensible units

> **Status:** Proposal; target LayerFS 2.x; not a released contract.
> Written 2026-11-03 from the owner's review of the crate layout in a design
> session, and four independent read-only research passes (PostgreSQL and
> sqlite on every machine; call flows and roundtrips; platform and backend
> polymorphism; the Store abstraction end to end). Every code fact was read
> at `layerfs_v2_0_0` `58b8d1742b` ("refactor(533): move the library and its
> front ends into api/crates"). Re-check anything that branch has changed
> since.

Builds on [#533](https://github.com/Ephemeral-AI-Lab/layerfs/issues/533)
(machines, Stores, one SDK), #527 (placement and backend), #529
(`sandbox.create({ platform })`), #506 (Store backends, PostgreSQL, remote
storage) and #463 (the layer split). Supersedes the #533 decisions named in
§3.

## 1. Outcome

The engine keeps persistence, the daemon, the overlay, Workspaces and the
kernel routes. Everything that reaches a machine or the network moves into
a new **connect** layer, organised so that what LayerFS supports, and how a
new platform is added, can be read from the folder tree.

- **Layers** are `core < engine < connect < api`. Nothing in `engine/`
  depends on `connect/`; the boundary guard enforces it.
- **Three connect crates, three questions:**
  - `layerfs-machine`: *where* Sandboxes can run: one folder per platform
    (`native`, `docker`, `apple-container`, `microsandbox`, `lima`,
    `firecracker`, `ssh`).
  - `layerfs-sandbox`: *how* a Sandbox runs on that machine's kernel: one
    folder per backend (`docker`, `native`, `namespaces`). Docker stays its
    own backend.
  - `layerfs-store-backup`: *how* a Store is backed up to a bucket (sqlite
    today; PostgreSQL is refused with a reason).
- **Every machine uses both Store kinds.** A machine holds any number of
  sqlite Stores, and every PostgreSQL Store is usable from it. A sqlite Store is
  still on exactly one machine (the one-kernel rule); PostgreSQL Store calls
  still run in the caller's process (#533 G2).
- **Choices are typed and decided once.** Store kind, platform, Sandbox
  backend and kernel route are enums parsed once per call; every refusal
  comes from one support table, and `connect/SUPPORT.json` is generated
  from it.
- **Extensible by folder.** Adding a platform is one new folder plus about
  three lines outside it. Adding a backend, a Store kind or a kernel route is
  one new folder plus the enum variant the compiler then demands everywhere.
- **Fewer roundtrips.** No ping before a worker call; Workspace calls go
  straight to the daemon; no scans of every Sandbox record per call.

Done means:

- the layout of §5 exists and nothing else; the boundary guard, LOC counter,
  CI and tools know the `connect` layer;
- `layerfs machine platforms` and `connect/SUPPORT.json` agree with every
  real refusal (a test compares them);
- a PostgreSQL Sandbox and a sqlite Sandbox run on every platform §7.3 marks
  supported, and every platform it does not support refuses with the reason
  the table gives;
- no `get_text("kind") == Some("postgresql")` remains in `layerfs-api`; no
  `LAYERFS_KERNEL` remains anywhere; `DaemonSetup` carries `DaemonStore` and
  `KernelRoute`;
- every move commit reports `Production LOC … (delta 0)` and passes the #463
  move checks; every behaviour commit passes the full check set (§11);
- the terminal pass (§10, phase 8) passes.

## 2. Settled owner decisions

Taken by the owner in the design session of 2026-11-03.

| # | Decision |
| --- | --- |
| D1 | The engine keeps persistence, daemon, overlay, Workspace and kernel routes; Sandbox and network code move out. |
| D2 | PostgreSQL stays inside `layerfs-persistence`, parallel to SQLite (`backend/{sqlite,postgresql}`). |
| D3 | NFS stays parallel to FUSE. Both become peers over a new route-neutral `layerfs-mount-ports` crate (ports, operations, phase timing moved out of `layerfs-fuse`). |
| D4 | A new top layer `connect/` is created. This **supersedes #533 O21b** ("the crates stay in `engine/crates/` for 2.0"), whose reason (the guard and LOC counter scan only `core/crates` and `engine/crates`) no longer holds since `api/crates` exists. |
| D5 | Connect crates: `layerfs-machine`, `layerfs-sandbox`, `layerfs-store-backup`. `layerfs-remote-storage` is renamed `layerfs-store-backup` and absorbs `layerfs-store-service/src/remote/`; there is **no** `layerfs-remote-sync` crate. |
| D6 | The machine `Kind` is renamed **`Platform`** (the word the calls and records already use: `machine.create(name, platform)`, record field `"platform"`). |
| D7 | One folder per platform. Each make/unmake pair lives in one file: `create.rs` (create + remove), `power.rs` (start + stop), ssh `connect.rs` (connect + disconnect). The public call stays `remove`. |
| D8 | Docker is **not** treated as a namespaces machine; it keeps its own backend. |
| D9 | Every machine can use both PostgreSQL and sqlite Stores, and each machine can hold multiple of each. |
| D10 | The support matrix is generated (`connect/SUPPORT.json`) and exposed as the call `machine.platforms`. |

## 3. Gates and open decisions

Each row gives the answer recommended in the design session of 2026-11-03
and reviewed there by the owner. A row becomes settled when it is recorded
in this issue's DECISIONS log (phase 0); no phase starts before the rows it
needs are recorded.

| Gate | Question | Recommended answer | Needed by |
| --- | --- | --- | --- |
| G1 | Supersede #533 O3, G13 and the "Step-12 deferral closed" row ("PostgreSQL Sandboxes: native and docker; VMs and ssh later"), and the AGENTS.md wording that follows from them? | **Yes.** A PostgreSQL Sandbox runs on every machine where §7.3 marks the platform supported (no attach; G3). The daemon already opens PostgreSQL and Docker already runs without a Store volume; what remains is policy plus three mechanisms (namespaces binds, `reach`, credential hand-off; §7.4). | phase 5 |
| G2 | Supersede #533 DECISIONS "Step 5.2" ("No trait: … one `match` per call")? | **Yes, narrowly:** one **crate-private, sealed** trait per extension point (`Adapter`, `BackendAdapter`), with no default methods, reached through **one exhaustive `match`**. No public trait, registry, factory or plugin. The step's reason ("every caller already knows the kind") still holds. | phase 3 |
| G3 | How does a machine hold PostgreSQL connections? | **Owner, 2026-11-03: every PostgreSQL Store is usable from every machine automatically**, with no attach records or calls (this replaces the earlier recommendation of `store.attach`/`store.detach`). `Adapter::reach` rewrites the URL per platform and `sandbox.create` checks reach inside its own request. A per-machine URL (a host that sees the server at another address) is deferred; such a machine is refused with the address it tried. | phase 5 |
| G4 | May Workspace calls reach a VM Sandbox's daemon directly (its control keys in the home Sandbox record, its port forwarded to host loopback)? | **Yes.** Native and docker Sandboxes already keep these keys in the home (0600, same user); the loopback + secret rule is unchanged. | phase 6 |
| G5 | May a worker connection carry several requests (today one per connection, `layerfs-cli/src/worker.rs` header)? | **Defer.** It changes the worker contract; revisit after R1 and R2 are measured. | phase 7 |
| G6 | The Sandbox backend folder `native` is confused with the platform `native` (a Linux `native` platform uses `namespaces`), and it holds two things: the shared "daemon as a local process" plumbing that `namespaces` also uses, and the macOS host-process backend. What to do? | **Split, and rename what is internal.** Shared plumbing moves to `src/local/`; the backend becomes `backend/host_process/` (matching `IsolationModel::HostProcess`); `NativeMacosProvider` → `HostProcessProvider`, `Runtime::MacosNative` → `Runtime::HostProcess`, `Backend::Native` → `Backend::HostProcess`. **Stored and displayed values stay `"native"`** (Sandbox records `runtime`, `sandbox.get`, `SUPPORT.json`, `machine.platforms`, which add a `description`). Changing the public value is a later, separate decision. The pre-2.0 Linux reopen/stop/delete path goes to `host_process/legacy_linux.rs`, deleted in its own commit once the 2.0 conversion is confirmed to reclaim those Sandboxes. | phase 1 (1.8, 1.9) |
| G7 | Enable PostgreSQL Sandboxes on `lima`, `apple-container`, `microsandbox`, `firecracker` before their network facts are verified live? | **No.** Until a live receipt records the host alias (lima `host.lima.internal`, the others' gateway alias) and firecracker's outbound NAT, `support.rs` refuses with that specific missing fact. | phase 5b, 5c |
| G8 | sqlite Stores on `docker` on a Linux host share the host kernel; should their Store calls run in the caller (Local executor) instead of the docker machine's worker? | **No; Worker on every host.** Local needs the Store directory's privacy rule changed (the daemon is root in the container; the caller is not; #534) or a root caller, which does not belong in a structural refactor. After R1 a worker call is one loopback connection, and Commits and Workspace I/O do not use the worker. `Profile.executor(host)` keeps Local a one-line change if measurements justify it. The comment on `Kind::route` that says otherwise is corrected. | phase 4 |

## 4. Facts at the base

Read at `58b8d1742b`. Paths are repository-relative.

**Layers and guard.** `tools/check_product_boundary.py`:
`LAYER_ORDER = ("core", "engine", "api")`,
`CRATE_ROOTS = ("core/crates", "engine/crates", "api/crates", "api")`;
`layerfs-sandbox` and `layerfs-remote-storage` are `ENGINE`. Unknown crates
fail closed.

**Machines** live in `engine/crates/layerfs-sandbox/src/machine/` (23 files,
3,683 lines, feature `machine`, host builds only). `kind.rs` is the one
table (`Kind`, `HostOs`, `Route`, `backend()`, `route()`, `worker_bind()`,
`guest_stop()`); `setup.rs` matches once per call on the kind;
`vm/{apple_container,microsandbox,lima,firecracker,ssh}.rs`, `docker/`,
`native.rs` are the adapters; `worker.rs` holds setup steps 2–4 (prepare,
install, launch, ready); `guest.rs` the `Guest` enum. `machine/` never uses
`backend/`. Firecracker setup is spread over `lima.rs:34`
(`FIRECRACKER_ROOT`), `lima.rs:259` (`firecracker_files`), `worker.rs:20`
(`FIRECRACKER_DIR`) and `guest.rs:39` (`FIRECRACKER_GUEST`).

**Sandbox backends** are `backend/{docker,native,namespaces}` with
`docker_engine/`, `runtime.rs` (`Runtime` and `Sandbox` enums, about 25
match arms over 14 + 11 methods), `types/` and the helper binary. The
namespaces backend requires a Store directory (`backend/namespaces/create.rs`
`check_store`, `helper_spec.rs` `store: PathBuf`,
`bin/layerfs-sandbox-helper/mounts.rs` binds it read-write) and shares the
guest's network namespace (`init.rs` unshares mount, PID and IPC only).
`backend/native/` holds two things: the shared local-process lifecycle
(`Local`, `LocalPaths`, `LocalPhase`, `LocalProcess`, `listener`, `search`,
`streams`, `linux/`, `macos/`), which `backend/namespaces/{create,provider,
sandbox,exec,paths,host}.rs` import, and the one native backend,
`NativeMacosProvider` (macOS only since #533 O27; on Linux the module only
reopens, stops and deletes pre-2.0 records). Sandbox records store
`"runtime": "native"` (`api/crates/layerfs-api/src/host/runtime.rs` `NATIVE`).

**Kernel route** is an env string: native writes `LAYERFS_KERNEL` `fuse` on
Linux and `nfs` on macOS (`backend/native/provider.rs`), namespaces writes
`fuse` (`backend/namespaces/create.rs`), Docker leaves it unset (FUSE); the
daemon re-parses it (`layerfs-daemon/src/application/filesystem.rs`).
`ProviderCapabilities.mount_driver` exists and nothing reads it.

**Stores.** `layerfs-persistence` selects with `BackendSelection` and keeps
`PostgresHandles` beside `Handles` (whose `create`/`open` refuse non-sqlite).
`layerfs-store-service` `bootstrap.rs` has `enum Writer {Sqlite, Postgres}`;
`StoreService::open_postgres` builds a sqlite config and overwrites its
backend (an unused path). `layerfs-bridge` `DaemonSetup` carries
`existing_store: Option<StoreManifest>` and `postgresql: Option<DaemonPostgres>`
with an exclusivity check; `ProviderKind::Postgres` is refused everywhere.
The daemon re-derives the kind (`application/owner.rs:99-112`). In
`layerfs-api` the kind is a string compared at about 15 sites and the record
is re-parsed per call (`host/stores.rs` `identity_of`).

**PostgreSQL limits** are policy, not mechanism: `family/sandboxes.rs:266-280`
(only `native` or `docker`, "VMs and ssh later (O3)"),
`family/sandboxes.rs:367-372` (a PostgreSQL Sandbox is never sent to a
worker), `host/local.rs:53-59` ("a namespaces Sandbox binds a Store file"),
`machine/record.rs:233-244` (`stores_on` skips PostgreSQL). The daemon
already opens PostgreSQL; Docker already runs with no Store volume. A
loopback URL is rewritten only for Docker Desktop
(`backend/docker/sandbox.rs` `desktop_url`). Password references (`env:`,
`file:`) resolve only on the caller (`family/postgres.rs`).

**Roundtrips.** Every worker-routed call opens two connections: `ping`
(`machine/start.rs` `answering`) then the call (`worker/client.rs`). Every
Workspace call on a VM goes caller → worker (`sandbox.host`) → daemon.
`sandbox_of` scans every `machine/*/sandbox/*/record.json` per call
(`family/sandboxes.rs` `all`). The worker rewrites the Store record on every
`sandbox.host` call (`family/sandboxes.rs` `in_worker`). `family/deploy.rs`
has two copies of each deploy resolver (`docker_here`/`docker_in_worker`,
`native_here`/`native_in_worker`/`guest_settings`).

**Adding a platform today** touches 8 to 10 files and about 20 match arms
(`kind.rs` 9 arms, `setup.rs` 8, `probe.rs`, `guest.rs`, the adapter,
`spec.rs`, `api` `machine/{calls,record,start}.rs`, `calls/table.rs`).

## 5. Target layout

`A` new, `←` moved from (sandbox paths are under
`engine/crates/layerfs-sandbox/`), `M` modified, `·` unchanged.

```
Cargo.toml                               M  members += "connect/crates/*"
AGENTS.md  engine/AGENTS.md  api/AGENTS.md  M  layer order and ownership
connect/                                 A
├── AGENTS.md                            A  connect → core, engine; engine never → connect
├── README.md                            A  the three crates; how to read SUPPORT.json
├── SUPPORT.json                         A  generated: platform × host → backend, kernel, store kinds
└── crates/
    ├── layerfs-machine/                 A  WHERE (host builds only; replaces the `machine` feature)
    │   ├── src/
    │   │   ├── lib.rs                   ← src/machine/mod.rs
    │   │   ├── platform.rs              ← src/machine/kind.rs   Platform, HostOs, Profile table
    │   │   ├── adapter.rs               A  sealed Adapter trait + the one match
    │   │   ├── support.rs               A  Platform × HostOs × StoreCaps → Support | Refusal
    │   │   ├── options.rs               A  OptSpec: create/connect options each platform declares
    │   │   ├── probe.rs  spec.rs  error.rs  facts.rs   ← src/machine/
    │   │   ├── setup/
    │   │   │   ├── calls.rs             ← src/machine/setup.rs  public calls, one line each
    │   │   │   ├── worker.rs            ← src/machine/worker.rs steps 2–4
    │   │   │   ├── guest.rs             ← src/machine/guest.rs
    │   │   │   ├── tool.rs              ← src/machine/tool.rs
    │   │   │   ├── forward.rs           ← src/machine/vm/forward.rs
    │   │   │   └── vm.rs                ← src/machine/vm/shared.rs
    │   │   └── platforms/
    │   │       ├── native/              mod.rs  power.rs  network.rs
    │   │       ├── docker/              mod.rs  power.rs  network.rs  cli.rs  unix.rs  windows.rs
    │   │       ├── apple_container/     mod.rs  observe.rs  create.rs  power.rs  network.rs
    │   │       ├── microsandbox/        mod.rs  observe.rs  create.rs  power.rs  network.rs
    │   │       ├── lima/                mod.rs  observe.rs  create.rs  power.rs  network.rs  template.rs
    │   │       ├── firecracker/         mod.rs  observe.rs  create.rs  power.rs  network.rs  scripts.rs  host.rs
    │   │       └── ssh/                 mod.rs  observe.rs  connect.rs  power.rs  network.rs  host_key.rs
    │   └── tests/                       ← tests/machine_*.rs, tests/support/standin.rs; + support.rs (A)
    │
    ├── layerfs-sandbox/                 ← engine/crates/layerfs-sandbox   HOW (host + guest)
    │   └── src/
    │       ├── lib.rs  runtime.rs (public enums → adapter)  helper_spec.rs (binds)
    │       ├── local/                   ← backend/native/  shared "daemon as a local process" (not a backend)
    │       │   ├── create.rs  process.rs  listener.rs  search.rs  streams.rs  groups.rs
    │       │   └── os/linux/  os/macos/ (darwin.rs: the crate's only unsafe)
    │       ├── backend/
    │       │   ├── mod.rs               M  Backend {Docker, HostProcess, Namespaces}, sealed BackendAdapter, the one match
    │       │   ├── docker/              ·  (create.rs: ExtraHosts host-gateway on Linux)
    │       │   ├── host_process/        ← backend/native/{provider,sandbox,exec}.rs   macOS: this user's process, loopback NFS
    │       │   │   └── legacy_linux.rs  ← the pre-2.0 Linux native reopen/stop/delete path (G6)
    │       │   └── namespaces/          M  imports crate::local, not backend::native
    │       ├── docker_engine/  json/ (pub)  types/ (LaunchRequest, Bind, Capabilities)
    │       └── bin/layerfs-sandbox-helper/        (mounts.rs: binds, not one store path)
    │
    └── layerfs-store-backup/            ← engine/crates/layerfs-remote-storage   BACKUP
        ├── src/
        │   ├── backup/                  ← layerfs-store-service/src/remote/
        │   │   ├── mod.rs  create.rs (push.rs)  restore.rs (pull.rs, load)  heads.rs  copies.rs
        │   │   └── link.rs (settings.rs + state.rs)
        │   └── bucket/                  ← src/*.rs, s3_api/, cloudflare_r2/ (→ r2/), minio/, local_folder/
        └── tests/                       ·  + link.rs, copies.rs ← store-service tests/remote_*.rs

engine/crates/
├── layerfs-bridge/src/store.rs          A  DaemonStore, StoreAuthority
├── layerfs-bridge/src/support.rs        A  KernelRoute {Fuse, Nfs}, StoreKind {Sqlite, Postgresql}
├── layerfs-bridge/src/daemon_setup.rs   M  store: DaemonStore, route: KernelRoute; format version bump
├── layerfs-persistence/src/store/selection.rs  A  StoreSelection, StoreFile capability
├── layerfs-store-service/               M  OpenedStore::file(); no fake path; − src/remote/
├── layerfs-daemon/                      M  match DaemonStore; match KernelRoute; LAYERFS_KERNEL gone
├── layerfs-mount-ports/                 A  ← layerfs-fuse/src/{ports.rs, operations/, phase.rs}
├── layerfs-fuse/  layerfs-nfs/          M  peers over mount-ports
└── layerfs-overlay/  layerfs-workspace/  layerfs-project/  ·

api/crates/layerfs-api/src/
├── place/                               A  store_ref.rs  plan.rs  executor.rs  deploy.rs (← family/deploy.rs)
├── machine/index.rs                     A  derived index: sandbox → machine, machine → stores
├── machine/platforms.rs                 A  machine.platforms
├── store/route.rs  family/{sandboxes,stores,backups,projects}.rs  M  plan() → executor.run(); StoreRef
├── family/postgres.rs                   M  reach check step used by sandbox.create (worker side)
└── calls/table.rs                       M  + machine.platforms; options from Profile

tools/check_product_boundary.py (+ test), production_loc.py, ci_tests.py, worktree_build.py   M
.github/workflows/{ci,nightly}.yml       M
```

Dependency rules:

```
layerfs-api ──► layerfs-machine (optional: feature `machine`) ──► layerfs-sandbox ──► layerfs-bridge
            ──► layerfs-sandbox
            ──► layerfs-store-backup ──► layerfs-store-service, layerfs-persistence, layerfs-storage
connect     ──► engine, core only
engine      ──✗ connect
```

## 6. Object model

### 6.1 Store

```rust
// layerfs-persistence: one selection, one Handles; file-only operations are a capability
pub enum StoreSelection { Sqlite(SqliteSelection), Postgres(PostgresSelection) }
pub trait StoreFile {
    fn snapshot(&self, dest: &Path, progress: &mut dyn FnMut(u64, u64)) -> StorageResult<SealedStore>;
    fn heads_digest(&self) -> StorageResult<[u8; 32]>;
}
impl OpenedStore { pub fn file(&self) -> Option<&dyn StoreFile> }

// layerfs-bridge: the daemon contract, plain values only
pub enum DaemonStore {
    Sqlite   { locator: String, install: Option<SqliteInstall> },
    Postgres { url: String, schema: String, synchronous_commit: String },
}

// layerfs-api place/store_ref.rs: parsed once from store/<s>/record.json
pub enum StoreRef {
    Sqlite   { name: String, machine: String },
    Postgres { name: String, server: PgServer },   // usable from every machine (G3)
}
impl StoreRef {
    fn from_record(name: &str, record: &SdkValue) -> Result<Self>;
    fn calls_run(&self) -> CallSite;         // Sqlite → its machine; Postgres → caller (G2 of #533)
    fn usable_on(&self, machine: &str) -> bool;
    fn daemon_store(&self, reach: &dyn Reach) -> Result<DaemonStore>;  // password resolved here
    fn caps(&self) -> StoreCaps;             // { backup, movable_file, one_kernel }
}
```

A Store record names its machine(s); machine records never list Stores. A
derived index (`machine/index.rs`) answers `machine.get` and `InUse`.
PostgreSQL leases use the same `open/<pid>` files as sqlite, so
`store.disconnect`'s `InUse` check holds.

### 6.2 Machine

```rust
pub enum Platform { Native, Docker, AppleContainer, Microsandbox, Lima, Firecracker, Ssh }
pub struct Profile {
    pub name: &'static str, pub tool: Option<&'static str>, pub how: How,  // BuiltIn | Created | Connected
    pub hosts: &'static [HostOs],
    pub backend: fn(HostOs) -> Option<Backend>,
    pub kernel: fn(HostOs) -> Option<KernelRoute>,
    pub executor: fn(HostOs) -> ExecutorKind,          // Local | Worker
    pub worker_bind: Ipv4Addr, pub guest_stop: &'static str,
    pub options: &'static [OptSpec],
}

pub(crate) trait Adapter: Sync {                         // sealed; no default methods
    fn probe(&self, tools: &Tools) -> Probe;
    fn observe(&self, s: &Spec) -> Result<State>;
    fn guest(&self, s: &Spec) -> Result<Guest>;
    fn create(&self, s: &Spec, b: &Bundle) -> Result<Running>;   // ssh: connect
    fn remove(&self, s: &Spec) -> Result<()>;                     // ssh: disconnect
    fn start(&self, s: &Spec, b: &Bundle) -> Result<Option<Running>>;
    fn stop(&self, s: &Spec) -> Result<()>;
    fn status(&self, s: &Spec) -> Result<Status>;
    fn reach(&self, s: &Spec, url: &Url) -> Result<Url>;
    fn forward(&self, s: &Spec, guest_port: u16) -> Result<SocketAddr>;
}
pub(crate) fn adapter(p: Platform) -> &'static dyn Adapter { match p { /* one arm per platform */ } }
```

### 6.3 Sandbox backend

```rust
pub enum Backend { Docker, HostProcess, Namespaces }  // stored names: "docker", "native", "namespaces" (G6)
pub(crate) trait BackendAdapter: Sync {
    fn capabilities(&self) -> Capabilities;            // kernel_routes, isolation, binds
    fn launch(&self, req: LaunchRequest) -> Result<Sandbox, LaunchFailure>;
    fn reopen(&self, rec: &SandboxRecord) -> Result<Sandbox, RuntimeError>;
}
pub struct LaunchRequest {
    pub daemon: Executable, pub setup: Vec<u8>,        // opaque encoded DaemonSetup
    pub binds: Vec<Bind>,                              // sqlite: the Store dir; PostgreSQL: none
    pub options: BackendOptions,
}
```

The public `Runtime` and `Sandbox` enums stay (`Runtime::MacosNative` is
renamed `Runtime::HostProcess`) and dispatch through `adapter(backend)`.
Record values do not change. `namespaces` and `host_process` both build on
`crate::local` (directories, spawn, listener, streams, per-OS facts).

### 6.4 Placement and execution

```rust
pub struct Plan { executor: Executor, backend: Backend, route: KernelRoute,
                  store: StoreRef, binds: Vec<Bind> }
pub fn plan(home: &Path, store: &StoreRef, machine: &str, op: Op) -> Result<Plan, SdkRefusal>;

pub enum Executor { Local, Worker(Endpoint) }
impl Executor { pub fn run(&self, req: Request, events: &mut dyn FnMut(SdkValue)) -> Outcome }
```

`place/deploy.rs` resolves deploy settings on the side that runs the call;
the `*_here` copies are deleted.

## 7. Behaviour

### 7.1 Store calls

`StoreRef::from_record` → `plan()`:
- PostgreSQL → `Executor::Local` in the caller's process.
- sqlite → `Executor::Local` on `native`, else `Executor::Worker`: dial the
  recorded endpoint once; only a failure **before anything is sent** starts
  the machine (§8 R1).

Why sqlite needs a worker at all: a sqlite Store's openers are the process
running Store calls **and every Sandbox daemon** (each opens the file to
Commit). They share the WAL index (`-shm`, memory-mapped), POSIX locks and
the Linux directory lock, which hold only between processes on **one kernel
and a local filesystem** (`kernel_guard`: ext4, xfs, btrfs; apfs, hfs). A
machine with its own kernel (a VM, an ssh host, Docker Desktop's VM) runs
its daemons there, so its Stores live on its disk and its Store calls must
run in a process on that kernel: the worker. Opening the file across kernels
(a shared folder) breaks locks and the WAL index; sending daemon Store I/O
back to the caller is a host server in the data path, which root AGENTS.md
forbids. PostgreSQL is the location-free kind, which is why its calls are
always in the caller. `docker` on Linux shares the host kernel but keeps the
worker (G8).

### 7.2 Sandboxes and Workspaces

`sandbox.create(project, name, machine)`: `plan()` checks
`StoreRef::usable_on(machine)` and `support(platform, host, caps)`, takes
`backend` and `route` from the profile, builds `daemon_store` with
`adapter.reach` (password resolved in the caller and sent in the request,
never written in a guest), sets `binds` (sqlite: the Store directory;
PostgreSQL: none), and runs `sandbox.create` through the executor. The reply
carries the daemon endpoint (`adapter.forward`) and control keys, written to
the home Sandbox record (0600).

`workspace.mount | exec | commit | unmount` go from the caller straight to
the daemon (G4). The worker is used for create, close and reopen.

### 7.3 Support matrix (target)

Backend column: Rust name, with the stored and displayed value in brackets
where it differs (G6).

| Platform | Host | Backend | Kernel | sqlite Store calls | PostgreSQL Store calls | PostgreSQL Sandbox |
| --- | --- | --- | --- | --- | --- | --- |
| native | macOS | host_process (`native`) | nfs | in process | in process | yes (works today) |
| native | Linux | namespaces | fuse | in process | in process | phase 5a (binds empty) |
| native | Windows | — | — | worker (records only, O5) | in process | no backend |
| docker | any | docker | fuse | worker (G8) | in process | phase 5a (Linux: `host-gateway`) |
| ssh | any | namespaces | fuse | worker | in process | phase 5a; loopback URLs refused |
| lima | macOS | namespaces | fuse | worker | in process | phase 5b, after G7 receipt |
| apple-container | macOS (Apple silicon) | namespaces | fuse | worker | in process | phase 5b, after G7 receipt |
| microsandbox | any | namespaces | fuse | worker | in process | phase 5b, after G7 receipt |
| firecracker | macOS (M3+, 15+) | namespaces | fuse | worker | in process | phase 5c, after NAT and G7 receipt |

`reach(url)`: native unchanged; docker loopback → `host.docker.internal`;
lima → `host.lima.internal`; apple-container, microsandbox → their gateway
alias; firecracker → `172.30.<n>.1` plus outbound NAT on the Lima host; ssh
unchanged, loopback refused. A rewrite is refused when the URL asks for
`sslmode=verify-full` (the certificate names the original host); such a Store
needs a URL whose host every machine can verify.

### 7.4 Enabling PostgreSQL on every machine

PostgreSQL Store calls already run in the caller's process on every OS and
stay there. Enabling PostgreSQL is about **Sandboxes**: the daemon on a
machine must reach the server and receive credentials safely.

Every PostgreSQL Store is usable from every machine (G3): the Store record
is unchanged (`url` without password, `password` reference, `schema`,
`synchronous_commit`) and has no machine field. There are no attach records
or calls.

```
sandbox.create <project> <name> <machine>
  1. support(platform, host, Postgres)?        refusal names the missing fact (G7)
  2. url' = adapter.reach(record.url)           per-platform host alias; ssh refuses loopback
  3. password resolved IN THE CALLER (env:/file:); full URL sent in ONE worker request
  4. reach check, inside the same request, on the machine: connect, read the schema
     binding, compare with the record's binding (proves reach and the same Store);
     a failure refuses the create naming the address tried
  5. DaemonStore::Postgres{ url: url' + password, schema, sync }; binds = []
     executor.run (worker on VMs, ssh, docker; local on native)
```

Per layer:

| Layer | Change |
| --- | --- |
| api | `StoreRef::Postgres` (no machine). Remove the refusals (`family/sandboxes.rs:266-280`, `host/local.rs:53-59`) and the "never send to a worker" shortcut (`family/sandboxes.rs:367-372`). `stores_on` counts PostgreSQL. The worker's `sandbox.host` takes the already resolved `postgresql` value from the request, never resolves a password and never writes the URL into the guest's Store record. PostgreSQL Stores take `open/<pid>` leases like sqlite, so `store.disconnect` sees `InUse`. |
| bridge | `DaemonStore::Postgres` (phase 2); the URL stays out of `Debug`. |
| sandbox | namespaces: `binds: Vec<Bind>`, empty for PostgreSQL (`helper_spec.rs`, `mounts.rs`; `check_store` only for a non-empty bind). docker: `ExtraHosts: host.docker.internal:host-gateway` on Linux. host_process: works today. |
| machine | `Adapter::reach` per platform. firecracker: outbound NAT (masquerade for `172.30.0.0/16`) where the Lima host is created. ssh: refuse loopback URLs. |
| daemon | nothing new; it already opens PostgreSQL (`application/owner.rs:288-303`). |

Credentials: the password is resolved only in the caller, travels only in
the one worker request (loopback + secret) and in the Sandbox's private
daemon setup file (0600, out of `Debug`), and is deleted with the Sandbox.
While a PostgreSQL Sandbox runs on a VM or ssh host, that guest holds the
password in the daemon's setup file; this is inherent in a daemon that
dials the server, and the same as today's docker and native Sandboxes.

Rollout: **5a** native Linux, docker on every host, ssh (native macOS
already works); **5b** lima, apple-container, microsandbox, each after a
receipt records its host alias and a passing reach check; **5c** firecracker,
after NAT is added and a receipt shows a remote server and a host-loopback
server both reachable. Each step flips rows in `support.rs`; `SUPPORT.json`
and `machine.platforms` follow.

Tests: `reach()` rewrites and refusals per platform; a `sandbox.create`
whose reach check meets a wrong schema is refused; one end-to-end run per enabled platform against a
server reachable from that machine (create, mount, exec, Commit, then read
the Commit back from the caller's process).

## 8. Roundtrips

| # | Change | Saves |
| --- | --- | --- |
| R1 | No ping before a worker call; start only after a pre-send connect or hello failure | 1 connection + hello + ping per worker call |
| R2 | Workspace calls go caller → daemon (G4) | 1 hop per mount/exec/commit/unmount on VMs |
| R3 | `machine/index.rs` replaces the `sandbox_of` scan | per-call reads that grow with the home |
| R4 | `StoreRef` passed down; no `identity_of` re-parse | 2–3 record parses per call |
| R5 | Worker writes the Store record only when it changed | 1 write per `sandbox.host` call |
| R6 | `ensure_built_ins` once per process | 2 reads per call |
| R7 | Several requests per worker connection (G5, deferred) | 1 connection + hello per later call |

## 9. Extension recipes

| To add | Touch |
| --- | --- |
| a platform | `platforms/<p>/` (`mod.rs` with `PROFILE` and `ADAPTER`, `observe`, `create`, `power`, `network`); one `Platform` variant; one `adapter()` arm; a `Guest` variant only for a new shell transport |
| a Sandbox backend | `backend/<b>/`; one `Backend` variant; one `adapter()` arm; one record type |
| a Store kind | persistence `backend/<k>/` + `StoreSelection` variant; bridge `DaemonStore` variant; api `StoreRef` variant + `caps()` |
| a kernel route | `engine/crates/layerfs-<route>/` over `layerfs-mount-ports`; one `KernelRoute` variant; daemon `match` |
| a bucket provider | `layerfs-store-backup/src/bucket/<p>/` |

## 9a. SDK and CLI

The CLI, the Node package (`api/node`) and the Python package (`api/python`)
are generated from the call table; this issue only adds to them. After it:
**9 families, 60 calls** (today's 57 plus `machine.platforms`,
`machine.start` and `machine.stop`). Nothing is renamed or removed here;
`store.move` → `store.push` + `store.pull` is #536.

| | Call | Change |
| --- | --- | --- |
| ★ | `machine.platforms` | CLI `layerfs machine platforms`, SDK `layerfs.machine.platforms()`: the support matrix for this host and build (platform → backend, kernel route, sqlite and PostgreSQL support, what is missing); same content as `connect/SUPPORT.json` |
| ★ | `machine.start` | CLI `layerfs machine start <name>`: starts a stopped machine and its worker and waits until ready (the `Adapter::start` path that calls today run on demand); a running machine answers at once. For UIs, which must not boot a VM because a list was opened |
| ★ | `machine.stop` | CLI `layerfs machine stop <name>`: stops the worker and, where it has one, the VM; disks stay. Refused `InUse` while Sandboxes run on it (detail `related` lists them); built-in `native` answers at once |
| ✎ | `machine.list`, `machine.get` | each machine adds `supports` (below) |
| ✎ | `sandbox.create` | same arguments; a PostgreSQL project's Sandbox now runs on native Linux, docker on every host and ssh (5a), then the VM platforms (5b, 5c), with the reach check inside the call |
| ✎ | `machine.create`, `machine.connect` | options are declared by each platform's profile; an option the platform does not take is refused; `layerfs help machine create` lists them per platform |
| ✎ | refusals | the same codes; messages for unsupported platform and Store combinations come from `support.rs` and name the missing fact |
| ✎ | `sandbox.create … docker` | works without `machine update docker --image` (phase 4): with no image named, the bundle's `sandbox-image.tar` is loaded into Docker once and its digest used; `--image` still overrides |
| | Workspace calls, worker calls | same shapes; faster (no ping, R1; no worker hop on VMs, R2) |

**Calls that never start a machine:** every `*.list`, `*.get` and
`*.status` call, and `machine.platforms`. They answer from records and live
probes only; a stopped machine reads as `stopped`. Calls that need the
machine's worker (Store calls on a sqlite Store there, `sandbox.create`,
push and pull) still start it on demand, as today, for programmatic callers.

**`machine.platforms` result** (one entry per platform; the same rows as
`connect/SUPPORT.json`, for this host and build):

```jsonc
{ "host": "macos", "platforms": [
  { "platform": "lima", "how": "created",            // built_in | created | connected
    "backend": "namespaces", "kernel": "fuse",
    "sqlite": "worker",                              // in_process | worker | null
    "postgresql": { "store_calls": "in_process", "sandbox": false,
                    "reason": "lima host alias not verified (G7)" },
    "ready": false, "missing": ["tool:limactl"], "version": null,
    "options": [ { "name": "cpus", "kind": "int" }, { "name": "memory", "kind": "size" },
                 { "name": "disk", "kind": "size" }, { "name": "image", "kind": "text" },
                 { "name": "nested", "kind": "bool" } ],
    "description": "a Lima VM (vz); Sandboxes run in Linux namespaces" } ] }
```

It replaces `machine.list`'s `can_create` (removed in the same commit; the
Connect port reads `machine.platforms`).

**Per-machine `supports`** on `machine.list` and `machine.get`: the
platform row applied to that machine, so a UI never guesses:
`{ "sqlite": true, "postgresql_sandbox": false, "reason": "…" }`.

Docs updated in the same commits as the behaviour:
`docs/versioned/2.0.0/{calls,machines-and-stores,limitations}.md` and
`.agents/skills/layerfs-js-sdk/references/methods/{machine,sandbox}.md`.

## 10. Work order

Moves and behaviour changes never share a commit. Every commit carries
`Production LOC: <before> -> <after> (delta <signed>)`.

**Phase 0 (docs).** `core/docs/issues/535/DECISIONS.md` recording D1–D10
and the gate answers as the owner confirms them; supersession rows for #533
O21b (D4), O3, G13 and the "Step-12 deferral closed" row (G1) and Step 5.2
(G2) in #533's DECISIONS; `connect/AGENTS.md`, `connect/README.md`; root,
engine and api AGENTS.md layer paragraphs and the PostgreSQL Sandbox wording.

**Phase 1 (moves, delta 0).**
1. `layerfs-sandbox` → `connect/crates/`; guard (`LAYER_ORDER` += connect,
   `CRATE_ROOTS` += `connect/crates`), LOC counter, CI, tools.
2. `src/machine/` → `connect/crates/layerfs-machine` (`setup/` and flat
   platform files); feature `machine` becomes an optional dependency;
   `Backend` and `json` exposed by `layerfs-sandbox`.
3. Each platform file split into `platforms/<p>/`.
4. Firecracker pieces out of `lima`, `worker` and `guest` into
   `platforms/firecracker/host.rs`.
5. `Kind` → `Platform` (about 143 references; the record value
   `"platform"` is unchanged).
6. `layerfs-remote-storage` + `store-service/src/remote` →
   `connect/crates/layerfs-store-backup/{bucket,backup}`; `installer_user`
   made `pub`.
7. `layerfs-fuse/src/{ports.rs,operations/,phase.rs}` →
   `engine/crates/layerfs-mount-ports`.
8. `backend/native/` split (G6): shared plumbing → `src/local/`
   (`os/linux`, `os/macos`); the macOS backend → `backend/host_process/`;
   the pre-2.0 Linux path → `backend/host_process/legacy_linux.rs`; the
   guard's unsafe allowance `src/backend/native/macos/darwin.rs` →
   `src/local/os/macos/darwin.rs`.
9. Rename (G6): `NativeMacosProvider` → `HostProcessProvider`,
   `Runtime::MacosNative` → `Runtime::HostProcess`, `Backend::Native` →
   `Backend::HostProcess`; stored and displayed values stay `"native"`.
10. (separate commit, only if confirmed) delete `legacy_linux.rs` once the
    2.0 conversion is shown to reclaim pre-2.0 Linux native Sandboxes; the
    commit lists the tests it deletes.

**Phase 2 (typed contracts).** bridge `DaemonStore`, `StoreAuthority`,
`KernelRoute`; daemon matches them; `LAYERFS_KERNEL` deleted; persistence
`StoreSelection`, `StoreFile`, one `Handles`; store-service fake path
removed; api `StoreRef` replaces every kind string check and `identity_of`.

**Phase 3 (polymorphism; G2).** `Profile` table and `Adapter`;
`BackendAdapter`; `support.rs`; `machine.platforms` (shape in §9a, replacing
`can_create`) and per-machine `supports`; generated
`connect/SUPPORT.json` with a test that compares it, and every refusal, to
`support.rs`.

**Phase 4 (one executor; G8).** `place/{plan,executor,deploy}.rs`; `*_here`
copies deleted; a Sandbox always launches through its machine; docker keeps
the Worker executor on every host and the `Kind::route` comment is
corrected; `machine.start` and `machine.stop` over `Adapter::start`/`stop`,
and every list/get/status call checked not to start a machine; a docker Sandbox with no image named loads the bundle's
`sandbox-image.tar` into Docker once and uses its digest (§9a); R1, R3, R4,
R5, R6.

**Phase 5 (PostgreSQL everywhere; G1, G3, G7; §7.4).**
- **5a.** namespaces `binds`; `Adapter::reach` for native, docker and ssh;
  docker `ExtraHosts host-gateway` on Linux; the reach check inside
  `sandbox.create`; PostgreSQL leases;
  the worker takes the resolved URL from the request; refusals removed for
  native Linux, docker and ssh; `verify-full` rewrite refusal.
- **5b.** lima, apple-container, microsandbox: a live receipt under
  `receipts/` records each host alias and a passing reach check; then their
  `reach` and `support.rs` rows.
- **5c.** firecracker: outbound NAT on the Lima host; a receipt with a
  remote server and a host-loopback server both reachable; then its row.

**Phase 6 (direct Workspace calls; G4).** `Adapter::forward`; daemon keys
in the home Sandbox record; Workspace calls skip the worker.

**Phase 7 (optional; G5).** Persistent worker connections. Not run while
G5 is deferred.

**Phase 8 (terminal pass).** Recorded in `TERMINAL-PASS.md` with receipts
under `receipts/<commit>/`, all on one product commit and one sealed Linux
bundle (`tools/linux_bundle.py`), Disposable Stores only. It passes only
when every item passes; a NOT_RUN or an unexpected refusal fails it, except
items the owner has excused in writing (Windows, Durable, R2 without an
account, as in #533).

1. **sqlite regression:** the 12 #516 cells (`api/node/test/verify/cells/`)
   PASS on all seven platforms (`native`, `docker`, `apple-container`,
   `microsandbox`, `lima`, `firecracker`, `ssh`) and the Linux rows, as in
   #533 step 19; each VM platform's cells move the Store out of `native`
   and back.
2. **PostgreSQL Sandboxes:** for every platform §7.3 enables, the 12 cells
   PASS with a PostgreSQL Store on that machine (a server reachable from it;
   for VMs and ssh, both a remote server and, where `reach` rewrites, a
   host-loopback server).
3. **Many Stores per machine:** one VM machine and `native` each hold two
   sqlite Stores and use two PostgreSQL Stores; Sandboxes of all four run
   at once and Commit, and each Commit reads back from the caller.
4. **Refusals:** every platform or Store combination §7.3 does not support
   is refused with the text `support.rs` gives, and `connect/SUPPORT.json`
   and `layerfs machine platforms` agree with every observed result.
5. **Contracts:** no `get_text("kind") == Some("postgresql")` in
   `layerfs-api`; no `LAYERFS_KERNEL` in the tree; `DaemonSetup` carries
   `DaemonStore` and `KernelRoute`; the boundary guard reports no engine →
   connect edge.
6. **Roundtrips:** a trace (`LAYERFS_TRACE`) of a worker-routed Store call
   shows one worker connection (R1), and of `workspace.exec` on a VM
   Sandbox shows no worker hop (R2).
7. **Binding parity:** the same cell results from the `layerfs` binary,
   Node and Python on `native` and one VM platform.
8. **Checks:** the full §11 set passes on the final commit.
9. **Machine power:** on every VM platform and ssh, `machine.stop` then
   `machine.list`/`machine.status`/`store.status` leave the machine stopped;
   `machine.start` brings it back and the 12 cells pass after it.

Phases 2 and 3 may run in parallel worktrees (Store code vs machine and
sandbox code). Phase 5 needs phases 2 and 4.

## 11. Checks

Move commits (#463 MIGRATION_SPEC): identical `cargo metadata --no-deps`
graph except the declared crate changes; identical per-crate production LOC
(delta 0, plus declaration-only `mod.rs` files); `git diff -M --stat` shows
renames only, apart from import paths.

Every commit, through the selected worktree's registered resources, each
test selection under a wall timeout of at most 120 s:

```
cargo +1.85.1 test --locked --all-targets --no-run
cargo +1.85.1 clippy --locked --all-targets -- -D warnings
cargo +1.85.1 fmt --check
python3 -B tools/check_product_boundary.py
python3 -B tools/check_fuser_integrity.py
python3 -B -m unittest discover -s tools -p 'test_*.py'
```

Behaviour commits add the affected crates' tests, and from phase 3 the
`SUPPORT.json` comparison test. Every Store created by a test is Disposable.

## 12. Risks

- **Setup format.** The `DaemonSetup` version bump makes existing native and
  namespaces Sandbox config records unreadable; such Sandboxes are refused
  and recreated, or one release keeps the old decoder.
- **Cross-branch conflicts.** Branches based before `58b8d1742b` conflict
  with phase 1 moves; merge `layerfs_v2_0_0` into them, never rebase.
- **Guest networking.** Host aliases and firecracker NAT are unverified
  (G7); the support table refuses until a receipt exists.
- **Credentials.** PostgreSQL URLs with passwords travel in worker requests
  and daemon setup files only, never in guest records (`DaemonSetup` keeps
  them out of `Debug`). A guest running a PostgreSQL Sandbox holds the
  password in that Sandbox's setup file until it closes (§7.4).
- **TLS.** A host-alias rewrite breaks certificate host verification;
  `reach` refuses it for `sslmode=verify-full`; the Store's URL must then
  name a host every machine can verify (a per-machine URL is deferred, G3).
- **Two names for one backend.** After G6 the Rust name is `host_process`
  and the stored value is `native`; `SUPPORT.json` and the crate docs show
  both until the public value is changed by a later decision.
- **Test coupling.** `api/crates/layerfs-api/tests/sandbox_operations.rs`
  includes `layerfs-sandbox/tests/support/engine.rs` by `#[path]`; it is
  repointed in phase 1.

## 13. Not in this issue

One daemon serving several Stores; Docker as a namespaces machine (D8);
changing the stored and displayed backend value `"native"` (G6); a Local
executor for docker on Linux (G8); persistent worker connections (G5,
phase 7 optional); a long-lived per-user process for the CLI.

**Follow-up: [#536](https://github.com/Ephemeral-AI-Lab/layerfs/issues/536)**
(`layerfs-control`, starts after this issue's terminal pass) moves all
management into one library (`api/crates/layerfs-control`; Store calls into engine `store-service/methods/`), puts Sandboxes at `sandbox/<name>/`,
unifies intents and locks, and replaces `store.move` with `store.push` (native → machine) and `store.pull` (machine → native). G3's
owner answer (no attach records) is already applied here.

**Follow-up: [#537](https://github.com/Ephemeral-AI-Lab/layerfs/issues/537)**
(LayerFS Connect on the #535/#536 shape: Storage, Compute and projects;
starts after #536) ports the UI. It relies on this issue's
`machine.platforms`, `machine.start`/`machine.stop` and per-machine
`supports` (§9a).
