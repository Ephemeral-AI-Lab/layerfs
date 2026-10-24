# #428 `@layerfs/agent`: the agent package

> **Status:** Proposal; target LayerFS release to be set by the owner; not a
> released contract.

Issue: [#428](https://github.com/Ephemeral-AI-Lab/layerfs/issues/428).
Parent design: [README.md](README.md). Baseline: `main` at `1aa56ece78`
(2026-10-23).

`@layerfs/agent` is the package an agent's script imports to do its work: read
and change files, run commands, keep history, and manage the Sandboxes it uses.
Inspection, storage figures and background processes are in `@layerfs/backend`;
runtime control is in `@layerfs/runtime`
([layerfs-backend-runtime.md](layerfs-backend-runtime.md)). LayerFS ships no
browser code or HTTP; each integration builds its own UI
([ui-frontend.md](ui-frontend.md)). This document defines the agent package.

The per-method details (every argument, result field and refusal) stay in the
skill's method references under
[.agents/skills/layerfs-js-sdk/references/methods/](../../../../.agents/skills/layerfs-js-sdk/references/methods/)
until the package is implemented. This document fixes the surface, the names and
what changes.

## 1. Decisions recorded here

Owner, 2026-10-23, in the #428 discussion:

1. **Packages are named by audience:** `@layerfs/agent`, `@layerfs/backend`,
   `@layerfs/runtime`. There is no `@layerfs/frontend`
   ([ui-frontend.md](ui-frontend.md)). `agent` holds the client and the
   transports; `backend` and `runtime` build on it (section 7). `@layerfs/backend` and
   `@layerfs/runtime` replace the `@layerfs/operator` of earlier drafts.
2. **Sandbox create and close come back to agents.** This reverses #373's
   "callers never create, choose or close a Sandbox" (owner, 2026-10-21) for
   agents only, without bringing back #373's problems (section 4).
3. **`sandbox.list_workspaces` and `project.list_sandboxes` are agent
   methods**, so an agent can see which Sandboxes its project has and what
   each holds.
4. **`workspace` keeps exactly `mount`, `exec`, `commit` and `unmount`.**
   `workspace.status` moves to `@layerfs/backend`, and the skill's conflict
   flow imports it for that (section 6).
5. **The client stays the `LayerFS` class, opened by static factories:**
   `LayerFS.open` for an existing project and `LayerFS.init` for a new one,
   both async and both returning a ready client. The constructor is not public.
   `close` releases the client; a closed client refuses every call
   `InvalidInput`. `project.init` leaves the client (section 3.1).

## 2. Naming rules

These apply to `agent`, `backend` and `runtime`:

- **Listings use the call form.** `LayerFS.<method>` is a static method of the
  class; `shop.<group>.<method>` is a method of a client, where `shop` is the
  value `LayerFS.open` or `LayerFS.init` returned.
- **Methods are snake_case, verb first:** `list_branches`, `diff_files`,
  `list_workspaces`.
- **The verb fixes the result shape:** `get_` returns one record. `list_`
  returns one page, `{ items, hasMore, nextPagePos }`, and takes
  `(…, pageSize, pagePos)` last. `create`, `close`, `start_`, `cancel_` say
  they change state. A read never changes state.
- **Arguments keep the public order:** target first (Branch, ref, Workspace,
  Sandbox), then path, then paging.

## 3. The surface

31 entries: the two static factories and 29 client methods in five groups
(`project` 2, `branch` 6, `fs` 14, `sandbox` 3, `workspace` 4), plus `close`.
Markers: `=` today's name and behaviour, `+` new, `~` changed.

### 3.1 Client: open, init, close

```js
import { LayerFS, LayerFSError, isRefusal, CODES } from "@layerfs/agent";

const shop = await LayerFS.open(name, options);           // + existing project
const cart = await LayerFS.init(name, folder, options);   // ~ was shop.project.init(folder)
await shop.close();                                       // ~ refuses later calls

// LayerFSError, isRefusal(error, code)                   =
// CODES                                                  ~ plus Capacity (section 4.3)
```

A script opens one client per project and closes it in `finally`:

```js
const shop = await LayerFS.open("shop");
try {
  await shop.fs.read("main", "/README.md");
} finally {
  await shop.close();
}
```

**`LayerFS.open(name, options)`** is a static async factory.

1. Checks the name and options: the same project-name pattern and the same
   options as today's constructor (`trace`, `traceStep`, `storePath`,
   `daemonHost`, `storePolicy`, and the `LAYERFS_*` settings; since
   2026-10-24 (owner) no other prefix is read).
2. Opens the project's Store: in this process on Linux, through the worker on
   macOS and Windows.
3. Confirms the project exists.
4. Returns the client.

One attempt, no retry. If any step fails, nothing stays open. Refusals:
`InvalidInput` (name or options), `NoSuchProject`, and `Failed` or `Unknown`
when the Store or the worker can't be reached.

**`LayerFS.init(name, folder, options)`** is a static async factory.

1. Creates the project from `folder`: today's `project.init`, unchanged
   inside.
2. Opens it, as `open` does.
3. Returns the client.

Refusals: `InvalidInput`, `Exists` (the project is already there; open it
instead), and `Failed` or `Unknown` for the Init itself. An uncertain Init
keeps everything it made, as today; nothing is deleted on a guess. Another
process can follow a running Init with
`runtime.project.get_init(job)`.

**`shop.close()`** releases this client.

- Clients of the same project in one process share one Store handle; the last
  one to close closes it. Today's `client.js` already shares it this way.
- It does not unmount Workspaces. They live in Sandboxes and outlive the
  process, and the skill relies on that ("Workspaces are reusable across
  processes"). Unmounting stays the agent's job.
- A second `close` does nothing.
- Every call on a closed client is refused `InvalidInput`, with a message
  saying the client is closed. Today a closed client silently opens the Store
  again on its next call, which hides mistakes.
- The client implements `Symbol.asyncDispose` (as `close`) where the runtime
  defines that symbol, so `await using shop = await LayerFS.open("shop")`
  closes it at the end of the block. Where the symbol doesn't exist, the
  method is simply not installed.

**No public constructor.** `new LayerFS(...)` is refused with a message that
points to `LayerFS.open` and `LayerFS.init`, because a client that skipped them
would be half ready: not opened, and not known to exist. As with #373, this is
a breaking change with no compatibility layer; scripts and the skill change in
the same release.

### 3.2 project

```js
await shop.project.list_branches(pageSize, pagePos);    // =
await shop.project.list_sandboxes(pageSize, pagePos);   // ~ was operator.sandbox.list
```

`list_sandboxes` lists the project's running Sandboxes, each
`{ sandbox, image, workspaces }`, where `workspaces` is how many Workspaces it
holds. It reads the project's Sandbox records and starts nothing. Unlike
today's `operator.sandbox.list`, it changes nothing either: a stopped
Sandbox's record is skipped, not deleted. Placement still removes stale records
on the next mount, and `runtime.sandbox.reclaim(project)` removes them on request.

### 3.3 branch

```js
await shop.branch.list_commits(branch, pageSize, pagePos);                                   // =
await shop.branch.diff_commits(fromCommit, toCommit, pageSize, pagePos, { cursor, hunks });  // =
await shop.branch.diff_branches(fromBranch, toBranch, pageSize, pagePos, options);           // =
await shop.branch.fork(name, fromRef = "main");                                              // =
await shop.branch.merge(a, b, name, message, units);                                         // =
await shop.branch.publish(branch, commit);                                                   // =
```

### 3.4 fs

Reads take any Branch or Commit and change no history. Each write is one
Commit on a Branch when it changes something, and `UpToDate` with no Commit
when it doesn't; the whole change lands or nothing does.

```js
await shop.fs.read(ref, path, startLine, endLine);                  // =
await shop.fs.list(ref, path, pageSize, pagePos);                   // =
await shop.fs.stat(ref, path);                                      // =
await shop.fs.grep(ref, pattern, path, pageSize, pagePos);          // =
await shop.fs.glob(ref, pattern, path, pageSize, pagePos);          // =
await shop.fs.blame(ref, path, fromLine);                           // =
await shop.fs.diff_files(leftRef, leftPath, rightRef, rightPath);   // =

await shop.fs.write(branch, path, content, message);                // =
await shop.fs.edit(branch, path, oldText, newText, message);        // =
await shop.fs.apply(branch, changes, message);                      // =
await shop.fs.mkdir(branch, path, message);                         // =
await shop.fs.rm(branch, path, message);                            // =
await shop.fs.cp(branch, fromPath, toPath, message);                // =
await shop.fs.mv(branch, fromPath, toPath, message);                // =
```

### 3.5 sandbox

```js
await shop.sandbox.create({ image });                             // +
await shop.sandbox.list_workspaces(sandbox, pageSize, pagePos);   // +
await shop.sandbox.close(sandbox);                                // +
```

Section 4 defines them.

### 3.6 workspace

```js
const ws = await shop.workspace.mount(branch, { sandbox, image });   // ~ optional sandbox
await shop.workspace.exec(ws, command, message, { stdout, stderr }); // =
await shop.workspace.commit(ws, message, force = false);             // =
await shop.workspace.unmount(ws, discard = false);                   // =
```

The signatures show argument names and defaults; they aren't runnable as
written. A Workspace is still `{ sandbox, path }`. `unmount` still refuses
`Dirty` while the Workspace has uncommitted changes, so leaving `status` out
doesn't let an agent lose work by accident.

## 4. Sandboxes

### 4.1 One kind of Sandbox

There is still one kind of project Sandbox, `<prefix><project>-<n>`, numbered
per project, with the records under `<home>/<project>/sandboxes/<n>/` and the
Docker labels `layerfs.role`, `layerfs.project` and `layerfs.sandbox` that
#373 introduced. `create` and placement start the same thing. An agent that
never calls `create` behaves exactly as today.

### 4.2 Methods

**`sandbox.create({ image })`** starts the project's lowest free number on
`image` (default `LAYERFS_IMAGE`) under the project's `ownership.lock`, the
same lock placement uses, and waits until its daemon is ready.

- Returns `{ sandbox, image }`, such as `{ sandbox: "3", image: "sha256:…" }`.
- Refusals and failures are the ones a mount that starts a Sandbox has today:
  `Failed` (with `detail.removed` and `detail.log` when nothing is left) and
  `Unknown` (an earlier start's outcome is unknown; every start is refused
  until a person resolves it). Nothing is retried.
- There is no caller-chosen name, no `project` argument and no `volumes`. All
  three were dropped by #373 and stay dropped.

To find Sandboxes, use `project.list_sandboxes` ([section 3.2](#32-project)).

**`sandbox.list_workspaces(sandbox, pageSize, pagePos)`** lists the Workspaces
mounted in one Sandbox. Each item is `{ sandbox, path, branch, base,
activity, changed }`: `activity` is `Idle` or `Busy`, and `changed` says
whether it holds uncommitted changes. A Sandbox holds at most 16 Workspaces,
so one page normally covers it; the paging arguments follow the naming rule.
Refusal: `NoSuchSandbox`.

**`sandbox.close(sandbox)`** stops that Sandbox now.

- Refused `MountsActive` while any Workspace is mounted in it. `MountsActive`
  is already in `CODES`, unused since #373 removed the old `close`.
- Refused `NoSuchSandbox` when it isn't running.
- Stop then delete, each attempted once. A stop that fails is not followed by
  a delete, and the failure keeps the original cause.
- It closes Sandboxes made by `create` and by placement alike.

### 4.3 Placement and capacity

- **`mount(branch)` without `sandbox`** places the Workspace as today: the
  lowest-numbered running Sandbox of its image with a free slot, else a new one.
  It never fails `Capacity` while Docker can start another Sandbox.
- **`mount(branch, { sandbox })`** mounts in that Sandbox only.
  - Refused `NoSuchSandbox` when it isn't running.
  - Refused `Capacity` when it already holds 16 Workspaces. `Capacity` is a new
    code in `CODES`; the agent chose this Sandbox, so LayerFS doesn't move the
    Workspace elsewhere.
  - Refused `InvalidInput` when `image` is given and differs from the
    Sandbox's image.

### 4.4 Lifetime

Every Sandbox, created or placed, still closes itself two minutes after it has
no Workspaces and no running processes (owner, 2026-10-21). `close` only
means "stop now". A Sandbox an agent forgets to close can't leak.

### 4.5 Why this doesn't bring back #373's problems

| #373 problem | This design |
|---|---|
| P1: two ways to get the same container | One kind of Sandbox; `create` and placement start the same thing, and placement stays the default |
| P2: names global to a LayerFS home | Numbers per project; no caller-chosen name |
| P3: no placement, `Capacity` after 16 | Placement stays the default; `Capacity` only for a Sandbox the agent chose |
| P4: Import needs a Sandbox | Unchanged: Import needs none |
| P5, P6: random names, no labels, app-specific names | Same names and labels as placed Sandboxes |
| P7: lifetimes differ; nothing closes when idle | Idle close for every Sandbox; `close` stops one sooner |

### 4.6 What agents gain

- **A Sandbox of their own:** another agent's runaway process or dev server
  can't slow it down.
- **A chosen image** for a whole session.
- **A known place for long work**, such as a dev server and the test runs
  against it.
- **Immediate teardown** instead of the two-minute idle wait.
- **Cleanup with agent calls only:** `project.list_sandboxes`, then
  `sandbox.list_workspaces` for each, then `commit` or `unmount` each idle
  Workspace. Today's skill lists Sandboxes with `operator.sandbox.list` and
  probes paths 1 to 1024 in each with `status`
  ([workspace.md, Clearing idle Workspaces](../../../../.agents/skills/layerfs-js-sdk/references/methods/workspace.md#clearing-idle-workspaces)).

## 5. What moves to backend and runtime

Everything an agent doesn't need for most steps. The full lists are in
[layerfs-backend-runtime.md](layerfs-backend-runtime.md); these are the calls
that leave the agent's reach or change name.

| Today | Becomes |
|---|---|
| `lf.workspace.status(ws)` | `backend.daemon.workspace.status(ws)` |
| `operator.workspace.changes(ws, …)` | `backend.daemon.workspace.list_changes(ws, pageSize, pagePos)` |
| `operator.workspace.overlay(ws)` | `backend.daemon.workspace.get_storage(ws)` |
| `operator.process.start(ws, command)` | `backend.daemon.process.start(ws, command)` |
| `operator.process.list()` | `backend.daemon.process.list(pageSize, pagePos)` |
| `operator.process.cancel(process)` | `backend.daemon.process.cancel(process)` |
| `operator.store.stats()` | `backend.store.project.get_storage()` |
| `operator.store.overwritten(layer, …)` | `backend.store.project.list_overwritten(layer, pageSize, pagePos)` |
| `operator.store.commits(branch, …)` | `backend.store.branch.list_commit_storage(branch, pageSize, pagePos)` |
| `operator.files.stats(ref, path)` | `backend.store.fs.get_storage(ref, path)` |
| `operator.files.history(ref, path, …)` | `backend.store.fs.list_revisions(ref, path, pageSize, pagePos)` |
| the reclaiming inside `operator.sandbox.list()` | `runtime.sandbox.reclaim(project)` |

`operator.sandbox.list()` itself moves the other way, to the agent, as
`shop.project.list_sandboxes` ([section 3.2](#32-project)).

Agents still import `@layerfs/backend` for three jobs: conflicts that need
`workspace.status` (section 6), background processes (`process.start`,
`process.list`, `process.cancel`), and observing a run (the
[layerfs-sdk-observe](../../../../.agents/skills/layerfs-sdk-observe/SKILL.md)
skill). That matches today, where agents already import
`@layerfs/sdk/operator`. `agent` is what most steps need, not a fence around
what an agent may call.

## 6. Skill changes

These edits land with the implementation, not before. An integration may hand
live agents this checkout's skill (dsh does), so an edit that names a method
before it exists breaks running agents.

| File | Change |
|---|---|
| [SKILL.md](../../../../.agents/skills/layerfs-js-sdk/SKILL.md) | Import from `@layerfs/agent`. Every example opens with `await LayerFS.open(…)` instead of `new LayerFS(…)` and closes in `finally`; a new project uses `LayerFS.init(name, folder)`. Replace "You never create, pick or close a Sandbox" with: placement is the default; `sandbox.create` gives you a Sandbox of your own; `close` it when done. "Check a Workspace" row → `backend.daemon.workspace.status`. The methods table loses `status` from `workspace`, gains `list_sandboxes` in `project` and gains the `sandbox` row |
| [methods/project.md](../../../../.agents/skills/layerfs-js-sdk/references/methods/project.md) | `project.init` becomes `LayerFS.init`, with `LayerFS.open` and `close` beside it; adds `project.list_sandboxes` |
| [methods/workspace.md](../../../../.agents/skills/layerfs-js-sdk/references/methods/workspace.md) | `mount`'s `sandbox` option and its refusals. The `status` section moves to the observe skill's backend reference. "Clearing idle Workspaces" uses `project.list_sandboxes` and `sandbox.list_workspaces`, with no operator import |
| `methods/sandbox.md` (new) | `create`, `list_workspaces`, `close` |
| [rules/conflicts.md](../../../../.agents/skills/layerfs-js-sdk/references/rules/conflicts.md) | Scenario 5 imports `@layerfs/backend` for `status` (below) |
| [layerfs-sdk-observe](../../../../.agents/skills/layerfs-sdk-observe/SKILL.md) and [references/operator.md](../../../../.agents/skills/layerfs-sdk-observe/references/operator.md) | `@layerfs/backend` names; gains `workspace.status` ([layerfs-backend-runtime.md §7](layerfs-backend-runtime.md#7-skill-changes)) |

Scenario 5 in `conflicts.md`, after the change:

```js
// ws: the Workspace holding your changes. Don't mount again: a new mount is a new Workspace,
// without these changes or any conflict markers.
import { LayerFS } from "@layerfs/agent";
import { LayerFSBackend } from "@layerfs/backend";

const shop = await LayerFS.open("shop");
const backend = LayerFSBackend.of(shop);
const s = await backend.daemon.workspace.status(ws);
if (s.headMovedSinceMount) {
  const c = await shop.workspace.commit(ws, "step 15: full rebuild", true);
  console.log(c.kind, "displaced", c.displacedHead);
}
```

The rest of the conflict flow uses only agent methods (`fs.diff_files`,
`fs.read`, `workspace.exec`, `branch.list_commits`, `workspace.commit`,
`workspace.unmount`) and doesn't change.

## 7. Package and implementation

- **Three packages, `agent` at the base.** `@layerfs/agent` holds the client,
  the transports and the agent methods; `@layerfs/backend` and
  `@layerfs/runtime` build on it. Each method is one file, written once; today's
  files already take the client as their first argument (`fork(client, name,
  fromRef)` in [api/agent/sdk/src/branch/fork.js](../../../../api/agent/sdk/src/branch/fork.js)).
  The expected folders are in [layout.md](layout.md).
- **Same version for all packages**, released together with `layerfs.node` and
  `layerfs-daemon`. `backend` and `runtime` depend on an exact `agent` version.
- **One copy of `agent` per process.** `agent` registers itself on a global
  key when it loads and refuses to load a second time, so the same Store is
  never opened twice.
- **Client lifecycle** (JavaScript only, in today's
  [api/agent/sdk/src/index.js](../../../../api/agent/sdk/src/index.js)):
  - `LayerFS.open` runs today's constructor checks, opens the Store with the
    existing `storeOpen`, then makes one project lookup on it, the same check
    every Store call makes today, so a missing project is `NoSuchProject` at
    once.
  - `LayerFS.init` runs today's `project.init`, then the same open.
  - The constructor takes a private token that only the factories hold, and
    refuses `InvalidInput` without it.
  - `close` sets a closed flag before releasing the Store with the existing
    `storeClose`; `call` checks the flag first. This replaces today's
    behaviour of reopening the Store on the next call.
  - `[Symbol.asyncDispose]` is installed when `Symbol.asyncDispose` exists.
- **Native work:**
  - `create` exposes the existing Sandbox start in
    [api/binding/src/sandbox.rs](../../../../api/binding/src/sandbox.rs)
    under the project's `ownership.lock`.
  - `close` adds a stop refused while Workspaces are mounted, reusing the
    reclaim path's stop and delete.
  - `mount` takes an optional `sandbox`; placement is skipped when it's given.
  - `list_workspaces` reads the Sandbox's mount records or asks its daemon.
- `list_sandboxes` is today's `operator.sandbox.list` record read without
  the reclaim step.
- **Protocol names:** `project.list_sandboxes`, `sandbox.create`,
  `sandbox.list_workspaces` and `sandbox.close` join the method list ([README.md §3.2](README.md#32-method-list-current-names)).
  The trace's `method` field uses them as written.
- **Tests** follow the repository rules: each under 2 minutes, real Disposable
  Stores, and the #373 placement tests kept (a 17th placed Workspace still
  lands in Sandbox 2).

## 8. Open questions

1. **`list_workspaces` source:** Sandbox records only (fast, starts nothing),
   or the daemon (adds `activity` and `changed`, but needs the Sandbox
   running, which it is whenever it has Workspaces)?
2. **`close` with background processes:** a process runs in a Workspace, so
   `MountsActive` already covers it. Confirm that's enough, or add a separate
   refusal.
3. **Actor label:** should `create` record the creating agent in the
   Sandbox's Docker labels for the UI ([README.md §5.3](README.md#53-shared-base))?
