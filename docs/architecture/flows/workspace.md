# A Workspace session

[Architecture](../README.md) › [Flows](README.md) › A Workspace session

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`sandbox.create`, `workspace.mount`, ordinary file access, `exec` and `unmount`.

A Sandbox is one `layerfs-daemon` started by a backend on a machine. Each
`workspace.mount` binds one Branch into it as a Workspace, a complete
mutable filesystem view. Any process that can see the mount uses it through
ordinary system calls. Terms are in the [glossary](../glossary.md);
`workspace.commit` is followed in [A Commit](commit.md).

## The path

```text
 caller             worker             Sandbox daemon     Store
 (host process)     (machine, if not   (container or      (file in the
                     local)             local process)     shared volume)
 -----------------  -----------------  -----------------  ------------
 sandbox.create
 [home lock] claim name, op intent
 support table: sandbox_allowed
 ==|== sandbox.host (one request) ==>|
                    launch daemon ---> start; open Store -----> open
                    <--- LAYERFS_DAEMON_LISTEN <addr>
                    TCP + Noise KK --> handshake
                    Hello{wait} -----> ControlReady
 <== {sandbox, route, runtime} ======|
 record started, intent dropped

 workspace.mount
 ==|== workspace.mount ============>|  (or direct when forwarded)
                    Branch id <------------------------------- read
                    Mount ----------> copy template ->
                                      <n>.sqlite; Store reader
                    Attach ---------> FUSE mount (Linux) or
                                      NFS export (macOS); Ready
 <== {kind: Mounted, path, ws} =====|

 any process: open/read/write/rename in <path>
                    kernel ---------> dispatch -> overlay job
 workspace.exec: bash -c via the Sandbox runtime; daemon not involved

 workspace.unmount
 ==|== Unmount{token, discard} ====>|  dirty? detach, drain,
                                      revoke, close, delete
                                      <n>.sqlite -> Unmounted
```

| Step | Component |
| --- | --- |
| 1 | [Control: Management](../control/management/README.md) |
| 2 | [Control: Support matrix](../control/support-matrix/README.md) |
| 3 | [Control: Machines](../control/machines/README.md) |
| 4 | [Control: Sandboxes](../control/sandboxes/README.md) |
| 5 | [Engine: Daemon](../engine/daemon/README.md) |
| 6 | [Engine: Kernel routes](../engine/kernel-routes/README.md) |
| 7 | [Engine: Workspace](../engine/workspace/README.md) |
| 8 | [Engine: Overlay](../engine/overlay/README.md) |

## Steps

### Choosing the machine, backend and kernel route

| Choice | Decided by | Source |
| --- | --- | --- |
| Machine | the `machine` argument; else a sqlite Store's own machine; else `native` for a PostgreSQL Store. A sqlite Store's Sandbox must run on the Store's machine. | [`create.rs`](../../../control/crates/layerfs-sandbox/src/create.rs) |
| Allowed at all | `sandbox_allowed(platform, host OS, Store kind)`, the support table's answer word for word | [Support matrix: refusal reasons](../control/support-matrix/table.md#refusal-reasons), [`support.rs`](../../../control/crates/layerfs-platform/src/support.rs) |
| Where the host operation runs | `place::on_machine`: this process when the platform runs calls locally, else the machine's worker (`sandbox.host`) | [`route.rs`](../../../control/crates/layerfs-sandbox/src/route.rs), [`place.rs`](../../../control/crates/layerfs-store/src/place.rs) |
| Backend | the platform: `docker` → Docker container; `native` on macOS → host process (stored `native`); `native` on Linux and every VM or ssh guest → namespaces | [Sandboxes: backends](../control/sandboxes/backends.md), [Machines: platforms and profiles](../control/machines/platforms-and-profiles.md) |
| Kernel route | `KernelRoute::Nfs` for the host-process backend (macOS); `KernelRoute::Fuse` otherwise | [`launch/local.rs`](../../../control/crates/layerfs-sandbox/src/launch/local.rs), [`serve.rs`](../../../control/crates/layerfs-sandbox/src/serve.rs) |

### Sequence

| # | What happens | Component / topic | Source |
| --- | --- | --- | --- |
| 1 | `sandbox.create(project, name, machine?, { image })`: under the home lock the Store's place and the name are checked and the name is claimed with an `op` intent (`sandbox.create`, phase `claim`, then `start`). | [Management](../control/management/README.md), [Management: home and records](../control/management/home-and-records.md#records) | [`create.rs`](../../../control/crates/layerfs-sandbox/src/create.rs), [`op.rs`](../../../control/crates/layerfs-home/src/op.rs) |
| 2 | On the machine (here or in its worker): deploy settings are chosen; a PostgreSQL Store's reach is checked once; the Store is held open under a lease. | [Routing: workers](../control/routing/workers.md#deploy), [Network: reach](../control/network/reach.md) | [`route.rs`](../../../control/crates/layerfs-sandbox/src/route.rs) |
| 3 | Daemon start: fresh controller and daemon key pairs; a `DaemonSetup` names the listen address, keys, kernel route, overlay template path, mount home, command identity (501:20) and limits (8 control connections, 16 Workspaces, 64 MiB cache). The backend launches the daemon once and waits for its `LAYERFS_DAEMON_LISTEN` line. | [Sandboxes](../control/sandboxes/README.md), [Sandboxes: Docker](../control/sandboxes/docker.md), [Native and namespaces](../control/sandboxes/native-and-namespaces.md), [Daemon: bridge](../engine/daemon/bridge.md#daemon-setup) | [`serve.rs`](../../../control/crates/layerfs-sandbox/src/serve.rs), [`launch/provider.rs`](../../../control/crates/layerfs-sandbox/src/launch/provider.rs), [`runtime.rs`](../../../control/crates/layerfs-isolation/src/runtime.rs) |
| 4 | The daemon builds its overlay template and opens the global Store directly (the setup names the existing Store; no install is sent). | [Daemon: startup](../engine/daemon/startup.md#owner), [Overlay: database](../engine/overlay/database.md#template-and-startup) | [`store.rs` (bridge)](../../../engine/crates/layerfs-bridge/src/store.rs) `opens_existing` |
| 5 | Readiness: TCP connect, Noise KK handshake, then `Hello { wait_for_store: true }`, which waits until the daemon leaves its startup phases; the phase must be `ControlReady`. | [Daemon: startup](../engine/daemon/startup.md#serve-and-readiness), [Daemon: bridge](../engine/daemon/bridge.md#duplex-channels) | [`launch/connection.rs`](../../../control/crates/layerfs-sandbox/src/launch/connection.rs) |
| 6 | The caller records the Sandbox as started and drops the intent in one write. Where the machine carries the daemon's port to this host, the caller keeps its control keys and later Workspace calls dial the daemon directly. | [Network: forwards](../control/network/forwards.md), [Routing: daemon channel](../control/routing/daemon-channel.md) | [`create.rs`](../../../control/crates/layerfs-sandbox/src/create.rs), [`direct.rs`](../../../control/crates/layerfs-sandbox/src/direct.rs) |
| 7 | `workspace.mount(sandbox, branch, { name })`: `main` is refused; the Sandbox must be started, the name free and fewer than 16 Workspaces mounted. A random `WorkspaceId` is drawn and the Branch id is read through the Store. | [Routing: daemon channel](../control/routing/daemon-channel.md#workspace-calls) | [`workspace/serve.rs`](../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) `mount_with` |
| 8 | `Mount` (bind): the daemon copies the overlay template to a new `<n>.sqlite`, opens it, opens a Store reader at the Branch snapshot and registers the Workspace. No tree is scanned or copied. | [Daemon: control calls](../engine/daemon/control-calls.md#attach-and-detach), [Overlay: one file per Workspace](../engine/overlay/database.md#one-file-per-workspace), [Workspace: base](../engine/workspace/base.md) | [`daemon/workspace/mount.rs`](../../../control/crates/layerfs-sandbox/src/daemon/workspace/mount.rs) |
| 9 | `Attach`: the kernel mount is made (FUSE session on Linux, loopback NFSv3 export on macOS). The reply is Ready only when every receive loop serves; it names the mount directory. | [Kernel routes: FUSE](../engine/kernel-routes/fuse.md#session-startup), [Kernel routes: NFS](../engine/kernel-routes/nfs.md#mount) | [`control/attach.rs`](../../../engine/crates/layerfs-daemon/src/control/attach.rs) |
| 10 | The caller saves the Workspace record and answers `{ kind: "Mounted", path, ws, … }`; `ws` is `<sandbox>:<path>` or `<sandbox>:<name>`. | [Management: home and records](../control/management/home-and-records.md#records) | [`workspace/calls.rs`](../../../control/crates/layerfs-sandbox/src/workspace/calls.rs) |
| 11 | File access: any process that sees the mount (a shell, an editor, a build) calls `open`, `write`, `rename`. The kernel sends each request to the daemon, which runs it as Workspace operations; each change is one short overlay job under the Workspace's lock. No Exec registration is needed. | [Workspace: the effective view](../engine/workspace/README.md#the-effective-view), [Workspace: operations](../engine/workspace/operations.md#kernel-mutations), [Workspace: mutation](../engine/workspace/mutation.md#jobs), [Kernel routes: coherence](../engine/kernel-routes/coherence.md) | [`operations/kernel_mutation.rs`](../../../engine/crates/layerfs-workspace/src/operations/kernel_mutation.rs), [`mutation/job.rs`](../../../engine/crates/layerfs-workspace/src/mutation/job.rs) |
| 12 | `workspace.exec(ws, command)`: the Sandbox runtime runs `bash -c <command>` as the command identity in the Workspace directory and returns the exit code and output; output also goes out as events. The daemon is not involved. | [Sandboxes: commands and streams](../control/sandboxes/commands-and-streams.md) | [`exec.rs`](../../../control/crates/layerfs-sandbox/src/exec.rs) |
| 13 | `workspace.unmount(ws, { discard, force })`: under the Sandbox folder's guard an unmount attempt is recorded; then one `Unmount { token, discard }`. The daemon refuses a dirty Workspace unless `discard`; else it detaches the kernel mount, drains requests, revokes the engine mount, closes the overlay and Store reader, and deletes `<n>.sqlite`. The caller removes the Workspace record. | [Daemon: control calls: unmount](../engine/daemon/control-calls.md#unmount), [Overlay: lifetimes](../engine/overlay/lifetimes-and-reclamation.md) | [`workspace/serve.rs`](../../../control/crates/layerfs-sandbox/src/workspace/serve.rs) `unmount`, [`control/detach.rs`](../../../engine/crates/layerfs-daemon/src/control/detach.rs) |

Terminal unmount deletes only the Workspace's own overlay file. Committed
history in the Store is never touched; uncommitted changes go with the file
only when `discard` (or `force`) was given. A forced unmount makes one abort
write and answers `ForceUnmounted` with what it established
([Daemon: forced teardown](../engine/daemon/forced-teardown.md)).
`sandbox.close` stops and deletes the Sandbox, each once.

## Failures along the way

| Step | What can fail | What the caller sees |
| --- | --- | --- |
| 1 | Name `native` or `docker`, or an invalid name | `InvalidInput` |
| 1 | The support table refuses this platform, host and Store kind | `Unsupported`, with the table's reason |
| 1 | A sqlite Store on another machine | `Unsupported`, naming `store pull` and `store push` |
| 1 | Name taken | `Exists` |
| 2 | Machine's worker not reached | `Unavailable`; nothing sent; the claimed name is released |
| 2 | PostgreSQL server not reached from the machine, or it holds another Store | `Unavailable`, naming the machine and host, never the URL |
| 3–5 | Start failed and every effect is known | `Failed` with the daemon's last log lines; the container was stopped and deleted, each once |
| 3–5 | Start outcome not known, or records not written | `Unknown`; the intent stays `uncertain` and later calls on the name are `Unresolved` until `sandbox.close` |
| 7 | Branch `main` | `InvalidInput` "main is the project's published line, not a Branch" |
| 7 | No such Sandbox; Workspace name in use; 16 mounted | `NoSuchSandbox`; `Exists`; `Capacity` |
| 8–9 | Bind or attach refused by the daemon, or the exchange failed | `Failed` carrying the `MountFailure`; a failed attach leaves the Workspace bound |
| 11 | A file operation is refused | an errno on that system call; nothing reaches the API |
| 12 | The command exits non-zero | not a refusal: `kind: "exit:<code>"`, `exitCode` |
| 13 | Uncommitted changes and no `discard` | `Dirty`, listing up to 256 changed paths |
| 13 | A process still holds the mount | `Busy` from the daemon; reversible, the Workspace stays Ready |
| 13 | A step after detach stops | `Unknown` with the retained owner; nothing is retried |
| 13 | The daemon reply is lost after the request was sent | `Failed`, its message the exchange's phase with `attempted: true`; not resent |
| 13 | The daemon no longer knows the Workspace (`Missing`) | answered `Unmounted` |

## Demonstration

These commands were written from source and were not run while writing this
page.

| Test | What it sets up | What it checks | Command | Needs |
| --- | --- | --- | --- | --- |
| [`kernel_mount.rs`](../../../engine/crates/layerfs-daemon/tests/kernel_mount.rs) | An installed Store and a daemon control service with a real FUSE mount | Ready evidence; the complete root read through ordinary system calls by an unregistered process; attach refusals have no effect; reversible `Busy`; full drain; two Workspaces mount and unmount independently | `cargo +1.85.1 test --locked -p layerfs-daemon --test kernel_mount` | Linux (`#![cfg(target_os = "linux")]`); `/dev/fuse`; root, since commands run as uid 65534 |
| [`mounted_execute.rs`](../../../engine/crates/layerfs-daemon/tests/mounted_execute.rs) | A mounted Workspace holding a program | The kernel executes a stored program, and after an in-place rewrite runs the rewrite | `cargo +1.85.1 test --locked -p layerfs-daemon --test mounted_execute` | Linux; `/dev/fuse`; root |
| [`sandbox_family.rs`](../../../api/crates/layerfs-api/tests/sandbox_family.rs) | A home with a Store on `native` | Sandbox names unique across the home, the wrong-machine refusal, then (live half) mount, exec, status, commit, unmount and close | `cargo +1.85.1 test --locked -p layerfs-api --test sandbox_family` | Live half needs `LAYERFS_DAEMON` naming a daemon binary on macOS, or Linux as root; without it the test prints NOT_RUN |

---

← [Flow: Project Init](init.md) · [Up: Flows](README.md) · [Flow: A Commit](commit.md) →
