# Backends

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

How `layerfs-sandbox-backend` starts one daemon on a machine's kernel and runs
commands beside it, for the two backends [Sandboxes](README.md) use:
`process` (macOS) and `namespaces` (Linux).

| | |
| --- | --- |
| Source | [`layerfs-sandbox-backend/src`](../../../../control/crates/layerfs-sandbox-backend/src): [`runtime/`](../../../../control/crates/layerfs-sandbox-backend/src/runtime), [`shared/`](../../../../control/crates/layerfs-sandbox-backend/src/shared), [`backend/`](../../../../control/crates/layerfs-sandbox-backend/src/backend), [`support/`](../../../../control/crates/layerfs-sandbox-backend/src/support), [`bin/layerfs-sandbox-helper`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper) |

The crate is grouped by owner:

| Folder | What it holds |
| --- | --- |
| [`runtime/`](../../../../control/crates/layerfs-sandbox-backend/src/runtime) | The backend-neutral surface: `dispatch.rs` (`Runtime`, `Sandbox`, matched over the backends), `cancel.rs` (one command's stop handle), `trace.rs` (a traced command's step times) and `types/` (what the backends share). |
| [`shared/`](../../../../control/crates/layerfs-sandbox-backend/src/shared) | The process lifecycle both backends use: the daemon start (`daemon.rs`), its listener, the search path, streams and confined reads. |
| [`backend/process/`](../../../../control/crates/layerfs-sandbox-backend/src/backend/process) | Everything only `process` uses, its OS facts included: the provider and Sandbox, process groups (`groups.rs`) and `macos/` (the `mount` table, `umount -f`, confined reads). macOS runs only `process`. |
| [`backend/namespaces/`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces) | Everything only `namespaces` uses, its OS facts included: the provider and Sandbox, the cgroup, the helper spec (`spec.rs`), host checks (`host_checks.rs`), the command filter (`seccomp.rs`), the root (`rootfs/`), the Sandbox's network (`network/`) and `linux/` (`mountinfo`, confined reads). Linux runs only `namespaces`. |
| [`support/`](../../../../control/crates/layerfs-sandbox-backend/src/support) | Small formats other crates read: `json/`, `postgres_url.rs`, `removed_platforms.rs`. |

A machine's platform fixes its backend and callers never choose it: `process`
on macOS runs `Process`; `process` on Linux and every `lima`,
`apple-container` and `docker` machine run `Namespaces`; Windows has none
([`native/profile.rs`](../../../../control/crates/layerfs-platform/src/platforms/host/profile.rs),
`namespaces` in [`platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs);
[Machines: platforms](../machines/platforms.md)).

## `backend/`: the sealed adapter and stored names

```text
 Backend::stored_name / parse / description / capabilities      kind.rs
        |
        v
 adapter(backend) -> &'static dyn BackendAdapter    adapter.rs: one match
        +-- Backend::Native      => &native::ADAPTER
        +-- Backend::Namespaces  => &namespaces::ADAPTER

 trait BackendAdapter: Sync     pub(crate), no default methods, no registry
   stored_name()   description()   capabilities()
```

`BackendAdapter` holds only what is fixed per backend. Each adapter builds on
every Unix host, so names and capabilities answer even where the backend
cannot run. Stored names do not follow Rust names; `capabilities.name` is a
different string and is not what records store.

| | `Process` | `Namespaces` |
| --- | --- | --- |
| Stored name | `process` | `namespaces` |
| Description | "Sandboxes run as this user's processes, served over loopback NFS" | "Sandboxes run in Linux namespaces and a cgroup" |
| `capabilities.name` | `native-macos` | `namespaces-linux` |
| `isolation` | `HostProcess` | `Namespaces`: mount, PID and IPC namespaces, a cgroup and a network namespace for its commands on the kernel that runs it (a VM's, or the host's own on a Linux host); no VM of its own |
| `mount_driver` | `LoopbackNfs` | `Fuse` |
| `requires_image` | no | yes |
| `search_path_policy` | `InheritHost` | `FixedContainer` |
| `supports_resource_limits` | no | no |
| `direct_store_access` | yes | yes |
| Launch options | `ProviderLaunchOptions::Local` | `::Namespaces` (identity, environment, image digest, archive, closed ports) |
| Record | `SandboxRecord::Local` | `SandboxRecord::Namespaces` (local facts + image + network) |
| Process boundary | named process groups | cgroup v2 directory |
| Kernel requirements | none | cgroup v2 with `cgroup.kill` (Linux 5.14), and `clone3` with `CLONE_INTO_CGROUP` allowed (Linux 5.7; not answered `ENOSYS` or `EPERM` by a seccomp filter around the worker): every command is created inside its cgroup, with no fallback |
| Source | [`native/adapter.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/process/adapter.rs) | [`namespaces/adapter.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/adapter.rs) |

### Every `namespaces` Sandbox's own network

A `process` Sandbox's commands use this host's network, its `127.0.0.1`
included (`Process`, macOS: the documented exception). Every
`namespaces` Sandbox's commands run in a network namespace of their own
(2026-11-24, spec "every Sandbox gets its own private network"): they bind
any port, `127.0.0.1:3000` in two Sandboxes at once; they reach the
internet, this host and the machine's other services as before; they cannot
reach another Sandbox's commands, and LayerFS's ports on the machine's own
addresses are closed to them: every daemon and a loopback worker listen on
the machine's `127.0.0.1`, which is not theirs, and the `inet layerfs`
input rule drops a non-loopback worker's port. LayerFS's forwards on the
host are not closed (Host-side forwards, below; owner to confirm). Nothing
chooses it: no call option, flag, record field or setting turns it off, and
`sandbox.create` has no new input. The daemon stays on the machine's
network, so its loopback listener, the worker's dial, lima's forward, the
daemon's Store connection (the URL as configured) and its name resolution
(the machine's own resolver file) are what they were.

| Machine | Where the namespace, its host end and the table live | Commands reach |
| --- | --- | --- |
| `process` on macOS (`Process`) | none | this host's network, its `127.0.0.1` included |
| `process` on a Linux host | this host's network namespace | out through this host (masqueraded); not this host's `127.0.0.1` |
| `docker` on a Linux host | this host's network namespace (`--network host` worker) | as `process` on Linux |
| `docker` on macOS (Docker Desktop) | the worker container's namespace | out through the worker's bridge; not the worker's port 7070 on the container's addresses (its published port on this Mac stays reachable by `host.docker.internal`) |
| `lima` | the VM's | out through the VM; not the VM's `127.0.0.1` (its worker, its daemons; their forwards on this Mac stay reachable by `host.lima.internal`) |
| `apple-container` | the container's in its VM | out through the container; not the worker's port 7070 on the container's addresses |

```text
 machine netns            | Sandbox netns (one per Sandbox)
   daemon 127.0.0.1:<p>   |   commands: lo, eth0 10.213.0.2k+1/31,
   worker, lima ssh -L    |   default via 10.213.0.2k
   lfs<id12> 10.213.0.2k/31 ===== veth ===== eth0
   table inet layerfs: forward (siblings dropped, out accepted, replies
   only in), input (the launcher's non-loopback port dropped), postrouting
   (pool masqueraded)
```

| Item | Rule | Source |
| --- | --- | --- |
| Holder | The outer `helper init` stage (deviation D1): it is the process the launcher records (pid, start time, boot), checks at reopen and names to every exec as `--init`; it is in the Sandbox's leaf, so one `cgroup.kill` ends the namespace with everything else, and it exits when the daemon's stage does. No nsfs bind mount is made, so a crash leaves nothing to unmount. | [`helper init.rs`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper/init.rs), [`helper netns.rs`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper/netns.rs) |
| Order (D18) | After its mount, pid and IPC `unshare` and the private mount propagation, outer keeps a handle on the machine's namespace, takes the lock, plans a slot, `unshare(CLONE_NEWNET)`, opens the new namespace, creates the veth pair from a routing socket opened in the machine's namespace (peer `eth0` created directly in the new one with `IFLA_NET_NS_FD`), addresses the host end, unlocks, configures the inside, writes `<sandbox>/network`, then `setns` back to the machine's namespace, spawns `init-inner` (which with the daemon inherits the machine's network) and `setns` into the Sandbox's namespace again to hold it. All host-path work (`/proc/self/ns`, `/proc/sys`, the lock) is done before `init-inner` starts, because its `pivot_root` moves the root of every process in the mount namespace, outer's included. A failure before the spawn ends the stage and the start, so the launcher's listener wait implies a wired network. Only a Sandbox with a written resolver file has a gate between the stages (Resolver, below): one socket pair, `init-inner`'s end its standard input; `init-inner` builds, pivots and shares the root, says so, and starts the daemon only once outer answered that the commands' mount view is made; a failure on either side ends the Sandbox before its daemon starts. | `helper init.rs`, `helper netns.rs` |
| Names | Host end `lfs` + the first 12 hex digits of the `ContainerId` (15 bytes, the kernel's limit); guest end `eth0`, never in the machine's namespace. | [`network/pool.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/network/pool.rs) `link_name` |
| Pool and slot (D3, D4) | One /16 per network namespace: the pool of the `lfs*` links already there (all must agree, else `Unavailable` naming both), else the first of `10.213.0.0/16`, `10.214.0.0/16`, `10.215.0.0/16`, `172.30.0.0/16`, `172.31.0.0/16` that overlaps no route (the default excepted) and no address; none free, or a pool in use that now clashes, is `Unavailable` naming the routes in the way; nothing is renumbered. The init stage checks the pool and the link again under the lock; a machine that changed in between (another start, a new route) ends it with a `LAYERFS_SANDBOX_UNAVAILABLE <reason>` log line, which the launcher reads after the listener wait failed and answers `Unavailable` with, as its own check does (spec §3.2), not `Failed`. One /31 per Sandbox, host `.2k`, guest `.2k+1`: the lowest whose host address is on no `lfs*` link. The address on the link is the allocation, freed by the kernel with the namespace; a lingering link keeps its slot. The kernel is the record, not the machine record (owner to confirm): on a Linux host several machines and homes share one namespace, and only its own links are seen by all of them. | `network/pool.rs`, [`network/links.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/network/links.rs), [`network/setup.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/network/setup.rs) |
| Lock (D17) | The abstract Unix socket `@layerfs/network`, bound in the machine's namespace (abstract names are per network namespace, the resource's own scope, across the host's and a Linux Engine worker's filesystems); a waiter connects and waits for end of file, bounded by 10 s. Held for the slot, the pair and the host address only. | [`network/lock.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/network/lock.rs) |
| Inside | `lo` up; `eth0` with IPv6 off (`lo` keeps `::1`), the /31, up, default route via the host end; `net.ipv4.ping_group_range = 0 2147483647` (D9; a fresh namespace refuses unprivileged ICMP). The pair's MTU is the machine's default-route interface's (D10). The host end has no IPv6 link-local address (D11). | `network/setup.rs` `configure`, `network/links.rs` |
| Record | `<sandbox>/network` (0644, `lfs<id12> <guest>/31`), read by the launcher after the listener (missing or empty fails the start in its listener phase) into the namespace record and the daemon record, and the home's Sandbox record (`network`), which [`sandbox.get`](../../api/sdk/sandbox.md#sandboxget)'s long form answers. State for reopen, doctor and tests; nothing allocates from it. | [`namespaces/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/create.rs), [`record/sandbox.rs`](../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs) |
| Firewall (D6, D7, D8, D15, D20) | `table inet layerfs`, one nfnetlink transaction at every Sandbox create and every worker start (owner to confirm): `NEWTABLE` and three base chains (forward filter 0, input filter 0, postrouting nat 100, policy accept, no `EXCL`), a flush of the table's rules, then the rules: forward `iifname "lfs*" oifname "lfs*" drop`, `iifname "lfs*" accept`, `oifname "lfs*" ct state established,related accept`, `oifname "lfs*" drop`; input `iifname "lfs*" tcp dport <port> drop` per closed port (the chain is always there, empty with none); postrouting `ip saddr <pool> oifname != "lfs*" masquerade`. Idempotent, atomic, and self-healing after an `nft flush ruleset`. Only `meta`, `cmp`, `ct`, `bitwise`, `payload`, `immediate` and `masq`: no `fib` (Docker Desktop's kernel has none). No chain drops by policy, so Docker's and lima's tables keep working. | [`network/nftables.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/network/nftables.rs), [`network/netlink.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/network/netlink.rs) |
| Closed ports (D7) | A command's loopback is its own, so every daemon (`127.0.0.1`, which the listener parse asserts) and every worker on loopback (lima, a Linux Engine) is out of reach at the machine's own addresses by construction. The input chain drops the launcher's own listening port when its address is not loopback: Docker Desktop's and apple-container's worker, `0.0.0.0:7070` (`launch::network::serving_on`, set by `layerfs worker` after it binds). On a Linux host no user port is closed; a host's `127.0.0.1` listener is not reachable at its other addresses. What this closes is the machine's own addresses only; see Host-side forwards. | [`layerfs-sandbox launch/network.rs`](../../../../control/crates/layerfs-sandbox/src/launch/network.rs), [`worker_server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs) `prepare_network` |
| Forwarding | LayerFS sets `net.ipv4.ip_forward=1` only where it owns the namespace: the lima and apple-container preparation (`PREPARE`, every start) and Docker Desktop's worker entry (not a Linux Engine's `--network host` worker). On a Linux host (`process`, Linux Engine `docker`) it never writes it, and a start reads it first: 0 is `Unavailable`, naming the setting (owner to confirm). | [`setup/worker.rs`](../../../../control/crates/layerfs-platform/src/setup/worker.rs), [`docker/power.rs`](../../../../control/crates/layerfs-platform/src/platforms/docker/power.rs), `network/setup.rs` `forwarding` |
| Dropping forward chains (D19) | In nftables an accept in one table does not override a drop in another at the same hook. A start refuses `Unavailable` when a forward-hook base chain of an `ip` or `inet` table has policy drop and its table has no rule accepting `iifname "lfs*"` and none accepting `oifname "lfs*"` (Docker's `filter FORWARD` once it enabled forwarding; ufw's default), naming the chain and the rules that admit the host ends (`iptables -I DOCKER-USER -i lfs+ -j ACCEPT` and the return rule; owner to confirm). A rule matching the `lfs` prefix, as iptables-nft writes `-i lfs+`, is recognised, so a host that added them is admitted. Legacy (x_tables) iptables is not visible over nfnetlink and is not checked. | `network/nftables.rs` `dropping_forward`, `network/setup.rs` `forward_policy` |
| Resolver (D12, D13) | Decided by the launcher before any effect from its `/etc/resolv.conf`; a Sandbox has its own loopback and no IPv6. No loopback (or `0.0.0.0`) and no IPv6 `nameserver`: its file is bound as before (Docker Desktop `192.168.65.7`). A loopback one (systemd-resolved's `127.0.0.53`, a local dnsmasq) or an IPv6 one: `<sandbox>/resolv.conf` (0644) is written with its `search`, `domain` and `options` lines and only IPv4 nameservers that are not this host's own, its own and, for `127.0.0.53`, those of `/run/systemd/resolve/resolv.conf` (lima: `192.168.5.2`; a dual-stack upstream's IPv6 servers, which glibc would try first, are left out); no IPv4 server left, `Unavailable` naming `/etc/resolv.conf`, what it names and the fix: LayerFS runs no DNS forwarder (owner to confirm). The written file is the commands' alone (spec §3.1: the daemon's paths are unchanged): the daemon's view binds the launcher's file, and the outer init stage, which commands join, holds a mount namespace of its own made from the daemon's after the pivot (`mounts::commands_view`): the daemon's root is made shared (`MS_SHARED`) within the Sandbox's mount namespace, which stays private to the machine's, the commands' copy a slave (`MS_SLAVE`), so every mount and unmount the daemon makes later (a Workspace's) reaches commands and nothing commands' view mounts goes back, and the written file is bound read-only at `/etc/resolv.conf` there. Every command joins that view through `/proc/<init>/ns/mnt` as before (no per-command cost); it exists before the daemon starts, and a failure to make it ends the Sandbox, so no command runs without it (the gate is one byte each way on the inner stage's standard input: an end of file means the inner stage failed and its own error is logged; any other byte is logged as the outer stage's error). The file is a snapshot taken at create: a long-lived Sandbox on a roaming host keeps the upstream servers it started with. `/etc/hosts` is bound unchanged; its `localhost` now means the Sandbox. | [`network/resolver.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/network/resolver.rs), `namespaces/create.rs`, [`helper root.rs`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper/root.rs) |
| Joining | Every command helper opens `/proc/<init>/ns/net` with the other handles and its own `/proc/self/ns/net`; equal `(st_dev, st_ino)` (a Sandbox started by an earlier build, or a reused pid) ends the stage by a signal before anything runs, so a command never runs on the machine's network. Then it enters the network namespace first, before the mount (`/proc/<init>/ns/mnt`: the commands' view where a resolver file was written, else the daemon's), pid and IPC ones, no-new-privs and the uid change: one open, two `fstat`, one `setns` per command. | [`helper exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper/exec.rs) |
| Reopen (D16) | Also requires that the recorded init holds a network namespace other than the launcher's (`network::private`): a Sandbox started before this change is not reopened and answers its broken state with the `sandbox remove` fix (no backward compatibility in 2.0). | [`namespaces/sandbox.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/sandbox.rs) |
| Leftovers (D5) | A pair dies with its namespace, which dies with the Sandbox's cgroup; a host end outlives it only while something outside holds the namespace (a user's `nsenter`) or during the kernel's asynchronous teardown (under a second). Doctor reports and never removes (owner to confirm): `machine.network` lists `lfs*` links no Sandbox recorded in the home owns, on any machine (`process` and a Linux Engine `docker` share a Linux host's namespace, and a worker reads only its own machine's records, so the caller matches the links against all of the home's; when any of those records cannot be read it has no verdict and lists no link, since a delete fix could cut a live Sandbox's network), a missing table while Sandboxes are recorded and `ip_forward` 0, each with its fix; `sandbox.network` checks a running Sandbox's init namespace and host end. On a Linux host a link may be another home's, so nothing is swept, and doctor has no `--fix` action (spec §6 and §7.7 ask for one; owner to confirm). | [`doctor/network.rs`](../../../../control/crates/layerfs-control/src/doctor/network.rs) |
| Removal | lima, apple-container and Docker Desktop lose the namespace, links and table with the VM or container. On a Linux host the `process` and `docker` machines are built in and never removed, so nothing removes this host's `table inet layerfs` (owner to confirm): with no `lfs*` link left it matches nothing, and `nft delete table inet layerfs` removes it. | — |

Breaking changes (2.0; no backward compatibility, owner 2026-11-19): a
command that relied on reaching a service on its machine's `127.0.0.1`
(another Sandbox's server, the daemon, a Linux host's loopback service)
reaches it at the machine's address instead, or not at all for another
Sandbox; a Sandbox started by a build before this change is broken after
the upgrade (`layerfs sandbox remove <sandbox>` and create it again); a
Linux host with `net.ipv4.ip_forward=0`, a dropping forward chain or a
resolver with no IPv4 nameserver a Sandbox reaches refuses `sandbox.create`
with `Unavailable` and the fix; IPv6 destinations other than `::1` fail at
once (`ENETUNREACH`).

Owner to confirm (built as the conservative defaults, 2026-11-24): a Linux
host's `ip_forward=0` is refused, never written; a dropping forward chain
is detected and refused with the rules to add; no pool copy in the machine
record; a resolver with no IPv4 nameserver a Sandbox reaches is refused;
the table is rewritten at every create and worker start; doctor reports
leftovers with a fix hint and has no `--fix` action (spec §6 and §7.7 ask
for one; the doctor call surface has no mutating action, and adding one is
a call-surface change); a Linux host's table is left behind and doctor
lists what is left; host-side forwards stay reachable (below).

**Host-side forwards (owner to confirm, 2026-11-24).** Goal 5 holds for the
machine's own addresses only. LayerFS's forwards on the host that runs the
machine are not closed: on a Mac, lima's `ssh -L` daemon forwards and its
worker forward, Docker Desktop's published worker port
(`127.0.0.1::7070`) and apple-container's (`127.0.0.1:<port>:7070`) all
listen on the Mac's `127.0.0.1`, which a command reaches through the host
gateway name (`host.lima.internal`, `host.docker.internal`): the packet
leaves through `lfs*` as forward traffic, so the input rule never sees it.
apple-container's gateway is the Mac's vmnet address, which does not reach
the Mac's loopback. Closing them would take a destination rule on the
gateway, which goal 3 forbids (no destination special case), on ports that
change at every start. What guards them is what guards them from any other
local process: a daemon admits only its controller (a Noise handshake with
the controller's static key, the peer key its setup authorizes, and the
expected instance), and a worker only a client that sends its 32-byte
secret first. Neither is readable by a command: the controller keys are in
`sandbox/<name>/control.json` (0600) in the caller's home on the Mac or in
the worker's root (`/layerfs/data`, outside the Sandbox's pivoted root);
the worker's secret is `/layerfs/state/secret` (0600, root; a 0700 tmpfs in
the Docker worker), outside it too; the daemon's own setup in the Sandbox
directory, which is bound into the root, is `config/daemon.setup` 0600 in a
0700 root-owned directory; the machines mount nothing of the Mac (lima
`mounts: []`; the Docker worker has its data volume, the bundle read-only
and the state tmpfs; apple-container its data volume); and every command
runs as a nonroot uid with no capabilities and no-new-privs. On a Linux
host there is no such forward: the daemons and a Linux Engine's worker are
on the host's `127.0.0.1`, which no Sandbox reaches.

A `namespaces` Sandbox resolves names as the kernel that runs it does: the
init stage binds that machine's `/etc/hosts` and its resolver file (above)
read-only over the image's, so Docker Desktop's `host.docker.internal`, a
lima guest's `host.lima.internal`, a Linux Engine's `--add-host` entry and
any DNS name the machine resolves resolve in the Sandbox too. The image
copy leaves both files out: the binds are the Sandbox's only resolver
files. What a PostgreSQL Store's URL may name on each machine is
[Reach](../machines/workers-and-reach.md#reach-reachrs); the daemon dials
it from the machine's network.

On a Linux host, then, a `namespaces` Sandbox on `process` or `docker` shares
the host's kernel: its separation is its namespaces (its network among
them), cgroup, seccomp filter and command identity (uid 501), not a VM. No
call option, record or answer has an `isolation` or `scheduling` field;
`isolation` above is this in-memory capability only, fixed per backend, so
it never counts a VM the machine may add (`IsolationModel::Namespaces`,
[`runtime/types/capabilities.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime/types/capabilities.rs)).

Capabilities "describe support, not qualification"
([`runtime/types/capabilities.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime/types/capabilities.rs)).

## `runtime/dispatch.rs`: `Runtime` and `Sandbox`

| Enum | Variants | Methods |
| --- | --- | --- |
| `Runtime`: one deployment's choice | `Native(NativeProvider)` (macOS), `Namespaces(NamespaceProvider)` (Linux) | `backend`, `capabilities`, `local_directory`, `native_paths`, `launch`, `launch_with_binds`, `reopen`, `reopen_stopped` |
| `Sandbox`: the launched owner | `Process` (macOS), `Namespaces(Box<_>)` (Linux) | `backend`, `identity`, `endpoint`, `run`, `run_in`, `exec_stop`, `track`, `log_tail`, `inspect`, `stop`, `delete`, `scratch_directory`, `local_record`, `record` |

The native backend is macOS only: Linux starts no native Sandbox
(#533 O27), and a native record found there is refused with the
records-only `sandbox remove` as its fix (RV3 FAIL 3,
[`native/mod.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/process/mod.rs)).
A reopened namespaces Sandbox runs the helper copy in its own `bin/`; one
without it (an earlier build's) refuses a command with that fix
(`namespaces::NO_HELPER_COPY`).
`reopen` starts nothing and checks the recorded identity; `reopen_stopped`
admits only one Delete. `endpoint` of a Sandbox whose daemon (or init) exited
is the stopped observation (`WireFailure::container_stopped`). `inspect`
leaves unknown exit and storage facts `None`.

### Stop and delete

| Step | Native | Namespaces |
| --- | --- | --- |
| `stop(s)` | Detach mounts; SIGTERM, wait `s`; SIGKILL, wait 5 s; then SIGKILL every named process group and the daemon's, wait 5 s | One write to `cgroup.kill`; wait `max(s, 5 s)` for the init to be gone and `populated 0` |
| Second `stop` | refused | refused |
| `delete` before a known stop | refused ("known stop required") | refused |
| `delete` | kill groups if not yet, detach mounts, remove the directory | refuse while the cgroup holds a process; remove cgroup tree, directory and private root |
| Source | [`native/sandbox.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/process/sandbox.rs) | [`namespaces/sandbox.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/sandbox.rs) |

Neither is retried. On `Drop` an owner only reaps a daemon or init it started
that already exited; a running one is left for `stop`.

### Exit codes

| Backend | `ExecExit.code` | `pid` | Source |
| --- | --- | --- | --- |
| Native | the exit code, or 128 + signal | the command's | [`native/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/process/exec.rs) |
| Namespaces | the helper's code, which is the command's or 128 + its signal; 125 is the helper's own failure; a helper ended by a signal (its setup failed, or the Sandbox ended) is a failure, never an exit code | the helper's | [`namespaces/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/exec.rs), [`helper main.rs`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper/main.rs) |

Both: stdout and stderr are copied to the sinks as they arrive in 8 KiB
windows (stderr on a scoped thread); counts are what each sink accepted. A
sink failure closes the pipes, still waits for the exit, and returns the
sink's error. End of stream is never the exit. Stdin is refused
(`Unsupported`); there is no timeout. The exit is the root process's; it does
not prove descendants ended.

## `shared/`: one daemon process on this kernel

```text
 <root>/<name>/                0755  one directory per Sandbox
   config/daemon.setup         0600  in a 0700 dir; written once
   config/helper.spec          0600  namespaces only
   overlay/overlay.sqlite            in a 0700 dir (daemon's template)
   overlay/<namespace>.sqlite        one per mounted Workspace
   overlay/upper/  work/       0700  namespaces only: root's writable layer
   owner                       0600  real path of the record's folder
   workspaces/                       Workspace mounts
   daemon.log                  0600  daemon stdout + stderr; new with the
                                     directory, never rotated (below)
   tmp/                        1777  commands' scratch and HOME
   groups/                     0700  native: one empty file per group
   bin/layerfs-daemon, layerfs-sandbox-helper   namespaces only, 0755
   runtime/lib/  runtime/bin/  0755  namespaces only: the bundle's runtime layer
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
| Root | Absolute, this user's, not group or other writable, `o+x`. `Local::new` (namespaces) also needs every ancestor `o+x`, since commands run as another identity; `Local::native` does not (a macOS home is 0750). | [`shared/daemon.rs`](../../../../control/crates/layerfs-sandbox-backend/src/shared/daemon.rs) |
| Process identity | `LocalProcess { pid, start, boot }`: all three must match to count as alive; it lives in the bridge's `process` module. | [`process/identity.rs`](../../../../engine/crates/layerfs-bridge/src/process/identity.rs) |
| `daemon.log` | Made new (`O_EXCL`) with the Sandbox's directory by each start, which refuses a directory that exists, and removed with it at close: a daemon never opens a log an earlier one wrote, so there is no `daemon.1.log`. Nothing rotates it while the daemon runs (a rename would not move the daemon's descriptor, and its launcher waits for the listener line from the file's start); it grows until the Sandbox closes. With tracing off the daemon writes one line for each control connection that ends without `EndSession` (a caller that closes it) and for each reclaimed slot or expired request, none for a mount, exec or Commit itself. Rotating it while the daemon runs is not built in 2.0 (layerfs-project#180, live writers). | [`shared/daemon.rs`](../../../../control/crates/layerfs-sandbox-backend/src/shared/daemon.rs), [`backend/namespaces/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/create.rs) |
| Search path | Linux: `/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin`. macOS: this process's `PATH`, else that plus `/opt/homebrew/bin`. | [`shared/search_path.rs`](../../../../control/crates/layerfs-sandbox-backend/src/shared/search_path.rs) |
| OS facts | Each lives in the one backend that runs on its OS; `shared/` reaches it as `os`. `backend/namespaces/linux/`: `mountinfo`, lazy `MNT_DETACH`. `backend/process/macos/`: the `mount` table, `umount -f`. The seccomp filter is `backend/namespaces/seccomp.rs`, the crate's only `unsafe`. | [`shared/mod.rs`](../../../../control/crates/layerfs-sandbox-backend/src/shared/mod.rs), [`namespaces/linux/`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/linux), [`native/macos/`](../../../../control/crates/layerfs-sandbox-backend/src/backend/process/macos), [`lib.rs`](../../../../control/crates/layerfs-sandbox-backend/src/lib.rs) |

### The run folder

`<root>` is the run root, `machine/<host>/run/` of the home or
`machine/linux-guest/run/` of a worker's root, and `<name>` the Sandbox's
run folder name
([Home: names outside the records](../home/README.md#names-outside-the-records)).
A `process` Sandbox's entries are this user's; a `namespaces` Sandbox's are
root's, and its commands run as another, nonroot identity. Everything in
the folder goes with it: `delete` removes it whole after a known stop, at
`sandbox.remove` or when a start fails
([Stop and delete](#stop-and-delete)).

| Entry | Written by | Read by | Source |
| --- | --- | --- | --- |
| `<name>/` | the launcher, one `mkdir` | anyone may pass and list it (0755) | [`shared/daemon.rs`](../../../../control/crates/layerfs-sandbox-backend/src/shared/daemon.rs) `create` |
| `config/daemon.setup` | the launcher, once, before the spawn: the daemon's private key, its caller's public key and the Store, a PostgreSQL URL with its password included | the daemon (`--config`). A `namespaces` command cannot read it; a macOS `process` command, running as the user who owns it, can | same, `prepare`; [`store.rs`](../../../../engine/crates/layerfs-bridge/src/store.rs) `DaemonStore` |
| `config/helper.spec` | the launcher, once: absolute paths and binds, no secret | the helper's init stage, and a reopen | [`namespaces/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/create.rs) `prepare`, [`namespaces/spec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/spec.rs) |
| `overlay/overlay.sqlite`, `overlay/<namespace>.sqlite` | the daemon: its template before readiness, then one copy per mounted Workspace, named by the `namespace` of the Workspace's record and deleted at its terminal unmount | the daemon only; an inspection reads their sizes and never opens them | [`workspaces.rs`](../../../../engine/crates/layerfs-store-service/src/overlay/workspaces.rs), [Inspection](inspect.md#what-is-kept-private) |
| `overlay/upper/`, `overlay/work/` | made empty by the launcher; then whatever a command writes outside its Workspaces, through the Sandbox's `/` | commands, through that overlay only | [`namespaces/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/create.rs), [`helper root.rs`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper/root.rs) |
| `workspaces/` | made by the daemon (`process`) or the launcher (`namespaces`); the daemon mounts each Workspace in it | commands, as the mounts allow | [`shared/daemon.rs`](../../../../control/crates/layerfs-sandbox-backend/src/shared/daemon.rs) `LocalPaths` |
| `daemon.log` | the daemon's stdout and stderr, into the file the launcher made | the launcher (the listener line), `log_tail` and the inspect route; a macOS `process` command can read it too | same; [Inspection](inspect.md) |
| `tmp/` | commands: it is their `HOME` and scratch directory | anyone who can reach it (1777) | [`exec/run.rs`](../../../../control/crates/layerfs-sandbox/src/exec/run.rs), [`runtime/dispatch.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime/dispatch.rs) `scratch_directory` |
| `groups/` | each `process` command's start: one empty file per process group, removed once the group has no member | the caller's side only | [`native/groups.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/process/groups.rs) |
| `bin/`, `runtime/` | the launcher: copies of the daemon, the helper and the runtime layer | anyone may read and run them; only root may change them | [`namespaces/paths.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/paths.rs) |
| `owner` | the caller's side, once, after the daemon started | a reopen with no `deploy.localDir`, and the run root sweep, which removes a run folder only when its marker is older than a minute and names no live daemon ([`sweep.rs`](../../../../control/crates/layerfs-sandbox/src/sweep.rs) `owner`) | [`launch/local.rs`](../../../../control/crates/layerfs-sandbox/src/launch/local.rs) `mark_owner`, `owned` |
| `<root>/<name>.root/` | the launcher makes it empty; the init stage mounts the Sandbox's root on it inside the Sandbox's own mount namespace, so from outside it stays empty | nothing outside the Sandbox | [`namespaces/paths.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/paths.rs); `delete` removes it with `rmdir` |
| `<root>/images/` | the first Sandbox that needs a digest unpacks it once, under that digest's `<64 hex>.2.lock`, beside its sidecar `<64 hex>.2.json` | every Sandbox built on it, read-only; it is not removed with a Sandbox, and the lock files stay | [`rootfs/cache.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/rootfs/cache.rs), [`image_artifact.rs`](../../../../control/crates/layerfs-sandbox/src/image_artifact.rs) |

`tmp/` (the commands' `HOME`), the root's writable layer and the
Workspaces' overlay databases are plain folders and files of the run root,
on the filesystem of the home or of the machine's data root, which also
holds that side's sqlite Stores. Nothing gives a Sandbox a quota or
reserves space for it (`supports_resource_limits` is false for both
backends): what a command writes to its `HOME` takes space from the
Stores' disk. A `namespaces` Sandbox's `/tmp` is a tmpfs, in memory.

## `backend/process/`: native

The machine is the boundary: daemon and commands share this kernel's
processes, network, users and filesystem. `NativeProvider::launch`
accepts only `Local` options whose identity is this process's own uid and gid
and whose environment has no `PATH`
([`native/provider.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/process/provider.rs)).

This backend isolates nothing. A command is an ordinary process of the
calling user: it starts in the Workspace's directory with `HOME` set to the
Sandbox's `tmp/`, and from there reads and writes every file that user can,
on the host's own filesystem, including the LayerFS home and this
Sandbox's `daemon.setup`, and uses the host's network. It gets this
process's search path and, when present, the user's `RUSTUP_HOME`,
`CARGO_HOME` and `GIT_CONFIG_GLOBAL`, and nothing else of the caller's
environment
([`native/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/process/exec.rs),
[`launch/local.rs`](../../../../control/crates/layerfs-sandbox/src/launch/local.rs) `start_named`).

| Rule | Source |
| --- | --- |
| Each command leads its own process group, named in `groups/<boot 32 hex>-<pid>-<start>` before anything waits on it; a group that cannot be named is killed at once. | [`native/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/process/exec.rs) |
| A name is removed once its group has no member; a process that leaves its group (`setsid`) is followed only when named with `Sandbox::track(pid, start)`. | [`native/groups.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/process/groups.rs) |
| Groups of another boot, or whose leader id now names another start time, are never signalled. | `end`, `members` in `native/groups.rs` |

## `backend/namespaces/`: Linux namespaces and the helper

```text
 NamespaceProvider::new(root)   refused before any effect unless: root,
   user namespaces blocked, cgroup v2, /dev/fuse, root on a filesystem
   that can hold an overlay upper (not overlay, nfs, cifs, smb3, 9p,
   virtiofs or fuse*)
 launch_with_binds
   check binds; rootfs: unpack the docker-archive once per digest
   prepare: Local parts + copied daemon, helper and runtime layer + spec
            + upper/work
   start_request (before any effect): ip_forward 1, no dropping forward
     chain, the pool, the resolver plan; else Unavailable
   cgroup::create /sys/fs/cgroup/layerfs/<id>  (needs cgroup.kill)
     layerfs/ gives its children +memory +pids, those its parent lists
     (give_controllers; a failed write is one log line, not a failure)
     split: makes the leaf <id>/init, then <id> gives its children the
     same controllers it has (a failed write is one log line)
   network::table: table inet layerfs, one transaction
   spawn: helper init --spec <spec>
            joins <id>/init; unshare(NEWNS | NEWPID | NEWIPC); private;
            lock, slot, unshare(NEWNET), veth lfs<id12>/eth0, unlock,
            configure, write <sandbox>/network; setns(machine netns)
            v                                    then setns(own netns): holder
          helper init-inner --spec <spec>          PID 1 of the namespace
            overlay(lower = image, upper/work = the Sandbox's), /proc,
            /dev (fuse null zero full random urandom tty, pts, shm), /tmp,
            Sandbox directory and each Bind at their own paths, the
            runtime layer read-only at /.layerfs/runtime, this host's
            /etc/hosts and resolver file read-only over the root's; pivot
            v
          /.layerfs/runtime/lib/ld-linux-<arch> --inhibit-cache
            --library-path /.layerfs/runtime/lib layerfs-daemon --config
            <setup> as root (layerfs-daemon --config <setup> without a
            runtime layer), on the machine's network; init reaps
   await_listener on daemon.log; read <sandbox>/network
```

| Item | Rule | Source |
| --- | --- | --- |
| `HelperSpec` | One 0600 file of absolute paths (`root`, `sandbox`, `daemon`, `config`, `cgroup`, `lower`, `upper`, `work`), an optional `runtime=` line (the Sandbox's copy of the runtime layer), `bind=<rw\|ro>\t<host>\t<guest>` lines, `network=<pool>\t<link>` (its pool and host end name) and an optional `resolver=<file>` (the resolver file written for it). A spec without `runtime=` (written before it) still parses and reopens unchanged. No secret: the helper never reads the daemon setup. | [`namespaces/spec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/spec.rs) |
| `Bind` | A sqlite Store's directory, read-write at its own path, so SQLite's WAL and the writers' lock see one inode; none for a PostgreSQL Store. A bind may not hold the runtime root, the Sandbox directory or its private root. | [`runtime/types/bind.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime/types/bind.rs), `check_bind` in [`namespaces/create.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/create.rs) |
| Image | `sha256:<64 hex>`, the archive's config digest; a `docker save` archive with uncompressed layers (gzip or zstd layers refused); unpacked once per digest under `<root>/images` and shared read-only. A reopen unpacks nothing: an image gone is refused. | [`rootfs/mod.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/rootfs/mod.rs), [`runtime/types/image.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime/types/image.rs), [`namespaces/provider.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/provider.rs) |
| Helper | `LAYERFS_SANDBOX_HELPER`, else beside the daemon or this executable; copied into the Sandbox's `bin/`, which a reopen uses. | `namespaces/provider.rs` |
| Runtime layer | `runtime/` beside the daemon (the sealed bundle's, R5), when it holds this architecture's loader; else none, and the daemon runs on the image's own loader and libraries as before. Copied into the Sandbox's `runtime/` (regular files of `lib/` and `bin/` only; anything else refused; the loader and programs 0755, the libraries 0644), so a bundle replaced under a running Sandbox changes nothing. The init stage binds it read-only, nosuid and nodev at `/.layerfs/runtime`, refusing an image whose `/.layerfs` or `/.layerfs/runtime` is not a plain directory, and starts the daemon through its loader (`--inhibit-cache --library-path /.layerfs/runtime/lib`) with `LAYERFS_SANDBOX_RUNTIME=/.layerfs/runtime` and no `LD_*` variable: the image's loader, libraries and `/etc/ld.so.cache` are never used, so an image with an older glibc, musl or no libc starts it. The image's `/etc/ld.so.preload` is still read by the loader. Its files are unmodified Debian binaries (glibc's loader and libraries, `libgcc_s`, OpenSSL, `getent`, `bash-static`, `busybox-static`), several GPL or LGPL: the bundle's `licenses/` holds each package's copyright file and `SOURCES`, the written source offer naming every file's Debian source package, version and snapshot.debian.org page (`sandbox_runtime.sources` in `bundle.json`); `tools/linux_bundle.py sources` downloads exactly those sources to publish beside a release. | `namespaces/provider.rs`, `namespaces/create.rs`, [`helper root.rs`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper/root.rs), [`helper init.rs`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper/init.rs), [`sandbox_runtime.rs`](../../../../engine/crates/layerfs-bridge/src/sandbox_runtime.rs) |
| Resolver files | `RESOLVER_FILES` (`/etc/resolv.conf`, `/etc/hosts`) of the launcher's root, each bound read-only at its own path in the Sandbox root, so the Sandbox resolves names as the launcher does; in the commands' mount view, `resolv.conf` is the spec's `resolver=` file instead when the launcher's names a loopback or IPv6 server (Resolver, above), and the daemon keeps the launcher's. A mount point the image lacks is made empty, 0644, in the Sandbox's own writable layer, and one that is a link or another non-file is replaced there; an image whose `/etc` is not a plain directory (a bind through a link would leave the root before the pivot), or whose entry is a directory, keeps its own; a file the launcher lacks is not bound. They are never in a Workspace, a diff or a Commit: they land on the root's `/etc` (the image and the Sandbox's upper layer), Workspaces are mounted under the Sandbox directory, and `HelperSpec::resolver_targets` refuses the start when the Sandbox directory or a bind would hold one | [`namespaces/spec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/spec.rs), [`helper root.rs`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper/root.rs) `resolver` |
| Commands | `helper exec --init <pid> --cgroup <dir> [--command-cgroup <dir>] [--runtime <dir>] --uid --gid --cwd -- <argv>`: forks itself into the Sandbox's leaf group `<id>/init`, the only group a command helper uses (`clone3` with `CLONE_INTO_CGROUP`; the parent, outside it, waits and ends as its child did, a signal included), the child enters the namespaces, the init stage's network namespace first (Joining, above), becomes uid:gid with no capabilities and no-new-privs, installs the filter below, execs. `--runtime /.layerfs/runtime` (a Sandbox with the runtime layer) appends `/.layerfs/runtime/bin` to the end of the command's `PATH` once; `argv[0]` (`bash`) is looked up on it inside the Sandbox, so the image's own `bash` wins and an image with none runs the runtime's static one. That directory holds only `bash`, `busybox` (no applet links) and `getent`. | [`namespaces/exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/exec.rs), [`helper exec.rs`](../../../../control/crates/layerfs-sandbox-backend/src/bin/layerfs-sandbox-helper/exec.rs) |
| Command cgroup | With an exec slot, the command (never the helper) is created inside `ws-<workspace>/exec-<exec>` (`clone3` with `CLONE_INTO_CGROUP`), without one inside the helper's (the leaf); once its exec succeeded the helper writes one byte on a socket it was given as stdin, and only then is the stop handle told. A command that cannot be created there never runs. No exec moves a process between groups: a write to `cgroup.procs` waits for an RCU grace period (5–28 ms after an idle spell), which every command paid before. Only the init stage joins its group, once per Sandbox. Once the command's exit is reported, the run removes its group (and its Workspace's, when empty) if nothing of it runs, with one look and no wait. A group's `populated` drops when its last process has exited, not when it is reaped, but not in order with the parent being told: since Linux 6.19 (d245698d727a) an exiting task leaves its cgroup after `exit_notify`, at its final context switch (later on PREEMPT_RT), so that look can still count the command (650828a686's message said it leaves before `exit_notify`, which holds before 6.19 only); a child signalled with the command may also exit after it. A cancel, early or not, removes the group itself once it reports it empty, which needs no ordering. A group something still runs in (nothing cancelled it) stays and ends with its Sandbox. Two known gaps follow the table. | `namespaces/exec.rs`, [`namespaces/cgroup.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/cgroup.rs) |
| Sandbox group layout | The Sandbox's group `<id>` holds no process: its init stages, daemon, command helpers and commands with no exec slot are in its leaf `<id>/init`, commands with one in `ws-<workspace>/exec-<exec>`. cgroup v2's no-internal-process rule allows only such a group to enable controllers for its children, so `create` splits it before any process joins and gives `init` and each `ws-<workspace>` the memory and pids controllers it has: a Workspace's memory, processes and OOM kills can be read ([`sandbox.get`](../../api/sdk/sandbox.md#sandboxget) `workspaces[].usage`), and `<id>`'s own files still count everything. `ws-<workspace>` enables nothing for its command groups. No limit is ever written. | `namespaces/cgroup.rs` `create`, `split`, `leaf` |
| End | One `cgroup.kill` on `<id>` ends everything the Sandbox started, its leaf and Workspace groups included, whatever session it left; `track` is a no-op. Reopen checks the recorded init and its membership of the leaf. | `namespaces/cgroup.rs`, [`namespaces/host_checks.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/host_checks.rs) `in_cgroup` |

Known gaps in the command groups (owner decisions, no product change made;
review 650828a686 F1 and F3):

| Gap | What happens | Options |
| --- | --- | --- |
| A command's group left after its exit (Linux 6.19 and later) | The run's one look after an uncancelled command (`remove_exec` from `namespaces/exec.rs` `run_in`) can find the group still counted, since the kernel unlinks the exiting task after its parent may already have been told; the empty group then stays until the Sandbox ends. The cancel path is unaffected (it waits for `populated 0`). The machines measured here run 6.8 and 6.18 | a bounded readiness wait on `cgroup.events` in the exec return path, which adds exec latency; or record the leftover as accepted |
| A Workspace's group removed under another command's create | `remove_exec` (after a run, or a cancel's release) removes `ws-<workspace>` once it is empty; a second command of the same Workspace whose `create_exec` found `ws-<workspace>` before that removal then fails to make `exec-<exec>` (`ENOENT`): it never runs and the failure is reported, never retried. The window is between that command's two `mkdir`s | stop removing `ws-<workspace>` per command (the Sandbox's end removes it), or a cross-process lock around a Workspace group's create and remove |

A command that made a user namespace could hold its own copy of a Workspace
mount outside the daemon's unmount. Two guards stop it:

| Guard | Rule | Source |
| --- | --- | --- |
| Command filter | Seccomp, installed after no-new-privs: `unshare` and `clone` with `CLONE_NEWUSER` answer `EPERM`, `clone3` answers `ENOSYS` (the C library falls back to `clone`), all else allowed. A foreign ABI (i386 on x86_64, arm on aarch64) is filtered by its own numbers, x32 by the native ones; an unknown ABI is killed. A filter that cannot be installed fails the command. | [`namespaces/seccomp.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/seccomp.rs) |
| Host check | `user.max_user_namespaces=0`, or `/proc/sys/kernel/seccomp/actions_avail` listing `allow`, `errno` and `kill_process`; else `Unsupported`. The runtime never sets the limit. | [`namespaces/host_checks.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/namespaces/host_checks.rs) |

### On the `docker` machine

There is no docker backend. The `docker` machine runs `namespaces` Sandboxes
inside its worker container, as a VM's worker does; the container is root,
has `/dev/fuse` and a writable cgroup v2 tree, and the Sandbox root is the
bundle's image archive. The bundle is bound into the container, so its
`runtime/` is there beside the daemon. Its worker never sets `user.max_user_namespaces`
(that would change the whole Docker host), so the command filter alone blocks
user namespaces. Its Sandboxes resolve names with the worker's resolver
files (above): `host.docker.internal` names this host on Docker Desktop.
Their commands' networks are in the worker container's namespace on Docker
Desktop and in this host's on a Linux Engine, and the worker's own port on
the container's addresses is closed to them (Every `namespaces` Sandbox's
own network, above; its port published on the Mac is not: Host-side
forwards).
Docker publishes ports only when a container starts, so the
machine refuses a forward and every call goes through the worker
([Machines: platforms](../machines/platforms.md)).

A Sandbox record written by the retired docker backend (`backend` or
`runtime` `docker`, or a started docker-machine record with no `runtime`) is
never reopened: every call but `sandbox.remove` is `Unsupported`, asking to
recreate it; `sandbox.remove` removes its records (outcome `already_gone`)
and touches no container, its `left` naming `docker rm -f <container>` when
the daemon record keeps the container's name (no older name is computed);
`sandbox.create` of that name clears the record first
([`record/sandbox.rs`](../../../../control/crates/layerfs-sandbox/src/record/sandbox.rs),
[`reopen.rs`](../../../../control/crates/layerfs-sandbox/src/reopen.rs),
[`remove.rs`](../../../../control/crates/layerfs-sandbox/src/remove.rs) `forget`).

## `runtime/cancel.rs`: stop handles

| Handle | Backend | Cancel |
| --- | --- | --- |
| `Stop::Cgroup { path }` | namespaces: `<sandbox cgroup>/ws-<workspace>/exec-<exec>` | SIGTERM to each process in `cgroup.procs`, wait the grace, `cgroup.kill`, wait 5 s for it to be empty; once empty (or found empty), remove the group and its Workspace's when that is empty; a cancel told before the command started kills and removes it the same way |
| `Stop::Group { pgid, leader }` | native: the command's process group, leader by pid, start and boot | SIGTERM to the group, wait the grace, SIGKILL, wait 5 s; a child that called `setsid` escapes it |

`cancel` answers `Ended` (nothing ran, not signalled) or `Signalled(SIGTERM
| SIGKILL)`; a process left after SIGKILL is an error. One attempt; nothing
here times a command out
([`runtime/cancel.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime/cancel.rs)).

## Shared types and formats

| Module | What | Source |
| --- | --- | --- |
| `runtime/types/` | `LaunchRequest`, `ProviderLaunchOptions`, `LocalRecord`, `NamespaceRecord`, `SandboxRecord`, `ExecRequest`, `ExecExit`, `SandboxInspection`, `CommandIdentity`, `Bind`, `Image`, `ContainerId` (a random 64-hex id per Sandbox), `RuntimeError` (`Io`, `Protocol`, `Unsupported`, `Unavailable`: the machine cannot give a Sandbox its network, answered `Unavailable` once the start is undone), `WireFailure` (whether attempted). They compile on Windows too. | [`runtime/types/`](../../../../control/crates/layerfs-sandbox-backend/src/runtime/types) |
| `runtime/trace.rs` | Step times of a traced command (`exec.*`, and the helper's `helper.*` via `--trace <file>`). | [`runtime/trace.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime/trace.rs) |
| `support/postgres_url.rs` | Reads every server host a `postgres://` URL names, as its client (tokio-postgres) reads them (`url_hosts`: the host list and `host`/`hostaddr` parameters, percent-decoded), and says which name this host's loopback (`loopback`, with `ip` reading every IPv4 form `inet_aton` reads and IPv6 zone ids), so a machine that does not share it can refuse it; the URL is never rewritten and never in a message. | [`support/postgres_url.rs`](../../../../control/crates/layerfs-sandbox-backend/src/support/postgres_url.rs) |
| `support/removed_platforms.rs` | Platforms removed in 2.0 (`firecracker`, `microsandbox`, `ssh`): a record naming one is refused, never run. | [`support/removed_platforms.rs`](../../../../control/crates/layerfs-sandbox-backend/src/support/removed_platforms.rs) |
| `support/json/` | A small streaming JSON reader (`read.rs`: image manifests) and the quoting `SUPPORT.json` is written with (`strings.rs`). | [`support/json/`](../../../../control/crates/layerfs-sandbox-backend/src/support/json) |

## Adding a backend

There is no provider trait, registry or factory: a backend is one folder under
`backend/` (`adapter.rs`, `provider.rs`, `create.rs`, `sandbox.rs`,
`exec.rs`), one enum variant, and one arm in each exhaustive `match`; the
compiler lists every place that still needs it.

| # | Step | Where | Breaks until done |
| --- | --- | --- | --- |
| 1 | Add `Backend::<B>` to `Backend::ALL`. | [`kind.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/kind.rs) | `adapter()` |
| 2 | A `static ADAPTER` with `stored_name`, `description`, `capabilities`, and its arm in `adapter()`. The stored name is permanent. | [`adapter.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/adapter.rs) | — |
| 3 | A `ProviderLaunchOptions` variant if it needs its own inputs, and a `SandboxRecord` variant, so no other provider reopens it. | [`runtime/types/options.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime/types/options.rs) | provider refusals |
| 4 | Provider (`launch` or `launch_with_binds`, `reopen`, `reopen_stopped`) and owner (`identity`, `endpoint`, `run_in`, `track`, `log_tail`, `stop`, `delete`, ...); reuse `shared/` for a process on this kernel; keep what only it uses, OS facts included, in its folder. | `backend/<b>/` | — |
| 5 | `Runtime::<B>` and `Sandbox::<B>` with every arm; refuse binds before any effect if it takes none; its `Stop` form. | [`runtime/dispatch.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime/dispatch.rs), [`runtime/cancel.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime/cancel.rs) | every `match self` |
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
