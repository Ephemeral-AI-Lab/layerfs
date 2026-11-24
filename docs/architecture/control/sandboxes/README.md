# Sandboxes

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How a Sandbox is started, reached, used and removed: one `layerfs-daemon` on a
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
 lifecycle.rs, launch/, reopen.rs, remove.rs     exec/: run, record, cancel
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
| Choose the backend | No setting chooses it. The Sandbox record's machine decides where its host operations run (`place::on_machine`: `native` in this process, any other machine in its worker), and that side sets `deploy.sandbox` from its own OS: `native` on macOS, `namespaces` on Linux, a worker always Linux. `runtime::backend` still checks the value: unset or `docker` names the retired docker backend and is refused, as are `native` on Linux and `namespaces` off Linux, all before any effect. A later call follows the record's `runtime`. | [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) `host_args`, [`runtime.rs`](../../../../control/crates/layerfs-sandbox/src/runtime.rs) `backend` |
| `sandbox.create` | Under the Sandbox folder's guard: a started record is `Exists` (a record of the retired docker backend is cleared first). If an existing SQLite project record without an `op` lacks a complete `manifest`/`installed`, it is rebuilt read-only from the local Store file before launch (`restored::repair_project`), once and best effort: a repair that does not complete it leaves the call's refusal as it was. Builds a fresh `DaemonSetup` (listen `127.0.0.1:0`, keys, kernel route, overlay template, mount home, command identity, limits), launches once and waits up to 30 s for the daemon's listen line. A sqlite Store's directory is bound in; a PostgreSQL Store's server is reach-checked first. | [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) `create`, `setup`, `installed`; [`launch/local.rs`](../../../../control/crates/layerfs-sandbox/src/launch/local.rs); [`launch/provider.rs`](../../../../control/crates/layerfs-sandbox/src/launch/provider.rs); [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) `reach_check` |
| Command identity | Linux: uid 501, gid 20. macOS: the caller's own uid and gid. | [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) `setup`, [`launch/local.rs`](../../../../control/crates/layerfs-sandbox/src/launch/local.rs) |
| Launch on the kernel | `Runtime::launch_with_binds` starts the daemon once in a private directory and returns the owner; a failure after an effect returns it in `LaunchFailure::instance` for one stop and one delete. | [`runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs), [Backends](backends.md) |
| Reopen | A later call reopens the Sandbox by its recorded runtime (`native` or `namespaces`); `Runtime::reopen` checks the recorded process identity and starts nothing. A daemon found gone or stopped is `NoSuchSandbox`, with every Workspace it held lost. | [`reopen.rs`](../../../../control/crates/layerfs-sandbox/src/reopen.rs), [`registry.rs`](../../../../control/crates/layerfs-sandbox/src/registry.rs) `sandbox_live` |
| Route a host operation | One of ten operations (`OPS`) runs in this process or is sent once to the machine's worker as `sandbox.host`; nothing starts a stopped machine. | [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs), [`host/call.rs`](../../../../control/crates/layerfs-sandbox/src/host/call.rs), [Daemon channel and exec](daemon-channel-and-exec.md#routers-and-host-the-host-operation) |
| Workspace calls | Mount (bind, then attach), Commit, unmount and status go to the daemon over a pooled authenticated connection, one exchange each. | [`daemon/`](../../../../control/crates/layerfs-sandbox/src/daemon), [Daemon channel and exec](daemon-channel-and-exec.md#daemon-the-control-channel) |
| `workspace.exec` | Runs `bash -c <command>` through `Sandbox::run_in` in the Workspace directory, `HOME` the Sandbox scratch directory; keeps an exec record while it runs; output inline up to 1 MiB per stream, else spooled. The daemon is not involved. | [`exec/run.rs`](../../../../control/crates/layerfs-sandbox/src/exec/run.rs), [Daemon channel and exec](daemon-channel-and-exec.md#exec-commands-and-cancel) |
| `workspace.cancel` (`--exec E` for one command) | Marks the record under the Sandbox lock, then signals once: SIGTERM, the grace (5000 ms default), SIGKILL. | [`exec/cancel.rs`](../../../../control/crates/layerfs-sandbox/src/exec/cancel.rs), [`stop.rs`](../../../../control/crates/layerfs-sandbox-backend/src/stop.rs) |
| `sandbox.remove` | Records its `sandbox.remove` intent, then settles running commands (`Busy`, or each cancelled with `force`); refuses `MountsActive` while a Workspace is mounted, unless `force`; then `stop(5)` and `delete`, each attempted once, through the internal host op `sandbox.close` (`lifecycle::close`; a forced remove on `native` goes through `force::close`), ends each mount intent its Workspaces held once a forced stop is confirmed (`mount_intents.rs`; one it cannot end is a `left` sentence; a close that ran without a PostgreSQL Store URL its machine cannot dial answers those bound by id in `ending`, and `route::reach` ends them in the caller with the Store's own URL, `mount_intents::end_handed`), and removes the record. A record with no id whose create may have started a daemon (on `native` its record names one, which its own side writes before the id; on a machine with a worker, its create got past claiming the name, and the worker keeps its own record) is closed the same way; a close that cannot run refuses the remove and the record keeps its create's intent. Only a record with no such daemon goes by its records alone. On a machine recorded `stopped`, a Sandbox whose runtime a forced `machine.stop` ended, or any with `force`, goes by the home's records alone, its worker not asked (`stopped_on`): `already_gone`, `left` saying the worker's record and run folder stay until its worker next starts, which removes them (`sweep::worker_start`: a worker record whose daemon is known gone, then the run folder it named); without either it is refused `Unavailable`, naming `machine start` and `--force`. | [`remove.rs`](../../../../control/crates/layerfs-sandbox/src/remove.rs), [`lifecycle.rs`](../../../../control/crates/layerfs-sandbox/src/lifecycle.rs) `close` |
| Direct route | A Sandbox whose daemon port its machine forwards to this host (`lima`'s `ssh -L`) keeps its keys in the home's `control.json` (0600); its Workspace calls dial the daemon from the home, while exec still goes through the worker. | [`direct.rs`](../../../../control/crates/layerfs-sandbox/src/direct.rs) |
| `sandbox.get` | Its record, then two reads that ask only the daemon: the Workspace list (`sandbox.list_workspaces` with `history: false`, so the daemon reads no Branch page, Layer or Commit) and, on the same lent connection, `backend.daemon.connection_failures`, bounded to 2 s and fenced at that deadline. It takes no Store lease, opens no Store and writes no record or folder on any side ([Store leases](#store-leases)). | [`get.rs`](../../../../control/crates/layerfs-sandbox/src/get.rs), [`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) `host_op`, `in_worker`, [`direct.rs`](../../../../control/crates/layerfs-sandbox/src/direct.rs) `served`, [`daemon/capacity.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/capacity.rs) |
| Inspection | The reads a client polls (`machine.log`, `sandbox.log`, `sandbox.files`, `machine.files`, Workspace overlay sizes) go through one bounded route, the worker operation `inspect`: no lock, lease, daemon call or write; two at a time per process. | [`inspect/`](../../../../control/crates/layerfs-sandbox/src/inspect), [Inspection](inspect.md) |
| Retired docker records | A record whose `backend` or `runtime` is `docker`, or a started docker-machine record with no `runtime`: every call but `sandbox.remove` is `Unsupported`, naming `docker rm -f <container>` when the daemon record keeps the container's name (else saying it does not); `sandbox.remove` removes its records, `outcome: already_gone`, and `left` says its container was not touched. | [`record/sandbox.rs`](../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs) `retired_container`, [`reopen.rs`](../../../../control/crates/layerfs-sandbox/src/reopen.rs) `retired`, [`remove.rs`](../../../../control/crates/layerfs-sandbox/src/remove.rs) `forget` |
| Machine-stop fields (record format) | A forced `machine.stop` adds two Bool fields to the Sandbox's `record.json`: `machine_stopped` (its runtime was stopped with the machine; shown `gone`) and `machine_stop_unknown` (the stop was not confirmed; shown `unresolved`). An absent field reads `false`, so older records need no change and `"v"` stays `1`. A build older than these fields keeps them as read but shows such a Sandbox as it was before the stop. | [`record/sandbox.rs`](../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs) `machine_stopped`, `with_machine_stop`; [`machine_force.rs`](../../../../control/crates/layerfs-control/src/machine_force.rs) `mark` |

## Store leases

A lease is `store/<store>/open/<pid>` in the home, taken under the Store's
lock ([Stores and projects](../stores-and-projects/README.md)); `store move`,
`remove` and `disconnect` refuse `InUse` while another process holds
one. Which Sandbox and Workspace calls take one depends on where the call
runs:

| Call | Machine that runs in this process (`native`) | Machine with a worker | Daemon port forwarded here (`lima`) |
| --- | --- | --- | --- |
| `sandbox.list` | none: records only | none | none |
| `sandbox.get` | none; no Store opened, nothing written | none at the home; the worker writes no record and opens no Store | none; no Store opened, nothing written |
| `sandbox.create`, `sandbox.remove` | lease; a sqlite Store is opened here and keeps the lease until it closes (idle 60 s); a PostgreSQL Store's lease lasts the call | none at the home; the worker keeps the Store's and project's records and opens the Store under its root | as a machine with a worker |
| `workspace.mount`, `commit`, `unmount`, `status` | as `sandbox.create` | as `sandbox.create` | lease for the call; no Store opened by the Sandbox code |
| `workspace.exec`, `workspace.cancel` | as `sandbox.create` | as `sandbox.create` | as a machine with a worker |

`sandbox.get` is the call a client polls, so it is the one that holds
nothing: `Op::asks_daemon_only` names its two host operations, and
`route::host_op`, `route::in_worker` and `direct::served` return for them
before the lease, the Store open, the worker's record writes and the run
root's creation (layerfs-project#181). A recorded Sandbox still holds its
Store by its record: `store move`, `remove` and `disconnect` refuse
`InUse` while one of the Store's projects has a Sandbox, whether or not
anything reads it. Every public call, a get included, appends one line to the
home's call log (`trace/calls.jsonl`).

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
| Control connections | 6 per process (`CONNECTIONS`) of the daemon's 8; a refused open holds the process to the connections it has for 2 s (`RECOVER`); no free one in 60 s is `Busy`, "nothing was attempted". | [`daemon/pool.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/pool.rs) |
| Lost daemon reply | `Unavailable` with `outcome: "unknown"`, `retryable: false`. | `refusal` ([`daemon/values.rs`](../../../../control/crates/layerfs-sandbox/src/daemon/values.rs)) |
| Exec id | 1 to 64 of `A-Za-z0-9._-`, not starting with `.` or `-`, else `InvalidInput`; a live duplicate is `Exists`. | [`exec/id.rs`](../../../../control/crates/layerfs-sandbox/src/exec/id.rs), [`exec/record.rs`](../../../../control/crates/layerfs-sandbox/src/exec/record.rs) |
| Exec output | 1 MiB per stream inline, then spooled to a file; no total cap, nothing cut. | [`output.rs`](../../../../control/crates/layerfs-sandbox/src/output.rs) |
| Exec duration | No timeout; a command runs until it ends or is cancelled. | [`host_process/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/exec.rs), [`namespaces/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/exec.rs) |
| Stop and delete | A second `stop`, and `delete` before a known stop, are refused. | [Backends](backends.md#stop-and-delete) |
| Unmount or remove with live commands | `Busy` (`detail.execs`); with `force` each is cancelled first; one not confirmed is `Unknown`. | `settle` ([`exec/cancel.rs`](../../../../control/crates/layerfs-sandbox/src/exec/cancel.rs)) |

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`conformance.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/conformance.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox-backend --test conformance` | Unix, no root | Over `Backend::ALL`: stored names round-trip; each mount driver serves its `SUPPORT.json` rows' kernel route; bind refusal or carriage; runtimes refuse other backends' options and records; the lifecycle this host can run. |
| [`runtime_enum.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/runtime_enum.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox-backend --test runtime_enum` | Unix | Stored names `["native", "namespaces"]`, capabilities, bind refusal. |
| [`native_provider.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/native_provider.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox-backend --test native_provider` | macOS | Native launch with a stub daemon; a partial launch keeps its owner; a record reopens the exact process. |
| [`stop_handle.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/stop_handle.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox-backend --test stop_handle` | Linux, macOS | A cancel ends a command's process group, SIGTERM within the grace and SIGKILL after; a cancel before the handle ends it as it starts. |
| [`namespaces_lifecycle.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/namespaces_lifecycle.rs) | `cargo +1.85.1 build --locked -p layerfs-sandbox-backend --bin layerfs-sandbox-helper`, then `cargo +1.85.1 test --locked -p layerfs-sandbox-backend --test namespaces_lifecycle` | Linux with root, `/dev/fuse`, cgroup v2 and `user.max_user_namespaces=0`; otherwise each test reports why and returns (not proof) | Own namespaces and cgroup; two Sandboxes on one Store cannot see each other; reopen by record; close leaves nothing behind. Each Sandbox's commands have their own network (two bind one port, a sibling is unreachable, the host end goes with the Sandbox), resolve through a server their network reaches, see the design's firewall table, and never run where the init holds no network of its own; an init spec naming a written resolver file (on any host, whatever its own resolver) gives commands their own view with that file while the daemon keeps the machine's, a daemon-side mount and unmount reach that view and not the machine, and a view that cannot be made ends the stage (125) before the daemon starts, leaving no process; with `LAYERFS_TEST_NETWORK_ALONE=1` and `--test-threads=1`, a machine with `ip_forward` 0 or a dropping `FORWARD` policy refuses the start `Unavailable` and is admitted once the hinted rules are added. |
| [`sandbox_network.rs`](../../../../api/crates/layerfs-api/tests/sandbox_network.rs) | `cargo +1.85.1 test --locked -p layerfs-api --features machine --test sandbox_network` | the docker machine (`LAYERFS_TEST_IMAGE`, `LAYERFS_BUNDLE`) or running VM machines (`LAYERFS_TEST_MACHINE_HOME`, `LAYERFS_TEST_MACHINES`, any of them `docker`); optional `LAYERFS_TEST_NET_PG`, `LAYERFS_TEST_NET_PROXY`, `LAYERFS_TEST_NET_GIT`; otherwise NOT_RUN | Live, per machine: two Sandboxes bind `127.0.0.1:3000`; a sibling is unreachable by TCP, ICMP and UDP, each probe showing that it ran (a refusal or timeout, an echo or datagram that left) beside a control the same probe gets through (ICMP through an unprivileged ICMP datagram socket from the Sandbox image's `python3`, which the check requires: commands have no `CAP_NET_RAW` for `busybox ping`); out to the internet, git and PostgreSQL by the machine's host name; names resolve; the daemon's and the worker's ports are closed; a fresh call keeps the address; doctor reports a planted leftover link; remove and a killed daemon take the host end. |
| [`namespaces_userns.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/namespaces_userns.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox-backend --test namespaces_userns` | Linux, no root | Under the filter `unshare(CLONE_NEWUSER)` answers `EPERM` and `clone3` `ENOSYS`, while a threaded command still runs. |
| [`exec_start_race.rs`](../../../../control/crates/layerfs-sandbox/tests/exec_start_race.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox --test exec_start_race` | Unix | A cancel before the command runs answers `cancelled` and the owner ends the command at spawn. |
| [`pool_admission.rs`](../../../../control/crates/layerfs-sandbox/tests/pool_admission.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox --test pool_admission` | Unix | A full daemon answers the next connection with a capacity notice before the handshake: `Capacity`, nothing sent. |
| [`sandbox_get_no_lease.rs`](../../../../api/crates/layerfs-api/tests/sandbox_get_no_lease.rs) | `cargo +1.85.1 test --locked -p layerfs-api --test sandbox_get_no_lease` (also with `--features machine`) | Unix; a stand-in daemon and this build's worker on a scratch root | While the Store's lock is held and its record carries a running `store.push` (a pre-2.0 move intent, still read as `store.move`), `sandbox.get` answers on the native, worker and forwarded routes for 0, 1 and 4 Workspaces; no lease folder, Store file or run root appears and no file changes in the home or under the worker's root; a daemon from before `history: false` gives the same answer. |
| [`half_created.rs`](../../../../control/crates/layerfs-sandbox/tests/half_created.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox --test half_created` | Unix; the native half macOS | A record with no id whose create may have started a daemon: on a machine, the remove asks the worker and, unreached, refuses and keeps the create's intent; on `native`, a remove that cannot close it refuses and leaves the daemon (a `sleep`) running, and a forced one stops it and deletes its run folder. |
| [`stopped_machine.rs`](../../../../control/crates/layerfs-sandbox/tests/stopped_machine.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox --test stopped_machine` | Unix | On a machine recorded `stopped`: a Sandbox a forced machine stop ended, or any with `force`, goes by its records (`already_gone`, `left`); without either `Unavailable` naming `machine start` and `--force`. A starting worker removes its record of a Sandbox whose daemon ended, then that run folder, and keeps one whose daemon lives. |
| [`pool_recovery.rs`](../../../../control/crates/layerfs-sandbox/tests/pool_recovery.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox --test pool_recovery` | Unix | After a refused open the limit holds the connections open for `RECOVER`, then is 6 again, and never more. |
| [`retired_docker.rs`](../../../../control/crates/layerfs-sandbox/tests/retired_docker.rs) | `cargo +1.85.1 test --locked -p layerfs-sandbox --test retired_docker` | Linux, macOS | A docker-backend record is refused `Unsupported` and `sandbox.remove` removes its records, touching nothing it names. |

## Pages

| Page | What it covers |
| --- | --- |
| [Daemon channel and exec](daemon-channel-and-exec.md) | The caller side of `layerfs-sandbox`: the daemon control pool and Workspace calls, exec records and cancel, the host operation's route and the direct route. |
| [Backends](backends.md) | `layerfs-sandbox-backend`: the sealed adapter, `Runtime` and `Sandbox`, the native and namespaces backends, stop handles, the docker machine, and adding a backend. |
