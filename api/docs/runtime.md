# Using `@layerfs/runtime`

> **Status:** Current implementation guide for `@layerfs/runtime` in LayerFS
> 1.1.0; not a release receipt. Part of the
> [JavaScript packages wiki](../README.md).

`@layerfs/runtime` runs LayerFS on a machine for an integration: it initializes
and removes projects, starts the worker on macOS and Windows, and finds and
cleans up Sandboxes and volumes. It works on a LayerFS home, not a project.
Agents don't import it; a host that runs projects for its users does.

## Calls

```js
import { LayerFSRuntime } from "@layerfs/runtime";
const runtime = await LayerFSRuntime.open({ home, image, daemon, bundle, docker, containerPrefix });
try {
  await runtime.status();                  // { platform, placement, missing, docker, worker, vm, disk, sweep }
  const job = await runtime.project.start_init("shop", "/src/shop", { timeout }); // timeout optional: LayerFS.init's 300 s default
  await runtime.project.get_init(job.id);  // { step, scanned, totals, imported, project, error, ended }
  await runtime.project.list();            // [{ name, volume, image, source, created, initialized }]
  await runtime.project.get_state("shop", since);
  await runtime.sandbox.reclaim("shop");   // { reclaimed: [ids] }
  await runtime.sandbox.list_leftovers();
  await runtime.sandbox.clean_leftovers(["layerfs-old-1"]);
  await runtime.sandbox.list();            // [{ name, project, sandbox, state, running, startedAt, finishedAt, image, ports, scratch, owned }]
  await runtime.sandbox.sweep();           // { reclaimed: { project: [ids] }, failed: [{ project, code, message }] }
  await runtime.volume.list();             // [{ name, size, containers, owner, project, live, removable }]
  await runtime.volume.remove(["layerfs-shop-store-k2"]); // { results: [{ name, ok, message? }] }
  for await (const event of runtime.events.subscribe({ project: "shop" })) { /* a trace line + actor */ }
  await runtime.project.delete("shop", { confirm: "shop", stopSandboxes: false });
} finally {
  await runtime.close();
}
```

A `vm:<provider>` home has one call group more (`vm`), names a project in
`worker.start`/`worker.restart` (one VM per Store, see
[Placement](#placement-one-linux-vm-per-store)), and reports each project's VM:

```js
  await runtime.worker.start("shop");       // this project's VM: create/start, bundle, worker, endpoint
  await runtime.worker.restart("shop", { confirm: "restart" });
  await runtime.vm.status();               // { provider, placement, vms: [{ project, name, state, worker, disk }] }
  await runtime.vm.status("shop");         // that project's VM record
  await runtime.vm.stop("shop");           // stops it; its Store disk and its Store stay
  await runtime.vm.delete("shop", { confirm: "delete", discardStore: false }); // the VM record only, unless discardStore
```

## Opening and settings

The runtime opens on a LayerFS home, not a project, so it works before the
first project exists, after one is removed and while the worker is down.
Agents don't import it. Settings (`settings.js`): `home` (required), `image`,
`daemon` (default the bundle's), `bundle`, `docker` (default `/var/run/docker.sock`;
on macOS Docker Desktop's `~/.docker/run/docker.sock` when only that one exists,
checked once at `open`), `volumes` (Linux),
`traceDir` (default `<home>/traces`), `containerPrefix`, `stateDir` (default
`<home>/.runtime`, the runtime's project records), `sweep`
(default true), `placement` (default `local`; `vm:<provider>` places the home
in one Linux VM, below) and on Windows `wsl: { distribution, user, node, home }`. A
missing setting does not refuse `open`: `status().missing` lists it.

The runtime names each project's Sandbox backend itself (#527 F4): a `local`
home's clients get `LAYERFS_SANDBOX=docker` with `LAYERFS_SANDBOX_BACKEND`,
`LAYERFS_LOCAL_DIR` and `LAYERFS_IMAGE_ARCHIVE` unset, and a `vm:` home's get
native Sandboxes on the namespaces backend. Values of those settings in the
runtime's own environment do not reach its clients.

## Init

`start_init(name, folder, { image, timeout })` refuses a bad name, folder,
image or timeout and a taken name before anything changes; then it counts the
source, makes the volume and runs one Init. `timeout` (milliseconds, a
positive whole number) is passed unchanged to `LayerFS.init(name, folder,
timeout)` and bounds the Init itself, not the count or volume steps; the
runtime has no default of its own, so an omitted one gets `LayerFS.init`'s
300 s. A Deadline is a failed Init like any other. A failed Init removes the
volume and the home entry it made; an uncertain one (`Unknown`, or any failure
on Windows) keeps everything and is recorded in
`<stateDir>/init-failures.jsonl`.

`runtime.project.get_init` (native; `@layerfs/runtime`) reads the project's Init while `project.init` runs in the
same process, without a Sandbox: `{ step, entries, files, bytes }`, or null when
no Init runs. `step` is `scan` (reading the source), `files` (constructing each
regular file), `tree` (building the namespace), `publish` (genesis Layer) or
`seal` (first Branch and sealing). `entries` counts source entries scanned,
including the root; `files` and `bytes` count regular-file identities
constructed and their bytes, a hardlinked file once. The counts say how far
Init has come, not its totals; a caller that wants a fraction counts the source
itself. Init then installs the Store into the volume in the same process.

## Removing a project

`remove` needs `confirm` equal to the project's name and refuses `Busy` while
a running container uses the volume, unless `stopSandboxes`.

## Events

`events.subscribe` reads the trace folder's `*.jsonl` files every `pollMs`:
lines from now on, or from `since`, each with `actor`, its trace file's name.

## Disk the home holds in Docker (#423)

- `sandbox.list` lists this home's Sandbox containers, running or not, with
  `scratch`, the bytes of the container's writable layer (what commands wrote
  outside every Workspace), its ports and whether a project of this home owns
  it. One `docker ps` and one `docker inspect --size`; it changes nothing.
- `sandbox.sweep` runs `sandbox.reclaim` for every project of the home that
  has an exited or dead Sandbox container, found by the container's
  `layerfs.project` and `layerfs.store` labels rather than its name (the
  binding owns the name format). The project's records and lock
  decide what goes, so a container of another home, or one starting now, is
  never touched; containers no project records stay for `list_leftovers`.
  `open` runs it once in the background unless `sweep: false`;
  `status().sweep` is `null`, `{ state: "running" }` or
  `{ state: "done" | "failed", at, … }`, and `close` waits for it.
- Init labels a project's volume `layerfs.role=store`, `layerfs.home=<home>`
  and `layerfs.project=<name>` (Docker sets labels only at create).
  `volume.list` lists every volume on the engine with Docker's size, the
  containers that use it and its `owner`: `"this"` (labelled with this home
  or named by a project record), `"other"` (another home's label) or
  `"unknown"` (no label and no record: made before labels or by another
  tool). `live` is true while this home has the owning project; an uncertain
  Init, which keeps a home entry and no record, counts as live. Docker's own
  "reclaimable" figure counts every idle project's Store and is not used.
- `volume.remove(names)` removes only volumes that `volume.list` marks
  `removable`: this home's, no longer a project's, and used by no container.
  Anything else is refused by name; one `docker volume rm` each, no retry.

## Platforms and the worker

`src/platform/` holds one folder per placement kind (#527 §6, D4):
`platform/docker/` for `local` (`linux.js`, `macos.js` and `windows/`: Docker
on this host, in Docker Desktop's VM or in WSL2) and `platform/vm/` for
`vm:<provider>`. Each platform object implements one interface (`endpoint`,
`status`, `start`, `restart`, `init`, `path`, the Store hooks `createStore`,
`storeHolders`, `releaseStoreHolders`, `releaseStore` and `removeStore`, and
the home and disk helpers) and may declare capabilities instead of being named:
`initCounts: false` (macOS: this process cannot read Init's counts) and
`uncertainInit: true` (Windows: any failed Init is kept and recorded, never
cleaned up); `platform/vm/` implements the same interface over a VM
(see [Placement](#placement-one-linux-vm-per-store)).

The three Docker platforms also carry `platform.docker`, one Docker group
built in `platform/docker/index.js` from that platform's own CLI call: `run`
(the host socket on Linux and macOS, `wsl.exe` on Windows), `version`,
`sandboxes` (this home's Sandbox containers, `sandboxes.js`), `stores` (Store
volumes and their hooks, `stores.js`) and `disk` (the engine's volumes,
`disk.js`). A `vm:` platform has no group, and the helpers that read host
Docker refuse there. Every helper reaches Docker through
`runtime.platform.docker.run`, so a test replaces that one call.

The Store hooks are keyed by Store, `{ name, volume, legacy }` (#527 step
10b): `legacy` marks the Store a project's own Init makes, named after the
project; any other is a Store of the home. On a Docker placement
`createStore` makes the Store's labelled volume (the project's own volume,
labelled `layerfs.project`, or `<prefix>store-<name>`, labelled
`layerfs.store`) and refuses `Exists` for a volume that exists instead of
reusing it; Init, `store.create` and `store.start_load` all make their
volume through it.

- **Linux** runs Store calls in this process. There is no worker:
  `worker.start` and `worker.restart` are refused `InvalidInput`. Init
  installs the Store under Docker's volumes directory, so the process must be
  able to write it (`init` checks first).
- **macOS** runs the worker as one container per home, `<prefix>worker`
  (label `layerfs.role=worker`), with the bundle's `worker/server.js`; Init
  runs in a container of its own with the source folder mounted read-only.
  `worker.start()` takes no options and starts or reuses `<prefix>worker`
  only; it stops or removes no other container. `worker.restart({ confirm:
  "restart" })` restarts the worker, which cuts off every project's Store
  calls meanwhile. The worker's secret reaches `docker
  run` through its environment, never its command line.
- **Windows** runs the worker as a Node process in a WSL2 distribution
  (`server.js --stdin`); Init runs in the worker, which maps the folder with
  `wslpath`. This process owns the worker through its stdin: `close()` drains
  and ends it.

The runtime starts a worker only on macOS and Windows, and in a
`vm:<provider>` home. The worker route is open to any integration (owner,
2026-10-24): on macOS and Windows the runtime hands its endpoint to
`@layerfs/agent` (`useWorker(endpoint)`, without changing the environment),
and on `vm:` it hands over a resolver (`useWorker((project) => endpoint)`),
so every client made after `open` in its process uses the worker; a client
made before keeps its in-process binding. One worker route per process: a
second runtime of another home is refused. Another process, such as an
agent's command, uses the worker when its `LAYERFS_WORKER_ENDPOINT` names
the endpoint file; the integration that runs it sets that (on a `vm:` home
`deploymentEnv` sets it to that project's VM). Such a process also needs
the project's settings: make the project with `project.start_init`, which
creates and records its volume, then set `LAYERFS_VOLUME` to the volume
`project.list` gives,
`LAYERFS_STORE_PATH` to `/var/lib/docker/volumes/<volume>/_data/global/store.sqlite`
(or on `vm:` `/layerfs/store/global/store.sqlite`), and `LAYERFS_IMAGE` and
`LAYERFS_DAEMON` for its Sandboxes (on macOS, the bundle's `layerfs-daemon`).
A volume made outside the runtime works for Store calls, but `project.list`
shows the project with `volume: null` and `volume.list` marks the volume
`unknown`. On macOS and Windows the endpoint is
`<home>/worker/endpoint.json`, on `vm:` `<home>/worker/<project>.json`, `{ home,
port, secret, build, daemonHost?, docker?, init?, vm? }`, readable by its user
only. On macOS `docker` is the engine's socket inside its VM,
`/var/run/docker.sock`, which the worker and Init's container mount; it
replaces every call's `deploy.docker`, so the Mac's own socket
(`LAYERFS_DOCKER`) never reaches Linux. The worker checks every file of the
bundle's `manifest.json` before it listens.

## Placement: one Linux VM per Store

`placement` (also read from `LAYERFS_PLACEMENT` when the setting is absent) is
`local` (the default: this machine's Docker, macOS worker container or WSL2
worker, unchanged) or `vm:<provider>`: **each of this home's projects — its
Stores — lives in one Linux VM of its own** on this machine (#515 §6, V9, V1).
A project's VM holds that project's Store on its own ext4 disk, its Sandboxes
are Linux namespaces (`LAYERFS_SANDBOX_BACKEND=namespaces`, H36's backend) and
its worker runs the real SDK as root; the host reaches only each VM's worker,
with one attempt per call and no replay. Providers, verified with stand-in
provider CLIs (`test/vm_placement.mjs`; real runs of `test/verify/vm_real.mjs`:
see the #527 platform pass): `microsandbox`
(`msb` 0.7.8), `apple-container` (`container` 1.6.1) and `ssh` (any Linux host
the user provides, reached with the system `ssh` client; see
[the SSH provider](#the-ssh-provider-any-linux-host) below).
Each provider runs on its own host systems, checked when the settings resolve
and refused `Unsupported` elsewhere: `apple-container` on macOS only,
`microsandbox` on Linux (KVM) and macOS (Apple silicon), `ssh` on Linux and
macOS. No provider runs on a Windows host, since the placement maps this
host's folders into the guest as POSIX paths (#527 step 8).

```sh
LAYERFS_PLACEMENT=vm:microsandbox layerfs-home # any integration that sets it
```

Every per-VM thing is that project's alone: the VM `<prefix><project>-vm`, its
Store disk `<prefix><project>-vm-store`, its host port, its bundle copy and its
worker secret. Nothing is shared between two projects' VMs except the read-only
host home bind mount (§ Guest layout).

Settings: `vm: { image, cpus, memory, storeSize }` (`LAYERFS_VM_IMAGE`,
`LAYERFS_VM_CPUS`, `LAYERFS_VM_MEMORY`, `LAYERFS_VM_STORE_SIZE`; defaults
`node:22-trixie-slim`, 2, `2G`, `20G`). The image is the VM image: it must hold
`node`, and it is not the Sandbox image. A VM home needs `bundle` and no
`image`; its `daemon` setting is the guest's, `<bundle>/layerfs-daemon`, with
`layerfs-sandbox-helper` beside it. `storeSize` is the disk's size where the
provider has one: Microsandbox takes it when it makes the volume, Apple
Container at `volume create -s` (the CLI's own default is a sparse 504 GiB
image), and an existing volume keeps the size it was made with.

Guest layout (`settings.js`'s `GUEST`), the same paths in every project's VM:
the home is a bind mount of the host's `<home>`, so project entries and traces
are the same files on both sides; **the Store disk is mounted at the Store
directory**, so the Store is `/layerfs/store/global/store.sqlite`; the sealed
bundle is copied in at `/layerfs/bundle`; an Init source folder is copied to
`/layerfs/import/<hash>`; the worker's settings, pid and log live in
`/layerfs/state`; **Sandbox directories are on the Store disk, at
`/layerfs/store/local`**, not on the VM's root: a namespace Sandbox's root is
an `overlay` whose writable layer must sit on a local filesystem, and a
provider's own root can be an overlay itself (Microsandbox's is), while the
Store disk is an ext4 disk on both providers. The home mount is the one thing
two VMs share, and it holds records, not Stores: what a VM cannot see is
another project's Store, which is on another disk in another VM.

A bundle may also carry the Sandbox image as `<bundle>/images/<64 hex>.tar`
(#515 §4.7 V7, the namespaces backend's convention): it comes in with the rest
of the bundle — its own copy in each VM — and the worker is started with
`LAYERFS_IMAGE=sha256:<64 hex>` and `LAYERFS_IMAGE_ARCHIVE=/layerfs/bundle/images/<64 hex>.tar`,
while `deploymentEnv` gives clients that digest and that archive path as
`LAYERFS_IMAGE` and `LAYERFS_IMAGE_ARCHIVE`. The namespaces backend requires an
image (`capabilities.requires_image`), so a bundle without one starts no
Sandbox: the request is refused, naming the missing setting.

The adapter's five steps, each attempted once, all for one named project:

1. the project's VM exists and runs, from `settings.vm`, with its named disk
   volume and the home bind mount — `create` once, else `start`; a record of
   another home, or one whose mounts or port differ, is refused rather than
   reused or repaired;
2. the sealed bundle is copied into that VM once per bundle hash (the hash of
   its `manifest.json`), with a marker at `/layerfs/bundle/.sealed`. Apple
   Container's `container copy` moves about 35 MB/s (a 2.7 GB bundle with its
   Sandbox image took 77–90 s, #527), so that adapter stages the folder in a
   private `<home>/.vm-copy-*` folder (an APFS clone where the volume allows),
   the guest copies it from its home bind mount onto its own disk with
   `cp -Rp`, and the staging folder is removed whatever the outcome (4.8 s for
   the same bundle). Init sources take the same route; a folder already inside
   the home is read in place;
3. `user.max_user_namespaces=0` is written and read back (V4; Apple Container's
   guest mounts `/proc/sys` read-only, so its adapter remounts it first), then
   the worker starts as root with its secret in a 0600 guest file — never in a
   command line — and the pid it wrote;
4. the endpoint is written to `<home>/worker/<project>.json` once the worker
   answers on `127.0.0.1:<host>`, the VM's `-p 127.0.0.1:<host>:7070`
   (V5: the Sandboxes share the VM's network, so a daemon's loopback port
   stays inside it);
5. `runtime.vm.stop(project)` stops that VM and keeps its disk; `runtime.vm.delete(
   project, { confirm: "delete" })` removes the VM record and keeps the Store
   disk unless `discardStore`. **A delete is refused while that project
   exists**: its Store lies on that disk and nowhere else, and this build has
   no remote Store push (#506), so V8's "never delete a Store that was not
   pushed" makes the refusal the only safe answer. `project.delete` is the
   explicit removal: it stops the project's Sandboxes, closes its Store, then
   deletes the VM and its disk together.

Init runs as one more call to that project's worker: `platform.init` starts its
VM and copies the source folder into `/layerfs/import`, and `LayerFS.init`
reads it there. `deploymentEnv` gives clients and agents the VM's settings:
`LAYERFS_SANDBOX=native`, `LAYERFS_SANDBOX_BACKEND=namespaces` (always: a
`LAYERFS_SANDBOX_BACKEND` in the caller's environment does not replace it), `LAYERFS_DAEMON`,
`LAYERFS_LOCAL_DIR`, `LAYERFS_HOME=/layerfs/home`, the Store's
`LAYERFS_STORE_PATH`, this project's `LAYERFS_WORKER_ENDPOINT`, and the bundle
image's `LAYERFS_IMAGE` when it has one — no `LAYERFS_DOCKER` or `_VOLUME`.

A VM home's `status()` reports `placement`, `docker: { level: "not-used" }`
(host Docker is not in its data path), `worker: null` (no single worker serves
the home) and `vm: { provider, placement, vms: [{ project, name, state, image,
home, store, created, worker, disk }] }`, one entry per project of
`project.list()`. The Docker-shaped helpers read host Docker and refuse
`Unsupported` with the reason: `volume.list`, `volume.remove`, `sandbox.list`,
`sandbox.list_leftovers` and `sandbox.clean_leftovers`; `sandbox.sweep` answers
`{ reclaimed: {}, failed: [], host: false }`. The SDK's
`project.list_sandboxes` reads the VM's Sandboxes.

A VM home keeps no Store of its own: each project's Store is in that project's
VM, so a call on the home's Stores or remotes has no VM to run in (#527 F3).
`store.create`, `store.remove`, `store.attach_postgresql`, every
`store.start_*` job, `store.unlink`, `store.check`, every `remote.*` call,
`postgresql.status` and `project.start_init` with `{ store }` refuse
`Unsupported` before any effect, and so does a client of a project in a
PostgreSQL Store. The VM's Store hooks hold only a project's own Store and
refuse any other the same way. `store.list` still lists the home's records, with `bytes`,
`open` and `heads` null. A project is made with `project.start_init(name,
folder)` and removed with `project.delete`.

The real proof is `node api/runtime/test/verify/vm_real.mjs <sealed Linux
bundle> [microsandbox|apple-container]`: two projects, each in its own VM — Init
through each VM's worker, a fork, a write, a Sandbox, an exec as the nonroot
identity with user namespaces blocked, a Commit, a stop/start of one VM and a
worker restart, then a check that neither Store holds the other's Branch, and
that removing one project leaves the other's VM running. The unit tests
(`node api/runtime/test/vm_placement.mjs settings|flow|two|bundle|delete|hosts|stores|apple-container|ssh|cleanup`)
use a stand-in provider CLI.

### The SSH provider (any Linux host)

`vm:ssh` places the home on a Linux host the user already has — an EC2
instance, a machine on the network, or a VM they made — instead of one this
runtime starts (#515 §6). The host and its disk are the user's: the adapter
makes no disk and formats nothing, it proves that the path given is an ext4
filesystem on that host's own kernel (`uname -s` is `Linux`, `findmnt` or
`blkid` says `ext4`) and attaches it at `/layerfs/store` (a bind mount for a
directory, `mount -t ext4` for a block device) when it is somewhere else.

```sh
LAYERFS_PLACEMENT=vm:ssh LAYERFS_SSH_HOST=ec2-…amazonaws.com \
LAYERFS_SSH_IDENTITY=~/.ssh/deploy LAYERFS_SSH_STORE=/mnt/layerfs \
  layerfs-home
```

Settings (each also read from the environment named):

| Setting | Environment | Default |
| --- | --- | --- |
| `vm.host` | `LAYERFS_SSH_HOST` | required |
| `vm.user` | `LAYERFS_SSH_USER` | `root` |
| `vm.port` | `LAYERFS_SSH_PORT` | `22` |
| `vm.identityFile` | `LAYERFS_SSH_IDENTITY` | none: the agent's keys |
| `vm.storePath` | `LAYERFS_SSH_STORE` | `/layerfs/store` |
| `vm.strictHostKeyChecking` | — | `yes` |
| `vm.knownHostsFile` | `LAYERFS_SSH_KNOWN_HOSTS` | the agent's |

Key login only: a password is refused by name, the key is a path that goes to
`ssh -i` (its bytes never enter an argument list or a log), and
`StrictHostKeyChecking` is `yes` unless the setting says otherwise — the
loosest it accepts is `accept-new`.

What differs from a microVM provider, and why:

- **One host serves one Store.** The Store path is one directory on the user's
disk, so a second project naming the same host and path is refused `Busy`,
naming the VM that holds it, instead of writing one Store file twice.
- **The private stream is `ssh -L 127.0.0.1:<host>:127.0.0.1:7070`**, started
with `-o ExitOnForwardFailure=yes`: a busy local port fails at once, before
anything is uploaded or started. Its stderr goes to
`<state>/<vm>-forward.log`, not a pipe, so the host process (CLI, test, an
integration) exits while the user's VM keeps running. `stop` ends the forward
and the remote worker (SIGTERM, then SIGKILL, bounded).
- **Init sources are copied, not shared.** A microVM bind-mounts the host home;
an SSH host shares no path with this machine, so `platform.init` copies the
source to `/layerfs/import/<hash>`. The host's `/layerfs/home` is therefore its
own directory, not this machine's home.
- **A delete never touches the disk.** `runtime.vm.delete` removes the session
record, the forward, `/layerfs/bundle` and `/layerfs/state`, and keeps the
Store; the refusal while the project exists is the same V8 rule as above. The
host's `/layerfs/home` and `/layerfs/import` also stay (they are LayerFS's own
copies, but removing the home entry would let a second Init duplicate a project
that the live Store already holds).

Its session records live in `LAYERFS_SSH_STATE_DIR` (default
`~/.layerfs/ssh`), one JSON file per VM, and a forward that no longer runs reads
as a stopped VM, so the next `worker.start` re-opens it. The failure matrix is
`node api/runtime/test/verify/ssh_faults.mjs [port|store|ext4|host|drop|disconnect|remote|delete|shared]`:
a busy local port, a missing or non-ext4 Store path, a non-Linux host, an
unreachable network and a reconnected one, a dropped forward, a rebooted host,
a refused and a permitted delete, and a second project on the same host.

## Docker

The runtime calls the `docker` CLI, as the dsh Host did. It has no command
line of its own: the `lfs` command it carried is retired (owner 2026-10-23).
Agents use the `layerfs` command instead, described in [the agent guide](agent.md#the-layerfs-command).
