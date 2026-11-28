---
name: layerfs-js-sdk
description: Use when building, editing, testing or inspecting a project through LayerFS 2.0 with the `@layerfs/node` package (`import layerfs from "@layerfs/node"`), its Python twin or the `layerfs` command. Activates on LayerFS projects (`<store>/<project>`), Branches, Commits, Workspaces or Sandboxes; `layerfs.file.*`, `layerfs.branch.*`, `layerfs.commit.*`, `layerfs.sandbox.*`, `layerfs.workspace.*` or `layerfs.project.*` calls such as `layerfs.file.edit`, `layerfs.commit.diff`, `layerfs.sandbox.create`, `layerfs.workspace.mount`, `layerfs.workspace.exec`, `layerfs.workspace.status`, `layerfs.workspace.commit` or `layerfs.branch.publish`; `layerfs file read …`-style commands; running npm install, build or tests inside a mounted Workspace; or resolving LayerFS `Conflict`, `Dirty`, `InUse`, `Busy` or `InvalidInput` refusals (`LayerFSError`).
---

# LayerFS: working on a project

> **Status:** Current general guide for the LayerFS 2.0 call surface: 83
> calls in 10 families. Sources: the built call table (`layerfs help --json`;
> one file per call, `api/crates/layerfs-api/src/<family>/<call>.rs`), the
> frozen spec rows
> ([calls.json](../../../core/docs/issues/call-surface-2-0/calls.json),
> [decisions](../../../core/docs/issues/call-surface-2-0/DECISIONS.md)), the
> package (`api/node/src/index.js`) and the manual's
> [calls page](../../../docs/versioned/2.0.0/calls.md). Outputs below were
> printed by the built `layerfs` command (2.0.0) on a throwaway home on
> macOS (`native` machine) on 2026-11-18; ids are shortened. The Node
> package makes the same call into the same library and answers the same
> objects; the Node snippets themselves were not run in that pass.

Your project lives in LayerFS, not on your local disk. You read and change it
through LayerFS calls: from a Node script, from Python, or one call at a time with
the `layerfs` command. All three make the same call into the same library, so
arguments, answers and refusals are the same.

## Inside an integration: start here

An integration is a host that runs you with a LayerFS project already set up. Its
instructions name the project (`<store>/<project>`, such as `shop/api`), your
Branch, your Sandbox and how to reach the package or the command.

- **One setting:** `LAYERFS_ROOT`, the LayerFS root (default `~/.layerfs`)
  whose home, `<root>/home` (or the root while it holds `layerfs.json`),
  holds every record. Nothing else is needed.
- **The integration may give you a folder scope** (`--scope <folder>
  --scope-project <store>/<project>`, or `client({ scope })`). Then you work
  in that one project, your paths must stay inside that folder, your Branch
  names are relative to a namespace, and only file, Branch and Workspace
  calls are allowed. See [folder scope](#folder-scope).
- Don't look into Docker, Store files, the home's records or the integration's
  source. The calls are the only way in.
- Your current folder is for scratch files only: your scripts and notes. Files
  you find there may be an old copy, not the project.
- **`main` is read-only.** Read it and fork from it. A write to it, a merge
  into it (without `into`) and a mount of it are refused `InvalidInput`; the
  mount's refusal gives a runnable `fix`
  (`"layerfs branch fork shop/api work && layerfs workspace mount <sandbox> work"`).
  Work on a Branch, then `branch.publish` it.
- **Always unmount.** Put every `workspace.unmount` in a `finally` block. A
  forgotten Workspace stays mounted and its Sandbox cannot be removed.

## How an agent works in LayerFS

Every tool call an agent makes is one **step**. Every step that changes files
becomes a Commit on the agent's Branch, so the Branch's history is the record of
what the agent did.

| Agent tool | LayerFS calls | Recorded as |
| --- | --- | --- |
| File read: read, list, search, history | `file.read`, `file.list`, `file.stat`, `file.grep`, `file.glob`, `file.history`, `file.blame`, `commit.diff` | Nothing: reads don't change history |
| File change: write, edit, many-file change | `file.write`, `file.edit`, `file.apply`, `file.mkdir`, `file.mv`, `file.cp`, `file.rm` | One Commit per call. All or nothing |
| Bash | A Workspace: `workspace.mount` → `workspace.exec` → `workspace.commit` → `workspace.unmount` | One Commit with everything the command changed |

**`file.*` for cheap, targeted file work; a Workspace for Bash commands.**

### Cheap file work: `file.*`

- A `file` call runs on the project's Store, in your process or in the worker
  of the machine that holds the Store. No Workspace of yours, no mount.
- A write is atomic and is exactly one Commit with your message. Without a
  message the Commit says `Apply filesystem changes`.
- A read sees the Branch's latest Commit (or `at`, one Commit of its
  history): what every Workspace and writer has committed, never a mounted
  Workspace's uncommitted changes.
- **A moved Branch merges:** if another writer committed on the Branch during
  the write, the change is merged with the new head and the answer says
  `merged: true`; a path both changed is a `Conflict` and nothing is written.
  Pass `expected: <commit>` to refuse `Conflict` instead unless the head is
  still the one you read.

### Bash commands: a Workspace

Use a Workspace for commands, which usually change files too: `npm install`
(`node_modules`, the lockfile), builds, generators, formatters, `git`, tests
that write snapshots. Also use one for large changes across many files, and to
reach a Workspace's uncommitted state, such as conflict markers.

- **You name the Sandbox.** A Sandbox is created once with
  `sandbox.create(project, name, { machine })`, holds up to 16 Workspaces,
  and runs on the machine that holds the project's sqlite Store (else
  `native`). `workspace.mount(sandbox, branch)` never starts one. See
  [sandbox](references/methods/sandbox.md).
- **A Workspace per Bash call is the default.** Mounting does not copy or
  scan the tree. A Workspace is named `<sandbox>:<name>`, such as
  `tests:agent`: `mount` answers it as `workspace` (the name defaults to the
  Branch's, made valid; `{ name }` picks another). Pass that handle to
  `exec`, `commit`, `unmount`, `status` and `cancel`.
- **Inside a command, use relative paths.** `exec` starts in the Workspace
  root (or `cwd`, a folder of the Workspace). The mount path changes with
  every mount.
- **Commit after every Bash call, even a failing one,** with a message naming
  the step and its result, such as `"step 12: npm test (exit 1)"`.
- **A step that changes nothing makes no Commit.** `workspace.commit`
  answers `{ commit, unchanged: true }`, `commit` being the head.

## Merge rules: `/.lww`

Changes meet in a merge: in `workspace.commit` and the `file.*` writes when
the Branch moved, and in `branch.merge`. Source files merge line by line;
overlaps become a `Conflict` you resolve by reading. Generated state
(`node_modules`, lockfiles, build output) can't merge that way, so list it in
`/.lww` at the project root:

```
%include .gitignore
```

Merges then keep the newer version of those paths whole and name each such
path in the answer's `overwritten` list (the first 20; `overwritten_count`
only when it was cut). Never list source files. Details:
[merge rules](references/rules/lww.md) and
[resolving conflicts](references/rules/conflicts.md).

## The pieces

| Piece | What it is | Family |
| --- | --- | --- |
| Home | the home in `LAYERFS_ROOT`: the records of every machine, Store, project, Sandbox and bucket, and the call log | `home` |
| Store | One database holding projects and all their history: a sqlite file on one machine, or a PostgreSQL database | `store` |
| Project | `<store>/<project>`, imported once from a host folder | `project` |
| Branch | A named line of Commits. `main` changes only through `branch.publish` | `branch` |
| Commit | One saved change on a Branch, named by a 66-hex id (a prefix of 8 or more is accepted where a call takes a Commit) | `commit` |
| Files | Read and change files on a Branch directly | `file` |
| Sandbox | Named, bound to one project, running on one machine; holds up to 16 Workspaces | `sandbox` |
| Workspace | A Branch mounted in a Sandbox, `<sandbox>:<name>`; commands run there | `workspace` |
| Machine | Where Sandboxes run: `native`, `docker`, VMs (`apple-container`, `lima`) | `machine` |
| Bucket | An r2 or S3 bucket holding backups of sqlite Stores | `backup` |

## The three front ends

```js
// Node 22+, ES module
import layerfs, { LayerFSError } from "@layerfs/node";
const { text } = await layerfs.file.read("shop/api", "agent", "/README.md");
```

```python
import layerfs
text = layerfs.file.read("shop/api", "agent", "/README.md")["text"]
```

```bash
layerfs file read shop/api agent /README.md --text
```

| Shell | Node | Python |
| --- | --- | --- |
| `layerfs <family> <call> <a> <b> [<c>] --opt <v> --flag` | `family.call(a, b, c?, { opt: v, flag: true })` | `family.call(a, b, c=None, opt=v, flag=True)` |
| `-m <text>` | `{ message }` | `message=` |
| `--old-string S` | `{ old_string: "S" }` | `old_string="S"` |
| `--detail` (long form) | `{ detail: true }` | `detail=True` |
| `--with live,stats` | `{ with: ["live", "stats"] }` | `with_=["live", "stats"]` |
| `--progress` | `{ on_progress: (e) => … }` | `on_progress=fn` |
| one line of JSON on stdout (`--text`: readable) | a plain object | a dict |
| a refusal: JSON on stderr, exit 125 | throws `LayerFSError` | raises `layerfs.LayerFSError` |

- **Positionals go from the outside in:** project, Branch, path, other
  inputs. Everything optional is an option, except a few optional trailing
  positionals (`branch.fork`'s `from`, `file.glob`/`file.grep`'s `path`,
  `sandbox.files`/`machine.files`'s `path`, `machine.prune`'s `image`).
- **Every call takes its project** (or its Sandbox, Workspace, machine,
  Store). There is no client to open and no default project.
  `layerfs.client({ root, scope })` returns the same ten families bound to
  another LayerFS root or a folder scope. `layerfs.close()` releases the Stores this
  process holds open; call it before a script exits.
- **Node calls return promises.** Python calls are synchronous.
- **Answers are snake_case** in every front end. Bytes are `Buffer` in Node,
  `bytes` in Python.
- An unknown option or a wrong argument count is a usage error: exit 2 in the
  shell (seen: `layerfs: file.read takes 3 positional arguments, not 1`),
  `InvalidInput` in Node and Python, before any effect.
- `layerfs help`, `layerfs help <family>`, `layerfs help <family> <call>` and
  `layerfs help --json` print the table. `layerfs.table` holds it in code.

In this worktree the package is `api/node` (import `api/node/src/index.js` by
path until it is installed; it needs the addon built with
`npm --prefix api run build:native`). The command is `target/debug/layerfs`
after `cargo +1.85.1 build -p layerfs-cli`.

### The form of an answer

- **Short form by default:** the few keys an agent needs; a key is left out
  when it is null, false or empty. A call that changed something and has
  nothing to say answers `{"ok":true}`. **Long form** with `{ detail: true }`:
  every key the call declares, in a fixed order, null where there is no value.
  Seen: `file stat` → `{"type":"file","size":8,"mode":"0644"}`; with
  `--detail` it adds `commit`, `root`, `path`, `inode`, `modified_at`.
- **Sections:** `with: [...]` adds a named part that costs a read of its own.
  A call takes only the sections its row lists (`live`, `stats`, `space`,
  `docker`, `platforms`, `tree`, `patch`); another is refused (seen:
  `home.get: --with names \`live\`; this call takes platforms, tree`).
- **Lists:** every list takes `{ limit, cursor }` and answers
  `{ items, next_cursor }`. `next_cursor` is left out on the last page; pass
  it back as `cursor`, with the same filters. Defaults: 100 rows for lists,
  200 lines for logs, 2000 lines for text windows; at most 1000; a page also
  ends early past 64 KiB, with a cursor.
- **Counts:** a list inside an answer (`overwritten`, `conflicts`,
  `changes`, `projects`) holds its first 20 entries; `<list>_count` is set
  only when it was cut.
- **States:** a thing's `state` is `ready`, `busy` (with
  `op { name, phase, started_at }`), `stopped`, `blocked` (with
  `blocked_by { <machine|store|sandbox>, reason }`), `broken` (with `reason`
  and `fix`, one shell line) or `unknown` (with `observed_at`). `ready` is
  left out of most short forms.
- **Refusals:** `{ error, message }`, `fix` when there is one, and the fields
  the next call needs, flat beside them (a `Conflict` names the head as
  `commit`; a `Dirty` lists `changes`). `error` is one of thirteen codes:
  `NotFound`, `Exists`, `InUse`, `Busy`, `Unresolved`, `Conflict`, `Dirty`,
  `Capacity`, `InvalidInput`, `Unsupported`, `Unavailable`, `Failed`,
  `Unknown`. With `detail: true` a refusal also has `fix` (null when none)
  and `detail`, which holds the raised code as `detail.code` (seen:
  `{"error":"NotFound","message":"no Store named nope","fix":null,"detail":{"code":"NoSuchStore"}}`).
- **Exit codes:** 0 done; 2 the command line is wrong and nothing was
  called; 125 refused or failed. `workspace exec` exits with the command's
  own code and prints its answer on stdout (seen: `exit 3` →
  `{"exit_code":3}`, exit 3). A doctor exits 0 once its report is made.

```js
import layerfs, { LayerFSError } from "@layerfs/node";
try {
  await layerfs.file.edit("shop/api", "agent", "/docs/a.txt", { old_string: "nope", new_string: "three" });
} catch (e) {
  if (!(e instanceof LayerFSError)) throw e;
  // e.error ("NotFound"; also e.code), e.message, e.fix, e.detail (null unless
  // the call passed detail: true) and each flat field as a property
  console.log(e.error, e.message);   // seen in the shell: NotFound "the old text is not in /docs/a.txt"
}
```

## One script: fork, edit, test, publish

```js
// step.mjs: run with `node step.mjs`
import layerfs from "@layerfs/node";

const P = "shop/api";
await layerfs.branch.fork(P, "agent", "main");                       // { commit }
await layerfs.file.write(P, "agent", "/hello.js", "console.log('hello');\n", { message: "add hello.js" });
await layerfs.file.edit(P, "agent", "/hello.js", { old_string: "'hello'", new_string: "'hello, shop'", message: "greet the shop" });
console.log((await layerfs.file.read(P, "agent", "/hello.js")).text);

// a Bash step; the Sandbox "tests" was made once with sandbox.create(P, "tests")
const { workspace } = await layerfs.workspace.mount("tests", "agent");   // "tests:agent"
try {
  const run = await layerfs.workspace.exec(workspace, "timeout 110 node hello.js | tee hello.txt");
  await layerfs.workspace.commit(workspace, { message: `step 3: node hello.js (exit ${run.exit_code})` });
} finally {
  await layerfs.workspace.unmount(workspace);
}
await layerfs.branch.publish(P, "agent");                             // { commit }: main's new head
layerfs.close();
```

The same file steps in the shell (content comes from stdin or `--from <file>`):

```bash
layerfs branch fork shop/api agent main
printf "console.log('hello');\n" | layerfs file write shop/api agent /hello.js -m "add hello.js"
layerfs file edit shop/api agent /hello.js --old-string "'hello'" --new-string "'hello, shop'" -m "greet the shop"
layerfs file read shop/api agent /hello.js --text
```

## Rules

1. **Read before you retry.** Every call is one attempt; nothing is retried for
   you. On a refusal, read its `fix` and look at the project, then decide. On
   `Unknown` the outcome is not known: check with `commit.list`, `file.read`
   or `workspace.status`, and report.
2. **A non-zero exit is not a refusal.** `workspace.exec` answers normally;
   check `exit_code`.
3. **Keep each command under 2 minutes.** `workspace.exec` has no timeout: wrap
   long commands, `timeout 110 npm test`. `workspace.cancel(workspace)` ends
   what still runs.
4. **A Workspace shows its Branch as it was at mount.** After `file.*` changes
   `workspace.status` answers `branch_moved: true` and the next
   `workspace.commit` merges them in; mount again to see them before you
   build or test.
5. **Commit every step that changes files**, failures included; publish to
   `main` at milestones.
6. **Workspaces outlive your process.** The handle works from any later
   process with the same `LAYERFS_ROOT`. Keep each Workspace's calls
   sequential.
7. **Clean up:** commit or discard, then unmount every Workspace;
   `sandbox.remove` a Sandbox you created when you are done with it.

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

From the manual (the scope was checked live before it named its project; not
re-run on this build):

- It is an allow list: the `file` calls, `branch` `list`, `get`, `fork`,
  `merge` and `publish`, and the `workspace` calls. Every other call is
  refused `InvalidInput` `out of scope: <call>`, such as
  `out of scope: store.remove`; the `commit` calls are refused
  `out of scope: history`.
- Every `file` and `branch` call must name the scope's project
  (`out of scope: project`); a `workspace` call's Sandbox must be one of that
  project's (`out of scope: sandbox`).
- Paths must be the folder or beneath it (`out of scope: path`).
- Branch names become `<branch>.<name>`: `main` means `agent.main`. A Commit
  id passes through unchanged. `branch.list` shows only the namespace's
  Branches, named without the prefix. Without `scope.branch`, every call
  that names a Branch is refused (`out of scope: branch`).
- `workspace.mount` under a scope mounts only the namespace's `main`, and
  `workspace.exec` starts the command in the folder.
- The namespace's `main` must exist first, and the Sandbox too: create them
  without a scope (`branch.fork(project, "agent.main", "main")`).
- `--scope` without `--scope-project` is a usage error (exit 2).

## Which call for which job

| Job | Use |
| --- | --- |
| Create or change one file | `file.write`, or `file.edit` with `{ old_string, new_string }` (exactly one match, or `replace_all`) |
| Several related changes as one Commit | `file.apply(project, branch, changes)` |
| Folders, copy, move, delete | `file.mkdir`, `file.cp`, `file.mv`, `file.rm` (`recursive` for a folder) |
| Install, build, test, run anything | `workspace.mount`, `exec`, `commit`, `unmount` |
| Look at files | `file.read` (line windows), `file.list`, `file.stat`, `file.grep` (literal text), `file.glob` |
| Compare | `commit.diff(project, ref, { from, path, with: ["patch"] })` |
| History | `commit.list`, `commit.get`, `file.history`, `file.blame` |
| Check a Workspace before Commit | `workspace.status` (changed paths, whether the Branch moved) |
| Sandboxes | `sandbox.create`, `sandbox.list`, `sandbox.get`, `sandbox.remove`, `workspace.list` |
| Combine Branches | `branch.merge(project, branch, from)`, then `branch.publish(project, branch)` |
| Resolve a `Conflict` | [resolving conflicts](references/rules/conflicts.md) |
| Is something wrong in the home? | the six doctors ([doctors](references/methods/doctor.md)) |

For Rust projects, follow the [Cargo workflow](references/rules/cargo.md).

## Calls

83 calls in ten families. Each family's page has arguments, answers,
refusals and examples. Every option object may also carry `detail`, `with`
(only where listed) and `log_tag`.

| Family | Calls |
| --- | --- |
| [home](references/methods/home.md) | `init()`, `get({ with: ["platforms", "tree"] })`, `log({ call, project, client, tag, outcome, since, limit, cursor })`, `clear_log()`, `doctor({ on_progress })` |
| [project](references/methods/project.md) | `init(project, folder, { timeout, on_progress })`, `list({ store, limit, cursor, with: ["stats"] })`, `get(project)`, `stats(project, { branch, at })`, `export(project, destination, { branch, at, on_progress })`, `delete(project)`, `hide_path(project, path)`, `doctor(project, { full, on_progress })` |
| [branch](references/methods/branch.md) | `list(project, { prefix, exclude_prefix, limit, cursor })`, `get(project, branch)`, `fork(project, name, from?, { at })`, `merge(project, branch, from, { into, units, expected, message })`, `publish(project, branch, { at, expected, force })`, `remove(project, branch)` |
| [commit](references/methods/commit.md) | `list(project, branch, { at, limit, cursor })`, `get(project, commit, { branch })`, `diff(project, ref, { from, path, limit, cursor, with: ["patch"] })` |
| [file](references/methods/file.md) | reads `read(project, branch, path, { at, line, limit })`, `stat(project, branch, path, { at })`, `list(project, branch, path, { at, limit, cursor })`, `glob(project, branch, pattern, path?, { exclude, at, limit, cursor })`, `grep(project, branch, pattern, path?, { include, exclude, ignore_case, at, limit, cursor })`, `history(project, branch, path, { at, traversal, limit, cursor })`, `blame(project, branch, path, { at, line, limit })`; writes `write(project, branch, path, content, { mode, expected, message })`, `edit(project, branch, path, { old_string, new_string, replace_all, expected, message })`, `rm(project, branch, path, { recursive, expected, message })`, `mkdir(project, branch, path, { mode, expected, message })`, `mv(project, branch, from, to, { expected, message })`, `cp(project, branch, from, to, { expected, message })`, `apply(project, branch, changes, { expected, message })` |
| [sandbox](references/methods/sandbox.md) | `create(project, name, { machine, image, mount, on_progress })`, `list({ project, machine, limit, cursor })`, `get(sandbox, { with: ["stats"] })`, `remove(sandbox, { force, on_progress })`, `files(sandbox, path?, { depth, limit, cursor, byte_offset, byte_length })`, `log(sandbox, { limit, cursor })`, `doctor(sandbox, { on_progress })` |
| [workspace](references/methods/workspace.md) | `list({ sandbox, limit, cursor })`, `mount(sandbox, branch, { name })`, `exec(workspace, command, { cwd, env, stdin, stream, stdout, stderr, on_progress })`, `cancel(workspace, { exec })`, `commit(workspace, { message, expected, force })`, `unmount(workspace, { discard, force })`, `status(workspace, { path })` |
| [machine](references/methods/machine.md) | `list({ with: ["live"] })`, `get(machine, { with: ["live"] })`, `images(machine, { with: ["docker"] })`, `load_image(machine, image, { on_progress })`, `prune(machine, image?, { unused })`, `stats(machine, { with: ["space"] })`, `files(machine, path?, { depth, limit, cursor, byte_offset, byte_length })`, `log(machine, { limit, cursor })`, `start(machine, { on_progress })`, `stop(machine, { force, on_progress })`, `create(name, platform, { cpus, memory, disk, image, os_image, on_progress })`, `remove(machine, { on_progress })`, `update(machine, { cpus, memory, image, docker, wsl, daemon })`, `doctor(machine, { on_progress })` |
| [store](references/methods/store.md) | `list({ machine, with: ["stats"] })`, `get(store, { with: ["live", "stats"] })`, `create(name, { machine })`, `connect(store, url, { user, password, synchronous_commit })`, `disconnect(store)`, `attach(store, { machine })`, `detach(store, { machine })`, `move(store, machine, { on_progress })`, `remove(store)`, `compact(store, { on_progress })`, `doctor(store, { machine, on_progress })` |
| [backup](references/methods/backup.md) | `connect(name, url, { access_key, secret_key })`, `disconnect(bucket)`, `list({ bucket })`, `create(store, bucket, { file, on_progress })`, `restore(bucket, file, store, { on_progress })`, `doctor(bucket, { full })` |

`home`, `machine`, `store` and `backup` set up where things live; an agent
working on a project rarely needs them. Reading state, the call log and
tracing are the [`layerfs-sdk-observe`](../layerfs-sdk-observe/SKILL.md) skill.

Paths are absolute in the project (`/src/index.js`).

## From 1.x

Removed names have no alias: they are not in the table (Node has no such
function; the shell has no such command).

| 1.x | 2.0 |
| --- | --- |
| `branch.commits`, `branch.get_commit` | `commit.list`, `commit.get(project, commit, { branch })` |
| `branch.diff`, `file.diff` | `commit.diff(project, ref, { from, path, with: ["patch"] })` |
| `branch.fork(project, name, from_branch, from_commit?)` | `branch.fork(project, name, from?, { at })` |
| `branch.merge(project, name, a, b)` | `branch.merge(project, a, b, { into: name })` |
| `branch.publish(project, branch, commit?, { expected_base, expected_head })` | `branch.publish(project, branch, { at, expected })` |
| `file.revision` | `file.history` |
| `file.read(…, commit?, { start_line, end_line })`, `file.blame(…, { from_line, line_count })` | `file.read(…, { at, line, limit })`, `file.blame(…, { at, line, limit })` |
| `file.glob(project, branch, path, pattern)`, `file.grep(…path, pattern)` | `file.glob(project, branch, pattern, path?)`, `file.grep(project, branch, pattern, path?)` |
| `file.edit(…, { old, new })` | `file.edit(…, { old_string, new_string })` |
| `project.remove_file` | `project.hide_path` |
| `project.list(store?)`, `project.stats(project, branch?, commit?)` | `project.list({ store })`, `project.stats(project, { branch, at })` |
| `sandbox.close(name)` | `sandbox.remove(sandbox)` |
| `sandbox.create(project, name, machine?)` | `sandbox.create(project, name, { machine })` |
| `workspace.exec_cancel(ws, exec_id)` | `workspace.cancel(workspace, { exec })` |
| `ws` (`<sandbox>:<path>`), `kind: "UpToDate"`, `lww` | `workspace` (`<sandbox>:<name>`), `unchanged: true`, `overwritten` |
| `logs.list`, `logs.reset` | `home.log`, `home.clear_log` |
| `logs.sandbox`, `logs.machine` | `sandbox.log`, `machine.log` |
| `layerfs doctor` / `doctor()`, `doctor.fix(id)` | `home.doctor()` and the other five doctors; leftovers are removed by LayerFS itself |
| `machine.status`, `machine.space`, `machine.load`, `machine.platforms` | `machine.get(m, { with: ["live"] })`, `machine.stats(m, { with: ["space"] })`, `machine.load_image`, `home.get({ with: ["platforms"] })` |
| `store.status`, `store.stats`, `store.push`, `store.pull` | `store.get(s, { with: ["live"] })`, `store.get(s, { with: ["stats"] })`, `store.move(s, machine)` |
| `page`, `page_size`, `page_pos`, `has_more`, `next_page_pos`, `continuation` | `{ limit, cursor }` in, `{ items, next_cursor }` out |
| `LayerFSError { code, message, detail }` with raised codes (`HeadMoved`, `NoSuchPath`, `ReadOnly`, …) | `LayerFSError { error, message, fix, … }` with 13 public codes; the raised code is `detail.code` under `detail: true` |
