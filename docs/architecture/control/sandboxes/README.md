# Sandboxes

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How a Sandbox is started, reached, used and closed: one `layerfs-daemon` on a
machine's kernel, the commands run beside it, and the caller-side channel that
carries its Workspace calls.

| | |
| --- | --- |
| Layer | [Control](../README.md) |
| Crates | `layerfs-sandbox`, `layerfs-sandbox-backend` |
| Source | [`layerfs-sandbox/src`](../../../../control/crates/layerfs-sandbox/src), [`layerfs-sandbox-backend/src`](../../../../control/crates/layerfs-sandbox-backend/src) |
| Uses | [Engine: Bridge](../../engine/bridge/README.md) (wire records), [Engine: Daemon](../../engine/daemon/README.md) (the process started), [Machines](../machines/README.md) (worker route, a platform's backend), [Stores and projects](../stores-and-projects/README.md) (resolve, lease, open Store), [Home](../home/README.md) (records, locks, secrets) |
| Used by | [API: `sandbox` calls](../../api/sdk/sandbox.md), [API: `workspace` calls](../../api/sdk/workspace.md), [Machines: workers and reach](../machines/workers-and-reach.md) (a worker runs `sandbox.host`), [Doctor](../doctor/README.md) |
| Flows | [A call](../../flows/call.md), [A Workspace session](../../flows/workspace.md) |

A Sandbox is one daemon a backend started, plus the commands run in it. The
daemon serves the Workspaces' files and supervises no command; the Sandbox
runtime owns command launch, streams, exit status and cancel
([glossary](../../glossary.md)). Which process serves each call is the single
"Where each call runs" table in [Control](../README.md).

## Picture

```text
 caller (API control)                         machine's worker
   sandbox.* / workspace.*                    (op "sandbox.host")
        |                                            ^
        v                                            |
 route.rs: Executor::Local ------or------ Executor::Worker
        |   (both sides run the same host code below)
        v
 lifecycle.rs, launch/, reopen.rs, close.rs      exec/: run, record, cancel
        |                                            |
 ======= layerfs-sandbox-backend ====================|=================
        v                                            v
 Runtime --launch_with_binds--> Sandbox --run_in--> bash -c <command>
   HostProcess (macOS)            endpoint            as uid:gid, in its
   Namespaces  (Linux)            stop, delete        own process group
        |                                             or cgroup; streams
        v                                             -> output Spool
 layerfs-daemon on 127.0.0.1:<port>
        ^
        | daemon/: Controls pool (<= 6 per process), one exchange each
 Workspace calls: mount, commit, unmount, status   (direct.rs: from the
                                                    home, when forwarded)
```

## What it does

| Behaviour | Detail | Source |
| --- | --- | --- |
| Choose the backend | `deploy.sandbox` (`LAYERFS_SANDBOX`) is `native` (macOS) or `namespaces` (Linux). Unset or `docker` names the retired docker backend and is refused; `native` on Linux and `namespaces` off Linux are refused; all before any effect. | [`runtime.rs`](../../../../control/crates/layerfs-sandbox/src/runtime.rs) `backend` |
| `sandbox.create` | Under the Sandbox folder's guard: a started record is `Exists` (a retired docker record is forgotten first). Builds a fresh `DaemonSetup` (listen `127.0.0.1:0`, keys, kernel route, overlay template, mount home, command identity, limits), launches once and waits up to 30 s for the daemon's listen line. A sqlite Store's directory is bound in; a PostgreSQL Store's server is reach-checked first. | [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) `create`, `setup`; [`launch/local.rs`](../../../../control/crates/layerfs-sandbox/src/launch/local.rs); [`launch/provider.rs`](../../../../control/crates/layerfs-sandbox/src/launch/provider.rs); [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) `reach_check` |
| Command identity | Linux: uid 501, gid 20. macOS: the caller's own uid and gid. | [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) `setup`, [`launch/local.rs`](../../../../control/crates/layerfs-sandbox/src/launch/local.rs) |
| Launch on the kernel | `Runtime::launch_with_binds` starts the daemon once in a private directory and returns the owner; a failure after an effect returns it in `LaunchFailure::instance` for one stop and one delete. | [`runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs), [Backends](backends.md) |
| Reopen | A later call reopens the Sandbox by its recorded runtime (`native` or `namespaces`); `Runtime::reopen` checks the recorded process identity and starts nothing. A daemon found gone or stopped is `NoSuchSandbox`, with every Workspace it held lost. | [`reopen.rs`](../../../../control/crates/layerfs-sandbox/src/reopen.rs), [`registry.rs`](../../../../control/crates/layerfs-sandbox/src/registry.rs) `sandbox_live` |
| Route a host operation | One of ten operations (`OPS`) runs in this process or is sent once to the machine's worker as `sandbox.host`; nothing starts a stopped machine. | [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs), [`host/call.rs`](../../../../control/crates/layerfs-sandbox/src/host/call.rs), [Daemon channel and exec](daemon-channel-and-exec.md#routers-and-host-the-host-operation) |
| Workspace calls | Mount (bind, then attach), Commit, unmount and status go to the daemon over a pooled authenticated connection, one exchange each. | [`daemon/`](../../../../control/crates/layerfs-sandbox/src/daemon), [Daemon channel and exec](daemon-channel-and-exec.md#daemon-the-control-channel) |
| `workspace.exec` | Runs `bash -c <command>` through `Sandbox::run_in` in the Workspace directory, `HOME` the Sandbox scratch directory; keeps an exec record while it runs; output inline up to 1 MiB per stream, else spooled. The daemon is not involved. | [`exec/run.rs`](../../../../control/crates/layerfs-sandbox/src/exec/run.rs), [Daemon channel and exec](daemon-channel-and-exec.md#exec-commands-and-cancel) |
| `workspace.exec_cancel` | Marks the record under the Sandbox lock, then signals once: SIGTERM, the grace (5000 ms default), SIGKILL. | [`exec/cancel.rs`](../../../../control/crates/layerfs-sandbox/src/exec/cancel.rs), [`stop.rs`](../../../../control/crates/layerfs-sandbox-backend/src/stop.rs) |
| `sandbox.close` | Settles running commands (`Busy`, or each cancelled with `force`); refuses `MountsActive` while a Workspace is mounted, unless `force`; then `stop(5)` and `delete`, each attempted once. | [`close.rs`](../../../../control/crates/layerfs-sandbox/src/close.rs), [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) |
| Direct route | A Sandbox whose daemon port its machine forwards to this host (`lima`'s `ssh -L`) keeps its keys in the home's `control.json` (0600); its Workspace calls dial the daemon from the home, while exec still goes through the worker. | [`direct.rs`](../../../../control/crates/layerfs-sandbox/src/direct.rs) |
| `sandbox.get` capacity | Reads the daemon's `backend.daemon.connection_failures` on a lent connection, bounded to 2 s and fenced at that deadline. | [`daemon/capacity.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/capacity.rs), [`get.rs`](../../../../control/crates/layerfs-sandbox/src/get.rs) |
| Retired docker records | A record whose `backend` or `runtime` is `docker`, or a started docker-machine record with no `runtime`: every call but `sandbox.close` is `Unsupported`, naming `docker rm -f <container>`; `sandbox.close` forgets it. | [`record/sandbox.rs`](../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs) `retired_container`, [`reopen.rs`](../../../../control/crates/layerfs-sandbox/src/reopen.rs) `retired`, [`close.rs`](../../../../control/crates/layerfs-sandbox/src/close.rs) `forget` |

## Invariants

| Rule | Enforced by |
| --- | --- |
| The Sandbox runtime, not the daemon, launches commands and owns their streams, exit status and cancel. | `Sandbox::run_in` ([`runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs)); [`exec/run.rs`](../../../../control/crates/layerfs-sandbox/src/exec/run.rs) sends nothing to the daemon |
| One attempt per operation: no resend, rebind, re-attach, adoption or implicit cleanup. | `*_attempted` flags ([`host_process/sandbox.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/sandbox.rs), [`namespaces/sandbox.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/sandbox.rs)); `Control::call` ([`daemon/channel.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/channel.rs)) |
| A failed or fenced connection is never lent again. | `Lease::drop` ([`daemon/pool.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs)) |
| Commands run as a nonroot identity. | `CommandIdentity::check` refuses uid or gid 0 ([`types/execution.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/execution.rs)) |
| A record is reopened only by the backend that wrote it. | `SandboxRecord` variants; each provider refuses another's record before any effect ([`types/options.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/options.rs)) |
| No namespaces command can create a user namespace. | Seccomp filter ([`local/os/linux/seccomp.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/os/linux/seccomp.rs)); host check ([`namespaces/host.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/host.rs)) |
| Debug output never shows the daemon setup or command arguments. | `LaunchRequest` and `ExecRequest` `Debug` ([`types/options.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/options.rs), [`types/request.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/request.rs)) |
| Stored names do not follow Rust names: `HostProcess` is `"native"`. | `HostProcessBackend::stored_name` ([`host_process/adapter.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/adapter.rs)) |
| Deleting a Sandbox never touches the shared Store. | `delete` removes only the Sandbox's directory, cgroup and private root ([`namespaces/sandbox.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/sandbox.rs), [`host_process/sandbox.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/sandbox.rs)) |
| A cancel that arrives before its command runs never answers `ended` for a command that then runs. | `Running::spawned` refuses as interrupted ([`exec/record.rs`](../../../../control/crates/layerfs-sandbox/src/exec/record.rs)) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Backend per host | `HostProcess` launches on macOS only, `Namespaces` on Linux only; Windows has no backend. | [`runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs), [`native/profile.rs`](../../../../control/crates/layerfs-platform/src/platforms/native/profile.rs) |
| Daemon setup size | 1 to 8192 bytes, else `Protocol`. | [`local/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/create.rs) |
| Startup wait | 30 s (`STARTUP`), then `TimedOut`; a daemon that exits first is `Protocol`. | [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs), [`local/listener.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/listener.rs) |
| Binds | Only `Namespaces` binds; `HostProcess` refuses a non-empty list `Unsupported`. | `launch_with_binds` ([`runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs)) |
| Namespaces host | Root, user namespaces blocked, cgroup v2 with `cgroup.kill`, `/dev/fuse`, a runtime root that can hold an overlay upper, the helper executable; else `Unsupported`. | [`namespaces/host.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/host.rs), [`namespaces/provider.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/provider.rs) |
| Workspaces per Sandbox | 16 (`WORKSPACES`); a mount into a full one is `Capacity`. | [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) |
| Control connections | 6 per process (`CONNECTIONS`) of the daemon's 8; no free one in 60 s is `Busy`, "nothing was attempted". | [`daemon/pool.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs) |
| Lost daemon reply | `Unavailable` with `outcome: "unknown"`, `retryable: false`. | `refusal` ([`daemon/values.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/values.rs)) |
| Exec id | 1 to 64 of `A-Za-z0-9._-`, not starting with `.` or `-`, else `InvalidInput`; a live duplicate is `Exists`. | [`exec/id.rs`](../../../../control/crates/layerfs-sandbox/src/exec/id.rs), [`exec/record.rs`](../../../../control/crates/layerfs-sandbox/src/exec/record.rs) |
| Exec output | 1 MiB per stream inline, then spooled to a file; no total cap, nothing cut. | [`output.rs`](../../../../control/crates/layerfs-sandbox/src/output.rs) |
| Exec duration | No timeout; a command runs until it ends or is cancelled. | [`host_process/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/exec.rs), [`namespaces/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/exec.rs) |
| Stop and delete | A second `stop`, and `delete` before a known stop, are refused. | [Backends](backends.md#stop-and-delete) |
| Unmount or close with live commands | `Busy` (`detail.execs`); with `force` each is cancelled first; one not confirmed is `Unknown`. | `settle` ([`exec/cancel.rs`](../../../../control/crates/layerfs-sandbox/src/exec/cancel.rs)) |

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`conformance.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/conformance.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox-backend --test conformance` | Unix, no root | Over `Backend::ALL`: stored names round-trip; each mount driver serves its `SUPPORT.json` rows' kernel route; bind refusal or carriage; runtimes refuse other backends' options and records; the lifecycle this host can run. |
| [`runtime_enum.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/runtime_enum.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox-backend --test runtime_enum` | Unix | Stored names `["native", "namespaces"]`, capabilities, bind refusal. |
| [`native_provider.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/native_provider.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox-backend --test native_provider` | macOS | Native launch with a stub daemon; a partial launch keeps its owner; a record reopens the exact process. |
| [`stop_handle.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/stop_handle.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox-backend --test stop_handle` | Linux, macOS | A cancel ends a command's process group, SIGTERM within the grace and SIGKILL after; a cancel before the handle ends it as it starts. |
| [`namespaces_lifecycle.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/namespaces_lifecycle.rs) | `cargo +1.85.1 build --locked -p layerfs-sandbox-backend --bin layerfs-sandbox-helper`, then `cargo +1.85.1 test --locked -p layerfs-sandbox-backend --test namespaces_lifecycle` | Linux with root, `/dev/fuse`, cgroup v2 and `user.max_user_namespaces=0`; otherwise each test reports why and returns (not proof) | Own namespaces and cgroup; two Sandboxes on one Store cannot see each other; reopen by record; close leaves nothing behind. |
| [`namespaces_userns.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/namespaces_userns.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox-backend --test namespaces_userns` | Linux, no root | Under the filter `unshare(CLONE_NEWUSER)` answers `EPERM` and `clone3` `ENOSYS`, while a threaded command still runs. |
| [`exec_start_race.rs`](../../../../control/crates/layerfs-sandbox/tests/exec_start_race.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox --test exec_start_race` | Unix | A cancel before the command runs answers `cancelled` and the owner ends the command at spawn. |
| [`pool_admission.rs`](../../../../control/crates/layerfs-sandbox/tests/pool_admission.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox --test pool_admission` | Unix | A full daemon answers the next connection with a capacity notice before the handshake: `Capacity`, nothing sent. |
| [`retired_docker.rs`](../../../../control/crates/layerfs-sandbox/tests/retired_docker.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox --test retired_docker` | Linux, macOS | A docker-backend record is refused `Unsupported` and `sandbox.close` forgets it, touching nothing it names. |

## Pages

| Page | What it covers |
| --- | --- |
| [Daemon channel and exec](daemon-channel-and-exec.md) | The caller side of `layerfs-sandbox`: the daemon control pool and Workspace calls, exec records and cancel, the host operation's route and the direct route. |
| [Backends](backends.md) | `layerfs-sandbox-backend`: the sealed adapter, `Runtime` and `Sandbox`, the native and namespaces backends, stop handles, the docker machine, and adding a backend. |
