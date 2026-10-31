# Sandbox runtimes

Status: Current general guide.

`backend::runtime` holds the concrete runtime choice and the launched owner.
There are no provider or instance traits: each backend is an ordinary concrete
type and the only dispatch is one `match` per method.

- `Runtime` is one deployment's selection and is kept for every Sandbox it
  starts: `Docker(DockerProvider)` (the default), `MacosNative(NativeMacosProvider)`,
  `LinuxNative(NativeLinuxProvider)`, `Namespaces(NamespaceProvider)` (Linux
  only) and `Windows`, a placeholder with no backend.
  `Runtime::{capabilities, local_directory, native_paths, launch, reopen,
  reopen_stopped}` dispatch to them.
- `Sandbox` is what `Runtime::launch`/`reopen` return: `Docker(docker::Sandbox)`,
  `Native(native::Sandbox)` or `Namespaces(Box<namespaces::Sandbox>)`, with
  `identity`, `endpoint`, `run`, `track`, `log_tail`, `inspect`, `stop`,
  `delete`, `scratch_directory`, `local_record` and `record`.

`layerfs_sandbox::{Runtime, Sandbox}` re-export both. `provider` keeps the
inputs and observations they share: `LaunchRequest`, `ProviderLaunchOptions`,
`LocalRecord`, `DockerRecord`, `SandboxRecord`, `ExecExit`,
`SandboxInspection`, `ProviderCapabilities` and the `LaunchPhase`/`LaunchFailure`
pair. The runtime layer has no daemon control dependency. `backend::HelperSpec`
is the namespace helper's line format; it is compiled and tested on every
platform, while only the helper that reads it is Linux only.

## Docker

`Runtime::Docker(DockerProvider::new(docker))` adapts the existing Engine client
and `SandboxRequest`. It shares its lifecycle admission, API 1.48–1.54
negotiation cache, streamed private archive, logs, exact deployment inspection,
and explicit stop/delete. Call `runtime.launch(LaunchRequest { .. })` with the
executable, private configuration, name, explicit startup wait, and
`ProviderLaunchOptions::Docker { image, store_volume, port, connect_host }`;
Docker validates the immutable image digest, existing volume name, port,
connect_host format and labels before any Engine effect. The runtime does not
decode daemon configuration or perform authentication.

`Sandbox::run` streams stdout/stderr to caller sinks with bounded windows and
returns `ExecExit` only after one inspection knows the root process exit. It
rejects `stdin=true` before effects: interactive users keep the existing
concrete `create_exec/start/into_parts` stream API. EOF alone is not an exit and
no automatic poll follows an unknown exit. `track` needs no host registration
because container teardown owns descendants; scratch is `/tmp` and
`local_record()` is absent. `inspect` checks the recorded deployment but neither
adopts it nor changes lifecycle admission; unknown PID/exit/storage fields stay
absent.

## Native

`Runtime::MacosNative(NativeMacosProvider::new(root))` runs the daemon as this
user and serves loopback NFS; `Runtime::LinuxNative(NativeLinuxProvider::new(root))`
runs a root daemon serving FUSE with ordinary commands as a nonroot identity.
Each refuses the wrong platform before root access, and Linux refuses a non-root
caller the same way. `backend::native` owns the one lifecycle both use; only the
platform facts differ:

- `backend::macos`: `darwin.rs` (`proc_pidinfo`, `sysctl`, the `mount` table and
  `umount -f`) and `mounts.rs` (force detach, deepest first).
- `backend::linux`: `process.rs` (`/proc/<pid>/stat`, the boot id and the
  process table) and `mounts.rs` (`/proc/self/mountinfo`, lazy `MNT_DETACH`).

The provider validates the command identity, the environment it is given and
the daemon executable, then starts one daemon in a private directory with the
platform's kernel marker (`LAYERFS_KERNEL=fuse` on Linux, `nfs` on macOS); it
rejects conflicting overrides. Launch preserves the original single-attempt
process lifecycle and partial owner on failure. `inspect` reports exact process
liveness; unknown exit/storage facts stay absent. Records remain the Node
binding's responsibility; runtimes neither scan directories nor infer project
ownership.

## Namespaces (Linux)

`Runtime::Namespaces(NamespaceProvider::new(root))` is the same local root and
daemon control as `LinuxNative`, with each Sandbox's daemon in its own mount,
PID and IPC namespaces and its own cgroup v2 directory. The VM that holds this
kernel is the boundary (one per Store); the namespaces keep Sandboxes on one
Store out of each other.

- `backend::namespaces::create` makes the Sandbox's private directory, copies
  the daemon to `<directory>/bin/layerfs-daemon` (0755), writes the setup (0600)
  and a `HelperSpec`, makes `/sys/fs/cgroup/layerfs/<id>`, then spawns the
  helper's init stage.
- `backend::namespaces::sandbox` owns the launched handle: the init process's
  identity, `cgroup.kill` for stop (a `SIGKILL` to everything the Sandbox
  started, `populated 0` awaited), cgroup and directory removal on delete, and
  reopen by the recorded process plus its cgroup membership.
- `backend::namespaces::exec` runs one command through the helper's exec stage.
- `src/bin/layerfs-sandbox-helper` performs the calls that act on the calling
  process, so the product crate stays `unsafe`-free: the init stage creates the
  namespaces, joins the cgroup and starts the inner stage (PID 1), which mounts
  the private root (the VM's userland read-only, a private `/proc`, `/dev`,
  `/dev/pts`, `/dev/shm`, `/tmp`, plus the Sandbox's directory and the Store's
  directory at their own paths), pivots, starts the daemon and reaps; the exec
  stage enters the namespaces, sets no-new-privs and runs the command as the
  requested uid:gid (its own exit is a signal, never a command's status).

Preconditions are checked before any effect and refused with the existing
`RuntimeError::Unsupported`, never replaced by a weaker Sandbox: root, the
unified cgroup v2 hierarchy (and `cgroup.kill`), `/dev/fuse`, and
`user.max_user_namespaces=0` — without the last one a command could unshare a
user namespace and hold its own copy of the Workspace mount. The runtime never
sets that limit itself. There is no image: the private root binds the VM's own
userland, so the Sandbox runs the daemon and libraries the VM already has.
`ProviderLaunchOptions::Namespaces { command_identity, environment, store }`
names the Store file whose directory is bound in; `SandboxRecord::Namespaces`
keeps the same physical facts as `LocalRecord`.

## Windows

`Runtime::Windows` is a placeholder: `capabilities`, `local_directory`,
`native_paths`, `launch`, `reopen` and `reopen_stopped` all refuse with the
existing `RuntimeError::Unsupported` before any effect, and no `Sandbox` variant
exists for it. No Windows host runtime is implemented; see
[`src/backend/windows/README.md`](src/backend/windows/README.md) for the planned
routes.

## Failure and records

A `LaunchFailure` retains the acknowledged `Sandbox`, when any. Its cause may be
`RuntimeError::Docker(Box<docker::ProviderFailure>)`: match the typed variant for
the original Create/Upload/Start/Listener receipt, partial IDs, upload work and
pending source bytes. Upload is within existing `LaunchPhase::Create`; inspect
the typed cause to distinguish it. A partially decoded Create ID is never
adopted, and retaining a handle permits an explicit later observation, never a
replay of creation or Start.

`Runtime::reopen(id, &SandboxRecord)` validates the exact existing deployment or
process and endpoint without starting it; `reopen_stopped` requires the caller's
prior exact stopped observation and admits only non-forcing Delete. Both accept
`SandboxRecord`, which represents `LocalRecord`, `Namespaces(LocalRecord)` or
`DockerRecord`; each runtime refuses another runtime's record before effects. `Sandbox::record()` exposes the persisted deployment facts. No disk
format changes occur: a record without a `runtime` field is Docker's.
