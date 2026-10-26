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
  await runtime.status();                  // { platform, missing, docker, worker, disk, sweep }
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
  await runtime.project.remove("shop", { confirm: "shop", stopSandboxes: false });
} finally {
  await runtime.close();
}
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
(default true) and on Windows `wsl: { distribution, user, node, home }`. A
missing setting does not refuse `open`: `status().missing` lists it.

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
  has an exited or dead Sandbox container. The project's records and lock
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

Each platform folder implements one interface (`endpoint`, `status`,
`start`, `restart`, `init`, `path` and the Docker, home and disk helpers):

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

The runtime starts a worker only on macOS and Windows. The worker route is
open to any integration (owner, 2026-10-24): on those platforms the runtime
hands its endpoint to `@layerfs/agent` (`useWorker(endpoint)`, without
changing the environment), so every client made after `open` in its process
uses the worker; a client made before keeps its in-process binding. One
worker per process: a second runtime of another home is refused. Another
process, such as an agent's command, uses the worker when its
`LAYERFS_WORKER_ENDPOINT` names the endpoint file; the integration that runs
it sets that. Such a process also needs the project's settings: make the
project with `project.start_init`, which creates and records its volume, then
set `LAYERFS_VOLUME` to the volume `project.list` gives,
`LAYERFS_STORE_PATH` to `/var/lib/docker/volumes/<volume>/_data/global/store.sqlite`,
and `LAYERFS_IMAGE` and `LAYERFS_DAEMON` for its Sandboxes (on macOS, the
bundle's `layerfs-daemon`). A volume made outside the runtime works for Store
calls, but `project.list` shows the project with `volume: null` and
`volume.list` marks the volume `unknown`. The endpoint is `<home>/worker/endpoint.json`, `{ home, port,
secret, build, daemonHost?, docker?, init? }`, readable by its user only. On macOS
`docker` is the engine's socket inside its VM, `/var/run/docker.sock`, which the
worker and Init's container mount; it replaces every call's `deploy.docker`, so the
Mac's own socket (`LAYERFS_DOCKER`) never reaches Linux. The worker
checks every file of the bundle's `manifest.json` before it listens.

## Docker

The runtime calls the `docker` CLI, as the dsh Host did. It has no command
line of its own: the `lfs` command it carried is retired (owner 2026-10-23).
Agents use the `layerfs` command instead, described in [the agent guide](agent.md#the-layerfs-command).
