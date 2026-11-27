# `layerfs-machine` organization review (#828 planned layout)

Scope: the planned `control/crates/layerfs-machine/src/` (rows of
`nav/site/layout.json` whose `n` is under it), checked against the old sources
in `v2tree/control/crates/{layerfs-machine,layerfs-platform,layerfs-sandbox-backend}`:
`//!` headers, `crate::`/`super::`/`layerfs_*::` lines, the two manifests, and
callers of `layerfs_machine::` across the archive. LOC are production LOC from
`prod_loc_files.txt` (the plan's values, `layerfs_v2_0_0` at `1317ebf7ca`).
Planning only; nothing in the repository was touched.

## 1. Verdict

The plan is mostly sound. `lifecycle/`, `status/` and `route/` hold what their
names say. The four `platform/` variants really share one file set, and the
extra files in docker and lima each have a one-line reason. There is one real
defect: `table/backend.rs` cannot be a rename, because the file calls into
`layerfs-sandbox`. There is one misplaced file: `setup/machine_spec.rs` is
record I/O and not setup, and moving it gives machine the `record/` folder that
Store and Sandbox already have. The open `probe.rs` question has a
`use`-line answer: `setup/`. The rest are small.

## 2. Findings (ranked)

### F1. `table/backend.rs` is not a move: its code calls into `layerfs-sandbox` (rules 22, 24, 23)

Evidence:

- Old `layerfs-sandbox-backend/src/backend/kind.rs` lines 3–4:
  `use super::adapter::adapter;` and `use crate::ProviderCapabilities;`. Its
  methods `stored_name`, `description` and `capabilities` all call
  `adapter(self)`.
- The plan keeps both targets in Sandbox: `backend/adapter.rs` →
  `layerfs-sandbox/src/backend/adapter.rs` and `runtime/types/capabilities.rs`
  → `layerfs-sandbox/src/runtime/capabilities.rs`. Machine sits below Sandbox
  (home < machine < store < sandbox), so the planned
  `layerfs-machine/src/table/backend.rs` (row `c: rename`, 22 LOC) cannot
  `use` either one.
- Machine does need the enum. `table/platform.rs` has
  `backend: fn(HostOs) -> Option<Backend>`, `table/support.rs` calls
  `backend.description()`, and both `platform/available.rs` (`b.stored_name()`)
  and `setup/machine_spec.rs` (`Backend::stored_name`) use it. The Sandbox side
  uses `adapter(self.backend()).capabilities()` (old `runtime/dispatch.rs:47`).
  A third `Backend` enum `{HostProcess, Namespaces}` sits in Sandbox
  `runtime.rs`, which becomes `backend/select.rs`.

**Fix:** turn the row into a split. Machine `table/backend.rs` keeps the
`Backend` enum with `stored_name`, `description` and `parse` as a plain match:
the names and descriptions are table data, and machine is the lowest crate that
needs them. `capabilities()` stays in Sandbox `backend/adapter.rs`, and
`BackendAdapter` drops `stored_name` and `description`, so the backend names
have one owner (rule 23). Record it as a code change with its own LOC delta,
not a rename. Merging the third enum in `backend/select.rs` is the code
follow-up the Sandbox review already raised.

### F2. `setup/machine_spec.rs` is record I/O: move it into a new `record/` (rules 11, 20, 9). This revises one line of the accepted review-2 F2 (`prepare/spec.rs` → `setup/`).

Evidence:

- What it does (old `layerfs-machine/src/spec.rs`): `machine_of`, `update`,
  `save_running` and `save_unready` (these write `record.json` and
  `worker.json` through `write_record`), `ensure_built_ins`,
  `forget_built_ins` and `write_built_ins` (the built-in `native` and `docker`
  records), `worker` (reads `worker.json`), `docker_socket` (from the docker
  record), `spec` and `provider_spec` (record → `Spec`), and `refusal`.
- Its callers already treat it as the record module. Every one imports it
  under that name: `use super::spec::{self as record, …}` in `create.rs`,
  `endpoint.rs`, `image.rs`, `live.rs`, `power.rs` and `remove.rs`, and
  `use super::spec as record;` in `platforms.rs`.
- The other setup files are VM setup steps and the host-command toolkit:
  `tool`, `guest`, `vm`, `facts`, `forward`, `worker` and `artifacts`. They come
  from `layerfs-platform`, whose manifest has no `layerfs-home` dependency
  ("Knows nothing of the LayerFS home"). `machine_spec.rs` is the only planned
  setup file that writes records (`use layerfs_home::home::{self, paths, write_record}`).
  Its own header says "Nothing in the adapters reads the home."
- Rule 20 gives an entity's record and identity a `record/` folder. Store has
  `admin/record/{identity,intent,store}.rs` and Sandbox has
  `record/{sandbox,…}.rs`. Machine keeps `record.rs` loose at the root.

**Fix:** add `record/` with:

- `machine.rs`, from root `record.rs`: `MachineRecord`, `vm_name`, the op
  names. Same pattern as Store `record/store.rs` and Sandbox
  `record/sandbox.rs`.
- `machine_spec.rs`, from `setup/`.
- `built_ins.rs`, new. `ensure_built_ins`, `forget_built_ins`,
  `write_built_ins` and `ENSURED` are split out of `machine_spec.rs`. They are
  the record-level existence of the two built-ins and sit beside
  `MachineRecord::built_in`. Callers: api `calls/dispatch.rs`, control
  `home.rs`, `remove.rs`.

Result: the root drops to 3 loose files (`update.rs`, `users.rs`,
`with_users.rs`), and `setup/` drops to 11 files (with F3). Cost:
`record.rs` is built without the `machine` feature (guest worker build) but
`machine_spec.rs` is not, so `record/mod.rs` needs a per-file cfg. That is the
same cfg the root needs today.

If the owner does not want a function-level split: use
`record/{machine.rs, machine_spec.rs, default_image.rs}`. `default_image.rs`
is mostly bookkeeping on the docker record (`loaded_image` read, compare and
write under the home lock) around one `cached_image` call. This option is
weaker, because loading the image is also setup work, and its only caller is
`lifecycle/boot.rs`.

### F3. `probe.rs` (still open): recommend `setup/`, beside `tool.rs` and `facts.rs` (rules 11, 23)

Evidence:

- It is the only planned `table/` file that runs commands or dispatches to
  adapters: `use crate::adapter::adapter;` and `use crate::setup::tool;`.
  `probe()` is `adapter(kind).probe(tools, …)`. The other `table/` files are
  data (`Platform`/`Profile`, options, removed names, SUPPORT.json) plus the
  G3 reach rule and its URL parser.
- Its helpers are the host-command toolkit that the variants use just like
  `tool.rs`:
  - `first_line`: apple_container and lima `observe.rs`, docker `cli.rs`,
    docker `adapter.rs`.
  - `tool_version`, `macos_missing`: apple_container and lima adapters.
  - `native_missing`: the native adapter, native `observe.rs` and the inspect
    doctor `host_install.rs`.
- `tool.rs`'s header already claims this job: "Host commands … and the host
  facts setup needs".

**Fix:** `table/probe.rs` → `setup/probe.rs`. `table/` keeps 7 files, all
table data or a rule that the table encodes. `setup/` has 11 files with F2.

### F4. `platform/docker/windows.rs` is misnamed, and the plan's note on it is wrong (rule 11)

Evidence: old `docker/mod.rs` compiles it under
`cfg(not(any(target_os = "linux", target_os = "macos")))`, inside a crate that
is `#![cfg(unix)]` (old `layerfs-platform/src/lib.rs`). It never builds on
Windows; it builds only on other Unix hosts. It stands in for three files,
not one: it exports `cached_image`, `load_image`, `start`, `stop`, `guest`,
`provider_state` and `status`, the twins of `images.rs`, `lifecycle.rs` and
`status.rs`. The plan's row says "windows.rs stays its cfg twin" of `status.rs`.

**Fix:** rename it `docker/unsupported.rs` ("every call answers that this host
is unsupported"), and correct the row note. Its `//!` WSL2 sentence is a code
or doc follow-up.

### F5. (low) `platform/mod.rs` must re-export `setup/` and `table/` items under `platform::`

The plan keeps `layerfs_machine::platform::{Spec, reach, sandbox_allowed, forward, …}`
("path kept"). Sandbox `direct.rs`, `route.rs` and `create.rs`, Store, and
inspect `machine_target.rs` use these paths. A reader who follows
`platform::Spec` then finds it in `setup/spec.rs`, and `platform::sandbox_allowed`
in `table/support.rs`. Either accept the facade and say so in the
`platform/mod.rs` header, or change the callers to `setup::`/`table::` paths in
the same commit. I recommend changing the callers: the folders then predict
where each path is defined (rule 11). This is a cost decision, not a defect.

## 3. Proposed layout (changed folders only)

Scope: planned `control/crates/layerfs-machine/src/`, LOC from
`tools/production_loc.py --files` at `1317ebf7ca` (as in `prod_loc_files.txt`).
The crate root, `setup/` and `table/` are shown in part. Bracketed subtotals
include removed files, as the style rule requires; the annotation gives the
total after the move.

```diff
  layerfs-machine/src/                         [3,487]   (partial: root, record/, setup/, table/; moved files counted at both ends)
    lib.rs                                          64
    update.rs                                      115
    users.rs                                        45
    with_users.rs                                  138
-   record.rs                                      187   (to record/machine.rs, F2)
+   record/                                      [380]   (new: record and identity, rule 20)
+     built_ins.rs                                   —   new (split from machine_spec.rs: ensure/forget/write built-ins, F2)
+     machine.rs                                   187   (was record.rs; like store record/store.rs)
+     machine_spec.rs                              193   (from setup/; record ↔ Spec, record/worker.json writes)
+     mod.rs                                         —   new
    setup/                                     [1,856]   (1,663 after; 11 files)
      artifacts.rs                                 152
      bundle.rs                                    201
      default_image.rs                              65
      facts.rs                                      68
      forward.rs                                    80
      guest.rs                                     208
      mod.rs                                        14
+     probe.rs                                      71   (from table/; host-command helpers + adapter probe, F3)
      spec.rs                                      169
      tool.rs                                      339
      vm.rs                                         62
      worker.rs                                    234
-     machine_spec.rs                              193   (to record/, F2)
    table/                                       [702]   (631 after; 7 files)
      backend.rs                                    22   (split, not rename: names only; capabilities stay in sandbox, F1)
      mod.rs                                         —
      options.rs                                    28
      platform.rs                                  140
      postgres_url.rs                              148
      reach.rs                                      78
      removed.rs                                    11
      support.rs                                   204
-     probe.rs                                      71   (to setup/, F3)
```

```diff
  layerfs-machine/src/platform/docker/          [727]   (partial)
+   unsupported.rs                                  39   (was windows.rs: other-Unix twin of images/lifecycle/status, F4)
-   windows.rs                                      39   (to unsupported.rs)
```

## 4. Left alone, and why

- **`lifecycle/`, `status/`, `route/`**: settled, and the `use` lines agree.
  `boot.rs` (old `endpoint.rs`) holds the start lock that create and remove take.
  `live.rs` probes this machine's own provider and worker, so it is not the
  Sandbox `live.rs` that moves to inspect. `runs_here.rs` decides between
  in-process and the worker, which is route work. Its `provided()` is a
  built-in fact that `record.rs` and `state.rs` call; it would also fit in
  `record/built_ins.rs`, but moving it is not worth reopening `route/`.
- **Variant file sets**: all four have `adapter`, `lifecycle`, `profile` and
  `status`. Extras with a one-line reason each:
  - docker `cli.rs`: Engine CLI on its socket.
  - docker `images.rs`: content-addressed image cache.
  - docker `socket.rs`: Engine socket choice. Its callers are `setup/spec.rs`,
    `record/machine_spec.rs`, `available.rs` and Sandbox `docker.rs`, so it is
    docker's own fact.
  - docker `unsupported.rs`: see F4.
  - lima `limit.rs`: Lima's instance-name limit.
  - lima `template.rs`: the `limactl create` template.
- **`platform/` loose files**: `adapter.rs`, `dispatch.rs` and `error.rs` are
  the group's contract, the same shape as Sandbox
  `backend/{adapter,dispatch}.rs`. `available.rs` is settled. It is the only
  `platform/` file that reads the home (`use super::spec as record`, for
  `docker_socket`), and by rule 20 it is a status read. I found no defect
  beyond that, so it stays.
- **`table/` name, and `reach.rs` + `postgres_url.rs` in it**: the old crate
  names "the platform table" (`platform.rs`) and "the one support table"
  (`support.rs`). `reach.rs` is the G3 rule from the same §7.3. The upward
  users that the move creates are allowed: Sandbox `route.rs` and
  `launch/local.rs` use `url_hosts`; Sandbox `runtime.rs` and `state.rs`, api
  `platform_policy.rs` and inspect `inventory.rs` use `removed_reason`.
- **`table/platform.rs` vs the variants' `profile.rs`**: each `profile.rs`
  fills one row of `table/platform.rs` (`Profile`), while each `adapter.rs`
  implements `platform/adapter.rs`. That split matches the source's own words
  ("table" for data, "adapter" for behaviour), so it is not a rule-23 defect.
- **`setup/bundle.rs`, `setup/default_image.rs`**: both are the sealed bundle
  and what it supplies (`artifacts.rs` installs it in the guest), so they are
  setup inputs. `bundle.rs` reads the per-build cache, which is outside every
  home.
- **Other `layerfs-sandbox-backend` dependencies of the old platform crate**:
  `json` and `RuntimeError` in `facts.rs`, `support.rs` and `template.rs` are
  covered by the plan's move of `json/` to `layerfs-home` ("errors become home
  `JsonError`"). F1 is the only one left unresolved.
