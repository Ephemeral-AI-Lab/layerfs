---
name: layerfs-js-sdk
description: Use when building, editing, testing or inspecting a project through LayerFS 2.0 with the `layerfs` package (`import layerfs from "layerfs"`), its Python twin or the `layerfs` command. Activates on LayerFS projects (`<store>/<project>`), Branches, Commits, Workspaces or Sandboxes; `layerfs.file.*`, `layerfs.branch.*`, `layerfs.commit.*`, `layerfs.sandbox.*`, `layerfs.workspace.*` or `layerfs.project.*` calls; `layerfs file read …`-style commands; running npm install, build or tests inside a mounted Workspace; or resolving LayerFS Conflict, HeadMoved, Dirty or ReadOnly refusals.
---

# LayerFS: working on a project

> **Status:** Current general guide for LayerFS 2.0 (#533, branch `issue533-v2`).
> It replaces the 1.1.0 guide for `@layerfs/agent`, `@layerfs/cli`,
> `@layerfs/runtime` and `@layerfs/backend`, which are deleted. Sources: the
> call table (one file per call, `api/crates/layerfs-api/src/<family>/<call>.rs`), the package
> (`api/node/src/index.js`, [api/README.md](../../../api/README.md)), the
> command (`api/crates/layerfs-cli/src`) and
> [the decisions log](../../../core/docs/issues/533/DECISIONS.md). File,
> Branch, Commit, project and Store results below were captured on macOS from
> the built `layerfs` command and the Node package in a private home on
> 2026-11-02. Sandbox and Workspace calls were **not run** in this pass; their
> shapes come from source.

Your project lives in LayerFS, not on your local disk. You read and change it
through LayerFS calls: from a Node script, from Python, or one call at a time with
the `layerfs` command. All three make the same call into the same library, so
arguments, results and refusals are the same.

## Inside an integration: start here

An integration is a host that runs you with a LayerFS project already set up. Its
instructions name the project (`<store>/<project>`, such as `shop/api`), your
Branch, and how to reach the package or the command.

- **One setting:** `LAYERFS_HOME`, the home folder that holds every record
  (default `~/.layerfs`). Nothing else is needed. 1.1.0's other `LAYERFS_*`
  settings are gone.
- **The integration may give you a folder scope** (`--scope <folder>
  --scope-project <store>/<project>`, or `client({ scope })`). Then you work
  in that one project, your paths must stay inside that folder, your Branch
  names are relative to a namespace, and only file, Branch and Workspace
  calls are allowed. See [folder scope](#folder-scope).
- Don't look into Docker, Store files, the home's records or the integration's
  source. The calls are the only way in.
- Your current folder is for scratch files only: your scripts and notes. Files
  you find there may be an old copy, not the project.
- **`main` is read-only.** Read it and fork from it. Writes to it are refused
  `ReadOnly`; `commit.list` on it is refused `NoSuchBranch` (it is a chain of
  Layers, not a Branch). Fork a Branch, work there, then `branch.publish` it.
- **Always unmount.** Put every `workspace.unmount` in a `finally` block. A
  forgotten Workspace stays mounted and its Sandbox never closes.

## How an agent works in LayerFS

Every tool call an agent makes is one **step**. Every step that changes files
becomes a Commit on the agent's Branch, so the Branch's history is the record of
what the agent did.

| Agent tool | LayerFS calls | Recorded as |
| --- | --- | --- |
| File read: read, list, search, diff | `file.read`, `file.list`, `file.grep`, `file.glob`, `file.diff` | Nothing: reads don't change history |
| File change: write, edit, many-file change | `file.write`, `file.edit`, `file.apply`, `file.mkdir`, `file.mv`, `file.cp`, `file.rm` | One Commit per call. All or nothing |
| Bash | A Workspace per call: `workspace.mount` → `workspace.exec` → `workspace.commit` → `workspace.unmount` | One Commit with everything the command changed |

**`file.*` for cheap, targeted file work; a Workspace for Bash commands.**

### Cheap file work: `file.*`

- A `file` call runs on the project's Store, in your process or in the worker
  of the machine that holds the Store. No Workspace, no mount.
- A write is atomic and is exactly one Commit with your message. Without a
  message the Commit says `Apply filesystem changes`.
- A read sees the Branch's latest Commit: what every Workspace and writer has
  committed, never a mounted Workspace's uncommitted changes.
- **No merge:** if another writer committed on the Branch during the call, it is
  refused `HeadMoved` and nothing is written. Read again, then redo the change.

### Bash commands: a Workspace

Use a Workspace for commands, which usually change files too: `npm install`
(`node_modules`, the lockfile), builds, generators, formatters, `git`, tests
that write snapshots. Also use one for large changes across many files, and to
reach a Workspace's uncommitted state, such as conflict markers.

- **A Workspace per Bash call is the default.** Mounting does not copy or scan
  the tree. Each mount is a new Workspace, named `<sandbox>:<path>`, such as
  `tests:/workspaces/1`. Pass that `ws` string to `exec`, `commit`, `unmount`
  and `status`.
- **You name the Sandbox.** A Sandbox is created once with
  `sandbox.create(project, name, machine?)`, holds up to 16 Workspaces, and runs
  on the machine that holds the project's Store. `workspace.mount(sandbox,
  branch)` never starts one. See [sandbox](references/methods/sandbox.md).
- **Inside a command, use relative paths.** `exec` starts in the Workspace
  root. The mount path changes with every mount.
- **Commit after every Bash call, even a failing one,** with a message naming
  the step and its result, such as `"step 12: npm test (exit 1)"`.
- **A step that changes nothing makes no Commit.** `workspace.commit` returns
  `kind: "UpToDate"`.

## Merge rules: `/.lww`

Workspaces and Branches meet in a merge: in `workspace.commit` when the Branch
moved, and in `branch.merge`. Source files merge line by line; overlaps become a
`Conflict` you resolve by reading. Generated state (`node_modules`, lockfiles,
build output) can't merge that way, so list it in `/.lww` at the project root:

```
%include .gitignore
```

Both merges then keep the newer version of those paths whole and report each
decision in the result's `lww` list. Never list source files. Details:
[merge rules](references/rules/lww.md) and
[resolving conflicts](references/rules/conflicts.md).

## The pieces

| Piece | What it is | Family |
| --- | --- | --- |
| Home | `LAYERFS_HOME`: the records of every machine, Store, project and Sandbox | — |
| Store | One database holding projects and all their history: a sqlite file on one machine, or a PostgreSQL database | `store` |
| Project | `<store>/<project>`, imported once from a host folder | `project` |
| Branch | A named line of Commits. `main` changes only through `branch.publish` | `branch` |
| Commit | One saved change on a Branch, named by a 66-hex id | `commit` |
| Files | Read and change files on a Branch directly | `file` |
| Sandbox | Named, bound to one project, running on one machine; holds up to 16 Workspaces | `sandbox` |
| Workspace | A Branch mounted in a Sandbox, `<sandbox>:<path>`; commands run there | `workspace` |
| Machine | Where Sandboxes run: `native`, `docker`, VMs, ssh hosts | `machine` |
| Backup | A copy of a sqlite Store in an r2 or MinIO bucket | `backup` |

## The three front ends

```js
// Node 22+, ES module
import layerfs, { LayerFSError } from "layerfs";
const text = (await layerfs.file.read("shop/api", "work", "/README.md")).text;
```

```python
import layerfs
text = layerfs.file.read("shop/api", "work", "/README.md")["text"]
```

```bash
layerfs file read shop/api work /README.md --text
```

| Shell | Node | Python |
| --- | --- | --- |
| `layerfs <family> <call> <a> <b> [<c>] --opt <v> --flag` | `family.call(a, b, c?, { opt: v, flag: true })` | `family.call(a, b, c=None, opt=v, flag=True)` |
| `-m <text>` | `{ message }` | `message=` |
| `--start-line 10` | `{ start_line: 10 }` | `start_line=10` |
| one line of JSON on stdout (`--text`: readable) | a plain object | a dict |
| a refusal: JSON on stderr, exit 125 | throws `LayerFSError { code, message, detail }` | raises `layerfs.LayerFSError` (`.code`, `.message`, `.detail`) |

- **Positionals go from the outside in:** project, Branch, path, other inputs,
  and an optional Commit last. Options say how.
- **Every call takes its project.** There is no client to open or close and no
  default project. `layerfs.client({ home, scope })` returns the same nine
  families bound to another home or a folder scope.
- **Results use snake_case keys** in every front end: `exit_code`,
  `has_more`, `next_page_pos`. Bytes are `Buffer` in Node, `bytes` in Python.
  (Exception seen live: a refusal's `detail` keeps the library's camelCase
  keys, such as `mergeBase` and `hunksOmitted` in a `Conflict`.)
- **Node calls return promises.** Python calls are synchronous; `layerfs.aio`
  has the same calls as coroutines.
- **Shell exit codes:** 0 done; 2 the command line is wrong and nothing was
  called; 125 LayerFS refused or failed. `workspace exec --stream` exits with
  the command's own code.
- An unknown option or a missing argument is refused `InvalidInput` before any
  effect (Node: ``file.read: `branch` is required``).
- `layerfs help`, `layerfs help <family>` and `layerfs help <family> <call>`
  print the table. `layerfs.table` holds it in code.

In this worktree the package is `api/node` (import `api/node/src/index.js` by
path until it is installed as `layerfs`; it needs the addon built with
`npm --prefix api run build:native`). The command is `target/debug/layerfs`
after `cargo +1.85.1 build -p layerfs-cli`.

## One script: fork, edit, test, publish

```js
// step.mjs: run with `node step.mjs`
import layerfs from "layerfs";

const P = "shop/api";
await layerfs.branch.fork(P, "work", "main");           // { kind: "Forked", branch, base, head: null }
await layerfs.file.write(P, "work", "/hello.js", "console.log('hello');\n", { message: "add hello.js" });
await layerfs.file.edit(P, "work", "/hello.js", { old: "'hello'", new: "'hello, shop'", message: "greet the shop" });
console.log((await layerfs.file.read(P, "work", "/hello.js")).text);

// a Bash step; the Sandbox "tests" was made once with sandbox.create(P, "tests", <machine>)
const mounted = await layerfs.workspace.mount("tests", "work");
try {
  const run = await layerfs.workspace.exec(mounted.ws, "timeout 110 node hello.js | tee hello.txt", { message: "run hello" });
  await layerfs.workspace.commit(mounted.ws, { message: `step 3: node hello.js (exit ${run.exit_code})` });
} finally {
  await layerfs.workspace.unmount(mounted.ws);
}
await layerfs.branch.publish(P, "work");                 // { kind: "Added", published_layer, … }
```

The same file steps in the shell (content comes from stdin or `--from <file>`):

```bash
layerfs branch fork shop/api work main
printf "console.log('hello');\n" | layerfs file write shop/api work /hello.js -m "add hello.js"
layerfs file edit shop/api work /hello.js --old "'hello'" --new "'hello, shop'" -m "greet the shop"
layerfs file read shop/api work /hello.js --text
```

## Rules

1. **Read before you retry.** Every call is one attempt; nothing is retried for
   you. On a refusal, look at the project, then decide. On `Unknown` or
   `Failed` the outcome may be uncertain: check with `commit.list` and
   `file.read`, and report.
2. **A non-zero exit is not a refusal.** `workspace.exec` returns normally;
   check `exit_code`.
3. **Keep each command under 2 minutes.** `workspace.exec` has no timeout: wrap
   long commands, `timeout 110 npm test`.
4. **A Workspace shows its Branch as it was at mount.** After `file.*` changes,
   mount again before you build or test.
5. **Commit every step that changes files**, failures included; publish to
   `main` at milestones.
6. **Workspaces outlive your process.** The `ws` string works from any later
   process with the same `LAYERFS_HOME`. Keep each Workspace's calls sequential.
7. **Clean up:** commit or discard, then unmount every Workspace;
   `sandbox.close` a Sandbox you created when you are done with it.

## Folder scope

A scope `{ project, folder, branch? }` binds a client to one project, one
folder of it and one Branch namespace. It guards calls; it is not isolation (a
command in a Workspace can still `cd` anywhere).

```js
const lfs = layerfs.client({ scope: { project: "shop/api", folder: "/web", branch: "agent" } });
await lfs.file.read("shop/api", "main", "/web/index.html");   // reads Branch agent.main
```

```bash
layerfs file read shop/api main /web/index.html --scope /web --scope-project shop/api --scope-branch agent
```

`--scope` without `--scope-project` is a usage error (exit 2).

Checked live (before the scope named its project; the rules below still hold):

- Paths must be canonical and inside `folder`; anything else is refused
  `InvalidInput` (`out of scope: path`).
- Branch names become `<branch>.<name>`: `main` means `agent.main`. A Commit id
  passes through unchanged. `branch.list` shows only the namespace's Branches,
  named without the prefix.
- `file` calls need `scope.branch`; without it they are refused
  (`out of scope: branch`).
- `commit.*` and `branch.diff` are refused (`out of scope: history`).
- The namespace's `main` must exist first. Fork it without a scope:
  `branch.fork(project, "agent.main", "main")`.
- From source, not run: `workspace.mount` under a scope accepts only `main`
  (mounting `agent.main`), and `workspace.exec` starts the command in the
  folder.
- From source (`797ecca2c7`): every `file` and `branch` call must name the
  scope's project (`out of scope: project`), and a `workspace` call's Sandbox
  must be one of that project's (`out of scope: sandbox`). Only the `file`
  calls, `branch` list/get/fork/merge/publish and the `workspace` calls are
  allowed; every other call (Stores, machines, backups, projects, Sandboxes)
  is refused `out of scope: <call>`, such as `out of scope: store.remove`,
  before any effect. Create the Sandbox and fork the namespace's `main`
  without a scope.

## Which call for which job

| Job | Use |
| --- | --- |
| Create or change one file | `file.write`, or `file.edit` with `{ old, new }` (exactly one match) |
| Several related changes as one Commit | `file.apply(project, branch, changes)` |
| Folders, copy, move, delete | `file.mkdir`, `file.cp`, `file.mv`, `file.rm` (recursive) |
| Install, build, test, run anything | `workspace.mount`, `exec`, `commit`, `unmount` |
| Look at files | `file.read` (line ranges), `file.list`, `file.stat`, `file.grep` (plain text), `file.glob` |
| Compare | `file.diff`, `commit.diff`, `branch.diff` |
| History | `commit.list`, `commit.get`, `file.blame` |
| Check a Workspace before Commit | `workspace.status` (base, whether the head moved, uncommitted counts) |
| Sandboxes | `sandbox.create`, `sandbox.list`, `sandbox.get`, `sandbox.close` |
| Combine Branches | `branch.merge(project, name, a, b)`, then `branch.publish(project, name)` |
| Resolve a `Conflict` | [resolving conflicts](references/rules/conflicts.md) |

For Rust projects, follow the [Cargo workflow](references/rules/cargo.md).

## Calls

61 calls in nine families. Each family's page has arguments, results, refusals
and examples.

| Family | Calls |
| --- | --- |
| [project](references/methods/project.md) | `init(project, folder, { timeout, on_progress })`, `list(store?, { page_size, page_pos })`, `get(project)`, `remove(project)` |
| [branch](references/methods/branch.md) | `list(project)`, `get(project, branch)`, `fork(project, name, from_branch, from_commit?)`, `merge(project, name, a, b, { message, units })`, `publish(project, branch, commit?)`, `diff(project, from_branch, to_branch, { paths_only, page_size, page_pos, cursor })` |
| [commit](references/methods/commit.md) | `list(project, branch)`, `get(project, branch, commit)`, `diff(project, from_commit, to_commit, { paths_only, … })` |
| [file](references/methods/file.md) | reads `read`, `stat`, `list`, `glob`, `grep`, `blame`, `diff`; writes `write`, `edit`, `rm`, `mkdir`, `mv`, `cp`, `apply` |
| [sandbox](references/methods/sandbox.md) | `create(project, name, machine?, { image })`, `list(project?, { page_size, page_pos })`, `get(name, { page_size, page_pos })`, `close(name, { force })` |
| [workspace](references/methods/workspace.md) | `mount(sandbox, branch, { name })`, `exec(ws, command, { message, stream, stdout, stderr })`, `commit(ws, { message, force })`, `unmount(ws, { discard, force })`, `status(ws)` |
| [machine](references/methods/machine.md) | `list`, `platforms`, `get`, `status`, `start`, `stop`, `create`, `connect`, `remove`, `disconnect`, `update` |
| [store](references/methods/store.md) | `list`, `get`, `status`, `create`, `connect`, `push`, `pull`, `remove`, `disconnect` |
| [backup](references/methods/backup.md) | `connect`, `disconnect`, `list`, `create`, `restore` |

One top-level function is outside the families: `doctor({ home })` (shell
`layerfs doctor [--text]`) reports what is wrong in the home as `{ ok,
findings: [{ severity, entity: { kind, name }, message, fix }] }`, each
finding naming the call that fixes it. It changes nothing and starts no
machine.

`machine`, `store` and `backup` set up where things live; an agent working on a
project rarely needs them. Reading state and tracing are the
[`layerfs-sdk-observe`](../layerfs-sdk-observe/SKILL.md) skill.

Paths are absolute in the project (`/src/index.js`). Pages are
`{ items, has_more, next_page_pos }`; pass `page_pos: next_page_pos` for the
next one.

## From 1.1.0

| 1.1.0 | 2.0 |
| --- | --- |
| `LayerFS.open("shop")`, then calls on the client, `close()` | no client: every call takes `<store>/<project>` |
| `LayerFS.init(name, folder)` | `store.create(store)` once, then `project.init("<store>/<project>", folder)` |
| `fs.*(ref, path, …)` | `file.*(project, branch, path, commit?, …)` |
| `fs.edit(branch, path, oldText, newText, message)` | `file.edit(project, branch, path, { old, new, message })` |
| `fs.diff_files(lRef, lPath, rRef, rPath)` | `file.diff(project, left_branch, left_path, right_branch, right_path, { left_commit, right_commit })` |
| `project.list_branches()` | `branch.list(project)` |
| `branch.fork(name, fromRef?)` | `branch.fork(project, name, from_branch, from_commit?)` |
| `branch.merge(a, b, name)` | `branch.merge(project, name, a, b)` |
| `branch.list_commits`, `diff_commits`, `diff_branches` | `commit.list`, `commit.diff`, `branch.diff` |
| `sandbox.create()` → a number | `sandbox.create(project, name, machine?)` |
| `project.list_sandboxes()`, `sandbox.list_workspaces(n)` | `sandbox.list(project?)`, `sandbox.get(name)` |
| `{ sandbox, path }` Workspace object | `ws` string `<sandbox>:<path>` |
| positional `message`, `force`, `discard` | options `{ message }`, `{ force }`, `{ discard }` |
| `exitCode`, `hasMore`, `startLine` | `exit_code`, `has_more`, `start_line` |
| `backend.daemon.workspace.status(ws)` | `workspace.status(ws)` |
| `LAYERFS_DOCKER`, `LAYERFS_IMAGE`, `LAYERFS_STORE_PATH`, … | records in the home; run `layerfs import-env` once to convert |
