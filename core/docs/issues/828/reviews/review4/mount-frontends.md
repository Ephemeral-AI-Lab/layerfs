# Review 4: mount front ends (`layerfs-nfs`, `layerfs-fuse`, `layerfs-mount-ports`)

Base: the planned tree (`nav/site/layout.json`, `nav/trees/tree-engine.txt`). Code read from the
archive of `layerfs_v2_0_0` at `1317ebf7ca` (`v2tree/`). Production LOC from `prod_loc_files.txt`
(the `tools/production_loc.py --files` counter at that commit). Layout only: no Cargo dependency,
`[patch.crates-io]`, `patches/fuser-0.18.0/` or `#![forbid(unsafe_code)]` line changes anywhere, so
the fuser provenance checks (`tools/check_fuser_integrity.py`, engine/AGENTS.md "Fuser provenance")
and `tools/check_product_boundary.py` (crate names, `lib.rs` paths, `mount-ports/src/ports.rs`) are
untouched.

## 1. Verdict per crate

- **layerfs-nfs: needs regrouping (no extra churn).** Every NFS row already moves in the plan, so
  choosing better destinations costs nothing extra. The planned split between `protocol/` and
  `procedures/` cuts one job in half. `program.rs` (in `protocol/`) dispatches every NFS procedure
  and hands the changing ones to `change_args.rs` (in `procedures/`). `server/` holds two files that
  the server never uses. `apple_double/names.rs` holds the silly-rename rule, which is not
  AppleDouble. The `kernel.rs` split is only partly specified.
- **layerfs-fuse: sound, with small fixes.** `coherence/` is three tiny files (50 LOC) used only by
  `request/`. Three file names differ from the words the rest of the stack uses for the same job:
  `mutate`, `startup` and `force`. `tests/` is 4 flat binaries, which breaks rule 7.
- **layerfs-mount-ports: src sound.** `operations/` uses the same opcode words as vfs
  `operations/{kernel,namespace}/` and overlay `kernel/`. Its one test file cannot form a
  rule-7 area on its own; the fix is to move it to the FUSE test area (finding 5).

### Do FUSE and NFS share one inner design?

They should share words for the same jobs, not a forced common folder set. Where the code really
differs, the difference fits in one line. Proposed mapping after the findings below:

| Job | FUSE | NFS | Shared below (mount-ports / vfs / overlay / daemon) |
| --- | --- | --- | --- |
| Wire format | (none: the pinned fuser crate) + `mount.rs` syscalls | `protocol/` (rpc, xdr, attributes, handle) | — |
| Per-request handlers | `request/` (fuser callbacks) | `procedures/` (ONC RPC programs) | — |
| Mutations | `request/mutation.rs` | `procedures/mutation.rs` → `kernel/mutation.rs` | `operations/mutation.rs`, vfs `operations/kernel/mutation.rs`, overlay `kernel/mutation.rs` |
| Directory listing | `request/directory.rs` | `procedures/directory.rs` | `operations/directory.rs`, vfs/overlay `…/directory.rs` |
| Attribute projection, `Identity` | `attributes.rs` | `protocol/attributes.rs` | — |
| Kernel references and open state | the real kernel | `kernel/` (the synthetic kernel) | overlay `kernel/references.rs`, daemon `workspace/kernel/` |
| Connection lifecycle | `session/` (`attach`, `drain`, `forced`) | `server.rs` (listener) | daemon `workspace/kernel/attach.rs`, `teardown/forced.rs` |
| Request ownership and drain | `dispatch/` | none: one call at a time per connection thread (`kernel/wait.rs`) | daemon `workspace/kernel/dispatcher.rs` |
| Cache coherence after an install | `session/invalidate.rs` (notification channel) | `kernel/refresh.rs` (a mark in the synthetic kernel; NFS has no channel) | daemon `workspace/kernel/invalidate.rs` |
| Client-specific names | — | `apple_double/`, `kernel/names.rs` | — |

The one-line differences:

- FUSE has no `protocol/` because fuser owns the wire.
- NFS has `kernel/` because a stateless client holds no references.
- FUSE uses `session/` and NFS uses a server because one is a `/dev/fuse` session and the other a TCP listener.
- `request/` and `procedures/` each use their own protocol's word for one call.

## 2. Findings (ranked)

### 1. NFS: the NFS program's two halves sit in two folders; regroup by layer (protocol < kernel < procedures < server)

- **What.** The plan splits one job across two folders:
  - `protocol/program.rs` (was `nfs.rs`) decodes every NFS call, answers the reading procedures
    and calls `changing::call(kernel, procedure, …)` (nfs.rs:245) for the rest.
  - `procedures/change_args.rs` (was `changing.rs`) is that other half: it decodes the changing
    procedures, calls `kernel.setattr/write/create/…` and encodes results with `wcc`.

  Meanwhile `procedures/change.rs` is `impl Kernel { create, mkdir, symlink, remove, rmdir, rename,
  link, write, setattr … }` plus `Tickets/Sattr/How`. That is the synthetic kernel's mutation work,
  not a procedure decoder. `procedures/read.rs` is `Answer/Refused/failure`, and its main user is
  `kernel.rs` (`use crate::read::{failure, read_failure, Answer, Refused}`).

  So in the plan, `protocol/` mixes wire primitives with Kernel-calling programs (`mount.rs` uses
  `kernel::Kernel` too), and `procedures/` mixes decoders with kernel internals.
- **Evidence (use lines).**
  - These four files do not name the `Kernel` type:
    - `attributes.rs`: `use crate::xdr::Writer`, plus content and workspace types.
    - `handle.rs`: std only.
    - `rpc.rs`: `xdr` only.
    - `xdr.rs`: std only.
  - Every program file does name it:
    - `nfs.rs`: `kernel::Kernel, change::Tickets, directory::list, rpc, xdr`.
    - `changing.rs`: `change::{How, Sattr, Tickets, Wcc}, kernel::Kernel, xdr`.
    - `mount.rs`: `kernel::Kernel, rpc::Accept, xdr`.
  - `change.rs` uses `kernel::{Kernel, Table}`, `block::wait` and mount-ports
    `KernelMutation`. It implements Kernel methods.
- **Fix.** One clean seam: `protocol/` never names `Kernel`. `kernel/` is the Kernel and
  everything it does on the engine mount. `procedures/` decodes a call, runs it on the Kernel and
  encodes the reply.
  - `protocol/program.rs` → `procedures/program.rs` (NFS program 100003: dispatch and the reading
    procedures).
  - `protocol/mount.rs` → `procedures/mount.rs` (MOUNT program 100005).
  - `procedures/change_args.rs` → `procedures/mutation.rs` (the changing procedures' arguments and
    results).
  - `procedures/change.rs` → `kernel/mutation.rs`. The word matches mount-ports
    `operations/mutation.rs` (`KernelMutation`, `MutationFailure`), vfs
    `operations/kernel/mutation.rs` and overlay `kernel/mutation.rs`. This also ends the
    change/changing pair the plan tried to fix with `_args`.
  - `procedures/read.rs` → `kernel/read.rs` (see finding 2 for what joins it).

  The same name at two layers (`procedures/mutation.rs` decodes; `kernel/mutation.rs` does the
  work) is deliberate. vfs, mount-ports and overlay already name this job this way.
  Public paths: `change`, `read` and `nfs` have no users outside the crate (archive grep of
  `layerfs_nfs::`).

### 2. NFS: the `kernel.rs` split as planned cannot compile as written; specify it

- **What.** The plan maps `kernel.rs` (790 LOC, 998 lines) to `kernel/references.rs` with all 790
  LOC, plus a new `kernel/descriptors.rs`. The `Kernel` and `Table` fields are private
  (`services`, `table`, `handles`, …), and today's child `kernel/released.rs` reaches them through
  `use super::{Kernel, Table}`.

  Rust does not let one sibling file reach another sibling's private fields. If `Kernel` lives in
  `references.rs`, then `descriptors.rs` and `released.rs` lose access. Neither the plan's
  declaration-only `mod.rs` nor the 200-line ceiling allows keeping it in `mod.rs`.
- **Fix.** Split by job, with the type in its own file. FUSE already uses this convention:
  `session/state.rs` and `request/state.rs` hold the folder's state type.
  - `kernel/state.rs` (new): consts, `Held`, `Table` and its name binding, `OpenFile`, the `Kernel`
    struct, `new`/builders, `encode`/`decode`, and `collectable`/`oldest`. This is today's lines
    60–300 and 963–998. Fields become `pub(super)`.
  - `kernel/references.rs`: `held_reference`, `resolve`, `request`, `entry_taken`, `renamed`,
    `forget`, `unlinked*`, `saw_directory`, `observed`, `directory_wcc` and `held_stat`, roughly
    lines 307–371 and 513–715.
  - `kernel/descriptors.rs`: `open_file`, `file_taken`, `close_file`, `open_directory`,
    `directory`, `close_directory`, `close_files` and `close_directories`, roughly lines 735–960
    without `read`.
  - `kernel/read.rs`: the moved `read.rs` (`Answer`, `Refused`, failure mapping) gains `observe`,
    `getattr`, `lookup`, `lookup_view`, `readlink`, `read` and `read_into`, roughly lines 375–512,
    716–734 and 824–866. This pairs with `kernel/mutation.rs` (finding 1), so `kernel/` holds read
    and mutation visits side by side, as mount-ports `operations/` does.
  - `kernel/mod.rs` re-exports `LOOKUPS_HELD`, `RECENT_KEPT` and `Released`. Tests use
    `layerfs_nfs::kernel::{LOOKUPS_HELD, RECENT_KEPT}`.

  The plan's `apple_double.rs` → `apple_double/companions.rs` needs the same change.
  `edit.rs`, `index.rs` and `reclaim.rs` use the private `Body`, `Orphan`, `edited_len` and
  `record` through `use super::`, so these need `pub(super)`. This is a visibility edit, not
  behaviour.

### 3. NFS: dissolve `server/`; two of its three files are not server code

- **What.**
  - `server/block.rs` (`wait`) has three callers: `change.rs:18`, `kernel.rs:40` and
    `directory.rs:26`. `server.rs` never calls it. Its doc: "Runs one port future to completion on
    the connection thread."
  - `server/refresh.rs` is entirely `impl Kernel { refresh, arm_refresh, refreshed }`. Its callers
    are `server.rs:144`/`:238`, which forward, and `change.rs:724` (`self.refreshed` in SETATTR).
  - "block" also clashes with the storage `blocks/` concept and with NFS FSSTAT block counts
    (`BLOCK_BYTES` and `DECLARED_BLOCKS` in `nfs.rs`).
- **Fix.**
  - `server/block.rs` → `kernel/wait.rs`, named after its one function.
  - `server/refresh.rs` → `kernel/refresh.rs`.
  - `server/listener.rs` goes back to the crate root as `server.rs`. This is today's name, and
    `lib.rs` already does `pub use server::{Count, ExportConfig, Refresher, Server}`.

  The planned `server/` would otherwise be a thin folder holding one file. The root becomes
  `lib.rs` plus `server.rs` and four folders (a branch with 1 loose file).

  Contrast with FUSE `session/invalidate.rs`: that one writes through the session's fuser
  `Notifier`, so it does belong to the connection. NFS has no channel; its refresh is a mark in
  the kernel table.

### 4. NFS: `names.rs` is not AppleDouble

- **What.** The planned `apple_double/names.rs` holds `silly()` (Linux and macOS `.nfs…`
  silly-rename names) and `apple_double()` (`._` names). Its doc's first bullet is silly renames.
  - `silly` is used only by `change.rs:582` and `:604`, which become `kernel/mutation.rs`.
  - `apple_double()` is used by `kernel.rs:480`, `change.rs:260` and `:273`, and five times in
    `apple_double.rs`.
  - The decision to keep these names out of the Workspace belongs to the Kernel
    (`refuses_apple_double`, `Table.silly`).
- **Fix.** `apple_double/names.rs` → `kernel/names.rs`. A reader looking for silly renames will
  not look in `apple_double/`. `apple_double/` keeps 7 files.

### 5. Tests: mount-ports `tests/` cannot form a rule-7 area; FUSE `tests/` is not one (rule 7)

- **What.** mount-ports has one test, `tests/stale.rs`, so any `tests/<area>/` would be a thin
  folder (validator: "only 1 file(s) (min 3)"). FUSE keeps 4 flat binaries (`attributes`,
  `opcode_names`, `opcode_time`, `requests`). Rule 7 asks for one binary per area.
- **Evidence for the cross-crate move (rule 24).** `stale.rs` uses `layerfs_content::ContentError`,
  `layerfs_mount_ports::ports::{stale, ServiceError}`, `layerfs_overlay::…` and
  `layerfs_workspace::…`. All four are normal dependencies of `layerfs-fuse` (its `Cargo.toml`).
  The predicate it pins is the one behind FUSE's ESTALE answer: `request/terminal.rs` doc, and the
  `failure.stale()` / `error.stale()` calls in `request/{terminal,directory,reply}.rs`.
  `tools/ci_tests.py` names no test target of either crate.
- **Fix.**
  - Create one FUSE area `tests/request/main.rs` with five topics:
    - `attributes.rs` (from `tests/attributes.rs`);
    - `opcode_names.rs` and `opcode_time.rs`;
    - `dispatch.rs` (from `tests/requests.rs`, which tests `dispatch/`'s `MountRequests`;
      `requests` beside the area `request/` would nearly repeat the folder name);
    - `stale.rs` (from `layerfs-mount-ports/tests/stale.rs`).

    The three `#![cfg(target_os = "linux")]` become `#[cfg(target_os = "linux")]` on their `mod`
    lines. `dispatch` and `stale` run everywhere.
  - mount-ports keeps no `tests/` folder.
  - Fallback if the owner wants the test in its own package: keep
    `layerfs-mount-ports/tests/stale.rs` and record an exception ("one public-API test; an area
    folder would be thin").

### 6. NFS tests: two standalone binaries belong in `tests/standalone/`; `export/export.rs` repeats its folder

- **What.**
  - The plan keeps `tests/helper_deadline.rs` (60.1 s in `ci_tests.py`) and `tests/write.rs` at the
    root, because "a folder needs at least three files". Rule 7 exempts `standalone/` from the
    3–12 rule, and the control crates already use that exemption. `write.rs` sets
    `RLIMIT_NOFILE` (`write.rs:1016–1025`), which is process-global.
  - `tests/export/export.rs` repeats its folder name (rule 14). Its doc: "A committed Branch
    served read-only over loopback NFSv3".
- **Fix.**
  - `tests/helper_deadline.rs` → `tests/standalone/helper_deadline.rs`.
  - `tests/write.rs` → `tests/standalone/write.rs`.
  - Add `[[test]]` entries with `name = "helper_deadline"` and `name = "write"`, so the
    `ci_tests.py` key `("layerfs-nfs", "helper_deadline")` still matches.
  - `tests/export/export.rs` → `tests/export/read_only.rs`.

### 7. FUSE: fold `coherence/` (3 files, 50 LOC) into `request/coherence.rs`

- **What.** The folder holds three short files:
  - `coherence/attributes.rs` (24 LOC: `Incoherent`, `published`);
  - `pages.rs` (15: `OpenCache`, `open_cache`, `written`);
  - `reply_order.rs` (11: `attempted`, `retained`).

  The rest is contract prose. The only users are in `request/`: `mutate.rs:11`, `reply.rs:200` and
  `terminal.rs:23`. Nothing outside the crate uses `layerfs_fuse::coherence`. The folder also
  gives the crate two `attributes.rs` files, at the root and in `coherence/`.
- **Fix.** Merge the three files into `request/coherence.rs`, keeping the three docs as sections of
  its header, and drop `pub mod coherence` from `lib.rs`. `request/` then holds 10 files.

### 8. FUSE: three file names should use the stack's words (rule 19)

- `request/mutate.rs` → `request/mutation.rs`. It is the same job as mount-ports
  `operations/mutation.rs` (which it drives through `KernelMutation`), vfs and overlay
  `mutation.rs`, and NFS (finding 1). The module is private.
- `session/startup.rs` → `session/attach.rs`. It holds only `KernelSession::attach` and its helper.
  Its types are `AttachFailure`, `AttachPhase` and `AttachRemainder`, and the daemon's caller is
  `workspace/kernel/attach.rs` ("One kernel Attach"). "startup" also names daemon
  `application/startup/` (process start) and overlay `database/startup.rs`. The module is private.
- `session/force.rs` → `session/forced.rs`. Its doc says "Forced teardown of one connection" and
  its types are `Forced` and `ForceRefusal`. The daemon side is `teardown/forced.rs`. The module is
  private.

Architecture docs that cite these paths need the same update:

- `core/docs/architecture/76-native-mount-session.md` (`session/startup.rs`, `force.rs`);
- `77-native-mutation-coherence.md` (`coherence/*`);
- `75-native-request-service.md` (`session/force.rs`).

### 9. NFS `examples/`: record the exception

`examples/` holds 2 Cargo example targets (`daemon_nfs`, `serve_tree`), and the validator flags it
"only 2 file(s)". Add the exception the plan already grants telemetry: "Cargo examples/ left
alone: 2 example targets (leaf under 3)".

## 3. Proposed layouts

Convention: base is the planned tree. A space marks a file that stays, including one this review
moves in, with "(from <planned path>)". `-` marks a planned folder or file this review removes,
with its LOC shown and counted in its parent's subtotal. `+` marks a path that is in neither the
plan nor today's tree. LOC is production LOC at `1317ebf7ca`. Files that do not exist yet show `—`.
Test files carry 0 production LOC.

### layerfs-nfs (src, all folders shown; tests)

```diff
  engine/crates/layerfs-nfs/src/                    [5,276]
    lib.rs                                              23   (re-exports Server, ExportConfig, Refresher, Count again)
    server.rs                                          331   (from planned server/listener.rs; today's name)
    apple_double/                                   [1,884]
      codec.rs                                         309
      companions.rs                                    614   (needs pub(super) on Body, Orphan, edited_len, record)
      edit.rs                                          375
      index.rs                                         274
      mod.rs                                             —   new
-     names.rs                                          13   (to kernel/names.rs: silly renames are not AppleDouble)
      reclaim.rs                                        99
      spill.rs                                         122
      trace.rs                                          78
    kernel/                                         [1,700]
      descriptors.rs                                     —   new (split from kernel.rs: implicit file and directory descriptors)
      mod.rs                                             —   new (re-exports LOOKUPS_HELD, RECENT_KEPT, Released)
      mutation.rs                                      683   (from planned procedures/change.rs: impl Kernel mutations, Tickets)
      names.rs                                          13   (from planned apple_double/names.rs)
      read.rs                                           52   (from planned procedures/read.rs; gains getattr/lookup/readlink/read from kernel.rs)
      references.rs                                    790   (split from kernel.rs: references, resolve, parent index)
      refresh.rs                                        66   (from planned server/refresh.rs: impl Kernel marks)
      released.rs                                       70
+     state.rs                                           —   new (split from kernel.rs: Kernel, Table, bounds, eviction)
      wait.rs                                           26   (from planned server/block.rs; its callers are kernel and directory)
    procedures/                                       [746]
      directory.rs                                     189
      mod.rs                                             —   new
      mount.rs                                          61   (from planned protocol/mount.rs: MOUNT program)
      mutation.rs                                      223   (from planned procedures/change_args.rs: changing procedures' args and results)
      program.rs                                       273   (from planned protocol/program.rs: NFS program dispatch, reading procedures)
    protocol/                                         [605]  (wire only; never names Kernel)
      attributes.rs                                    173
      handle.rs                                         49
      mod.rs                                             —   new
      rpc.rs                                           278
      xdr.rs                                           105
-   server/                                           [423]  (dissolved; 2 of 3 files were kernel code)
-     block.rs                                          26   (to kernel/wait.rs)
-     listener.rs                                      331   (to server.rs)
-     mod.rs                                             —
-     refresh.rs                                        66   (to kernel/refresh.rs)
```

(The root subtotal counts each file once, at its new place: 23 + 331 + 1,871 + 1,700 + 746 + 605 =
5,276. The `-` rows show where files came from and are not counted again.)

```diff
  engine/crates/layerfs-nfs/tests/
    apple_double/                                      [0]  (as planned: main.rs, codec, decode, view)
    export/                                            [0]
      collect.rs                                         0
      daemon.rs                                          0
      main.rs                                            —   new
      mounter.rs                                         0
      protocol.rs                                        0
      read_only.rs                                       0   (from planned export/export.rs: repeated its folder)
      refresh.rs                                         0
+   standalone/                                        [0]  ([[test]] entries; names kept for ci_tests.py)
      helper_deadline.rs                                 0   (from tests/helper_deadline.rs: 60.1 s lane rule)
      write.rs                                           0   (from tests/write.rs: sets RLIMIT_NOFILE)
    support/                                           [0]  (unchanged: client, rig, spy, tree)
```

### layerfs-fuse (src changed folders; tests)

```diff
  engine/crates/layerfs-fuse/src/                   [3,394]  (partial: dispatch/ unchanged, not shown)
    attributes.rs                                      112
    lib.rs                                              15   (drops pub mod coherence)
    mount.rs                                           211
-   coherence/                                         [53]  (dissolved into request/coherence.rs)
-     attributes.rs                                     24
-     mod.rs                                             3
-     pages.rs                                          15
-     reply_order.rs                                    11
    request/                                        [2,010]
      accounting.rs                                    225
      callbacks.rs                                     713
+     coherence.rs                                       —   new (merges coherence/{attributes,pages,reply_order}.rs, 50 LOC today)
      directory.rs                                     234
      failure.rs                                        70
      inline.rs                                         76
      mod.rs                                            14
      mutation.rs                                      251   (renamed from mutate.rs; the stack's word)
      reply.rs                                         191
      state.rs                                         187
      terminal.rs                                       49
    session/                                          [706]
      attach.rs                                        170   (renamed from startup.rs: holds KernelSession::attach)
      drain.rs                                         136
      forced.rs                                         87   (renamed from force.rs: daemon teardown/forced.rs)
      invalidate.rs                                     48
      mod.rs                                            12
      state.rs                                         253
```

```diff
  engine/crates/layerfs-fuse/tests/
-   attributes.rs                                        0   (to request/attributes.rs)
-   opcode_names.rs                                      0   (to request/opcode_names.rs)
-   opcode_time.rs                                       0   (to request/opcode_time.rs)
-   requests.rs                                          0   (to request/dispatch.rs)
+   request/                                           [0]  (one area binary)
+     attributes.rs                                      —   new (moved; linux cfg on its mod line)
+     dispatch.rs                                        —   new (from requests.rs: tests dispatch/ MountRequests)
+     main.rs                                            —   new
+     opcode_names.rs                                    —   new (moved; linux cfg)
+     opcode_time.rs                                     —   new (moved; linux cfg)
+     stale.rs                                           —   new (from layerfs-mount-ports/tests/stale.rs)
```

### layerfs-mount-ports (tests only; src unchanged)

```diff
  engine/crates/layerfs-mount-ports/
-   tests/                                             [0]  (removed: its one topic joins layerfs-fuse tests/request/)
-     stale.rs                                           0
```

## 4. Checked and left alone

- **mount-ports `src/`.**
  - `operations/` has 11 files, one per opcode family, named after the call (rule 15), with the
    same words as vfs `operations/kernel/{directory,lookup,mutation,read}` and
    `operations/namespace/{create,remove,rename}`.
  - `phase.rs` is fine.
  - `ports.rs` partly repeats the crate name. It is kept because daemon `ports.rs` implements it
    (same word, same job) and because `tools/test_check_product_boundary.py` uses that path.
- **FUSE `dispatch/` (3 files).** Its doc denies being a "daemon-wide request dispatcher". Its
  types (`MountRequests`, `DispatchError`) are what daemon `workspace/kernel/dispatcher.rs` builds
  on, so dispatch and dispatcher agree. `work.rs` (8 LOC) is acceptable as the plan's rename.
- **FUSE `request/` vs NFS `procedures/`, and FUSE `session/` vs NFS `server.rs`.** Each uses its
  protocol's own word. Forcing one word would mislabel one of them.
- **FUSE `session/invalidate.rs` vs NFS `kernel/refresh.rs`.** These are different mechanisms
  (notification vs chmod mark), so different words are right.
- **NFS Counters in `server.rs` vs FUSE `request/accounting.rs`.** These are the same idea, but the
  NFS one is about 50 LOC inside the listener. Splitting it out to match FUSE adds a file for
  little gain.
- **`kernel/descriptors.rs` vs overlay `open/`.** "descriptor" is the NFS and FUSE docs' own word
  for the implicit opens. Kept.
- **NFS `procedures/program.rs`.** Not renamed to `read.rs`, because it also dispatches NULL,
  FSSTAT, FSINFO and PATHCONF.
- **Daemon `workspace/kernel/` (out of scope).** It is FUSE-only by `use layerfs_fuse` in
  `attach.rs`, `dispatcher.rs` and `invalidate.rs`, while `state.rs` is route-neutral (the
  "kernel half" of an entry for both routes). So "kernel" there means the FUSE route plus shared
  state. No change proposed here; review2 already weighed `fuse/`.
- **Behaviour note, not layout.** NFS `read.rs` has its own private `stale()`, which walks the
  whole error source chain. mount-ports `ports::stale` (pinned by `stale.rs`) deliberately does
  not count "a wrapper that merely carries one". This is worth a separate look; nothing is moved
  for it.
- **Depth and counts after the changes.**
  - NFS: `apple_double/` 7, `kernel/` 9, `procedures/` 4, `protocol/` 4. The root is a branch with
    1 loose file.
  - FUSE: `dispatch/` 3, `request/` 10, `session/` 5. The root is a branch with 2 loose files.
  - All folders are at depth 2 or less.

## 5. Machine-readable moves

```json
[
 {"from": "engine/crates/layerfs-nfs/src/protocol/program.rs", "to": "engine/crates/layerfs-nfs/src/procedures/program.rs", "op": "move", "why": "NFS program dispatch names Kernel; pairs with the changing half"},
 {"from": "engine/crates/layerfs-nfs/src/protocol/mount.rs", "to": "engine/crates/layerfs-nfs/src/procedures/mount.rs", "op": "move", "why": "MOUNT program uses Kernel; protocol/ is wire only"},
 {"from": "engine/crates/layerfs-nfs/src/procedures/change_args.rs", "to": "engine/crates/layerfs-nfs/src/procedures/mutation.rs", "op": "rename", "why": "changing procedures' args/results; stack word is mutation"},
 {"from": "engine/crates/layerfs-nfs/src/procedures/change.rs", "to": "engine/crates/layerfs-nfs/src/kernel/mutation.rs", "op": "move", "why": "impl Kernel mutations and Tickets; matches mount-ports operations/mutation.rs"},
 {"from": "engine/crates/layerfs-nfs/src/procedures/read.rs", "to": "engine/crates/layerfs-nfs/src/kernel/read.rs", "op": "move", "why": "Answer/Refused are the kernel read visit's outcome; kernel.rs is its main user"},
 {"from": "engine/crates/layerfs-nfs/src/apple_double/names.rs", "to": "engine/crates/layerfs-nfs/src/kernel/names.rs", "op": "move", "why": "holds silly-rename names too; the kernel keeps both out"},
 {"from": "engine/crates/layerfs-nfs/src/server/block.rs", "to": "engine/crates/layerfs-nfs/src/kernel/wait.rs", "op": "rename", "why": "callers are kernel, mutation, directory; 'block' clashes with storage blocks"},
 {"from": "engine/crates/layerfs-nfs/src/server/refresh.rs", "to": "engine/crates/layerfs-nfs/src/kernel/refresh.rs", "op": "move", "why": "impl Kernel marks; also used by SETATTR"},
 {"from": "engine/crates/layerfs-nfs/src/server/listener.rs", "to": "engine/crates/layerfs-nfs/src/server.rs", "op": "move", "why": "server/ would hold one file"},
 {"from": "engine/crates/layerfs-nfs/src/server/mod.rs", "to": "engine/crates/layerfs-nfs/src/lib.rs", "op": "merge", "why": "server/ dissolved; re-exports return to lib.rs"},
 {"from": "engine/crates/layerfs-nfs/src/kernel/references.rs", "to": "engine/crates/layerfs-nfs/src/kernel/references.rs", "op": "split", "why": "kernel.rs split by job: state, references, descriptors, read visits"},
 {"from": "", "to": "engine/crates/layerfs-nfs/src/kernel/state.rs", "op": "new", "why": "Kernel and Table types with pub(super) fields; siblings cannot reach private fields"},
 {"from": "engine/crates/layerfs-nfs/tests/export/export.rs", "to": "engine/crates/layerfs-nfs/tests/export/read_only.rs", "op": "rename", "why": "file repeated its folder; doc: read-only export"},
 {"from": "engine/crates/layerfs-nfs/tests/helper_deadline.rs", "to": "engine/crates/layerfs-nfs/tests/standalone/helper_deadline.rs", "op": "move", "why": "rule 7 standalone (60.1 s lane rule); [[test]] name kept"},
 {"from": "engine/crates/layerfs-nfs/tests/write.rs", "to": "engine/crates/layerfs-nfs/tests/standalone/write.rs", "op": "move", "why": "rule 7 standalone (sets RLIMIT_NOFILE); [[test]] entry"},
 {"from": "engine/crates/layerfs-fuse/src/coherence/attributes.rs", "to": "engine/crates/layerfs-fuse/src/request/coherence.rs", "op": "merge", "why": "50-LOC folder used only by request/"},
 {"from": "engine/crates/layerfs-fuse/src/coherence/pages.rs", "to": "engine/crates/layerfs-fuse/src/request/coherence.rs", "op": "merge", "why": "50-LOC folder used only by request/"},
 {"from": "engine/crates/layerfs-fuse/src/coherence/reply_order.rs", "to": "engine/crates/layerfs-fuse/src/request/coherence.rs", "op": "merge", "why": "50-LOC folder used only by request/"},
 {"from": "engine/crates/layerfs-fuse/src/coherence/mod.rs", "to": "engine/crates/layerfs-fuse/src/request/mod.rs", "op": "merge", "why": "coherence/ dissolved"},
 {"from": "engine/crates/layerfs-fuse/src/request/mutate.rs", "to": "engine/crates/layerfs-fuse/src/request/mutation.rs", "op": "rename", "why": "same word as mount-ports/vfs/overlay/NFS mutation.rs"},
 {"from": "engine/crates/layerfs-fuse/src/session/startup.rs", "to": "engine/crates/layerfs-fuse/src/session/attach.rs", "op": "rename", "why": "holds KernelSession::attach; daemon kernel/attach.rs; startup means process start elsewhere"},
 {"from": "engine/crates/layerfs-fuse/src/session/force.rs", "to": "engine/crates/layerfs-fuse/src/session/forced.rs", "op": "rename", "why": "forced teardown; daemon teardown/forced.rs"},
 {"from": "engine/crates/layerfs-fuse/tests/attributes.rs", "to": "engine/crates/layerfs-fuse/tests/request/attributes.rs", "op": "move", "why": "rule 7 area binary"},
 {"from": "engine/crates/layerfs-fuse/tests/opcode_names.rs", "to": "engine/crates/layerfs-fuse/tests/request/opcode_names.rs", "op": "move", "why": "rule 7 area binary"},
 {"from": "engine/crates/layerfs-fuse/tests/opcode_time.rs", "to": "engine/crates/layerfs-fuse/tests/request/opcode_time.rs", "op": "move", "why": "rule 7 area binary"},
 {"from": "engine/crates/layerfs-fuse/tests/requests.rs", "to": "engine/crates/layerfs-fuse/tests/request/dispatch.rs", "op": "rename", "why": "rule 7 area; tests dispatch/ MountRequests"},
 {"from": "", "to": "engine/crates/layerfs-fuse/tests/request/main.rs", "op": "new", "why": "area binary root: 5 topic modules, linux cfg on 3 mod lines"},
 {"from": "engine/crates/layerfs-mount-ports/tests/stale.rs", "to": "engine/crates/layerfs-fuse/tests/request/stale.rs", "op": "move", "why": "one-file tests/ cannot form an area; all its deps are fuse deps; pins FUSE ESTALE predicate"}
]
```
