# LayerFS JavaScript SDK and terminal UI

> **Status:** Released for LayerFS 0.1.7.
> This is part of the [0.1.7 manual](README.md). It describes the tagged source
> [`v0.1.7`](https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.7).
> Method details and limits are in the [SDK reference](sdk-reference.md). The
> acceptance record is
> [PHASE5](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/327/PHASE5-20261015.md).

The JavaScript SDK, `@layerfs/sdk`, is the way programs use LayerFS: Branches of
a project's files, Commits, and Sandboxes whose Workspaces run ordinary commands.
The terminal UI in [`sdk_sim/`](../../../sdk_sim/) shows the same model in a
browser. It runs either on an in-page simulator or live on the real product,
through a small local server.

## 1. How the pieces fit

```
your program ──► @layerfs/sdk (sdk/js/src)          29 public methods
                 @layerfs/sdk/operator (sdk/js/operator)   statistics, processes, read model
                       │
                       ▼
                 native addon (sdk/js/native)        one call per operation; runs commands
                       │
                       ▼
                 layerfs-api ──► layerfs-bridge ──► daemon in each Sandbox container ──► Store
```

- **Project.** Init copies a source directory into one Store file, on a Docker
  volume that the project's daemons share.
- **Sandbox.** A Sandbox is one container running one LayerFS daemon. It holds
  zero or more mounted Workspaces.
- **Workspace.** A Workspace is a complete, writable filesystem view of a Branch
  head, mounted at a path inside the Sandbox. Mounting does not scan or copy the
  tree.
- **Commit.** Commit saves a Workspace's changes to its Branch. If the Branch
  moved since the mount, Commit merges with the new head first.
- **main.** `main` changes only through `branch.publish`.

## 2. Setting up

You need Docker, Node 20 or later, the host Rust toolchain `+1.85.1`, and the
pinned images.

1. **Build the Linux daemon** inside the pinned build image. The command is in
   [PHASE5, "Exact Linux build"](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/327/PHASE5-20261015.md).
   It writes `core/target/cluster2-linux/release/layerfs-daemon`.
2. **Build the host addon:**
   ```bash
   npm --prefix sdk/js run build:native
   ```
   Add `-- --debug` for a dev build. The addon is renamed into place
   atomically, so a running process keeps its old copy.
3. **Set the environment** the client reads when it is constructed:

   | Variable | Meaning |
   | --- | --- |
   | `LAYERFS_HOME` | Private directory (0700) for provisioning and discovery records |
   | `LAYERFS_DOCKER` | Docker Engine Unix socket; defaults to `/var/run/docker.sock` |
   | `LAYERFS_IMAGE` | Runtime image, as an exact `sha256:` digest |
   | `LAYERFS_VOLUME` | An existing Docker volume shared by the project's daemons |
   | `LAYERFS_DAEMON` | Host path to the Linux daemon executable |
   | `LAYERFS_NATIVE` | Optional addon path; defaults to `sdk/js/native/layerfs.node` |

Every Store is Disposable (WAL, `synchronous=OFF`). Stores created before
#327's format change, schema versions 7–9 and 13–15, are refused without
migration.

## 3. Using the SDK

```js
import { LayerFS, isRefusal } from "@layerfs/sdk";

const layerfs = new LayerFS("website");        // no I/O yet
await layerfs.project.init("./website");        // once per project

await layerfs.branch.fork("feature");
await layerfs.fs.write("feature", "/README.md", "Hello\n", "Update introduction");
const text = await layerfs.fs.read("feature", "/README.md", 1, 20);

const id = await layerfs.sandbox.create();      // an opaque id; another process can reuse it
const ws = await layerfs.sandbox.mount(id, "feature");
await layerfs.sandbox.exec(id, ws.path, "npm test", "Run tests");
try {
  await layerfs.sandbox.commit(id, ws.path, { message: "Save changes" });
} catch (error) {
  if (isRefusal(error, "Conflict")) console.log(error.detail);  // markers were written
  else throw error;
}
await layerfs.sandbox.unmount(id, ws.path);
await layerfs.sandbox.close(id);
await layerfs.branch.publish("feature");          // main now has feature's files
```

### Methods

Every method returns a Promise, and every argument is positional.

| Group | Methods |
| --- | --- |
| `project` | `init(sourcePath)`, `list_branches(pageSize?, pagePos?)` |
| `fs`, reads | `read(ref, path, startLine?, endLine?)`, `list(ref, path?, pageSize?, pagePos?)`, `stat(ref, path)`, `grep(ref, pattern, path?, pageSize?, pagePos?)`, `glob(ref, pattern, path?, pageSize?, pagePos?)`, `diff_files(leftRef, leftPath, rightRef, rightPath)`, `blame(ref, path, fromLine?)` |
| `fs`, writes | `write(branch, path, content, message?)`, `edit(branch, path, oldText, newText, message?)`, `apply(branch, changes, message?)`, `mkdir(branch, path, message?)`, `rm(branch, path, message?)`, `cp(branch, from, to, message?)`, `mv(branch, from, to, message?)` |
| `branch` | `fork(name, fromRef?)`, `list_commits(branch, pageSize?, pagePos?)`, `diff_commits(from, to, pageSize?, pagePos?)`, `diff_branches(from, to, pageSize?, pagePos?)`, `merge(a, b, name, message?)`, `publish(branch, commit?)` |
| `sandbox` | `create()`, `mount(id, branch)`, `exec(id, path, command, message?)`, `status(id, path)`, `commit(id, path, { message?, force? })`, `unmount(id, path, { discard? })`, `close(id)` |

- **Refs.** A `ref` is `main`, a Branch name, a Commit id or a Layer id. Paths
  are absolute inside that view.
- **Writes.** Each `fs` write publishes exactly one Commit on its Branch.
  `apply` takes an ordered list of `{ op, ... }` changes (`write`, `edit`,
  `mkdir`, `rm`, `cp`, `mv`) and publishes them as one Commit. A refusal names
  the failing change in `detail.index`, and nothing is published.
- **Recursive operations.** `cp`, `mv` and `rm` work on whole trees. A
  destination must not exist already. Copies keep modes, symlinks and hardlinks
  within the copied set.
- **Messages.** Messages are optional UTF-8 strings of at most 4096 bytes.
  Newline and tab are allowed; other control characters are refused before any
  work. A Commit's message is part of its identity (`layerfs/commit/v3`). An
  `exec` message only labels that run and creates no history.
- **Paging.** A page is `{ items, hasMore }`. The defaults are `pageSize = 50`
  and `pagePos = 0`, and the largest `pageSize` is 1000. Advance with
  `pagePos + items.length`.
- **Read bounds.** `read` and `blame` use 1-based inclusive line numbers. One
  answer holds at most 2000 lines and 48 KiB, and reports `truncated` and
  `nextLine`. Binary content is refused.
- **Committing a stale Workspace.** `commit` merges with a head that moved since
  the mount. A conflict writes markers into the Workspace and commits nothing.
  `{ force: true }` overwrites the newer head.

### Refusals

A refused call throws a `LayerFSError` with a stable `code`, a readable
`message` and a structured `detail`. `isRefusal(error, code)` tests for a code.
The codes callers should handle are exported as `CODES`, for example
`Conflict`, `HeadMoved`, `Dirty`, `Exists`, `NoSuchPath`, `NoSuchBranch`,
`InvalidInput`, `MountsActive` and `NoSuchSandbox`.

Nothing is retried. An uncertain operation is reported as uncertain and is never
replayed.

### Another process

A Sandbox id is persistent. A fresh Node process with the same `LAYERFS_HOME`
can mount, run commands and commit with a saved id, without Init and without
reinstalling. Recovery after a daemon or container restart is not supported.

## 4. Operator surface

`@layerfs/sdk/operator` holds views that the UI and test harnesses need. It is
an internal surface: the public SDK cannot call it, and every layer refuses
`operator.*` names that arrive through public dispatch.

```js
import { LayerFSOperator } from "@layerfs/sdk/operator";
const operator = new LayerFSOperator(layerfs);
```

| Group | Methods | What they return |
| --- | --- | --- |
| `store` | `stats()`, `commits(branch, pageSize?, pagePos?)` | Store totals; per-Commit byte counters |
| `files` | `stats(ref, path)`, `history(ref, path, pageSize?, pagePos?)` | A file's current size, revision count and revisions |
| `workspace` | `overlay(id, path)`, `changes(id, path, pageSize?, pagePos?)` | A mounted Workspace's uncommitted rows and bytes; changed paths |
| `process` | `start(id, path, command)`, `list(id)`, `cancel(id, process)` | Background commands, owned by the native executor |
| `inspect` | `snapshot(since?)` | The read model the UI draws |

Size terms used by the statistics:

| Term | Meaning |
| --- | --- |
| logical | Bytes of regular files as written, counted every place a file appears in history |
| canonical | Bytes of distinct stored objects, after content-addressed dedupe |
| stored | Bytes of pack bodies, after compression and delta encoding |
| metadata | Metadata pack bodies, plus SQLite space outside pack bodies |
| dedupe | logical / canonical |
| compression | canonical / stored |

Per-file physical bytes are not attributed, so they show as "not reported". The
revision count is exact up to 1000.

## 5. The terminal UI

### Starting it

- **Simulator.** Open `sdk_sim/tui.html` in a browser. It needs no server and no
  Docker, and its sizes are labelled estimates.
- **Live, on the real product:**
  ```bash
  node sdk_sim/server/server.mjs 8327
  ```
  Then open `http://localhost:8327/tui.html?backend=live`. The page starts a
  fresh project from the test fixture (`sdk/js/test/fixture/website`). The
  server needs the built daemon and the runtime image, as in section 2.
  Stopping the server closes the project and removes its volume.

### Layout

| Area | What it does |
| --- | --- |
| Top bar | `main` and one tab per Branch; `+` forks a Branch. The buttons open the demo player, the Files pane (⌘O) and a new Sandbox terminal (⌘T), toggle the fs terminal (⌘1) and History (⌘2), switch the agent mascots, reset the project and change the theme |
| fs terminal | Edits the active Branch directly; each write is one Commit. Its footer shows the Store totals (`STORE`) and the dedupe and compression rates (`RATE`) |
| Sandbox terminal | One mounted Workspace. Shell lines run through `sandbox.exec`, and a trailing `&` starts a background process. Its footer shows the Workspace's overlay: payload, metadata and change counts |
| History | The project graph (main's Layers, Branches, Commits, merges), plus a project-wide terminal. Click a node to type its id |
| Files pane | Browse any ref. Its modes are view, diff, blame and history, with per-file statistics |
| Sandboxes bar | Each Sandbox, its mount count and a close action |
| Demo player | Plays a story step by step: play/pause, next step, speed (1×, 2×, 4×), story choice, and seeking by clicking a step dot |

Ids on screen are display ids (`C1`, `L0`, `R0`, `sb_01`). Real 66-hex ids never
reach the page, including inside command output such as conflict markers. Each
command shows how long it took. Clicking the time shows the SDK call it made.
These times are diagnostics, not measurements.

### Terminal commands

Type `help` in any terminal.

| Terminal | Commands |
| --- | --- |
| fs | `ls [path]`, `cat <path>`, `stat <path>`, `grep <text> [path]`, `diff <a>[@C#] <b>[@C#]`, `blame <path>[@ref]`, `open <path>[@ref]`, `write <path> <<EOF … EOF`, `edit <path> <old> <new>`, `cp`, `mv`, `mkdir [-p]`, `rm [-r]`, and `apply` … `done` (or `abort`) for one Commit |
| Sandbox | Any shell command; `commit [--force]`; `unmount [--discard]` |
| History | `branches`, `log <branch\|main>`, `fork <name> [from <branch\|main\|C#>]`, `diff <a> <b>`, `merge <a> <b> as <c>`, `publish <branch>` |

Keyboard: ⌘T new Sandbox, ⌘D split, ⌘1 fs, ⌘2 History, ⌘O Files, ⌘⌥←/→ move
focus, ↑/↓ recall earlier lines.

### Stories

| Story | Steps | Shows |
| --- | ---: | --- |
| Tour | 24 | Forks, fs and Workspace Commits, a background dev server and its stop, a `Busy` refusal, publish |
| Conflicts | 15 | A stale Workspace's Commit writes conflict markers, which are then resolved and committed; Branch merges, one refused |
| Commit & merge | 21 | Commits from Workspaces and the fs terminal, Branch merges and conflicts, publishing over a moved main |
| 5 agents | 52 | Five agents on two Branches, one Workspace per tool call |

Add `?demo=<story>` to the URL to start a story when the page loads. The values are `tour` (the default), `conflicts`, `merge` and `agents`.

### Server routes

The server listens on 127.0.0.1 only. It runs one operation at a time, in
arrival order.

| Route | Purpose |
| --- | --- |
| `GET /tui.html`, any `sdk_sim/` file | The UI |
| `POST /api/reset` | Close the current project and Init a fresh one |
| `GET /api/state?since=<version>` | The read model, or `{ unchanged: true }` |
| `POST /api/tree {root}` | A saved root's files, for the Files pane |
| `POST /api/close` | Close the project |
| `GET /api/{sdk\|operator}/{group}/{method}?args=[…]` | Read methods |
| `POST /api/{sdk\|operator}/{group}/{method} {args:[…]}` | Methods that change state |

HTTP exists only in this demo server. It is not a product surface.

## 6. Tests

Each command below is one case. Give it a wall timeout of at most 120 seconds.

| Command | What it checks |
| --- | --- |
| `node sdk_sim/test.js` | Simulator model steps |
| `node sdk_sim/test_demo.js` | Every story on the simulator |
| `node sdk_sim/test_terminal.js` | Terminal command expectations |
| `node sdk_sim/test_live.js <tour\|conflicts\|merge\|agents>` | One story live, compared with the simulator |
| `node sdk/js/test/verify/run.mjs <case>` | One VERIFICATION case live, for example `S328` or `XPROCESS` |

Receipts are written under `core/target/issue327/verify/`.

## 7. Known limits

- **Restart recovery.** Recovery after a daemon or container restart, and replay
  of uncertain operations, are excluded.
- **Deferred FUSE behaviour.** Zombie reaping in Sandboxes and kernel requests on
  replaced inodes are deferred by the owner. A cancelled command can leave one
  unreaped zombie.
- **Linear scans.** Deep page offsets skip linearly through bounded windows.
  Workspace change and overlay scans also walk the namespace linearly.
- **Per-file sizes.** Per-file physical bytes are not reported.
- **Durable profile.** Durable Stores compile but are not run until the owner
  reauthorizes them.
- **Open VERIFICATION rows.** A9 keeps its historical FAIL, with XPROCESS as
  separate passing evidence. B7 was not run.
