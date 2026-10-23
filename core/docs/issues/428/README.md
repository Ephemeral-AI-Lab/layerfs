# #428 LayerFS JavaScript SDK and integration design

> **Status:** Proposal; target LayerFS release to be set by the owner; not a
> released contract.

Issue: [#428](https://github.com/Ephemeral-AI-Lab/layerfs/issues/428).
Baseline: `main` at `1aa56ece78` (2026-10-23).

This document describes how the LayerFS JavaScript SDK is organised and how
any integration connects to LayerFS. An integration is a tool that lets agents
or people work on LayerFS projects: today dsh ([dsh/README.md](../../../../dsh/README.md)),
Pi ([#382](https://github.com/Ephemeral-AI-Lab/layerfs/pull/382), open) and
layerfs-wsl ([#425](https://github.com/Ephemeral-AI-Lab/layerfs/issues/425)).
The design is generic: dsh is the first user, not the shape of the API.

What exists today is marked **current**. Changes this proposal asks for are
marked **proposed**, and owner decisions are listed in
[Open decisions](#open-decisions). The current SDK reference is
[api/README.md](../../../../api/README.md); this document does not
replace it. What #428 has changed so far, with its commits and where each
piece stands, is listed in [CHANGES.md](CHANGES.md).
The install-and-test prompt for macOS, Linux and Windows, with the runs so
far, is [live-verification-prompt.md](live-verification-prompt.md).

## 1. Goals

- **One contract.** Every SDK, transport and integration follows one versioned
  list of methods. A new integration or language needs no new API.
- **Same code everywhere.** An integration calls the SDK the same way on
  Linux, macOS and Windows. Only the transport underneath changes.
- **Thin integrations.** Runtime management, the SDK calls and the agent
  skill are shared. An integration adds only what is specific to its host:
  how sessions map to projects, who the agent is, and its own routes and UI
  ([ui-frontend.md](ui-frontend.md)).
- **No private imports.** Integrations use public SDK entry points only.

Out of scope: a standalone LayerFS server or UI page (`AGENTS.md`: the UI is
the dsh plugin), a plugin registry, and any change to Store, daemon or FUSE
semantics.

## 2. Layers

```
 integration adapter    dsh-layerfs, Pi, layerfs-wsl, …   sessions ↔ projects, actor, routes and UI
 integration kit        runtime · layerfs CLI · skill
 SDK                    @layerfs/sdk (JS); other languages later   Project · Branch · Files · Workspace
                                                                    Runtime · Observe · shared base
 transport              in-process (Linux)  |  worker (loopback, secret, one per LayerFS home)
 protocol               versioned method list: name, arguments, result, refusal codes
 core                   Store (one SQLite file) · daemons in Sandboxes
```

Each layer depends only on the layers below it. Agents' scripts use the SDK
directly. An integration's backend uses the SDK and the kit. An integration's
UI talks to its own backend, never to the Store or a daemon. LayerFS ships no
UI and serves no HTTP; see [ui-frontend.md](ui-frontend.md).

## 3. Protocol

### 3.1 Call shape (current)

Every SDK method becomes one binding call: a method name and a plain argument
object. The binding returns either a value or a refusal:

```text
call(method, args)  →  value
                    →  { refused: { code, message, detail } }   (thrown as LayerFSError)
```

The JavaScript methods add argument checks and paging helpers, then call the
binding through `invokeClient` / `invokeOperator` in
[api/agent/sdk/src/client.js](../../../../api/agent/sdk/src/client.js). Store calls (`fs`,
`branch`, `project`, Store operator calls) run against the Store this process
opened. Workspace calls go to a daemon in one of the project's Sandboxes.

### 3.2 Method list (current names)

| Group | Methods | Runs against |
| --- | --- | --- |
| `project` | `init`, `list_branches` | Store |
| `branch` | `list_commits`, `diff_commits`, `diff_branches`, `fork`, `merge`, `publish` | Store |
| `fs` | `read`, `list`, `stat`, `grep`, `glob`, `diff_files`, `blame`, `write`, `edit`, `apply`, `mkdir`, `rm`, `cp`, `mv` | Store |
| `workspace` | `mount`, `exec`, `status`, `commit`, `unmount` | daemon |
| `workspace` (internal) | `exec_release`: frees an exec's output buffer; called by the SDK's `exec`, never by users | binding |
| `operator.store` | `stats`, `commits`, `overwritten` | Store |
| `operator.files` | `stats`, `history` | Store |
| `operator.inspect` | `snapshot`, `history` | Store |
| `operator.project` | `progress` | the running Init (no Sandbox) |
| `operator.sandbox` | `list` | Sandbox records |
| `operator.process` | `start`, `list`, `cancel` | daemon |
| `operator.workspace` | `overlay`, `changes` | daemon |

**Proposed:** publish this list as a machine-readable file in the SDK package
(method, arguments, result shape, refusals each can return, protocol version).
SDKs, transports and the worker codec check against it. The refusal codes are
[`CODES`](../../../../api/agent/sdk/src/errors.js): `Conflict`, `HeadMoved`, `Dirty`,
`Busy`, `NoSuchPath`, `NoSuchBranch`, `NoSuchCommit`, `NoSuchProject`,
`NoCommit`, `NoMergeBase`, `IsDirectory`, `NotDirectory`, `NotFile`, `Binary`,
`NotText`, `NoMatch`, `AmbiguousMatch`, `Exists`, `NotEmpty`, `NameTaken`,
`ReservedName`, `ReadOnly`, `MountsActive`, `TooLarge`, `InvalidInput`,
`NotMounted`, `NoSuchSandbox`, `NoSuchProcess`.

### 3.3 Calls are attempted once

The repository rule applies to every transport: one attempt, no automatic
retry, no replay.

- A call that fails before it is sent is `Failed`.
- A call that was sent and whose reply is lost is `Unknown`, and is never
  resent. A published mutation stays published even if its reply is lost.
- **Proposed:** each call carries a call id (today's trace `id`,
  `<run>-<n>`). A later status query by that id tells the caller whether the
  call landed. That is a lookup, not a retry, and it lets an integration
  resolve `Unknown` without guessing.

## 4. Transports

Both transports carry the same calls with the same results. An integration
picks one by platform; its own code does not change.

### 4.1 In-process (current, Linux)

The SDK loads the native addon `layerfs.node` in the caller's process, from
`LAYERFS_NATIVE` or `api/binding/layerfs.node`, and opens the Store at
`storePath` directly. The Store must be on a local Linux filesystem (ext4, xfs,
btrfs) that the daemons share. This is the only general route today
(`AGENTS.md`, "SDK Store calls … run in the SDK's own process").

### 4.2 Worker (current on macOS and Windows, for any integration)

> **Updated (owner, 2026-10-24):** the worker route is open to any
> integration on macOS and Windows. `@layerfs/runtime` starts the worker on
> those platforms only. Any process that names its endpoint
> (`LAYERFS_WORKER_ENDPOINT`, or runtime's `useWorker(endpoint)`) sends its
> binding calls there; LayerFS checks no integration name. On Linux, Store
> calls still run in the caller's process.

On macOS and Windows, Store calls can't run in the caller's process. The SDK
runs unchanged in the caller's process, but its binding forwards each call to
one long-lived Linux worker per LayerFS home. The worker makes the call with
the real Linux binding and keeps each project's Store and daemon connections
open while the project is in use, closing them after a quiet minute.

| | macOS (first allowed 2026-10-20) | Windows (first allowed 2026-10-22) |
| --- | --- | --- |
| Worker runs in | the container `<containerPrefix>worker` (label `layerfs.role=worker`) | a Node process in a chosen WSL2 distribution |
| Source today | [api/runtime/src/platform/macos/](../../../../api/runtime/src/platform/macos/), [worker/](../../../../api/runtime/src/worker/) | [api/runtime/src/platform/windows/](../../../../api/runtime/src/platform/windows/), [worker/](../../../../api/runtime/src/worker/) |
| Bundle | sealed bundle with `manifest.json`; hashes checked | sealed bundle with `manifest.json`; hashes checked |

Wire format (current, [frames.js](../../../../api/agent/sdk/src/transport/frames.js)):
loopback TCP only, a random secret, and frames made of a 4-byte big-endian
length followed by JSON tagged `layerfs-worker` with its wire version. Every compound value is tagged
(`["buffer", base64]`, `["bigint", "…"]`, `["object", …]`), so binary values
and user objects can't be confused. It doesn't depend on Node or V8 versions.
A codec mismatch is a `ProtocolError`, and the call is never replayed. Both
ends run the same SDK, which the worker checks, so they upgrade together, with
no calls in flight.

**HTTP.** The worker does not speak HTTP today. If integrations outside Node
need it, HTTP (or WebSocket) can frame the same tagged JSON calls, with the
same secret, loopback-only and no-replay rules. It's another framing of the
same protocol, not a second API. **Proposed, pending [Open decisions](#open-decisions) 1.**

**Done:** the worker moved out of `dsh/` into `@layerfs/runtime`
(`api/runtime/src/worker/`), one program for macOS and Windows. The route was
first allowed for dsh only; since 2026-10-24 (owner) it is open to any
integration ([Open decisions](#open-decisions) 1).

## 5. SDK groups

> **Superseded in part (owner, 2026-10-23):** the SDK splits into the
> `@layerfs/agent`, `@layerfs/backend` and `@layerfs/runtime` packages, with no
> UI package ([ui-frontend.md](ui-frontend.md)); agents get `sandbox.create`,
> `sandbox.list_workspaces` and `sandbox.close` back; `workspace` keeps only
> `mount`, `exec`, `commit` and `unmount`; and the operator is replaced by
> `backend` (Store and daemon calls) and `runtime` (the LayerFS home). See
> [layerfs-agent.md](layerfs-agent.md),
> [layerfs-backend-runtime.md](layerfs-backend-runtime.md) and the expected
> folders in [layout.md](layout.md). The tables below show the grouping before
> those decisions. Where LayerFS keeps its files on each platform is in
> [layerfs-home.md](layerfs-home.md).

The SDK's methods sort into groups by who calls them.

### 5.1 Used by agents' scripts

| Group | Purpose | Current methods |
| --- | --- | --- |
| **Project** | create and look at projects | `project.init`, `project.list_branches` |
| **Branch** | history: fork, merge, publish, compare | `branch.fork`, `merge`, `publish`, `list_commits`, `diff_commits`, `diff_branches` |
| **Files** | read files at a Branch or Commit; write as one Commit | `fs.read`, `list`, `stat`, `grep`, `glob`, `blame`, `diff_files`, `write`, `edit`, `apply`, `mkdir`, `rm`, `cp`, `mv` |
| **Workspace** | a mounted, writable view where commands run | `workspace.mount`, `exec`, `status`, `commit`, `unmount` |

**Sandbox is not one of these groups.** An agent asks for a Workspace, and
LayerFS places it in one of the project's Sandboxes: `mount` returns
`{ sandbox, path }` and the other Workspace calls take it (#373). An agent
doesn't need to know about Sandboxes beyond passing that value along.

### 5.2 Used by integration backends and UIs

| Group | Purpose | Current | Proposed additions |
| --- | --- | --- | --- |
| **Runtime** | what is running and what to do about it | `operator.sandbox.list`, `operator.process.start/list/cancel`, `operator.workspace.overlay/changes` | worker and Store-container status, start and restart; the settings check behind `GET /layerfs/status`; project removal (now in the dsh Host's `remove.js`) |
| **Observe** | what happened, for views | `operator.inspect.snapshot`, `operator.store.stats/commits/overwritten`, `operator.files.stats/history`, `operator.project.progress` | `inspect.history` as a public method; a live event stream |

Gaps (current):

- `operator.inspect.history` exists in the native binding, but
  `LayerFSOperator` has no method for it. The dsh Host reaches it by importing
  the private `invokeOperator` from `api/agent/sdk/src/client.js`
  ([layerfs-client.js](../../../../dsh/packages/host/src/layerfs-client.js)).
  **Proposed:** add `operator.inspect.history()` and remove the deep import.
- Views learn about new calls by tailing trace files. **Proposed:** an Observe
  event stream carrying the same fields as a trace line (method, id, step or
  actor, outcome, result ids), delivered in-process or over the worker.

### 5.3 Shared base

| Part | Current | Proposed |
| --- | --- | --- |
| Errors | `LayerFSError`, `isRefusal(error, code)`, `CODES` | listed per method in the method list |
| Trace | `LAYERFS_TRACE` file, format `layerfs_trace: 1` ([api/how-to-use/backend.md, The call trace](../../../../api/how-to-use/backend.md#the-call-trace)) | unchanged; event stream reuses its fields |
| Actor | `LAYERFS_TRACE_STEP` / `traceStep`: a free-text label | an `actor` option `{ id, name, parent }` on the client, written into every trace line and event |
| Configuration | `LAYERFS_HOME`, `_DOCKER`, `_IMAGE`, `_VOLUME`, `_DAEMON`, `_STORE_PATH`, `_CONTAINER_PREFIX`, `_DAEMON_HOST`, `_TRACE`, `_TRACE_STEP`, `_TRACE_START`, `_PROJECT` (CLI), `_WORKER_ENDPOINT`; read by their `LAYERFS_` names only (owner, 2026-10-24) | unchanged; an integration that can set only its own prefix maps the names in its own wrapper (dsh-layerfs does) |

**Why actor matters.** Every integration that runs several agents wants to
show who made each Branch and Commit. dsh does it today by labelling trace
lines with a step and matching them to agents in
[collector.js](../../../../dsh/packages/host/src/collector.js). A first-class
actor gives every integration that for free. Whether the actor is also stored
with each Commit (a Store format change) is
[Open decisions](#open-decisions) 2.

## 6. The integration kit

An integration needs the same pieces regardless of its host. Most already
exist inside `dsh/packages/host`. The kit has no UI or HTTP: dsh's routes,
reads, view model and views stay in dsh-layerfs
([ui-frontend.md](ui-frontend.md)).

### 6.1 What the kit provides

| Package (proposed name) | Provides | Comes from |
| --- | --- | --- |
| `@layerfs/sdk` | the SDK (section 5) | `api/agent/sdk` |
| `@layerfs/runtime` | deployment settings and their check, the worker transports, Init jobs with progress, project removal, the runtime overview and actions | `box.js`, `port/`, `windows/`, `config.js`, `init.js`, `jobs.js`, `remove.js`, `runtime.js`, `layerfs-client.js` |
| `layerfs` CLI | one `@layerfs/agent` call per command, JSON out (`api/agent/cli/`, [api/how-to-use/agent.md](../../../../api/how-to-use/agent.md#the-layerfs-command)); replaces dsh's `lfs.js`, retired with its short forms (owner 2026-10-23) | `lfs.js` (retired) |
| SDK skill | what agents read before their first call | `.agents/skills/layerfs-js-sdk/` |
| simulator backend (optional) | Sim projects without Docker, for demos and tests | `sim/` |

### 6.2 Where today's dsh Host code would go

Scope: `dsh/packages/host/src` at `1aa56ece78`, production LOC from
`python3 tools/production_loc.py --files` (scope `dsh`). The counter does not
count `.mjs` files, so `port/call.mjs`, `frames.mjs`, `run.mjs` and
`worker.mjs` are not listed here.

```diff
  host/src/                           [2834]
    binding.js                           139   (dsh: session ↔ project bindings)
    box.js                                83   (kit runtime: macOS Store container)
    collector.js                         213   (dsh: trace lines → agents and nodes)
    config.js                             37   (kit runtime: deployment settings)
    env.js                                66   (dsh: bash environment, skill path)
    index.js                              62   (dsh: cordis plugin entry, settings schema)
    init.js                              139   (kit runtime: Init jobs)
    jobs.js                               28   (kit runtime)
    layerfs-client.js                     76   (kit runtime: per-project clients)
    lfs.js                               119   (retired: the layerfs CLI in api/agent/cli/ replaces it)
    migrate.js                            44   (#373 one-time upgrade; drop at the move)
    reads.js                              80   (dsh: UI reads)
    records.js                             8   (dsh)
    remove.js                             47   (kit runtime: project removal)
    route.js                             112   (dsh: its routes)
    runtime.js                           207   (kit runtime: overview and actions)
    storage.js                            21   (dsh: UI reads)
    trees.js                              98   (dsh: UI reads)
    port/                              [133]   (kit transport: macOS worker; #382 moves it)
      binding.js                         123
      operator.js                          2
      sdk.js                               8
    sim/                               [719]   (kit, optional: simulator backend)
      binding.js                         321
      client.js                           43
      operator.js                          2
      sdk.js                               3
      sed.js                             125
      shell.js                            53
      state.js                           172
    windows/                           [403]   (kit transport: WSL worker; #425)
      binding.js                          88
      host.js                            165
      operator.js                          2
      paths.js                            29
      sdk.js                              11
      worker.js                          108
```

About 799 lines are specific to dsh, including its routes and UI reads. The
rest is kit material. `model` (960), `view` (2,672) and `client` (1,978) are
dsh's UI and stay in dsh-layerfs.

### 6.3 dsh's routes

dsh's `/layerfs` routes ([route.js](../../../../dsh/packages/host/src/route.js))
stay in dsh-layerfs. They are not part of the kit, and LayerFS serves no route
of its own. Each route and the SDK calls behind it are mapped in
[ui-frontend.md §6](ui-frontend.md#6-dsh-as-the-worked-example). Another
integration writes its own routes, under its own prefix and authentication,
on the same calls.

## 7. Building an integration

An integration has three sides. The kit covers most of each.

| Side | The integration does | The kit gives it |
| --- | --- | --- |
| **Agent** | puts the SDK, the `layerfs` CLI, the skill path and the project's settings into the agent's environment; sets the actor | SDK, CLI, skill, configuration names |
| **Backend** | maps its sessions to projects; records which agent is which; starts and stops the runtime with its own lifecycle; serves its own routes | `@layerfs/runtime`, `@layerfs/backend`, the event stream |
| **UI** | draws its own views from its own routes | nothing: LayerFS ships no UI ([ui-frontend.md](ui-frontend.md)) |

A new integration:

1. Depends on `@layerfs/agent`, `@layerfs/backend` and `@layerfs/runtime`, at
   one pinned LayerFS version (section 8).
2. Chooses a transport by platform: in-process on Linux, the worker elsewhere
   (if [Open decisions](#open-decisions) 1 allows it).
3. Gives each agent the SDK, the skill and an actor.
4. Builds its own routes and views if it has a UI
   ([ui-frontend.md](ui-frontend.md)).
5. Follows the repository rules that apply to callers: one attempt per call,
   Disposable Store profile only, tests under 2 minutes.

Pi (#382) is close to this shape already: its tools call `fs.*` for reads and
edits, and `mount → exec → commit → unmount` for bash, through `api/agent/sdk` on
Linux and `port/` on macOS.

## 8. Versions and distribution

The SDK JavaScript, the native addon and the daemon must come from the same
build. The Windows worker already enforces this with the sealed bundle's
`manifest.json`.

| Option | How an integration gets LayerFS | Fits |
| --- | --- | --- |
| Sibling checkout | `LAYERFS_SOURCE=<path>` at a pinned commit, built with the existing Docker recipe (like `DSH_SOURCE` for dsh) | first step; nothing is published |
| Release artifacts | `@layerfs/sdk` and kit packages at a version; `layerfs-daemon` and `layerfs.node` per architecture; the runtime image; the skill | outside integrations and packaged installs (#406) |

The protocol version (3.2) and the worker codec version are checked when a
transport connects, so a mismatched pair fails clearly instead of misbehaving.

## 9. dsh-layerfs migration

The dsh plugin moves to [Ephemeral-AI-Lab/dsh-layerfs](https://github.com/Ephemeral-AI-Lab/dsh-layerfs)
as the first integration built on this design. Order:

1. **Here first:** add the public methods from 5.2, publish the method list,
   and pull the kit out of `dsh/packages/host` (coordinated with #382 and
   #425). dsh keeps working throughout.
2. **Move:** `packages/` (dsh-layerfs, host, client, model, view; the kit parts
   once they have moved out), `sim/`, README, TUTORIAL, `media/`, `docs/`,
   `brand/`, `demo/`, `demo-workspace/`, `tools/build-client.mjs`,
   `tools/check-session.mjs`, `tools/checks/` and the runtime Dockerfile. Keep
   history with `git subtree split --prefix=dsh` or use one import commit.
3. **Drop:** `dsh-layerfs-demo` (disabled permanently, owner 2026-10-21) and
   `mock`, which exists only for it.
4. **Keep here:** the #352 replay harness (`tools/history/`, `scenarios/`,
   `run-scenario`, `restore-store`, `import-run`). It measures LayerFS builds,
   not the plugin.
5. **Clean up here:** remove `dsh/`; update README links, the `AGENTS.md` UI
   rule and Store-call exceptions, and the `dsh` scope in
   `tools/production_loc.py`. The LOC change is a relocation, not a
   simplification.
6. **Check:** `pnpm test` in dsh-layerfs, and an empty `missing` list from
   `GET /layerfs/status` on a fresh profile.

## Open decisions

1. **Worker route for every integration.** **Decided (owner, 2026-10-24):**
   the worker carries Store calls for any integration on macOS and Windows
   (4.2). Optional HTTP framing is still open; until it is decided, LayerFS
   serves no HTTP at all.
2. **Actor storage.** Trace and events only, or also recorded with each
   Commit (a Store format change)?
3. **Sandbox placement.** Should an integration be able to choose a
   Workspace's Sandbox, or does LayerFS always place it (current, and this
   document's suggestion)?
4. **Distribution.** Sibling checkout first, or release artifacts from the
   start (section 8)?
5. **Skill.** Ship the agent skill with the SDK package, or copy it into
   each integration at a pinned version?

## Related

[#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160) (adapters are
sibling projects that use the SDK),
[#373](../373/README.md) (Sandbox placement),
[#382](https://github.com/Ephemeral-AI-Lab/layerfs/pull/382) (Pi, shared `port/`),
[#406](https://github.com/Ephemeral-AI-Lab/layerfs/issues/406) (packaged
install), [#413](https://github.com/Ephemeral-AI-Lab/layerfs/issues/413) and
[#417](https://github.com/Ephemeral-AI-Lab/layerfs/issues/417) (SDK binary
values), [#425](https://github.com/Ephemeral-AI-Lab/layerfs/issues/425) (shared
WSL2 route).
