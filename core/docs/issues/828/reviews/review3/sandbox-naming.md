# layerfs-sandbox naming review (#828 plan)

Scope: every planned folder and file under `control/crates/layerfs-sandbox/src/`
in `nav/site/layout.json`. Each name was checked against the file's `//!` header
and public items in the old source (`v2tree/control/crates/layerfs-sandbox{,-backend}/src`,
`layerfs-control/src/inventory.rs`, `machine_force.rs`). Cross-crate checks used
`nav/trees/tree-{control,api}.txt`. Planning only; nothing in the repository was touched.

## 1. Verdict

The top-level folders hold up: `backend/`, `exec/`, `image/`, `lifecycle/`, `process/`,
`record/`, `route/`, `runtime/` and `status/` are all settled or clearly fine. The
problems are file names, and they cluster in `lifecycle/` and `status/`:

- In `lifecycle/`, `force.rs` and `force_stop.rs` sit side by side. One is a forced
  `sandbox.remove`. The other is the Sandbox side of `machine.stop --force`. Both
  names suggest a Sandbox stop call, which does not exist.
- Also in `lifecycle/`, `machine_side.rs`, `local.rs`, `startup.rs`, `launch.rs`,
  `create.rs` and `connection.rs` are six names for one start pipeline plus one
  call. From the names alone, a reader cannot tell which file starts what.
- `exec/cancel.rs` keeps the name of a call it no longer holds once
  `workspace.cancel` moves to layerfs-workspace. That leaves three `cancel.rs` files
  with three different jobs: `runtime/`, `exec/`, and workspace `exec/`.
- Three `status/` files are named for plumbing or clash with crate words: `ask.rs`
  (the inspect route), `owner.rs` (inspect admission) and `process.rs` (daemon stats,
  next to a `process/` folder).
- `image/artifact.rs` and `image/machine.rs` do not describe their contents, and
  both collide with other concepts.

## 2. Proposed renames (ranked by value)

| # | Planned path | Proposed path | Reason (evidence) |
|---|---|---|---|
| 1 | `lifecycle/force.rs` | `lifecycle/force_remove.rs` | Its main item is `close`: "Forced `sandbox.remove` of the Sandbox `host` names, on this process's kernel". Next to `force_stop.rs`, a reader cannot tell the two apart. This name matches machine's `lifecycle/force_stop.rs` (call plus flag). |
| 2 | `lifecycle/force_stop.rs` (new, split from `machine_force.rs`) | `lifecycle/machine_stop.rs` | The Sandbox side of `machine.stop --force` (native runtimes stopped, commands settled, Workspaces reported `unknown`). There is no `sandbox.stop` call, and the planned name copies machine's `lifecycle/force_stop.rs`, which is the call itself. Rule 15: name it after the call it serves. |
| 3 | `lifecycle/machine_side.rs` | `lifecycle/serve.rs` | Header: "Named Sandboxes on the machine that runs them". It holds `setup`/`start`/`create`/`close`/`reclaim`, which `route/host/dispatch.rs` runs. This is the executing side of Sandbox host operations, and layerfs-workspace names its own counterpart `host/serve.rs`. `machine_` wrongly suggests the machine entity and would sit next to #2. |
| 4 | `exec/cancel.rs` (after split) | `exec/settle.rs` | After the split, `workspace.cancel` (`cancel`, `call`, `every`, `listed_answer`) moves to `layerfs-workspace/src/exec/cancel.rs`. What stays is `settle`/`stale`/`claim`/`stale_remote`/`unreached`: what remove, unmount and force do with running commands. Rule 15 keeps `cancel.rs` for the call. This also removes the second `cancel.rs` in this crate (`runtime/cancel.rs` is the stop handle). |
| 5 | `status/ask.rs` | `route/inspect.rs` | Header: "The inspect route: where a request runs ([`ask`])". `on_owner` decides between this process and the machine's worker. Per rule 20, that belongs in `route/` (how a call reaches it), not `status/`. "ask" is a verb that fits every call. |
| 6 | `status/owner.rs` | `status/admission.rs` | Header: "how much of the process it may hold… refused `Busy` at once, which is admission". Items are `PERMITS`, `SOFT`, `HARD`, `Budget` and `admit`. "owner" already means an op intent's owner process (home `op/`, exec record `owner`). |
| 7 | `status/process.rs` | `status/stats.rs` | It holds `live`/`shown`/`usage` (the daemon process's liveness, start time and cgroup usage). The plan's own note says "sandbox.get --with stats". Store `admin/status/stats.rs` and project `status/stats.rs` use `stats.rs` for the same idea. It also stops clashing with the crate's `process/` folder. |
| 8 | `image/artifact.rs` | `image/cache.rs` | The header's first words are "A machine's image cache, on the side that runs its Sandboxes": staged parts, unpack, sidecars. "artifact" says nothing and collides with machine `setup/artifacts.rs` (bundle artifacts). It also lines up with `backend/namespaces/rootfs/cache.rs`, which manages the same `<root>/images/<hex>.2` folder. |
| 9 | `lifecycle/local.rs` | `lifecycle/daemon_start.rs` | Main item is `start`: "Starts the Sandbox the call names as a native (macOS) or namespaces (Linux) daemon". Every remaining backend is "local", so `local` is a word that fits everything here. |
| 10 | `lifecycle/startup.rs` | `lifecycle/launch_phase.rs` | It holds `SandboxPhase` and `SandboxCause` (where a launch stopped and why), plus `hello` and `SandboxApi`. "startup" is a synonym of launch and create, and `connection.rs` defines a `struct Startup`, so the planned name points readers to the wrong file. Keep `connection.rs`: a third `launch_*` file would force a folder (rule 2). |
| 11 | `image/machine.rs` | split into `image/list.rs` + `image/prune.rs` | Header: "A machine's Sandbox images (`machine.images`, `machine.prune`)". The file holds two calls and their worker sides (`list`/`in_worker`, `prune`/`prune_in_worker`), in two clean halves (lines 69–445 and 446 onward). Rule 15 wants one call per file. "machine" collides with the machine entity. If a split is not wanted, keep the file whole; no single noun is clearly better. |
| 12 | `process/confined.rs` | `backend/confined.rs` | This reopens the 2026-11-27 decision because of a concrete defect. The file has nothing to do with the daemon process: "Reads of a folder tree that stay inside it… An inspection read uses nothing else". Like `backend/dispatch.rs`, it is the neutral front over the OS files `backend/namespaces/linux/confined.rs` and `backend/native/macos/confined.rs`. With it, `backend/` has 3 loose files (rule 3 allows 4), and `process/` keeps 4 files. |
| 13 | `exec/record.rs` | `record/exec.rs` | The crate has a `record/` folder for its records, which already includes `workspace.rs` for `workspace/<id>.json`. The exec record is the sibling file `workspace/<id>/exec/<exec id>.json`. Two places named "record" break "one concept, one place". A move inside the crate needs only the naming rules (rule 24). `exec/` keeps 5 files. |
| 14 | `route/daemon/channel/operator.rs` | `route/daemon/operator.rs` | Header: "Daemon backend method admission". It is the allowlist of operator methods, and every branch calls `call::exchange_call`. It belongs beside `route/daemon/call.rs` ("One daemon operator call"), not with the connection plumbing in `channel/`. `route/daemon/` then has 4 loose files. |
| 15 | `lifecycle/network.rs` | `lifecycle/firewall.rs` | It holds `serving_on`, `closed_ports` and `worker_start`: the worker port that the firewall table closes to commands. `network.rs` collides with `backend/namespaces/network/`, which is the Sandbox's actual network. |
| 16 | `backend/namespaces/instance/create.rs` | `backend/namespaces/instance/start.rs` | Header: "One namespace Sandbox start". `create.rs` reads as the `sandbox.create` call (`lifecycle/create.rs`, rule 15). |
| 17 | `backend/namespaces/linux/host_checks.rs` | `backend/namespaces/linux/checks.rs` | It holds the `require_*` checks plus `cgroup_of`. The folder already says the OS, and "host" is a LayerFS word that means something else here: host operations in `route/host/`, and the host as opposed to a machine. |
| 18 | `exec/pending.rs` | `exec/pending_cancel.rs` | Header: "A cancel received before registration" (`mark`/`consume`). `pending` alone reads as a pending exec. |
| 19 | `runtime/execution.rs` | `runtime/command_identity.rs` | Its only item is `CommandIdentity` (nonroot uid:gid). The current header ("Process observation is independent of pipe EOF…") is stale, and the name gives no hint of the content. |
| 20 | `runtime/options.rs` | `runtime/launch.rs` | Header: "Launch inputs and observations": `LaunchRequest`, `LaunchPhase`, `ProviderLaunchOptions`, and the reopen records. "options" fits anything. |
| 21 | `runtime/transfer.rs` | `runtime/pending_request.rs` | Its only item is `PendingRequest` (undelivered request bytes). "transfer" suggests an image or file copy. |

Rows 1–8 are the ones worth taking. Rows 9–14 are clear gains of moderate size, and
rows 15–21 are small polish.

## 3. Names checked and judged fine

- **Root.**
  - `deploy.rs`: matches the `Deploy` field and type; `choose`/`resolve`.
  - `family.rs`: matches the api `families/` word; the lookups `sandbox_of`/`named`. Its header is stale: it still describes the `workspace` family and `direct.rs`/`route.rs`.
  - `lib.rs`.
  - `bin/layerfs-sandbox-helper/{main,init,exec,netns,root}.rs`: the helper's stages.
- **`backend/`.**
  - `adapter.rs`, `dispatch.rs`: dispatch matches machine `platform/dispatch.rs`.
  - `namespaces/`, `native/`: the stored backend names.
  - Each variant's `adapter.rs`, `provider.rs` (see §4), `mod.rs`, `seccomp.rs`.
  - `instance/`: settled. Note that it weakly echoes the daemon "instance" (`DaemonStatus.instance`).
  - `instance/{cgroup,exec,handle,paths,spec}.rs`, `native/instance/{exec,groups,handle}.rs`.
  - `linux/{confined,mounts}.rs`, `macos/{confined,mounts,mount_table}.rs`. `mount_table.rs` also holds `force_unmount`, which is tolerable.
  - `network/{links,lock,netlink,nftables,pool,resolver,setup}.rs`.
  - `rootfs/{cache,docker_archive,layers,tar}.rs`.
  - `rootfs/root.rs`: acceptable. It is better than `types.rs`, though it nearly repeats `rootfs`.
- **`exec/`.** `id.rs`, `output.rs`. `remote.rs` uses "remote" for a command in a machine's worker, as store `delete/remote.rs` and inspect `sandbox/remote.rs` do.
- **`image/`.**
  - `copy/` with `load.rs` (`machine.load_image` minus the folder word), `lock.rs`, `sweep.rs`, `docker.rs`.
  - `tar/{filter,header,pax,sparse}.rs`.
- **`lifecycle/`.** `create.rs` and `remove.rs` (the 2.0 calls), `launch.rs`, `connection.rs`, `mount_intents.rs`, `registry.rs`.
- **`process/`.** `daemon.rs` (`Local`, `LocalPaths`), `listener.rs`, `search_path.rs`, `streams.rs`.
- **`record/`.** `daemon.rs`, `discovery.rs` (matches store project `record/discovery.rs`), `reopen.rs`, `sandbox.rs`, `sweep.rs`, `tombstone.rs`, `unmount_guard.rs`, `workspace.rs`.
- **`route/`.**
  - `direct.rs`: matches workspace `route/direct.rs`.
  - `host/{call,dispatch,frame,send}.rs`: `send.rs` matches workspace `route/send.rs`. "host" is the wire op `sandbox.host`.
  - `daemon/{call,failure,refusal}.rs`.
  - `daemon/channel/{capacity,exchange,fence,internal,pool}.rs`. `internal.rs` is a weak word, but it names a deliberately hidden hook and no alternative is clearly better.
  - `daemon/workspace/{mount,commit,status,unmount}.rs`, which match the api workspace calls, and `recovery.rs`.
- **`runtime/`.** The folder is settled. `bind.rs`, `cancel.rs` (the `Stop` handle), `capabilities.rs`, `failure.rs`, `identity.rs` (`ContainerId`), `image.rs`, `request.rs` (`ExecRequest`), `sandbox.rs` (the control-side facade: `Backend`, `platform`, `run`), `trace.rs`.
- **`status/`.** `get.rs` and `list.rs` (the 2.0 calls), `state.rs` (as in machine and store), `live.rs`, `overlay.rs`, `run_folder.rs`, `inventory.rs`, `users.rs` (implements the `MachineUsers`/`StoreUsers` traits the lower crates declare).
- **Singular and plural.** These are consistent throughout. Concern folders are singular. Plurals are used for real sets: `mount_intents.rs`, `groups.rs`, `layers.rs`, `links.rs`, `mounts.rs`, `streams.rs`, `capabilities.rs`. `namespaces/` is a proper name (`LAYERFS_SANDBOX=namespaces`).

## 4. Cross-crate inconsistencies

1. **The executing side of host operations has two names.** Workspace uses
   `host/serve.rs`, and Sandbox uses `lifecycle/machine_side.rs`; #3 fixes this.
   The word "host" also does two jobs across the two crates. In workspace, `host/`
   is the executing side. In sandbox, `route/host/` is the typed host-operation
   request, its frame and its sending. Consider renaming workspace `host/` to
   something like `serve/` or `side/` in that crate's review.
2. **Driver variant file sets differ for the same job.** Machine variants are
   `platform/<v>/{adapter,lifecycle,profile,status}.rs`. Sandbox variants are
   `backend/<v>/{adapter,provider}.rs` plus `instance/` and an OS folder. Sandbox
   `provider.rs` does `launch`/`reopen`/`reopen_stopped`, which is the job of
   machine's `lifecycle.rs`. Under the proposed rule 19, consider renaming it to
   `lifecycle.rs`. "provider" is also the store's word for bucket providers
   (`backup/providers/`) and machine's word for VM providers.
3. **`stats.rs` vs `process.rs`.** Store and project use `status/stats.rs` for their
   own usage; Sandbox uses `status/process.rs`. #7 fixes this.
4. **Two `exec/cancel.rs` files with different roles.** Workspace's file is the
   `workspace.cancel` call; Sandbox's is the settling of running commands. #4 fixes
   this.
5. **One image cache, two unrelated names.** `image/artifact.rs` (receive, publish,
   sidecars) and `backend/namespaces/rootfs/cache.rs` (unpack once) share the folder
   `<root>/images/`. #8 aligns the names.
6. **Two tar codecs.** `image/tar/` (export filter and writer) and
   `backend/namespaces/rootfs/tar.rs` (unpack reader). The names are fine, but this
   is one concept in two places (rule 23). Record it; no rename is needed.
7. **Duplicate type names under the merged crate.** `runtime/options.rs` defines a
   backend `SandboxRecord` (the reopen facts), and `record/sandbox.rs` defines the
   Sandbox's `SandboxRecord`. Both now sit in one crate. This is out of scope for
   file naming but worth renaming the backend type (for example `ReopenRecord`)
   when the crates merge.
8. **Outside this crate:** store project `status/listing.rs` uses `listing` where
   every other crate uses `list.rs`.
