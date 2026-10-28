# workspace methods

> **Status:** Current general guide. Part of the `layerfs-js-sdk` skill. Result
> examples were captured live in issue #334 Phase A at `b30e4fb8f` through the
> older per-Sandbox calls; the Workspace API (#373) returns the same result
> shapes. Opaque IDs are shortened. Mounting into a chosen Sandbox, and `status`
> moving to `@layerfs/backend`, are #428's.

A Workspace is a complete, writable view of a Branch where ordinary commands run.
**Use a Workspace for Bash commands** (install, build, test, run, generators, changes
across many files); every file a command changes is recorded at the Commit. For one
small read or edit, [`fs.*`](fs-read.md) is cheaper. Examples follow
[SKILL.md](../../SKILL.md#one-script-for-each-group): `layerfs` is the client and `ws`
a mounted Workspace of the Branch `"work"`, as `mount` returned it.

**Shared rules**

- A Workspace is `{ sandbox, path }`: the number of the Sandbox that holds it and its
  path there. It persists: any later process with the same `LAYERFS_HOME` can use it.
- **A mount names its Sandbox; there is no placement.** A project's Sandboxes are
  numbered `1`, `2`, …; each is a container with one LayerFS daemon and holds up to 16
  Workspaces, of any Branches of the project. Each runs one image, set when
  `sandbox.create()` starts it; one project's Sandboxes may run different images.
  `sandbox.create()` is the only call that starts a Sandbox; `mount(sandbox, branch)`
  never does ([sandbox](sandbox.md)).
- **A Sandbox closes itself when idle:** its daemon ends two minutes after its last
  Workspace is unmounted, with no call in flight. A mount there is then refused
  `NoSuchSandbox`; the next `sandbox.create()` reclaims it, and its number is used
  again.
- Keep each Workspace's operations sequential.
- No restart recovery: if the container dies, its Workspaces are gone. Stop and report.

## Workspaces

The same model as [How an agent works in LayerFS](../../SKILL.md#how-an-agent-works-in-layerfs),
in more detail:

- **Each mount is a new Workspace.** `mount` returns a fresh `{ sandbox, path }` (such
  as `{ sandbox: "1", path: "/workspaces/3" }`; paths are numbered per Sandbox). Pass it
  to `exec`, `commit` and `unmount`. Two mounts of one Branch are two separate
  Workspaces, each with its own uncommitted changes. Keep the Workspace from the result;
  never build one yourself, and never use one after its unmount.
- **A Workspace per Bash call is the default:** mount, exec, commit, unmount. Mounting
  doesn't copy or scan the tree, so it's cheap; the Commit then holds exactly what that
  call changed.
- **Longer and parallel work is supported.** Keep a Workspace mounted across several
  `exec` calls and commit at any point, or mount several at once, on one Branch or
  several.
- **Commands use relative paths.** `exec` starts at the Workspace root: write
  `npm test` or `cat src/cart.js`, never the mount path. The path changes with every
  mount, and one written into a file would be committed with it.
- **The view is fixed at mount.** It shows the Branch as it was then and doesn't follow
  later Commits. `commit` merges in what moved; to *see* it, mount a new Workspace.
- **Everything in it is committed:** sources, `.git`, `node_modules`, build output and
  caches. Scratch files belong in `/tmp`, which is outside the Workspace.

One Bash call as one Workspace:

```js
const ws = await layerfs.workspace.mount(sandbox, "work");         // a NEW Workspace in your Sandbox
try {
  const run = await layerfs.workspace.exec(ws, "timeout 110 npm test 2>&1", "step 12");
  await layerfs.workspace.commit(ws, `step 12: npm test (exit ${run.exitCode})`);
} finally {
  await layerfs.workspace.unmount(ws);                            // ws is now invalid
}
```

## `workspace.mount(sandbox, branch)`

Mounts a Workspace showing the Branch head at this moment, in the running Sandbox
`sandbox` names, and nowhere else: it never starts a Sandbox. Doesn't scan or copy the
tree.

| Arg | Type | Required | Meaning |
| --- | --- | --- | --- |
| `sandbox` | string | yes | A running Sandbox's number, such as `"1"` from `sandbox.create()` or `project.list_sandboxes()`. The Sandbox fixes the image |
| `branch` | string | yes | Branch to mount. Not `main`: fork a Branch from it |

**Returns:** the Workspace, `{ sandbox, path, image }`, such as
`{ sandbox: "1", path: "/workspaces/1", image: "sha256:…" }`. Pass it to `exec`, `commit` and
`unmount`; the backend's `workspace.status` tells which Commit it shows (the
`layerfs-sdk-observe` skill).

**Refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `NoSuchBranch` | No such Branch, or `main` | Fork first |
| `InvalidInput` | A missing or bad `sandbox` or `branch`, or an options object (there are none) | Pass `mount(sandbox, branch)` with a Sandbox number |
| `NoSuchSandbox` | The Sandbox isn't running (it closed itself, or was closed). The message says what to do | `sandbox.create()` a new one and mount there |
| `Capacity` | The Sandbox already holds 16 Workspaces. The message says what to do | `sandbox.create()` a new one and mount there, or `sandbox.list_workspaces(sandbox)` and `workspace.unmount` the ones you're done with |
| `Failed` | The mount failed in the daemon | Stop and report the message |

```js
const ws = await layerfs.workspace.mount("1", "work");   // a NEW Workspace: { sandbox: "1", path: "/workspaces/1" }; never hard-code it
```

**Notes:** every call mounts a new Workspace, even on the same Branch (see
[Workspaces](#workspaces)). The view doesn't follow the Branch: after an `fs.*` write on
the same Branch, mount again to see it (or let the next `workspace.commit` merge it in).

## `workspace.exec(ws, command, message?)`

Runs one shell command in a mounted Workspace and waits for it to end.

| Arg | Type | Required | Meaning |
| --- | --- | --- | --- |
| `ws` | `{ sandbox, path }` | yes | The Workspace from `mount` |
| `command` | string | yes | Run with `bash -c`; the working directory is the Workspace root, so use relative paths (`src/cart.js`), never the mount path; `HOME=/tmp` |
| `message` | string | no | Labels the run; at most 4096 bytes; creates no history |

**Returns**

| Field | Type | Meaning |
| --- | --- | --- |
| `kind` | string | `"exit:<code>"` |
| `exitCode` | number | The command's exit code. Non-zero is **not** a refusal |
| `stdout`, `stderr` | string | Each stream, whole |
| `truncated` | boolean | Always `false`: no output is cut |
| `message` | string \| null | Your message, echoed |

**Refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `NotMounted` | The Workspace isn't mounted | Use the Workspace saved from `mount` |
| `NoSuchSandbox` | Its Sandbox is gone (`detail.containerGone`), or a different `LAYERFS_HOME` | Its Workspaces are gone: mount again |
| `Unknown` | The exit code isn't known | Stop: the command may or may not have run |

**Limits**

- No timeout: put `timeout 110` inside the command.
- Waits until everything holding its output has ended, including background jobs.
- Output is returned whole; for a long log, redirect it to a file in `/tmp` and read
  the end, so your script doesn't hold it all.
- Files the command writes are in the Workspace and get committed; keep scratch files
  in `/tmp`.

**Run a check and report its output**

```js
// ws: from workspace.mount. Every mount creates a new Workspace; never hard-code one.
const run = await layerfs.workspace.exec(ws, "timeout 110 npm test 2>&1", "cart tests");
console.log(JSON.stringify(run, null, 2));
const saved = await layerfs.workspace.commit(ws, `step 12: npm test (exit ${run.exitCode})`);
console.log(saved.kind);                  // "UpToDate" when the command changed nothing
```
```json
{
  "kind": "exit:0",
  "exitCode": 0,
  "stdout": "\n> test\n> node --test\n\nTAP version 13\n# Subtest: cart\nok 1 - cart\n  ---\n  duration_ms: 0.433125\n  type: 'test'\n  ...\n# Subtest: cart-api\nok 2 - cart-api\n  ---\n  duration_ms: 0.453209\n  type: 'test'\n  ...\n# Subtest: loads\nok 3 - loads\n  ---\n  duration_ms: 0.533125\n  type: 'test'\n  ...\n1..3\n# tests 3\n# suites 0\n# pass 3\n# fail 0\n# cancelled 0\n# skipped 0\n# todo 0\n# duration_ms 56.672166\n",
  "stderr": "",
  "message": "cart tests",
  "truncated": false
}
```

Commit whatever the command did, failure or not, so the step is recorded. If
`exitCode` isn't 0, read the failing output and the code it points at before you
decide what to change.

**Big output:** write it to `/tmp` and read the end in a second call.

```js
// ws: from workspace.mount. Every mount creates a new Workspace; never hard-code one.
await layerfs.workspace.exec(ws, "timeout 110 npm run build >/tmp/build.log 2>&1; echo $? >/tmp/build.code", "build");
const tail = await layerfs.workspace.exec(ws, "cat /tmp/build.code; tail -n 60 /tmp/build.log", "build log");
```

**Smoke-test a server:** start, probe and stop in **one** call. Send the server's output
to `/tmp`, so it doesn't hold `exec` open or land in the Workspace.

```js
// ws: from workspace.mount. Every mount creates a new Workspace; never hard-code one.
const cmd = [
  "timeout 60 bash -c '",
  "node src/main.js >/tmp/server.log 2>&1 & pid=$!;",
  "sleep 2;",
  "node -e \"fetch(\\\"http://127.0.0.1:3000/health\\\").then(r => { console.log(r.status); process.exit(r.ok ? 0 : 1) }).catch(e => { console.error(e.message); process.exit(1) })\";",
  "s=$?; kill $pid; cat /tmp/server.log; exit $s'",
].join(" ");
const run = await layerfs.workspace.exec(ws, cmd, "smoke test");
```

**Write a file inside a mounted Workspace:** only when this Workspace must see the
change before its next Commit; otherwise use `fs.*` and mount again. Pass the content as
base64 so the shell doesn't reinterpret it (command lines have an OS length limit, so
use this for files up to a few tens of KiB).

```js
// ws: from workspace.mount. Every mount creates a new Workspace; never hard-code one.
const content = "export const TAX = 0.08;\n";
const b64 = Buffer.from(content).toString("base64");
const run = await layerfs.workspace.exec(ws, `printf '%s' ${b64} | base64 -d > src/tax.js`, "write src/tax.js");
if (run.exitCode !== 0) console.log(run.stderr);
```

## Checking a Workspace

`status` moved to `@layerfs/backend` as `backend.daemon.workspace.status(ws)`: base
Commit, whether the Branch head moved since mount, activity and overlay counts. See the
`layerfs-sdk-observe` skill's backend reference, and Scenario 5 of
[resolving conflicts](../rules/conflicts.md) for its one use in an agent's flow.
Background processes (`backend.daemon.process.start`, `list`, `cancel`) are there too.

## `workspace.commit(ws, message?, force?)`

Saves everything changed in the Workspace as one Commit on its Branch. If the Branch
moved since the mount, it merges first. The Workspace stays mounted.

| Arg | Meaning |
| --- | --- |
| `ws` | The Workspace from `mount` |
| `message` | Commit message |
| `force` | `true` to overwrite a newer head with this Workspace's view instead of merging. Drops the other changes; avoid |

**Returns**

| `kind` | Fields | Meaning |
| --- | --- | --- |
| `"Committed"` | `head`, `commit`, `displacedHead`, `merged`, `lww` | New Commit. `merged` is `{ from, head }` when a merge ran, else `null`. `lww` lists files decided last-writer-wins by the head's [`/.lww`](../rules/lww.md), each `{ path, kept, deleted, rule }` |
| `"UpToDate"` | `head`, `root`, `merged`, `lww` | Nothing changed; no Commit |

**Refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `Conflict` | The merge with the moved head overlaps. Markers are written into the Workspace; nothing is committed | See [resolving conflicts](../rules/conflicts.md#commit-conflicts) |
| `NotMounted`, `HeadMoved` | As elsewhere | Check the Workspace; commit again after a look at the backend's `workspace.status` |

```js
// ws: from workspace.mount. Every mount creates a new Workspace; never hard-code one.
const c = await layerfs.workspace.commit(ws, "deps: npm install");
```
```json
{
  "kind": "Committed",
  "head": "122215f69eba…",
  "commit": {
    "id": "122215f69eba…",
    "branch": null,
    "parent": "12df71934c6f…",
    "secondParent": null,
    "base": "32a35edb1f9f…",
    "root": "caf388018578…",
    "via": "exec",
    "message": "A: resolved both changes",
    "mergedFrom": null,
    "time": 1792085429866,
    "logical": 2478,
    "newCanonical": 2849,
    "newStored": 1204,
    "newMetadata": 783,
    "displaced": null,
    "lww": [],
    "lwwTruncated": false
  },
  "displacedHead": null,
  "merged": null,
  "lww": []
}
```

**When Commit reports a `Conflict`:** nothing was committed, and the conflicted files in
the Workspace now hold markers. Read both sides before changing anything. Fixing the
files, ruling generated ones in `/.lww`, and overwriting with `force` are covered,
scenario by scenario, in [resolving conflicts](../rules/conflicts.md#commit-conflicts).

Merge diagnostic lists use byte windows. `lwwTruncated` marks omitted decisions;
conflict detail also exposes `conflictsTruncated`, and individual conflicts mark
`hunksOmitted`. Full marker files remain in the Workspace even when a reply omits
their text. Inspect those files with bounded `workspace.exec` commands and Commit
each command. The omission flags never mean the unresolved source is safe to drop.

## `workspace.unmount(ws, discard?, force?)`

Unmounts a Workspace. A Sandbox left with no Workspace closes itself two minutes later.

| Arg | Meaning |
| --- | --- |
| `ws` | The Workspace from `mount` |
| `discard` | `true` to drop uncommitted changes instead of refusing |
| `force` | `true` to abort the Workspace's kernel connection instead; drops uncommitted changes. Use it only for an unmount that stalled (below) |

**Returns:** `{ kind: "Unmounted" }`, or with `force`
`{ kind: "ForceUnmounted", abort, detach, cleanup }`.

**Refusals**

| Code | Meaning | Do |
| --- | --- | --- |
| `Dirty` | Uncommitted changes; `detail.changed` lists paths | Commit them, so the step is recorded; discard only a Workspace you're abandoning |
| `Busy` | A background process still runs there | Stop it (`backend.daemon.process.cancel`, in the observe skill), or wait for it |
| `NotMounted` | Already unmounted, or the wrong Workspace | Use the Workspace from `mount` |
| `Failed` with `TeardownCustody { stage: Join` in the message | The mount is gone but its connection did not end, so the unmount stalled; every later plain unmount answers the same | `workspace.unmount(ws, true, true)` once |

```js
// ws: from workspace.mount. Every mount creates a new Workspace; never hard-code one.
await layerfs.workspace.unmount(ws);
```

## Clearing idle Workspaces

Nothing unmounts a Workspace for you: a script that exits, throws or is cut off before
its `unmount` leaves it mounted, and its Sandbox never closes. Put every `unmount` in a
`finally` block.

To clear leftovers, list each running Sandbox's Workspaces and unmount only the ones
that are idle, have nothing uncommitted, and that no agent is keeping on purpose, such
as a build Workspace or one running a dev server:

```js
const keep = new Set([/* "<sandbox>:<path>" of Workspaces agents said they're keeping */]);
for (const { sandbox } of (await layerfs.project.list_sandboxes()).items) {
  for (let pos = 0; pos !== null; ) {
    const page = await layerfs.sandbox.list_workspaces(sandbox, 100, pos);
    for (const w of page.items) {
      if (w.activity === "Idle" && w.changed === false && !w.pendingMerge && !keep.has(`${w.sandbox}:${w.path}`)) {
        await layerfs.workspace.unmount({ sandbox: w.sandbox, path: w.path });
      }
    }
    pos = page.nextPagePos;
  }
}
```

Don't pass `discard` for a Workspace another agent mounted. Without it, a Workspace
with uncommitted changes is refused `Dirty` and stays mounted for its owner to commit.

## Daemon logs

When `LAYERFS_TRACE` is set, save each Sandbox's daemon log while it runs, for example
before your last unmount, with `@layerfs/backend`:

```js
import { writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { LayerFSBackend } from "@layerfs/backend";

const log = await LayerFSBackend.of(layerfs).daemon.sandbox.get_log(ws.sandbox, { lines: 2000 });
writeFileSync(`${dirname(process.env.LAYERFS_TRACE)}/daemon-${log.container}.log`, log.text);
```

It reads the last lines (at most 2000 and 64 KiB) through Docker where the SDK runs, so
it works on macOS and Windows too. A Sandbox that closed itself is deleted at the next
mount, and its log with it. This is diagnostic collection; project reads and changes
still use the SDK.

A Workspace call to a Sandbox whose container no longer runs is refused
`NoSuchSandbox` with `detail.containerGone: true`: Docker no longer has the container
(removed outside LayerFS), or has it stopped (`detail.containerStopped: true`, after
Docker restarted, for instance). Its daemon took every mount with it. The failing
call is not resent; mount again.
