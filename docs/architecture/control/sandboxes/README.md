# Sandboxes

[Architecture](../../README.md) › [Control](../README.md) › Sandboxes

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How a Sandbox runs on a machine's kernel: the native and namespaces backends, commands and streams.

| | |
| --- | --- |
| Layer | [Control](../README.md) |
| Crates | `layerfs-sandbox-backend` |
| Source | [`control/crates/layerfs-sandbox-backend/src/backend`](../../../../control/crates/layerfs-sandbox-backend/src/backend), [`control/crates/layerfs-sandbox-backend/src/local`](../../../../control/crates/layerfs-sandbox-backend/src/local), [`control/crates/layerfs-sandbox-backend/src/runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs) |
| Uses | [Engine: Daemon](../../engine/daemon/README.md) |
| Used by | [Machines](../machines/README.md), [Control: Management](../management/README.md) |
| Flows | [A Workspace session](../../flows/workspace.md) |

## Topics

| Topic | What it covers |
| --- | --- |
| [Backends](backends.md) | The sealed BackendAdapter and the backend kinds. |
| [Docker machine Sandboxes](docker.md) | The `docker` machine runs namespaces Sandboxes in its worker container; the docker backend was retired (D-8). |
| [Native and namespaces](native-and-namespaces.md) | Local-process Sandboxes: native and namespaces. |
| [Commands and streams](commands-and-streams.md) | Running a command and returning its streams and status. |
| [Adding a backend](adding-a-backend.md) | The recipe for a new Sandbox backend. |

## The picture

```text
 API control (sandbox.create, workspace.exec)  -- in the caller, or in the
        |                                         machine's worker
        v
 Runtime (enum: the deployment's backend)      runtime.rs
   |  launch / launch_with_binds / reopen / reopen_stopped
   v
 Sandbox (enum: the launched owner)            runtime.rs
   |  endpoint  run  track  inspect  log_tail  stop  delete
   |
   +-- HostProcess -- local/ : one daemon process on this kernel (macOS)
   +-- Namespaces --- local/ + layerfs-sandbox-helper : daemon in its own
                       mount/PID/IPC namespaces and cgroup (Linux: VMs,
                       Linux native, the docker machine's worker)

 adapter(Backend) -> &dyn BackendAdapter      backend/adapter.rs
   stored_name  description  capabilities     (what is fixed per backend)
```

A Sandbox is one `layerfs-daemon` started by a backend, plus the commands
run in it. The daemon serves the Workspaces; the Sandbox runtime owns the
commands (see [glossary](../../glossary.md)).

## What it does

| Call | What the Sandbox side does | Source |
| --- | --- | --- |
| `sandbox.create` | `Runtime::launch_with_binds` starts one daemon once with its private 0600 setup, waits for its `LAYERFS_DAEMON_LISTEN <addr>` line, and returns the owner. A failure after an effect returns the owner in `LaunchFailure::instance` for one stop and delete. | [`runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs) |
| `workspace.exec` | API control builds an `ExecRequest` (`bash -c <command>`, the command identity, `HOME=<scratch>`, the Workspace directory, no stdin) and calls `Sandbox::run`, which streams stdout and stderr to sinks and returns `ExecExit`. The daemon is not involved. | [`runtime.rs`](../../../../control/crates/layerfs-sandbox/src/runtime.rs), [`exec/run.rs`](../../../../control/crates/layerfs-sandbox/src/exec/run.rs) |
| `sandbox.close` | `stop`, then `delete`, each attempted once. | [`runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs) |
| reopen | `Runtime::reopen` checks the recorded identity (pid + start time + boot) and starts nothing. | [`runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs) |

Which backend a machine uses is fixed by its platform:

| Platform | Backend | Source |
| --- | --- | --- |
| `native` on macOS | `HostProcess` (stored `"native"`) | [`native/profile.rs`](../../../../control/crates/layerfs-platform/src/platforms/native/profile.rs) |
| `native` on Linux; every VM and `ssh` guest; `docker` (in its worker container, D-8) | `Namespaces` | same; [`docker/profile.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/profile.rs); `namespaces` in [`platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs) |

## Invariants

| Rule | Enforced by |
| --- | --- |
| The `host_process` backend is stored and shown as `"native"`. | `HostProcessBackend::stored_name` ([`host_process/adapter.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/adapter.rs)); checked by `stored_names_parse_back_and_do_not_follow_the_rust_names` ([`runtime_enum.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/runtime_enum.rs)) |
| Sandbox, not the daemon, owns command launch, streams and exit status. | `Sandbox::run` in [`runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs); API `exec` "the daemon is not involved" ([`exec/run.rs`](../../../../control/crates/layerfs-sandbox/src/exec/run.rs)) |
| One attempt per operation; no resend, adoption or implicit cleanup. | Each owner's `*_attempted` flags (`host_process/sandbox.rs`, `namespaces/sandbox.rs`); `WireFailure` keeps what was sent ([`types/failure.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/failure.rs)) |
| Commands run as a nonroot identity. | `CommandIdentity::check` refuses uid or gid 0 ([`types/execution.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/execution.rs)) |
| A record is reopened only by the backend that wrote it. | `SandboxRecord` variants; each provider refuses another's record ([`types/options.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/options.rs)) |
| No command can create a user namespace. | namespaces: every command runs under a seccomp filter that refuses one ([`local/os/linux/seccomp.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/os/linux/seccomp.rs)); the host must offer that filter's actions or have `user.max_user_namespaces=0` ([`namespaces/host.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/host.rs)) |
| Debug output never shows the daemon setup or command arguments. | `LaunchRequest` and `ExecRequest` `Debug` print counts only ([`types/options.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/options.rs), [`types/request.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/request.rs)) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Backends per host | `HostProcess` launches on macOS only; `Namespaces` on Linux only. A platform without a backend is refused by the [support matrix](../support-matrix/README.md). | [`runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs), [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs) |
| Daemon setup size | 1 to 8192 bytes, else `Protocol`. | [`local/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/create.rs) |
| Startup wait | Caller's `startup_timeout`; must be non-zero. | same |
| Binds | Only `Namespaces` takes binds; HostProcess refuses a non-empty list `Unsupported` before any effect. | `launch_with_binds` in [`runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs) |
| Stop | Native: SIGTERM, wait `<s>`, SIGKILL, then every named process group. Namespaces: `cgroup.kill`, wait at least 5 s. | [Commands and streams](commands-and-streams.md#keep-alive-and-stop) |
| Delete before a known stop | Refused ("known stop required"). | each `sandbox.rs` |
| Old docker records | A Sandbox recorded under the retired docker backend: every call `Unsupported` (recreate it), except `sandbox.close`, which forgets it. | [Docker machine Sandboxes](docker.md#sandboxes-recorded-under-the-docker-backend) |

## Demonstration

The commands were not run while writing this page.

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`native_provider.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/native_provider.rs) | `layerfs-sandbox-backend` | A native launch through `Runtime` with a stub daemon script; a partial launch keeps its owner; a record reopens the exact process. |
| [`namespaces_lifecycle.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/namespaces_lifecycle.rs) | `layerfs-sandbox-backend` | A Sandbox is its own namespaces and cgroup; two on one Store cannot see each other; a record reopens the exact Sandbox; a running command ends with its Sandbox; close leaves nothing behind. |
| [`namespaces_userns.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/namespaces_userns.rs) | `layerfs-sandbox-backend` | A command run under the filter runs under one more seccomp filter, `unshare(CLONE_NEWUSER)` answers `EPERM` and `clone3` answers `ENOSYS`, while the command itself (a threaded test harness) still runs; needs no root or Sandbox. |

- `native_provider`: macOS only (`#![cfg(target_os = "macos")]`); writes a
  stub daemon under the temp directory.
  `cargo +1.85.1 test --locked -p layerfs-sandbox-backend --test native_provider`
- `namespaces_lifecycle`: Linux only; needs root, `/dev/fuse`, cgroup v2 and
  either `user.max_user_namespaces=0` or the seccomp filter's actions, else
  each test reports why and returns
  (not proof). Build the helper first:
  `cargo +1.85.1 build --locked -p layerfs-sandbox-backend --bin layerfs-sandbox-helper`, then
  `cargo +1.85.1 test --locked -p layerfs-sandbox-backend --test namespaces_lifecycle`.
  `LAYERFS_H38_KEEP` keeps the scratch folder.
- `namespaces_userns`: Linux only; runs its own test binary again as the
  filtered command.
  `cargo +1.85.1 test --locked -p layerfs-sandbox-backend --test namespaces_userns`

The cross-backend kit is
[`conformance.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/conformance.rs):
`cargo +1.85.1 test --locked -p layerfs-sandbox-backend --test conformance` (Unix,
no root).

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`control/crates/layerfs-sandbox-backend/PROVIDERS.md`](../../../../control/crates/layerfs-sandbox-backend/PROVIDERS.md):
  covered in [Backends](backends.md) and [Adding a backend](adding-a-backend.md).
  Some names there are older than the source (`NativeMacosProvider`,
  `MacosNative`, `native/` folder); the source has `HostProcessProvider`,
  `Runtime::HostProcess` and `backend/host_process/`.
- [`core/docs/architecture/72-owned-sandbox-lifecycle.md`](../../../../core/docs/architecture/72-owned-sandbox-lifecycle.md):
  the Docker owner lifecycle it describes was retired with the docker
  backend (D-8, [Docker machine Sandboxes](docker.md)); historical for its
  R1 proof scope.

---

← [Network: Credentials](../network/credentials.md) · [Up: Control](../README.md) · [Sandboxes: Backends](backends.md) →
