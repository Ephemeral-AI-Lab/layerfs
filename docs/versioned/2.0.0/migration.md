# Migrating to LayerFS 2.0.0

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Part of the [2.0.0 manual](README.md). It maps what a program written against
the earlier JavaScript surface used (packages, constructors, methods and
behaviour) to what 2.0.0 offers, or says that it is gone and why. Every row
names the file or page that backs the 2.0 side. The code wins over this page
where they differ.

Two earlier surfaces are meant by "old":

- **1.1.0**, the released tag `v1.1.0`: `@layerfs/agent` (the `LayerFS` client
  and the `layerfs` command), `@layerfs/backend`, `@layerfs/runtime`. Its guide
  is `api/how-to-use/agent.md` at that tag.
- **#521**, the pull request
  [Ephemeral-AI-Lab/layerfs#521](https://github.com/Ephemeral-AI-Lab/layerfs/pull/521)
  (branch `area-p/complete-p1-p15`), which was closed and never merged. It added
  `@layerfs/mcp`, a folder scope, events, per-call results, a Python client and
  more. Nothing of it shipped from `main`; it is listed here because programs
  were written against its documents. A row marked (#521) never existed in a
  release.

| Status | Meaning |
| --- | --- |
| **Replaced** | the same job under a new name or shape |
| **Changed** | the same name or job with different behaviour |
| **Removed** | taken out on purpose; the reason and the record of the decision are given |
| **Not restored** | existed before, has no 2.0 equivalent, and the owner kept the project baseline ([history features](#10-history-features-that-are-gone)) |
| **Unavailable** | not built or not verified here; a pointer says what is missing |

What was run for this page, and what was not, is at the [end](#what-was-run).
Nothing here is published to npm, PyPI or crates.io.

## 1. Packages and install

| Old | 2.0.0 | Status and reason |
| --- | --- | --- |
| npm `@layerfs/agent` (`import { LayerFS } from "@layerfs/agent"`) | npm `layerfs`: `import layerfs from "layerfs"` or `import { client, LayerFSError } from "layerfs"`. An ES module for Node 22 or later, MIT | **Replaced.** One package, one native addon. [`api/node/package.json`](../../../api/node/package.json), [Node](../../architecture/api/front-ends/node.md#package) |
| npm `@layerfs/cli` (the Node `layerfs` command; #521: 106 commands) | the native `layerfs` command, one Rust binary, 61 calls plus `doctor`, `import-env`, `worker` and `help` | **Replaced.** It is not an npm package: `layerfs-cli` has `publish = false`. The command counts are not comparable (the old ones included the backend and runtime commands). [CLI](../../architecture/api/front-ends/cli.md) |
| npm `@layerfs/backend` (`LayerFSBackend.of(client)`, 14 operator and diagnostic calls) | none, except `layerfs.workspace.status(ws)` | **Removed** (O25). [Limitations](limitations.md#not-built-refused-as-unsupported), [README](README.md#compatibility-with-110) |
| npm `@layerfs/runtime` (`LayerFSRuntime.open`, worker start and restart, `project.start_init` / `get_init` / `subscribe_init`, Sandbox and volume sweep) | no package. The `machine`, `store`, `project` and `sandbox` families; `project.init` with `on_progress`; `machine start` and `machine stop` by hand (nothing starts a machine on demand, D11) | **Replaced** in part, **Removed** for the job registry (O22) and the sweep. [Machines and Stores](machines-and-stores.md#starting-and-stopping-manual-535-d11), [Node and Python](sdk.md#functions-as-options) |
| npm `@layerfs/mcp` (#521: 50 tools, stdio) | none | **Removed.** MCP is shelved (O32): 2.0 has no MCP server, and this guide gives no MCP setup. [Coverage](../../architecture/api/call-table/coverage.md#outside-the-table) |
| Python `layerfs` (#521: `LayerFS.open/init`, `client.fs.*`) | Python `layerfs`: `import layerfs; layerfs.file.read(...)`, `layerfs.client(home=, scope=)`, and `layerfs.aio` for coroutines. Python 3.10 or later | **Replaced.** [Python](../../architecture/api/front-ends/python.md#package) |
| `plugins/*` (`dsh-layerfs`, `cursor-agents`, `pi-layerfs`, `magpie-layerfs`, `herdr-layerfs`, `codex-parallel-verify`) | not migrated | **Unavailable.** Their documents still name `@layerfs/agent`, `@layerfs/runtime` or `@layerfs/backend` and break on 2.0 until each is rewritten |

### Install

None of the three artifacts is published. Build and install the files, here
into `$OUT` (the steps are in
[api/README.md](../../../api/README.md#package-and-install); the commands below
were run, see [what was run](#what-was-run)):

```bash
# Node: needs the addon first; npm pack refuses to pack without it
npm --prefix api run build:native
(cd api/node && npm pack --pack-destination "$OUT")      # layerfs-2.0.0.tgz
npm install "$OUT/layerfs-2.0.0.tgz"                     # in an empty project

# Python: a wheel for a virtualenv
(cd api/python && maturin build --release --locked -o "$OUT")
pip install "$OUT"/layerfs-2.0.0-*.whl

# the command: one file
cargo +1.85.1 build --release --locked -p layerfs-cli    # target/release/layerfs
```

The default addon, wheel and command have no PostgreSQL and no TLS; a call
that needs them is refused `Unsupported`. Each artifact needs the glibc of the
host that built it (2.39 on the host used). See
[artifact-qualification-2.0.0.md](../../general/artifact-qualification-2.0.0.md).

## 2. Constructors, settings and entry points

| Old | 2.0.0 | Status and reason |
| --- | --- | --- |
| `LayerFS.open(name, { home, storePath, deploy, storePolicy, trace, scope })` | `client({ home, scope })` (Node), `client(home=, scope=)` (Python). The default export and the family exports use `client()`. Every call names its project as `"<store>/<project>"` | **Replaced.** There is no default project and no per-client Store, deploy or policy option. [Node and Python](sdk.md), [README](README.md#compatibility-with-110) |
| `LayerFS.init(name, folder, timeoutMs)` | `store.create(store)`, then `project.init("<store>/<project>", folder, { timeout, on_progress })` | **Replaced.** `LayerFS.init` and `LayerFS.open` have no alias (D5). [`project.init`](../../architecture/api/sdk/project/init.md) |
| `client.call(method, args)`, `LayerFS.call` | not exported. Node exports `table`, `families`, `client`, `doctor`, `close` and the nine families; Python `TABLE`, `FAMILIES`, `client`, `doctor`, `close` | **Removed** from the packages. Rust callers use `layerfs_api::call_scoped`. [`index.js`](../../../api/node/src/index.js), [Table](../../architecture/api/call-table/table.md#layerfstable) |
| `await client.close()`, `Symbol.asyncDispose` (#521) | module-level `layerfs.close()` (Node) and `layerfs.close()` (Python): closes the Stores this process holds idle, answers how many | **Changed.** There is no client object to dispose; call `close()` before the process ends. [Node and Python](sdk.md) |
| `LAYERFS_STORE_PATH`, `storePath` | none. A Store lives under the home (`store/<name>/`); `store.create`, `store.connect`, `store.push` and `store.pull` make and move Stores | **Removed.** [The home](home.md), [Machines and Stores](machines-and-stores.md#stores) |
| `LAYERFS_WORKER_ENDPOINT`, worker started by `@layerfs/runtime` | none. The home's machine records say where a Store's worker is; `layerfs worker` serves it | **Removed.** [Machines and Stores](machines-and-stores.md#routes) |
| `LAYERFS_DAEMON`, `_IMAGE_ARCHIVE`, `_DOCKER`, `_IMAGE`, `_REMOTE_*`, `_SANDBOX`, `_PLACEMENT` and the other 1.1.0 platform settings | records under `LAYERFS_HOME`. Run `layerfs import-env` once to write the current environment as records; later changes by `machine update` | **Replaced.** Only `LAYERFS_HOME`, `LAYERFS_TRACE` and the measurement settings are read. [The home](home.md#layerfs-import-env), [README](README.md#compatibility-with-110) |
| `LAYERFS_STORE_POLICY` (`storePolicy`) | none. Every sqlite Store is Disposable | **Removed.** Durable execution stays disabled by the owner. `import-env` ignores the setting |
| init timeout (`LayerFS.init` third argument) | the `timeout` option of `project.init` | **Replaced.** There is no generic per-call deadline: every call is one attempt |
| 1.1.0 homes and Stores | converted once, on the first 2.0 call | **Changed.** [The home](home.md#a-110-home) |

## 3. Methods

Positionals now go from the outside in: project, Branch, path or new name,
other inputs, an optional Commit last (D7). Every call takes the project first.
Node and Python take the table's options as a trailing object or as keywords;
1.1.0 methods took positionals only. Result keys are snake_case in every front
end (`exit_code`, not `exitCode`), and paging is `page_size` / `page_pos`
options. The argument lists are in the generated
[coverage matrix](../../architecture/api/call-table/coverage.md#the-matrix).

| Old (1.1.0) | 2.0.0 | Notes |
| --- | --- | --- |
| `project.list_branches(pageSize?, pagePos?)` | `branch.list(project, { page_size, page_pos })` | **Replaced.** [`branch.list`](../../architecture/api/sdk/branch/list.md) |
| `project.list_sandboxes(pageSize?, pagePos?)` | `sandbox.list([project], { page_size, page_pos })` | **Replaced.** |
| `fs.read(ref, path, startLine?, endLine?)` | `file.read(project, branch, path, commit?, { start_line, end_line })` | **Replaced.** A `ref` was `main`, a Branch, a Commit id or a Layer id. Now `branch` is always given and a 66-hex Commit or Layer id goes in the optional `commit`, which then replaces `branch` ([`file.read`](../../architecture/api/sdk/file/read.md)) |
| `fs.list`, `fs.stat` | `file.list(project, branch, path, commit?, { page_size, page_pos })`, `file.stat(project, branch, path, commit?)` | **Replaced.** |
| `fs.grep(ref, pattern, path?, ...)`, `fs.glob(ref, pattern, path?, ...)` | `file.grep(project, branch, path, pattern, commit?, ...)`, `file.glob(...)` | **Changed.** `path` now comes before `pattern` and is required |
| `fs.blame(ref, path, fromLine?)` | `file.blame(project, branch, path, commit?, { from_line })` | **Replaced.** |
| `fs.diff_files(leftRef, leftPath, rightRef, rightPath)` | `file.diff(project, left_branch, left_path, right_branch, right_path, { left_commit, right_commit })` | **Replaced.** |
| `fs.write(branch, path, content, message?)` | `file.write(project, branch, path, content, { message, mode })` | **Replaced.** `main` is refused `ReadOnly`: it changes only through `branch.publish` |
| `fs.edit(branch, path, oldText, newText, message?)` | `file.edit(project, branch, path, { old, new, message })` | **Changed.** `old` and `new` are options |
| `fs.apply(branch, changes, message?)` | `file.apply(project, branch, changes, { message })` | **Replaced.** A change names `path`, `from_path`, `to_path` and the texts `old`, `new`, `content`; the Store's own names (`from`, `to`, `oldText`) are refused ([Limitations](limitations.md#seen-in-the-code-not-tested)). An empty list answers `UpToDate` ([known gaps](../../general/interface-report-2.0.0.md#known-gaps)) |
| `fs.mkdir`, `fs.rm`, `fs.cp`, `fs.mv` | `file.mkdir`, `file.rm`, `file.cp(project, branch, from_path, to_path)`, `file.mv(...)` | **Replaced.** |
| `branch.fork(name, fromRef?)` | `branch.fork(project, name, from_branch, from_commit?)` | **Changed.** The source Branch is a required positional (`main` is written out) |
| `branch.list_commits(branch, pageSize?, pagePos?)` | `commit.list(project, branch, { page_size, page_pos })` | **Replaced.** [`commit.list`](../../architecture/api/sdk/commit/list.md) |
| `branch.diff_commits(from, to, ..., { cursor, hunks })` | `commit.diff(project, from_commit, to_commit, { paths_only, page_size, page_pos, cursor })` | **Changed.** `hunks` became `paths_only` (the inverse) |
| `branch.diff_branches(from, to, ...)` | `branch.diff(project, from_branch, to_branch, { paths_only, ... })` | **Replaced.** |
| `branch.merge(a, b, name, message?, units?)` | `branch.merge(project, name, a, b, { message, units })` | **Changed.** `name` comes before `a` and `b` |
| `branch.publish(branch, commit?)` | `branch.publish(project, branch, commit?)` | **Replaced.** |
| `sandbox.create({ image })` answering a numbered Sandbox | `sandbox.create(project, name, machine?, { image })` | **Changed.** A Sandbox is named, bound to one project and runs on a machine; with no machine, on its sqlite Store's machine. [`sandbox`](../../architecture/api/sdk/sandbox/README.md) |
| `sandbox.list_workspaces(sandbox, ...)` | `sandbox.get(name, { page_size, page_pos })` | **Replaced.** The answer's `workspaces` page |
| `sandbox.close(sandbox)` | `sandbox.close(name, { force })` | **Changed.** `MountsActive` while a Workspace is mounted, unless `force` |
| `workspace.mount(sandbox, branch)` answering `{ sandbox, path }` | `workspace.mount(sandbox, branch, { name })` answering `ws` | **Changed.** `ws` is `<sandbox>:<path>` or `<sandbox>:<name>`; `main` is refused, mount a forked Branch |
| `workspace.exec(ws, command, message?, { stdout?, stderr? })` | `workspace.exec(ws, command, { message, stream, stdout, stderr })` | **Replaced.** See [section 7](#7-progress-and-exec-output) |
| `workspace.commit(ws, message?, force?)`, `workspace.unmount(ws, discard?)` | `workspace.commit(ws, { message, force })`, `workspace.unmount(ws, { discard, force })` | **Replaced.** |
| `backend.daemon.workspace.status(ws)` | `workspace.status(ws)` | **Replaced** (O14). |
| background processes (`process.*` of the backend facade) | none | **Removed** with the backend facade (O25). [Limitations](limitations.md#not-built-refused-as-unsupported) |

Calls with no counterpart in the 1.1.0 method table above: `project.remove`,
`commit.get`, and the `machine`, `store` and `backup` families, plus the
commands `layerfs doctor` and `layerfs import-env`. In all there are 61 calls in
nine families ([coverage](../../architecture/api/call-table/coverage.md)).

## 4. Folder scope

A scope is a client option of every front end: `client({ scope })`,
`client(scope=)`, `--scope --scope-project [--scope-branch]`. Mechanics and
tests: [Table: Scopes](../../architecture/api/call-table/table.md#scopes),
[calls](calls.md#folder-scope).

| Old (#521) | 2.0.0 | Status and reason |
| --- | --- | --- |
| `{ folder, branch? }`, the project implicit | `{ project, folder, branch? }`. Every `file` and `branch` call must name `project` (`out of scope: project`) | **Changed.** The scope is bound to one project |
| `sandbox.create`, `sandbox.close`, `sandbox.list` allowed | refused `InvalidInput`, `out of scope: sandbox.create` and so on. `workspace.*` is allowed only on a Sandbox whose record names the scope's project | **Changed.** A scoped client cannot start a Sandbox |
| `workspace.exec` with a working directory | the command is prefixed `cd '.<folder>' && ...` | **Changed.** A scope guards call arguments, not processes: the command can still reach anywhere in the mounted tree |
| `workspace.mount` of the namespace's `main` | the same; mounted as `<ns>.main`, any other Branch is refused | unchanged |
| `commit.*`, `branch.diff` | refused `out of scope: history` | unchanged |
| `events` and `backend` guards | none | **Removed** with those surfaces |
| (2.0 before the scope fix) `branch.fork` `from_commit` and `branch.publish` `commit` were not scoped | checked like any other reference; an id is left as it is | **Changed** in the base (the scope fix). It can refuse a call that used to pass |

Four gaps are open and undecided by the owner
([#33](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/33)); they are
the "Not confined" table of
[Table: Scopes](../../architecture/api/call-table/table.md#scopes):

1. a 66-hex Commit or Layer id passes as given wherever a reference is taken;
2. such an id placed in a `file` call's `branch` field is accepted;
3. `workspace.commit` on a Workspace of a Branch outside the namespace writes
   to it (`Scope::owns` checks only the Sandbox's project);
4. a scope without `branch` refuses every `file` and `branch` call.

Scope across the three front ends was not run by the coverage checks
([coverage: NOT RUN](../../architecture/api/call-table/coverage.md#not-run)).
The two refusals on a scoped client in [the examples](examples/README.md) were.

## 5. Bytes and integers

| Old | 2.0.0 | Status and reason |
| --- | --- | --- |
| Node `bigint` sent as a `Number` (it lost precision: `2n ** 60n + 1n` became another valid integer) | exact: a `bigint` is sent as its digits, a result is a `number` when safe and a `bigint` otherwise. A `number` beyond ±(2^53 - 1), `NaN` and `Infinity` are refused `InvalidInput` before the call. Node 22 is the floor (`JSON.rawJSON`) | **Changed** (owner decision ([#31](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/31#issuecomment-6657201351)): exact). [Node: Values](../../architecture/api/front-ends/node.md#values) |
| Python `int` (no release had a Python package; #521 only) | exact both ways; past signed 64 bits the library refuses `InvalidInput`, "number out of range"; a float is refused | **New.** [Python](../../architecture/api/front-ends/python.md#package) |
| the shell | a signed 64-bit decimal; larger is exit 2 | [Coverage: differences](../../architecture/api/call-table/coverage.md#where-the-front-ends-differ-on-purpose) |
| bytes as `{ "$layerfs": "bytes", "base64": "..." }` (#521, MCP results) | `Buffer` / `Uint8Array` (Node), `bytes` / `bytearray` / `memoryview` (Python) go in as `{ "base64": "..." }`; the same shape comes back as `Buffer` / `bytes` | **Changed.** The key `base64` is reserved: a map whose only key is `base64` with base64 text is read as bytes. A map with another key beside it stays a map |
| reading bytes | `file.read` returns text only and refuses a file that is not UTF-8 text as `Binary`; bytes can be written | **Not restored** (binary `file.read`, see [section 10](#10-history-features-that-are-gone)) |
| exec output as a string | a `string` when an event's bytes are UTF-8, else a `Buffer` / `bytes`, **per event** | **Changed.** A caller that joins output must accept both ([Events](../../architecture/api/call-table/events.md#workspaceexec-output)) |

## 6. Errors, Unknown and Unresolved

The refusal is `{ code, message, detail }` in every front end: Node throws
`LayerFSError`, Python raises `layerfs.LayerFSError` (`.code`, `.message`,
`.detail`), the command prints the JSON on stderr and exits 125. A wrong command
line is exit 2 with one plain `layerfs: ...` line. The codes are in the
[catalog](../../architecture/api/call-table/errors.md#error-kinds); which of
them are 1.1.0's own (`NoSuchStore`, `NoSuchPath`, `NoSuchSandbox` and others)
is noted in [calls: errors](calls.md#errors-and-exit-codes).

- **Every call is one attempt.** The library never retries, resumes or replays,
  and nothing commits or unmounts on its own.
- **`Unknown`** means the outcome may have happened (a lost answer). Both sides
  are kept and logged; **read the state with `list`, `get` or `status` before
  acting again.**
- **`Unresolved`** means an earlier `Unknown` on that entity stands and this
  call did nothing; the message names the call that shows it and the one that
  clears it ([Errors: uncertain outcomes](../../architecture/api/call-table/errors.md#uncertain-outcomes)).
- A refusal before any effect (bad arguments, unknown option, out of scope) is
  known: nothing happened. The coverage checks list Commits, Branches and
  Stores before and after such a call and compare them.
- Not run: a real `Unknown` or `Unresolved` outcome (it needs a daemon, a fault
  or a second process).

## 7. Progress and exec output

| Old | 2.0.0 | Status and reason |
| --- | --- | --- |
| `events.subscribe` / `poll` / `unsubscribe`, SSE and NDJSON streams (#521); the trace subscription of `@layerfs/backend` (1.1.0) | none | **Removed** (O26). Trace files were meant to be the feed, but `<home>/trace/` is written by nothing and `LAYERFS_TRACE` records only worker connections ([home and records](../../architecture/api/control/home-and-records.md#traces)) |
| job registry: `start_init` / `get_init` / `subscribe_init` | one `on_progress` stream on the call itself | **Removed** (O22). Another process cannot watch a call it did not start; `store status` shows a push or pull in progress |
| progress of any call | `on_progress` on exactly five calls: `project.init`, `store.push`, `store.pull`, `backup.create`, `backup.restore`. The shell form is `--progress` (JSON lines on stderr) | **Changed.** Three payload shapes; events mark the **entry** of a phase, never completion. [Events](../../architecture/api/call-table/events.md) |
| `exec` output | `stdout` / `stderr` functions, `stream: true`, `--stream` | **Changed.** Output is **buffered and delivered after the command ends**, never live; stdout then stderr, not interleaved; a stream over 1 MiB only as event pages (the answer has it `null`, with `stdout_bytes` and `streamed: true`), and dropped when there is no sink. All of this is **UNVERIFIED by run**: no Sandbox could be started here ([Events](../../architecture/api/call-table/events.md#what-was-run-and-not-run)) |
| `--stream` exit code | the command's own code; 125 still means refused | A command that exits 2 or 125 looks like a usage error or a refusal; stderr tells them apart ([CLI: Streams](../../architecture/api/front-ends/cli.md#streams)) |

Seven decisions about this contract are open
([#37](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/37), listed in
[Events: open decisions](../../architecture/api/call-table/events.md#open-decisions)).
A function that throws does not stop a call; a Node `async` function that
rejects ends the process. Cancelling a Python coroutine stops its functions,
not the native call.

## 8. Diagnostics and doctor

| Old | 2.0.0 | Status and reason |
| --- | --- | --- |
| `client.results(after?)`, a ring of per-call results (#521) | none | **Not restored** (owner decision, [#31](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/31#issuecomment-6657201351)) |
| the call trace and operator reads of `@layerfs/backend` | `LAYERFS_TRACE=<file>` appends one `worker.connection` line per connection to a machine's worker; nothing else is traced | **Removed** (O25, O26) |
| a health check | `layerfs doctor [--text]`, `doctor({ home })` (Node), `layerfs.doctor(home)` (Python): outside the call table, reads the home, takes no lock, writes nothing, starts no machine | **New.** [Doctor](../../architecture/api/control/doctor.md), [calls](calls.md#layerfs-doctor) |
| `close()` | `close()` closes idle Stores; the command does it after every call | **Changed** (section 2) |

`doctor` **exits 0 whenever it produces a report**, including a report with
`"ok": false`; `ok` says whether the home is clean. It is 125 only when the home
cannot be named, and 2 for a wrong command line. This is by design, and the
reliability of the checks belongs to the control plane
([#23](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/23)); a strict
exit mode is open ([#36](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/36)).
Both exits were run: [what was run](#what-was-run).

## 9. The Workspace flow

A Workspace is a mounted side Branch where ordinary commands run. The route is
`sandbox.create`, `workspace.mount`, `workspace.exec`, `workspace.commit`,
`workspace.unmount`, `sandbox.close`
([workspace family](../../architecture/api/sdk/workspace/README.md),
[quickstart section 4](quickstart.md#4-a-sandbox-a-workspace-and-a-commit)).

| Old | 2.0.0 | Status and reason |
| --- | --- | --- |
| a single-project MCP session that unmounted at end of input or on a signal (#521) | nothing commits or unmounts for you | **Removed** with MCP. `workspace.unmount` is `Dirty` while changes are uncommitted, unless `discard`; `sandbox.close` is `MountsActive` while a Workspace is mounted, unless `force` |
| `mount` of `main` | refused `InvalidInput`: fork a Branch from it | **Changed** |
| `commit` | one Commit; merges first when the Branch moved since the mount, `force` publishes over a moved head; a conflict writes markers and commits nothing (`Conflict`) | unchanged in meaning |
| the Store profile | Disposable only (WAL, `synchronous=OFF`); Durable is disabled by the owner | unchanged from 1.1.0 |

**NOT RUN.** No Workspace call was made. With the installed command, in a
Disposable home, `layerfs sandbox create shop/site tests native` is refused
`Unavailable`: `no LayerFS daemon at <home dir>/.cache/layerfs/bundle/2.0.0/linux-x86_64/layerfs-daemon; install the bundle`.
An earlier run on this host, with a built daemon, reached
`unsupported runtime capability: user namespaces are not blocked` and stopped
there ([Events: what was run](../../architecture/api/call-table/events.md#what-was-run-and-not-run)).
The route is written out, not run, in
[the examples](examples/README.md#the-workspace-route-not-run).

Back up and move Stores with the existing guides, not with this page:
[Store push and pull](machines-and-stores.md#sqlite-stores) and
[backups](machines-and-stores.md#backups).

## 10. History features that are gone

These existed in #521 and have no 2.0 call. The owner decision of 2026-11-05
(recorded in the [#31](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/31#issuecomment-6657201351) decision record, §0) is to **follow the project baseline**: they
are **not restored in 2.0** and are listed as known gaps in the
[interface report](../../general/interface-report-2.0.0.md#known-gaps). A
named consumer would reopen each one.

| Old (#521) | What 2.0 has instead | Evidence |
| --- | --- | --- |
| `branch.graph` (first-parent graph, fork point, publication state, main's Layer walk) | pieces only: `branch.list` answers each Branch's `base`, `head` and `forked_from`; `commit.list` answers `parent` and `second_parent`. No call lists Layers or main's publications | not in the table ([coverage](../../architecture/api/call-table/coverage.md#the-matrix)); fields seen in the [CLI example](examples/README.md#cli) |
| `changed_paths(commit)` | `commit.diff(project, from_commit, to_commit, { paths_only: true })` for a Commit that has a parent. A root Commit (empty tree) cannot be expressed: UNVERIFIED by run | [`commit.diff`](../../architecture/api/sdk/commit/diff.md) |
| commit author and labels | none. A Commit carries `id`, `branch`, `parent`, `second_parent`, `base`, `root`, `via`, `message`, `merged_from`, `time` and size counters | the `commit.list` output in the [CLI example](examples/README.md#cli); [`commit.list`](../../architecture/api/sdk/commit/list.md) |
| per-call results (`client.results`) | none | section 8 |
| cheap project summary (`get_summary`: Branch and Commit counts, last Commit, size) | `project.get` answers the record only: Store, kind, machine, when recorded | [`project.get`](../../architecture/api/sdk/project/get.md) |
| binary `file.read`, `offset` / `nextOffset` cursor | `file.read` is text only and pages by `start_line` / `end_line`, answering `next_line`; a non-text file is `Binary` | [`file.read`](../../architecture/api/sdk/file/read.md); [coverage: values](../../architecture/api/call-table/coverage.md#what-is-checked-across-the-front-ends) |

Also not restored, by earlier decisions: the backend facade (O25), the event
streams (O26), the Node CLI, MCP (O32). A by-id Commit lookup is a page walk:
`commit.get` reads the Branch's Commits until it finds the id, so its cost grows
with the history ([Limitations](limitations.md#not-built-refused-as-unsupported)).

## What was run

On Linux x64 (Ubuntu under WSL2), Node 24.22.2, Python 3.14.4, rustc 1.85.1,
from the tree pinned in the [interface report](../../general/interface-report-2.0.0.md#pinned-identity),
with the three artifacts built from it and installed into clean directories
outside the checkout. The runnable files and their real output are in
[examples](examples/README.md). The commands of this page that are shell
commands were run: the three install paths (section 1), `layerfs --version`,
`layerfs import-env` (empty home: `{"written":[],"kept":[],"commands":[],"ignored":[]}`, exit 0),
`layerfs doctor` on a clean home (`{"ok":true,"findings":[]}`, exit 0), on a
home with a record that does not parse (`"ok":false`, exit 0, `--text` also
exit 0), and with a relative `LAYERFS_HOME` (exit 125, "LAYERFS_HOME must be an
absolute path"), and the refused `sandbox create` of section 9.

**NOT RUN:** every Sandbox, Workspace and daemon call; macOS, Lima, Apple
Container, Windows, Linux aarch64; PostgreSQL through any front end here;
Python 3.10 to 3.13 and Node 22; a real `Unknown` outcome; folder scope across
the front ends; progress of backup and PostgreSQL routes; `project.export`
([#53](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/53), not
implemented).
