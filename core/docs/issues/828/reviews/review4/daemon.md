# Review 4: engine `layerfs-daemon`

Scope: `engine/crates/layerfs-daemon/` planned layout (`S/nav/site/layout.json` rows),
judged against the old source at `S/v2tree/engine/crates/layerfs-daemon/` (branch
`layerfs_v2_0_0` at 1317ebf7ca). LOC is production LOC from `S/prod_loc_files.txt`.
Settled and not reopened: `control/` → `workspace/`; `teardown/` (not `unmount/`); the
`bin/` one-file exception.

## 1. Verdict

`layerfs-daemon`: the plan is mostly sound. The top-level split works: `application/`
(process, connections, observed calls), `install/`, `operator/` (the backend method
table) and `workspace/` (the Workspace service). Depth, loose-file counts and leaf sizes
pass. Four things are worth changing:

- `workspace/kernel/` sits beside `workspace/nfs/` but holds only FUSE code plus the
  route-neutral `Kernel` enum.
- The plan moves `sandbox.list_workspaces` out of `workspace/` into `operator/`. That
  forces registry internals to widen, and it splits that call from its twin
  `sandbox.doctor_workspaces`.
- `operator/workspace.rs` collides with the `workspace/` folder next to it, and it holds
  three calls.
- `application/startup/lifecycle.rs` mixes the connection-slot state machine into
  startup.

The other findings are smaller naming fixes.

## 2. Findings (ranked)

### F1. `workspace/kernel/` → `workspace/fuse/`; `kernel/state.rs` → `workspace/kernel.rs`

**What.** `kernel/` and `nfs/` are siblings, but they are not two kinds of one thing.
NFS is also a kernel route: bridge `KernelRoute { Fuse, Nfs }`, and the `Kernel` enum
has NFS variants. `kernel/` actually holds:

- the route-neutral `Kernel` enum (`state.rs`);
- the FUSE route: `attach.rs` and `dispatcher.rs` (`KernelServing`, the FUSE session
  owners);
- `invalidate.rs`, which is FUSE notify with an early return for NFS.

**Evidence.**

- `control/kernel.rs` (→ `kernel/state.rs`) has the header "Kernel half of one registry
  entry". Its enum has the variants `Exported(Box<super::export::Exported>)`,
  `ExportLeaving(..)` and `ExportKept(..)`, and `block()` calls `super::export::block`.
  So `nfs/` is part of what `kernel/` claims to own, yet it sits outside it.
- `control/serving.rs` (→ `kernel/dispatcher.rs`) uses
  `layerfs_fuse::{session::{KernelSession, SessionObserver, ..}, MountRequests}` and has
  the header "One shared dispatcher for every mount". These are FUSE mounts only; NFS has
  `ExportServing` in `nfs/export.rs`.
- `control/attach.rs` uses `layerfs_fuse::{attributes::Identity, session::{AttachRemainder,
  KernelSession, SessionConfig}}`. Its NFS branch is `return self.attach_export(..)`.
- In the engine, "kernel" already names kernel-facing state inside each route: nfs
  `kernel/` (references, released) and vfs `operations/kernel/`. A daemon `kernel/` beside
  `nfs/` reads as if NFS were not a kernel route.

**Fix.**

- `workspace/kernel/{attach,dispatcher,invalidate}.rs` → `workspace/fuse/`. Its sibling
  `nfs/` is then the other variant of `KernelRoute`, and the two names match the engine
  crates `layerfs-fuse` and `layerfs-nfs`. This follows rule 9 (one group, two variants).
- `workspace/kernel/state.rs` → `workspace/kernel.rs`. It holds the neutral `Kernel` enum
  and `bounded()`, and sits beside `registry.rs`, whose `Binding.kernel: Kernel` is its
  only holder. The file gets back its original name.
- `workspace/` root then has 4 loose files (failure, kernel, receipts, registry), which is
  at the limit.

`fuse/` does not repeat the daemon's own crate name. It names the route, as `nfs/` does.

### F2. Put `sandbox.list_workspaces` back in `workspace/status/` as `list.rs`, not `operator/list_workspaces.rs`

**What.** The plan moves `control/inspect.rs` to `operator/list_workspaces.rs`. Its twin
`control/diagnostic.rs` goes to `workspace/status/doctor.rs`. Both are `impl Service`
reads of the private registry.

**Evidence.**

- `control/inspect.rs` reads `use super::{registry::Entry, Service};`. Its body uses
  `self.entries.lock()` and `self.job(route, ..)`.
- In `registry.rs`, `pub(super) enum Entry` and
  `pub(super) entries: Mutex<BTreeMap<..>>` are module-private, and so is
  `operations.rs` `pub(super) fn job`. The plan's move compiles only if all three widen
  to `pub(crate)`.
- `control/diagnostic.rs` uses `super::{inspect::activity, registry::Entry, Service}`.
  With the plan, `status/doctor.rs` must reach into `operator/list_workspaces.rs` for
  `activity()`, which is a back-edge from `workspace/` into `operator/`.
- The existing pattern is that `operator/` holds SDK-value adapters that call Service
  reads in `workspace/`. For example, `operator/workspace.rs::changed` calls
  `service.changed()` (`control/changed.rs`), and `operator/dispatch.rs` calls
  `service.inspect(call)` and `service.diagnostic_workspaces(call)`.

**Fix.** Move `operator/list_workspaces.rs` → `workspace/status/list.rs`. The skeleton
says `status/` holds get, list and state words, and Sandbox, machine and Store
`admin/status/` each have a `list.rs`. The plan's reason ("inspect is the layerfs-inspect
crate") is still met. `activity()` and `doctor.rs` then share one folder, and the registry
stays private. The store-service half of the same call stays at store-service
`operator/list_workspaces.rs`, as planned.

### F3. Split `operator/workspace.rs` by call: `get_storage.rs`, `list_changes.rs`, `list_changed.rs`

**What.** After F2, `operator/` would hold only `dispatch.rs` and `workspace.rs`. The
file `workspace.rs` collides with the `workspace/` folder beside it (rule 13), and it
holds three calls (rule 15).

**Evidence.** `operator/dispatch.rs` routes three calls:

- `backend.daemon.workspace.get_storage` to `super::workspace::overlay`;
- `backend.daemon.workspace.list_changes` to `super::workspace::changes`;
- `backend.daemon.workspace.list_changed` to `super::workspace::changed`.

`application/internals.rs` re-exports `crate::operator::workspace::changes_of`.

**Fix.**

- `operator/list_changes.rs` gets `changes`, `changes_of` and the walk (`Frame`, `Live`,
  `VisibleBase`, `with_source`, `marker`, `scan`). This is the bulk of the file.
- `operator/list_changed.rs` gets `changed`.
- `operator/get_storage.rs` gets `overlay`.
- The 6-line `selected()` helper goes to `dispatch.rs`, and the internals path is updated.

`operator/` then holds 4 files, each named after its call, as in store-service
`methods/file/`.

### F4. Move the connection-slot state machine out of `startup/lifecycle.rs` into `connection/slots.rs`

**What.** `application/owner.rs` → `startup/lifecycle.rs` (646) holds two concerns: the
admission slots and the `Application` startup.

**Evidence.**

- The header reads: "One startup lifecycle owner, Store transfer and bounded connection
  custody."
- Lines 28–430 hold the slot types and functions: `RESERVED_SLOTS`, `SlotClass`,
  `AdmissionClass`, `Phase`, `SlotState`, `SlotTicket`, `Finished`, `SlotCounts`, `Slots`,
  `Disposition`, `recovery_request` and `disposition`.
- Their users are in connection code:
  - `connection.rs` uses
    `owner::{disposition, recovery_request, AdmissionClass, Finished, SlotClass,
    SlotCounts, SlotTicket, ..}`;
  - `retained.rs` uses `super::owner::SlotTicket` and `SlotClass`;
  - `internals.rs` re-exports them for `tests/slot_state.rs` and `daemon_slots.rs`.
- The planned `connection/` already holds `workers.rs` and `retained.rs`, the other halves
  of slot custody.

**Fix.** Split `startup/lifecycle.rs`. The slot types move to `connection/slots.rs`. The
`Application`, `Startup`, `State`, `start`, Store selection and `Drop` stay in
`lifecycle.rs`. `connection/` then holds 4 files and `startup/` holds 5.

### F5. Rename `application/startup/serve.rs` → `install.rs` and `startup/filesystem.rs` → `serving.rs`

**Evidence.**

- `serve.rs`: its header is "Exact one-time installation admission and original
  Store-to-Service transfer", and its one entry is `Application::install`, which calls
  `crate::install::receive_install_manifest`. The plan's own note says "install admission
  and Store transfer". "serve" predicts the request loop, which actually lives in
  `connection/workers.rs` and `workspace/route/serve.rs`.
- `filesystem.rs`: its header is "Kernel serving assembly over the daemon's one Store and
  one overlay owner". It holds `assemble` (`KernelServing`), `assemble_export`
  (`ExportServing`), `service`, `check_route` and the `LAYERFS_NFS_*` names. "filesystem"
  fits everything in a filesystem daemon (rule 12). `serving.rs` matches both types it
  builds.

**Fix.** Make both renames. Once `serve.rs` is gone, `serving.rs` has nothing to be
confused with.

### F6. Rename `application/diagnostics/` → `application/observed/`, and `counters.rs` → `records.rs`

**What.** In this folder, "diagnostics" means the numeric observation records of an
`Observed` call. Elsewhere `diagnostic` means integrity checks: persistence
`backend/*/diagnostic/`, store `backend/*/diagnostic.rs`, and the dissolved overlay
`diagnostics/`. That is the same word for a different idea (rules 13 and 19).

**Evidence.**

- `diagnostics.rs` (→ `counters.rs`) has the header "Explicit per-control numeric
  observations". It defines `Records<'a, W>`, `type Observed`, and
  `Application::observed_control`, which unwraps `Request::Observed { scope, traced, .. }`.
- `resources.rs` holds `ResourceObservation`, and `kernel.rs` holds "kernel observer
  facts".
- Other code calls these "observation records": bridge `tests/observed_records.rs`, and
  `tools/decode_observations.py`, whose docstring reads "the daemon's numeric observation
  records".

**Fix.** Rename `diagnostics/` → `observed/`, and `counters.rs` → `records.rs` (it holds
`Records`). `kernel.rs`, `resources.rs` and `schema.rs` keep their names. `observed/` is
named after the request kind it serves. It does not collide with `workspace/status/observation.rs`,
which holds the Service-side observations that `observed/` calls.

### F7. Move `workspace/teardown/dirty.rs` → `workspace/status/dirty.rs`

**Evidence.**

- `dirty.rs` is a bounded read: "Uncommitted changes that refuse an Unmount without
  `discard`". `fn dirty(..) -> Result<Option<DirtyReport>, Failure>` changes nothing.
- `control/changed.rs` (→ `status/changed.rs`) has `use super::dirty::{join, Names,
  WALK};`, so with the plan `status/` depends on helpers in `teardown/`. Its header says
  it reads "the same indexed windows the Dirty check and a capture read".
- The only other caller is `unmount.rs` → `teardown/normal.rs` (`self.dirty(token)?`). A
  terminal step calling a read is the natural direction.

**Fix.** Move the file. `status/` then has 7 files and `teardown/` has 4 (`normal`,
`forced`, `kernel_detach`, `detach_retry`).

### F8. Rename `workspace/route/operations.rs` → `route/dispatch.rs`

**Evidence.** The file's job is `execute`, `execute_control` and the `fn control` match
from each `Request` variant to a Service method (`Request::Mount*`, `Status`, `Cleanup`,
`Commit`, `Unmount`, `ForceUnmount`, `Attach`, `Locate`, and `Sdk` to
`crate::operator::dispatch`).

The engine already uses "operations" for filesystem operation sets: vfs
`operations/{file,kernel,namespace}/` and mount-ports `operations/`. `dispatch.rs` is the
word used for the same role in daemon `operator/dispatch.rs`, store-service
`methods/dispatch.rs` and `operator/dispatch.rs`, control workspace `route/dispatch.rs`
and sandbox `route/host/dispatch.rs`.

**Fix.** Rename only. The file also holds the bodies of the mount bind and Commit, plus
the `settle`/`job` helpers; see §4 for why it is not split.

### F9. Tests: dissolve area `tests/control/`; small renames

**What.** The plan's area `tests/control/` brings back the word that `src/control/` was
renamed away from, and it mixes application and Workspace topics. The plan's `root.rs`
dropped "control" as if it were a campaign prefix. It is not: the test covers fuse
`mount::control_root`.

**Evidence.** From the test headers:

- `daemon_slots` and `slot_state` cover "the real daemon's control-slot custody" (the
  `application` slots);
- `telemetry_recording` covers the Recording guard;
- `doctor_registry`, `mount_holders`, `named_mount`, `operator_workspace_changes` and
  `workspace_databases` cover Workspace calls;
- `control_root` covers "The control tree a daemon binds abort controls from
  (`mount::control_root`)".

**Fix.**

- New area `tests/application/`: `daemon_slots`, `slot_state`, `telemetry_recording`
  (3 topics).
- New area `tests/workspace/`: `control_root.rs` (original name kept), `doctor_registry`,
  `mount_holders`, `named_mount`, `operator_workspace_changes`, and
  `workspace_databases.rs` → `databases.rs` (6 topics).
- `support/gate/history_gate.rs` → `gate/history.rs` and `support/gate/store_gate.rs` →
  `gate/store.rs` (rule 14).
- `mounted_commit/orphan_commit.rs` → `orphan.rs` and `mounted_commit/parallel_commits.rs`
  → `parallel.rs` (rule 16).

### F10 (low). Examples: `store_accounting/startup_records.rs` → `store_accounting/engine_records.rs`

**Evidence.** `examples/store_accounting.rs` already declares this module as
`#[path = "e2_startup/records.rs"] mod engine_records;`. The neighbouring
`store_accounting/records.rs` has the header "... kept separate from engine families".

**Fix.** Name the file after its module.

## 3. Proposed layout of changed folders

Scope: `engine/crates/layerfs-daemon/src/` (production) and `tests/` and `examples/`
(test/example code, no LOC). The source is archive 1317ebf7ca and the counter is
`tools/production_loc.py --files` (`S/prod_loc_files.txt`). Split files show their
pre-split LOC.

```diff
  workspace/                          [4,495]  (planned 4,424 + list.rs moved in)
    mod.rs                               35
    failure.rs                          260
    kernel.rs                            85   (from kernel/state.rs: Kernel enum of both routes)
    receipts.rs                          67
    registry.rs                         198
-   kernel/                             [542]  (dissolved: FUSE files → fuse/, state.rs → kernel.rs)
+   fuse/                               [457]  (FUSE route; sibling of nfs/)
+     mod.rs                              —   new
      attach.rs                         236   (from kernel/; neutral entry delegates NFS to nfs/export.rs)
      dispatcher.rs                     172   (from kernel/; KernelServing, FUSE session owners)
      invalidate.rs                      49   (from kernel/)
    nfs/                              [1,104]  (unchanged)
      mod.rs                              —
      custody.rs                         70
      export.rs                         821
      helper.rs                         213
    route/                              [622]
      mod.rs                              —
      dispatch.rs                       410   (renamed from operations.rs)
      resolve.rs                         64
      serve.rs                          148
    status/                             [724]
      mod.rs                              —
      changed.rs                        174
      cleanup.rs                         18
      dirty.rs                          181   (from teardown/: a read; changed.rs uses its walk)
      doctor.rs                          88
      get.rs                             85
      list.rs                            71   (from operator/list_workspaces.rs; sandbox.list_workspaces)
      observation.rs                    107
    teardown/                           [943]
      mod.rs                              —
      detach_retry.rs                   100
      forced.rs                         423
      kernel_detach.rs                  205
      normal.rs                         215
```

```diff
  operator/                           [480]
    mod.rs                                4
    dispatch.rs                          27   (gains selected())
+   get_storage.rs                        —   new  (backend.daemon.workspace.get_storage)
+   list_changed.rs                       —   new  (backend.daemon.workspace.list_changed)
+   list_changes.rs                       —   new  (backend.daemon.workspace.list_changes + walk)
-   list_workspaces.rs                   71   (moved to workspace/status/list.rs)
-   workspace.rs                        378   (split into the three call files)
```

```diff
  application/                        [3,510]
    mod.rs                               18
    failure.rs                           95
    internals.rs                          9   (re-export paths updated)
    connection/                       [1,029]
      mod.rs                              —
      bodies.rs                          50
      retained.rs                       243
+     slots.rs                            —   new  (Slots, SlotClass, SlotTicket, disposition… from startup/lifecycle.rs)
      workers.rs                        736
-   diagnostics/                      [1,185]  (renamed observed/)
+   observed/                         [1,185]  (Request::Observed records)
+     mod.rs                              —   new
      kernel.rs                         117
      records.rs                        373   (renamed from diagnostics/counters.rs)
      resources.rs                      219
      schema.rs                         476
    startup/                          [1,174]
      mod.rs                              —
      cli.rs                             44
      config.rs                         157
      install.rs                        144   (renamed from serve.rs: Application::install)
      lifecycle.rs                      646   (split: slot state machine → connection/slots.rs)
      serving.rs                        183   (renamed from filesystem.rs: kernel/export serving assembly)
```

```diff
  tests/                                       (partial: changed areas only)
-   control/                                   (dissolved into application/ and workspace/)
+   application/
+     main.rs                             —   new
      daemon_slots.rs                         (from control/)
      slot_state.rs                           (from control/)
      telemetry_recording.rs                  (from control/)
+   workspace/
+     main.rs                             —   new  (replaces control/main.rs)
      control_root.rs                         (from control/root.rs; names fuse mount::control_root)
      databases.rs                            (from control/workspace_databases.rs)
      doctor_registry.rs                      (from control/)
      mount_holders.rs                        (from control/)
      named_mount.rs                          (from control/)
      operator_workspace_changes.rs           (from control/)
    mounted_commit/                            (partial)
      orphan.rs                               (renamed from orphan_commit.rs)
      parallel.rs                             (renamed from parallel_commits.rs)
    support/                                   (partial)
      gate/
        history.rs                            (renamed from history_gate.rs)
        store.rs                              (renamed from store_gate.rs)
  examples/
    store_accounting/                          (partial)
      engine_records.rs                       (renamed from planned startup_records.rs)
```

## 4. Checked and left alone

- **Crate root** (`lib.rs`, `ports.rs`, `telemetry.rs`, plus `application/`, `bin/`,
  `install/`, `operator/`, `workspace/`): fine.
  - `ports.rs` (the mount-ports adapter) is used only by `fuse/attach.rs` and
    `nfs/export.rs` (`crate::service::Mounted`). It would fit in `workspace/`, but that
    root is at 4 loose files, and the file is public API (`Mounted`, `mount_request`),
    which `layerfs-nfs` tests also use. Left alone.
  - `telemetry.rs` is used crate-wide.
- **`install/`** (`receive`, `file`, `outcome`): fine. `outcome.rs` also holds
  `StoreSettings`, but no better non-catch-all name exists.
- **Does `workspace/nfs/` duplicate a word the nfs crate owns?** No. `layerfs-nfs` owns
  `server/` (one export on a loopback listener), `protocol/`, `procedures/`, `kernel/`
  and `apple_double/`. The daemon's `nfs/` is the host side of the route: `Mounter` and
  `Helper` (`mount_nfs`), `ExportServing`, and export custody. It names the route, like
  F1's `fuse/` and like bridge `KernelRoute::Nfs`. It re-implements nothing.
  `export.rs` shares the word "export" with the crate's `ExportConfig` because it is the
  same concept.
- **`route/`**: it follows the control skeleton ("how a call reaches it") and is the
  server end of sandbox `route/daemon/` (`channel/`, `workspace/{commit,mount,status,unmount}`).
  The word overlaps the vfs `binding/route.rs` `Route` type, but the skeleton meaning is
  settled, so left alone.
  - `route/resolve.rs` is Commit custody, not routing. I considered a
    `workspace/commit/` folder (`run.rs` split from `dispatch.rs`, `resolve.rs`,
    `invalidate.rs`). That needs a three-way split of `operations.rs`, leaves `route/` at
    2 files unless the mount bind is also split out, and pulls FUSE invalidation away from
    `fuse/`. The churn outweighs the gain for now.
- **`receipts.rs` and `failure.rs`**: I considered merging them, since `Failure` is
  defined in `receipts.rs` while its `impl` is in `failure.rs`. A merged file would hold
  `Success` under a failure name, so I left them.
- **`teardown/` names** (`normal`, `forced`, `kernel_detach`, `detach_retry`): match
  their headers. `kernel_detach` and `detach_retry` are FUSE-only (`layerfs_fuse::session`),
  but moving them to `fuse/` would leave `teardown/` at 2 files.
- **`status/` names**: `get`, `cleanup` (`Request::Cleanup`) and `observation` (it
  serves `observed/` and `operator/`) are fine. `doctor.rs` is kept as planned.
- **Cross-crate consistency**: `operator/` matches store-service `operator/` and sandbox
  `route/daemon/channel/operator.rs`. `status/` and `route/` match the control skeleton.
  `failure.rs` matches fuse `request/` and sandbox `runtime/`.
- **`bin/layerfs-daemon.rs`**: granted exception.
- **`application/internals.rs`** (the test re-export seam): fine.
- **Tests left alone**:
  - `tests/unmount/` names the call under test, not a source folder.
  - `tests/kernel/` (12 topics) covers kernel-mount behaviour; I kept it rather than
    renaming it to `fuse/`.
  - `tests/standalone/` entries are fine.
  - `support/` (`complete/`, `install/`, `kernel/`, `rigs/`, `trace/`, 4 loose files) is
    fine. `support/install/control_fixture.rs` keeps "control" (it is the fixture for
    control proofs); a low-value rename.
  - Note: `examples/resource_growth.rs` and `store_accounting.rs` load
    `../tests/support/{complete_bytes,complete_fixture,host_install}.rs` by `#[path]`.
    These paths must follow the planned support moves (`complete/bytes.rs`,
    `complete/fixture.rs`, `install/host.rs`).
- **Depth**: everything is at most `src/a/b/file.rs`.
- **Leaf counts after the proposal**: `fuse` 3, `nfs` 3, `route` 3, `status` 7,
  `teardown` 4, `operator` 4, `connection` 4, `observed` 4, `startup` 5.

## 5. Machine-readable

```json
[
  {"from": "engine/crates/layerfs-daemon/src/workspace/kernel/state.rs", "to": "engine/crates/layerfs-daemon/src/workspace/kernel.rs", "op": "move", "why": "route-neutral Kernel enum (FUSE and NFS variants) belongs beside registry.rs, not in a FUSE folder"},
  {"from": "engine/crates/layerfs-daemon/src/workspace/kernel/attach.rs", "to": "engine/crates/layerfs-daemon/src/workspace/fuse/attach.rs", "op": "move", "why": "kernel/ holds only the FUSE route; fuse/ and nfs/ are the two KernelRoute variants"},
  {"from": "engine/crates/layerfs-daemon/src/workspace/kernel/dispatcher.rs", "to": "engine/crates/layerfs-daemon/src/workspace/fuse/dispatcher.rs", "op": "move", "why": "KernelServing is the FUSE session dispatcher; sibling of nfs/"},
  {"from": "engine/crates/layerfs-daemon/src/workspace/kernel/invalidate.rs", "to": "engine/crates/layerfs-daemon/src/workspace/fuse/invalidate.rs", "op": "move", "why": "FUSE notify invalidation; NFS path delegates to nfs/export.rs"},
  {"from": "engine/crates/layerfs-daemon/src/workspace/kernel/mod.rs", "to": "engine/crates/layerfs-daemon/src/workspace/fuse/mod.rs", "op": "rename", "why": "folder renamed kernel/ -> fuse/"},
  {"from": "engine/crates/layerfs-daemon/src/operator/list_workspaces.rs", "to": "engine/crates/layerfs-daemon/src/workspace/status/list.rs", "op": "move", "why": "impl Service read of the private registry (Entry, entries, job); beside its twin status/doctor.rs; avoids widening visibility"},
  {"from": "engine/crates/layerfs-daemon/src/operator/workspace.rs", "to": "engine/crates/layerfs-daemon/src/operator/list_changes.rs", "op": "split", "why": "three calls in one file named like the sibling workspace/ folder; one call per file"},
  {"from": "", "to": "engine/crates/layerfs-daemon/src/operator/list_changed.rs", "op": "new", "why": "backend.daemon.workspace.list_changed, from operator/workspace.rs"},
  {"from": "", "to": "engine/crates/layerfs-daemon/src/operator/get_storage.rs", "op": "new", "why": "backend.daemon.workspace.get_storage, from operator/workspace.rs"},
  {"from": "engine/crates/layerfs-daemon/src/application/startup/lifecycle.rs", "to": "engine/crates/layerfs-daemon/src/application/startup/lifecycle.rs", "op": "split", "why": "slot admission state machine is connection custody used by workers.rs and retained.rs"},
  {"from": "", "to": "engine/crates/layerfs-daemon/src/application/connection/slots.rs", "op": "new", "why": "Slots, SlotClass, AdmissionClass, SlotTicket, Finished, Disposition, disposition, recovery_request"},
  {"from": "engine/crates/layerfs-daemon/src/application/startup/serve.rs", "to": "engine/crates/layerfs-daemon/src/application/startup/install.rs", "op": "rename", "why": "holds Application::install (one-time install admission), not serving"},
  {"from": "engine/crates/layerfs-daemon/src/application/startup/filesystem.rs", "to": "engine/crates/layerfs-daemon/src/application/startup/serving.rs", "op": "rename", "why": "assembles KernelServing and ExportServing; 'filesystem' fits everything"},
  {"from": "engine/crates/layerfs-daemon/src/application/diagnostics/counters.rs", "to": "engine/crates/layerfs-daemon/src/application/observed/records.rs", "op": "move", "why": "Request::Observed record stream (Records, observed_control); diagnostic means integrity checks elsewhere"},
  {"from": "engine/crates/layerfs-daemon/src/application/diagnostics/kernel.rs", "to": "engine/crates/layerfs-daemon/src/application/observed/kernel.rs", "op": "move", "why": "folder renamed diagnostics/ -> observed/"},
  {"from": "engine/crates/layerfs-daemon/src/application/diagnostics/resources.rs", "to": "engine/crates/layerfs-daemon/src/application/observed/resources.rs", "op": "move", "why": "folder renamed diagnostics/ -> observed/"},
  {"from": "engine/crates/layerfs-daemon/src/application/diagnostics/schema.rs", "to": "engine/crates/layerfs-daemon/src/application/observed/schema.rs", "op": "move", "why": "folder renamed diagnostics/ -> observed/"},
  {"from": "engine/crates/layerfs-daemon/src/application/diagnostics/mod.rs", "to": "engine/crates/layerfs-daemon/src/application/observed/mod.rs", "op": "rename", "why": "folder renamed diagnostics/ -> observed/"},
  {"from": "engine/crates/layerfs-daemon/src/workspace/teardown/dirty.rs", "to": "engine/crates/layerfs-daemon/src/workspace/status/dirty.rs", "op": "move", "why": "a read (DirtyReport); status/changed.rs uses its join/Names/WALK"},
  {"from": "engine/crates/layerfs-daemon/src/workspace/route/operations.rs", "to": "engine/crates/layerfs-daemon/src/workspace/route/dispatch.rs", "op": "rename", "why": "Request-to-Service dispatch; 'operations' is the vfs/mount-ports filesystem-operation word"},
  {"from": "engine/crates/layerfs-daemon/tests/control/daemon_slots.rs", "to": "engine/crates/layerfs-daemon/tests/application/daemon_slots.rs", "op": "move", "why": "control/ area word was renamed away; slots are application connection custody"},
  {"from": "engine/crates/layerfs-daemon/tests/control/slot_state.rs", "to": "engine/crates/layerfs-daemon/tests/application/slot_state.rs", "op": "move", "why": "application slot state machine"},
  {"from": "engine/crates/layerfs-daemon/tests/control/telemetry_recording.rs", "to": "engine/crates/layerfs-daemon/tests/application/telemetry_recording.rs", "op": "move", "why": "application-level recording guard"},
  {"from": "", "to": "engine/crates/layerfs-daemon/tests/application/main.rs", "op": "new", "why": "area binary root for application/"},
  {"from": "engine/crates/layerfs-daemon/tests/control/main.rs", "to": "engine/crates/layerfs-daemon/tests/workspace/main.rs", "op": "rename", "why": "area control/ -> workspace/"},
  {"from": "engine/crates/layerfs-daemon/tests/control/root.rs", "to": "engine/crates/layerfs-daemon/tests/workspace/control_root.rs", "op": "move", "why": "'control' is fuse mount::control_root, not a campaign prefix"},
  {"from": "engine/crates/layerfs-daemon/tests/control/workspace_databases.rs", "to": "engine/crates/layerfs-daemon/tests/workspace/databases.rs", "op": "move", "why": "area rename; drop word the folder says"},
  {"from": "engine/crates/layerfs-daemon/tests/control/doctor_registry.rs", "to": "engine/crates/layerfs-daemon/tests/workspace/doctor_registry.rs", "op": "move", "why": "area control/ -> workspace/"},
  {"from": "engine/crates/layerfs-daemon/tests/control/mount_holders.rs", "to": "engine/crates/layerfs-daemon/tests/workspace/mount_holders.rs", "op": "move", "why": "area control/ -> workspace/"},
  {"from": "engine/crates/layerfs-daemon/tests/control/named_mount.rs", "to": "engine/crates/layerfs-daemon/tests/workspace/named_mount.rs", "op": "move", "why": "area control/ -> workspace/"},
  {"from": "engine/crates/layerfs-daemon/tests/control/operator_workspace_changes.rs", "to": "engine/crates/layerfs-daemon/tests/workspace/operator_workspace_changes.rs", "op": "move", "why": "area control/ -> workspace/"},
  {"from": "engine/crates/layerfs-daemon/tests/support/gate/history_gate.rs", "to": "engine/crates/layerfs-daemon/tests/support/gate/history.rs", "op": "rename", "why": "file repeats its folder name"},
  {"from": "engine/crates/layerfs-daemon/tests/support/gate/store_gate.rs", "to": "engine/crates/layerfs-daemon/tests/support/gate/store.rs", "op": "rename", "why": "file repeats its folder name"},
  {"from": "engine/crates/layerfs-daemon/tests/mounted_commit/orphan_commit.rs", "to": "engine/crates/layerfs-daemon/tests/mounted_commit/orphan.rs", "op": "rename", "why": "drop word the folder says"},
  {"from": "engine/crates/layerfs-daemon/tests/mounted_commit/parallel_commits.rs", "to": "engine/crates/layerfs-daemon/tests/mounted_commit/parallel.rs", "op": "rename", "why": "drop word the folder says"},
  {"from": "engine/crates/layerfs-daemon/examples/store_accounting/startup_records.rs", "to": "engine/crates/layerfs-daemon/examples/store_accounting/engine_records.rs", "op": "rename", "why": "matches its module name engine_records and its 'engine families' role"}
]
```
