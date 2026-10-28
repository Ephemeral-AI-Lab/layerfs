# #428 `@layerfs/backend` and `@layerfs/runtime`

> **Status:** Proposal; target LayerFS release to be set by the owner; not a
> released contract.

Issue: [#428](https://github.com/Ephemeral-AI-Lab/layerfs/issues/428).
Parent design: [README.md](README.md). Agent package:
[layerfs-agent.md](layerfs-agent.md). UI boundary:
[ui-frontend.md](ui-frontend.md). Baseline: `main` at `1aa56ece78`
(2026-10-23).

Today's internal operator (`@layerfs/sdk/operator`, `LayerFSOperator`) is split
in two, plus the runtime code that lives in the dsh Host:

- **`@layerfs/backend`**: calls about one opened project's data, answered by
  LayerFS's back end: the Store (committed history) and the Sandboxes (live
  Workspaces and processes).
- **`@layerfs/runtime`**: calls about one LayerFS installation (a LayerFS home):
  the worker, Docker, the list of projects, Init jobs, removal, one project's
  live picture and events. It serves no HTTP
  ([ui-frontend.md](ui-frontend.md)) and has no CLI: the `layerfs` command
  in `api/agent/cli/` is the agents' CLI (owner 2026-10-23).

The name "operator" is retired.

## 1. Decisions recorded here

Owner, 2026-10-23, in the #428 discussion:

1. **Two packages, `@layerfs/backend` and `@layerfs/runtime`**, replace
   `@layerfs/operator`. With `@layerfs/agent` there are three packages; there
   is no `@layerfs/frontend`, because UIs and HTTP are outside LayerFS
   ([ui-frontend.md](ui-frontend.md)). `agent` holds the client and the
   transports; `backend` and `runtime` build on it, with no separate shared
   package.
2. **`backend` has two parts, `store` and `daemon`**, named for where a call is
   answered: `backend.store.*` needs no running Sandbox; `backend.daemon.*`
   needs its Sandbox running.
3. **Calls that fit neither part move to runtime:** one project's live
   picture (`get_state`), reclaiming stale Sandbox records and the event stream.
4. **Processes get their own group** (`backend.daemon.process`), instead of
   being spread over the workspace, project and sandbox groups.
5. **Runtime has its own entry**, not a property of a project client, because
   it must work when no project can be opened (section 4.1).

## 2. Who uses what

| Package | Scope | Entry | Used by |
| --- | --- | --- | --- |
| `@layerfs/agent` | one project | `LayerFS.open`, `LayerFS.init` | agents |
| `@layerfs/backend` | one project | `LayerFSBackend.of(shop)` | agents (conflicts, background processes, observing), integration hosts |
| `@layerfs/runtime` | one LayerFS home | `LayerFSRuntime.open({ home, … })` | integration hosts only |

**Terms.** An *integration host* is the server side of an integration, such as
the dsh Host. *Back end* means LayerFS's own Store and daemons. Integration
hosts use `runtime`; agents and integration hosts both use `backend`.

```text
agent ◄── backend ◄── runtime
  │
  └──► layerfs.node (Linux), or the worker (macOS, Windows)
```

`backend` takes an opened agent client; `runtime` builds `get_state` from
agent and backend calls. All three packages ship together at one version, with `layerfs.node` and
`layerfs-daemon`.

## 3. `@layerfs/backend`

```js
import { LayerFSBackend } from "@layerfs/backend";

const backend = LayerFSBackend.of(shop);   // shop from LayerFS.open or LayerFS.init
```

`of` is synchronous and opens nothing: it uses `shop`'s Store. It has no
`close` of its own; once `shop` is closed, every backend call is refused
`InvalidInput`, as `shop`'s own calls are. There is no public constructor.

### 3.1 `backend.store`: committed history

Answered by the project's Store, in this process on Linux or in the worker on
macOS and Windows. Needs no running Sandbox and starts nothing.

```js
await backend.store.project.get_history();                                // ← operator.inspect.history (native only today)
await backend.store.project.get_storage();                                // ← operator.store.stats
await backend.store.project.get_comparable(id);                           // + the Commit with a saved id's files
await backend.store.project.list_overwritten(layer, pageSize, pagePos);   // ← operator.store.overwritten
await backend.store.branch.list_commit_storage(branch, pageSize, pagePos); // ← operator.store.commits
await backend.store.fs.get_storage(ref, path);                            // ← operator.files.stats
await backend.store.fs.list_revisions(ref, path, pageSize, pagePos);      // ← operator.files.history
```

| Method | Returns |
| --- | --- |
| `project.get_history` | Branches, Commits and Layers, with a version token |
| `project.get_storage` | Store totals: logical, canonical, stored, metadata bytes; dedupe and compression ratios; what Init recorded |
| `project.get_comparable` | for a saved Commit or Layer id, the Commit with the same files: `{ commit, root, exact }`; refuses `NotComparable` (new; today in dsh `reads.js`, [ui-frontend.md §4](ui-frontend.md#4-rules-that-move-out-of-route-code)) |
| `project.list_overwritten` | the `main` Layers a publication replaced without containing them, newest first |
| `branch.list_commit_storage` | each Commit's new canonical, stored and metadata bytes |
| `fs.get_storage` | a file's size and its stored cost across revisions (bounded count) |
| `fs.list_revisions` | the Commits that changed a path, newest first |

Today's handler: [layerfs-store-service/src/operator/dispatch.rs](../../../crates/layerfs-store-service/src/operator/dispatch.rs).

### 3.2 `backend.daemon`: live state

Needs the Sandbox holding the Workspace or process to be running; a call to
one that is gone is refused `NoSuchSandbox`.

```js
// workspace
await backend.daemon.workspace.status(ws);                          // ← lf.workspace.status (public today)
await backend.daemon.workspace.list_changes(ws, pageSize, pagePos); // ← operator.workspace.changes
await backend.daemon.workspace.get_storage(ws);                     // ← operator.workspace.overlay

// process
await backend.daemon.process.start(ws, command);                    // ← operator.process.start
await backend.daemon.process.list(pageSize, pagePos);               // ← operator.process.list
await backend.daemon.process.cancel(process);                       // ← operator.process.cancel

// sandbox
await backend.daemon.sandbox.get_log(sandbox, { since });           // + new
```

| Method | Answered by | Returns |
| --- | --- | --- |
| `workspace.status` | the Sandbox's daemon | base Commit, whether the Branch head moved since mount, activity, overlay counts |
| `workspace.list_changes` | the Sandbox's daemon | uncommitted added, modified and removed paths |
| `workspace.get_storage` | the Sandbox's daemon | the Workspace overlay's payload, inodes, entries and metadata rows |
| `process.start` | the Sandbox's command launcher | a process id, `proc_<sandbox>_<hex>`, which names its Sandbox |
| `process.list` | every running Sandbox | the project's background processes, each with its `sandbox` |
| `process.cancel` | the process's Sandbox | — |
| `sandbox.get_log` | the Sandbox's container | the daemon's log lines since `since` |

Background processes run through the Sandbox's ordinary command launcher, not
the daemon's protocol: the daemon supervises no commands
([core/AGENTS.md](../../../AGENTS.md#filesystem-access-and-execution-ownership)).
They are in `daemon` because they share its condition: the Sandbox must be
running.

`sandbox.get_log` replaces the step both skills give agents today: read the
container id from `$LAYERFS_HOME/<project>/sandboxes/<n>/container` and run
`docker logs` before the Sandbox closes itself. The binding reads it instead,
so it also works on macOS and Windows, where the agent's process may have no
Docker access.

Today's handlers: [api/binding/src/dispatch.rs](../../../../api/binding/src/dispatch.rs)
(process calls) and [layerfs-daemon/src/operator/dispatch.rs](../../../crates/layerfs-daemon/src/operator/dispatch.rs)
(Workspace calls).

## 4. `@layerfs/runtime`

```js
import { LayerFSRuntime } from "@layerfs/runtime";

const runtime = await LayerFSRuntime.open({ home, image, docker, … });
try {
  await runtime.status();
} finally {
  await runtime.close();
}
```

`LayerFSRuntime.open` reads the deployment settings that today's dsh Host reads
from its profile ([config.js](../../../../dsh/packages/host/src/config.js)):
`home`, `image`, `docker`, `daemon`, `bundle`, `volumes`, `traceDir`,
`containerPrefix`, `stateDir`, and on Windows the WSL settings. A missing
required setting is listed by `status()` rather than refusing `open`, so a
runtime page can show what to fix.

### 4.1 Why runtime is not part of a project client

On macOS and Windows, opening a project goes through the worker. When the
worker is down, `LayerFS.open` fails, so no project client exists to reach a
`worker.start` hanging off it. The same holds before the first project exists,
after a project is removed, and for a page that lists every project. Runtime
therefore opens on a LayerFS home, not on a project.

### 4.2 Calls

```js
await runtime.status();                                    // Docker, worker, disk, missing settings
await runtime.worker.start();                              // the worker on macOS and Windows; refused on Linux
await runtime.worker.restart({ confirm: "restart" });

await runtime.project.list();                              // projects in this home
const job = await runtime.project.start_init(name, folder);
await runtime.project.get_init(job);                       // ← operator.project.progress
await runtime.project.get_state(name, since);              // ← inspect/snapshot.js
await runtime.project.remove(name, { confirm: name, stopSandboxes });

await runtime.sandbox.reclaim(project);                    // ← the side effect of operator.sandbox.list
await runtime.sandbox.clean_leftovers(names);
await runtime.sandbox.list();                              // this home's Sandbox containers with scratch bytes (#423)
await runtime.sandbox.sweep();                             // reclaim every project's stopped Sandboxes; also once at open
await runtime.volume.list();                               // every volume with size, owner and use
await runtime.volume.remove(names);                        // this home's orphan volumes only

runtime.events.subscribe({ project, since });              // + live calls from every process
```

| Call | Does | Today |
| --- | --- | --- |
| `status` | Docker reachability, the worker's state, free disk, missing settings | dsh `runtime.js` overview, `GET /layerfs/status` |
| `worker.start`, `worker.restart` | start a stopped or missing worker; restart a running one | dsh `box.js`, `windows/host.js`, `POST /layerfs/runtime/store[/restart]` |
| `project.list` | every project in the home | dsh `bindings.projects()` |
| `project.start_init`, `project.get_init` | run Init as a background job in the runtime's process, and read its step and counts | dsh `init.js`, `jobs.js`; native `operator.project.progress` |
| `project.get_state` | one project's live picture: `backend.store.project.get_history`, the agent's `project.list_sandboxes` and `sandbox.list_workspaces`, and `backend.daemon.workspace.get_storage`, joined under one version token | [api/runtime/src/project/state.js](../../../../api/runtime/src/project/state.js) |
| `project.remove` | delete a project's volume and records, optionally stopping its Sandboxes first | dsh `remove.js` |
| `sandbox.reclaim` | drop records of Sandboxes whose containers are gone or stopped | the reclaim inside today's `operator.sandbox.list` |
| `sandbox.clean_leftovers` | remove named stopped containers | dsh `runtime.js`, `POST /layerfs/runtime/leftovers` |
| `sandbox.list` | this home's Sandbox containers: state, ports, image and `scratch` (writable-layer bytes) | dsh `runtime.js` `containersOf` and `nameImages` (#423) |
| `sandbox.sweep` | `sandbox.reclaim` for every project with an exited or dead Sandbox; run once at `open` unless `sweep: false` | none: a stopped Sandbox waited for its project's next mount (#423) |
| `volume.list`, `volume.remove` | every volume with size, owner (`this`, `other`, `unknown`) and use; remove this home's orphans | none (#423) |
| `events.subscribe` | each call from every process in this home, as trace-line fields plus the actor | dsh reads trace files in `collector.js` |

Modules, not methods: the platform folders and the worker program
([section 4.4](#44-platforms)). The `lfs` command that was
`@layerfs/runtime/cli` is retired (owner 2026-10-23): agents use the
`layerfs` command in `api/agent/cli/`, one `@layerfs/agent` call each, with no short
forms.

### 4.3 Safety

- **Destructive calls name what they destroy.** `project.remove` needs
  `confirm` equal to the project's name; `worker.restart` needs
  `confirm: "restart"`. A missing or wrong `confirm` is refused
  `InvalidInput` before any effect.
- **One attempt, no retry**, as everywhere. A `remove` or Init whose outcome is
  uncertain keeps what exists and reports `Unknown`; nothing is deleted on a
  guess. The Windows and macOS Init failure records (`init-failures.jsonl`,
  `windows-init-failures.jsonl`) stay.
- **Agents don't import runtime.** No skill teaches it, and opening it needs
  deployment settings an agent's environment doesn't normally have. That's a
  convention, not a security boundary.

### 4.4 Platforms

Linux, macOS and Windows each get one platform folder that implements the same
interface. One worker program and one worker client serve macOS and Windows;
Linux needs neither. This replaces today's dsh Host code, which builds the
worker twice (`port/` for macOS, `windows/` for Windows) and shares only the
wire format.

**What each platform provides:**

| | `linux` | `macos` | `windows` |
| --- | --- | --- | --- |
| Where Store calls run | in the caller's process | the worker | the worker |
| The worker | none | a Docker container | a Node process in a chosen WSL2 distribution |
| How the worker starts | — | `docker run` | `wsl.exe` |
| Docker engine for Sandboxes | `/var/run/docker.sock` | Docker Desktop or colima | the distribution's own Docker Engine |
| Path mapping | none | the host path, bind-mounted into the container | `wslpath` |
| Where Init runs | in-process, into the project's volume | its own container with the source folder mounted | in the worker, reading the folder through WSL |
| Native build | `LAYERFS_NATIVE`, else `api/binding/layerfs.node` | bundle folder | sealed bundle, checked against `manifest.json` |

**The interface.** Every platform folder exports the same object:

```js
export default {
  endpoint(settings),             // where the worker listens; null on linux
  status(settings),               // → { worker: null | { state, build }, docker, … }
  start(settings),                // start the worker
  restart(settings),              // restart a running worker
  init(settings, name, folder),   // run Init where the Store lives
  path(settings, hostPath),       // a host path as the Linux side sees it
};
```

`runtime.worker.*` and `runtime.project.start_init` go through it, so no
runtime caller tests the platform. On Linux `status()` reports
`worker: null`, and `start` and `restart` are refused `InvalidInput` ("Linux
runs Store calls in-process; there is no worker"). Doing nothing silently would
hide a misconfigured deployment.

**Choosing the transport.** `@layerfs/agent` picks its own binding, because it
can't depend on runtime: the worker client (`transport/worker_client.js`, the
other end of runtime's `worker/server.js`) when a worker endpoint is named,
else the real addon in the caller's process. The worker client connects to the
endpoint that runtime's platform launcher writes; the endpoint is a setting of
its own (`LAYERFS_WORKER_ENDPOINT`, or runtime's `useWorker(endpoint)`). Only
the macOS and Windows platforms start a worker. Any integration may name the
endpoint; LayerFS checks no integration name (owner, 2026-10-24). `LAYERFS_NATIVE` only ever names a real
addon again; today dsh points it at a stand-in binding
([port/sdk.js](../../../../dsh/packages/host/src/port/sdk.js),
[windows/sdk.js](../../../../dsh/packages/host/src/windows/sdk.js)) in every
bash call, and that injection goes away.

**Folder layout.** The expected folders, with production LOC and change
markers, are in [layout.md](layout.md): agent's `transport/` in section 2 and
runtime's `worker/` and `platform/` in section 4.

Removed, because `transport/index.js` picks the binding: `port/sdk.js` (8),
`port/operator.js` (2), `windows/sdk.js` (11) and `windows/operator.js` (2).

**Names made consistent:**

| Today | Consolidated |
| --- | --- |
| `port/` (macOS), `windows/` | `platform/macos/`, `platform/windows/`, and a new `platform/linux/` |
| "box", "Store container", container `layerfs-store` | the worker: container `<prefix>worker`, label `layerfs.role=worker` |
| two worker programs | `worker/server.js` |
| two stand-in bindings | `transport/worker_client.js` |
| `LAYERFS_NATIVE` pointed at a stand-in | `LAYERFS_NATIVE` names the real addon only; the worker endpoint is its own setting |
| macOS build record in `<home>/.layerfs-store.json`; Windows `manifest.json` hashes | one rule: the worker refuses a bundle whose `manifest.json` hashes don't match |

Renaming the container broke running installs once. For a while, a
confirmed `worker.start` or `worker.restart` stopped and removed the older
`<prefix>store` container before starting `<prefix>worker`. That upgrade path is
retired (owner, 2026-10-24): `worker.start()` takes no options and stops or
removes no other container, `worker.restart({ confirm: "restart" })` only
restarts the worker, and `status()` no longer reports an older Store
container.

**What stays the same:** loopback only, a random secret, versioned frames, a
`ProtocolError` on a codec mismatch, one attempt per call, a lost reply
reported `Unknown` and never resent, and worker and clients upgraded together
with no calls in flight. The Windows failure record
(`windows-init-failures.jsonl`) and the macOS one (`init-failures.jsonl`) stay
until Init failures have one shared record.

**Open for the owner** ([section 8](#8-open-questions)): whether Linux may
opt in to the worker. The route serves every integration on macOS and Windows
(owner, 2026-10-24).

## 5. Protocol names

The native method names follow the JavaScript names, so trace lines and code
read the same:

| Today | New |
| --- | --- |
| `operator.inspect.history` | `backend.store.project.get_history` |
| `operator.store.stats` | `backend.store.project.get_storage` |
| `operator.store.overwritten` | `backend.store.project.list_overwritten` |
| `operator.store.commits` | `backend.store.branch.list_commit_storage` |
| `operator.files.stats` | `backend.store.fs.get_storage` |
| `operator.files.history` | `backend.store.fs.list_revisions` |
| `workspace.status` | `backend.daemon.workspace.status` |
| `operator.workspace.changes` | `backend.daemon.workspace.list_changes` |
| `operator.workspace.overlay` | `backend.daemon.workspace.get_storage` |
| `operator.process.start` | `backend.daemon.process.start` |
| `operator.process.list` | `backend.daemon.process.list` |
| `operator.process.cancel` | `backend.daemon.process.cancel` |
| `operator.inspect.snapshot` (per Sandbox) | `sandbox.list_workspaces` (agent) |
| `operator.sandbox.list` | `project.list_sandboxes` (agent) and `runtime.sandbox.reclaim` |
| `operator.project.progress` | read by `runtime.project.get_init` |

- The public `call()` now refuses `operator.*` names; it refuses `backend.*`
  names instead.
- Old trace files keep the old names. A viewer that replays them needs this
  table as an old-to-new map.
- The dispatch tables that change: [api/binding/src/dispatch.rs](../../../../api/binding/src/dispatch.rs),
  [layerfs-store-service/src/operator/dispatch.rs](../../../crates/layerfs-store-service/src/operator/dispatch.rs),
  [layerfs-daemon/src/operator/dispatch.rs](../../../crates/layerfs-daemon/src/operator/dispatch.rs),
  and the workers, which forward names unchanged.

## 6. What moves out of the dsh Host

Runtime is mostly JavaScript and Docker code that lives in the dsh Host today.
Scope: `dsh/packages/host/src` at `1aa56ece78`, production LOC from
`python3 tools/production_loc.py --files` (scope `dsh`). The counter does not
count `.mjs` files, so `port/call.mjs`, `frames.mjs`, `run.mjs` and
`worker.mjs` (which move with `port/`) are not in these numbers.

| Today's file | prod LOC | Goes to |
| --- | ---: | --- |
| `box.js` | 83 | `platform/macos/index.js` |
| `port/` (`binding.js`, `operator.js`, `sdk.js`) | 133 | `binding.js` merges into agent's `transport/worker_client.js`; `sdk.js` and `operator.js` are removed (#382 moves `port/` first) |
| `windows/` (six files) | 403 | `host.js` and `paths.js` to `platform/windows/`; `worker.js` merges into `worker/server.js`; `binding.js` into agent's `transport/worker_client.js`; `sdk.js` and `operator.js` removed (#425) |
| `config.js` | 37 | `LayerFSRuntime.open` settings |
| `init.js` | 139 | `runtime.project.start_init` |
| `jobs.js` | 28 | `runtime.project.start_init`, `get_init` |
| `remove.js` | 47 | `runtime.project.remove` |
| `runtime.js` | 207 | `runtime.status`, `runtime.sandbox.clean_leftovers` |
| `trees.js`, `reads.js`, `storage.js` | 199 | stay in dsh ([ui-frontend.md §6](ui-frontend.md#6-dsh-as-the-worked-example)); `asCommit` in `reads.js` becomes `backend.store.project.get_comparable` |
| `lfs.js` | 119 | retired; the `layerfs` command in `api/agent/cli/` replaces it (owner 2026-10-23) |
| `layerfs-client.js` | 76 | `runtime/src/clients.js` (per-project clients), using agent's public API only |

The dsh-specific files stay in dsh-layerfs: `index.js`, `binding.js`,
`collector.js`, `env.js`, `records.js`, `route.js` (112), and the UI reads
above.
`migrate.js` (44) is #373's one-time upgrade and is dropped when the code moves.

## 7. Skill changes

Like the agent's, these land with the implementation, because dsh hands live
agents this checkout's skills.

| File | Change |
| --- | --- |
| [layerfs-sdk-observe/SKILL.md](../../../../.agents/skills/layerfs-sdk-observe/SKILL.md) | Import `@layerfs/backend`; new method names; daemon logs through `backend.daemon.sandbox.get_log` |
| [layerfs-sdk-observe/references/operator.md](../../../../.agents/skills/layerfs-sdk-observe/references/operator.md) | Renamed to `backend.md`; gains `workspace.status` and `get_log` |
| [layerfs-sdk-observe/references/daemon-logs.md](../../../../.agents/skills/layerfs-sdk-observe/references/daemon-logs.md) | `get_log` replaces the `docker logs` step |
| [layerfs-js-sdk/references/rules/conflicts.md](../../../../.agents/skills/layerfs-js-sdk/references/rules/conflicts.md) | Scenario 5 uses `LayerFSBackend.of(shop)` for `status` ([layerfs-agent.md §6](layerfs-agent.md#6-skill-changes)) |
| [layerfs-js-sdk/references/methods/workspace.md](../../../../.agents/skills/layerfs-js-sdk/references/methods/workspace.md) | Background processes through `backend.daemon.process`; daemon logs through `get_log` |

## 8. Open questions

1. **`get_log` source:** read through Docker by the binding (works today's way,
   needs Docker access where the binding runs), or a new daemon call that
   returns its own log (works anywhere the daemon answers)?
2. **`events.subscribe` source:** tail the home's trace files, as dsh's
   collector does, or a stream from the worker on macOS and Windows?
3. **Closed: `runtime.http` authentication.** Runtime serves no HTTP, so each
   integration authenticates its own routes ([ui-frontend.md](ui-frontend.md)).
4. **A Linux worker, opt-in:** Linux runs Store calls in-process, so Init
   writes into `/var/lib/docker/volumes` and dsh web must run as root there
   (dsh README, step 4). `platform/linux` could also offer the macOS launcher
   as an opt-in (`worker: "docker"`), so the host process needs no root. That
   widens `AGENTS.md`'s "Linux: in-process only" rule.
5. **Closed: the worker route for every integration** (owner, 2026-10-24).
   The route is open to any integration on macOS and Windows. A named endpoint
   alone opens it; there is no adapter setting or check. It was first allowed
   for dsh only (macOS 2026-10-20, Windows 2026-10-22); #382 and #425 asked
   for it ([README.md, Open decisions](README.md#open-decisions)).
6. **`daemon` as a name:** background processes run through the Sandbox's
   launcher, not the daemon. If that reads as wrong, `backend.sandbox.*` would
   describe all seven calls; it would sit beside the agent's
   `shop.sandbox.*`, which may confuse.
