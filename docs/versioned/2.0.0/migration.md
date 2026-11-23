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
| npm `@layerfs/agent` (`import { LayerFS } from "@layerfs/agent"`) | npm `@layerfs/node` (`api/node`): `import layerfs from "@layerfs/node"` or `import { client, LayerFSError } from "@layerfs/node"`. An ES module for Node 22 or later, MIT | **Replaced.** One package, one native addon. [`api/node/package.json`](../../../api/node/package.json), [Node](../../architecture/api/front-ends/bindings.md#layerfs-node) |
| npm `@layerfs/cli` (the Node `layerfs` command; #521: 106 commands) | the native `layerfs` command, one Rust binary: the 83 calls plus `worker`, `help` and `--version` | **Replaced.** It is not an npm package: `layerfs-cli` has `publish = false`. The command counts are not comparable (the old ones included the backend and runtime commands). [CLI](../../architecture/api/front-ends/README.md#the-layerfs-command) |
| npm `@layerfs/backend` (`LayerFSBackend.of(client)`, 14 operator and diagnostic calls) | none, except `layerfs.workspace.status(workspace)` | **Removed** (O25). [Limitations](limitations.md#not-built-refused-as-unsupported), [README](README.md#compatibility-with-110) |
| npm `@layerfs/runtime` (`LayerFSRuntime.open`, worker start and restart, `project.start_init` / `get_init` / `subscribe_init`, Sandbox and volume sweep) | no package. The `machine`, `store`, `project` and `sandbox` families; `project.init` with `on_progress`; `machine start` and `machine stop` by hand (nothing starts a machine on demand, D11) | **Replaced** in part, **Removed** for the job registry (O22) and the sweep. [Machines and Stores](machines-and-stores.md#starting-and-stopping-manual-535-d11), [Node and Python](sdk.md#functions-as-options) |
| npm `@layerfs/mcp` (#521: 50 tools, stdio) | `@layerfs/mcp` in `api/mcp` (`layerfs-mcp`, stdio; not published): one tool per call, named `<family>_<call>`; the default `work` profile lists the 17 `agent` calls, `--profile admin` all 83 | **Replaced.** Generated from the Node package's table. [MCP](../../architecture/api/front-ends/mcp.md) |
| Python `layerfs` (#521: `LayerFS.open/init`, `client.fs.*`) | Python `layerfs`: `import layerfs; layerfs.file.read(...)`, `layerfs.client(home=, scope=)`, and `layerfs.aio` for coroutines. Python 3.10 or later | **Replaced.** [Python](../../architecture/api/front-ends/bindings.md#layerfs-python) |
| `plugins/*` (`dsh-layerfs`, `cursor-agents`, `pi-layerfs`, `magpie-layerfs`, `herdr-layerfs`, `codex-parallel-verify`) | not migrated | **Unavailable.** Their documents still name `@layerfs/agent`, `@layerfs/runtime` or `@layerfs/backend` and break on 2.0 until each is rewritten |

### Install

None of the three artifacts is published. Build and install the files, here
into `$OUT` (the steps are in
[api/README.md](../../../api/README.md#package-and-install); they were not
rerun for this version of the page, see [what was run](#what-was-run)):

```bash
# Node: needs the addon first; npm pack refuses to pack without it
npm --prefix api run build:native
(cd api/node && npm pack --pack-destination "$OUT")      # the @layerfs/node tarball
npm install "$OUT"/layerfs-node-*.tgz                    # in an empty project

# Python: a wheel for a virtualenv
(cd api/python && maturin build --release --locked -o "$OUT")
pip install "$OUT"/layerfs-2.0.0-*.whl

# the command: one file
cargo +1.85.1 build --release --locked -p layerfs-cli    # target/release/layerfs
```

PostgreSQL is built into every artifact, so no build flag selects it; only TLS
is optional (`--features tls`), and a call that needs an `https://` remote is
refused `Unsupported` without it. Each artifact needs the glibc of the host
that built it (2.39 on the host used). See
[artifact-qualification-2.0.0.md](../../general/artifact-qualification-2.0.0.md).

## 2. Constructors, settings and entry points

| Old | 2.0.0 | Status and reason |
| --- | --- | --- |
| `LayerFS.open(name, { home, storePath, deploy, storePolicy, trace, scope })` | `client({ home, scope })` (Node), `client(home=, scope=)` (Python). The default export and the family exports use `client()`. Every call names its project as `"<store>/<project>"` | **Replaced.** There is no default project and no per-client Store, deploy or policy option. [Node and Python](sdk.md), [README](README.md#compatibility-with-110) |
| `LayerFS.init(name, folder, timeoutMs)` | `store.create(store)`, then `project.init("<store>/<project>", folder, { timeout, on_progress })` | **Replaced.** `LayerFS.init` and `LayerFS.open` have no alias (D5). [`project.init`](../../architecture/api/sdk/project.md#projectinit) |
| `client.call(method, args)`, `LayerFS.call` | not exported. Node's default export has `table`, `families`, `client`, `close`, `LayerFSError` and the ten families (the named exports are nine families, without `home`); Python exports `TABLE`, `FAMILIES`, `client`, `close`, `LayerFSError` and the ten families | **Removed** from the packages. Rust callers use `layerfs_api::call_scoped`. [`index.js`](../../../api/node/src/index.js), [Table](../../architecture/api/calls/README.md#what-it-does) |
| `await client.close()`, `Symbol.asyncDispose` (#521) | module-level `layerfs.close()` (Node) and `layerfs.close()` (Python): closes the Stores this process holds idle, answers how many | **Changed.** There is no client object to dispose; call `close()` before the process ends. [Node and Python](sdk.md) |
| `LAYERFS_STORE_PATH`, `storePath` | none. A Store lives under the home (`store/<name>/`); `store.create`, `store.connect` and `store.move` make and move Stores | **Removed.** [The home](home.md), [Machines and Stores](machines-and-stores.md#stores) |
| `LAYERFS_WORKER_ENDPOINT`, worker started by `@layerfs/runtime` | none. The home's machine records say where a Store's worker is; `layerfs worker` serves it | **Removed.** [Machines and Stores](machines-and-stores.md#routes) |
| `LAYERFS_DAEMON`, `_IMAGE_ARCHIVE`, `_DOCKER`, `_IMAGE`, `_REMOTE_*`, `_SANDBOX`, `_PLACEMENT` and the other 1.1.0 platform settings | records under `LAYERFS_HOME`, written by `machine create` and `machine update`; a 1.1.0 environment is not imported (owner, 2026-11-19) | **Replaced.** Only `LAYERFS_HOME`, `LAYERFS_TRACE` and the measurement settings are read. [README](README.md#compatibility-with-110) |
| `LAYERFS_STORE_POLICY` (`storePolicy`) | none. Every sqlite Store opens with `synchronous=NORMAL` | **Removed.** Durable execution stays disabled by the owner |
| init timeout (`LayerFS.init` third argument) | the `timeout` option of `project.init` | **Replaced.** There is no generic per-call deadline: every call is one attempt |
| 1.1.0 homes and Stores | not converted (owner, 2026-11-19): refused with the fix `mv <home> <home>.old && layerfs home init` | **Removed.** [The home](home.md#an-older-home) |

## 3. Methods

2.0 has one call table of 83 calls in ten families, the same in the shell,
Node, Python and MCP ([the calls](calls.md)). Positionals go from the outside
in: project, Branch, path or new name, other inputs; anything optional is an
option, and a Commit to read at is the option `at`. Every call takes the
project first. Node and Python take the options as a trailing object or as
keywords; 1.1.0 methods took positionals only. Result keys are snake_case
in every front end (`exit_code`, not `exitCode`). Every list takes `limit`
and `cursor` and answers `{ items, next_cursor }`. Every call answers a
short form; `detail: true` asks the long form
([the form of an answer](calls.md#the-form-of-an-answer)).

| Old (1.1.0) | 2.0.0 | Notes |
| --- | --- | --- |
| `project.list_branches(pageSize?, pagePos?)` | `branch.list(project, { prefix, exclude_prefix, limit, cursor })` | **Replaced.** [`branch.list`](../../architecture/api/sdk/branch.md#branchlist) |
| `project.list_sandboxes(pageSize?, pagePos?)` | `sandbox.list({ project, machine, limit, cursor })` | **Replaced.** |
| `fs.read(ref, path, startLine?, endLine?)` | `file.read(project, branch, path, { at, line, limit })` | **Replaced.** A `ref` was `main`, a Branch, a Commit id or a Layer id. Now the Branch is always given, and `at` names one Commit of its history (a full id or a prefix of 8 or more). A file that is not text answers `{ binary: true, size }` ([`file.read`](../../architecture/api/sdk/file.md#fileread)) |
| `fs.list`, `fs.stat` | `file.list(project, branch, path, { at, limit, cursor })`, `file.stat(project, branch, path, { at })` | **Replaced.** |
| `fs.grep(ref, pattern, path?, ...)`, `fs.glob(ref, pattern, path?, ...)` | `file.grep(project, branch, pattern, path?, { include, exclude, ignore_case, at, limit, cursor })`, `file.glob(project, branch, pattern, path?, { exclude, at, limit, cursor })` | **Replaced.** `path` defaults to `/`; `grep`'s pattern is literal text |
| `fs.blame(ref, path, fromLine?)` | `file.blame(project, branch, path, { at, line, limit })` | **Replaced.** |
| `fs.diff_files(leftRef, leftPath, rightRef, rightPath)` | `commit.diff(project, ref, { from, path, with: ["patch"] })` | **Changed.** One path between two Commits or Branches; comparing two different paths is gone (C7) |
| `fs.write(branch, path, content, message?)` | `file.write(project, branch, path, content, { mode, expected, message })` | **Replaced.** `main` is refused `InvalidInput` with a `fix` that forks a Branch: it changes only through `branch.publish` |
| `fs.edit(branch, path, oldText, newText, message?)` | `file.edit(project, branch, path, { old_string, new_string, replace_all, expected, message })` | **Changed.** `old_string` and `new_string` are required options |
| `fs.apply(branch, changes, message?)` | `file.apply(project, branch, changes, { expected, message })` | **Replaced.** A change is `{ op, … }` with that op's fields: `write` (`path`, `content`, `mode`), `mkdir`, `chmod`, `rm`, `cp` and `mv` (`from`, `to`), `edit` (`path`, `old_string`, `new_string`, `replace_all`) |
| `fs.mkdir`, `fs.rm`, `fs.cp`, `fs.mv` | `file.mkdir(project, branch, path, { mode })`, `file.rm(project, branch, path, { recursive })`, `file.cp(project, branch, from, to)`, `file.mv(project, branch, from, to)` | **Replaced.** Each takes `expected` and `message` |
| `branch.fork(name, fromRef?)` | `branch.fork(project, name, from?, { at })` | **Changed.** `from` is a Branch (`main` when left off); `at` one Commit of its history |
| `branch.list_commits(branch, pageSize?, pagePos?)` | `commit.list(project, branch, { at, limit, cursor })` | **Replaced.** [`commit.list`](../../architecture/api/sdk/commit.md#commitlist) |
| `branch.diff_commits(from, to, ..., { cursor, hunks })` | `commit.diff(project, ref, { from, path, limit, cursor, with: ["patch"] })` | **Changed.** `hunks` is the section `with: ["patch"]` |
| `branch.diff_branches(from, to, ...)` | `commit.diff(project, ref, { from })` | **Replaced.** A ref is a Branch name first, then a Commit |
| `branch.merge(a, b, name, message?, units?)` | `branch.merge(project, branch, from, { into, units, expected, message })` | **Changed.** Merges `from` into `branch`, or with `into` into a new Branch (1.1.0's form) |
| `branch.publish(branch, commit?)` | `branch.publish(project, branch, { at, expected, force })` | **Replaced.** |
| `sandbox.create({ image })` answering a numbered Sandbox | `sandbox.create(project, name, { machine, image, mount })` | **Changed.** A Sandbox is named, bound to one project and runs on a machine: with no `machine`, on its sqlite Store's machine, else `native`. [`sandbox`](../../architecture/api/sdk/sandbox.md) |
| `sandbox.list_workspaces(sandbox, ...)` | `sandbox.get(sandbox)` (its `workspaces`), `workspace.list({ sandbox, limit, cursor })` | **Replaced.** |
| `sandbox.close(sandbox)` | `sandbox.remove(sandbox, { force })` | **Changed.** `InUse` while a Workspace is mounted, `Busy` while a command runs, unless `force` |
| `workspace.mount(sandbox, branch)` answering `{ sandbox, path }` | `workspace.mount(sandbox, branch, { name })` answering `{ workspace, path }` | **Changed.** `workspace` is `<sandbox>:<name>`, the name the Branch's unless given; `main` is refused, mount a forked Branch |
| `workspace.exec(ws, command, message?, { stdout?, stderr? })` | `workspace.exec(workspace, command, { cwd, env, stdin, stream, stdout, stderr, on_progress })` | **Replaced.** See [section 7](#7-progress-and-exec-output) |
| `workspace.commit(ws, message?, force?)`, `workspace.unmount(ws, discard?)` | `workspace.commit(workspace, { message, expected, force })`, `workspace.unmount(workspace, { discard, force })` | **Replaced.** |
| `backend.daemon.workspace.status(ws)` | `workspace.status(workspace, { path })` | **Replaced** (O14). |
| background processes (`process.*` of the backend facade) | none | **Removed** with the backend facade (O25). [Limitations](limitations.md#not-built-refused-as-unsupported) |

Calls with no counterpart in the 1.1.0 method table above: the `home`
family, `project.delete`, `project.hide_path`, `project.export`, the six
doctors, `commit.get`, `workspace.cancel`, and the `machine`, `store` and
`backup` families, plus the command `layerfs worker`
([coverage](../../architecture/api/calls/coverage.md)).

### Calls and options renamed in 2.0

The 2.0 table replaced the call names and options of the 2.0 previews (the
table before the call surface was frozen, `caff58a43b`). Old names are gone
with **no alias** (ruling R1): an old call is refused as an unknown call
(exit 2 in the shell), an old option as an unknown option. The removed
calls:

| Removed | Use |
| --- | --- |
| `branch.commits` | `commit.list` |
| `branch.get_commit` | `commit.get` (no Branch positional; `--branch B` checks the Commit is in `B`'s history) |
| `branch.diff` | `commit.diff <project> <ref> [--from R]` |
| `file.diff` | `commit.diff --path P` (one path between two Commits) |
| `file.revision` | `file.history` |
| `project.remove_file` | `project.hide_path` (the engine method keeps its name) |
| `sandbox.close` | `sandbox.remove` |
| `workspace.exec_cancel` | `workspace.cancel <workspace> [--exec E]` |
| `logs.list`, `logs.reset` | `home.log`, `home.clear_log` |
| `logs.sandbox`, `logs.machine` | `sandbox.log`, `machine.log` |
| `machine.status` | `machine.get --with live` |
| `machine.space` | `machine.stats --with space` |
| `machine.load` | `machine.load_image` |
| `machine.platforms` | `home.get --with platforms` |
| `store.status`, `store.stats` | `store.get --with live`, `store.get --with stats` |
| `store.push`, `store.pull` | `store.move <store> <machine>` (to or from this computer) |
| `doctor.fix` | none: LayerFS removes leftovers itself (v20) |
| `layerfs doctor` (outside the table), Node `doctor()`, Python `layerfs.doctor()` | the six doctors `home.doctor`, `project.doctor`, `sandbox.doctor`, `machine.doctor`, `store.doctor`, `backup.doctor` |

Every call of the table, its shell line and what it was:

| 2.0 call | Shell | Was |
| --- | --- | --- |
| `project.init` | `layerfs project init <project> <folder> [--timeout 3600000]` | same name |
| `project.list` | `layerfs project list [--store S] [--with stats] [--limit 100] [--cursor C]` | same name |
| `project.get` | `layerfs project get <project>` | same name |
| `project.stats` | `layerfs project stats <project> [--branch B] [--at C]` | same name |
| `project.export` | `layerfs project export <project> <destination> [--branch B] [--at C]` | same name |
| `project.delete` | `layerfs project delete <project>` | same name |
| `project.hide_path` | `layerfs project hide-path <project> <path>` | `project.remove_file` |
| `project.doctor` | `layerfs project doctor <project> [--full]` | new |
| `file.read` | `layerfs file read <project> <branch> <path> [--at C] [--line 1] [--limit 2000]` | same name |
| `file.stat` | `layerfs file stat <project> <branch> <path> [--at C]` | same name |
| `file.list` | `layerfs file list <project> <branch> <path> [--at C] [--limit 100] [--cursor C]` | same name |
| `file.glob` | `layerfs file glob <project> <branch> <pattern> [path] [--exclude G] [--at C] [--limit 100] [--cursor C]` | same name |
| `file.grep` | `layerfs file grep <project> <branch> <pattern> [path] [--include G] [--exclude G] [--ignore-case] [--at C] [--limit 100] [--cursor C]` | same name |
| `file.history` | `layerfs file history <project> <branch> <path> [--at C] [--traversal provenance] [--limit 100] [--cursor C]` | `file.revision` |
| `file.blame` | `layerfs file blame <project> <branch> <path> [--at C] [--line 1] [--limit 2000]` | same name |
| `file.write` | `layerfs file write <project> <branch> <path> [--mode M] [--expected C] [-m msg] < content` | same name |
| `file.edit` | `layerfs file edit <project> <branch> <path> --old-string S --new-string S [--replace-all] [--expected C] [-m msg]` | same name |
| `file.rm` | `layerfs file rm <project> <branch> <path> [--recursive] [--expected C] [-m msg]` | same name |
| `file.mkdir` | `layerfs file mkdir <project> <branch> <path> [--mode M] [--expected C] [-m msg]` | same name |
| `file.mv` | `layerfs file mv <project> <branch> <from> <to> [--expected C] [-m msg]` | same name |
| `file.cp` | `layerfs file cp <project> <branch> <from> <to> [--expected C] [-m msg]` | same name |
| `file.apply` | `layerfs file apply <project> <branch> [--expected C] [-m msg] < changes.json` | same name |
| `sandbox.create` | `layerfs sandbox create <project> <name> [--machine M] [--image I] [--mount B]` | same name |
| `sandbox.list` | `layerfs sandbox list [--project P] [--machine M] [--limit 100] [--cursor C]` | same name |
| `sandbox.get` | `layerfs sandbox get <sandbox> [--with stats]` | same name |
| `sandbox.remove` | `layerfs sandbox remove <sandbox> [--force]` | `sandbox.close` |
| `sandbox.files` | `layerfs sandbox files <sandbox> [path] [--depth 1] [--limit 100] [--cursor C] [--byte-offset 0] [--byte-length 262144]` | same name |
| `sandbox.log` | `layerfs sandbox log <sandbox> [--limit 200] [--cursor C]` | `logs.sandbox` |
| `sandbox.doctor` | `layerfs sandbox doctor <sandbox>` | new |
| `workspace.list` | `layerfs workspace list [--sandbox S] [--limit 100] [--cursor C]` | new |
| `workspace.mount` | `layerfs workspace mount <sandbox> <branch> [--name N]` | same name |
| `workspace.exec` | `layerfs workspace exec <workspace> <command> [--cwd D] [--env K=V] [--stdin] [--stream]` | same name |
| `workspace.cancel` | `layerfs workspace cancel <workspace> [--exec E]` | `workspace.exec_cancel` |
| `workspace.commit` | `layerfs workspace commit <workspace> [-m msg] [--expected C] [--force]` | same name |
| `workspace.unmount` | `layerfs workspace unmount <workspace> [--discard] [--force]` | same name |
| `workspace.status` | `layerfs workspace status <workspace> [--path P]` | same name |
| `branch.list` | `layerfs branch list <project> [--prefix P] [--exclude-prefix P] [--limit 100] [--cursor C]` | same name |
| `branch.get` | `layerfs branch get <project> <branch>` | same name |
| `branch.fork` | `layerfs branch fork <project> <name> [from] [--at C]` | same name |
| `branch.merge` | `layerfs branch merge <project> <branch> <from> [--into NEW] [--units JSON] [--expected C] [-m msg]` | same name |
| `branch.publish` | `layerfs branch publish <project> <branch> [--at C] [--expected C] [--force]` | same name |
| `branch.remove` | `layerfs branch remove <project> <branch>` | same name |
| `commit.list` | `layerfs commit list <project> <branch> [--at C] [--limit 100] [--cursor C]` | `commit.list`, `branch.commits` |
| `commit.get` | `layerfs commit get <project> <commit> [--branch B]` | `commit.get`, `branch.get_commit` |
| `commit.diff` | `layerfs commit diff <project> <ref> [--from R] [--path P] [--with patch] [--limit 100] [--cursor C]` | `commit.diff`, `branch.diff`, `file.diff` |
| `home.init` | `layerfs home init` | same name |
| `home.get` | `layerfs home get [--with platforms,tree]` | `home.get`, `machine.platforms` |
| `home.log` | `layerfs home log [--call C] [--project P] [--client C] [--tag T] [--outcome O] [--since T] [--limit 200] [--cursor C]` | `logs.list` |
| `home.clear_log` | `layerfs home clear-log` | `logs.reset` |
| `home.doctor` | `layerfs home doctor` | new |
| `machine.list` | `layerfs machine list [--with live]` | same name |
| `machine.get` | `layerfs machine get <machine> [--with live]` | `machine.get`, `machine.status` |
| `machine.images` | `layerfs machine images <machine> [--with docker]` | same name |
| `machine.load_image` | `layerfs machine load-image <machine> <image>` | `machine.load` |
| `machine.prune` | `layerfs machine prune <machine> [image] [--unused]` | same name |
| `machine.stats` | `layerfs machine stats <machine> [--with space]` | `machine.stats`, `machine.space` |
| `machine.files` | `layerfs machine files <machine> [path] [--depth 1] [--limit 100] [--cursor C] [--byte-offset 0] [--byte-length 262144]` | same name |
| `machine.log` | `layerfs machine log <machine> [--limit 200] [--cursor C]` | `logs.machine` |
| `machine.start` | `layerfs machine start <machine>` | same name |
| `machine.stop` | `layerfs machine stop <machine> [--force]` | same name |
| `machine.create` | `layerfs machine create <name> <platform> [--cpus 2] [--memory 4G] [--disk 20G] [--image I] [--os-image I]` | same name |
| `machine.remove` | `layerfs machine remove <machine>` | same name |
| `machine.update` | `layerfs machine update <machine> [--cpus N] [--memory M] [--image I] [--docker S] [--wsl D] [--daemon P]` | same name |
| `machine.doctor` | `layerfs machine doctor <machine>` | new |
| `store.list` | `layerfs store list [--machine M] [--with stats]` | same name |
| `store.get` | `layerfs store get <store> [--with live,stats]` | `store.get`, `store.status`, `store.stats` |
| `store.create` | `layerfs store create <name> [--machine M]` | same name |
| `store.connect` | `layerfs store connect <store> <url> [--auth A] [--machine M] [--synchronous-commit local]` | same name |
| `store.disconnect` | `layerfs store disconnect <store> [--machine M]` | same name |
| `store.save_auth` | `layerfs store save-auth <auth> --user U --password-stdin` | new |
| `store.auths` | `layerfs store auths` | new |
| `store.remove_auth` | `layerfs store remove-auth <auth>` | new |
| `store.update` | `layerfs store update <store> [--auth A] [--synchronous-commit S]` | same name |
| `store.move` | `layerfs store move <store> <machine>` | `store.push`, `store.pull` |
| `store.remove` | `layerfs store remove <store>` | same name |
| `store.compact` | `layerfs store compact <store>` | same name |
| `store.doctor` | `layerfs store doctor <store> [--machine M]` | new |
| `backup.connect` | `layerfs backup connect <name> <url> --access-key K --key-stdin` | same name |
| `backup.disconnect` | `layerfs backup disconnect <bucket>` | same name |
| `backup.list` | `layerfs backup list [--bucket B]` | same name |
| `backup.create` | `layerfs backup create <store> <bucket> [--file F]` | same name |
| `backup.restore` | `layerfs backup restore <bucket> <file> <store>` | same name |
| `backup.doctor` | `layerfs backup doctor <bucket> [--full]` | new |

Every argument the table renamed or dropped, from the frozen rows
(`calls.json` `dropped`):

| Call | 1.x argument | 2.0 | Why |
| --- | --- | --- | --- |
| `project.list` | `[store]` | `--store S` | anything optional is an option |
| `project.list` | `--page-size` | `--limit` | one paging convention |
| `project.list` | `--page-pos` | `--cursor` | one paging convention |
| `project.stats` | `[branch]` | `--branch B` | anything optional is an option |
| `project.stats` | `[commit]` | `--at C` | one way to pin a read |
| `file.read` | `[commit]` | `--at C` | one way to pin a read; an optional Commit is an option |
| `file.read` | `--start-line` | `--line` | a 1-based line window |
| `file.read` | `--end-line` | `--limit` | a count of lines, as every window and page takes |
| `file.stat` | `[commit]` | `--at C` | one way to pin a read; an optional Commit is an option |
| `file.list` | `[commit]` | `--at C` | one way to pin a read; an optional Commit is an option |
| `file.list` | `--page-size` | `--limit` | one paging convention |
| `file.list` | `--page · --page-pos` | `--cursor` | one paging convention |
| `file.glob` | `<path>` | `[path]` | the pattern comes first; the folder follows it and defaults to / |
| `file.glob` | `[commit]` | `--at C` | one way to pin a read; an optional Commit is an option |
| `file.glob` | `--page-size` | `--limit` | one paging convention |
| `file.glob` | `--page · --page-pos` | `--cursor` | one paging convention |
| `file.grep` | `<path>` | `[path]` | the pattern comes first; the folder follows it and defaults to / |
| `file.grep` | `[commit]` | `--at C` | one way to pin a read; an optional Commit is an option |
| `file.grep` | `--page-size` | `--limit` | one paging convention |
| `file.grep` | `--page · --page-pos` | `--cursor` | one paging convention |
| `file.history` | `--at-commit` | `--at C` | one way to pin a read |
| `file.history` | `--page-size` | `--limit` | one paging convention |
| `file.history` | `--page · --continuation` | `--cursor` | one paging convention |
| `file.blame` | `[commit]` | `--at C` | one way to pin a read; an optional Commit is an option |
| `file.blame` | `--from-line` | `--line` | a 1-based line window |
| `file.blame` | `--line-count` | `--limit` | a count of lines, as every window and page takes |
| `file.edit` | `--old` | `--old-string S` | the name agents' edit tools use |
| `file.edit` | `--new` | `--new-string S` | the name agents' edit tools use |
| `file.mv` | `<from_path>` | `<from>` | shorter; the position says it is a path |
| `file.mv` | `<to_path>` | `<to>` | shorter; the position says it is a path |
| `file.cp` | `<from_path>` | `<from>` | shorter; the position says it is a path |
| `file.cp` | `<to_path>` | `<to>` | shorter; the position says it is a path |
| `sandbox.create` | `[machine]` | `--machine M` | anything optional is an option |
| `sandbox.list` | `[project]` | `--project P` | anything optional is an option |
| `sandbox.list` | `--page-size` | `--limit` | one paging convention |
| `sandbox.list` | `--page-pos` | `--cursor` | one paging convention |
| `sandbox.list` | `--live` | gone | a state comes from records; `sandbox get` asks one Sandbox and `machine stats` counts a machine's running daemons |
| `sandbox.get` | `<name>` | `<sandbox>` | a thing goes by its own word |
| `sandbox.get` | `--page-size · --page-pos` | gone | no paging: a Sandbox holds at most 16 Workspaces |
| `sandbox.remove` | `<name>` | `<sandbox>` | a thing goes by its own word |
| `sandbox.files` | `--offset` | `--byte-offset` | a byte window says bytes |
| `sandbox.files` | `--length` | `--byte-length` | a byte window says bytes |
| `sandbox.log` | `--before` | `--cursor` | one paging convention: the cursor reads the lines before a page; today's `--cursor` read the lines after it |
| `sandbox.doctor` | `--timeout (layerfs doctor sandbox)` | gone | each thing has its own time budget (30 s for the whole call today) |
| `sandbox.doctor` | `--text (layerfs doctor sandbox)` | `--text` | the global option, as on every call |
| `workspace.exec` | `<ws>` | `<workspace>` | a thing goes by its own word; the handle is always `sandbox:name` |
| `workspace.exec` | `--message` | gone | it was only echoed back in the result |
| `workspace.exec` | `--exec-id` | gone | LayerFS makes the id (`exec-` and 16 hex digits); the progress event and the long form name it |
| `workspace.cancel` | `<ws>` | `<workspace>` | a thing goes by its own word; the handle is always `sandbox:name` |
| `workspace.cancel` | `<exec_id>` | `--exec E` | optional now: the default is every running command |
| `workspace.cancel` | `--grace-ms` | gone | one grace, 5000 ms, today's default |
| `workspace.commit` | `<ws>` | `<workspace>` | a thing goes by its own word; the handle is always `sandbox:name` |
| `workspace.unmount` | `<ws>` | `<workspace>` | a thing goes by its own word; the handle is always `sandbox:name` |
| `workspace.status` | `<ws>` | `<workspace>` | a thing goes by its own word; the handle is always `sandbox:name` |
| `branch.list` | `--page-size` | `--limit` | one paging convention |
| `branch.list` | `--page · --page-pos` | `--cursor` | one paging convention |
| `branch.fork` | `<from_branch>` | `[from]` | optional now; main when left off |
| `branch.fork` | `[from_commit]` | `--at C` | one way to name a Commit |
| `branch.merge` | `<name>` | `--into NEW` | the new Branch is optional now |
| `branch.merge` | `<a>` | `<branch>` | the Branch merged into; its rules apply |
| `branch.merge` | `<b>` | `<from>` | the Branch merged from |
| `branch.publish` | `[commit]` | `--at C` | one way to name a Commit |
| `branch.publish` | `--expected-base` | `--expected C` | the one write guard; it guards main's head |
| `branch.publish` | `--expected-head` | `--at C` | naming the Commit to publish pins what is published; a moved Branch head cannot change it |
| `commit.list` | `--page-size` | `--limit` | one paging convention |
| `commit.list` | `--page · --page-pos` | `--cursor` | one paging convention |
| `commit.list` | `--at-commit` | `--at C` | one way to pin a read |
| `commit.list` | `--include-storage` | `--detail` | the byte counts are in every Commit record; the long row always carries them |
| `commit.get` | `<branch>` | `--branch B` | a Commit id names its project's Commit without a Branch |
| `commit.diff` | `<from_commit> (commit.diff) · <from_branch> (branch.diff) · --from-commit (branch.diff)` | `--from R` | one optional side, a Branch or a Commit |
| `commit.diff` | `<to_commit> (commit.diff) · <to_branch> (branch.diff) · --to-commit (branch.diff)` | `<ref>` | one required side, a Branch or a Commit |
| `commit.diff` | `--paths-only` | `(default) · --with patch` | paths are the default; the lines are the opt-in |
| `commit.diff` | `--page-size` | `--limit` | one paging convention |
| `commit.diff` | `--page · --page-pos · --continuation` | `--cursor` | one paging convention |
| `commit.diff` | `--path-prefix` | `--path P` | one file or one folder |
| `commit.diff` | `--include-attribution` | `file blame` | line ownership is `file blame` on each side; a diff no longer answers it |
| `commit.diff` | `<left_branch> · --left-commit` | `--from R` | file.diff's left side |
| `commit.diff` | `<right_branch> · --right-commit` | `<ref>` | file.diff's right side |
| `commit.diff` | `<left_path> · <right_path>` | `--path P` | one path on both sides; comparing two different paths is dropped |
| `commit.diff` | `--allow-missing` | `(dropped)` | a path present on one side only is a row with change added or removed |
| `home.log` | `--op` | `--call` | `op` is only ever the state companion; a log row names its call |
| `home.doctor` | `--timeout (layerfs doctor host)` | gone | each thing has its own time budget (30 s for a whole call today, 10 s for a connection check) |
| `home.doctor` | `--quick (layerfs doctor store integrity)` | gone | the whole-Store scan leaves the doctors; `store doctor` stops after the schema check |
| `home.doctor` | `--write-test (layerfs doctor backup connection)` | `backup doctor --full` | R5 |
| `home.doctor` | `--text (layerfs doctor)` | `--text` | the global option, as on every call |
| `machine.get` | `<name> (machine.get, machine.status)` | `<machine>` | an existing thing is named by its own word, never `name` |
| `machine.images` | `<name>` | `<machine>` | an existing thing is named by its own word, never `name` |
| `machine.load_image` | `<name>` | `<machine>` | an existing thing is named by its own word, never `name` |
| `machine.prune` | `<name>` | `<machine>` | an existing thing is named by its own word, never `name` |
| `machine.files` | `--offset` | `--byte-offset` | a byte window, named as the Windows rule names it |
| `machine.files` | `--length` | `--byte-length` | a byte window, named as the Windows rule names it |
| `machine.log` | `--before` | `--cursor` | one paging convention: next_cursor in, the page before it out |
| `machine.start` | `<name>` | `<machine>` | an existing thing is named by its own word, never `name` |
| `machine.stop` | `<name>` | `<machine>` | an existing thing is named by its own word, never `name` |
| `machine.remove` | `<name>` | `<machine>` | an existing thing is named by its own word, never `name` |
| `machine.update` | `<name>` | `<machine>` | an existing thing is named by its own word, never `name` |
| `store.get` | `<name> (store.get, store.status, store.stats)` | `<store>` | an existing thing is named by its own word, never `name` |
| `store.get` | `--machine (store.status)` | `store doctor <store> --machine M` | whether one machine reaches a Store is a check, and the doctor makes it |
| `store.connect` | `<name>` | `<store>` | an existing thing is named by its own word, never `name` |
| `store.connect` | `--password` | `--auth` | a login is saved once in the home and named by any number of Stores |
| `store.disconnect` | `<name>` | `<store>` | an existing thing is named by its own word, never `name` |
| `store.update` | `<name>` | `<store>` | an existing thing is named by its own word, never `name` |
| `store.update` | `--url` | `store connect <store> <url>` | an address change is a connect, checked like one |
| `store.update` | `--machine` | `store connect <store> <url> --machine M` | a machine's own address is a connect; the login is the Store's |
| `store.update` | `--password` | `--auth, or store save-auth to rotate` | the login is saved once and named |
| `store.update` | `--clear` | `store disconnect <store> --machine M` | removing an address is a disconnect |
| `store.move` | `<name> (store.push, store.pull)` | `<store>` | an existing thing is named by its own word, never `name` |
| `store.remove` | `<name>` | `<store>` | an existing thing is named by its own word, never `name` |
| `store.compact` | `<name>` | `<store>` | an existing thing is named by its own word, never `name` |
| `backup.connect` | `--key` | `--access-key · --key-stdin` | secrets come only on stdin; today's value is a reference (env:NAME or file:/path) to a key pair |
| `backup.connect` | `--region` | `(dropped)` | r2 is always auto and an S3 endpoint keeps us-east-1, today's defaults; no other region in 2.0 |
| `backup.connect` | `--part-size` | `(dropped)` | the upload piece size stays at its default |
| `backup.disconnect` | `<name>` | `<bucket>` | an existing thing is named by its own word, never `name` |
| `backup.list` | `[bucket]` | `--bucket B` | anything optional is an option |
| `backup.create` | `[file]` | `--file F` | anything optional is an option |
| `backup.restore` | `[file]` | `<file>` | a restore names the file it reads; today it is the Store's linked file, else its name, when left out |

## 4. Folder scope

A scope is a client option of every front end: `client({ scope })`,
`client(scope=)`, `--scope --scope-project [--scope-branch]`. Mechanics and
tests: [Table: Scopes](../../architecture/api/calls/README.md#folder-scope),
[calls](calls.md#folder-scope).

| Old (#521) | 2.0.0 | Status and reason |
| --- | --- | --- |
| `{ folder, branch? }`, the project implicit | `{ project, folder, branch? }`. Every `file` and `branch` call must name `project` (`out of scope: project`) | **Changed.** The scope is bound to one project |
| `sandbox.create`, `sandbox.close`, `sandbox.list` allowed | refused `InvalidInput`, `out of scope: sandbox.create` and so on; so is `workspace.list`. The other `workspace` calls are allowed only on a Sandbox whose record names the scope's project | **Changed.** A scoped client cannot start a Sandbox |
| `workspace.exec` with a working directory | the command is prefixed `cd '.<folder>' && ...` | **Changed.** A scope guards call arguments, not processes: the command can still reach anywhere in the mounted tree |
| `workspace.mount` of the namespace's `main` | the same; mounted as `<ns>.main`, any other Branch is refused | unchanged |
| `commit.*`, `file.history` | refused `out of scope: history` | unchanged |
| `events` and `backend` guards | none | **Removed** with those surfaces |
| (2.0 before the scope fix) `branch.fork` `from_commit` and `branch.publish` `commit` were not scoped | `at` is checked like any other reference; a 66-hex id is left as it is | **Changed** in the base (the scope fix). It can refuse a call that used to pass |

Four gaps are open and undecided by the owner
([#33](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/33)); they are
the "Not confined" table of
[Table: Scopes](../../architecture/api/calls/README.md#folder-scope):

1. a 66-hex Commit or Layer id passes as given wherever a reference is taken;
2. such an id placed in a `file` call's `branch` field is accepted;
3. `workspace.commit` on a Workspace of a Branch outside the namespace writes
   to it (`Scope::owns` checks only the Sandbox's project);
4. a scope without `branch` refuses every `file` and `branch` call.

Scope across the three front ends was not run by the coverage checks
([coverage: what is checked](../../architecture/api/calls/coverage.md#what-is-checked-across-the-front-ends)).
The two refusals on a scoped client in [the examples](examples/README.md) were.

## 5. Bytes and integers

| Old | 2.0.0 | Status and reason |
| --- | --- | --- |
| Node `bigint` sent as a `Number` (it lost precision: `2n ** 60n + 1n` became another valid integer) | exact: a `bigint` is sent as its digits, a result is a `number` when safe and a `bigint` otherwise. A `number` beyond ±(2^53 - 1), `NaN` and `Infinity` are refused `InvalidInput` before the call. Node 22 is the floor (`JSON.rawJSON`) | **Changed** (owner decision ([#31](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/31#issuecomment-6657201351)): exact). [Node: Values](../../architecture/api/front-ends/bindings.md#layerfs-node) |
| Python `int` (no release had a Python package; #521 only) | exact both ways; past signed 64 bits the library refuses `InvalidInput`, "number out of range"; a float is refused | **New.** [Python](../../architecture/api/front-ends/bindings.md#layerfs-python) |
| the shell | a signed 64-bit decimal; larger is exit 2 | [Coverage: differences](../../architecture/api/calls/coverage.md#where-the-front-ends-differ-on-purpose) |
| bytes as `{ "$layerfs": "bytes", "base64": "..." }` (#521, MCP results) | `Buffer` / `Uint8Array` (Node), `bytes` / `bytearray` / `memoryview` (Python) go in as `{ "base64": "..." }`; the same shape comes back as `Buffer` / `bytes` | **Changed.** The key `base64` is reserved: a map whose only key is `base64` with base64 text is read as bytes. A map with another key beside it stays a map |
| reading bytes | `file.read` returns text; a file that is not UTF-8 text answers `{ binary: true, size }` instead of being refused. Bytes can be written. `sandbox.files` and `machine.files` read a byte window | **Changed.** File bytes through `file.read` stay **not restored** ([section 10](#10-history-features-that-are-gone)) |
| exec output as a string | a `string` when an event's bytes are UTF-8, else a `Buffer` / `bytes`, **per event** | **Changed.** A caller that joins output must accept both ([Events](../../architecture/api/calls/errors-and-events.md#workspaceexec-output)) |

## 6. Errors, Unknown and Unresolved

The refusal is `{ error, message }`, with `fix` (one runnable shell line)
when LayerFS knows one and the fields the next call needs beside them, in
every front end; the long form adds `detail` (the raised code and what else
was recorded). Node throws `LayerFSError`, Python raises
`layerfs.LayerFSError` (`.error`, also `.code`; `.message`; `.fix`; `.detail`
when the call asked the long form), the command prints the JSON on stderr
and exits 125. A wrong command
line is exit 2 with one plain `layerfs: ...` line. The codes are in the
[catalog](../../architecture/api/calls/errors-and-events.md#refusals); which of
them are 1.1.0's own (`NoSuchStore`, `NoSuchPath`, `NoSuchSandbox` and others),
and the public code each now leaves under, is in
[calls: errors](calls.md#errors-and-exit-codes).

- **Every call is one attempt.** The library never retries, resumes or replays,
  and nothing commits or unmounts on its own.
- **`Unknown`** means the outcome may have happened (a lost answer). Both sides
  are kept and logged; **read the state with `list`, `get` or `status` before
  acting again.**
- **`Unresolved`** means an earlier `Unknown` on that entity stands and this
  call did nothing; the message names the call that shows it and the one that
  clears it ([Errors: uncertain outcomes](../../architecture/api/calls/errors-and-events.md#uncertain-outcomes)).
- A refusal before any effect (bad arguments, unknown option, out of scope) is
  known: nothing happened. The coverage checks list Commits, Branches and
  Stores before and after such a call and compare them.
- Not run: a real `Unknown` or `Unresolved` outcome (it needs a daemon, a fault
  or a second process).

## 7. Progress and exec output

| Old | 2.0.0 | Status and reason |
| --- | --- | --- |
| `events.subscribe` / `poll` / `unsubscribe`, SSE and NDJSON streams (#521); the trace subscription of `@layerfs/backend` (1.1.0) | none | **Removed** (O26). Trace files are the feed: `<home>/trace/calls.jsonl` holds one line per call, read with `home.log` (#659); `LAYERFS_TRACE` records worker connections and step times ([home and records](../../architecture/control/home/README.md#traces-and-the-call-log)) |
| job registry: `start_init` / `get_init` / `subscribe_init` | one `on_progress` stream on the call itself | **Removed** (O22). Another process cannot watch a call it did not start; a running operation is the `busy` state with its `op` (`store get`, `machine get`, `sandbox get`) |
| progress of any call | `on_progress` on the 19 calls whose row says progress (the long-running calls and the five doctors that check children); the shell form is `--progress` (JSON lines on stderr) | **Changed.** One event shape, `{ phase, done, total }`; a doctor adds `node`, `workspace.exec` adds `exec`. [Shell switches](calls.md#shell-switches), [Events](../../architecture/api/calls/errors-and-events.md#progress-events) |
| `exec` output | `{ exit_code, stdout, stderr }` in the answer; `stdout` / `stderr` functions, `stream: true`, `--stream` | **Changed.** Output is delivered after the command ends, never live. A stream of at most 1 MiB is answered whole; a longer one answers its last 1 MiB and names the file that holds all of it (`stdout_file`, read with `sandbox files`). Run for this page on macOS `native` ([quickstart](quickstart.md#4-a-sandbox-a-workspace-and-a-commit)) |
| exit code | `layerfs workspace exec` exits with the command's own code in every form; 125 still means refused, with nothing on stdout | A command that exits 2 or 125 looks like a usage error or a refusal; stderr tells them apart ([CLI: Streams](../../architecture/api/front-ends/README.md#the-layerfs-command)) |

Seven decisions about this contract are open
([#37](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/37), listed in
Events: open decisions).
A function that throws does not stop a call; a Node `async` function that
rejects ends the process. Cancelling a Python coroutine stops its functions,
not the native call.

## 8. Diagnostics and doctor

| Old | 2.0.0 | Status and reason |
| --- | --- | --- |
| `client.results(after?)`, a ring of per-call results (#521) | none | **Not restored** (owner decision, [#31](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/31#issuecomment-6657201351)) |
| the call trace and operator reads of `@layerfs/backend` | the home's call log: one line per call in `<home>/trace/calls.jsonl` (CLI words with redacted values, outcome, time, client, an optional tag), read with `home.log` and cleared with `home.clear_log`; `LAYERFS_TRACE=<file>` appends one `worker.connection` line per connection to a machine's worker | **Changed** (#659). [home and records](../../architecture/control/home/README.md#traces-and-the-call-log) |
| a health check (`layerfs doctor` of the 2.0 previews, Node `doctor()`, Python `layerfs.doctor()`) | six table calls: `home.doctor`, `project.doctor` (`--full` reads the data), `sandbox.doctor`, `machine.doctor`, `store.doctor`, `backup.doctor` (`--full` tests a write). Each checks its own thing, then each thing on it shallowly, and changes nothing | **Replaced**, with no alias (R1). [The doctors](calls.md#the-doctors) |
| leftovers in the doctor's report, `doctor.fix` | none: LayerFS removes what interrupted operations left itself, at fixed points in later calls and only where it can prove nothing owns it; each removal is a line of `log/sweep.jsonl` ([`home`](calls.md#home-5)). The 2.0 previews' `home.leftovers` and `home.prune` were removed in spec v20 (owner 2026-11-19) with no alias; both exit 2 | **Removed** (v20) |
| `close()` | `close()` closes idle Stores; the command does it after every call | **Changed** (section 2) |

A doctor **exits 0 once its report is made**, whatever it found; the short
form is `{"ok": true}` alone when nothing was found and every check was
made. It is 125 when the thing is not in the home (`NotFound`), and 2 for a
wrong command line. Run for this page: `layerfs home doctor`, `machine
doctor native`, `store doctor shop` and `project doctor shop/api --full` on a
scratch home ([the calls](calls.md#project-8)).

## 9. The Workspace flow

A Workspace is a mounted side Branch where ordinary commands run. The route is
`sandbox.create`, `workspace.mount`, `workspace.exec`, `workspace.commit`,
`workspace.unmount`, `sandbox.remove`
([workspace family](../../architecture/api/sdk/workspace.md),
[quickstart section 4](quickstart.md#4-a-sandbox-a-workspace-and-a-commit)).

| Old | 2.0.0 | Status and reason |
| --- | --- | --- |
| a single-project MCP session that unmounted at end of input or on a signal (#521) | nothing commits or unmounts for you | **Removed.** `workspace.unmount` is `Dirty` while changes are uncommitted, unless `discard`; `sandbox.remove` is `InUse` while a Workspace is mounted, unless `force` |
| `mount` of `main` | refused `InvalidInput`: fork a Branch from it | **Changed** |
| `commit` | one Commit; merges first when the Branch moved since the mount, `force` publishes over a moved head; a conflict writes markers and commits nothing (`Conflict`) | unchanged in meaning |
| a Sandbox's commands on its machine's network (a server another Sandbox ran was reachable on `127.0.0.1`, two Sandboxes could not use one port) | every namespaces Sandbox's commands have a network of their own: their own `127.0.0.1`, out to anything, siblings, the daemon's and the worker's port closed | **Changed** (2026-11-24, no backward compatibility): reach a machine service at the machine's address, not `127.0.0.1`; a Sandbox started by an earlier build is broken after the upgrade (`layerfs sandbox remove` and create it again); on a Linux host forwarding off, a dropping forward chain or a loopback-only resolver refuses `sandbox.create` `Unavailable` with the fix; IPv6 other than `::1` fails at once. macOS `native` unchanged. [Backends](../../architecture/control/sandboxes/backends.md#every-namespaces-sandboxs-own-network) |
| the Store profile | WAL with `synchronous=NORMAL`, the only SQLite profile (record label `normal`; a record that says `disposable` reads as `normal` and is never rewritten); Durable is disabled by the owner | **Changed** from 1.1.0's `synchronous=OFF` (owner, 2026-11-17; OFF removed 2026-11-23); no power-loss guarantee is claimed yet |

Run for this page on macOS `native` with a daemon of the same build, in the
shell, Node and Python ([quickstart](quickstart.md#4-a-sandbox-a-workspace-and-a-commit),
[examples](examples/README.md#the-workspace-route)).

Back up and move Stores with the existing guides, not with this page:
[Stores](machines-and-stores.md#stores) and
[backups](machines-and-stores.md#backups).

## 10. History features that are gone

These existed in #521 and have no 2.0 call. The owner decision of 2026-11-05
(recorded in the [#31](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/31#issuecomment-6657201351) decision record, §0) is to **follow the project baseline**: they
are **not restored in 2.0** and are listed as known gaps in the
[interface report](../../general/interface-report-2.0.0.md#known-gaps). A
named consumer would reopen each one.

| Old (#521) | What 2.0 has instead | Evidence |
| --- | --- | --- |
| `branch.graph` (first-parent graph, fork point, publication state, main's Layer walk) | pieces only: `branch.list` answers each Branch's head `commit`, with `forked_from`, `base` and `root` in the long form; `commit.list` answers `parent` and `second_parent`, and on `main` its publications with `source_branch` | not in the table ([coverage](../../architecture/api/calls/coverage.md#the-matrix)) |
| `changed_paths(commit)` | `commit.diff(project, commit)`: a Commit against its parent, and main's first publication against the empty tree (every path `added`) | [`commit.diff`](../../architecture/api/sdk/commit.md#commitdiff) |
| commit author and labels | none. A Commit carries `commit`, `message`, `branch`, `parent`, `second_parent`, `base`, `root`, `via`, `created_at`, `merged_from`, `source_commit` and size counters (long form) | the `commit.list --detail` output in [the calls](calls.md#commit-3); [`commit.list`](../../architecture/api/sdk/commit.md#commitlist) |
| per-call results (`client.results`) | none | section 8 |
| cheap project summary (`get_summary`: Branch and Commit counts, last Commit, size) | `project.get` answers the Store, state and main's head; `project.list --with stats` main's size from its head's record; `project.stats` counts by a bounded walk | [`project.get`](../../architecture/api/sdk/project.md#projectget) |
| binary `file.read`, `offset` / `nextOffset` cursor | `file.read` reads text and pages by `line` / `limit`, answering `next_line`; a non-text file answers `{ binary: true, size }` | [`file.read`](../../architecture/api/sdk/file.md#fileread) |

Also not restored, by earlier decisions: the backend facade (O25), the event
streams (O26) and the Node CLI. `commit.get` reads one Commit by its id or a
prefix of 8 or more in one Store call, without naming its Branch.

## What was run

For this version of the page (the 2.0 call table, branch `cs20-pkg-z` from
`call-surface-2-0` at `61960e3f5f`), on macOS arm64 with the debug builds of
the command, the Node addon, the Python module and the daemon, against
scratch homes: the four [examples](examples/README.md) and the
[quickstart](quickstart.md) (shell, Node and Python, a Sandbox and a
Workspace on `native` included), `layerfs sandbox close x` and `layerfs
doctor` (both exit 2, no such call), Node `layerfs.sandbox.close` (not a
function), the doctors of section 8, and the folder scope refusals in
[the calls](calls.md#folder-scope). The install steps of section 1 were
run on Linux x64 for the earlier version of this
page (tree pinned in the
[interface report](../../general/interface-report-2.0.0.md#pinned-identity))
and not rerun.

**NOT RUN for this version:** the installed artifacts; Linux, Lima, Apple
Container, Windows; PostgreSQL through Node or Python; Python 3.10 to 3.13
and Node 22; a real `Unknown` outcome; backups.
