# One control center for every platform

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.
> Written 2026-11-02 on top of #529 (PR #530, head `ddc7c5696b`), which is
> stacked on #527 (PR #528). Neither is merged. The commands are specified in
> [the `layerfs` command and SDK](layerfs-sdk-cli.md); this page holds the model,
> the records, the workers and the plan behind them. The owner's directions given
> while drafting are in §12; the decisions still open are in §13. All facts
> and counts below are read at `ddc7c5696b`.
> The work order is [#533](../issues/533/README.md).

## 1. Summary

A host service, such as Connect, dsh or any server that runs
`@layerfs/runtime`, should reach every Sandbox platform from **one process
and one LayerFS home**, configured in **one place**, with a small API for
its machines and Stores.

Today it can't:

- One runtime is one home, and one home is one placement. Reaching #529's
  eight Sandbox platforms takes eight homes and eight runtime processes.
  Connect's platform pass used a separate home per platform
  (`~/.cache/lfc-platforms/homes/docker`, `…/mac`, `…/lima`, `…/ssh`, …).
- The product reads **56 `LAYERFS_*` settings**. 35 of them only say how to
  reach a platform. Connect mirrors most of them again as `LAYERFS_CONNECT_*`.
- Metadata is spread over the home, `~/.layerfs/` and wherever
  `LAYERFS_LOCAL_DIR` points. The home itself keeps two Store lists, two
  records per project and two places a project can live.
- A macOS Docker home runs a separate Init container for every Init, and a Mac
  without Docker can't use the runtime at all.
- A `vm:` home makes one VM per project and refuses 17 Store, remote and
  PostgreSQL calls, so a Store in a VM can't be backed up to MinIO or R2.
- Moving a Store takes five differently named calls (`start_publish`,
  `start_push`, `start_pull`, `start_send`, `start_load`); PostgreSQL adds
  `attach_postgresql`, `start_import_postgresql` and a URL on every call.
- Sandboxes are numbered per project, and every call names the project.

This proposal separates **compute** from **storage**:

1. **The home is the control center.** Every record lives in it, in three
   parts: `machine/`, `store/` and `backup/`. Every `LAYERFS_*` platform
   setting goes. `LAYERFS_HOME` is the one setting, plus the trace settings
   and any variables a secret reference names. Secret references name
   variables; they are not settings.
2. **Machines run Sandboxes.** `native` (the host's own OS) and `docker` are
   built in, one of each. VMs are created by name, as many of a kind as the
   host holds; an ssh host is connected. A machine starts on the first call
   that needs it and stops when idle. While it runs it has one worker, when
   its kernel isn't the caller's.
3. **Stores hold history.** A sqlite Store is one file, created on `native`
   and moved to one machine at a time; r2 and minio buckets hold its
   backups. A postgresql Store is a database on a server, reached from any
   machine that supports it. Nothing converts one kind into the other.
4. **Projects are named with their Store,** `<store>/<project>` such as
   `shop/api`, so two Stores' projects never collide. Init reads a host folder
   in the host's own process and writes into that existing Store.
   Init keeps `LayerFS.init`'s arguments, `(project, folder, options)`, as
   `project.init`; `LayerFS.init` and `LayerFS.open` are removed (§12).
5. **Sandboxes are named,** bound to one project when created, and run on the
   machine named at creation. After that, no call needs the project.
6. **One Rust implementation, three front ends.** The `layerfs` binary, the
   Node package and a new Python package are thin layers over one Rust
   library; the command no longer needs Node (§7.10).

## 2. Why one home can't serve several platforms today

| Fact at `ddc7c5696b` | Where | Effect on a host service |
| --- | --- | --- |
| `LayerFSRuntime` picks one platform object from `settings.placement` | `api/runtime/src/index.js` (`platformFor`) | one runtime, one placement |
| `useWorker` accepts one endpoint (or one resolver) per process and refuses a second | `api/agent/src/transport/index.js` | one process can't reach the macOS Docker worker and a VM worker at once |
| The SDK decides which platforms a home allows from its transport, its OS and `LAYERFS_SANDBOX` | `api/agent/src/platform.js` (`homeOf`) | the same table is written again in `api/runtime/src/sandbox/platforms.js` (`allowedOf`) |
| On macOS the runtime always hands its clients the Docker worker | `platform/docker/macos.js` (`endpoint`) | the runtime, and so Connect, can never offer `mac` |
| The Sandbox settings travel with every call (`deploy` in `projectArgs`), read from the caller's environment | `api/agent/src/client.js` | each project's client needs its own set of variables; the runtime sets them with `withEnv` around client creation (`clients.js`) |
| On macOS, Init runs in a container of its own, because the worker container sees only the home and the bundle | `transport/worker_client.js` (`initRun`), `worker/init.js` | every Init needs Docker and reads the folder through Docker Desktop's file sharing |
| A VM home makes one VM per project, named `<prefix><project>-vm`, with its own Store disk | `api/runtime/src/platform/vm/index.js` (`vmNameOf`) | a VM can't be shared by two Stores or sized once for several |
| On a `vm:` home, 17 Store, remote and PostgreSQL calls are refused before any effect (`homeLevel`, #527 F3) | `api/runtime/src/index.js` | no Store create, push, pull, publish, send, load or check, and no remote list or add, for a home whose Stores are in VMs |
| `LAYERFS_PLACEMENT` is per home; per-project placement was left for later (#529 P2) | `api/runtime/src/settings.js` | `platform` on `sandbox.create` only checks the home's choice |
| A project is added only to a SQLite Store; a PostgreSQL Store gets data only by importing a SQLite Store | `api/runtime/src/project/init.js` (`prepareInto`), `store/sync.js` | Init can't write into PostgreSQL |
| A PostgreSQL URL is passed on every call and never stored | `api/runtime/src/store/calls.js` (`postgresqlTarget`) | every caller must hold the URL |
| A Sandbox is the lowest free number in its project | `api/agent/src/sandbox/create.js` | a Sandbox can't be named, and every Sandbox call needs the project |

## 3. Settings today

| Sandbox platform | Home setup | Where Store calls run | Settings it needs |
| --- | --- | --- | --- |
| `docker` on Linux | `local` | the SDK's own process | `DOCKER`, `IMAGE`, `VOLUME`, `DAEMON`, `STORE_PATH` |
| `linux` | `local` on Linux | the SDK's own process | `DAEMON`, `LOCAL_DIR`, `IMAGE_ARCHIVE`, `STORE_PATH`; the runtime's `localDir`, `imageArchive` |
| `docker` on macOS | `local` | a worker container per home | the runtime's `bundle`, `image`; the SDK's `WORKER_ENDPOINT` plus the Docker set |
| `mac` | `local` + `SANDBOX=native`; SDK only, no runtime | the SDK's own process, Store on APFS/HFS | `SANDBOX`, `DAEMON`, `LOCAL_DIR`, `STORE_PATH`, optional `NFS_*` |
| `docker` on Windows | `local` | a WSL2 worker per home | the runtime's `wsl.*` (no environment names) |
| `apple-container`, `microsandbox`, `lima` | `vm:<provider>` | a VM and worker per project | `PLACEMENT`, `VM_IMAGE`, `VM_CPUS`, `VM_MEMORY`, `VM_STORE_SIZE`, the runtime's `bundle` |
| `firecracker` | `vm:firecracker` | the same, nested in a Lima VM | the above plus `FIRECRACKER_LIMA`, `_DIR`, `_STATE_DIR` |
| `ssh` | `vm:ssh` | the same, on a host you give | the above plus `SSH_HOST`, `_USER`, `_PORT`, `_IDENTITY` or `_KEY`, `_STORE`, `_KNOWN_HOSTS`, `_STATE_DIR` |

All names are `LAYERFS_`-prefixed. Each is read through one of three routes:
the SDK's environment, the runtime's environment, or the runtime's
`open({...})` settings, and the same setting doesn't always use the same route.

### 3.1 All 56 settings, by kind

| Kind | Count | Names (`LAYERFS_` left off) | In this proposal |
| --- | ---: | --- | --- |
| Reaching a platform | 35 | `PLACEMENT`, `SANDBOX`, `SANDBOX_BACKEND`, `DOCKER`, `IMAGE`, `IMAGE_ARCHIVE`, `VOLUME`, `DAEMON`, `DAEMON_HOST`, `CONTAINER_PREFIX`, `LOCAL_DIR`, `STORE`, `STORE_PATH`, `STORE_POLICY`, `WORKER_ENDPOINT`, `VM_IMAGE`, `VM_CPUS`, `VM_MEMORY`, `VM_STORE_SIZE`, `SSH_HOST`, `SSH_USER`, `SSH_PORT`, `SSH_IDENTITY`, `SSH_KEY`, `SSH_STORE`, `SSH_KNOWN_HOSTS`, `SSH_STATE_DIR`, `FIRECRACKER_LIMA`, `FIRECRACKER_DIR`, `FIRECRACKER_STATE_DIR`, `NFS_MOUNT`, `NFS_UNMOUNT`, `NFS_FORCE`, `NFS_FLUSH`, `NFS_REFRESH` | **removed**: machine records (sizes, images, ssh hosts) and Store records in the home |
| Remote storage, not secret | 6 | `REMOTE_ENDPOINT`, `REMOTE_BUCKET`, `REMOTE_REGION`, `REMOTE_PREFIX`, `REMOTE_PART_SIZE`, `REMOTE_CREDENTIALS` | **removed**: a bucket connection (`backup connect`) |
| Secrets | 3 | `STORE_POSTGRESQL`, `REMOTE_ACCESS_KEY`, `REMOTE_SECRET_KEY` | **replaced** by a secret reference on each connection (`env:LAYERFS_…` or `file:…`), resolved at call time and never written to the home |
| The home | 1 | `HOME` | kept; **the one setting**; unset means `~/.layerfs` (proposed, O28) |
| Internal: LayerFS sets them for its own child processes | 7 | `DAEMON_LISTEN`, `KERNEL`, `SANDBOX_HELPER`, `WORKER_LISTEN`, `WORKER_SECRET`, `BUNDLE`, `NATIVE` | kept, documented as internal |
| Tracing and measurement | 4 | `TRACE`, `TRACE_STEP`, `TRACE_START`, `CONSTRUCTION_WORKERS` | kept |

Counted from product source (`api/*/src`, `engine/crates/*/src`,
`core/crates/*/src`, `api/binding/src`), tests excluded. Name prefixes such as
`LAYERFS_VM` and log markers such as `LAYERFS_DAEMON_IDLE_EXIT` are left out.

`mac` and `linux` are #529's names for Sandboxes on the host's own OS. In
this proposal they run on the `native` machine.

**Result:** every `LAYERFS_*` platform setting goes, the 41 in the first two
rows. `LAYERFS_HOME` is the one setting, plus the trace settings and any
variables a secret reference names. Secret references name variables; they
are not settings. Secrets live wherever the user keeps them; a connection
holds only a reference to them. When `LAYERFS_HOME` is unset the home is
`~/.layerfs` (proposed, §13 O28). The 1.1.0 CLI's `LAYERFS_PROJECT`, a
default for `--project`, is removed too: a call that needs a project names
it.

The Docker image, the Docker socket and the WSL distribution leave the
environment too. `native` and `docker` are never created, so their settings
are kept in their machine records (if O11 is accepted, §7.1).

## 4. Where metadata lives today

| What | Where |
| --- | --- |
| Remotes and each Store's link state | `<home>/remotes`, `<home>/remote-state/<store>` |
| Stores | `<home>/stores.json` (the runtime's list, with each Store's link) and `<home>/.stores/<store>/store.record` (the binding's list) |
| A project of a shared Store | `<home>/.stores/<store>/projects/<project>/` |
| A project with a Store of its own | `<home>/<project>/`, the same files in another place |
| Project and Sandbox records | `project.record`, `sealed.sqlite` (during Init), `ownership.lock`, `sandboxes/<n>/{sandbox.record, create.intent, container}` in either project folder |
| The runtime's second project record | `<home>/.runtime/projects/<project>.json` |
| Init failures | `<home>/.runtime/init-failures.jsonl` |
| Worker endpoints and secrets | `<home>/worker/endpoint.json`, or `<home>/worker/<project>.json` on a VM home |
| Traces | `<home>/traces/` |
| Native Sandbox folders | anywhere `LAYERFS_LOCAL_DIR` points; each holds an `owner` marker (#529) |
| SSH and Firecracker state | `~/.layerfs/ssh/`, `~/.layerfs/firecracker/` |
| VM instances | `~/.lima/`, `~/.microsandbox/sandboxes/`, Apple Container's own store |
| Windows projects | the WSL home `/var/lib/layerfs-windows/<hash>` |
| The Store file | a Docker volume, a VM's disk, the ssh host's path, or a host file named by `LAYERFS_STORE_PATH` |
| A PostgreSQL Store | a `connection` name and a schema in `stores.json`; the URL is held by the caller |
| A Store's open overlay | a folder per process in `$TMPDIR` (#436), removed when the Store closes |
| Connect's settings | `~/.layerfs-connect/settings.json`, then `LAYERFS_CONNECT_*` |

No project name is reserved: a project could be named `worker` or `traces`,
the same as a control folder in the home. Whether Init refuses that case was
not tested.

## 5. How a Store can be shared

A sqlite Store is one database in WAL mode. Every process that opens it must
share its locks and its shared-memory file, which works only when they all run
on **one kernel** with the file on that kernel's **local disk**. A folder
shared between two kernels (virtiofs, 9p, NFS) doesn't qualify, and
`kernel_guard.rs` already refuses it: Linux openers need ext4, xfs or btrfs;
macOS openers need APFS or HFS (#527 D5). One disk mounted by two kernels would
corrupt the filesystem itself.

So within one kernel, any number of Sandboxes share one sqlite file; across
kernels, a sqlite Store can't be shared at all. A postgresql Store has no such
rule: every opener reaches the server over the network.

| Relationship | Where |
| --- | --- |
| **n Sandboxes : 1 Store.** Every Sandbox's daemon opens its project's one Store, so a Commit in one Sandbox is visible at once to the others | all |
| **1 machine : n Stores.** A machine is one kernel and holds many sqlite Stores side by side | every machine; today a VM holds one project's Store |
| **n machines : 1 Store** | postgresql Stores only (#506); today only `docker` Sandboxes support them |
| **1 Workspace : 1 overlay** | every Workspace has its own `overlay.sqlite` scratch file in its daemon, deleted at unmount |

A machine with many Sandboxes is the design. They share its CPUs and memory,
so its size is the practical limit, and a Store that needs more is moved to a
bigger machine. A Store that must be shared across machines is a postgresql
Store.

## 6. Architecture, before and after (macOS)

```
BEFORE: one home and one host process per platform

+---------------------+   +---------------------+   +---------------------+
| process A           |   | process B           |   | process C           |
| home A: local       |   | home B: vm:lima     |   | home C: mac         |
| runtime + SDK       |   | runtime + SDK       |   | SDK only            |
+---------------------+   +---------------------+   +---------------------+
           |                         |                         |
           v                         v                         v
+- Docker VM ---------+   +- Lima VM per proj. -+   +- this Mac ----------+
| worker container    |   | VM worker           |   | mac daemons         |
| Init container      |   | whole folder copied |   |                     |
|                     |   |                     |   |                     |
| store.sqlite        |   | store.sqlite        |   | store.sqlite        |
| in Docker volume    |   | on the VM's disk    |   | at any STORE_PATH   |
+---------------------+   +---------------------+   +---------------------+

metadata: home A, home B, home C, ~/.layerfs/ssh, ~/.layerfs/firecracker,
          LAYERFS_LOCAL_DIR; Store location named by 42 env vars
```

```
AFTER: one home, one host service; machines run Sandboxes, Stores hold history

 env: LAYERFS_HOME only
 +--------------------------------------------------------------------------+
 | host service: the layerfs package, one process                           |
 | Init from host folders; store move, backup, restore; all s3 I/O          |
 | routes each call by its Store's and machine's records                    |
 +--------------------------------------------------------------------------+
                                       |
                                       v
 +- LAYERFS_HOME: the control center ---------------------------------------+
 | layerfs.json  machine/  store/  backup/  trace/  log/                    |
 |                                                                          |
 | store/a/           store/b/           store/c/, d/       store/pg-main/  |
 | sqlite, native     sqlite, docker     sqlite, dev        postgresql      |
 | store.sqlite here  record only        records only       record only     |
 +-------+------------------+------------------+------------------+---------+
         |                  |                  |                  |
         v                  v                  v                  v
 +- native ------+  +- docker ------+  +- dev ---------+  +- PostgreSQL --+
 | built in      |  | built in      |  | lima VM       |  | a server      |
 | host's own OS |  | worker        |  | worker        |  | no machine    |
 | Sandboxes     |  | Sandboxes     |  | Sandboxes     |  | Sandboxes on  |
 | Store a       |  | Store b       |  | Stores c, d   |  | any machine   |
 |               |  | in a volume   |  | on its disk   |  | docker today  |
 +---------------+  +---------------+  +---------------+  +---------------+

 +- s3: r2, minio ----------------------------------------------------------+
 | backups of sqlite Stores only: no Sandboxes; reached by the host service |
 +--------------------------------------------------------------------------+
```

A record's arrow shows where its Store is, not data moving: nothing is copied
per call. On Linux the host service opens a Store on `native` or `docker` in
its own process, as today. On Windows it sends calls to its WSL2 worker:
there `native` holds only records, and its Stores live in the WSL worker
(§13 O5). No guest reads the home (§7.7).

### 6.1 Inside one machine

Every Sandbox of a sqlite Store runs on the Store's machine and opens the one
`store.sqlite` there; nothing is copied per Sandbox. Other Stores on the same
machine sit beside it with their own Sandboxes. For a postgresql Store the
bottom box is the server instead, and its Sandboxes may be on several
machines.

```
+- one machine = one kernel: native / docker / a VM ---------------------+
| worker: only while the host service is on another kernel               |
|                                                                        |
| +- Sandbox "tests" ----------+    +- Sandbox "lint" -----------+       |
| | daemon (one per Sandbox)   |    | daemon (one per Sandbox)   |       |
| |                            |    |                            |       |
| | Workspace 1: mount         |    | Workspace 1: mount         |       |
| |   overlay.sqlite           |    |   overlay.sqlite           |       |
| |   (own, scratch)           |    |   (own, scratch)           |       |
| |                            |    |                            |       |
| | Workspace 2: mount         |    | commands run here          |       |
| |   overlay.sqlite           |    +----------------------------+       |
| |   (own, scratch)           |                                         |
| |                            |                                         |
| | commands run here          |                                         |
| +----------------------------+                                         |
|               |                                 |                      |
|               +---------------------------------+                      |
|                                v                                       |
|            store.sqlite: the Store, one copy,                          |
|            committed history shared by all Sandboxes                   |
+------------------------------------------------------------------------+
```

A Sandbox is a container on `docker`, host processes on `native` (macOS), a
set of namespaces on `native` (Linux), and a set of namespaces in a VM or on
an ssh host. Workspaces mount with FUSE on Linux and loopback NFS on macOS.
Each Workspace's `overlay.sqlite` is its own and is deleted at unmount.

### 6.2 Who calls what

```
 +- caller ---------------+
 | agent, Connect, CLI    |
 | the layerfs package    |
 | needs: LAYERFS_HOME    |
 +------------------------+
     |
     |-- Store calls (project, branch, commit, fs), routed by the records:
     |     sqlite on native, or on docker on Linux:   the caller's process
     |     sqlite on native on Windows:               the WSL worker (O5)
     |     sqlite on docker on macOS or Windows:      the docker worker
     |     sqlite on a VM or ssh machine:             that machine's worker
     |     postgresql:                                the caller's process,
     |                                                over the network
     |
     |-- Workspace calls (mount, exec, commit, status, unmount)
     |     -> the Sandbox's daemon, which opens its project's Store
     |        directly; on another kernel the call reaches the daemon
     |        through the machine's worker
     |
     +-- machine and store management (create, connect, move, backup)
           in the caller's process; all s3 I/O there too
```

The daemon serves no Store calls, as today.

### 6.3 A Commit

```
  Workspace 1 edits --> overlay.sqlite (Workspace 1 only)
                              |
                              | commit: the daemon builds the change
                              v
                        one writer, short hold
                              |
                              v
                        the Store  --> new Commit visible to every
                                       Sandbox on this Store
```

Unchanged by the proposal; shown so the model is complete.

## 7. Proposal

### 7.1 Machines

A machine is **one named kernel where Sandboxes run and sqlite Stores can
live.** It has a name and a kind.

| Kind | Machines on one host | Made by | Sandboxes |
| --- | --- | --- | --- |
| `native` | 1, built in | — (the host itself) | host processes on macOS (#529's `mac`); namespaces on Linux (#529's `linux`); none on Windows (§13 O5) |
| `docker` | 1, built in | — (Docker Desktop's VM, a Linux host's Docker, or Docker in WSL) | containers |
| `apple-container` | many | `machine create`; each its own lightweight VM; Apple silicon, macOS only | namespaces in the VM |
| `microsandbox` | many | `machine create`; each its own microVM | namespaces in the VM |
| `lima` | many | `machine create`; each its own Lima instance; macOS only today | namespaces in the VM |
| `firecracker` | many, in a Lima host | `machine create --host <lima machine>`; every microVM nests in a `lima` machine made with `--nested` (M3 or newer, macOS 15+) and shares it; one `172.30.<n>.0/30` network each, so at most 250 per host | namespaces in the VM |
| `ssh` | one per remote host | `machine connect`; LayerFS never creates or deletes the host | namespaces on the host |

- **A machine's disk layout.** A VM's data disk, or an ssh host's root, is
  mounted at `/layerfs/data`. It holds `store/<name>/store.sqlite` for each
  Store and `sandbox/<name>/` for each Sandbox. Today a VM's disk holds
  `/layerfs/store/global/store.sqlite` and `/layerfs/store/local/`. The new
  layout removes the `store/store` nesting and the clash with a Store named
  `local`.
- **A VM machine holds many Stores** on its one data disk, instead of
  today's disk per project. Its VM is named `<prefix><machine>` instead of
  `<prefix><project>-vm`. Several VMs of one kind on one host already work
  today, one per project.
- **A machine's size and image are its own.** `cpus`, `memory`, `disk` and
  `image` are set at `machine create` (§7.1.1); every Store and Sandbox on it
  shares them. In this proposal `image` is the Sandbox image, the root
  filesystem its Sandboxes run in. Copying its archive into the machine at
  create is new. The VM's own operating system comes from the bundle (§13
  O16).
- **Today `image` means something else.** `vm.image` names the VM's own
  operating system: an OCI image such as `node:22-trixie-slim` for
  apple-container and microsandbox, a disk URL for Lima. It must carry
  Node.js 22. The Sandbox image is the bundle's single
  `images/<digest>.tar`; a bundle with more than one is refused.
- **An ssh host is one machine.** A second machine on the same host would be
  one kernel twice and is refused `Exists`. The host is recognised by its
  host key, or else by `/etc/machine-id` (proposed). It holds many Stores
  under `/layerfs/data`, where today it holds one per path.
- **Removing:** `machine remove` deletes a VM and its disk.
  `machine disconnect` forgets an ssh host. It removes what connect wrote in
  `/layerfs/bundle` and `/layerfs/state`, as today's remove does, and never
  deletes the data root. Both are `InUse` while the machine holds Stores or
  running Sandboxes. `native` and `docker` can't be created, removed or
  disconnected.
- **Agents may create and remove machines** (§12).

**The built-in machines' settings.** `native` and `docker` are never
created, so their settings need a home. Proposed: `machine update native` and
`machine update docker` set the image, the Docker socket and the WSL
distribution (if O11 is accepted). Their binaries come from O16's per-build
cache: the macOS daemon; the Linux daemon, helper and image archive; and the
worker image. `native` on Linux needs a root caller, `/dev/fuse` and cgroup
v2. `machine list` names each one that is missing, as `host:root` and so on,
as `linuxMissing` in `api/runtime/src/sandbox/platforms.js` does today.

**On Windows,** `native` holds only records. Its Stores and Init are in the
WSL worker, on the same kernel as `docker` (§13 O5).

LayerFS sets no machine count of its own; the host's CPU, memory and disk
are the limit. Only macOS was tested (#529). Lima and Firecracker are macOS
only today: `lima.js` always writes `vmType: vz`, and `--nested` is a `vz`
feature. A Linux host needs a qemu template, which is not designed.

#### 7.1.1 Setting up a machine

Only the first step is outside LayerFS:

1. **Install the kind's tool once** (table below).
2. **`machine list`** shows each kind as `ready`, or names what is
   `missing`.
3. **`machine create`** for a VM, **`machine connect`** for an ssh host. Then
   move a Store there and create Sandboxes on it.

```bash
layerfs machine create dev lima --cpus 4 --memory 8G --disk 100G
```

```bash
layerfs store move shop dev
```

```bash
layerfs sandbox create shop/api tests dev
```

| Kind | Install once | Host needs |
| --- | --- | --- |
| `native` | nothing | macOS: Network Volumes access (§7.2). Linux: a root caller, `/dev/fuse`, cgroup v2 |
| `docker` | Docker Desktop, Docker Engine, or Docker in WSL | a reachable Docker socket |
| `lima` | `limactl` | macOS only today (`vmType: vz` is fixed); a Linux host needs a qemu template (not designed) |
| `apple-container` | Apple's `container` CLI | Apple silicon, macOS |
| `microsandbox` | `msb` | what microsandbox supports |
| `firecracker` | nothing more: it runs inside a `lima` machine made with `--nested` | macOS only, through its Lima host: M3 or newer, macOS 15+; `/dev/kvm`, `mkfs.ext4`, `ip` and `flock` in that machine |
| `ssh` | nothing locally | a reachable Linux host; login as root; Node.js 22 on root's PATH until §11 step 15; the host key already known; its data root on ext4 (O18) |

**What `machine create` does.** Today's adapter steps (`vm/index.js`), run
per machine instead of per project. `machine/` in `layerfs-sandbox` runs
steps 1 to 4 and returns the endpoint (§7.10.1); the `machine` calls in the
library crate run step 5.

1. **The VM and its data disk.** `--disk` sizes the disk; Lima formats it
   ext4 and mounts it at `/layerfs/data`.
2. **The Linux bundle** (daemon, worker, helper) is copied in once per
   bundle hash, and the machine's Sandbox image is copied in (new). Lima also
   installs a pinned Node.js 22 on first boot, downloaded from nodejs.org and
   checked against its published sums, so a first create needs the network.
3. **The worker** starts as root with a new secret read from a 0600 file.
   `user.max_user_namespaces=0` is set, so only LayerFS makes namespaces in
   the VM.
4. **One loopback port** on the host is forwarded to the worker, and the
   worker is ready.
5. **The records:** `machine/<name>/worker.json` and
   `machine/<name>/record.json` are written by the caller.

Today's last step, stop or delete, becomes idle stop (§7.2) and
`machine remove`. One attempt. A create that fails before the VM exists
removes what it made. Once the VM exists, a failure keeps it, in state
`failed`, for inspection; today `vm/index.js` `ensure` keeps a failed boot
the same way. An `Unknown` create keeps everything and is written to
`log/uncertain.jsonl` (proposed).

**An ssh host** is connected, not created:

```bash
layerfs machine connect ec2 ec2-1-2-3-4.compute.amazonaws.com \
  --identity /Users/me/.ssh/deploy
```

The user is `root` by default, the port 22 and the data root `/layerfs/data`
(today's default root is `/layerfs/store`). Today `ssh.js` runs every
command as the login user, with no sudo. So the user must be root; a user
that runs root commands through passwordless sudo is not supported today.

- **Login.** `--identity` is optional: with `BatchMode=yes`, ssh-agent and
  the default keys work too. Host keys are checked strictly, as today, so the
  key must already be in known_hosts. Proposed: `--known-hosts <file>`,
  default `~/.ssh/known_hosts`, with the accepted line copied to
  `machine/<name>/known_hosts`.
- **Checks.** `connect` checks that the host answers as Linux and that the
  data root is on ext4 (§13 O18).
- **Setup.** In today's code the `ssh -L` forward opens first. Then
  `connect` copies the bundle, sets the host-wide
  `user.max_user_namespaces=0`, mounts or bind-mounts the data root, writes
  `/layerfs/bundle` and `/layerfs/state`, and starts the worker. Until §11
  step 15 the worker is `nohup node ...`, so the host needs Node.js 22 on
  root's PATH.
- **Disconnect** stops the worker and the forward and removes
  `/layerfs/bundle` and `/layerfs/state`. It never deletes the data root.

**Firecracker** machines nest in a Lima machine:

```bash
layerfs machine create fc-host lima --nested --cpus 8 --memory 16G --disk 200G
```

```bash
layerfs machine create fc1 firecracker --host fc-host --cpus 2 --memory 2G --disk 20G
```

- `--nested` makes a `vz` Lima instance with nested virtualization.
  `machine list` shows it `missing` on a Mac older than M3 or macOS 15.
- Each Firecracker machine gets a copy of the base root filesystem, its own
  `172.30.<n>.0/30` tap network and its own data disk inside the host. The
  network allows at most 250 per host (`firecracker.js` checks
  `[ "$n" -le 250 ]`).
- The host is `InUse` while Firecracker machines exist on it.
- Today you place the Firecracker binary, guest kernel, base root filesystem
  and guest ssh key in `LAYERFS_FIRECRACKER_DIR` inside the host by hand
  (§13 O17). The bundle holds neither the guest kernel nor the root
  filesystem. `rootfs.ext4` must carry Node.js 22 and an sshd that accepts
  the key. The Lima host needs `mkfs.ext4`, `ip` and `flock` (the `CHECK` in
  `firecracker.js`).

### 7.2 Machine lifecycle and workers

A worker is a long-running process on a machine whose kernel the caller
can't open a sqlite Store on: `docker` on macOS and Windows, every VM and
every ssh host. It runs the real SDK, opens that machine's Stores and is
reached through a loopback port on the host, behind a secret. A machine has
**one worker for all its Stores**, where today a VM has one per project.
`native`, `docker` on Linux, postgresql and s3 need none.

**The worker owns idleness.** It closes a Store after 1 minute quiet, as
today (`IDLE_MS = 60_000` in `worker/server.js`). After N minutes with no
open Store and no Sandbox, it exits and, where the provider allows, powers
off its guest. The caller starts a stopped machine on the next call that
needs it:

```
               first call: the caller starts the
               machine and its worker, writes worker.json
 +-----------+                         +---------------+
 |  stopped  | ----------------------> |    running    |
 | 0 process |                         | Store(s) open |
 +-----------+                         +---------------+
       ^                                  |        ^
       |                     quiet 1 min: |        | next call:
       |               the worker closes  |        | reopen Store
       |                      the Store   v        |
       |                               +---------------+
       +------------------------------ |     idle      |
           quiet N min: the worker     | process alive |
           exits and powers off the    | no Store open |
           guest; the disk stays       +---------------+
```

- **Idle** means no Store open **and** no Sandbox on the machine; a machine
  with a running Sandbox never stops.
- The host notices a stop only at the next call. That call finds the worker
  gone, starts the machine and waits until its worker is ready, then
  replaces `worker.json`. The wait is a readiness wait, not a retry; the
  call itself is sent once.
- **Detached helpers.** The ssh `-L` forward and the macOS worker container
  must outlive a one-shot CLI call. They are started detached and recorded
  in `machine/<name>/worker.json`.
- `docker` stops only its worker, never Docker itself. `ssh` stops only its
  worker and forward; the host keeps running. `native` has nothing to stop.
- N is §13 O2. Boot times per kind are not measured.

| Host | Workers while in use |
| --- | --- |
| macOS | 1 for `docker` while a Store on it is in use; 1 per running VM or ssh machine |
| Linux | 0 for `docker` (it shares the host's kernel); 1 per running VM or ssh machine |
| Windows | 1 WSL2 worker |

A Mac without Docker still has `native` and the five VM and ssh kinds.
Workspaces on `native` on macOS are loopback NFS mounts, so the app needs
Network Volumes access in Privacy & Security.

### 7.3 sqlite Stores

A sqlite Store is one `store.sqlite` file on exactly one machine at a time.

- **Created on `native`:** `store create <name>` makes an empty Store at
  `<home>/store/<name>/store.sqlite`. On Windows it is made in the WSL
  worker instead (§13 O5).
- **Moved between machines:** `store move <name> <machine>`. Every move starts
  or ends on `native`. A move between two other machines goes through
  `native` in one call (if O12 is accepted). The Store must be idle: no
  running Sandboxes and no process holding a lease on it (§7.8).
- **Removed:** `store remove <name>` deletes the file wherever it is, with every
  project in it. Its s3 backups stay.

**A move runs these phases, once each:**

| Phase | What happens | Existing code |
| --- | --- | --- |
| check | names, the target machine is ready, the Store is idle | — |
| snapshot | a consistent snapshot on the source machine: the host for `native` and for `docker` on Linux; the worker where the machine has one | `remote/push.rs` |
| send | the bytes through the worker's loopback channel or the VM provider's copy | `vm/*` `copy` covers host to guest only; guest to host is new |
| verify | sha256, the file opens with the Store's binding, history heads | `remote/pull.rs` |
| install | rename into place on the target machine | `remote/pull.rs`, `install.rs` |
| record | the caller writes the Store's `record.json`: its machine and its last move | `stores.rs`, `sync.rs` |
| retire | remove the copy on the source machine | `vm/*`, `store/volume.js` |

One attempt, no retries. A failure reports its phase and keeps whatever
exists. An uncertain outcome (`Unknown`) keeps both copies and writes to
`log/uncertain.jsonl` (proposed); nothing is cleaned up on a guess.

A move reports progress through the call's progress stream (proposed, §13
O22). Another process can't watch a call it did not start. Proposed:
`store.status` shows a move in progress, from the mover's lease and an
`in_progress` field in the Store's record.

**Backups** belong to sqlite Stores and have their own family, `backup`, so
that `store` only ever names live Stores. A bucket connection
(`backup connect <name> <url> --key <ref>`) names an r2 or minio bucket and
prefix, with a key reference. The `local_folder` provider is not in 2.0.
LayerFS never deletes a bucket object.

The rules below are proposed (§13 O23): one link per Store per bucket, which
reverses #509's one link per Store.

- `backup create <store> <bucket> [<file>]` uploads a snapshot to
  `<prefix>/<file>/store.sqlite` and links the Store to it in that bucket.
  `file` is an optional last positional and defaults to the Store's name.
  A large Store is uploaded as a manifest at that key plus pieces named
  `store.sqlite.piece-NNNNNN`. A create is refused `Exists` when the object
  exists with a foreign `layerfs-exported-from`, one another home wrote.
- `backup restore <store> <bucket> [<file>]` downloads, verifies and installs
  a file as a sqlite Store on `native`: new, or replacing an idle Store that
  is on `native`. Replacing is refused `Conflict` when the Store changed
  since its last backup or restore; remove it first to discard the changes.
- `backup list [<bucket>]` lists every file on the connected buckets: whether a
  Store in this home is linked to it, which machine it was exported from, and
  its state against the Store (`in sync`, `Store changed since backup`,
  `s3 changed since`, `both changed`).
- `backup disconnect <bucket>` forgets the bucket and this home's links to
  it; the objects stay. A linked bucket can be disconnected. This replaces
  `store.unlink`.
- 1.1.0's `send` becomes `backup create` into a second bucket.

The Store's `record.json` holds the links:
`backups: { <bucket>: { file, heads, last: { create|restore, at, bytes } } }`.
The state comes from that record plus one HEAD per file. **Exported from**
is new: each backup writes the machine and the home's id into the object's
metadata.

**Where the bytes go.** The snapshot is taken on the Store's own machine and
the bytes pass through the caller's process. So machines never hold bucket
keys and need no path to the bucket. Both are new: today's s3 I/O runs
inside the worker (`backendCall` `runtime.store.pull`), and a snapshot sent
from a guest to the host is the new send phase above.

### 7.4 postgresql Stores

A postgresql Store is **one database**, with LayerFS's tables in one fixed
schema, `layerfs`; the schema is not a setting. Two postgresql Stores on one
server are two databases.

- **`store connect <name> <url> --password <ref>`.** `connect` in `store`
  always means a postgresql Store; `create` always means sqlite. The URL
  has no password; the password is a reference, resolved at each call and
  never stored. One connection checks it. A database that holds LayerFS data
  is read; an empty one is recorded, and the first Init creates the schema.
  The same database under a second name is refused `Exists`. This replaces
  `attach_postgresql`.
- **`store disconnect <name>`** forgets it; the data stays on the server.
- **`synchronous_commit`** stays `off` by default, as today.
- **No conversion:** `start_import_postgresql` is removed, and nothing replaces
  it. A project reaches PostgreSQL only by an Init into it. It is removed
  only after Init into PostgreSQL works (§11 steps 10b and 10c).
- **Init into PostgreSQL is new work.** Today `store/live.rs` `add` in
  `layerfs-project` opens SQLite handles only, and
  `sql/postgresql/schema.sql` has no acquisition tables. It needs a writable
  PostgreSQL open and acquisition backing in the Persistence crate (§10).
- **Sandboxes on any machine** that supports PostgreSQL and can reach the
  server (§7.6).

### 7.5 Init

A project is always named with its Store, `<store>/<project>`. Project names
are unique within a Store only (#506), so `shop/api` and `pg-main/api` are two
projects; a bare `api` is refused, even when one Store holds it, so that a
name keeps its meaning when another Store adds one. This replaces 1.1.0's
`{ store }` option for an ambiguous name.

Init keeps `LayerFS.init`'s arguments, `(project, folder, options)`, as
`project.init`, with no `LayerFS.init` alias (§12); its `project` is
`<store>/<project>`. Its options lose `image` and `store`. `LayerFS.open` is
removed: every call takes its project. Init always reads a host folder and
runs in the caller's process, except on Windows. The Store must exist; Init
never creates one. The Store's kind decides how it writes; there is no kind
flag:

```bash
layerfs project init shop/api ~/src/api
```

```bash
layerfs project init pg-main/load ~/src/load
```

- A sqlite Store must be on a machine whose Store calls run in the caller's
  process: `native`, or `docker` on Linux. A VM or Docker Desktop can't read
  the host folder. A Store on another machine is refused `InvalidInput`
  ("move it to native first"). To run a new project in a VM, Init it on
  `native` and move the Store.
- A Store that doesn't exist is `NotFound`; a project the Store already holds
  is `Exists`.
- On Windows, `native` holds only records; its Stores and Init are in the
  WSL worker (§13 O5).
- On macOS every Init needs a `native` Store, so §13 O7 is required.
- Init keeps its timeout, as a `{ timeout }` option. It reports progress
  through the call's progress stream (proposed, §13 O22).

| Target | Init today | Proposed |
| --- | --- | --- |
| `native` (#529's `mac`, `linux`) | the SDK's or runtime's process | Init in the caller's process, into a Store on `native` |
| `docker` on Linux | the runtime's process, writing into the volume's folder (root) | Init in the caller's process into the volume, unchanged |
| `docker` on macOS | **a separate Init container**, the folder mounted read-only | Init on `native`, then `store move` |
| VMs | **the whole folder copied into the VM**, then Init in its worker | Init on `native`, then `store move` |
| PostgreSQL | not possible; only an import of a SQLite Store | Init straight into the database (new work, §7.4) |
| Windows | the WSL2 worker | unchanged |

**Init into an existing Store is a live add, not a seal.** It is today's
`add` in `layerfs-project` `store/live.rs`: it opens the live Store
writable (`SqliteHandles::open_writable`) and writes through the Store's own
writer. No sealed file is made and nothing is installed. Today only an Init
that makes a Store of its own seals it and installs it (`install_local` in
`install.rs`); Init no longer makes Stores. What crosses the Mac/VM boundary
becomes the Store once, in the move, instead of the source folder plus a
sealed Store.

### 7.6 Sandboxes

A Sandbox is **named, bound to one project when it is created, and runs on
the machine named then.** The machine is not bound to the project: many
projects' Sandboxes share a machine.

```
 sandbox create shop/api tests docker
        |        project and machine are fixed here
        v
 workspace mount tests main
        |        no project needed from here on
        v
 workspace exec tests:/workspaces/1 "npm test"
        |
        v
 sandbox close tests
```

- **Names** are unique in the home, so that the name alone finds the Sandbox.
  `Exists` while any Sandbox has that name; free again after `close`. The
  check runs under the home lock (§7.8). Same rules as project names; the
  reserved names include `native`, `docker` and the Windows device names
  (`con`, `nul`, `aux`, `prn`, `com1` to `com9`, `lpt1` to `lpt9`).
- **The machine is a required argument.** For a sqlite project it must be the
  Store's machine; any other is `Unsupported`, naming where the Store is. The
  Store is never moved or copied implicitly. A postgresql project may name any
  machine with PostgreSQL support.
- **Image:** any digest on `docker`; the machine's image on `native` (Linux),
  VMs and ssh, another is `Unsupported`; none on `native` (macOS), where
  `image` is `InvalidInput` instead of being ignored as today. The image of
  `native` (Linux) is set by `machine update native` (if O11 is accepted).
- **Workspaces:** `workspace mount <sandbox> <branch>` returns
  `<sandbox>:<path>`; `exec`, `commit` and `unmount` take that handle.
  `sandbox get` lists a Sandbox's Workspaces.
- **Records** move to `<home>/machine/<machine>/sandbox/<name>/record.json`,
  naming the project and image. A native Sandbox's working folder is in the
  same folder (§7.8). A Sandbox on a VM or ssh host keeps its folder at
  `/layerfs/data/sandbox/<name>/` on that machine.
- **Close:** `sandbox close` stops it now; a Sandbox still closes itself two
  minutes after it has no Workspace and no process.

**PostgreSQL support per machine today:**

| Machine | Today | In this proposal |
| --- | --- | --- |
| `docker` | works (#506) | unchanged |
| `native` | not refused by the agent SDK, never tested; the host build needs the `postgresql` feature (`layerfs-node/src/postgres.rs` is `Unsupported` without it) | §13 O3; host builds turn the feature on (§7.10) |
| VMs, ssh | refused: `runtime/src/platform/vm/index.js` routes a VM project only to the Store on the VM's disk | §13 O3; the machine also needs a network path to the server |

The password reference is resolved by the caller and handed to the Sandbox's
daemon, as the URL is today; it is never written to the home.

### 7.7 Routes

- **The library finds a project's route from its Store's and machine's
  records:** in its own process for a sqlite Store on `native` (and on
  `docker` on Linux) and for every postgresql Store; otherwise the machine's
  worker. On Windows a Store on `native` is in the WSL worker (§13 O5).
  `useWorker`, its one-endpoint limit and `LAYERFS_WORKER_ENDPOINT` go away,
  so one process can use Stores on several machines at once.
- **Guests never read the home.** The caller reads the machine's and Store's
  records. It sends each worker call the settings that call needs: the
  machine record's Sandbox settings, the Store's path on the machine and the
  image. The caller writes every record itself. A caller of the API passes
  no `deploy`; the library adds it to the worker call. Locks in the home are
  taken by host processes only.
- **Today guests do read it.** Lima, apple-container and microsandbox
  bind-mount the host home at `/layerfs/home`, writable. ssh and Firecracker
  mount no home. Only the macOS Docker worker mounts the home at its own
  path. The `/layerfs/home` share is dropped: a root worker in one guest
  could otherwise read every machine's `worker.json` secret, and `flock`
  across virtiofs or 9p is not reliable.
- **How workers listen.** On the host the worker is reached through a
  loopback port, behind a secret, with one attempt and no replay. Inside a
  guest the worker binds `0.0.0.0:7070` today (`workerEnv` in
  `vm/index.js`, `worker/server.js`). On an ssh host it is therefore
  reachable on every interface, protected only by its secret. Proposed: the
  worker binds `127.0.0.1` in the guest wherever the forward allows it.
- **Unchanged:** on Linux, sqlite Store calls for a Store on `native` or
  `docker` run in the caller's process.

### 7.8 The home is the control center

There is no new service or repository. The home holds the state; the
`layerfs` package reads and writes it. A separate service would sit between
callers and the Store, which AGENTS.md forbids. Several host services
(Connect, dsh, the CLI) can share one home, taking the home lock to write
(below).

The home has three parts, one per thing a host manages: compute
(`machine/`), storage (`store/`) and backups (`backup/`). **What never moves
nests under its owner; what can move stays at the top.** A Sandbox never
leaves its machine, so it lives under it; a project never leaves its Store, so
it lives under the Store. A sqlite Store moves between machines, so it stays
at the top and its record names its machine.

This listing is of home files, not source, so it has no LOC column.

```diff
  <home>/                    (LAYERFS_HOME: the control center)
+   layerfs.json             (layout version and the home's id)
+   layerfs.lock             (the home lock: record writes, Sandbox names)
+   machine/                 (compute)
+     <name>/                (native, docker, each made or connected)
+       record.json          (kind, size, image, ssh host; lasting state)
+       worker.json          (port, secret, forward; 0600, while it runs)
+       known_hosts          (ssh: the host key accepted at connect)
+       sandbox/
+         <name>/            (named, unique in the home; was numbered)
+           record.json      (project, image, Workspaces)
+           intent.json      (while create runs; was create.intent)
+           work/            (native only: was LAYERFS_LOCAL_DIR)
+   store/                   (storage)
+     <name>/
+       record.json          (kind, machine or url, last move, backups)
+       store.sqlite         (sqlite, while it is on native)
+       open/
+         <pid>              (a lease while that process uses the Store)
+       project/
+         <name>/
+           record.json      (the one project record)
+           ownership.lock
+   backup/
+     <bucket>/
+       record.json          (provider, url, prefix, region, key reference)
+   trace/                   (was traces/)
+   log/
+     init-failures.jsonl    (was the runtime's stateDir)
+     uncertain.jsonl        (proposed: Unknown moves, backups, creates)
-   remotes                  (becomes backup/<bucket>/record.json)
-   remote-state/
-     <store>                (folded into store/<name>/record.json)
-   stores.json              (folded into each Store's record.json)
-   stores.v1.json           (folded into each Store's record.json)
-   .stores/                 (becomes store/)
-   <project>/               (moves to store/<store>/project/<project>/)
-   .runtime/                (into project records; failures to log/)
-   worker/                  (becomes machine/<name>/worker.json)
-   traces/                  (becomes trace/)
```

- **Every record is `record.json`;** the folder says what it records.
- **A machine's record** holds its kind, size and image. For ssh it adds the
  host, user, port, identity path and data root; for Firecracker, its Lima
  host. It also holds lasting guest state, such as Firecracker's tap index,
  which today lives in `~/.layerfs/ssh/` and `~/.layerfs/firecracker/`.
  Live forward state goes in `worker.json`. There is no `session.json`.
- **A Store's record** holds, for sqlite, its machine, its last move and its
  backup links (§7.3); for postgresql, the URL without a password and the
  password reference.
- **The Store is the authority for projects.** The home's project records
  are rebuilt from the Store on `store connect` and `backup restore`.
- **Init keeps no files in the home** while it runs: it is a live add
  (§7.5) and reports progress through its call (§13 O22). Its failures go to
  `log/init-failures.jsonl`.
- **A Sandbox is found by name** by checking `machine/*/sandbox/<name>/`, one
  directory per machine. `sandbox create` refuses a name that exists under any
  machine.
- **There is no `platform/` folder.** Firecracker's Lima host is an ordinary
  `lima` machine made with `--nested` (§7.1.1), and each Firecracker machine's
  record names it.
- **`machine/native/` and `machine/docker/`** exist from the first call and
  are never removed.
- **Secrets:** none of the caller's credentials (bucket keys, PostgreSQL
  passwords, ssh private keys) is in the home; records hold references and the
  identity file's path. The one secret LayerFS makes itself, a worker's, stays
  in its 0600 `worker.json`, as in today's `worker/<project>.json`.

**Leases and the home lock.** Every process that uses a sqlite Store holds a
lease file, `store/<name>/open/<pid>`. That is the process that opens the
Store, or, for a Store behind a worker, the host process that sends it
calls; guests never write the home. `store move` and `store remove` check
the leases under the home lock and refuse `InUse` while any exists. One home
lock, `<home>/layerfs.lock`, guards record writes and Sandbox-name
uniqueness; Sandboxes stay filed under their machine (§12). Only host
processes take it. Taking it waits, bounded: a readiness wait, never a
`Busy` refusal. Today `store/registry.js` takes an exclusive-create lock
file and refuses `Busy`.

**Still outside the home, on purpose:**

- a sqlite Store's file while it is on another machine. Its `record.json`
  says where: on a VM or ssh host it is
  `/layerfs/data/store/<name>/store.sqlite`, and on `docker` it is in a
  Docker volume;
- a VM or ssh Sandbox's folder, `/layerfs/data/sandbox/<name>/`;
- the VM tools' own folders (`~/.lima`, `~/.microsandbox`, Apple
  Container's store);
- Firecracker's files inside its Lima host;
- the sealed Linux bundle (§13 O16);
- a Store's open overlay in `$TMPDIR`;
- secrets;
- on Windows, the WSL home, which the Windows home points to.

### 7.9 One package, nine families

The calls are specified in [the `layerfs` command and SDK](layerfs-sdk-cli.md).
In short:

- **One package, `layerfs`,** replaces `@layerfs/agent`, `@layerfs/runtime`,
  `@layerfs/backend` and `@layerfs/cli`, on npm and PyPI and as a binary
  (§7.10). The npm package is `api/node/` and the PyPI package
  `api/python/`. There is no split between agents, operators and the
  command; every caller gets every call. Anything with the home could already
  do all of it, so the split was a convention, not a boundary. A call that
  needs a secret is `Denied` to a process without the reference's variable or
  file.
- **Nine families, the same in code and in the shell:** `project`, `branch`,
  `commit`, `fs`, `sandbox`, `workspace`, `machine`, `store`, `backup`;
  54 calls. `layerfs <family> <call> <args> --opts` is
  `family.call(args, { opts })`, with the same result and error.
- **Positional arguments say what and where; options say how.** A project is
  always `<store>/<project>`; a Commit is an optional last positional.

```js
import { project, branch, commit, fs, sandbox, workspace, machine, store, backup } from "layerfs"

await store.create("shop")
await project.init("shop/api", "/Users/me/api")
await store.move("shop", "docker")
await fs.read("shop/api", "main", "/src/index.ts")
const sb = await sandbox.create("shop/api", "tests", "docker")
const ws = await workspace.mount("tests", "main")
await ws.exec("npm test")
await ws.commit({ message: "fix tests" })
await ws.unmount()
await sb.close()
await backup.create("shop", "r2-main")
```

**Removed:**

```diff
- @layerfs/agent, runtime, backend, cli  // one package, layerfs (§7.10)
- LayerFSRuntime.open({ placement, … })  // the home's records
- LayerFS.open(project), client options  // every call takes its project
- runtime.store.create / remove / list   // store.*
- runtime.store.check                    // store.status
- runtime.store.start_publish / start_push   // backup.create
- runtime.store.start_send               // backup.create, a second bucket
- runtime.store.start_pull / start_load  // backup.restore
- runtime.store.unlink                   // backup.disconnect (O23)
- runtime.store.attach_postgresql        // store.connect
- runtime.store.start_import_postgresql  // nothing: no conversion
- runtime.remote.*                       // backup.connect, disconnect, list
- runtime.postgresql.status({ url, schemas })  // store.status
- sandbox numbers, LAYERFS_PROJECT, --project
- the confirm option on every remove
```

**Other 1.1.0 calls.** Every other 1.1.0 call and export has a stated fate:

| 1.1.0 | In 2.0 |
| --- | --- |
| `@layerfs/backend` (14): `store.project.get_history`, `get_storage`, `get_comparable`, `list_overwritten`; `store.branch.list_commit_storage`; `store.fs.get_storage`, `list_revisions`; `daemon.workspace.status`, `list_changes`, `get_storage`; `daemon.process.start`, `list`, `cancel`; `daemon.sandbox.get_log` | not in 2.0's 54 calls; open, §13 O25. `workspace.status` is §13 O14 |
| runtime `status` | `machine.list`, `store.list` |
| runtime `worker.start`, `worker.restart` | automatic (§7.2) |
| runtime `project.get_state` | `project.get` |
| runtime `project.delete` | open, §13 O24 (proposed: `project.remove`) |
| runtime `project.start_init`, `get_init` | `project.init` with its progress stream (O22) |
| runtime `sync.get` | the progress stream (O22) |
| runtime `remote.providers` | `backup.connect` checks the provider |
| runtime `sandbox.reclaim`, `list_leftovers`, `clean_leftovers`, `list`, `sweep` | `sandbox.list` and automatic cleanup |
| runtime `volume.list`, `volume.remove` | `store.list`, `store.remove` |
| runtime `vm.status`, `vm.stop`, `vm.delete` | `machine.status`, idle stop, `machine.remove` |
| runtime `events.subscribe` | open, §13 O26 (proposed: trace files under `trace/` are the feed; no call) |
| runtime `close` | none |
| agent `close()`, `[Symbol.asyncDispose]` | none: there are no clients |
| agent `LayerFS.call` | the call table's `call(name, args)`, exported for scripts (proposed) |
| agent `CODES`, `isRefusal` | kept as exports |
| agent `RENAMED_METHODS`, `currentMethod` | removed |
| agent `INIT_TIMEOUT_MS` | kept, as `project.init`'s default timeout |

### 7.10 One Rust implementation, three front ends

Today the logic between a caller and the Store is JavaScript, and the
`layerfs` command is a Node program on top of it. Production LOC at
`ddc7c5696b`, from `tools/production_loc.py --detail`:

| Part | Language | Production LOC |
| --- | --- | --- |
| `@layerfs/runtime` | JavaScript | 3,214 |
| `@layerfs/agent` | JavaScript | 981 |
| `@layerfs/cli` | JavaScript | 554 |
| `@layerfs/backend` | JavaScript | 130 |
| `layerfs-node`, the napi binding | Rust | 6,083 |

So the command needs Node, and a Python SDK would have to port the 4,879
lines of JavaScript again or run Node underneath.

```
BEFORE (1.1.0): the logic is in JavaScript; the CLI runs on Node

  shell                    Node app                Python app
    |                         |                        |
    v                         |                        X  no route: port the
+------------------+          |                           JS again, or run Node
| layerfs CLI (JS) |          |
| needs Node       |          |
+--------+---------+          |
         |                    |
         v                    v
+------------------------------------------------+
| JavaScript: runtime, agent, backend packages   |
| platform adapters, Store registry, Init,       |
| routing, worker client          4,879 prod LOC |
+-----------------------+------------------------+
                        | napi
                        v
+------------------------------------------------+
| Rust binding (layerfs-node)     6,083 prod LOC |
| call(op, args) dispatcher                      |
+-----------------------+------------------------+
                        v
                 LayerFS Rust crates
```

**Proposed:** all of it moves into one Rust library, and the command, Node
and Python become thin front ends over it.

```
AFTER (proposed): one Rust implementation, three thin front ends

   shell               Node app                Python app
     |                     |                        |
     v                     v                        v
+----------+       +---------------+       +----------------+
| layerfs  |       | napi-rs addon |       | PyO3 module    |
| binary   |       | + .d.ts types |       | + .pyi types   |
| no Node  |       | npm           |       | PyPI wheel     |
+----+-----+       +-------+-------+       +--------+-------+
     |                     |                        |
     +---------------------+------------------------+
                           |
                           |  call("store.move", json) -> json
                           |  plus one stream: exec output, progress
                           v
+-------------------------------------------------------------+
| layerfs-api (Rust), grown in place: the one implementation  |
|                                                             |
| call table          54 calls, 9 families; generates the     |
|                     CLI help, .d.ts and .pyi                |
| home and records    machine/, store/, backup/               |
| machines            calls, records, start; adapters in      |
|                     layerfs-sandbox machine/ (§7.10.1)      |
| Stores              Init, move, postgresql, backup (s3)     |
| routes              in-process, or the machine's worker     |
+------------------------------+------------------------------+
                               |
              +----------------+----------------+
              v                                 v
   +---------------------+          +-----------------------+
   | in this process     |          | the machine's worker  |
   | sqlite on native,   |          | VMs, ssh, docker on   |
   | docker on Linux,    |          | macOS and Windows;    |
   | postgresql          |          | loopback, secret,     |
   |                     |          | one attempt           |
   +---------------------+          +-----------------------+
```

- **One entry.** The library answers `call(name, args) -> result` with
  JSON-shaped values, driven by one table of the 54 calls. One stream
  carries `workspace.exec`'s output and the progress of `store.move`,
  `backup.create`, `backup.restore` and `project.init` (proposed, §13 O22).
  `layerfs-node` already dispatches this way (`call(op, args)` in
  `dispatch.rs`). Its host logic moves into `layerfs-api` (§13 O21a), and it
  keeps only the napi conversion. "The library crate" below means
  `layerfs-api` grown in place.
- **Front ends only translate.** Each parses its own arguments, makes one
  call, and returns or prints the result: the `layerfs` binary, a napi-rs
  addon for Node and a PyO3 module for Python. `layerfs store move shop dev`
  and `store.move("shop", "dev")` in Node or Python are one call, with one
  result and one `LayerFSError`.
- **Generated from the table:** the command's help and the `.d.ts` and
  `.pyi` types.
- **What moves to Rust:** the runtime's platform adapters (into
  `layerfs-sandbox` `machine/`, §7.10.1), Store registry, Init, routing and
  worker client, and the agent and backend packages' calls.
  Most of the runtime starts `docker`, `limactl`, `container`, `msb` and
  `ssh` and handles files.
- **The worker becomes the binary** (`layerfs worker`), so VMs and ssh hosts
  need no Node.js; today Lima installs Node.js 22 in each guest for it.
- **Unchanged:** on Linux, sqlite Store calls for a Store on `native` or
  `docker` run in the caller's process, for Node and Python alike. A worker
  is reached through a loopback port on the host, behind a secret, with one
  attempt and no replay. Inside a guest it binds `0.0.0.0:7070` today;
  binding `127.0.0.1` is proposed (§7.7).
- **Packaging:** prebuilt per target: the binary, npm packages per OS and
  CPU as the addon ships today (`api/node/`), and PyPI wheels (`api/python/`).
  The Linux bundle stays out of all three (§13 O16).
- **Python** gets a synchronous API and an asyncio one; the module releases
  the GIL during a call (proposed, §13 O19).
- **Builds.** Host builds (macOS, Linux and the Windows front end) turn on
  the `machine`, `tls` and `postgresql` features, since s3 I/O and
  PostgreSQL move to the caller. The guest `layerfs worker` is a second
  Linux build without `machine`. It is built in its own
  `cargo build -p ... --no-default-features` invocation, because resolver 2
  unifies features within one invocation and `layerfs-sandbox-helper` is a
  bin of the same package. The library crate gets its own `machine` feature,
  which turns on `layerfs-sandbox/machine`.

**Not chosen:**

| Approach | Why not |
| --- | --- |
| The command as the implementation; SDKs start it per call | a process start and a Store open on every call; streaming `exec` and long-lived Workspaces over pipes; a killed child leaves an uncertain outcome |
| A local daemon with RPC, as Docker's CLI and SDKs share dockerd | the host server AGENTS.md forbids: Linux Store calls run in the caller's process |
| A C library with ctypes, cffi or koffi wrappers | async, streaming and memory ownership by hand, which napi-rs and PyO3 provide |

#### 7.10.1 Machines and Sandbox backends

A machine is **which kernel**; a Sandbox backend is **how a Sandbox runs on
it**. Seven machine kinds map onto three backends, and callers never choose
the backend. The pairing today is in `api/agent/src/platform.js`
(`platformDeployment`) and `api/runtime/src/platform/vm/index.js`:

| Machine | The kernel | Sandbox backend | Where the backend runs |
| --- | --- | --- | --- |
| `native` on macOS | the Mac's | `native`: host processes, loopback NFS | the caller's process |
| `native` on Linux | the host's | `namespaces` | the caller's process |
| `docker` | Docker's: Desktop's VM or the Linux host | `docker`: containers | the caller on Linux; the docker worker on macOS and Windows |
| `apple-container`, `microsandbox`, `lima`, `firecracker` | a VM | `namespaces` | the VM's worker |
| `ssh` | the remote host | `namespaces` | the host's worker |

Today a Linux host can also run the `native` backend
(`LAYERFS_SANDBOX=native`). In 2.0 `native` on Linux uses `namespaces`.
Proposed: remove the Linux `native` pairing and `backend/native/linux/`
(133 LOC), unless the owner keeps it (§13 O27).

Both live in `layerfs-sandbox`, as two layers:

- **`machine/`** (new, behind a `machine` cargo feature, host only): one
  adapter per kind. Each names its backend and its route (in-process or the
  machine's worker) and makes and starts the kernel. It runs setup steps 1
  to 4 (VM and disk, bundle, worker, forward and readiness; §7.1.1) and
  returns the endpoint. It knows nothing of the home: the `machine` calls in
  the library crate write `worker.json` and `record.json`, and keep `InUse`.
- **`backend/`** (renamed from `platform/`): `docker`, `namespaces` and
  `native`, unchanged.
- **`docker_engine/`** (moved up from `platform/docker/client/`): the Docker
  Engine client both layers use. It depends on `types/` and `json/`.

Rules: `backend/` never uses `machine/`. `machine/` uses only
`docker_engine/`, `types/`, `json/` and `backend/`'s published types. The
kind-to-backend table lives once, in `machine/mod.rs`. Guest builds (the
bundle's worker and `layerfs-sandbox-helper`) leave the feature off, and
there the machine calls answer `Unsupported`.

Work the move needs:

- `layerfs_sandbox::docker::Docker` is public today. `lib.rs` re-exports
  `backend::docker` under the old path, so the rename stays a pure move.
- `backend/` is `#[cfg(unix)]` and `nix` is an unconditional dependency.
  `machine/docker/windows.rs` therefore needs its own cfg and dependency
  plan, which is not designed yet.

"Platform" then means one thing: a machine's kind, the word callers see
(`layerfs machine create dev lima`). This reverses #527 D7's name for the
backend folder; D7's reason, that the runtime's `platform/` meant placement,
goes away with the runtime. Folding backends into machines was not chosen:
`namespaces` serves six kinds (`native` on Linux, the four VM kinds and
`ssh`) and runs inside the guests.

### 7.11 Connect

Connect uses the `layerfs` package. Its changes are a follow-up in the
layerfs-connect repository:

- **Settings:** `settings.json` and `LAYERFS_CONNECT_*` shrink to the home's
  path plus Connect's own UI settings.
- **Compute page:** `machine.list()`, with the kinds that could be created
  and what each is missing; create, connect, remove.
- **Storage page:** `store.list()` with each Store's machine, size and backup
  state; `backup.list()` for the bucket side. One progress view covers moves,
  backups and restores, from each call's progress stream (O22). Connect stops
  checking file sizes itself through `docker exec`.
- **Activity:** Connect reads the trace files under `trace/` (if O26 is
  accepted).
- **New project:** `project.init` into an existing Store, then an optional
  move. **New Sandbox:** a name, and the machines the project's Store allows.
- **Agents' bash calls:** `LAYERFS_HOME` and the trace settings; every call
  names its project.
- **Scenarios with `-platform`** run in one home; the platform pass needs one
  home, not eight.

## 8. Performance

**Running a project does not change.** Daemons and routes stay the same. Each
process reads its Store's and machine's records once and caches them, and
callers stop passing `deploy`; both are far below the 5 ms per-call floor. The
first call to a stopped machine pays its start once (§7.2), which is not
measured per kind.

Moving the JavaScript into Rust (§7.10) leaves the work per call the same
and drops one JavaScript-to-native hop. The `layerfs` binary starts without
Node; neither is measured.

For a Mac without Docker, speed depends on the machine. From the #529 pass
(rough: platforms ran in parallel), geometric mean of the 12 cells against
Docker Desktop. The rows use #529's platform names; `mac` is `native` on
macOS.

| Sandbox platform | vs Docker Desktop |
| --- | ---: |
| `apple-container`, `microsandbox` | 0.78× |
| `ssh`, `lima` | 0.87×, 0.88× |
| `firecracker` | 1.89× |
| `mac` | 2.02× |

`mac` is about twice as slow on cells that create or check 1,000 files, which
pay for the loopback NFS mount. The gap exists today; this proposal neither
causes nor fixes it. Several Stores on one VM share its CPUs and memory; one
VM per project did not, so a busy machine may need a bigger size.

**Init plus a move is expected to be faster than today's Init for `docker` on
macOS and for VMs, but is unmeasured.** Measure under the
[measurement workflow](../../../docs/general/agent-measurement-policy.md) (one
sample per case, cold cache, `--setup clone`): a small repository, one with
many small files (a `node_modules`) and one with large files; today's Init
against Init on `native` plus a move, for `docker` on macOS and one VM;
Init time, Store size, and whether the two Stores are identical, including
owners, modes and file-name case.

A postgresql Store's Sandboxes read committed content over the network, so
they are expected to be slower than a local sqlite file; nothing here
measures it. Each move, backup or restore copies the whole Store file;
copying only what changed is later work (#495's bundles).

## 9. What does not change

- One kernel opens each sqlite Store. On Linux, sqlite Store calls for a
  Store on `native` or `docker` run in the caller's process. The macOS and
  Windows worker exceptions keep the worker rules (loopback, secret, one
  attempt, no replay); the routes they name change as §9.1 lists.
- Init's arguments, `(project, folder, options)`, now as `project.init`.
  Its options lose `image` and `store`.
- The Rust Sandbox backends `docker`, `native` and `namespaces`, and the
  record's `runtime` field, do not change. #529's `platform` option and its
  eight names are replaced by the machine argument. The 16-Workspace limit
  and idle close stay.
- The Disposable sqlite Store profile. The Store record says Disposable,
  the only allowed value. (`LAYERFS_STORE_POLICY` is a different thing:
  Init's construction policy. It leaves the environment, and Init uses the
  default policy.)
- A backup is one s3 object, or a manifest and pieces when large; the s3
  providers stay `minio` and `r2`. Links become one per Store per bucket
  (O23).
- The project, branch, commit and fs operations' behaviour, JSON output and
  error codes. Their argument order changes
  ([the `layerfs` command and SDK](layerfs-sdk-cli.md), §5).
- No secret is written to the home.

### 9.1 AGENTS.md changes this proposal needs

The proposal contradicts four rules in AGENTS.md as of `ddc7c5696b`. Each
needs the owner's approval of new wording before the code that breaks it
lands (§11 step 1). The wording below is proposed.

1. **Direct Store access.** Today: "SDK Store calls (fs, branch, project,
   operator) run in the SDK's own process, ... It is Linux only". The
   proposal runs postgresql Store calls and s3 I/O in the caller's process on
   macOS and Windows too. Proposed: "sqlite Store calls run in a process on
   the Store's kernel; postgresql Store calls run in the caller's process on
   any OS."
2. **VM placement.** Today: "One Linux VM per Store (V1)", naming
   `LAYERFS_PLACEMENT`, `useWorker((project) => endpoint)`,
   `project.start_init` and `project.delete`. Proposed: "A VM machine holds
   many Stores, with one worker per machine. Machines are created, removed
   and started by the `machine` calls and the routes in the home's records."
3. **Plugin-neutral LayerFS.** Today it names `@layerfs/runtime`,
   `useWorker(endpoint)` and `LAYERFS_WORKER_ENDPOINT`, and says LayerFS
   reads `LAYERFS_*` settings. Proposed: "The layerfs package routes by the
   home's records; LayerFS reads `LAYERFS_HOME` and the trace settings
   only."
4. **The macOS native exception (#527 D5).** Today it is selected with
   `LAYERFS_SANDBOX=native`. Proposed: name the `native` machine on macOS
   instead; the rest of the exception is unchanged.

One more sentence is stale, though the proposal does not break a rule with
it: the Target says "Project Init creates and seals one Store file,
installed into that volume". Init never creates a Store here (§7.5).
Proposed: "Project Init adds a project to an existing Store as a live
add."

## 10. Code layout

Production LOC at `ddc7c5696b` (#529 head), from `tools/production_loc.py
--files`, `api` and `engine` scopes. The listings are partial: they show the
files this proposal changes, moves or removes. A bracketed total marked
partial covers only the files shown. The runtime keeps its `platform/`
folder name, as the owner decided for #529.

These listings keep today's paths. The Rust library lands before any
behaviour change (§11 step 2), so the JavaScript below moves into Rust
instead of being edited in place, and no new JavaScript file is written. Each annotation says what that
file's behaviour becomes.

```diff
  api/runtime/src/        [3214]  (every file)
    index.js                150   (the home's collections; homeLevel refusals go)
    settings.js             125   (shrinks to `home`)
    clients.js               96   (withEnv and deploymentEnv go; route by records)
    status.js                28   (becomes machine.list and store.list)
    volume.js                51   (becomes store.list and store.remove)
    events/                 [59]
      subscribe.js           59   (no call if O26 is accepted: trace files)
    platform/             [1673]  (ports to layerfs-sandbox machine/, §7.10.1)
      shared.js              59   (endpoints: machine/<name>/worker.json)
      docker/              [403]
        disk.js              11
        index.js             17
        linux.js             32
        macos.js            111   (worker container started detached, §7.2)
        sandboxes.js         43
        stores.js            37
        windows/           [152]
          index.js          135
          paths.js           17
      vm/                 [1211]
        index.js            317   (a VM per machine; many Stores; one worker)
        apple-container.js  110   (no /layerfs/home share)
        cli.js               30
        firecracker.js      223   (host is a --nested lima machine)
        lima.js             175   (--nested hosts; no /layerfs/home share)
        microsandbox.js      72   (no /layerfs/home share)
        ssh.js              284   (connected, not created; root /layerfs/data)
    project/               [266]
      delete.js              51   (project.remove if O24 is accepted)
      init.js               118   (Init in the caller, into <store>/<project>)
      jobs.js                28   (no job registry if O22 is accepted)
      list.js                38   (second project record removed)
      state.js               31   (get_state becomes project.get)
    sandbox/               [116]
      leftovers.js           27   (automatic cleanup)
      list.js                21   (becomes sandbox.list)
      platforms.js           23   (allowedOf goes; the Store and machine decide)
      reclaim.js              5   (automatic cleanup)
      records.js             12   (named, home-wide records)
      sweep.js               28   (automatic cleanup)
    store/                 [426]
      calls.js              131   (list, get, status, create, remove)
      registry.js           108   (store/<name>/record.json; home lock waits)
      sync.js               176   (start_* replaced by move, backup, restore)
      volume.js              11
    worker/                [224]
      call.js                43
-     init.js                23   (removed: no Init container)
      server.js             158   (its machine's Stores; settings per call)
```

```diff
  api/agent/src/       [530]  (partial)
    client.js           146   (deployment() and env reads removed)
    errors.js            18   (moves to api/node/)
    index.js            121   (exports per §7.9; LayerFS.init, open go)
    names.js             18   (<store>/<project> and Sandbox names)
    platform.js          50   (homeOf and platformDeployment removed)
    project/             [8]  (partial)
      list_sandboxes.js   8   (becomes sandbox.list)
    sandbox/            [12]  (partial)
      create.js          12   (a name and a machine; no numbers)
    transport/         [157]  (partial)
      index.js           24   (route by records; useWorker removed)
      worker_client.js  133   (initRun removed)
```

```diff
  engine/crates/layerfs-node/src/  [3907]  (partial)
    copies.rs                        146   (to the library's backup/files.rs)
    discovery.rs                     888   (new home layout, §7.8)
    local.rs                         437   (folders in machine/native/sandbox/)
    local_stores.rs                  115   (size and heads of Stores on native)
    postgres.rs                      353   (to the library's store/postgresql.rs)
    progress.rs                       47   (the library's progress, if O22)
    project.rs                       233   (Init as a live add; PostgreSQL in 10b)
    registry.rs                      119   (running Inits, live Sandboxes by name)
    sandbox.rs                       504   (named; deploy from the record)
    stores.rs                        529   (one record per Store, either kind)
    sync.rs                          536   (to the library's store/transfer.rs)
```

Init into PostgreSQL also touches the Persistence crate (§7.4). Steps 10a to
10c of §11 split the work: the import goes only after Init into PostgreSQL
works.

```diff
  engine/crates/layerfs-persistence/  [703]  (partial)
    sql/                              [216]  (partial)
      postgresql/                     [216]  (partial)
        schema.sql                     216   (no acquisition tables today)
+       acquisition/                     —   cond. (acquisition tables for Init)
    src/                              [487]  (partial)
      backend/                        [410]  (partial)
        postgresql/                   [410]  (partial)
          mod.rs                        11   (import replaced by acquisition)
+         acquisition.rs                 —   new (Init into PostgreSQL)
-         import.rs                    399   (removed in 10c: no conversion)
      store/                           [77]  (partial)
        postgresql.rs                   77   (import() goes in 10c; writable open)
```

The bucket code in `layerfs-store-service` splits: object naming and
transfers stay, and the remotes list and link state move to the library.

```diff
  engine/crates/layerfs-store-service/src/  [1036]  (partial)
    install.rs                                187   (installs a file not sealed)
    remote/                                  [849]  (partial)
      mod.rs                                   13   (exports no Remotes or state)
      object.rs                               238   (was settings.rs, less remotes)
      pull.rs                                 233   (reused: download, verify)
      push.rs                                 197   (writes layerfs-exported-from)
-     state.rs                                168   (to the library's backup/)
```

The library crate's backup and Store files, as moves into `layerfs-api`
(§13 O21a). Its other new modules are not listed here.

```diff
  engine/crates/layerfs-api/src/  [1203]  (partial: moved-in files only)
+   backup/                        [314]  (new; LOC where the files are today)
+     buckets.rs                      —   new (bucket records, remotes parsing)
      files.rs                      146   (from layerfs-node copies.rs)
      links.rs                      168   (from store-service remote/state.rs)
+   store/                         [889]  (new; LOC where the files are today)
      postgresql.rs                 353   (from layerfs-node postgres.rs)
      transfer.rs                   536   (from layerfs-node sync.rs)
```

The packages become one, `layerfs`. The npm and PyPI packages hold no
hand-written JavaScript or Python beyond `errors.js`, moved from the agent.

```diff
  api/            [4879]
-   agent/src/     [981]  (moves to layerfs-api; errors.js to node/)
-   backend/src/   [130]  (moves to layerfs-api)
-   cli/src/       [554]  (replaced by the layerfs binary)
+   node/             —   new (npm layerfs: generated loader and .d.ts)
+   python/           —   new (PyPI layerfs: generated .pyi)
-   runtime/src/  [3214]  (to layerfs-api; platform/ to machine/)
```

The crates stay in `engine/crates/` for 2.0 (if O21b is accepted), with
`layerfs-cli` and `layerfs-python` beside `layerfs-node`. The LOC counter
must learn them (§11 step 2).

```diff
  engine/crates/     [16476]  (partial)
    layerfs-api/      [2103]  (grows into the library, O21a)
+   layerfs-cli/          —   new (the layerfs binary, also the worker)
    layerfs-node/     [6083]  (host logic moves out; napi conversion stays)
+   layerfs-python/       —   new (the PyO3 module; generated .pyi)
    layerfs-sandbox/  [8290]  (machine/ added; listed below)
```

`layerfs-sandbox` in full (§7.10.1). The renames (`platform/` to `backend/`,
`platform/docker/client/` to `docker_engine/`) are one pure-move commit;
`machine/` follows in the porting steps.

```diff
  engine/crates/layerfs-sandbox/src/      [8290]
    lib.rs                                   26   (exports machine, backend; re-exports docker)
    runtime.rs                              250   (the one match over backends)
    helper_spec.rs                          110
    json.rs                                 233
    json/                                  [172]
      strings.rs                            172
    types/                                 [486]
      mod.rs                                 22
      capabilities.rs                        29
      execution.rs                           35
      failure.rs                             97
      identity.rs                            74
      image.rs                               25
      options.rs                            138
      request.rs                             49
      transfer.rs                            17
    docker_engine/                         [983]  (from platform/docker/client/; shared)
      mod.rs                                  6
      body.rs                               207
      http.rs                               565
      mux.rs                                 10
      socket.rs                             119
      version.rs                             76
+   machine/                                  —   new (feature "machine", host only)
+     mod.rs                                  —   new (the Machine trait; kind to backend)
+     spec.rs                                 —   new (size, image, ssh host; no home paths)
+     setup.rs                                —   new (create steps 1 to 4; returns the endpoint)
+     probe.rs                                —   new (what each kind is missing)
+     native.rs                               —   new (the host: nothing to make)
+     docker/                                 —   new (backend docker; uses docker_engine/)
+       mod.rs                                —   new
+       linux.rs                              —   new
+       macos.rs                              —   new (the worker container)
+       windows.rs                            —   new (WSL2; own cfg and dependencies)
+     vm/                                     —   new (backend namespaces, through the worker)
+       mod.rs                                —   new (ports vm/index.js)
+       tool.rs                               —   new (ports vm/cli.js)
+       apple_container.rs                    —   new
+       microsandbox.rs                       —   new
+       lima.rs                               —   new (also --nested hosts)
+       firecracker.rs                        —   new
+       ssh.rs                                —   new
    backend/                              [5596]  (moved from platform/)
      mod.rs                                  5
      docker/                             [2129]
        mod.rs                               23
        create.rs                           178
        endpoint.rs                         273
        exec.rs                             314
        failure.rs                           29
        listener.rs                         197
        provider.rs                         116
        sandbox.rs                          207
        streams.rs                          375
        topology.rs                         189
        upload.rs                           228
      namespaces/                         [2009]
        mod.rs                               11
        cgroup.rs                            62
        create.rs                           254
        exec.rs                             103
        host.rs                              79
        paths.rs                             38
        provider.rs                         196
        sandbox.rs                          284
        rootfs/                            [982]
          mod.rs                              9
          archive.rs                        313
          cache.rs                          121
          layers.rs                         192
          tar.rs                            325
          types.rs                           22
      native/                             [1453]
        mod.rs                               23
        create.rs                           232
        exec.rs                             102
        groups.rs                           114
        listener.rs                          56
        process.rs                           59
        provider.rs                         169
        sandbox.rs                          271
        search.rs                            13
        streams.rs                           18
        linux/                             [133]  (removed if O27 is accepted)
          mod.rs                              3
          mounts.rs                          90
          process.rs                         40
        macos/                             [263]
          mod.rs                              3
          darwin.rs                         238
          mounts.rs                          22
    bin/                                   [434]
      layerfs-sandbox-helper/              [434]  (in the bundle; built without "machine")
        main.rs                              45
        exec.rs                             141
        init.rs                              88
        mounts.rs                           160
```

The new CLI gains `commit`, `machine`, `store` and `backup` and every
family's new argument order. Init straight into PostgreSQL touches the
Persistence crate as listed above: a writable PostgreSQL open, a new
`backend/postgresql/acquisition.rs`, acquisition SQL only if the existing
schema can't hold it, and the import's removal after that.

## 11. Plan

Small commits; moves and behaviour changes never share one. Each reports
Production LOC. The Rust library comes before every behaviour change, so no
new JavaScript is written.

1. **The AGENTS.md amendments** (§9.1). The owner approves the wording; it
   lands before any step that would break today's rules.
2. **The Rust library:** `layerfs-api` grown in place (§13 O21a), with the
   call table. Today's runtime, agent and backend calls move into it family
   by family with no behaviour change, and the Node package becomes its
   napi-rs front end (§7.10). Steps 3 to 15 are then built once, in Rust.
   - The LOC counter (`tools/production_loc.py`) learns `layerfs-cli` and
     `layerfs-python`, and `.py` files if Python sources ship. Today its
     `crate_layer()` files an unknown crate under core: `layerfs-nfs`
     (3,005 LOC) is counted as core because it is missing from
     `ENGINE_CRATES`.
3. `layerfs.json` and the layout version; read-only, no behaviour change.
4. The one layout for storage: `store/<name>/project/<name>/`,
   `backup/<bucket>/` and `record.json` throughout. `stores.json`,
   `remote-state/`, `.stores/`, `<home>/<project>/`, `.runtime/projects/`
   and `remotes` are moved in once. Machine folders come in step 6 and
   Sandbox folders in step 12.
5. The machine adapter shape in `layerfs-sandbox` (§7.10.1). First one
   pure-move commit: `platform/` becomes `backend/` and the Docker client
   moves to `docker_engine/`, with `docker` re-exported at its old path.
   Then `machine/`, behind its feature, wraps `native`, `docker`, the VM
   kinds and `ssh` as today, with no behaviour change. If O27 is accepted,
   the Linux `native` pairing and `backend/native/linux/` go in a commit of
   their own.
6. Machine records and folders, `machine/<name>/`: `list`, `get`, `status`,
   `create`, `remove`, `connect`, `disconnect`, and `update` if O11 is
   accepted. A VM per machine, named after it, holding many Stores under
   `/layerfs/data`, with one worker; `--nested` Lima hosts and Firecracker
   machines on them (§7.1.1). VM and ssh state move under
   `machine/<name>/`.
7. One route for every machine; calls route by records; `useWorker` and
   `WORKER_ENDPOINT` removed. The caller reads `deploy` from the machine's
   record and sends it with each worker call; the `/layerfs/home` share is
   dropped. The home lock and Store leases (§7.8).
8. sqlite Stores: `list`, `get`, `status`, `create`, `remove`, and `move`
   with its phases, including the new guest-to-host send. The five
   `start_*` calls and the `vm:` refusals removed.
9. Projects named `<store>/<project>` everywhere; `LAYERFS_PROJECT` and
   `--project` removed. Init in the caller's process into an existing Store,
   as a live add; the macOS Init container and the VM folder copy removed.
   Init's progress stream replaces its job (if O22 is accepted), and
   `project.remove` is added (if O24 is accepted).
10. postgresql Stores, in three parts:
    - **10a.** `connect`, `disconnect`, `status` and the fixed `layerfs`
      schema; `attach_postgresql` removed.
    - **10b.** Init into PostgreSQL: a writable PostgreSQL open and
      acquisition backing (`backend/postgresql/acquisition.rs`). Tests that
      reach PostgreSQL by import switch to Init.
    - **10c.** Only after 10b: remove `start_import_postgresql`,
      `backend/postgresql/import.rs` (399 LOC), `PostgresHandles::import`
      and the binding's import.
11. The `backup` family: `connect`, `disconnect`, `create`, `restore`, `list`,
    with the exported-from metadata; bucket I/O and snapshots through the
    caller's process. Links per Store per bucket (if O23 is accepted).
12. Named Sandboxes bound to a project, the machine argument, the image rules,
    `sandbox close`, home-wide names; `workspace mount` by Sandbox name.
    Sandbox records move under `machine/<name>/sandbox/<name>/`.
13. Machines and workers started on demand; the worker's idle exit and
    power-off (§7.2).
14. One package, `layerfs`, with the nine families and their argument order
    ([the `layerfs` command and SDK](layerfs-sdk-cli.md)), replacing
    `@layerfs/agent`, `@layerfs/runtime`, `@layerfs/backend` and
    `@layerfs/cli`. The npm package is `api/node/`; the 1.1.0 calls not in
    the table go as §7.9 lists.
15. The `layerfs` binary replaces the Node command; the worker runs as
    `layerfs worker`, so VMs and ssh hosts no longer install Node.js.
16. The Python module (`api/python/`): wheels per target, a synchronous and
    an asyncio API (if O19 is accepted), the same calls and errors.
17. The 41 settings removed, with `layerfs import-env` writing records from
    the current environment once (§13 O4).
18. Docs, the SDK skill, `api/docs/agent.md` and `runtime.md`, and the Python
    reference.
19. **Terminal pass:**
    - one home, one process, the 12 cells of #516 on each machine kind;
    - two Lima machines of different sizes, each with two Stores, used at once;
    - Init on `native`, then moves to `docker` (macOS) and one VM, and back,
      with the Stores compared;
    - backup to and restore from MinIO and R2, and `backup list`;
    - Init into PostgreSQL, then `docker` Sandboxes on it;
    - a Mac with Docker stopped: `native` and the VM kinds still work;
    - machines: none running while the home is idle; one started by the first
      call; an agent creating and removing a machine;
    - the same calls from the binary, Node and Python, with the results and
      errors compared.

    The macOS host covers `native`, `docker`, `apple-container`,
    `microsandbox`, `lima`, `firecracker` and `ssh`; a Linux host covers
    `native` and `docker`.

## 12. Owner directions (2026-11-02)

Given while drafting; they shape §7 and the CLI and are not reopened here.

- `LayerFS.init(project, folder, options)` keeps its arguments, as
  `project.init`; Init always comes from a host folder.
- `native` means the host's own operating system (Linux, macOS or Windows),
  not `LAYERFS_HOME`.
- Storage is separate from compute.
- sqlite and postgresql are separate kinds of Store; s3 belongs to sqlite. No
  import between the kinds.
- Init names an existing sqlite or postgresql Store; it never creates one.
- A project is named with its Store, `<store>/<project>`, so that two Stores'
  projects never collide.
- `LAYERFS_PROJECT` is removed; a call that needs a project names it.
- `store create` makes a sqlite Store on `native`; `store connect` attaches a
  postgresql database. A postgresql connection takes no schema, because
  LayerFS's format fixes it. A home may hold many postgresql connections.
- Backups have their own family, `backup`, apart from `store`: one is a live
  Store, the other a copy in a bucket.
- No `start_transfer` and no place registry: a sqlite Store moves only between
  `native` and another machine.
- `remove` takes no confirmation.
- Machines: `docker` and `native` are one each; VM kinds may have many named
  machines on one host. Agents may create and remove machines.
- Machines and workers start on demand and stop when idle.
- Sandboxes are named, not numbered, and bound to one project at creation;
  after that no call needs the project. The machine is not bound to a project;
  `sandbox create` must name one.
- `store list` shows each sqlite Store's backup state; the s3 side shows
  whether each file was pulled into this home and which machine it was
  exported from.
- The CLI is designed first; the SDK follows it, with the same families and
  calls in code and in the shell.
- One package, with no split between agents, operators and the command.
- Families: `project`, `branch`, `commit`, `fs`, `sandbox`, `workspace`,
  `machine`, `store`, `backup`. Sandboxes are created and closed explicitly;
  `workspace` has `mount`, `exec`, `commit` and `unmount`.
- `branch.fork` keeps its name; Branches are forked, not created.
- Machines and Sandbox backends are two layers of `layerfs-sandbox`:
  `machine/` (which kernel; host only, behind a feature) and `backend/`
  (renamed from `platform/`, how a Sandbox runs on it). Each machine kind
  names one backend; "platform" means only a machine's kind (§7.10.1).
- Calls take location positionally (project, Branch, path, then an optional
  Commit) and options for how.
- The home has three parts, `machine/`, `store/` and `backup/`. Sandboxes
  nest under their machine and projects under their Store; there is no
  `platform/` folder, and Firecracker's host is a `lima` machine.
- One Rust implementation; the `layerfs` binary, Node and Python are thin
  front ends over it (§7.10).
- `LayerFS.init` and `LayerFS.open` are removed, with no alias. Init keeps
  their arguments as `project.init(project, folder, options)`; every other
  call takes its project.

## 13. Open decisions

- **O1. Secret references.** Recommended: `env:LAYERFS_…` and `file:…`, as
  today. Otherwise: add `keychain:` (macOS Keychain, Linux Secret Service,
  Windows Credential Manager), which is new work.
- **O2. Machine idle time.** Recommended: about 10 minutes, so an agent's
  pauses between calls don't pay a VM boot.
- **O3. PostgreSQL Sandboxes beyond `docker`.** Recommended: `native` in this
  proposal (a build with the `postgresql` feature and a test); VMs and ssh
  later, since they need a network path and the password inside the machine.
- **O4. Old settings.** Recommended: removed in 2.0, with `layerfs
  import-env` writing records from the current environment once. Otherwise:
  read for one release when the home has no `layerfs.json`.
- **O5. Windows.** Recommended: the Windows home holds the records, and
  `native` on Windows holds only records. Its sqlite Stores live in the WSL
  home (ext4) and Init runs in the WSL worker, on the same kernel as
  `docker`; Sandboxes run on `docker`. `discovery.rs` uses `flock` and
  `std::os::unix`, so the Windows front end needs another lock (work).
  Otherwise: `linux` Sandboxes inside WSL, which shares that kernel (not
  tried).
- **O6. Existing PostgreSQL Stores in other schemas.** #506 lets a schema be
  named. Recommended: `connect` opens a database whose single LayerFS schema
  has another name, and refuses one with several. Otherwise: a one-time rename
  to `layerfs`.
- **O7. `native` and `docker` Stores together on macOS.** Required by §7.5 on
  macOS, since every Init there needs a `native` Store. Recommended: allowed,
  each Store on one of them. This needs the D5 macOS native Store exception
  that #527 recorded. The alternative is to keep today's Init container for
  `docker` on macOS.
- **O8. A Linux `docker` Sandbox on a `native` Store.** Later: on Linux,
  containers could open a `native` Store directly (same kernel, local disk),
  with no move. Not tried.
- **O9. The old Store calls.** Recommended: removed in 2.0. Otherwise: kept
  one release as wrappers.
- **O10. `native` or `local`** as the built-in machine's name.
- **O11. `machine update`.** Recommended: `machine update <name>` with new
  `cpus` and `memory`, refused while Sandboxes run on it, restarting the VM
  where the provider can't resize live; operators only at first. It also
  sets the built-in machines' settings: `machine update native` and
  `machine update docker` set the image, the Docker socket and the WSL
  distribution (§7.1). It would be a call beyond the 54.
- **O12. A move between two non-native machines.** Recommended: through
  `native` in one call. Otherwise: refused, asking for two moves.
- **O13. `fs` and Node's `fs`.** Recommended: rename the family `file`, since
  `import { fs } from "layerfs"` clashes with `node:fs` in most files.
- **O14. `workspace.status`.** Left out to keep `workspace` at four calls.
  Recommended: add it back; agents use it today to see a Workspace's changes
  before a commit.
- **O15. Package names.** `layerfs` was not checked on npm, PyPI or
  crates.io; `@layerfs/layerfs` is the npm fallback.
- **O16. Where the sealed Linux bundle comes from.** Every VM, ssh host and
  `docker` on macOS needs it (daemon, worker, helper; the VM's operating
  system); today it is the runtime's `bundle` setting, which leaves with the
  other settings. The built-in machines' binaries come from the same place.
  Recommended: fetched once per build into the user's cache, outside every
  home, checked by its hash, with the build pinned by the `layerfs` version.
  Otherwise: shipped inside the package, or a `bundle` option on
  `machine create`.
- **O17. Firecracker's files.** Recommended: `machine create --nested` puts
  them in the Lima host. The guest kernel and base root filesystem are added
  to the bundle (new; the bundle holds neither today). The Firecracker
  release binary is checked against its hash, and a guest ssh key is
  generated for that host. Otherwise: placed by hand as today, with
  `machine list` naming what is missing.
- **O18. ssh Store filesystems.** Today's ssh adapter accepts only ext4.
  Recommended: ext4, xfs and btrfs, the set AGENTS.md allows for a Linux
  Store.
- **O19. Python's API style.** Recommended: `import layerfs` is
  synchronous, and `layerfs.aio` has the same calls as coroutines. Otherwise:
  asyncio only, matching Node's promises.
- **O20. Targets for prebuilt packages.** Recommended: Linux x64 and arm64
  (glibc), macOS arm64, and the Windows front end that talks to its WSL
  worker. Otherwise: add Linux musl and macOS x64 when asked.
- **O21a. The library crate.** Recommended: grow `layerfs-api`, which is
  already the Rust method table, instead of adding a new `layerfs-sdk`
  crate. The docs call it "the library crate".
- **O21b. Where the caller-side crates live.** Recommended: keep them in
  `engine/crates/` for 2.0, with `layerfs-cli` and `layerfs-python` beside
  `layerfs-node`. `tools/production_loc.py` scans only `core/crates` and
  `engine/crates`, and its `crate_layer()` files an unknown crate under core.
  Otherwise: move `layerfs-api`, `layerfs-node`, `layerfs-cli` and
  `layerfs-python` to `api/crates/` in one pure-move commit, reversing the
  #428 regrouping; that needs a counter and layer-guard change, and stays
  possible later. `layerfs-sandbox` stays in `engine/` either way, since
  its backends and helper also run inside guests.
- **O22. Progress for long calls.** Recommended: one stream in the call
  table, used by `workspace.exec` for output and by `store.move`,
  `backup.create`, `backup.restore` and `project.init` for progress. Node
  takes `{ on_progress }`, Python `on_progress=`, and the shell
  `--progress`, which writes one JSON line per update to stderr; a refusal,
  if any, is the last stderr line, and stdout stays the single result.
  Another process can't watch a call it did not start. There is no job
  registry: `project/jobs.js`, the
  runtime's sync jobs, `runtime.sync.get` and `get_init` go, and
  layerfs-node's `progress.rs` becomes the library's. Otherwise: keep jobs
  that another process can read.
- **O23. Backup links.** Recommended: one link per Store per bucket, with
  `backup create` and `backup restore` taking an optional `file` (§7.3).
  This reverses #509's one link per Store. Otherwise: keep one link per
  Store.
- **O24. Removing one project.** Recommended: add `project.remove(project)`,
  a call beyond the 54. Otherwise: a project goes only with its Store, by
  `store.remove`.
- **O25. Operator and diagnostic calls.** The 14 `@layerfs/backend` calls
  listed in §7.9. Recommended: drop them from 2.0's surface, and keep what
  Connect needs behind the trace files. Otherwise: keep them as an
  `inspect` family outside the 54.
- **O26. The trace feed (`events.subscribe`).** Recommended: the trace files
  under `trace/` are the feed, with no call. Otherwise: keep a subscribe
  call.
- **O27. The Linux `native` backend.** Recommended: remove the Linux
  `native` pairing and `backend/native/linux/` (§7.10.1). Otherwise: keep
  it beside `namespaces` for `native` on Linux.
- **O28. An unset `LAYERFS_HOME`.** Recommended: the home is `~/.layerfs`.
  Otherwise: a call without `LAYERFS_HOME` is refused.
- **O29. `fs.diff`'s arguments.** Recommended: positional, matching the
  shell: `fs.diff(project, left_branch, left_path, right_branch, right_path,
  { left_commit, right_commit })`. Otherwise: two `{ branch, path, commit }`
  objects.
- **O30. Commit numbers.** Recommended: Commits stay ids. Otherwise:
  per-Branch numbers (Commit 12 of `fix`), which need a new index.
- **O31. Forcing a Sandbox closed.** Today Connect falls back to
  `docker stop` when a daemon does not answer. Recommended:
  `sandbox.close(name, { force })` stops the Sandbox through its backend
  and records `Unknown` for Workspaces it could not drain. Otherwise: no
  force, and a stuck Sandbox waits for `machine remove`.
- **O32. MCP and folder scope (#521).** #521 adds an MCP server
  (`@layerfs/mcp`) and folder-scoped clients (P13). Recommended: port folder
  scope into the library as a client option of every front end; MCP becomes
  a fourth thin front end over the call table, or is dropped from 2.0.
  Otherwise: keep both in JavaScript beside the package.
- **O33. Container names per home.** Two homes on one Docker engine must not
  see each other's containers (Connect used an `lfc-` prefix to stay apart
  from dsh). Recommended: the `docker` machine names containers with the
  home id from `layerfs.json`. Otherwise: a prefix in the `docker` machine's
  record (O11).
- **O34. Machine facts.** Recommended: `machine.status` reports state, free
  disk, the provider tool's version and why a machine is down, and has no
  change event; callers poll it. Otherwise: add a watch call.
- **O35. A lease left by a crashed process.** Recommended: a lease
  `store/<name>/open/<pid>` whose process is not alive on this host is
  stale and removed under the home lock, and the removal is logged.
  Otherwise: `store move` and `store remove` refuse `InUse` until a person
  removes it.

## 14. Not verified

- The VM kinds on a Linux host; the #529 pass ran on macOS only, and Lima and
  Firecracker are macOS only today (§7.1).
- A VM holding several Stores on one data disk at `/layerfs/data`, and one
  worker serving them all; today each VM holds one project's Store.
- Workers that get every setting with each call and never read the home
  (§7.7): not built. Today three VM providers mount the home in the guest.
- The home lock and Store leases (§7.8): not built.
- A snapshot sent from a guest to the host, and s3 I/O in the caller's
  process (§7.3): not built.
- That a Store made on macOS and moved to Linux opens with the Linux build;
  the manifest records the SQLite version it was written with.
- That Init on macOS's own disk records the same owners, modes and name case
  as Init through Docker Desktop's file sharing.
- Exactly how a PostgreSQL URL reaches a `docker` daemon today; it was traced
  only as far as the SDK process's environment (`LAYERFS_STORE_POSTGRESQL`).
- Init straight into PostgreSQL: not built; its cost against a sqlite Init is
  unknown.
- Machine boot and worker start times, and the worker's idle exit (§7.2).
- `--nested`: today the Firecracker Lima host is made by hand; LayerFS
  making it was not built or run.
- A Sandbox found by name across `machine/*/sandbox/` (§7.8): not built.
- §7.10: none of it is built. The 4,879 lines are the JavaScript to port,
  not a measured effort; the per-call cost of the Rust route and the
  binary's start time are not measured.
- That the `machine` feature stays out of every guest build (§7.10).
- That every adapter's behaviour ports to Rust without a change a caller
  would see; step 2 of §11 has to show it.
- Init, move, backup and restore speed (§8).
- Windows: nothing here was run on Windows.

## 15. Related

- [#533](https://github.com/Ephemeral-AI-Lab/layerfs/issues/533): the implementation spec and work order,
  [core/docs/issues/533/README.md](../issues/533/README.md).
- [The `layerfs` command](layerfs-sdk-cli.md): every command, its options,
  refusals and output.
- #527 (PR #528): placement and backend; D1–D7.
- #529 (PR #530): `sandbox.create({ platform })`; P1–P6; `PLATFORM-PASS.md`.
- #506, #509: Stores, remotes, PostgreSQL Stores, the home's `remotes` and
  `stores.json`.
- [AGENTS.md](../../../AGENTS.md): direct Store access, the worker exceptions
  and plugin-neutral LayerFS; §9.1 lists the changes this proposal needs.
- layerfs-connect `host/README.md`: the host settings and the eight-home
  platform pass this proposal would replace.
