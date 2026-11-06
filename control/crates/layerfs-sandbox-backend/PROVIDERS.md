# Sandbox runtimes

Status: Current general guide.

`src/runtime.rs` holds the concrete runtime choice and the launched owner.
There are no public provider or instance traits: each backend is an ordinary
concrete type and the dispatch is one `match` per method. What is fixed per
backend (its stored name, description and capabilities) is the crate-private,
sealed `BackendAdapter` (`src/backend/adapter.rs`, #535 G2): each backend
folder holds its `ADAPTER`, reached through one exhaustive `adapter(Backend)`
match, and `Backend::{stored_name, parse, description, capabilities}` read it.
Stored names do not follow the Rust names: `HostProcess` is `"native"` (G6).

There are two backends: `namespaces` (every Linux kernel: VMs, `native` on
Linux and the `docker` machine's worker container) and `host_process`
(macOS). The docker backend (one Docker container per Sandbox), its Engine
HTTP client `src/docker_engine/` and `examples/engine_streams.rs` were
deleted by owner decision D-8 (layerfs-project#111): the `docker` platform
now runs `namespaces` inside its worker container, as a VM's worker does.
`Backend::Docker`, `Runtime::Docker`, `Sandbox::Docker`, `DockerRecord`,
`SandboxRecord::Docker`, `ProviderLaunchOptions::Docker`, `ExecId`,
`ExecInspection`, `ErrorDiagnostic`, `RuntimeError::{Docker, Http}`,
`WireFailure::{observed_exec, requested_exec, error_body}`,
`WireFailure::container_gone` and the Engine-only `ExecRequest::check` went
with it. A Sandbox recorded under the docker backend is
refused by the caller (`layerfs-sandbox`) with a message to recreate it;
`sandbox.close` only forgets it.

Each backend has one folder under `src/backend/` (named `platform/` until
#533 step 5), named as `LAYERFS_SANDBOX` names it: `native/` (with
its OS facts in `native/linux/` and `native/macos/`) and `namespaces/`
(Linux). The modules are
private: callers use the crate root and one public module per backend,
`layerfs_sandbox_backend::native` and `::namespaces` (#527 step
9b; they were under the crate's `backend` module). The root re-exports `Runtime`,
`Sandbox`, `LaunchFailure`, the shared types, the providers
(`NativeMacosProvider` on macOS, `NamespaceProvider` on
Linux) and `HelperSpec`. `NativeLinuxProvider` and `Runtime::LinuxNative`
were removed in 2.0 (#533 O27).

The same file names hold the same roles in every backend: `provider.rs` (what
`Runtime` holds), `create.rs` (start one Sandbox), `sandbox.rs` (a running
Sandbox: stop, delete, inspect) and `exec.rs` (one command); the listener
and streams are the shared local ones. Native reads its
OS facts through the alias `os` (`native/linux/` or `native/macos/`). Every
backend's `run` returns the shared `ExecExit` (native's `LocalExit` is gone),
and the `NativeLinuxInstance` and `NativeMacosInstance`
aliases are removed (#527 step 9c).

- `Runtime` is one deployment's selection and is kept for every Sandbox it
  starts: `MacosNative(NativeMacosProvider)`
  (macOS only) and `Namespaces(NamespaceProvider)` (Linux only).
  `Runtime::{local_directory, native_paths, launch, reopen, reopen_stopped}`
  dispatch to them; `Runtime::backend` names the backend and
  `Runtime::capabilities` reads its adapter.
- `Sandbox` is what `Runtime::launch`/`reopen` return:
  `Native(native::Sandbox)` or `Namespaces(Box<namespaces::Sandbox>)`, with
  `identity`, `endpoint`, `run`, `track`, `log_tail`, `inspect`, `stop`,
  `delete`, `scratch_directory`, `local_record` and `record`.

`layerfs_sandbox_backend::{Runtime, Sandbox, LaunchFailure}` re-export them;
`LaunchFailure` (which holds a `Sandbox`) lives beside them in
`src/runtime.rs`. The inputs and observations every backend shares are in
the crate's private `types/` and re-exported at the root:
`LaunchRequest`, `LaunchPhase`, `ProviderLaunchOptions`, `LocalRecord`,
`NamespaceRecord`, `SandboxRecord`, `ExecExit`,
`SandboxInspection` and `ProviderCapabilities` (#527 step 9a; they were under
the crate's `provider` module). The runtime layer has no daemon control
dependency. `HelperSpec` (`src/helper_spec.rs`)
is the namespace helper's line format; it is compiled and tested on every
Unix platform, while only the helper that reads it is Linux only. `src/json/`
is a small streaming JSON reader (the namespace backend's image manifests)
and the string quoting `SUPPORT.json` is written with.

## Machines

The machine layer moved to its own crate in #535 phase 1:
`control/crates/layerfs-platform`. It is host only and depends on this crate
for the `Backend` enum, the shared types and `json`. See that crate's
`src/lib.rs` for the kind-to-backend table and the `create`, `connect`,
`start`, `stop`, `remove` and `status` calls, and `control/AGENTS.md` for
the layer.

## Native

`Runtime::MacosNative(NativeMacosProvider::new(root))` runs the daemon as this
user and serves loopback NFS. It is compiled on macOS only. Linux has no native
provider (#533 O27): a Linux host runs local Sandboxes with `namespaces`, which
builds on `native`'s shared lifecycle (`Local`, `LocalProcess`, `listener`,
`search`, `streams`) and its Linux facts. On Linux, `native::Sandbox` remains
only to reopen, stop and delete a Sandbox recorded by the Linux native runtime
before 2.0. Only the platform facts differ:

- `native/macos/`: `darwin.rs` (`proc_pidinfo`, `sysctl`, the `mount` table and
  `umount -f`) and `mounts.rs` (force detach, deepest first).
- `native/linux/`: `process.rs` (`/proc/<pid>/stat`, the boot id and the
  process table) and `mounts.rs` (`/proc/self/mountinfo`, lazy `MNT_DETACH`,
  and the filesystem under a path for the namespaces overlay). Namespaces
  reads the boot id, `stat` and `filesystem`; the process table and detach
  serve the stop and delete of a recorded pre-2.0 Linux native Sandbox.

The provider validates the command identity, the environment it is given and
the daemon executable, then starts one daemon in a private directory. The
kernel route travels in the daemon setup the caller sends (`nfs` for a
host_process daemon, `fuse` for namespaces); the daemon refuses
at startup any route other than `fuse` or `nfs` (only `nfs` on macOS). Launch preserves the original single-attempt
process lifecycle and partial owner on failure. `inspect` reports exact process
liveness; unknown exit/storage facts stay absent. Records remain the Node
binding's responsibility; runtimes neither scan directories nor infer project
ownership.

## Namespaces (Linux)

`Runtime::Namespaces(NamespaceProvider::new(root))` is the same local root and
daemon control as the native runtime, with each Sandbox's daemon in its own mount,
PID and IPC namespaces and its own cgroup v2 directory. The VM that holds this
kernel is the boundary (one per Store; on the `docker` machine, its worker
container on Docker's kernel); the namespaces keep Sandboxes on one Store out
of each other.

- `namespaces/create.rs` makes the Sandbox's private directory, copies
  the daemon to `<directory>/bin/layerfs-daemon` (0755), writes the setup (0600)
  and a `HelperSpec`, makes `/sys/fs/cgroup/layerfs/<id>`, then spawns the
  helper's init stage.
- `namespaces/sandbox.rs` owns the launched handle: the init process's
  identity, `cgroup.kill` for stop (a `SIGKILL` to everything the Sandbox
  started, `populated 0` awaited), cgroup and directory removal on delete, and
  reopen by the recorded process plus its cgroup membership.
- `namespaces/exec.rs` runs one command through the helper's exec stage.
- `src/bin/layerfs-sandbox-helper` performs the calls that act on the calling
  process: the init stage creates the
  namespaces, joins the cgroup and starts the inner stage (PID 1), which mounts
  the private root (the VM's userland read-only, a private `/proc`, `/dev`,
  `/dev/pts`, `/dev/shm`, `/tmp`, plus the Sandbox's directory and the Store's
  directory at their own paths), pivots, starts the daemon and reaps; the exec
  stage enters the namespaces, sets no-new-privs and runs the command as the
  requested uid:gid under the seccomp filter that refuses it a user namespace
  (`local::deny_user_namespaces`, in the audited
  `src/local/os/linux/seccomp.rs`; its own exit is a signal, never a
  command's status). A filter that cannot be installed fails the command.

Preconditions are checked before any effect and refused with the existing
`RuntimeError::Unsupported`, never replaced by a weaker Sandbox: root, the
unified cgroup v2 hierarchy (and `cgroup.kill`), `/dev/fuse`, and user
namespaces blocked — `user.max_user_namespaces=0`, or a kernel whose
`/proc/sys/kernel/seccomp/actions_avail` offers the command filter's actions
(`allow`, `errno`, `kill_process`). Without that a command could unshare a
user namespace and hold its own copy of the Workspace mount. The runtime never
sets the limit itself; a VM guest's setup does, and the `docker` machine's
worker never does (it would change the whole Docker host). The private root is
an overlay on an image unpacked once per digest from a `docker save` archive
(`namespaces/rootfs/`).
`ProviderLaunchOptions::Namespaces { command_identity, environment, image,
archive }` with `Runtime::launch_with_binds` binds a sqlite Store file's
directory in (`Bind::store`), or nothing for a PostgreSQL Store; `SandboxRecord::Namespaces`
keeps the same physical facts as `LocalRecord`.

## Windows

There is no Windows backend and no Windows variant (#527 D2). Windows runs the
Linux build: through the WSL2 worker on a local placement, or in a Linux VM
(`vm:<provider>`). The backends and `Runtime` are Unix only; the crate's shared
types still compile for Windows, and a build-only CI lane reports whether they
do (#527 D6).

## Failure and records

A `LaunchFailure` retains the acknowledged `Sandbox`, when any. Retaining a
handle permits an explicit later observation, never a replay of creation or
start.

`Runtime::reopen(id, &SandboxRecord)` validates the exact existing deployment or
process and endpoint without starting it; `reopen_stopped` requires the caller's
prior exact stopped observation and admits only non-forcing Delete. Both accept
`SandboxRecord`, which represents `LocalRecord` or `Namespaces(LocalRecord)`;
each runtime refuses another runtime's record before effects.
`Sandbox::record()` exposes the persisted deployment facts.
