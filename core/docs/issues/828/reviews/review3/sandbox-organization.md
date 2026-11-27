# `layerfs-sandbox` organization review (#828 planned layout)

Scope: the planned `control/crates/layerfs-sandbox/src/` (rows of
`nav/site/layout.json` whose `n` is under it, 150 rows), checked against the
old sources in `v2tree/control/crates/{layerfs-sandbox,layerfs-sandbox-backend}`
(`//!` headers, `crate::`/`super::` lines, and cross-crate `layerfs_sandbox::`
users). LOC are production LOC from `prod_loc_files.txt`, the same values as
the plan. Planning only; nothing in the repository was touched.

## 1. Verdict

The plan is mostly sound. The skeleton (`status/`, `lifecycle/`, `record/`,
`route/`, `backend/`), the parts only a Sandbox has (`image/`, `exec/`), the
Cargo `bin/` and the two backend support folders (`process/`, `runtime/`) are
a short row of distinct roles. All leaves hold 3 to 12 files, branches hold at
most 4 loose files, and nothing goes deeper than 4 levels. `backend/native` and
`backend/namespaces` share one design and differ only in ways one line can
explain.

Four placements are still wrong, and the `use` lines prove each one:

- `runtime/sandbox.rs` is not one of the backend-neutral types. It undoes the
  §5 statement that `runtime/` holds only those.
- `process/confined.rs`, together with the `os` alias in `process/mod.rs`, is
  the per-OS read contract. It is not code for the daemon process.
- `status/ask.rs` and `status/owner.rs` are the inspect route, not reads.
- `status/live.rs` has no user left in the crate.

`route/daemon/channel/capacity.rs` is a smaller case of the same problem: it
is a read model placed in a transport folder.

## 2. Findings (ranked by value)

### F1. `runtime/sandbox.rs` → `backend/select.rs` (rules 10, 20, 23; §5 "`runtime/` keeps the backend-neutral command types")

Evidence (old `layerfs-sandbox/src/runtime.rs`, 195 LOC):

- Header: "A Sandbox's runtime: `LAYERFS_SANDBOX=native` on macOS … or
  `LAYERFS_SANDBOX=namespaces` on Linux". `backend()` says: "This is the one
  place the setting is validated."
- Uses: `crate::host::Deploy`, `crate::output::{Place, Spool}`,
  `crate::record::DaemonRecord`, `crate::daemon::values`,
  `crate::image_artifact::unpacked` and `crate::state::remove_fix`. It also
  uses `layerfs_sandbox_backend::{CommandIdentity, ExecSlot, Sandbox}`, where
  `Sandbox` is the enum in the planned `backend/dispatch.rs`.
- It declares a second `enum Backend { HostProcess, Namespaces }`, the backend
  names (`NATIVE`, `NAMESPACES`, `LOCAL`) and `PLATFORMS`.

The other eleven files in `runtime/` come from the old `layerfs-sandbox-backend`
crate, so they cannot depend on anything above it. This one file depends on
`route/host`, `record/`, `exec/output`, `route/daemon` and `image/`, and on
`backend/dispatch.rs`. Meanwhile `backend/` itself depends on `runtime/` types.
Leaving it in `runtime/` therefore gives the two folders a dependency cycle,
and the bottom folder of the crate starts reaching into the top.

What the file does is choose which backend variant a deploy names, and name
and describe a running variant (`kind`, `image`, `network`, `scratch`,
`runtime_name`). That is backend contract work, beside `adapter.rs` (stored
name, capabilities) and `dispatch.rs` (the match over variants).

**Fix:** move it to `backend/select.rs` (`select` for "the backend a deploy
selects"; `choose` would clash with `deploy::choose`). `runtime/` then holds
only the 11 neutral types, as §5 already says.

Follow-up (a code question, not layout): this `Backend` enum, machine
`table/backend.rs` (old `backend/kind.rs`) and `backend/adapter.rs` all name the
backends. Rule 23 suggests one owner for the names.

### F2. `process/confined.rs` plus the `os` alias and `mounts_under` → `backend/` (rules 11, 9; §5 lists `confined` in `process/`, so this reopens it on a concrete defect)

Evidence:

- `shared/confined.rs` header: "Reads of a folder tree that stay inside it
  (layerfs-project#178 security rules 1 to 3 and 8) … An inspection read uses
  nothing else of the filesystem, so what a platform must supply is exactly
  these three". Nothing in it starts, watches or stops a process.
- Its users: `inspect/files.rs` and `inspect/root.rs`, which move to
  `layerfs-inspect` as `files/walk.rs` and `files/root_view.rs`. The OS halves
  also use it: `backend/namespaces/linux/confined.rs` and
  `backend/native/macos/confined.rs` both have
  `use crate::shared::confined::{Kind, Refused, Stat, View};`.
- In `shared/mod.rs`, the `pub(crate) use crate::backend::namespaces::linux as os`
  (and `…native::macos as os`) alias is used only by `confined.rs` and by
  `mounts_under`. `daemon.rs`, `listener.rs`, `search_path.rs` and `streams.rs`
  never use `os`. `mounts_under` is used only by `sweep.rs`
  (`layerfs_sandbox_backend::shared::mounts_under`).

`process/` therefore mixes two roles: the local daemon process (spawn,
listener line, search path, streams) and the contract between the
backend-neutral code and each OS. The second is the same pattern as
`adapter.rs`: a neutral contract at `backend/`, with a half in each variant
folder.

**Fix:** move `process/confined.rs` to `backend/confined.rs`, and move the `os`
alias and `mounts_under` (a one-line delegation) to `backend/mod.rs`.
`process/` keeps `daemon`, `listener`, `search_path` and `streams` (4 files),
and the name `process/` then describes everything in it. `backend/` gets 4
loose files (`adapter`, `confined`, `dispatch`, `select`), which is the rule-3
limit.

### F3. `status/ask.rs` and `status/owner.rs` → `route/inspect.rs` (rule 20 `route/`; rule 11)

Evidence:

- `inspect/route.rs` (planned `status/ask.rs`) header: "The inspect route:
  where a request runs ([`ask`]), the worker's side of it ([`in_worker`]), the
  one match on its kind ([`serve`]) and how a side finds a Sandbox's run
  folder". `ask` picks `Executor::Local` or `Executor::Worker` through
  `on_owner`, which is exactly what `route/host/send.rs` does for host
  operations.
- `inspect/owner.rs` header: "Who runs an inspect request, and how much of the
  process it may hold". It holds `PERMITS`, `Budget`, `admit` and `on_owner`,
  so it is admission, not a read.

Neither file answers how a Sandbox is. Rule 20 defines `route/` as "how a call
reaches it (worker client, host dispatch, daemon channel)".

There is also a dependency problem the move makes visible. `serve` matches
`log`, `files`, `machine`, `stats` and `space`, which all move up to
`layerfs-inspect` (the plan's new `layerfs-inspect/src/serve.rs`).
`in_worker` calls `serve`, so it goes up with it. What stays in the Sandbox
crate is `ask` and `run_folder`, which `status/overlay.rs` needs for
`overlay_sizes` (used by `route/direct.rs:331`). Because `ask`'s `Local` branch
calls `serve`, the server function has to be passed in. That is the same
pattern as `route::Local` and the `WorkspaceDrain` trait in
`review-control-v2.md`.

**Fix:** merge the remainder of `ask.rs` with `owner.rs` into one
`route/inspect.rs` (about 163 LOC at most): where an inspect request runs and
how much it may hold. Two separate files would need a 2-file folder, and
rule 4 forbids that. `route/` becomes `direct.rs`, `inspect.rs`, `daemon/`
and `host/`, and `status/` is left with reads only.

### F4. `status/live.rs` → `layerfs-inspect/src/machine/live.rs` (rules 22, 23, 24)

Evidence:

- Header: "What the inspect kinds share (layerfs-project#164): the lists of
  names a request holds, and [`checked`] … for `machine_stats`".
- Every item is `pub(super)` (`MOST`, `DEADLINE`, `listed`, `names`,
  `Checked`, `checked`). The only users are `inspect/stats.rs`
  (`use super::live::{self, Checked, DEADLINE}`) and `inspect/space.rs`
  (`live::listed`, `live::names`). Both move to `layerfs-inspect` as
  `machine/stats.rs` and `machine/space.rs`.
- It uses only `crate::record` and `super::process`. Both are public from
  `layerfs-sandbox` (`record`, `status::process`), so `layerfs-inspect`, which
  sits above `layerfs-sandbox`, can still reach them.

Leaving it in the Sandbox crate keeps request-parsing helpers in a crate where
nothing calls them.

**Fix:** move it next to its users. Inspect `machine/` grows from 7 to 8
files.

### F5. `route/daemon/channel/capacity.rs` → `status/capacity.rs` (rule 20 `status/`: "live probes, its own stats")

Evidence:

- Header: "A running Sandbox daemon's control-capacity read model (#540) …
  The doctor explains it and `sandbox.get` shows it as `daemon_capacity`."
- It uses `crate::direct`, `crate::record::SandboxRecord`, `layerfs_home::secrets`
  and `layerfs_home::home::paths`, besides the channel's `fence`, `operator`
  and `pool`.

The other `channel/` files (`exchange`, `fence`, `internal`, `operator`,
`pool`) are the transport and depend on nothing above it. `capacity.rs` is a
status read that happens to use that transport. The record and route
dependencies above show it belongs higher than the transport.

**Fix:** move it to `status/capacity.rs`. `channel/` keeps 5 files and
`status/` ends with 9 (after F3 and F4). This is lower value than F1 to F4; it
is fine to skip it if the owner prefers to keep everything that opens a daemon
connection in one place.

### F6. `runtime/cancel.rs` → `runtime/stop.rs` (rule 13)

The crate would have `runtime/cancel.rs` ("How one command is stopped … the
backend fills its stop handle in at spawn ([`Stop`] …)") and `exec/cancel.rs`
(settle/stale of command records). `layerfs-workspace` adds a third
`exec/cancel.rs` for the `workspace.cancel` call. The runtime file defines the
`Stop` handle, so `stop.rs` names its own concept. This only renames the file
inside its folder.

## 3. Proposed layout

Scope: the planned `layerfs-sandbox/src` from `nav/site/layout.json`. Per-file
production LOC come from `prod_loc_files.txt`, the same counter and values the
plan uses. Per AGENTS.md, a bracketed number sums every file listed beneath
it, `-` rows included. A file moved inside the crate therefore counts at both
its old (`-`) and new (space) path, and the root bracket is not the crate's
size. The planned crate is 22,635 LOC; after this proposal it is 22,571
(`status/live.rs`, 64 LOC, leaves for `layerfs-inspect`). Renames inside the
same folder show only the new name. Tests are not shown.

```diff
  layerfs-sandbox/src/                    [23,171]  (listed rows; crate after = 22,571)
    deploy.rs                                  111
    family.rs                                   27
    lib.rs                                     105
    backend/                               [6,553]
      adapter.rs                                13
      confined.rs                              251   (from process/confined.rs: the per-OS read contract, F2)
      dispatch.rs                              270
      mod.rs                                     6   (gains the `os` alias and `mounts_under` from process/mod.rs, F2)
      select.rs                                195   (from runtime/sandbox.rs: the backend a deploy selects, F1)
      namespaces/                          [4,829]
        adapter.rs                              23
        mod.rs                                  38
        provider.rs                            198
        seccomp.rs                             205
        instance/                          [1,374]
          cgroup.rs                            193
          create.rs                            354
          exec.rs                              237
          handle.rs                            293
          mod.rs                                 —   new
          paths.rs                              40
          spec.rs                              257
        linux/                               [291]
          confined.rs                          117
          host_checks.rs                        81
          mod.rs                                 2
          mounts.rs                             91
        network/                           [1,247]
          links.rs                             237
          lock.rs                               35
          mod.rs                                26
          netlink.rs                           223
          nftables.rs                          357
          pool.rs                              152
          resolver.rs                           84
          setup.rs                             133
        rootfs/                            [1,453]
          cache.rs                             245
          docker_archive.rs                    545
          layers.rs                            236
          mod.rs                                12
          root.rs                               22
          tar.rs                               393
      native/                                [989]
        adapter.rs                              23
        mod.rs                                  16
        provider.rs                            123
        instance/                            [597]
          exec.rs                              160
          groups.rs                            145
          handle.rs                            292
          mod.rs                                 —   new
        macos/                               [230]
          confined.rs                           65
          mod.rs                                 4
          mount_table.rs                       136
          mounts.rs                             25
    bin/                                     [831]
      layerfs-sandbox-helper/                [831]
        exec.rs                                289
        init.rs                                162
        main.rs                                 48
        netns.rs                                56
        root.rs                                276
    exec/                                    [958]
      cancel.rs                                304   (planned split: workspace.cancel leaves)
      id.rs                                     32
      mod.rs                                     7
      output.rs                                210
      pending.rs                                26
      record.rs                                342
      remote.rs                                 37
    image/                                 [3,659]
      artifact.rs                              536
      machine.rs                               733
      mod.rs                                     —   new
      copy/                                [1,325]
        docker.rs                              349
        load.rs                                711
        lock.rs                                148
        mod.rs                                   —   new
        sweep.rs                               117
      tar/                                 [1,065]
        filter.rs                              302
        header.rs                               75
        mod.rs                                   6
        pax.rs                                 419
        sparse.rs                              263
    lifecycle/                             [2,493]
      connection.rs                            103
      create.rs                                235
      force.rs                                  98
      force_stop.rs                              —   new
      launch.rs                                346
      local.rs                                 448
      machine_side.rs                          431
      mod.rs                                    17
      mount_intents.rs                         147
      network.rs                                18
      registry.rs                              110
      remove.rs                                449
      startup.rs                                91
    process/                                 [603]  (after 352)
-     confined.rs                              251   (to backend/confined.rs, F2)
      daemon.rs                                243
      listener.rs                               56
      mod.rs                                    17   (loses the `os` alias and `mounts_under`)
      search_path.rs                            13
      streams.rs                                23
    record/                                [2,370]
      daemon.rs                                389
      discovery.rs                             331
      mod.rs                                    10
      reopen.rs                                328
      sandbox.rs                               434
      sweep.rs                                 344
      tombstone.rs                              88
      unmount_guard.rs                         324
      workspace.rs                             122
    route/                                 [3,158]  (after 3,231)
      direct.rs                                267
+     inspect.rs                                 —   new (merge of status/ask.rs remainder + status/owner.rs, F3)
      mod.rs                                     —   new
      daemon/                              [1,626]
        call.rs                                 59
        failure.rs                              78
        mod.rs                                  20
        refusal.rs                             313   (planned split: Workspace answer values leave)
        channel/                             [867]
-         capacity.rs                           90   (to status/capacity.rs, F5)
          exchange.rs                          282
          fence.rs                              55
          internal.rs                           69
          mod.rs                                 —   new
          operator.rs                           20
          pool.rs                              351
        workspace/                           [289]
          commit.rs                             42
          mod.rs                                10
          mount.rs                              89
          recovery.rs                           82
          status.rs                             15
          unmount.rs                            51
      host/                                [1,265]
        call.rs                                208
        dispatch.rs                            109
        frame.rs                               357
        mod.rs                                   4
        send.rs                                587
    runtime/                                 [806]  (after 611; backend-neutral types only)
      bind.rs                                   22
      capabilities.rs                           30
      execution.rs                              17
      failure.rs                                83
      identity.rs                               54
      image.rs                                  25
      mod.rs                                    30
      options.rs                               112
      request.rs                                21
-     sandbox.rs                               195   (to backend/select.rs, F1)
      stop.rs                                  120   (was cancel.rs: names the Stop handle, F6)
      trace.rs                                  80
      transfer.rs                               17
    status/                                [1,497]  (after 1,270: reads only)
-     ask.rs                                    72   (serve/in_worker to layerfs-inspect serve.rs as planned; rest to route/inspect.rs, F3)
      capacity.rs                               90   (from route/daemon/channel/capacity.rs, F5)
      get.rs                                   203
      inventory.rs                             306
      list.rs                                  165
-     live.rs                                   64   (to layerfs-inspect machine/live.rs, F4)
      mod.rs                                     —   new
      overlay.rs                                46
-     owner.rs                                  91   (to route/inspect.rs, F3)
      process.rs                               227
      run_folder.rs                            115
      state.rs                                 118
      users.rs                                   —   new
```

Counts after the change. Leaves: `process/` 4, `runtime/` 11, `status/` 9,
`route/daemon/channel/` 5, `lifecycle/` 12, `record/` 8, `exec/` 6.
`backend/` has 4 loose files and 2 variant folders; `route/` has 2 loose
files and 2 folders. Depth is still 4 at most (`route/daemon/channel/*.rs`,
`backend/*/instance/*.rs`). The root keeps 10 folders and 2 loose files plus
`lib.rs`.

## 4. Deliberately left as is

- **The `running/` dissolution.** I checked where each file lands:
  - `process/` fits once `confined.rs` and the `os` facade leave (F2).
  - `record/discovery.rs` is the Sandbox's on-disk record files. The name
    matches store `project/discovery.rs` (rule 19).
  - `record/reopen.rs` builds a backend owner from a record alone.
  - `record/unmount_guard.rs` is a record file (`workspace/<id>.unmount`). It
    must stay in this crate: `route/host/dispatch.rs`, `route/direct.rs`,
    `lifecycle/registry.rs` and `lifecycle/machine_side.rs` call it.
  - `record/sweep.rs` (run folders no record names) is the weakest fit, but it
    is defined by the records, and `lifecycle/` is already at 12.
  - `lifecycle/registry.rs` is an in-memory table rather than a call. Only
    lifecycle creates and ends what it holds, and §5 settles it. No defect
    found.
- **`lifecycle/` at 12 (the ceiling).** It holds three roles: the calls, the
  machine-side half and the launch pipeline. No split meets the rules: a
  `lifecycle/launch/` leaf of 5 would leave 7 loose files, over the branch
  limit of 4. If one more file arrives, split the launch pipeline and the
  close path together. `machine_side.rs` names where the code runs rather than
  what it does (rule 16), but no better plain noun covers start, create,
  close and reclaim. The `WorkspaceDrain` trait that replaces
  `crate::workspace::serve::unmount` (`lifecycle.rs:399`) is declared in this
  file; that is already settled in `review-control-v2.md`.
- **`route/daemon/workspace/` stays in this crate.** `registry.rs` and
  `files.rs` hold `MountedWorkspace` and `BoundWorkspace`, and the inherent
  `impl WorkspaceApi` must live in the crate that defines it.
- **`exec/` stays here, including `output.rs`.** `route/host/send.rs`
  (`output::page`, `release`) and `backend/select.rs` (`Place`, `Spool`) use
  it, besides the Workspace crate.
- **`image/` and `backend/namespaces/rootfs/`.** `image/artifact.rs`,
  `image/machine.rs` and `image/copy/sweep.rs` use
  `layerfs_sandbox_backend::rootfs` for the same on-disk cache
  (`<root>/images/<hex>.2`). That is a correct dependency: the image copy
  feeds the namespaces backend's cache. `rootfs/` is settled inside
  `backend/namespaces/`, and the two tar readers are already logged as a code
  question in `review2/control.md`. The `artifact.rs`/`machine.rs` naming
  questions from that review are still open; nothing new here.
- **The `backend/native` and `backend/namespaces` designs.** They share
  `adapter`, `provider`, `instance/{exec,handle}` and an OS folder.
  `namespaces` alone has `seccomp`, `network/` and `rootfs/` because it
  isolates, and `cgroup` ↔ `groups` is the same job done two ways. Both OS
  folders have `confined.rs` and `mounts.rs`; macOS's extra `mount_table.rs`
  wraps the `mount`/`umount -f` tools, which Linux replaces with `/proc`.
- **The two routes, `route/host/` and `route/daemon/`.** Both have `call.rs`
  for the typed call. `daemon/` needs `channel/` and `workspace/` because it
  owns a connection pool and the Workspace calls; flattening it would mix 8
  loose files with one subfolder (rule 3).
- **The root files `deploy.rs` and `family.rs`.** `deploy.rs` (`Side`,
  `choose`, `resolve`) is used by `route/host`, `image/`, the inspect route
  and backend selection, which is too many roles for any one folder.
  `family.rs` is the public call facade.
- **Root row of 10 folders.** These are the 5 skeleton folders, the 2
  Sandbox-only parts, the 2 backend support folders that rule 10 keeps out of
  the variant row, and Cargo `bin/`. They are clearly different roles, not a
  mixed row.
