# Backends

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How `layerfs-sandbox-backend` starts one daemon on a machine's kernel and runs
commands beside it, for the two backends [Sandboxes](README.md) use: native
(`host_process`, macOS) and `namespaces` (Linux).

| | |
| --- | --- |
| Source | [`layerfs-sandbox-backend/src`](../../../../control/crates/layerfs-sandbox-backend/src): [`backend/`](../../../../control/crates/layerfs-sandbox-backend/src/backend), [`runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs), [`local/`](../../../../control/crates/layerfs-sandbox-backend/src/local), [`stop.rs`](../../../../control/crates/layerfs-sandbox-backend/src/stop.rs), [`types/`](../../../../control/crates/layerfs-sandbox-backend/src/types), [`bin/layerfs-sandbox-helper`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper) |

A machine's platform fixes its backend and callers never choose it: `native`
on macOS runs `HostProcess`; `native` on Linux and every `lima`,
`apple-container` and `docker` machine run `Namespaces`; Windows has none
([`native/profile.rs`](../../../../control/crates/layerfs-platform/src/platforms/native/profile.rs),
`namespaces` in [`platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs);
[Machines: platforms](../machines/platforms.md)).

## `backend/`: the sealed adapter and stored names

```text
 Backend::stored_name / parse / description / capabilities      kind.rs
        |
        v
 adapter(backend) -> &'static dyn BackendAdapter    adapter.rs: one match
        +-- Backend::HostProcess => &host_process::ADAPTER
        +-- Backend::Namespaces  => &namespaces::ADAPTER

 trait BackendAdapter: Sync     pub(crate), no default methods, no registry
   stored_name()   description()   capabilities()
```

`BackendAdapter` holds only what is fixed per backend. Each adapter builds on
every Unix host, so names and capabilities answer even where the backend
cannot run. Stored names do not follow Rust names; `capabilities.name` is a
different string and is not what records store.

| | `HostProcess` | `Namespaces` |
| --- | --- | --- |
| Stored name | `native` | `namespaces` |
| Description | "Sandboxes run as this user's processes, served over loopback NFS" | "Sandboxes run in Linux namespaces and a cgroup" |
| `capabilities.name` | `native-macos` | `namespaces-linux` |
| `isolation` | `HostProcess` | `MicroVm` (the VM or container holding the kernel is the boundary) |
| `mount_driver` | `LoopbackNfs` | `Fuse` |
| `requires_image` | no | yes |
| `search_path_policy` | `InheritHost` | `FixedContainer` |
| `supports_resource_limits` | no | no |
| `direct_store_access` | yes | yes |
| Launch options | `ProviderLaunchOptions::Local` | `::Namespaces` (identity, environment, image digest, archive) |
| Record | `SandboxRecord::Local` | `SandboxRecord::Namespaces` (local facts + image) |
| Process boundary | named process groups | cgroup v2 directory |
| Source | [`host_process/adapter.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/adapter.rs) | [`namespaces/adapter.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/adapter.rs) |

Capabilities "describe support, not qualification"
([`types/capabilities.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/capabilities.rs)).

## `runtime.rs`: `Runtime` and `Sandbox`

| Enum | Variants | Methods |
| --- | --- | --- |
| `Runtime`: one deployment's choice | `HostProcess(HostProcessProvider)` (macOS), `Namespaces(NamespaceProvider)` (Linux) | `backend`, `capabilities`, `local_directory`, `native_paths`, `launch`, `launch_with_binds`, `reopen`, `reopen_stopped` |
| `Sandbox`: the launched owner | `HostProcess` (macOS and Linux), `Namespaces(Box<_>)` (Linux) | `backend`, `identity`, `endpoint`, `run`, `run_in`, `exec_stop`, `track`, `log_tail`, `inspect`, `stop`, `delete`, `scratch_directory`, `local_record`, `record` |

On Linux, `Sandbox::HostProcess` exists only to reopen, stop and delete a
native Sandbox recorded before 2.0; Linux starts no native Sandbox
([`host_process/mod.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/mod.rs)).
`reopen` starts nothing and checks the recorded identity; `reopen_stopped`
admits only one Delete. `endpoint` of a Sandbox whose daemon (or init) exited
is the stopped observation (`WireFailure::container_stopped`). `inspect`
leaves unknown exit and storage facts `None`.

### Stop and delete

| Step | HostProcess | Namespaces |
| --- | --- | --- |
| `stop(s)` | Detach mounts; SIGTERM, wait `s`; SIGKILL, wait 5 s; then SIGKILL every named process group and the daemon's, wait 5 s | One write to `cgroup.kill`; wait `max(s, 5 s)` for the init to be gone and `populated 0` |
| Second `stop` | refused | refused |
| `delete` before a known stop | refused ("known stop required") | refused |
| `delete` | kill groups if not yet, detach mounts, remove the directory | refuse while the cgroup holds a process; remove cgroup tree, directory and private root |
| Source | [`host_process/sandbox.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/sandbox.rs) | [`namespaces/sandbox.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/sandbox.rs) |

Neither is retried. On `Drop` an owner only reaps a daemon or init it started
that already exited; a running one is left for `stop`.

### Exit codes

| Backend | `ExecExit.code` | `pid` | Source |
| --- | --- | --- | --- |
| HostProcess | the exit code, or 128 + signal | the command's | [`host_process/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/exec.rs) |
| Namespaces | the helper's code, which is the command's or 128 + its signal; 125 is the helper's own failure; a helper ended by a signal (its setup failed, or the Sandbox ended) is a failure, never an exit code | the helper's | [`namespaces/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/exec.rs), [`helper main.rs`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper/main.rs) |

Both: stdout and stderr are copied to the sinks as they arrive in 8 KiB
windows (stderr on a scoped thread); counts are what each sink accepted. A
sink failure closes the pipes, still waits for the exit, and returns the
sink's error. End of stream is never the exit. Stdin is refused
(`Unsupported`); there is no timeout. The exit is the root process's; it does
not prove descendants ended.

## `local/`: one daemon process on this kernel

```text
 <root>/<name>/                0755  one directory per Sandbox
   config/daemon.setup         0600  in a 0700 dir; written once
   config/helper.spec          0600  namespaces only
   overlay/overlay.sqlite            in a 0700 dir (daemon's template)
   overlay/upper/  work/       0700  namespaces only: root's writable layer
   workspaces/                       Workspace mounts
   daemon.log                  0600  daemon stdout + stderr
   tmp/                        1777  commands' scratch and HOME
   groups/                     0700  native: one empty file per group
   bin/layerfs-daemon, layerfs-sandbox-helper   namespaces only, 0755
 <root>/<name>.root/           0755  namespaces only: private root (sibling)
 <root>/images/<64 hex>.2/           namespaces only: unpacked image cache
```

| Phase (`LocalPhase`) | What | A failure keeps |
| --- | --- | --- |
| `Validate` | Name (ASCII letter or digit, then `A-Za-z0-9_.-`, at most 128), absolute non-empty executable, setup 1 to 8192 bytes, non-zero wait, directory absent. | nothing |
| `Create` | One `mkdir` of the directory (a name another start holds fails here), then the parts above. | the directory |
| `Start` | Spawn with `env_clear`, `PATH` from the search path, the provider's variables, its own process group, stdin null, output to `daemon.log`; record pid, start time and boot. | directory and process |
| `Listener` | Read only the appended bytes of `daemon.log` every 10 ms until `LAYERFS_DAEMON_LISTEN <addr>`, the daemon's exit (`Protocol`) or the wait (`TimedOut`). A line over 256 bytes is not the marker; a non-loopback or port-0 address is `Protocol`. | directory and process |

| Fact | Rule | Source |
| --- | --- | --- |
| Root | Absolute, this user's, not group or other writable, `o+x`. `Local::new` (namespaces) also needs every ancestor `o+x`, since commands run as another identity; `Local::native` does not (a macOS home is 0750). | [`local/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/create.rs) |
| Process identity | `LocalProcess { pid, start, boot }`: all three must match to count as alive; it lives in the bridge's `process` module. | [`process/identity.rs`](../../../../engine/crates/layerfs-bridge/src/process/identity.rs) |
| Search path | Linux: `/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin`. macOS: this process's `PATH`, else that plus `/opt/homebrew/bin`. | [`local/search.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/search.rs) |
| OS facts | `local/os/linux`: `mountinfo`, lazy `MNT_DETACH`, the seccomp filter. `local/os/macos`: the `mount` table, `umount -f`. `seccomp.rs` is the crate's only `unsafe`. | [`local/mod.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/mod.rs), [`lib.rs`](../../../../control/crates/layerfs-sandbox-backend/src/lib.rs) |

## `backend/host_process/`: native

The machine is the boundary: daemon and commands share this kernel's
processes, network, users and filesystem. `HostProcessProvider::launch`
accepts only `Local` options whose identity is this process's own uid and gid
and whose environment has no `PATH`
([`host_process/provider.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/provider.rs)).

| Rule | Source |
| --- | --- |
| Each command leads its own process group, named in `groups/<boot 32 hex>-<pid>-<start>` before anything waits on it; a group that cannot be named is killed at once. | [`host_process/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/exec.rs) |
| A name is removed once its group has no member; a process that leaves its group (`setsid`) is followed only when named with `Sandbox::track(pid, start)`. | [`local/groups.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/groups.rs) |
| Groups of another boot, or whose leader id now names another start time, are never signalled. | `end`, `members` in `local/groups.rs` |

## `backend/namespaces/`: Linux namespaces and the helper

```text
 NamespaceProvider::new(root)   refused before any effect unless: root,
   user namespaces blocked, cgroup v2, /dev/fuse, root on a filesystem
   that can hold an overlay upper (not overlay, nfs, cifs, smb3, 9p,
   virtiofs or fuse*)
 launch_with_binds
   check binds; rootfs: unpack the docker-archive once per digest
   prepare: Local parts + copied daemon and helper + spec + upper/work
   cgroup::create /sys/fs/cgroup/layerfs/<id>  (needs cgroup.kill)
   spawn: helper init --spec <spec>
            joins the cgroup; unshare(NEWNS | NEWPID | NEWIPC); private
            v
          helper init-inner --spec <spec>          PID 1 of the namespace
            overlay(lower = image, upper/work = the Sandbox's), /proc,
            /dev (fuse null zero full random urandom tty, pts, shm), /tmp,
            Sandbox directory and each Bind at their own paths; pivot
            v
          layerfs-daemon --config <setup> as root; init reaps
   await_listener on daemon.log
```

| Item | Rule | Source |
| --- | --- | --- |
| `HelperSpec` | One 0600 file of absolute paths (`root`, `sandbox`, `daemon`, `config`, `cgroup`, `lower`, `upper`, `work`) and `bind=<rw\|ro>\t<host>\t<guest>` lines. No secret: the helper never reads the daemon setup. | [`helper_spec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/helper_spec.rs) |
| `Bind` | A sqlite Store's directory, read-write at its own path, so SQLite's WAL and the writers' lock see one inode; none for a PostgreSQL Store. A bind may not hold the runtime root, the Sandbox directory or its private root. | [`types/bind.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/bind.rs), `check_bind` in [`namespaces/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/create.rs) |
| Image | `sha256:<64 hex>`, the archive's config digest; a `docker save` archive with uncompressed layers (gzip or zstd layers refused); unpacked once per digest under `<root>/images` and shared read-only. A reopen unpacks nothing: an image gone is refused. | [`rootfs/mod.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/rootfs/mod.rs), [`types/image.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/image.rs), [`namespaces/provider.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/provider.rs) |
| Helper | `LAYERFS_SANDBOX_HELPER`, else beside the daemon or this executable; copied into the Sandbox's `bin/`, which a reopen uses. | `namespaces/provider.rs` |
| Commands | `helper exec --init <pid> --cgroup <dir> [--command-cgroup <dir>] --uid --gid --cwd -- <argv>`: joins the cgroup, enters the namespaces, becomes uid:gid with no capabilities and no-new-privs, installs the filter below, execs. | [`namespaces/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/exec.rs), [`helper exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper/exec.rs) |
| Command cgroup | With an exec slot, the command (never the helper) moves into `ws-<workspace>/exec-<exec>` just before exec; the helper then writes one byte on a socket it was given as stdin, and only then is the stop handle told. A command that cannot join never runs. | `namespaces/exec.rs`, [`namespaces/cgroup.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/cgroup.rs) |
| End | One `cgroup.kill` ends everything the Sandbox started, whatever session it left; `track` is a no-op. Reopen checks the recorded init and its cgroup membership. | `namespaces/cgroup.rs`, [`namespaces/host.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/host.rs) `in_cgroup` |

A command that made a user namespace could hold its own copy of a Workspace
mount outside the daemon's unmount. Two guards stop it:

| Guard | Rule | Source |
| --- | --- | --- |
| Command filter | Seccomp, installed after no-new-privs: `unshare` and `clone` with `CLONE_NEWUSER` answer `EPERM`, `clone3` answers `ENOSYS` (the C library falls back to `clone`), all else allowed. A foreign ABI (i386 on x86_64, arm on aarch64) is filtered by its own numbers, x32 by the native ones; an unknown ABI is killed. A filter that cannot be installed fails the command. | [`local/os/linux/seccomp.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/os/linux/seccomp.rs) |
| Host check | `user.max_user_namespaces=0`, or `/proc/sys/kernel/seccomp/actions_avail` listing `allow`, `errno` and `kill_process`; else `Unsupported`. The runtime never sets the limit. | [`namespaces/host.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/host.rs) |

### On the `docker` machine

There is no docker backend. The `docker` machine runs `namespaces` Sandboxes
inside its worker container, as a VM's worker does; the container is root,
has `/dev/fuse` and a writable cgroup v2 tree, and the Sandbox root is the
bundle's image archive. Its worker never sets `user.max_user_namespaces`
(that would change the whole Docker host), so the command filter alone blocks
user namespaces. Docker publishes ports only when a container starts, so the
machine refuses a forward and every call goes through the worker
([Machines: platforms](../machines/platforms.md)).

A Sandbox record written by the retired docker backend (`backend` or
`runtime` `docker`, or a started docker-machine record with no `runtime`) is
never reopened: every call but `sandbox.close` is `Unsupported`, asking to
recreate it; `sandbox.close` forgets it and names the container for
`docker rm -f`; `sandbox.create` of that name forgets it first
([`record/sandbox.rs`](../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs),
[`reopen.rs`](../../../../control/crates/layerfs-sandbox/src/reopen.rs),
[`close.rs`](../../../../control/crates/layerfs-sandbox/src/close.rs)).

## `stop.rs`: stop handles

| Handle | Backend | Cancel |
| --- | --- | --- |
| `Stop::Cgroup { path }` | namespaces: `<sandbox cgroup>/ws-<workspace>/exec-<exec>` | SIGTERM to each process in `cgroup.procs`, wait the grace, `cgroup.kill`, wait 5 s for it to be empty |
| `Stop::Group { pgid, leader }` | native: the command's process group, leader by pid, start and boot | SIGTERM to the group, wait the grace, SIGKILL, wait 5 s; a child that called `setsid` escapes it |

`cancel` answers `Ended` (nothing ran, not signalled) or `Signalled(SIGTERM
| SIGKILL)`; a process left after SIGKILL is an error. One attempt; nothing
here times a command out
([`stop.rs`](../../../../control/crates/layerfs-sandbox-backend/src/stop.rs)).

## Shared types and formats

| Module | What | Source |
| --- | --- | --- |
| `types/` | `LaunchRequest`, `ProviderLaunchOptions`, `LocalRecord`, `NamespaceRecord`, `SandboxRecord`, `ExecRequest`, `ExecExit`, `SandboxInspection`, `CommandIdentity`, `Bind`, `Image`, `ContainerId` (a random 64-hex id per Sandbox), `RuntimeError` (`Io`, `Protocol`, `Unsupported`), `WireFailure` (whether attempted). They compile on Windows too. | [`types/`](../../../../control/crates/layerfs-sandbox-backend/src/types) |
| `steps.rs` | Step times of a traced command (`exec.*`, and the helper's `helper.*` via `--trace <file>`). | [`steps.rs`](../../../../control/crates/layerfs-sandbox-backend/src/steps.rs) |
| `postgres_url.rs` | Splits a PostgreSQL URL at its host so a daemon gets the same URL under another host name; no `Debug`, never in a message. | [`postgres_url.rs`](../../../../control/crates/layerfs-sandbox-backend/src/postgres_url.rs) |
| `platform_policy.rs` | Platforms removed in 2.0 (`firecracker`, `microsandbox`, `ssh`): a record naming one is refused, never run. | [`platform_policy.rs`](../../../../control/crates/layerfs-sandbox-backend/src/platform_policy.rs) |
| `json.rs` | A small streaming JSON reader (image manifests) and the quoting `SUPPORT.json` is written with. | [`json.rs`](../../../../control/crates/layerfs-sandbox-backend/src/json.rs) |

## Adding a backend

There is no provider trait, registry or factory: a backend is one folder under
`backend/` (`adapter.rs`, `provider.rs`, `create.rs`, `sandbox.rs`,
`exec.rs`), one enum variant, and one arm in each exhaustive `match`; the
compiler lists every place that still needs it.

| # | Step | Where | Breaks until done |
| --- | --- | --- | --- |
| 1 | Add `Backend::<B>` to `Backend::ALL`. | [`kind.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/kind.rs) | `adapter()` |
| 2 | A `static ADAPTER` with `stored_name`, `description`, `capabilities`, and its arm in `adapter()`. The stored name is permanent. | [`adapter.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/adapter.rs) | — |
| 3 | A `ProviderLaunchOptions` variant if it needs its own inputs, and a `SandboxRecord` variant, so no other provider reopens it. | [`types/options.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/options.rs) | provider refusals |
| 4 | Provider (`launch` or `launch_with_binds`, `reopen`, `reopen_stopped`) and owner (`identity`, `endpoint`, `run_in`, `track`, `log_tail`, `stop`, `delete`, ...); reuse `local/` for a process on this kernel. | `backend/<b>/` | — |
| 5 | `Runtime::<B>` and `Sandbox::<B>` with every arm; refuse binds before any effect if it takes none; its `Stop` form. | [`runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs), [`stop.rs`](../../../../control/crates/layerfs-sandbox-backend/src/stop.rs) | every `match self` |
| 6 | Platforms that use it (`backend` in each `profile.rs`) and their support rows; regenerate `control/SUPPORT.json`. | [`platforms/`](../../../../control/crates/layerfs-platform/src/platforms), [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs) | `SUPPORT.json` comparison |
| 7 | The caller: `deploy.sandbox` value, launch options, recorded `runtime` name and reopen by it. | [`layerfs-sandbox runtime.rs`](../../../../control/crates/layerfs-sandbox/src/runtime.rs), [`launch/local.rs`](../../../../control/crates/layerfs-sandbox/src/launch/local.rs), [`reopen.rs`](../../../../control/crates/layerfs-sandbox/src/reopen.rs) | follow the compiler from `Runtime` |

Each backend keeps: one attempt per operation; a launch failure after an
effect returns the owner; the daemon setup is 0600 and never in `Debug`,
argv, environment or labels; commands run as the nonroot `CommandIdentity`,
cannot create a user namespace and end with the Sandbox; `run` streams to the
sinks and reports the root's exit; the backend waits once for
`LAYERFS_DAEMON_LISTEN <loopback addr>`. A new backend must pass
[`conformance.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/conformance.rs),
whose exhaustive matches (`binds`, `options`, `record`, `runtime`,
`lifecycle`) do not compile until it states its answers; update the expected
lists in [`runtime_enum.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/runtime_enum.rs);
pass the platform kit
(`cargo +1.85.1 test --locked -p layerfs-platform --test conformance`); and
add a lifecycle test of its own gated by `#[cfg(target_os = ...)]`. Commands
are in [Sandboxes: Demonstration](README.md#demonstration).

## Related

- [Sandboxes](README.md) and [Daemon channel and exec](daemon-channel-and-exec.md): the caller of this crate.
- [Machines: platforms](../machines/platforms.md): which platform and host get which backend, and the support table.
- [Engine: Daemon](../../engine/daemon/README.md): the process these backends start.
- [Engine: Kernel routes](../../engine/kernel-routes/README.md): the FUSE and loopback NFS routes the mount drivers name.
