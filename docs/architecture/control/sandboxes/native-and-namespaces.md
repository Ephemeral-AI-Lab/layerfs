# Native and namespaces

[Architecture](../../README.md) › [Control](../README.md) › [Sandboxes](README.md) › Native and namespaces

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Local-process Sandboxes: native and namespaces.

| | |
| --- | --- |
| Component | [Sandboxes](README.md) |
| Source | [`control/crates/layerfs-sandbox-backend/src/local`](../../../../control/crates/layerfs-sandbox-backend/src/local), [`control/crates/layerfs-sandbox-backend/src/backend/host_process`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process), [`control/crates/layerfs-sandbox-backend/src/backend/namespaces`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces), [`control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper), [`control/crates/layerfs-sandbox-backend/src/helper_spec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/helper_spec.rs) |

Both backends start the daemon as a process on the machine's own kernel.
`host_process` (stored `"native"`, macOS) isolates nothing: the machine is
the boundary. `namespaces` (Linux) is the same local lifecycle with mount,
PID and IPC namespaces and a cgroup added.

## Local process plumbing

```text
 <root>/<name>/                 0755  one directory per Sandbox
   config/daemon.setup          0600  in a 0700 dir; written once
   overlay/overlay.sqlite             in a 0700 dir (daemon's template)
   workspaces/                        Workspace mounts (daemon makes 0755)
   daemon.log                   0600  daemon stdout + stderr
   tmp/                         1777  commands' scratch, goes with the Sandbox
   groups/                      0700  one empty file per named process group
   bin/  root/  upper/  work/  spec   namespaces only (below)
```

`Local::create` ([`local/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/create.rs)):

| Phase (`LocalPhase`) | What | Failure keeps |
| --- | --- | --- |
| `Validate` | Name (Docker rule, at most 128), absolute non-empty executable, setup 1 to 8192 bytes, non-zero wait, directory absent. | nothing |
| `Create` | One `mkdir` of the directory (a name another start holds fails here); `prepare` makes the parts above. | the directory |
| `Start` | Spawn `<daemon> --config <setup>` with `env_clear`, `PATH` from `search_path`, the provider's variables, its own process group, stdin null, output to `daemon.log`. Record pid, start time and boot. | directory and process |
| `Listener` | `await_listener` reads only appended bytes of `daemon.log` until `LAYERFS_DAEMON_LISTEN <loopback addr>`, the daemon's exit, or the wait. | directory and process |

| Fact | Rule | Source |
| --- | --- | --- |
| Root | Absolute, owned by this user, not group or other writable, `o+x`. `Local::new` also needs every ancestor `o+x` (commands run as another identity); `Local::native` does not (a macOS home is 0750). | [`local/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/create.rs) |
| Process identity | `LocalProcess { pid, start, boot }`: all three must match to count as alive; a zombie is not alive; a record without a boot is never signalled. | [`local/process.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/process.rs) |
| Search path | Linux: `/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin`. macOS: this process's `PATH`, else that plus `/opt/homebrew/bin`. | [`local/search.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/search.rs) |
| OS facts | `local/os/linux` (`/proc`, `mountinfo`, lazy `MNT_DETACH`) or `local/os/macos` (`proc_pidinfo`, `mount`, `umount -f`). The macOS file is the crate's only `unsafe`. | [`local/mod.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/mod.rs), [`lib.rs`](../../../../control/crates/layerfs-sandbox-backend/src/lib.rs) |

`HostProcessProvider::launch` accepts only `ProviderLaunchOptions::Local`
whose command identity is this process's own uid and gid, and no `PATH` in
the environment ([`host_process/provider.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/provider.rs)).

## Process groups

A container's processes end with it. A local Sandbox's would outlive it,
so each command leads its own process group and the Sandbox names it.

```text
 run(request)
   spawn command, process_group(0)          leader = pid
   groups::name: groups/<boot 32 hex>-<pid>-<start>        (before any wait)
   ... streams, wait ...
   groups::forget_if_empty                  only when no member is left

 stop / delete
   groups::end: every named group + the daemon's own, this boot only
     killpg(SIGKILL) where members remain  -> wait <= 5 s for none
     a leader id now held by another start time = reused: skipped
```

| Rule | Source |
| --- | --- |
| A group is named before anything waits on its leader; failing to name it kills it at once. | [`host_process/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/exec.rs) |
| A process that leaves its group (`setsid`) is followed only when named with `Sandbox::track(pid, start_time)`. | [`local/groups.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/groups.rs) |
| Groups of another boot are not signalled: they ended with that boot. | `end` in [`local/groups.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/groups.rs) |

## Listener and streams

| Item | Rule | Source |
| --- | --- | --- |
| Startup line | `LAYERFS_DAEMON_LISTEN ` then a loopback socket address with a non-zero port; a line longer than 256 bytes is not the marker; a non-loopback address is `Protocol`. Looks every 10 ms. | [`local/listener.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/listener.rs) |
| Daemon exits first | `Protocol("local daemon exited before its listener line")`. | same |
| Wait ends | `TimedOut`. Nothing is retried. | same |
| Command streams | Two pipes, each copied to its sink in 8 KiB windows; the count is what the sink accepted. | [`local/streams.rs`](../../../../control/crates/layerfs-sandbox-backend/src/local/streams.rs) |

The Docker backend reads the same marker from the container's log stream
([Docker](docker.md#containers)).

## Namespaces

```text
 NamespaceProvider::launch_with_binds          (as root in the machine)
   host checks: root, user.max_user_namespaces = 0, cgroup v2,
                /dev/fuse, overlay upper dir usable
   rootfs::ensure: unpack the docker-archive once per image digest
   prepare: Local parts + bin/layerfs-daemon, bin/layerfs-sandbox-helper
            (copied, 0755), root/ (0755), upper/ work/ (0700),
            spec (0600)
   cgroup::create  /sys/fs/cgroup/layerfs/<sandbox id>  (needs cgroup.kill)
   spawn: helper init --spec <spec>
            | joins the cgroup; unshare(NEWNS | NEWPID | NEWIPC)
            v
          helper init-inner --spec <spec>          PID 1 of the namespace
            | overlay: lower = image, upper/work = Sandbox's own
            | bind the Sandbox directory and each Bind at its own path
            | private /proc, /dev (fuse null zero full random urandom
            | tty), /tmp; pivot_root
            v
          layerfs-daemon --config <setup> as root; init reaps
   await_listener on daemon.log
```

| Item | What | Source |
| --- | --- | --- |
| `HelperSpec` | One 0600 file of absolute paths: root, sandbox, daemon, config, cgroup, lower, upper, work, and `bind=<rw\|ro>\t<host>\t<guest>` lines. No secret: the helper never reads the setup. | [`helper_spec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/helper_spec.rs) |
| `Bind` | A sqlite Store's directory, read-write at its own path, so SQLite's WAL and the writers' lock see one inode. None for a PostgreSQL Store. A bind may not hold the runtime root or the Sandbox directory. | [`types/bind.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/bind.rs), `check_bind` in [`namespaces/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/create.rs) |
| Image | A `docker save` archive named by its config digest; compressed layers are refused; unpacked once per digest and shared read-only. | [`rootfs/mod.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/rootfs/mod.rs) |
| Commands | `helper exec --init <pid> --cgroup <dir> --uid --gid --cwd -- <argv>`: joins the cgroup, enters the namespaces, becomes uid:gid with no capabilities and no-new-privs. | [`namespaces/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/exec.rs), [`bin/…/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper/exec.rs) |
| End | One write to `cgroup.kill` ends everything the Sandbox started, whatever session it left. `track` is a no-op here. | [`namespaces/cgroup.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/cgroup.rs), [`runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs) |
| Delete | Removes the cgroup and directory after a known stop; a populated cgroup is refused. The shared Store is never touched. | [`namespaces/sandbox.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/sandbox.rs) |

A host that cannot meet a check is refused before any effect, never given
a weaker Sandbox ([`namespaces/host.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/host.rs)).
The machine setup sets `user.max_user_namespaces=0` in every VM guest
([Network: Workers](../network/workers.md#worker-setup)); this runtime only
checks it.

## Related

- [Commands and streams](commands-and-streams.md): `run`, exit codes and stop for both.
- [Backends](backends.md): capabilities and stored names.
- [Network: Credentials](../network/credentials.md): what the 0600 setup holds.
- [Engine: Daemon](../../engine/daemon/README.md): the process these backends start.

---

← [Sandboxes: Docker](docker.md) · [Up: Sandboxes](README.md) · [Sandboxes: Commands and streams](commands-and-streams.md) →
