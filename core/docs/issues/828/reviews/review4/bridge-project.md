# Review 4: `layerfs-bridge` and `layerfs-project`

Scope: the planned rows in `S/nav/site/layout.json` whose `n` starts with
`engine/crates/layerfs-bridge/` or `engine/crates/layerfs-project/` (src, tests,
examples). Old source read from the archive `S/v2tree` (layerfs_v2_0_0 at
1317ebf7ca). LOC is the plan's `loc` column (= `S/prod_loc_files.txt`, prod).
`S` = `/private/tmp/claude-501/-Users-yifanxu-Ephemeral-AI-Lab-layerfs/641fd44c-e5a6-4baa-bdc0-04ce0d86438d/scratchpad`.

## 1. Verdicts

**layerfs-bridge: good overall, with one naming collision and two misplaced files.**
The crate has two jobs. (a) It holds the records sent over the daemon's
authenticated connection: `duplex/` (Noise channel), `control/` (Call/Answer
records), `provision/` (Store install and daemon setup), `initial_record.rs`
(picks Control or Install), `wire.rs` (checked fields) and `sdk_values.rs`. (b) It
holds local OS facilities that the home, the Sandbox drivers and the daemon all
use: `process/` and the planned `host/`. Every one of the 16 dependent crates
uses one of those two jobs (checked with `use` lines).

The plan's shape is right: `control/` 12, `duplex/` 8, `process/` 5,
`provision/` 4, `host/` 3, and 4 loose files at the root. The defects:
- `host/` collides with the control layer's word "host". In the control layer,
  "host" means the host side of host↔machine: a host operation, Sandbox
  `route/host/`, Workspace `host/`. Yet half of what bridge's `host/` holds runs
  inside the daemon in a Sandbox.
- `control/install.rs` is not a control record. It is the reply half of the
  Install record family. It also carries `ControlError`, the error type every
  codec in the crate uses.
- `provision/` mixes its file designs (`manifest.rs` + `wire.rs` for one record),
  and `kinds.rs` is a vague loose file.

`process/` does not collide with Sandbox `process/`. Both mean "an OS process on
this kernel": Sandbox's `process/daemon.rs` builds on `layerfs_bridge::process::LocalProcess`.

**layerfs-project: good; two names to fix.**
- `store/` is predictable and matches the engine's word. Persistence `store/`
  opens and seals Stores; store-service `store/install.rs` is the daemon side of
  the same install whose host side is project `store/install.rs`.
- `import/` is predictable: it is the opposite of control store
  `data/project/export/`. It also stays distinct from vfs `construction/`, and
  one line explains why: host files → Store, versus a captured Workspace → Store.
- `import/namespace.rs` uses the same word as vfs `construction/namespace/`.
- `import/` is at the 12-file limit. It holds one pipeline (scan → backing →
  files → namespace → hints, run by `init.rs`) plus its error and counters, which
  is one role.

The defects:
- The crate has two `init.rs` files with two different `InitRequest` types.
- A test file repeats its folder name.
- An "example helper" is used only by a test.

## 2. Findings (ranked by value)

### F1. Bridge `host/` → `system/`, and move `process/mount_table.rs` into it

**What.** Planned `src/host/{log_file,network,sandbox_runtime}.rs` reuses a word
the control layer already uses for a different idea. It also does not describe
the files.

**Evidence.**
- In the control layer, "host" means the host side of host↔machine:
  - The Sandbox "host operation" (`sandbox/src/host/mod.rs`: "A host operation, typed … the `sandbox.host` frame").
  - Planned `layerfs-sandbox/src/route/host/` ("host dispatch is how a Sandbox call reaches its machine").
  - Planned `layerfs-workspace/src/host/` ("host side of Workspace operations").
- The bridge files are not host-side:
  - `sandbox_runtime.rs`: "sealed into the Linux bundle as `runtime/` and bound read-only into every namespaces Sandbox at [`GUEST`]". Its users are the Sandbox helper binary (`bin/layerfs-sandbox-helper/{exec,root}.rs`) and namespaces `spec.rs`/`create.rs`.
  - `network.rs`: "a daemon in a namespaces Sandbox runs the bundle's `getent` through the bundle's loader" (`crate::sandbox_runtime::from_environment()`). The host-side store diagnostic uses the same function.
  - `log_file.rs`: "Log rotation, one rule for every log LayerFS writes". Its users are home `logs.rs`/`tail.rs` and platform `setup/forward.rs`/`worker.rs`.
- What the files share is that each is a facility of the OS this process runs
  on, usually one fixed system tool run under a deadline:
  - `network.rs`: "Bounded system-name resolution" (`dscacheutil`/`getent`).
  - `process/mount_table.rs`: "The macOS mount table under a deadline: one `/sbin/mount` run with a fixed path".
- `mount_table.rs` is not a process fact. `process/mod.rs` lists it only as an
  add-on ("…and the macOS mount table under a deadline"). It uses only
  `super::Retained`. Its callers want the mount table:
  - daemon `control/export_helper.rs`: `layerfs_bridge::process::mount_table()`
  - sandbox-backend `native/macos/darwin.rs`: `pub(crate) use layerfs_bridge::process::mount_table`
- After the move, `process/` holds exactly what its header promises: identity,
  boot, and retained child custody.
- "system" names no LayerFS concept: no `system` folder exists in any tree file.

**Fix.**
- `src/host/` → `src/system/`. The files keep their names.
- `src/process/mount_table.rs` → `src/system/mount_table.rs`. Change
  `use super::Retained` → `use crate::process::Retained`.
- Public paths become `layerfs_bridge::system::{log_file, network, sandbox_runtime, mount_table}`:
  - The plan already changes these paths for `host::`, which has 4 + 5 + 9 callers.
  - `mount_table` adds 2 more call sites.
- `tests/host/` → `tests/system/` (same 5 topics plus `main.rs`).
- Fallback name if the owner prefers it: `os/`. It is weaker because the
  Sandbox backend already uses `os` as an alias for the per-OS backend module.

### F2. `control/install.rs` → `provision/install.rs`; `ControlError` → root `wire.rs`

**What.** The planned `control/install.rs` (old `control.rs`, 156 LOC) holds two
unrelated things:
- the Install exchange's reply records: `InstallPhase`, `RefusalKind`, `InstallRefusal`, `InstallReply`, `INSTALL_FINISH`;
- `pub struct ControlError(pub &'static str)`, the crate-wide record error.

**Evidence.**
- `initial_record.rs` splits the first record into two families,
  `Control(Call)` (magic `LFSC`) and `Install(StoreManifest)` (magic `LFSI`,
  `provision_wire::MAGIC`). So Install is a sibling of Control, not part of it.
- Every caller imports the Install records together with `provision`:
  - daemon `install.rs`: `control::{ControlError, InstallPhase, InstallReply, INSTALL_FINISH}` with `provision::{StoreManifest, StoreProfile}`
  - project `store/install.rs`: `control::{…InstallReply, INSTALL_FINISH}` with `provision::StoreProfile`
  - project `store/install_types.rs`: `control::{ControlError, InstallPhase, InstallRefusal}` with `provision::StoreManifest`
  - sandbox `daemon/channel.rs`: `control::InstallPhase::Request`
- `ControlError` is used by every codec:
  - `wire.rs` starts with `use crate::control::ControlError`;
  - `provision_wire.rs`, `daemon_setup.rs` and `store.rs` all import it.
  
  It is the error of a checked field, so it belongs in `wire.rs`, the file that
  checks fields.

**Fix.**
- Move the Install records to `src/provision/install.rs`, and update the 4
  product call sites to `provision::{InstallPhase, …}`.
- Move `ControlError` into `src/wire.rs`. `control/mod.rs` keeps
  `pub use crate::wire::ControlError`, so the ~100 `control::ControlError` imports
  across crates are unchanged.
- Result: `control/` holds only Call/Answer records (11 files) and `provision/`
  holds the whole Install family.

### F3. Project: two `init.rs` files → rename `import/init.rs` to `import/run.rs`

**What.** `src/import/init.rs` and `src/store/init.rs` share a name but do
different jobs, and each defines a public `InitRequest`:
- `layerfs_project::InitRequest` runs the namespace import;
- `layerfs_project::store::InitRequest` runs the Store Init (`ProjectApi`).

**Evidence.**
- `import/init.rs`: "Scan, construct/save, consume inode identities and publish one genesis stack". It holds `init`, `init_observed`, `Initialized`, `init_unknown_state`.
- `store/init.rs`: "Host Init and original handoff/failure custody". It holds `ProjectApi`, `SealedProject`, `InitError`, the Seal step (`counters.step(InitStep::Seal)`), and it calls `crate::init::init_unknown_state`.
- The settled rule says "a call folder's entry file is `run.rs`" (project `delete/`, `export/`, `init/`).
- Control store's `data/project/init/run.rs` is the control-side counterpart and already uses that name.

**Fix.**
- `src/import/init.rs` → `src/import/run.rs`. In `lib.rs`, change the alias
  `pub(crate) use import::init` → `import::run`, and update
  `crate::init::init_unknown_state` in `store/init.rs` and `store/live.rs`.
- `store/init.rs` stays as the Init call. That matches api `families/project/init.rs`.
- Public function names (`init`, `init_observed`) are unchanged. The duplicate
  type name `InitRequest` is an API naming issue outside this pass; noted, not
  proposed.

### F4. Merge `provision/wire.rs` into `provision/manifest.rs`

**What.**
- The siblings in `provision/` do not share one design.
  `daemon_setup.rs` (`DaemonSetup::encode/decode`) and `store.rs`
  (`DaemonStore::encode/decode`) each keep a record's type and codec together.
  `StoreManifest` alone is split: the type is in `manifest.rs` (77 LOC), its codec
  in `wire.rs` (68 LOC, `impl StoreManifest { encode, decode }` plus `MAGIC`).
- The crate also gets two `wire.rs` files (root and `provision/`) with different roles.

**Fix.** Merge `src/provision/wire.rs` into `src/provision/manifest.rs` (145 LOC).
`initial_record.rs` then refers to `crate::provision::manifest::MAGIC`.

### F5. Dissolve loose `kinds.rs` (planned rename of `support.rs`)

**What.**
- "kinds" fits anything; it is the same kind of name as the rejected `support_rows.rs`.
- The file holds two unrelated enums: `KernelRoute` (FUSE/NFS) and `StoreKind` (SQLite/PostgreSQL), 40 LOC.

**Evidence.**
- Inside the crate, only `daemon_setup.rs` uses `KernelRoute` (`pub route: KernelRoute`, encoded as 0/1).
- `StoreKind` names the same split as `DaemonStore::{Sqlite, Postgres}` in `store.rs`.
- Outside the crate, both are reached only through the root re-export `layerfs_bridge::{KernelRoute, StoreKind}`:
  - `KernelRoute`: daemon `filesystem.rs`, sandbox `lifecycle.rs`/`launch/local.rs`, platform `platform.rs`
  - `StoreKind`: store `record.rs`, platform `support.rs`

**Fix.**
- Move `KernelRoute` → `src/provision/daemon_setup.rs`.
- Move `StoreKind` → `src/provision/store.rs`.
- `lib.rs` keeps `pub use provision::{KernelRoute, StoreKind}`, so callers are unchanged.
- The root then has 3 loose files (`initial_record.rs`, `sdk_values.rs`, `wire.rs`).

### F6. `examples/history_support/canonical_memo.rs` → `tests/support/canonical_memo.rs` (project)

**What.**
- The exception list grants it as a "shared example helper (1 file)", but no
  example uses it.
- Its only user is the test `tests/history_proof_memo.rs`, through
  `#[path = "../examples/history_support/canonical_memo.rs"] mod canonical_memo;`.
- Nothing in any `examples/*.rs` or `Cargo.toml` names it (grepped the whole archive).

**Fix.**
- Move the helper to `tests/support/canonical_memo.rs`; layout rule 7 puts test
  helpers in `tests/support/`.
- The `acquisition/` area then uses `support::canonical_memo`.
- `examples/history_support/` and its exception disappear.
- The `durable_init_costs/vfs.rs` exception remains valid: `durable_init_costs.rs` uses it.

### F7. `tests/acquisition/backed_acquisition.rs` → `tests/acquisition/backed.rs` (project)

The file name repeats its folder name (rule 14). The plan already drops the
prefix from its sibling (`acquisition_custody.rs` → `acquisition/custody.rs`), so
this applies the same treatment.

### F8. `control/merge.rs` → `control/commit.rs` (bridge, low value)

**What.** The file is named after a step of the call, not the call (rule 15).

**Evidence.**
- Its header: "Commit options, merged Commits and conflict reports on the control wire".
- Its contents belong to the Commit call alone: `Request::Commit(CommitControl)`, `Reply::MergeCommitted(MergedCommit)`, `Reply::Conflict(ConflictReport)`.
- The other call files in `control/` are already named after their call: `hello`, `mount`, `unmount`, `sdk`, `forced` (ForceUnmount; daemon `teardown/forced.rs` uses the same word).
- Elsewhere Commit is `commit` (store-service `workspace/commit/`, api `families/commit`).

**Fix.** `src/control/merge.rs` → `src/control/commit.rs`. This is internal only;
`control/mod.rs` re-exports everything.

### F9. (Optional) Fold `control/vocabulary.rs` into `request.rs` and `reply.rs`

**What.**
- `vocabulary.rs` (217 LOC) holds the record types, while `request.rs` and
  `reply.rs` hold only the codecs (`impl Call { encode, decode }` and
  `impl Answer { encode, decode }`).
- A reader who opens `request.rs` for `Request` will not find it there.
- Every other file in `control/` keeps types and codec together.

**Fix.**
- Move `Request`, `Call`, `WorkspaceToken`, `MountPlace` and `HISTORY_WINDOW` into `request.rs` (~430 LOC).
- Move `Reply`, `Answer`, `WorkspaceStatus`, `LocalObservation`, `Activity`, `ControlCode`, `ControlRefusal` and `CleanupObservation` into `reply.rs` (~500 LOC).
- Delete `vocabulary.rs`.
- Public paths do not change (`control::*`).

The gain is moderate; do it only if the owner wants the vague name gone.

### F10. (Optional) Merge `import/namespace_work.rs` into `import/observe.rs` (project)

**What.**
- `NamespaceWork` (20 LOC, "Count-driven acquisition diagnostics") holds the
  final counts of the same import whose live counts `observe.rs` holds:
  `InitCounters`, `InitCounts`, `InitStep`.
- Both are public at the root.

**Fix.**
- Merge `namespace_work.rs` into `observe.rs` (74 LOC). `import/` drops to 11 files.
- Drop the `import::work as namespace_work` alias in `lib.rs`.

This is a small gain. The planned rename `namespace_work.rs` is acceptable if
it is not taken.

## 3. Proposed layout of changed folders

Scope: the planned layout (`S/nav/site/layout.json`, `loc` = `tools/production_loc.py`
prod at archive 1317ebf7ca). Only changed folders are shown. Tests and examples
carry no production LOC (shown `0`).

```diff
  engine/crates/layerfs-bridge/src/      [4,875]  (crate total unchanged; moved files are listed at both ends)
    initial_record.rs                       24   (MAGIC refs → control::request, provision::manifest)
-   kinds.rs                                40   (F5: KernelRoute → provision/daemon_setup.rs, StoreKind → provision/store.rs)
    lib.rs                                  33
    sdk_values.rs                          293
    wire.rs                                 76   (F2: gains ControlError)
    control/                           [2,528]  (duplex/ unchanged, not shown)
      capacity.rs                          162
      commit.rs                            251   (F8: renamed from planned merge.rs)
      forced.rs                            119
      hello.rs                             102
      history.rs                           295
-     install.rs                           156   (F2: → provision/install.rs; ControlError → wire.rs)
      mod.rs                                 —   (planned new; adds pub use crate::wire::ControlError)
      mount.rs                             276
      reply.rs                             389   (F9 optional: gains Reply/Answer types)
      request.rs                           314   (F9 optional: gains Request/Call types)
      sdk.rs                               199
      unmount.rs                            48
      vocabulary.rs                        217   (F9 optional: removed, folded into request/reply)
    process/                             [453]
      darwin.rs                            144
      identity.rs                           59
      linux.rs                              40
      mod.rs                                16
-     mount_table.rs                        75   (F1: → system/mount_table.rs)
      retained.rs                          119
    provision/                           [623]
      daemon_setup.rs                      157   (F5: gains KernelRoute)
      install.rs                           156   (F2: moved in from control/install.rs, minus ControlError)
      manifest.rs                           77   (F4: gains StoreManifest codec + MAGIC from wire.rs)
      mod.rs                                 —   (planned new)
      store.rs                             165   (F5: gains StoreKind)
-     wire.rs                               68   (F4: merged into manifest.rs)
-   host/                                [220]  (F1: renamed system/)
-     log_file.rs                           35
-     mod.rs                                 —
-     network.rs                           140
-     sandbox_runtime.rs                    45
+   system/                              [295]
      log_file.rs                           35   (from host/)
+     mod.rs                                 —   new
      mount_table.rs                        75   (F1: from process/)
      network.rs                           140   (from host/)
      sandbox_runtime.rs                    45   (from host/)
```

```diff
  engine/crates/layerfs-bridge/tests/          (partial: wire/ unchanged)
-   host/                                        (F1: renamed system/)
-     main.rs                                0
-     log_file.rs                            0
-     network_deadline.rs                    0
-     network_linux_lifecycle.rs             0
-     process_identity.rs                    0
-     sandbox_runtime.rs                     0
+   system/
+     main.rs                                —   new (planned host/main.rs)
      log_file.rs                            0   (from host/)
      network_deadline.rs                    0   (from host/)
      network_linux_lifecycle.rs             0   (from host/)
      process_identity.rs                    0   (from host/)
      sandbox_runtime.rs                     0   (from host/)
```

```diff
  engine/crates/layerfs-project/src/     [2,516]  (store/ unchanged, not shown)
    lib.rs                                  21   (alias import::init → import::run; F10 drops namespace_work alias)
    import/                            [1,839]
      backing.rs                           434
      batch.rs                              86
      error.rs                             116
      files.rs                             203
      hints.rs                              46
      mod.rs                                17
      namespace.rs                         200
      namespace_work.rs                     20   (F10 optional: merged into observe.rs)
      observe.rs                            54   (F10 optional: gains NamespaceWork)
      run.rs                               351   (F3: renamed from init.rs)
      scan.rs                              274
      source.rs                             38
```

```diff
  engine/crates/layerfs-project/tests/         (partial: init/ unchanged)
    acquisition/
      backed.rs                              0   (F7: renamed from backed_acquisition.rs)
      custody.rs                             0
      history_proof_memo.rs                  0   (uses support::canonical_memo)
      main.rs                                —   (planned new)
    support/
      canonical_memo.rs                      0   (F6: moved from examples/history_support/)
      memory_acquisition.rs                  0
      memory_history.rs                      0
      memory_objects.rs                      0
      mod.rs                                 0
      old_snapshot.rs                        0
  engine/crates/layerfs-project/examples/      (partial)
-   history_support/                             (F6: exception no longer needed)
-     canonical_memo.rs                      0
```

## 4. Checked and left alone

- **Bridge `control/` folder**: settled and kept. Its contents are one role
  (Call/Answer records) once F2 removes Install. `history.rs` (History values on
  the control wire), `capacity.rs` (#540 read model parsed from `SdkValue`),
  `hello.rs` (merge of `daemon_wire` + `daemon_types`) and `forced.rs` (matches
  daemon `teardown/forced.rs`) are all fine.
- **Bridge `duplex/`** (8 files): one design, and the names predict their contents.
  - `duplex/admission.rs` correctly stays apart from `control/capacity.rs`: it is
    the unauthenticated notice sent before the handshake.
  - `ending.rs` (15 LOC) could join `peer.rs`, but the gain is too small.
- **Bridge `process/` vs Sandbox `process/`**: same meaning (an OS process on this
  kernel), layered engine → control. Sandbox's `process/daemon.rs` imports
  `layerfs_bridge::process::LocalProcess`. No collision.
  - `darwin.rs` also holds `sync_volume_wait`, a volume fact. It stays because
    `darwin.rs` is the crate's one audited `unsafe` file (lib.rs comment).
- **`system/network.rs` name**: "network" also names Sandbox
  `backend/namespaces/network/`, which writes `resolv.conf`. `resolve.rs` would be
  more exact, but `network` is clear enough and has 2 product callers. Not proposed.
- **Splitting bridge's OS facilities (`process/`, `system/`) into their own
  engine crate** would match the two roles. That is a crate-level change with
  Cargo churn across 16 dependents, so it is not proposed here.
- **Bridge loose `sdk_values.rs`** (293 LOC after the planned merge): SDK
  encodings of History records, used by store, sandbox, home and store-service.
  It is not a control-wire record, so it is right outside `control/`.
- **Bridge tests `tests/wire/`** (9 topics): fine. The `_records` suffix on 4 of
  them is a suffix, not a prefix, so the prefix rule does not apply.
- **Project `store/`**: public module (`layerfs_project::store::*` has ~25 call
  sites in control) and consistent with engine `store/` folders.
  - `install_types.rs` → `install_outcome.rs` (planned) matches daemon `install/outcome.rs`.
  - `live.rs` / `live_open.rs` (2 files) are below the prefix threshold.
- **Project `import/` name vs vfs `construction/` and control `init/`**: covered
  in §1; kept.
  - `lib.rs` re-aliases most `import::*` modules to the crate root
    (`pub(crate) use import::backing;` …), so the folder is internal grouping
    only. That is acceptable.
- **Project tests `init/`** (6 topics) are fine. `history_proof_memo.rs` sits in
  `acquisition/` only to keep that area at 3 topics, which is acceptable.
- **Depth and counts** after the changes:
  - bridge: root 3 loose; `control/` 11 (10 with F9); `provision/` 4; `process/` 4; `system/` 4; `duplex/` 8.
  - project: `import/` 12 (11 with F10); `store/` 5.
  - All are within limits; depth ≤ 2.

## 5. Machine-readable changes

```json
[
  {"from": "engine/crates/layerfs-bridge/src/host/log_file.rs", "to": "engine/crates/layerfs-bridge/src/system/log_file.rs", "op": "move", "why": "host/ collides with the control layer's host side; these are OS facilities, some run in the Sandbox daemon"},
  {"from": "engine/crates/layerfs-bridge/src/host/network.rs", "to": "engine/crates/layerfs-bridge/src/system/network.rs", "op": "move", "why": "host/ → system/: bounded system-name resolution, also run inside the Sandbox daemon"},
  {"from": "engine/crates/layerfs-bridge/src/host/sandbox_runtime.rs", "to": "engine/crates/layerfs-bridge/src/system/sandbox_runtime.rs", "op": "move", "why": "host/ → system/: the runtime bound into the Sandbox at GUEST is not host-side"},
  {"from": "engine/crates/layerfs-bridge/src/host/mod.rs", "to": "engine/crates/layerfs-bridge/src/system/mod.rs", "op": "rename", "why": "folder renamed host/ → system/"},
  {"from": "engine/crates/layerfs-bridge/src/process/mount_table.rs", "to": "engine/crates/layerfs-bridge/src/system/mount_table.rs", "op": "move", "why": "the mount table is a system fact under a deadline, not a process fact"},
  {"from": "engine/crates/layerfs-bridge/tests/host/main.rs", "to": "engine/crates/layerfs-bridge/tests/system/main.rs", "op": "rename", "why": "test area follows src host/ → system/"},
  {"from": "engine/crates/layerfs-bridge/tests/host/log_file.rs", "to": "engine/crates/layerfs-bridge/tests/system/log_file.rs", "op": "move", "why": "test area follows src rename"},
  {"from": "engine/crates/layerfs-bridge/tests/host/network_deadline.rs", "to": "engine/crates/layerfs-bridge/tests/system/network_deadline.rs", "op": "move", "why": "test area follows src rename"},
  {"from": "engine/crates/layerfs-bridge/tests/host/network_linux_lifecycle.rs", "to": "engine/crates/layerfs-bridge/tests/system/network_linux_lifecycle.rs", "op": "move", "why": "test area follows src rename"},
  {"from": "engine/crates/layerfs-bridge/tests/host/process_identity.rs", "to": "engine/crates/layerfs-bridge/tests/system/process_identity.rs", "op": "move", "why": "test area follows src rename"},
  {"from": "engine/crates/layerfs-bridge/tests/host/sandbox_runtime.rs", "to": "engine/crates/layerfs-bridge/tests/system/sandbox_runtime.rs", "op": "move", "why": "test area follows src rename"},
  {"from": "engine/crates/layerfs-bridge/src/control/install.rs", "to": "engine/crates/layerfs-bridge/src/provision/install.rs", "op": "split", "why": "Install records are the LFSI family (initial_record splits Control vs Install), not control records"},
  {"from": "engine/crates/layerfs-bridge/src/control/install.rs", "to": "engine/crates/layerfs-bridge/src/wire.rs", "op": "merge", "why": "ControlError is the crate-wide checked-field error; wire.rs already imports it; control/mod.rs re-exports"},
  {"from": "engine/crates/layerfs-bridge/src/provision/wire.rs", "to": "engine/crates/layerfs-bridge/src/provision/manifest.rs", "op": "merge", "why": "type + codec in one file like daemon_setup.rs and store.rs; removes a second wire.rs"},
  {"from": "engine/crates/layerfs-bridge/src/kinds.rs", "to": "engine/crates/layerfs-bridge/src/provision/daemon_setup.rs", "op": "merge", "why": "KernelRoute's only in-crate user is DaemonSetup; vague loose name removed"},
  {"from": "engine/crates/layerfs-bridge/src/kinds.rs", "to": "engine/crates/layerfs-bridge/src/provision/store.rs", "op": "merge", "why": "StoreKind names the same split as DaemonStore; root re-export kept"},
  {"from": "engine/crates/layerfs-bridge/src/control/merge.rs", "to": "engine/crates/layerfs-bridge/src/control/commit.rs", "op": "rename", "why": "call file named after the call: Request::Commit, MergeCommitted, Conflict"},
  {"from": "engine/crates/layerfs-bridge/src/control/vocabulary.rs", "to": "engine/crates/layerfs-bridge/src/control/request.rs", "op": "merge", "why": "optional: Request/Call types live with their codec"},
  {"from": "engine/crates/layerfs-bridge/src/control/vocabulary.rs", "to": "engine/crates/layerfs-bridge/src/control/reply.rs", "op": "merge", "why": "optional: Reply/Answer/status types live with their codec"},
  {"from": "engine/crates/layerfs-project/src/import/init.rs", "to": "engine/crates/layerfs-project/src/import/run.rs", "op": "rename", "why": "two init.rs with two InitRequest types; entry file is run.rs (settled), like control data/project/init/run.rs"},
  {"from": "engine/crates/layerfs-project/examples/history_support/canonical_memo.rs", "to": "engine/crates/layerfs-project/tests/support/canonical_memo.rs", "op": "move", "why": "only user is tests/history_proof_memo.rs via #[path]; no example uses it"},
  {"from": "engine/crates/layerfs-project/tests/acquisition/backed_acquisition.rs", "to": "engine/crates/layerfs-project/tests/acquisition/backed.rs", "op": "rename", "why": "file repeats its folder name (rule 14)"},
  {"from": "engine/crates/layerfs-project/src/import/namespace_work.rs", "to": "engine/crates/layerfs-project/src/import/observe.rs", "op": "merge", "why": "optional: Init's final counts beside its live counts; drops an alias"}
]
```
