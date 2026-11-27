# Review 835: layerfs-sandbox and layerfs-machine

Scope: the planned `control/crates/layerfs-sandbox` and `control/crates/layerfs-machine`, recursively, after #835's renames (`platform/host/`, `backend/process/`, root `process/` → `spawn/`). Source: S/v835tree. LOC: S/loc_835.txt (prod column). Layer order inside control: home < machine < store < sandbox < workspace < inspect.

## 1. Verdict

**Yes, but moderately.** No new crate, crate merge or concept-wide rename is needed. Three changes are needed:

1. **A layering correction (must fix).** Two planned machine files reach up into store, sandbox and inspect: `with_users.rs` and #835's `lifecycle/fence.rs`. As planned they cannot compile in `layerfs-machine`.
2. **A folder rename.** Sandbox `route/host/` now collides with #835's meaning of "host". Rename it `route/machine/`.
3. **A file split.** `spawn/daemon.rs` holds the process backend's own `create`. Move it into `backend/process/instance/` so both backend variants share one design.

The rest is right as planned:

- `spawn/` is the right name and the right place.
- Machine `route/{guest,host}/` is right.
- The entity skeleton holds in both crates.

## 2. Findings, ranked by value

### F1. Planned machine files depend upward (store, sandbox, inspect). Must fix.

**What is wrong.** `control/crates/layerfs-control/src/machine.rs` (154 prod) is planned as `layerfs-machine/src/with_users.rs`. `control/crates/layerfs-control/src/host_power.rs` (211, #835) is planned as `layerfs-machine/src/lifecycle/fence.rs`. Both call crates above machine. `layerfs-machine/Cargo.toml` depends only on bridge, home and (today) platform, and the settled order forbids machine → store/sandbox/inspect.

**Evidence: `use` lines and call sites.**

`with_users.rs` (control `machine.rs`):

- `use crate::inventory;` builds `RELATED` from `inventory::stores_on` / `sandboxes_on` / `hosted_on` / `can_*`. Inventory is planned as `layerfs-sandbox/src/status/inventory.rs`.
- l.58 `crate::doctor::host_worker::live`. Planned as `layerfs-inspect/src/machine/worker.rs`.
- l.73 and l.172 `crate::host_power::{start,stop}`. These go to `fence.rs` (see below).
- l.175 `crate::machine_force::stop`. That file is split between machine and sandbox.
- l.153 and l.156 `inventory::stores_on` and `layerfs_store::service::reconcile_machine` (store).
- l.191 `layerfs_store::postgresql::check::forget_machine` (store).
- l.201 `layerfs_sandbox::image_lock::ImageLock::dir` (sandbox).

`fence.rs` (control `host_power.rs`):

- `layerfs_store::open::close`, `layerfs_store::lease::{held_here,holders}` and `layerfs_store::record::{lock,StoreRecord,StoreRef}`, all in `store_holders` and the stop loop.
- `crate::inventory::{sandboxes,stores_on}`.
- `crate::machine_force::{force_record,host_locked}`.

What it uses that may stay in machine: `layerfs_machine::{force_stop_lock, machine_of, done, with_stopped}`, `worker::HostPaths` and `record::MachineRecord`.

**Fix: keep both files in machine, and route every upward reach through the trait machine declares.** This is rule 22: "checks flowing the other way go through a trait the lower crate declares".

- **`users.rs` (machine, was `related.rs`, 45).** `MachineUsers` keeps today's six reads and checks and gains four hooks for what a machine call does to its users:
  - `reconcile(home, machine, incarnation)`: machine.start settles the Store intents of a previous worker incarnation;
  - `forget(home, machine)`: machine.remove forgets PostgreSQL attachments and the image copy locks;
  - `holders(home, machine) -> Vec<String>`: the host stop's Store leases and Sandboxes;
  - `force_stop(home, events, machine)`: the Sandbox side of `machine stop --force`, already planned as sandbox `lifecycle/machine_stop.rs`.
- **`lifecycle/fence.rs` (machine): split.**
  - Stays: `set` (the record fence), `start` (wait out a draining worker, start the host worker), the drain call and `Drained`.
  - Leaves: `store_holders`, the idle-copy `open::close` loop and the Sandbox scan. They become `MachineUsers::holders`, implemented in sandbox `users.rs`.
- **`with_users.rs` (machine).** Each call takes `&dyn MachineUsers` and never names `inventory`, `layerfs_store` or `layerfs_sandbox`. The host-worker live section of `machine.get` (l.51–60) moves up to `api/crates/layerfs-api/src/families/machine/status/get.rs`, which already may call `layerfs-inspect` `machine/worker.rs`. No hook can carry it, because inspect sits above sandbox too.
- **Sandbox `status/users.rs` → `layerfs-sandbox/src/users.rs`.** Once it implements action hooks (reconcile, forget, force stop), it is no longer a status read (rule 20: status/ only reads). At the root it mirrors machine's root `users.rs` (same word). Sandbox root loose files: `deploy.rs`, `family.rs`, `users.rs` (3 of 4).
- **Same theme, smaller.** Machine `setup/facts.rs` (68) returns `layerfs_sandbox_backend::RuntimeError`, which becomes sandbox `runtime/failure.rs`, an upward reach. It must take the home `JsonError` the plan already gives the moved `json/` module.

**Cost and gain.**

- Touched: 2 machine files edited (`users.rs`, `with_users.rs`), 1 split (`fence.rs`), 1 sandbox file moved (`users.rs`), 1 api file edited (`status/get.rs`) and `setup/facts.rs`.
- Gain: the plan compiles under the settled crate order, and machine.stop/start/remove read in one place per side.
- Store's settled `admin/with_users.rs` has the same `inventory` reach (control `store.rs` l.7). That is not my scope; I flag it for the store reviewer.

### F2. Sandbox `route/host/` collides with #835's "host". Rename it `route/machine/`.

**What is wrong.** After #835, "host" means the host machine, this computer:

- `platform/host/`;
- machine `route/host/`, the host-worker client;
- `lifecycle/fence.rs` "host stop fences";
- the `machine/<host machine>/run` run root.

Sandbox `route/host/` means something else: a "host operation", i.e. a Sandbox op run on whichever machine runs the Sandbox, in this process or in a guest worker. Its headers say so:

- `route.rs`: "run in this process, or sent to its machine's worker";
- `host/call.rs`: "which operation it is, from the one table of them";
- `dispatch.rs`: "runs on the Sandbox's machine, here or in its worker".

Two neighbouring crates with `route/host/` meaning two things breaks rule 13 (no collision with another concept). The plan's own note already says "host dispatch is how a Sandbox call reaches its machine".

**Fix.** `control/crates/layerfs-sandbox/src/route/host/` → `route/machine/` (`call.rs`, `dispatch.rs`, `frame.rs`, `send.rs`, `mod.rs`). Sandbox `route/` then reads `daemon/` (how a call reaches the daemon), `machine/` (how it reaches the machine), `direct.rs` (straight to a forwarded daemon) and `inspect.rs`. The wire name `sandbox.host` stays. Renaming the `Host` request type is optional code follow-up, not layout.

**Cost and gain.**

- Cost: 5 files move, and about 27 non-test files change `crate::host::` paths (`grep -rln 'crate::host::'` over control and api).
- Gain: one meaning of "host" across control. `route/` becomes a clean pair of targets.

### F3. `spawn/daemon.rs` holds the process backend's create. Split it so the variants match.

**What is wrong.** `backend/namespaces/instance/create.rs` (354) is that variant's daemon start. The process variant has no `create.rs`. Its start is `Local::create` in `spawn/daemon.rs` (sandbox-backend `shared/daemon.rs`, 243): `#[cfg(target_os = "macos")] pub fn create(...)`, documented as "A process daemon's start, macOS only". With it go `prepare` and `LocalCreateFailure`, about 109 nonblank lines from l.187.

So a variant member sits in the shared folder, and the two variants do not share one design (rules 9 and 10). The rest of `daemon.rs` really is shared. `Local::new` / `process` / `naming` / `paths`, `LocalPaths`, `LocalPhase` and `LOG` are used by both:

- `namespaces/provider.rs`: `use crate::shared::{Local, LocalPaths}`;
- `namespaces/paths.rs` wraps `LocalPaths`;
- `process/provider.rs`: `Local::process`.

**Fix.**

- New `backend/process/instance/create.rs` takes the macOS `create`, `prepare` and `LocalCreateFailure`.
- `spawn/daemon.rs` keeps the shared run directory, paths, phases and `LOG`.
- The process `instance/` then has `create`, `exec`, `groups`, `handle`; namespaces has `cgroup`, `create`, `exec`, `handle`, `paths`, `spec`.

**Cost and gain.** 1 split plus 1 new file. Afterwards a reader finds each backend's start in `<variant>/instance/create.rs`.

**`spawn/` itself is right, kept at the root beside `backend/`.**

- Its only users are `backend/namespaces/*`, `backend/process/*` and `runtime/dispatch.rs` (grep `crate::shared::`). Rule 10 puts a non-member beside the group's parent.
- It does not belong under `route/daemon/`. That folder is the control channel (TCP/Noise/Hello, pool, fence) and uses nothing from `spawn/`.
- It does not belong under `lifecycle/` either. Those are entity calls, which reach spawning only through `Runtime`.
- "spawn" predicts its contents: the run directory, the daemon's listener line, the spawned search path and the spawned command's pipes.
- One loose end: the `os` alias and `mounts_under` (today in `shared/mod.rs`) must live with `backend/confined.rs` now that review 3 moved `confined` there. Put them in `backend/mod.rs`. The plan row for `spawn/mod.rs` still says "confined reads".

### F4. Two `Backend` enums and a `NATIVE` constant: one concept in two places.

**What is wrong.**

- Sandbox `backend/select.rs` (today's `runtime.rs`, 195) declares its own `pub enum Backend { HostProcess, Namespaces }`, plus `pub const NATIVE: &str = "process"`, `NATIVE_ON_LINUX` and `LOCAL`.
- `layerfs-sandbox-backend/src/backend/kind.rs`, planned as machine `table/backend.rs`, already declares `pub enum Backend { Process, Namespaces }` with `stored_name()` and `parse()`.
- Users of the duplicate: `reopen.rs` (`use crate::runtime::NATIVE`), `files.rs` (`use crate::runtime::{LOCAL, NATIVE}`), `inspect/process.rs`, `launch/local.rs` (`Backend::HostProcess`, `NATIVE_ON_LINUX`).

This breaks rule 23 (one concept, one place) and rule 19 (same word everywhere). It is also where most of the leftover "native" identifiers live.

**Fix.** `select.rs` returns `layerfs_machine::table::Backend` (sandbox may use machine), parses with `Backend::parse` and prints with `stored_name()`. Drop the local enum and `NATIVE`, and rename `NATIVE_ON_LINUX` to `PROCESS_ON_LINUX`. There is no layout change.

**Cost and gain.** About 6 files edited. Afterwards there is one backend word, and "native" is gone from the Sandbox crate's identifiers.

### F5. `lifecycle/local.rs` → `lifecycle/boot.rs`

**What is wrong.**

- Its header still reads "Native and namespaces Sandboxes".
- Since the docker backend's retirement, both backends are "local", so the word now distinguishes nothing. It also clashes with #835, where machine `local.rs` became `route/runs_here.rs`.
- What the file does: it boots a Sandbox's daemon in its run folder on the side that runs it (`start`, `start_named`). It also places and checks that folder (`directory`, `owned`, `mark_owner`), which `record/reopen.rs` and `record/discovery.rs` call.

**Fix.**

- Rename to `control/crates/layerfs-sandbox/src/lifecycle/boot.rs`. That is the accepted word (rule 12), and machine `lifecycle/boot.rs` means the same thing: the shared start work a create or start runs.
- Avoid `start.rs`, which is a machine call name; Sandbox has no start call.
- The other suspect names are fine. `backend/confined.rs`, machine `route/runs_here.rs` and machine `table/*` say nothing native or local in their file names.

**Cost and gain.** 1 rename plus 3 path updates (`lifecycle/serve.rs`, `record/reopen.rs`, `record/discovery.rs`).

### F6. Plan bookkeeping left stale by #835. Fix before "Update".

- **Exceptions.** `site/exceptions.json` still names `control/crates/layerfs-machine/src/platform/native` and `control/crates/layerfs-sandbox/src/backend/native`. They should be `platform/host` and `backend/process`.
- **Row notes.**
  - The `backend/process/instance/*` notes still say "one native Sandbox's lifecycle leaf; one running local Sandbox".
  - The `spawn/*` notes still say "joins running/".
  - The `spawn/mod.rs` note still lists "confined reads".
- **LOC = None** on every renamed row:
  - the 13 `backend/process/**` rows;
  - the 5 `platform/host/*` rows;
  - the 6 #835 machine rows: `lifecycle/fence.rs` 211 and `route/host/{client 687, diagnostic 155, frame 255, hello 152, stream 313}`.

  The correct values are in the listings below.
- **Drifted LOC:** `with_users.rs` is 154 (plan says 138), `lifecycle/create.rs` is 236 (plan says 235) and `lifecycle/mount_intents.rs` is 143 (plan says 147).

### F7. Identifier drift, code only, not layout (optional follow-up)

- **Three words for "runs in this process":** `ExecutorKind::Local` (machine `table/profile.rs`), `Route::InProcess` / `CallSite::InProcess` (`table/support.rs`), and `runs_here` / `in_process` (`route/runs_here.rs`). `Local` should become `InProcess`.
- **Leftover native/local names:**
  - `NativeBackend` (`backend/process/adapter.rs`);
  - `Runtime::native_paths` (`backend/dispatch.rs`);
  - `native_missing` (`setup/probe.rs`, used by `platform/host/`);
  - `Local` / `LocalPaths` / `LocalPhase` / `LocalRecord` (`spawn/`, `runtime/`);
  - `force_remove::local_host`;
  - `deploy.localDir`.
- **Doc headers** in `family.rs`, `lifecycle.rs`, `record/daemon.rs`, `reopen.rs`, `force.rs` and machine `lib.rs` still say `native`.

None of this needs a folder change. List it so the rename pass is complete.

### F8. For the host-worker reviewer (out of my scope, noted only)

Machine `route/host/client.rs` (today's `worker/host.rs`, 687) mixes three roles:

- endpoint custody that the server side uses (`HostPaths::bind`, `bind_writes`, `remove_endpoints`, `try_hold`);
- the client exchange (`call*`, `exchange`, `acquire`);
- spawning the host worker (`for_management().start()`, `spawn`, which `fence.rs` calls).

It is the one file in `route/` that is not a route. A split such as `route/host/{paths.rs, client.rs}`, with worker spawn beside fence, would match the guest side. I leave it to the host-worker reviewer.

## 3. Proposed layout of changed folders

Scope: the plan's new paths over S/v835tree. Production LOC per file comes from S/loc_835.txt. Only changed folders are shown; partial folders are marked.

```diff
  layerfs-sandbox/src/                         [243]  (partial: root loose files only)
    lib.rs                                       105  (74 + 31 merged from sandbox-backend)
    deploy.rs                                    111
    family.rs                                     27
+   users.rs                                       —   new (was planned status/users.rs; MachineUsers + StoreUsers, with F1 hooks)
    status/                                   [1277]
      capacity.rs                                 90
      get.rs                                     203
      inventory.rs                               306
      list.rs                                    165
      overlay.rs                                  46
      run_folder.rs                              115
      state.rs                                   118
      stats.rs                                   227
-     users.rs                                     —   (planned new; moves to root users.rs)
    lifecycle/                                [2490]
      mod.rs                                      17
+     boot.rs                                    448   (renamed from planned local.rs)
-     local.rs                                     —   (renamed to boot.rs; not counted twice)
      connection.rs                              103
      create.rs                                  236
      force_remove.rs                             98
      launch.rs                                  346
+     machine_stop.rs                              —   new (planned; implements MachineUsers::force_stop)
      mount_intents.rs                           143
      network.rs                                  18
      registry.rs                                110
      remove.rs                                  449
      serve.rs                                   431
      startup.rs                                  91
    route/                                    [3231]
+     mod.rs                                       —   new (planned)
      direct.rs                                  267
      inspect.rs                                 163   (72 route + 91 owner, merged as planned)
      daemon/                                 [1536]   (unchanged; contents not shown)
-     host/                                        —   (renamed to machine/; not counted twice)
      machine/                                [1265]   (was planned route/host/: how a Sandbox call reaches its machine)
        mod.rs                                     4
        call.rs                                  208
        dispatch.rs                              109
        frame.rs                                 357
        send.rs                                  587
    spawn/                                     [352]
      mod.rs                                      17   (os alias and mounts_under leave for backend/mod.rs)
      daemon.rs                                  243   (split: macOS process create leaves; keeps Local, LocalPaths, LocalPhase, LOG)
      listener.rs                                 56
      search_path.rs                              13
      streams.rs                                  23
    backend/                                   [989]   (partial: process/ only)
      process/                                 [989]
        mod.rs                                    16
        adapter.rs                                23
        provider.rs                              123
        instance/                              [597]
+         mod.rs                                   —   new (planned)
+         create.rs                                —   new (split from spawn/daemon.rs: Local::create, prepare, LocalCreateFailure)
          exec.rs                                160
          groups.rs                              145
          handle.rs                              292
        macos/                                 [230]
          mod.rs                                   4
          confined.rs                             65
          mount_table.rs                         136
          mounts.rs                               25
```

```diff
  layerfs-machine/src/                         [314]  (partial: root loose files only)
    update.rs                                    115
    users.rs                                      45   (MachineUsers gains reconcile, forget, holders, force_stop)
    with_users.rs                                154   (calls take &dyn MachineUsers; machine.get host-live section leaves for api families/machine/status/get.rs)
    lifecycle/                                [1358]
+     mod.rs                                       —   new (planned)
      boot.rs                                    339
      create.rs                                  265
      fence.rs                                   211   (split: fence, worker start, drain stay; Store/Sandbox holders → sandbox users.rs)
      force_stop.rs                              286   (planned split; Sandbox side via MachineUsers::force_stop)
      progress.rs                                 23
      remove.rs                                  108
      start.rs                                   126
+     stop.rs                                      —   new (planned, split from power.rs)
```

## 4. Checked and left alone

- **Entity skeleton.**
  - Sandbox: `status/`, `lifecycle/`, `record/`, `route/`, `backend/`, plus `exec/`, `image/`, `runtime/`, `spawn/`, `bin/`.
  - Machine: `status/`, `lifecycle/`, `record/`, `route/`, `platform/`, plus `setup/`, `table/`.
  - Both hold the skeleton.
  - Sandbox has 10 root folders. That is long, but the driver trio (`backend/`, `runtime/`, `spawn/`) follows the settled "variants-only `backend/`", so I did not regroup it.
- **Backend variants.** Both have `adapter.rs`, `provider.rs`, `instance/{exec,handle}` and an OS folder with `confined` and `mounts`. Each difference fits in one line:
  - namespaces isolates (`seccomp`, `network/`, `rootfs/`, `cgroup`, helper `spec`, `linux/host_checks`, `linux/runtime`);
  - process uses process groups and macOS's `mount` tool (`mount_table.rs`).

  `instance/` is not a LayerFS concept elsewhere. The only gap was the create (F3).
- **Machine `route/{guest,host}/`.** Right. Both sides have the same file set (`client`, `diagnostic`, `frame`, `hello`; host adds `stream`). "guest" matches `setup/guest.rs` and the `machine/linux-guest/run` run root.
- **`route/runs_here.rs`.** The name is predictable and its callers are route decisions. `provided()` is a platform fact riding along; it is not worth moving.
- **Machine `platform/`.** All four variants, `host/` included, have `adapter`, `lifecycle`, `profile`, `status`, with 4 loose files. Settled.
- **Machine `table/`** (`backend`, `options`, `postgresql_url`, `profile`, `reach`, `removed`, `support`) and **`setup/`** (11 files). The use lines fit: `postgresql_url` is used by `reach.rs` and sandbox `route`/`boot`.
- **Sandbox `record/`** (9 files), **`exec/`** (5), **`image/`** (`cache`, `list`, `prune`, `copy/`, `tar/`), **`runtime/`** (12), **`status/`** (8 after F1) and **`route/daemon/`**. All are within limits and settled.
- **Depth.** The deepest path is `backend/namespaces/instance/*.rs` and `route/daemon/channel/*.rs`, 4 levels. That is OK.
- **#835 legacy host-record refusals** live in `layerfs-home` `home/legacy_host.rs`, outside these crates.

## 5. Machine-readable list

```json
[
  {"from": "control/crates/layerfs-machine/src/lifecycle/fence.rs", "to": "control/crates/layerfs-machine/src/lifecycle/fence.rs", "op": "split", "why": "record fence, host worker start and drain stay; the Store-lease/Sandbox holder report uses layerfs-store and inventory, so it becomes MachineUsers::holders in control/crates/layerfs-sandbox/src/users.rs"},
  {"from": "control/crates/layerfs-machine/src/with_users.rs", "to": "control/crates/layerfs-machine/src/with_users.rs", "op": "split", "why": "every inventory/layerfs_store/layerfs_sandbox reach becomes a MachineUsers hook; the host-worker live section of machine.get moves to api/crates/layerfs-api/src/families/machine/status/get.rs (needs layerfs-inspect)"},
  {"from": "control/crates/layerfs-sandbox/src/status/users.rs", "to": "control/crates/layerfs-sandbox/src/users.rs", "op": "move", "why": "implements MachineUsers/StoreUsers incl. action hooks (reconcile, forget, holders, force stop), so not a status read; mirrors machine root users.rs"},
  {"from": "control/crates/layerfs-sandbox/src/route/host", "to": "control/crates/layerfs-sandbox/src/route/machine", "op": "rename", "why": "#835 made 'host' the host machine (machine route/host/ is the host-worker client); this folder is how a Sandbox call reaches whichever machine runs it"},
  {"from": "control/crates/layerfs-sandbox/src/spawn/daemon.rs", "to": "control/crates/layerfs-sandbox/src/spawn/daemon.rs", "op": "split", "why": "the macOS-only process daemon create is a variant member; shared Local/LocalPaths/LocalPhase/LOG stay"},
  {"from": "control/crates/layerfs-sandbox/src/spawn/daemon.rs", "to": "control/crates/layerfs-sandbox/src/backend/process/instance/create.rs", "op": "new", "why": "the process backend's daemon start, the twin of backend/namespaces/instance/create.rs"},
  {"from": "control/crates/layerfs-sandbox/src/lifecycle/local.rs", "to": "control/crates/layerfs-sandbox/src/lifecycle/boot.rs", "op": "rename", "why": "'local' distinguishes nothing since both backends run locally and #835 retired it; boots the daemon in its run folder, same word as machine lifecycle/boot.rs"}
]
```
