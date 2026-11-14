# A Workspace session

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

`sandbox.create`, `workspace.mount`, ordinary file access, `exec` and `unmount`: one daemon on a machine serving a Branch as a mutable filesystem any process can use.

## The path

```text
 caller             worker             Sandbox daemon     Store
 (host process)     (machine, if not   (container or      (file in the
                     local)             local process)     shared volume)
 -----------------  -----------------  -----------------  ------------
 sandbox.create
 [home lock] claim name, op intent
 support table: allowed?
 ==|== sandbox.host (one request) ==>|
                    launch daemon ---> start; open Store -----> open
                    <--- listen line <addr>
                    TCP + Noise KK --> handshake
                    Hello{wait} -----> ControlReady
 <== {sandbox, route, runtime} ======|
 record started, intent dropped

 workspace.mount
 ==|== workspace.mount ============>|  (or direct when forwarded)
                    bind ------------> find Branch by name;
                                      copy template ->
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

Terminal unmount deletes only the Workspace's own overlay file; committed
history is never touched. `workspace.commit` is followed in
[A Commit](commit.md).

## Steps

| # | What happens | Owning topic |
| --- | --- | --- |
| 1 | `sandbox.create` picks the machine (a sqlite Store's own machine), asks the support table whether platform, host and Store kind are allowed, and claims the name with an `op` intent. | [Sandboxes: what it does](../control/sandboxes/README.md#what-it-does), [Platforms: support table](../control/machines/platforms.md#support-table-supportrs), [Home: the `op` intent](../control/home/README.md#the-op-intent) |
| 2 | The host operation runs here or is sent once to the machine's worker as `sandbox.host`; a PostgreSQL Store's reach is checked first. | [Daemon channel and exec: the host operation](../control/sandboxes/daemon-channel-and-exec.md#routers-and-host-the-host-operation), [Workers and reach](../control/machines/workers-and-reach.md#reach-reachrs) |
| 3 | The machine's platform fixes the backend (`native` on macOS; `namespaces` on Linux and every `lima`, `apple-container` and `docker` machine), which launches the daemon once with a fresh `DaemonSetup`. | [Sandboxes: backends](../control/sandboxes/backends.md), [Bridge: daemon setup](../engine/bridge/README.md#daemon-setup) |
| 4 | The daemon builds its overlay template and opens the global Store directly. | [Daemon: startup](../engine/daemon/README.md#startup), [Overlay: database](../engine/overlay/README.md#database) |
| 5 | Readiness: TCP connect, Noise KK handshake, then `Hello` waits for `ControlReady`. | [Daemon: phases and Hello](../engine/daemon/README.md#phases-and-hello), [Bridge](../engine/bridge/README.md) |
| 6 | The caller records the Sandbox as started; where the machine forwards the daemon's port, later Workspace calls dial it directly. | [Sandboxes: what it does](../control/sandboxes/README.md#what-it-does), [Workers and reach: forwards](../control/machines/workers-and-reach.md#forwards) |
| 7 | `workspace.mount` (not `main`) binds: the daemon finds the Branch by name, copies the overlay template to the Workspace's own file and opens a Store reader at the Branch snapshot. No tree is scanned or copied. | [Daemon control calls: mount and attach](../engine/daemon/control-calls.md#mount-and-attach), [Workspace: base](../engine/workspace/README.md#base) |
| 8 | Attach makes the kernel mount: a FUSE session on Linux, a loopback NFSv3 export on macOS. | [Kernel routes](../engine/kernel-routes/README.md), [FUSE](../engine/kernel-routes/fuse.md), [NFS](../engine/kernel-routes/nfs.md) |
| 9 | Any process that sees the mount uses ordinary system calls; each change is one short overlay job under the Workspace's lock. No Exec registration is needed. | [Workspace: mutation](../engine/workspace/README.md#mutation), [Workspace operations: kernel mutations](../engine/workspace/operations.md#kernel-mutations) |
| 10 | `workspace.exec` runs `bash -c <command>` through the Sandbox runtime in the Workspace directory; the daemon is not involved. | [Daemon channel and exec: commands and cancel](../control/sandboxes/daemon-channel-and-exec.md#exec-commands-and-cancel) |
| 11 | `workspace.unmount` refuses a dirty Workspace unless `discard`; else the daemon detaches, drains, revokes, closes the overlay and Store reader, and deletes the overlay file. | [Daemon control calls: unmount](../engine/daemon/control-calls.md#unmount), [Overlay lifetimes: terminal disposal](../engine/overlay/lifetimes.md#terminal-disposal) |

## Related

- [Sandboxes](../control/sandboxes/README.md), [Machines](../control/machines/README.md)
- [Daemon](../engine/daemon/README.md), [Workspace](../engine/workspace/README.md), [Overlay](../engine/overlay/README.md), [Kernel routes](../engine/kernel-routes/README.md)
- [SDK: sandbox](../api/sdk/sandbox.md), [SDK: workspace](../api/sdk/workspace.md)
