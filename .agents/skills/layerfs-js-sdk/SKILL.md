---
name: layerfs-js-sdk
description: Use when building, editing, testing or inspecting a project through LayerFS with the JavaScript SDK (`@layerfs/agent`) from the command line. Activates on LayerFS projects, Branches, Commits, Workspaces or Sandboxes; `LayerFS.open`, `LayerFS.init`, `layerfs.project.*`, `layerfs.fs.*`, `layerfs.branch.*`, `layerfs.sandbox.*` or `layerfs.workspace.*` calls; running npm install, build or tests inside a mounted Workspace; or resolving LayerFS Conflict, Dirty or Busy refusals.
---

# LayerFS JavaScript SDK

> **Status:** Current general guide for the current SDK source. Base example
> outputs were captured live in issue #334 Phase A at `b30e4fb8f`; the
> Workspace API and idle close are #373's (section 8 of
> [its design](../../../core/docs/issues/373/README.md)). The `@layerfs/agent`
> package, its client factories and the `sandbox` methods are #428's
> ([its design](../../../core/docs/issues/428/layerfs-agent.md)). There is no
> placement since 2026-10-23 (owner): a mount names its Sandbox.

Your project lives in LayerFS, not on your local disk. You read and change it through
the SDK, from small Node scripts you run with `node`.

## Inside an integration: start here

An integration is a host that runs you with a LayerFS project already set up. When it
has, its own instructions name the project, how to import the SDK and how to run the
`layerfs` command. Use those, and skip [Import the SDK](#import-the-sdk),
[Connect to a project](#connect-to-a-project) and `LayerFS.init`:

- The SDK reads every setting from `LAYERFS_*` variables, which the integration provides.
  `LayerFS.open(<project>)` needs nothing else.
- If the SDK's location is given in a variable, import it with `await import(<path>)` in a
  module script. A static `import` of a variable is a syntax error.
- Don't look into Docker, Store containers, Store files or the integration host's source.
  Nothing there helps, and the SDK is the only way in.
- Your current folder is for temporary files only: your scripts, notes and scratch output.
  Files you find there may be an old copy, not the project. Do all work on the project
  through the SDK: `fs.*` and Workspaces for files and commands, and `project.*` and
  `branch.*` for versioning, as you would use git.
- The `layerfs` command makes one SDK call per command, for example
  `layerfs project list_branches` or `layerfs fs read <branch> <path>`. Use it for single
  steps and an SDK script for anything more.
- **Always unmount.** Put every `workspace.unmount` in a `finally` block. A forgotten
  Workspace stays mounted, and its Sandbox never closes. To clear idle, clean leftovers,
  see [Clearing idle Workspaces](references/methods/workspace.md#clearing-idle-workspaces).
  End every script with `await layerfs.close()`, in a `finally` block too.
- **`main` is read-only.** It's published, not a Branch: read it and fork from it, but
  don't write, edit, mount or commit to it. Those calls, and `branch.list_commits("main")`,
  are refused with `NoSuchBranch`. Fork a Branch, work there, and publish it. A new
  project's `main` is empty.

## How an agent works in LayerFS

Every tool call an agent makes is one **step**, and every step that changes files becomes
a Commit on the agent's Branch. The Branch's history is the record of what the agent
did, step by step.

| Agent tool | LayerFS calls | Recorded as |
| --- | --- | --- |
| File read: read, list, search, diff | `fs.read`, `fs.list`, `fs.grep`, `fs.glob`, `fs.diff_files` | Nothing: reads don't change history |
| File change: write, edit, many-file change | `fs.write`, `fs.edit`, `fs.apply`, `fs.mkdir`, `fs.mv`, `fs.cp`, `fs.rm` | One Commit per call. Atomic: the whole change lands, or nothing does |
| Bash | A Workspace per call: `workspace.mount` → `workspace.exec` → `workspace.commit` → `workspace.unmount` | One Commit with everything the command changed: sources, lockfiles, `node_modules`, build output |

Two ways in: **`fs.*` for cheap, targeted file work; a Workspace for Bash commands**,
which often change files too, and for file work too heavy for single calls.

### Cheap file work: `fs.*`

Use `fs.*` for the reads and small changes your own file tools make: read a file, list a
folder, search, diff, write or edit one file, move or delete a path, or a handful of
related changes as one Commit (`fs.apply`).

- **Why:** an `fs.*` call runs in your process on the project's Store, without a
  Workspace or kernel mount, so it avoids the Bash mount lifecycle. A change is atomic and is exactly one Commit with your
  message, so the Branch records each edit as its own step. A read returns the Branch's
  latest Commit, so it sees what every Workspace and every other writer has committed.
- **When:** reading code before you change it, creating a file, editing source, renaming
  or deleting a path.
- **No merge:** if another writer committed on the Branch during the call, it's refused
  with `HeadMoved` and nothing is written. Read the file again, then redo your change.
- **Reads see committed state only**, never a mounted Workspace's uncommitted changes.

### Bash commands: a Workspace

Use a Workspace to run Bash commands. Most of them change files as well, and all of
those changes are recorded:

- **Commands that change files as a side effect:** `npm install` (`node_modules`,
  the lockfile), builds (`dist/`), code generators, formatters, `git`, tests that write
  snapshots or coverage. The Commit holds everything the command changed.
- **Heavier file work:** changes across many files or a large tree (`find … | xargs
  sed -i …`, a scaffolder, unpacking an archive), where one command beats many `fs.*`
  calls.
- **Files `fs.*` can't reach:** a mounted Workspace's uncommitted state, such as
  conflict markers after a Commit merge ([resolving conflicts](references/rules/conflicts.md)).

A mount costs more than an `fs.*` call, and its changes are recorded only at the
Commit, so for one small read or edit prefer `fs.*`.

- **A Workspace per Bash call is the default.** Mounting is cheap: it doesn't copy or
  scan the tree. Each call sees the Branch as it is right now, and its Commit holds
  exactly what that call did.
- **Each mount is a new Workspace.** `workspace.mount(sandbox, branch)` returns a fresh
  Workspace, `{ sandbox, path }` (such as `{ sandbox: "1", path: "/workspaces/3" }`);
  pass it to `exec`, `commit` and `unmount`. Two mounts of one Branch are two
  separate Workspaces, and a Workspace is never valid after its unmount.
- **You name the Sandbox.** A project's Sandboxes are numbered `1`, `2`, …, each a
  container holding up to 16 Workspaces on one image (`LAYERFS_IMAGE`, or
  `sandbox.create({ image })`). `sandbox.create()` starts one and returns its number;
  `workspace.mount(sandbox, branch)` mounts there and never starts one. When a mount
  is refused `NoSuchSandbox` (it closed itself) or `Capacity` (16 Workspaces), do what
  its message says: `sandbox.create()` a new one, or `sandbox.list_workspaces(sandbox)`
  and unmount Workspaces you're done with. A Sandbox closes itself two minutes after
  its last Workspace is unmounted; `sandbox.close(sandbox)` stops it now
  ([sandbox](references/methods/sandbox.md)).
- **Inside a command, use relative paths.** `exec` starts in the Workspace root, so
  write `npm test` or `cat src/cart.js`, never the mount path. The path changes with
  every mount, and a mount path written into a file would be committed with it.
- **Longer and parallel work is supported.** A Workspace can stay mounted across calls
  for a long-running command (a dev server, a watcher, a long build) and commit at any
  point. Several Workspaces can be mounted at once, on one Branch or several; when their
  Commits touch the same lines, the later one merges or reports a `Conflict`.
- **Commit after every Bash call, even a failing one,** with a message naming the step
  and its result, such as `"step 12: npm test (exit 1)"`. The Branch then shows exactly
  what each step did, failures included. Discard only a Workspace you're deliberately
  abandoning. `main` is unaffected until you publish.
- **A step that changes nothing makes no Commit.** `workspace.commit` returns `UpToDate`,
  and a message alone never creates history: a read or a passing `npm test` that writes
  nothing leaves no trace on the Branch, by design.

## Merge rules: `/.lww`

Parallel Workspaces and Branches meet in a merge, either when a `workspace.commit` finds
that its Branch moved or in `branch.merge`. Source files merge line by line, and
overlaps become a `Conflict` you resolve by reading. Generated state (`node_modules`,
lockfiles, build output) can't be merged that way, so list it in a `/.lww` file at the
project root. Both kinds of merge then keep the **newer** version of those paths whole
and report each decision in the result's `lww` list.

```
%include .gitignore
```

Treat it as your `.gitignore`: `%include .gitignore` is normally the whole file. Add a
line only in the extreme case, for a generated file you keep in Git that keeps
conflicting, such as a lockfile. Never list source files. Create it in the project's
first step; syntax and which `/.lww` each merge reads are in [merge
rules](references/rules/lww.md). When a merge does conflict, [resolving
conflicts](references/rules/conflicts.md) covers each case: fix it, rule it, or
overwrite it.

## The pieces

| Piece | What it is | SDK group |
| --- | --- | --- |
| Project | One Store holding all files and their whole history. `LayerFS.init` copies a folder into it, once; `LayerFS.open` connects to it | `project` |
| Branch | A named line of Commits. `main` changes only through `branch.publish`; work on your own Branch | `branch` |
| Files | Read and change files on a Branch directly. Every write is one immutable Commit; `fs.apply` makes many changes as one | `fs` |
| Workspace | A complete, writable view of a Branch where ordinary commands run, `{ sandbox, path }`. `workspace.commit` saves **everything** in it: `node_modules`, `.git`, build output, caches | `workspace` |
| Sandbox | A numbered container running one LayerFS daemon, holding up to 16 Workspaces. LayerFS starts one when a mount needs it, or you `create` one; it closes itself when idle | `sandbox`, `project.list_sandboxes` |

## Import the SDK

The SDK is the `api/agent` folder of the LayerFS checkout. Link it into
your folder once:

```bash
npm init -y >/dev/null && npm pkg set type=module
npm install --no-audit --no-fund "file:$LAYERFS_REPO/api/agent"   # LAYERFS_REPO: the LayerFS checkout
```

Then, in any `.mjs` file:

```js
import { LayerFS, isRefusal } from "@layerfs/agent";
```

## Connect to a project

The client reads its settings from the environment. They're set up for you; every
process of one run must see the same values.

| Variable | Meaning |
| --- | --- |
| `LAYERFS_HOME` | Private folder where LayerFS records your project and its Sandboxes |
| `LAYERFS_DOCKER` | Docker daemon Unix socket/endpoint, such as `/var/run/docker.sock`; use the `docker` executable for log collection |
| `LAYERFS_IMAGE` | Runtime image digest, `sha256:…`, that a new Sandbox runs |
| `LAYERFS_VOLUME` | Existing named Docker volume for this project's Store |
| `LAYERFS_DAEMON` | Host path to the built Linux daemon executable |
| `LAYERFS_STORE_PATH` | The project's Store file, which `fs.*`, Branch and project calls open in your process (Linux only) |

```js
const layerfs = await LayerFS.open("shop");   // opens the project's Store; NoSuchProject if it doesn't exist
try {
  // … calls …
} finally {
  await layerfs.close();
}
```

A new project needs `LayerFS.init("shop", folder)` once, which returns an open client;
it starts no Sandbox. After that, `LayerFS.open("shop")` in any later process connects
to the same project. There is no `new LayerFS(…)`: it's refused `InvalidInput`. Store calls
(`fs.*`, Branch calls and branch listing) run in your process. Workspace calls go to
the Sandbox that holds the Workspace. Several agents each mount their own Workspaces,
each in a Sandbox they name, so agents share nothing but the project. Finish each client with
`await layerfs.close()` in a `finally` block; a closed client refuses every call
`InvalidInput`, and its Workspaces stay mounted.

## One script for each group

The four scripts run in this order; each is complete and runs with `node <file>`.

### project.mjs: create the project

```bash
mkdir -p /work/shop/.git
printf 'ref: refs/heads/main\n' > /work/shop/.git/HEAD
printf '# Shop\n' > /work/shop/README.md
printf 'node_modules/\ndist/\ncoverage/\n*.log\n' > /work/shop/.gitignore
printf '%%include .gitignore\n' > /work/shop/.lww
```

```js
// project.mjs
import { LayerFS } from "@layerfs/agent";

const layerfs = await LayerFS.init("shop", "/work/shop");   // once per project
try {
  console.log(layerfs.created.kind);
  const branches = await layerfs.project.list_branches();
  console.log(branches.items.map((b) => b.name));
} finally {
  await layerfs.close();
}
```

```
Initialized
[ 'main' ]
```

### branch.mjs: make your own Branch

```js
// branch.mjs
import { LayerFS } from "@layerfs/agent";

const layerfs = await LayerFS.open("shop");
try {
  const forked = await layerfs.branch.fork("work");   // from main
  console.log(forked.kind, forked.branch, forked.head); // head: null until the first Commit
  const branches = await layerfs.project.list_branches();
  console.log(branches.items.map((b) => b.name));
} finally {
  await layerfs.close();
}
```

```
Forked work null
[ 'main', 'work' ]
```

### fs.mjs: write and read files on the Branch

```js
// fs.mjs
import { LayerFS } from "@layerfs/agent";

const layerfs = await LayerFS.open("shop");
try {
  await layerfs.fs.write("work", "/hello.js", "console.log('hello');\n", "add hello.js");
  await layerfs.fs.edit("work", "/hello.js", "'hello'", "'hello, shop'", "greet the shop");
  const file = await layerfs.fs.read("work", "/hello.js");
  console.log(file.text);
  const listing = await layerfs.fs.list("work", "/");
  console.log(listing.items.map((e) => `${e.type} ${e.name}`));
} finally {
  await layerfs.close();
}
```

```
console.log('hello, shop');

[ 'dir .git', 'file .gitignore', 'file .lww', 'file README.md', 'file hello.js' ]
```

### workspace.mjs: run a command, commit, publish

```js
// workspace.mjs
import { LayerFS } from "@layerfs/agent";

const layerfs = await LayerFS.open("shop");
try {
  const { sandbox } = await layerfs.sandbox.create();   // or one from project.list_sandboxes() with room
  const ws = await layerfs.workspace.mount(sandbox, "work");   // { sandbox: "1", path: "/workspaces/1" }: never hard-code it
  try {
    const run = await layerfs.workspace.exec(ws, "node hello.js | tee hello.txt", "run hello");
    console.log(run.exitCode, run.stdout);
    const saved = await layerfs.workspace.commit(ws, "save hello.txt");
    console.log(saved.kind);
  } finally {
    await layerfs.workspace.unmount(ws);
  }
  const published = await layerfs.branch.publish("work");
  console.log(published.kind);
} finally {
  await layerfs.close();
}
```

```
0 hello, shop

Committed
Added
```

## Rules

1. **Read before you retry.** A refused call throws a `LayerFSError` with a stable
   `code`, a `message` and a `detail`; `isRefusal(error, "Dirty")` tests for one code.
   Look at the project, then decide; never resend the same call blindly. On `Unknown`
   or `Failed` the outcome is uncertain: stop, check with `branch.list_commits` and
   `fs.read`, and report. A mount refused `Failed` with `detail.removed` started a
   Sandbox that failed and left nothing behind; report its message, which carries the
   cause.
2. **A non-zero exit is not a refusal.** `workspace.exec` returns normally; check
   `exitCode`.
3. **Keep each command under 2 minutes.** `workspace.exec` has no timeout: wrap long
   commands inside it, `timeout 110 npm test`.
4. **A Workspace shows its Branch as it was at mount.** After `fs.*` changes, mount
   again before you build or test.
5. **Commit every step that changes files, with a message naming the step and its
   result**, failures included; publish to `main` at milestones.
6. **Workspaces are reusable across processes.** A `{ sandbox, path }` works from any
   later process with the same `LAYERFS_HOME`, so a long task can be split into small
   scripts. Keep each Workspace's operations sequential.
7. **Clean up:** commit or discard, then unmount every Workspace. Its Sandbox closes
   itself two minutes after its last Workspace is gone; `sandbox.close` a Sandbox you
   created. With `LAYERFS_TRACE` set, save a Sandbox's daemon log while it runs with
   `backend.daemon.sandbox.get_log`, as described in
   [daemon logs](references/methods/workspace.md#daemon-logs); its closing deletes it.

## Which method for which job

| Job | Use |
| --- | --- |
| Create or change one file | `fs.write`, or `fs.edit` (exactly one match of `oldText`) |
| Create or change a few related files at once | `fs.apply(branch, [{op:"write",…}, …], message)`, one Commit |
| Change many files or a whole tree | A command in a Workspace (`sed`, a generator, a formatter), then `workspace.commit` |
| Create folders, copy, move, delete | `fs.mkdir`, `fs.cp`, `fs.mv`, `fs.rm` (recursive) |
| Install, build, test, run anything | `workspace.mount`, then `workspace.exec`, then `workspace.commit` |
| Look at files | `fs.read` (line ranges), `fs.list`, `fs.stat`, `fs.grep`, `fs.glob` |
| Compare | `fs.diff_files`, `branch.diff_commits`, `branch.diff_branches` |
| History | `branch.list_commits`, `fs.blame` |
| Check a Workspace | `backend.daemon.workspace.status` from `@layerfs/backend` (base, whether the head moved, overlay counts; see [resolving conflicts](references/rules/conflicts.md), Scenario 5) |
| Sandboxes | `sandbox.create`, `project.list_sandboxes`, `workspace.mount(sandbox, branch)`, `sandbox.close` |
| Combine Branches | `branch.merge(a, b, name)`, then `branch.publish(name)` |
| Resolve a `Conflict` | [resolving conflicts](references/rules/conflicts.md) |

For Rust projects, follow the live-checked [Cargo workflow](references/rules/cargo.md).

## Methods

Copied from [`api/docs/agent.md`](../../../api/docs/agent.md#methods), which is the source of truth.
Every method is positional and returns a Promise. Each group's page has, for every
method, its arguments, result fields, refusals with what to do, limits and examples.

| Group | Methods |
| --- | --- |
| [project](references/methods/project.md) | `LayerFS.init(name, sourcePath, timeout?)`, `LayerFS.open(name)`, `close()`, `list_branches(pageSize?, pagePos?)`, `list_sandboxes(pageSize?, pagePos?)` |
| [fs reads](references/methods/fs-read.md) | `read(ref, path, startLine?, endLine?)`, `list(ref, path?, pageSize?, pagePos?)`, `stat(ref, path)`, `grep(ref, pattern, path?, pageSize?, pagePos?)`, `glob(ref, pattern, path?, pageSize?, pagePos?)`, `diff_files(leftRef, leftPath, rightRef, rightPath)`, `blame(ref, path, fromLine?)` |
| [fs writes](references/methods/fs-write.md) | `write(branch, path, content, message?)`, `edit(branch, path, oldText, newText, message?)`, `apply(branch, changes, message?)`, `mkdir(branch, path, message?)`, `rm(branch, path, message?)`, `cp(branch, from, to, message?)`, `mv(branch, from, to, message?)` |
| [branch](references/methods/branch.md) | `fork(name, fromRef?)`, `list_commits(branch, pageSize?, pagePos?)`, `diff_commits(from, to, pageSize?, pagePos?)`, `diff_branches(from, to, pageSize?, pagePos?)`, `merge(a, b, name, message?)`, `publish(branch, commit?)` |
| [sandbox](references/methods/sandbox.md) | `create(options?)`, `list_workspaces(sandbox, pageSize?, pagePos?)`, `close(sandbox)` |
| [workspace](references/methods/workspace.md) | `mount(sandbox, branch)` → `{ sandbox, path, image }`, `exec(ws, command, message?)`, `commit(ws, message?, force?)`, `unmount(ws, discard?, force?)` |

A `ref` is `main`, a Branch name, a Commit id or a Layer id. Paths are absolute in the
view (`/src/index.js`). Pages are `{ items, hasMore }`; advance with
`pagePos + items.length`.

Deeper material: the [SDK guide](../../../api/docs/agent.md), the
[`layerfs` command](../../../api/docs/agent.md#the-layerfs-command) and the
[0.1.7 SDK and UI guide](../../../docs/versioned/0.1.7/sdk-and-ui.md).
