# The `layerfs` command and SDK

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.
> Written 2026-11-02 from the owner's review of
> [one control center for every platform](host-platform-connections.md),
> which holds the model, records, workers and plan behind these calls; this
> page holds the calls. The work order is [#533](../issues/533/README.md).
> Nothing here is built. Today's surface is four
> JavaScript packages over one Rust binding: `@layerfs/agent` (the SDK),
> `@layerfs/runtime` (homes, Stores, VMs), `@layerfs/backend` and
> `@layerfs/cli` (the `layerfs` command, families `project`, `branch`, `fs`,
> `sandbox`, `workspace`), all at 1.1.0. The target is one Rust
> implementation with three front ends: the command, Node and Python.
> Owner choices that are still open are marked "(proposed, O<n>)". §6 lists
> them; [the proposal's §13](host-platform-connections.md#13-open-decisions)
> holds each full question.

## 1. Concepts

```
 LAYERFS_HOME          the control center: every record below lives here
   |
   +-- machines        where Sandboxes run          native, docker, VMs, ssh hosts
   +-- Stores          where history lives          sqlite: a file on one machine
   |                                                postgresql: a database on a server
   +-- backups         copies of sqlite Stores      r2 and minio buckets
   |
   +-- projects        <store>/<project>, each in one Store
         +-- Branches and Commits
         +-- Sandboxes      named, bound to one project, running on one machine
               +-- Workspaces   a Branch mounted in a Sandbox: <sandbox>:<path>
```

On disk the home has three parts: `machine/<name>/` with its Sandboxes under
it, `store/<name>/` with its projects under it, and `backup/<bucket>/`. What
never moves nests under its owner; a sqlite Store moves, so it stays at the
top. The full layout is in
[the proposal, §7.8](host-platform-connections.md#78-the-home-is-the-control-center).

- **Machine.** One kernel where Sandboxes run. `native` (the host's own OS)
  and `docker` are built in, one of each. On macOS `native` runs host
  processes; on Linux it runs Linux namespaces (proposed, O27). #529 called
  these Sandboxes `mac` and `linux`. VMs (`apple-container`, `microsandbox`,
  `lima`, `firecracker`) are created and named, as many as the host holds.
  An ssh host is connected. A machine starts on the first call that needs
  it. Its worker stops it when idle (§3.7), and it keeps its disk.
- **sqlite Store.** One `store.sqlite` file, created on `native` and moved to
  one machine at a time. Its Sandboxes run only on that machine. On Windows,
  `native` holds only records; its Stores and Init are in the WSL worker
  (proposed, O5).
- **postgresql Store.** One database on a server, in LayerFS's own schema
  `layerfs`. It is on no machine; Sandboxes on any machine that supports it
  reach it over the network.
- **Backup.** A copy of a sqlite Store in an r2 or minio bucket. Nothing runs
  from a backup.
- **Project.** Imported once from a host folder into one Store, and always
  named with it: `<store>/<project>`, such as `shop/api`. `shop/api` and
  `pg-main/api` never collide.
- **Sandbox.** Named (unique in the home), bound to one project when created,
  running on one machine. After `create`, its name is enough.
- **Workspace.** A Branch mounted in a Sandbox, named `<sandbox>:<path>`.

## 2. One package, one surface

`layerfs` is **one implementation** holding what `@layerfs/agent`,
`@layerfs/runtime`, `@layerfs/backend` and `@layerfs/cli` hold today:
machine management, Stores, routes, workers and every call. It is one Rust
library with three thin front ends, all named `layerfs`
([the proposal, §7.10](host-platform-connections.md#710-one-rust-implementation-three-front-ends)).
The library is the `layerfs-api` crate grown in place (proposed, O21).

| Front end | Ships as | Needs |
| --- | --- | --- |
| the `layerfs` command | a native binary | nothing else; no Node |
| Node | an npm package with a napi-rs addon | Node |
| Python | a PyPI wheel with a PyO3 module | Python |

The npm package's source is `api/node/` and the PyPI package's is
`api/python/`. Today's `api/agent/src/errors.js` moves to `api/node/`.

There is **no split** between agents, operators and the command: every
caller gets every call.

```js
import { project, branch, commit, fs, sandbox, workspace, machine, store, backup, LayerFSError, home } from "layerfs"
```

```python
from layerfs import project, branch, commit, fs, sandbox, workspace, machine, store, backup, LayerFSError, home
```

Each family is bound to `LAYERFS_HOME`. For another home, `home("/path")`
(in both languages) returns the same nine families bound to that home. The
shell form is the variable: `LAYERFS_HOME=/path layerfs ...`.

In Python, file content is `bytes` or `str`, results are dicts and handles
are objects.

A few commands are not among the 54 calls: `layerfs worker` (run inside a
machine), `layerfs import-env` (proposed, O4), `layerfs help` (`-h`,
`--help`) and `layerfs --version`.

### 2.1 Shell and code are the same calls

| Shell | Node | Python |
| --- | --- | --- |
| `layerfs <family> <call> <a> <b> [<c>] --opt <v> --flag` | `family.call(a, b, c?, { opt: v, flag: true })` | `family.call(a, b, c=None, opt=v, flag=True)` |
| `-m <text>` | `{ message }` | `message=` |
| `--start-line 10` | `{ start_line: 10 }` | `start_line=10` |
| `--progress`: JSON lines on stderr | `{ on_progress }` | `on_progress=` |
| one line of JSON on stdout | the same plain object | the same dict |
| a refusal as JSON on stderr, exit 125 | `LayerFSError { code, message, detail }` | `LayerFSError` with `.code`, `.message`, `.detail` |

All three make the same call into the same library, so the results and
errors are the same by construction. Node calls return promises. Python calls
are synchronous, with the same calls as coroutines in `layerfs.aio`
(proposed, O19). Every call is listed in §3 once, as Node code; its shell and
Python forms follow from this table. Where the shell differs (content from
stdin or a file), §3 says so.

Results keep their 1.1.0 shape unless §3 says otherwise. Keys are snake_case
in every front end: `exit_code`, not 1.1.0's `exitCode`. Each `fs` write and
`workspace.commit` returns `{ commit }`. `project.init` returns
`{ project, branch: "main", commit }`.

### 2.2 Arguments

- **Positionals go from the outside in:** the owner (project, Store, Sandbox
  or Workspace), then the Branch, then the path or new name, then other
  inputs. An optional Commit is last.
- **Options say how,** or carry a message or a credential.
- Optional positionals are told apart by type: a Commit id is a string, an
  options object is an object, so leaving one out needs no `null`. In the
  shell, an optional positional is simply left off.
- In Python, optional positionals and options are keywords.
- A shell option's dashes become underscores in code: `--start-line` is
  `start_line`.

### 2.3 Names

| Thing | Form | Unique in |
| --- | --- | --- |
| machine | `dev` | the home |
| Store | `shop` | the home |
| bucket connection | `r2-main` | the home (its own name space, apart from Stores) |
| project | `shop/api` | always written with its Store; a bare `api` is refused |
| Branch | `main`, `fix` | its project |
| Commit | an id, such as `c7f3a9` | its project |
| Sandbox | `tests` | the home |
| Workspace | `tests:/workspaces/1` | — (returned by `workspace.mount`) |

Machine, Store, bucket, project and Sandbox names are 1–63 lowercase letters,
digits, `_ . -`, starting and ending alphanumeric; none contains `/`.

Some names are reserved. `native` and `docker` name the built-in machines,
so `machine create` and `machine connect` refuse them. Each name is a folder
in the home, so the Windows device names are refused everywhere: `con`,
`nul`, `aux`, `prn`, `com1` to `com9` and `lpt1` to `lpt9`. A reserved name
is `InvalidInput`.

### 2.4 Handles

A handle is the call's result object with its calls bound. `sandbox.create`
and `sandbox.get` return `{ sandbox, project, machine, image }` with
`sb.close()`. `workspace.mount` returns `{ ws, sandbox, branch, path }` with
`ws.exec(command)`, `ws.commit({ message })` and `ws.unmount()`. Python
handles are the same.

A handle holds only names, so another process gets a Sandbox's handle back
from `sandbox.get(name)`. A Workspace has no getter: pass its `ws` string to
the `workspace` calls. Mounting in a closed Sandbox is `NoSuchSandbox`; create
the Sandbox again.

### 2.5 Settings

| Name | Meaning |
| --- | --- |
| `LAYERFS_HOME` | the home; **the one setting**. Unset, it is `~/.layerfs` (proposed, O28) |
| `LAYERFS_TRACE`, `LAYERFS_TRACE_STEP`, `LAYERFS_TRACE_START` | tracing, as today |
| `LAYERFS_CONSTRUCTION_WORKERS` | measurement, as today |

Every other `LAYERFS_*` setting a user or an integration sets goes.
[The proposal's §3.1](host-platform-connections.md#31-all-56-settings-by-kind)
counts all 56. A secret reference (§2.8) names variables; they are not
settings. LayerFS still sets its seven internal names for its own child
processes.

`LAYERFS_STORE_POLICY` goes too. Today it is Init's JSON construction policy
(`smallFileThreshold`, `wholeFileDepth`, `chunkDepth`, `metadataDepth`), not
the persistence profile. With no setting, Init uses the default policy. Every
sqlite Store's record says Disposable, the only allowed profile.

There is no default project: 1.1.0's `LAYERFS_PROJECT` and `--project` are
removed, because every call that needs a project takes it as an argument.

### 2.6 Output and exit codes

| | |
| --- | --- |
| stdout | the call's result as one line of JSON (`--text`: readable) |
| stderr | a refusal as JSON: `{"code", "message", "detail"}` |
| exit 0 | the call succeeded (`workspace exec` without `--stream`: the command's exit code is in the JSON) |
| exit 2 | the command line is wrong; nothing was called |
| exit 125 | LayerFS refused or failed the call |

Exit 2 means a wrong number of arguments, an unknown option, a value that is
not a whole number where one is needed, bad JSON in `--units` or stdin, or an
unreadable `--from` file. Names, kinds and references are checked by the
call: a bad one is `InvalidInput`, exit 125.

`workspace exec --stream` exits with the command's own code; 125 still means
LayerFS failed, as in 1.1.0.

**Long calls report as they run (proposed, O22).** One stream serves five
calls: `workspace.exec` sends the command's output on it, and `project.init`,
`store.move`, `backup.create` and `backup.restore` send progress. In Node it
is `{ on_progress }` and in Python `on_progress=`, a function called with each
update. In the shell, `--progress` writes one JSON line per update to stderr,
`{"progress": {...}}`. A refusal, if any, is the last stderr line, and stdout
keeps the single result. There is no job registry: 1.1.0's start and get
pairs (`runtime.project.start_init` and `get_init`, `runtime.sync.get`) go
away. Another process cannot watch a call it did not start; `store.status`
shows a move in progress (§3.8).

### 2.7 Verbs

| Verb | Used for |
| --- | --- |
| `list`, `get`, `status` | reading; never changes anything |
| `init` | a project, from a host folder |
| `create` / `remove` | what LayerFS makes and owns: a VM, a sqlite Store |
| `move` | a sqlite Store's file, to another machine |
| `create` / `restore` | a backup: a file in a bucket, written from a Store and read back into one. LayerFS never deletes bucket objects |
| `fork` | a Branch: a new one from another Branch's head or Commit, as in 1.1.0 |
| `merge`, `publish`, `diff` | Branches and Commits, as in 1.1.0 |
| `connect` / `disconnect` | what exists outside LayerFS: an ssh host, a PostgreSQL database, a bucket. `disconnect` forgets it; its data stays |
| `create` / `close` | a Sandbox: started, then stopped |
| `mount` / `unmount` | a Workspace |
| `exec`, `commit` | in a Workspace: run a command, save its changes |
| file verbs | the `fs` family, with Unix names: `read`, `write`, `edit`, `rm`, `mkdir`, `mv`, `cp`, `apply`, `stat`, `list`, `glob`, `grep`, `blame` |

No call asks for confirmation. Every call is **one attempt**: no retry. After
`Unknown`, read the state with `list`, `get` or `status` before acting again.
A call that does not apply to a name's kind is refused `InvalidInput` with the
reason, before any effect.

### 2.8 Secret references

| Reference | Reads |
| --- | --- |
| `env:LAYERFS_<NAME>` | a bucket: `LAYERFS_<NAME>_ACCESS_KEY` and `LAYERFS_<NAME>_SECRET_KEY`; PostgreSQL: `LAYERFS_<NAME>` (the password) |
| `file:/absolute/path` | a bucket: lines `access_key=…` and `secret_key=…`; PostgreSQL: the password. Refused if anyone but its owner can read it |

The bucket forms are today's (`layerfs-remote-storage/src/credentials.rs`);
the PostgreSQL forms are new. Keeping only these two schemes is proposed
(O1). A reference is resolved at each call and its value is never written to
the home. Anyone with the home can
call every call. A call whose reference can't be read (the variable is unset,
or the file is missing or readable by others) is refused `InvalidInput`
(proposed; today's resolver returns `Invalid`).

### 2.9 Errors

The project, branch, commit, fs, sandbox and workspace calls keep 1.1.0's
codes: `CODES` in `api/agent/src/errors.js`, 30 codes. They include
`Conflict`, `HeadMoved`, `Dirty`, `NoMatch`, `AmbiguousMatch`, `NotEmpty`,
`TooLarge`, `Capacity`, `MountsActive` and the `NoSuch*` codes
(`NoSuchProject`, `NoSuchBranch`, `NoSuchCommit`, `NoSuchPath`,
`NoSuchSandbox`, `NoSuchProcess`). A Sandbox's 16-Workspace limit is
`Capacity`. The machine, store and backup calls add the codes below.

| Code | Meaning |
| --- | --- |
| `InvalidInput` | the request is wrong for this name or kind; nothing happened |
| `NotFound` | no machine, Store or bucket of that name. Projects, Branches, Commits and Sandboxes keep their `NoSuch*` codes |
| `Exists` | the name is taken; the same database, bucket or host is already connected; or a backup file was written by another home |
| `InUse` | it is held: running Sandboxes, processes with the Store open, Stores on a machine |
| `Unsupported` | not possible here: a missing tool, a Sandbox off its Store's machine |
| `Unavailable`, `Denied` | a server, bucket or host can't be reached, or refused the login |
| `Conflict` | as in 1.1.0, a merge or Commit conflict. For `backup.restore`, the Store changed since its last backup or restore. The detail says which |
| `Failed` | it was tried and did not happen; whatever exists is kept |
| `Unknown` | the outcome is not known; both sides are kept and logged in `log/` |

## 3. The calls

Nine families, 54 calls. Two open decisions would each add one:
`project.remove` (O24) and `machine update` (O11).

### 3.1 `project` (3)

```js
project.init(project, folder, { timeout, on_progress })
project.list(store?, { page_size, page_pos })
project.get(project)
```

**`project.init("shop/api", "/Users/me/api")`** imports a host folder as a new
project, in the caller's own process, into the existing Store named before the
`/`; Init never creates a Store. Init adds the project through the Store's
writer while the Store stays live; it makes no sealed file. The Store's kind
decides where Init can run:

- a sqlite Store must be on a machine whose Store calls run in the caller's
  process: `native`, or `docker` on Linux, where Init writes straight into
  the volume as today. On any other machine it is `InvalidInput` ("move it
  to native first"): a VM, an ssh host or Docker on macOS or Windows can't
  read the host folder. On Windows, `native` Init runs in the WSL worker
  (proposed, O5).
- a postgresql Store is written straight into its database. **New**: today a
  project reaches PostgreSQL only by importing a SQLite Store, and Init into
  PostgreSQL is new work.
- `NotFound` for a Store that doesn't exist; `Exists` for a project the Store
  already holds. `timeout` is in ms, default 300000, as today. `on_progress`
  is §2.6's stream.

**`project.list(store?)`**: every project in the home, or one Store's, as
`<store>/<project>` with the Store's kind and machine, a page at a time
(1.1.0's `runtime.project.list` had no paging). Listing a postgresql Store's
projects needs the network and its password reference.

**`project.get(project)`**: the project's record: its Store, and that Store's
kind and machine (was `runtime.project.get_state`).

**Removing one project** has no call: `store.remove` removes a Store with
every project in it. Proposed (O24): add `project.remove(project)`, a 55th
call.

### 3.2 `branch` (6)

```js
branch.list(project, { page_size, page_pos })
branch.get(project, branch)
branch.fork(project, name, from_branch, from_commit?)
branch.merge(project, name, a, b, { message, units })
branch.publish(project, branch, commit?)
branch.diff(project, from_branch, to_branch, { paths_only, page_size, page_pos, cursor })
```

- **`list`**: the project's Branches, a page at a time (was
  `project list_branches`). **`get`**: one Branch and its head Commit.
- **`fork`**: a new Branch from another Branch's head, or from one of its
  Commits. The name stays `fork`, as in 1.1.0; the project and the source
  Branch become positional.
- **`merge`**: merges `a` and `b` into the new Branch `name`; `units` is a
  JSON list of patterns or `{ name, patterns }` groups, as today. The new
  Branch's name is second, as in `fork` (today it is last).
- **`publish`**: a Branch's head, or one of its Commits, as `main`'s next
  Layer.
- **`diff`**: files that differ between two Branches' heads, with their line
  changes; `paths_only` lists paths only (was `diff_branches`, `--no-hunks`).
  It pages as in 1.1.0: `cursor` resumes after the previous page's
  `next_cursor`.

### 3.3 `commit` (3)

```js
commit.list(project, branch, { page_size, page_pos })
commit.get(project, branch, commit)
commit.diff(project, from_commit, to_commit, { paths_only, page_size, page_pos, cursor })
```

- **`list`**: a Branch's Commits, newest first; `page_size` default 50
  (was `branch list_commits`).
- **`get`**: one Commit of that Branch: its message, parent, time and Layer.
  New; 1.1.0 has no single-Commit call.
- **`diff`**: files that differ between two Commits (was `branch
  diff_commits`), paged as `branch.diff` is.

A Commit id is unique in its project, so `commit.diff` needs no Branch.
`commit.get` takes one and checks that the Commit is on it. A Commit given
with a Branch must belong to that Branch; otherwise `InvalidInput`. On
`main`, a Layer id is accepted wherever a Commit is.

### 3.4 `fs` (14)

```js
// reads: the Branch's head, or the Commit given
fs.read(project, branch, path, commit?, { start_line, end_line })
fs.stat(project, branch, path, commit?)
fs.list(project, branch, path, commit?, { page_size, page_pos })
fs.glob(project, branch, path, pattern, commit?, { page_size, page_pos })
fs.grep(project, branch, path, pattern, commit?, { page_size, page_pos })
fs.blame(project, branch, path, commit?, { from_line })
fs.diff(project, left_branch, left_path, right_branch, right_path, { left_commit, right_commit })

// writes: one Commit on the Branch's head each; no Commit argument
fs.write(project, branch, path, content, { message, mode })
fs.edit(project, branch, path, { old, new, message })
fs.rm(project, branch, path, { message })
fs.mkdir(project, branch, path, { message, mode })
fs.mv(project, branch, from_path, to_path, { message })
fs.cp(project, branch, from_path, to_path, { message })
fs.apply(project, branch, changes, { message })
```

```bash
layerfs fs read shop/api main /src/index.ts
```

```bash
layerfs fs read shop/api main /src/index.ts c7f3a9 --start-line 10 --end-line 40
```

- Paths are absolute, starting with `/`. `glob` and `grep` search under
  `path` (`/` for all); `pattern` is a glob or a regular expression.
- `list`, `glob` and `grep` page as in 1.1.0.
- `mode` sets a file's or folder's permission bits (`--mode 755`), as in
  1.1.0.
- **`diff`** takes its two sides positionally, as the shell does:
  `<left-branch> <left-path> <right-branch> <right-path>`, with
  `--left-commit` and `--right-commit` (proposed, O29). The alternative is
  two `{ branch, path, commit }` objects in code.
- **Shell differences:** `write` and `apply` read their content from stdin,
  or from `--from <file>`; `edit` takes `--old <text>` or `--old-from <file>`
  and `--new <text>` or `--new-from <file>`.
- `edit`'s `{ old, new, message }` is the owner-approved shape; `new` is a
  valid property name in JavaScript.
- `apply` takes a JSON array of changes and makes one Commit, all or none.
- A Commit or Layer id in the Branch position of a write is `InvalidInput`.

### 3.5 `sandbox` (4)

```js
sandbox.create(project, name, machine, { image })
sandbox.list(project?, { page_size, page_pos })
sandbox.get(name, { page_size, page_pos })
sandbox.close(name)
```

```bash
layerfs sandbox create shop/api tests docker
```

**`sandbox.create(project, name, machine)`** starts a Sandbox, binds it to the
project and returns its handle when it is ready:
`{ sandbox, project, machine, image }`. It starts the machine first if it is
stopped. The `machine` argument replaces 1.1.0's `--platform` (#529).

| The project's Store | `machine` |
| --- | --- |
| sqlite | must be the machine the Store is on; any other is `Unsupported` ("Store shop is on docker; create the Sandbox there, or move the Store first") |
| postgresql | any machine that supports PostgreSQL; today only `docker` (O3) |

| Machine | `image` |
| --- | --- |
| `docker` | any image digest; default the docker machine's image |
| `native` on Linux, VMs, ssh | the machine's image; another digest is `Unsupported` |
| `native` on macOS | none: `image` is `InvalidInput` |

`Exists` while any Sandbox in the home has that name.

**`sandbox.list(project?)`**: every Sandbox in the home, or one project's, a
page at a time.

```
$ layerfs sandbox list --text
NAME    PROJECT       MACHINE  IMAGE          STATE    WORKSPACES
tests   shop/api      docker   sha256:3f2a…   running  2
load    pg-main/load  docker   sha256:3f2a…   running  0
```

**`sandbox.get(name)`**: its handle and its mounted Workspaces, a page at a
time (was `list_workspaces`).

**`sandbox.close(name)`**: stops it now. A Sandbox also closes itself two
minutes after it has no Workspace and no process.

### 3.6 `workspace` (4)

```js
workspace.mount(sandbox, branch)
workspace.exec(ws, command, { message, stream, stdout, stderr })
workspace.commit(ws, { message, force })
workspace.unmount(ws, { discard, force })
```

```bash
layerfs workspace mount tests main
```

- **`mount`**: a new Workspace of a Branch of the Sandbox's project. Returns
  its handle, `{ ws: "tests:/workspaces/1", sandbox, branch, path }`. A
  Sandbox holds up to 16 Workspaces; one more is `Capacity`.
- **`exec`**: one bash command in the Workspace. `message` is kept, as in
  1.1.0. Without `stream`, the result carries `exit_code`, `stdout` and
  `stderr`. With `stream`, the shell passes the output and exit code through.
  Node and Python write the output to the caller's stdout and stderr, or to
  `{ stdout, stderr }` sinks when given; the result carries `exit_code` and
  byte counts.
- **`commit`**: the Workspace's changes to its Branch; returns `{ commit }`.
  `force` publishes over a moved head instead of merging, as in 1.1.0.
- **`unmount`**: ends the Workspace; `discard` drops uncommitted changes,
  `force` aborts its connection.

### 3.7 `machine` (7)

```js
machine.list()
machine.get(name)
machine.status(name)
machine.create(name, platform, { cpus, memory, disk, image, nested, host })
machine.connect(name, host, { user, port, identity, known_hosts, root })
machine.remove(name)
machine.disconnect(name)
```

**`machine.list()`**: every machine, and the platforms that could be created
here.

```
$ layerfs machine list --text
NAME     PLATFORM  BACKEND     STATE    CPUS  MEMORY  STORES   SANDBOXES
native   native    native      running  -     -       web      1
docker   docker    docker      stopped  -     -       shop     0
dev      lima      namespaces  running  4     8G      tmp      2
ec2      ssh       namespaces  running  -     -       -        0

CAN CREATE
lima              ready
apple-container   missing: container CLI
firecracker       missing: KVM in the Lima VM
microsandbox      ready
```

On Linux, `native` needs a root caller, `/dev/fuse` and cgroup v2;
`machine list` names what is missing, such as `host:root`, as
`api/runtime/src/sandbox/platforms.js` does today.

**Machines per platform on one host:**

| Platform | Machines | Notes |
| --- | --- | --- |
| `native` | 1, built in | the host itself |
| `docker` | 1, built in | one Docker engine per host |
| `apple-container` | many | each its own lightweight VM; Apple silicon, macOS only |
| `microsandbox` | many | each its own microVM |
| `lima` | many | each its own Lima instance; macOS only today |
| `firecracker` | many, in a Lima host | macOS has no KVM: every microVM nests in a `lima` machine made with `--nested` (M3 or newer, macOS 15+) and shares it; one `172.30.<n>.0/30` network each, so at most 250 per host. macOS only today, as `lima` is |
| `ssh` | one per remote host | connected, never created |

**A machine's platform decides how its Sandboxes run;** a caller never
chooses. `native` runs host processes on macOS and Linux namespaces on Linux,
`docker` runs containers, and every VM kind and `ssh` run Linux namespaces
inside the machine, through its worker
([the proposal, §7.10.1](host-platform-connections.md#7101-machines-and-sandbox-backends)).
Today Linux can also run Sandboxes as host processes (`LAYERFS_SANDBOX=native`,
`api/agent/src/platform.js`); 2.0 removes that pairing (proposed, O27).

LayerFS sets no machine count of its own; the host's CPU, memory and disk
are the limit. Today's VM code already makes one VM per project, so several
VMs of one kind on one host is current behaviour. Only macOS was tested
(#529).

- **`create`**: a VM of one of the VM kinds. It starts now. `Unsupported`
  when the platform's tools are missing (repeating `missing`); `InvalidInput`
  for `native`, `docker` or `ssh`.
  - `image` is the image its Sandboxes run in. Today's `vm.image` is
    something else: the VM's own OS, which must carry Node.js 22 (an OCI
    image such as `node:22-trixie-slim` for `apple-container` and
    `microsandbox`, a disk URL for `lima`). Today the Sandbox image is the
    bundle's single `images/<digest>.tar`. In 2.0 the VM's OS comes from the
    bundle (O16), and copying the `image` archive in at create is new.
  - `memory` and `disk` are sizes with `K`, `M` or `G` in the shell
    (`--memory 64G`); code takes the same string or a number of bytes
    (proposed). Today's VM settings take `M` or `G` only.
  - `nested` (`lima` only): a `vz` instance with nested virtualization, able
    to host Firecracker machines.
  - `host` (`firecracker` only, required): the `lima` machine made with
    `nested` that this microVM runs in. That machine is `InUse` while it
    hosts any.
  - A create that fails before the VM exists removes what it made. Once the
    VM exists, a failure keeps it, in state `failed`, for inspection, as
    today. `Unknown` keeps everything and logs it.
- **`connect`**: an existing Linux host over ssh, which LayerFS never creates.
  - Every command runs as the login user, with no sudo. So `user` is `root`,
    the default, as today; a user that gains root through sudo is not
    supported today. `port` defaults to 22.
  - `identity` is a plain path to a key file, and is optional: ssh-agent and
    the default keys also work.
  - The host's key must already be known, and is checked strictly, as today.
    Proposed: `known_hosts` names the file, default `~/.ssh/known_hosts`,
    and it is copied to `machine/<name>/known_hosts`.
  - `root` is mounted or bind-mounted at `/layerfs/data`, which holds
    `store/<name>/` and `sandbox/<name>/`. It defaults to `/layerfs/data`.
  - One connection checks it; `Unavailable` or `Denied` records nothing. A
    second machine on the same host is `Exists`. The host is recognised by
    its host key or `/etc/machine-id` (proposed).
- **`remove`**: deletes a VM and its disk. **`disconnect`**: forgets an ssh
  host. It keeps the data root and every Store in it; it deletes
  `/layerfs/bundle` and `/layerfs/state`, as today's remove does. Both are
  `InUse` while the machine holds Stores or running Sandboxes. `native` and
  `docker` are never created, removed or disconnected.
- **`status`**: live state, size and what runs on it; a stopped VM answers
  without being started.
- **The built-in machines' settings.** `native` and `docker` are never
  created, so `machine update native` and `machine update docker` set their
  image, Docker socket and WSL distribution (proposed, O11; the same call
  resizes a VM). Their binaries come from the per-build cache (O16).
- **Idle.** The worker owns idleness. It closes a Store after one quiet
  minute, as today. After some minutes with no open Store and no Sandbox
  (about 10, proposed, O2), it exits and, where the provider allows, powers
  off its guest. An ssh machine stops only its worker and forward; the host
  keeps running. The next call notices the stop and starts the machine
  again. That is a readiness wait, not a retry.

**Setting one up.** Install the platform's tool once; LayerFS does the rest.

| Platform | Install once | Host needs |
| --- | --- | --- |
| `lima` | `limactl` | macOS only today: `vmType` is fixed to `vz`. A Linux host needs a qemu template (not designed) |
| `apple-container` | Apple's `container` CLI | Apple silicon, macOS |
| `microsandbox` | `msb` | what microsandbox supports |
| `firecracker` | nothing more | a `lima` machine made with `--nested`: M3 or newer, macOS 15+, `/dev/kvm` in it; `mkfs.ext4`, `ip` and `flock` in that Lima host |
| `ssh` | nothing locally | a reachable Linux host, root login by key, its data root on ext4. Until the worker is the `layerfs` binary ([§11 step 14](host-platform-connections.md#11-plan)), Node.js 22 on root's PATH |

`create` makes the VM and its ext4 data disk, mounted at `/layerfs/data`,
and copies in the sealed Linux bundle (daemon, worker, Sandbox image), once
per bundle. It then starts the worker as root behind a new secret, with user
namespaces blocked in the VM, and forwards one loopback port to it. The
forward is started detached and recorded in `machine/<name>/worker.json`, so
it outlives a one-shot command. Lima installs a pinned Node.js 22 on first
boot, so a first create needs the network.

`connect` does the same over ssh after checking that the host is Linux and
its root is on ext4. It opens the forward first, then copies the bundle. It
sets `user.max_user_namespaces=0` for the whole host, mounts or bind-mounts
the root, and writes `/layerfs/bundle` and `/layerfs/state`. Where the
bundle comes from once `LAYERFS_HOME` is the only setting is open (O16).

### 3.8 `store` (8)

```js
store.list()
store.get(name)
store.status(name)
store.create(name)
store.connect(name, url, { password, synchronous_commit })
store.move(name, machine, { on_progress })
store.remove(name)
store.disconnect(name)
```

The verb says the kind: `create` makes a sqlite Store, `connect` attaches a
postgresql one.

**`store.list()`**: every Store, from the home's records, with no network
call.

```
$ layerfs store list --text
NAME      KIND        MACHINE  PROJECTS   SIZE     BACKUP
shop      sqlite      docker   api, web   1.2 GB   r2-main/shop: Store changed since backup
web       sqlite      native   web        80 MB    r2-main/web: in sync
tmp       sqlite      dev      tmp        10 MB    -
pg-main   postgresql  -        load       -        -
```

Every process that opens a sqlite Store holds a lease for it in the home, so
"held" below means Sandboxes or any process with the Store open.

- **`get`**: the Store's record: its kind, its machine or URL (without a
  password), its projects and its backup links.
- **`status`**: sqlite: its machine, size, whether it is open, and whether a
  move is in progress (proposed: from its leases and an `in_progress`
  field), read from the machine. postgresql:
  `{ reachable, server_version, latency_ms, bytes }` from one connection.
- **`create`**: an empty sqlite Store on `native`.
- **`connect`**: `url` is `postgresql://<user>@<host>:<port>/<database>[?sslmode=…]`,
  without a password; `password` is a reference (§2.8). One connection checks
  it: a database that holds LayerFS data is read and its projects listed (a
  schema with another name is O6); an empty one is recorded and the first
  `project.init` creates the schema; `Unavailable` or `Denied` records
  nothing; a second name for the same database is `Exists`.
  `synchronous_commit` defaults to `off`, as today.
- **`move`**: moves a sqlite Store to another machine. A move between two
  machines other than `native` goes through `native` in the same call
  (proposed, O12; otherwise it is refused and asks for two moves). Each step
  snapshots on the source machine, sends, verifies (sha256, the file opens,
  history heads), installs and removes the old copy. Sending from a guest to
  the host is new work: today's copy runs from the host into a guest only.
  `InUse` while the Store is held; `InvalidInput` for a postgresql Store.
  `on_progress` is §2.6's stream.
- **`remove`**: deletes a sqlite Store's file, wherever it is, with every
  project in it; its backups stay. **`disconnect`**: forgets a postgresql
  Store; the data stays on the server. Both are `InUse` while the Store is
  held, and each is `InvalidInput` on the other kind.

### 3.9 `backup` (5)

```js
backup.connect(name, url, { key, region, part_size })
backup.disconnect(name)
backup.list(bucket?)
backup.create(store, bucket, file?, { on_progress })
backup.restore(store, bucket, file?, { on_progress })
```

Backups are for sqlite Stores only; a postgresql Store is `InvalidInput`.
Buckets are r2 or MinIO; 1.1.0's `local_folder` provider is not in 2.0. A
Store has one link per bucket, so it can be backed up to several buckets
(proposed, O23; #509 allowed one link per Store).

A backup file `file` is `<prefix>/<file>/store.sqlite`: the whole Store file,
or, when it is large, a manifest there plus pieces
`store.sqlite.piece-NNNNNN`. `file` is an optional last positional and
defaults to the Store's name.

- **`connect`**: `url` is endpoint, bucket and prefix. A host
  `<account>[.<jurisdiction>].r2.cloudflarestorage.com` is R2 (region `auto`,
  a build with the `tls` feature); any other host is MinIO. `key` is a
  reference (§2.8). One request checks the bucket; `Unavailable`, `NotFound`
  or `Denied` records nothing. The same bucket and prefix twice is `Exists`.
- **`disconnect`**: forgets the bucket and this home's links to it; its
  objects stay. A linked bucket can be disconnected; this replaces 1.1.0's
  `store.unlink`.
- **`list(bucket?)`**: every Store file on every connected bucket, or one,
  checked live.

```
$ layerfs backup list --text
BUCKET    FILE   PULLED HERE           EXPORTED FROM      AT                 STATE
r2-main   shop   shop (now on docker)  docker             2026-11-01 18:02   Store changed since backup
r2-main   web    web (now on native)   native             2026-11-02 09:10   in sync
r2-main   ci     no                    dev (other home)   2026-10-30 11:40   -
lab       shop   no                    dev                2026-10-28 08:00   -
lab       -      -                     -                  -                  connected, empty
```

| Column | Meaning |
| --- | --- |
| PULLED HERE | the Store in this home linked to the file and its machine now; `no` if none |
| EXPORTED FROM | the machine the Store was on when the file was written, and whether that was this home |
| STATE | `in sync`; `Store changed since backup` (new Commits here); `s3 changed since` (written by someone else); `both changed` |

One listing per bucket plus at most 64 HEAD requests, as `remote.list_stores`
does today. EXPORTED FROM needs each backup to write the machine and the
home's id into the object's metadata (`layerfs-exported-from`); files written
before that show `-`.

- **`create(store, bucket, file?)`**: uploads a snapshot to the file and
  links the Store to it. The snapshot is taken on the Store's own machine and
  the bytes pass through the caller's process, so machines never hold bucket
  keys. This is new work: today's s3 I/O runs inside the worker. `Exists`
  when the object is there and its `layerfs-exported-from` names another
  home. To copy a Store to a second bucket (1.1.0's `start_send`), back it up
  there too.
- **`restore(store, bucket, file?)`**: downloads the file (default `file` =
  `store`), verifies it and installs it as the sqlite Store `store` on
  `native`: a new Store, or replacing one that is on `native` and idle.
  Replacing is `Conflict` when the Store changed since its last backup or
  restore; remove it first to discard those changes. `InvalidInput` while the
  Store is on another machine ("move it to native first"). The home's
  project records for the Store are rebuilt from it.

Each link lives in the Store's record, `store/<name>/record.json`:
`backups: { <bucket>: { file, heads, last: { create|restore, at, bytes } } }`.

## 4. Walkthroughs

### 4.1 A project in Docker

```bash
layerfs store create shop
```

```bash
layerfs project init shop/api ~/src/api
```

```bash
layerfs store move shop docker
```

```bash
layerfs sandbox create shop/api tests docker
```

```bash
layerfs workspace mount tests main
```

```bash
layerfs workspace exec tests:/workspaces/1 "npm test" --stream
```

```bash
layerfs workspace commit tests:/workspaces/1 -m "fix tests"
```

```bash
layerfs workspace unmount tests:/workspaces/1
```

```bash
layerfs sandbox close tests
```

The same in code:

```js
import { store, project, sandbox, workspace } from "layerfs"

await store.create("shop")
await project.init("shop/api", "/Users/me/src/api")
await store.move("shop", "docker")
const sb = await sandbox.create("shop/api", "tests", "docker")
const ws = await workspace.mount("tests", "main")
await ws.exec("npm test", { stream: true })
await ws.commit({ message: "fix tests" })
await ws.unmount()
await sb.close()
```

And in Python, once the Store is on `docker`:

```python
from layerfs import project, sandbox, workspace, LayerFSError, home

sb = sandbox.create("shop/api", "tests", "docker")
ws = workspace.mount("tests", "main")
ws.exec("npm test", stream=True)
ws.commit(message="fix tests")
ws.unmount()
sb.close()
```

### 4.2 A bigger VM

```bash
layerfs machine create big lima --cpus 16 --memory 64G --disk 200G
```

After §4.1 the Store is on `docker`, so this move goes through `native`
(proposed, O12):

```bash
layerfs store move shop big
```

```bash
layerfs sandbox create shop/api build big
```

When it is done, close the Sandbox, move the Store home and delete the VM.
The move is `InUse` while the Sandbox runs.

```bash
layerfs sandbox close build
```

```bash
layerfs store move shop native
```

```bash
layerfs machine remove big
```

### 4.3 Firecracker and ssh

```bash
layerfs machine create fc-host lima --nested --cpus 8 --memory 16G --disk 200G
```

```bash
layerfs machine create fc1 firecracker --host fc-host --cpus 2 --memory 2G --disk 20G
```

The ssh host's key must already be in `~/.ssh/known_hosts`. `--root` is left
at its default, `/layerfs/data`.

```bash
layerfs machine connect ec2 ec2-1-2-3-4.compute.amazonaws.com --user root --identity /Users/me/.ssh/deploy
```

Each is used like any machine: `store move shop fc1`, then
`sandbox create shop/api t1 fc1`.

### 4.4 A PostgreSQL project

```bash
layerfs store connect pg-main "postgresql://layerfs@db.internal:5432/layerfs?sslmode=require" --password env:LAYERFS_PG_MAIN
```

```bash
layerfs project init pg-main/load ~/src/load
```

```bash
layerfs sandbox create pg-main/load load-1 docker
```

### 4.5 History without a Sandbox

```bash
layerfs fs read shop/api main /src/index.ts
```

```bash
layerfs commit list shop/api main
```

```bash
layerfs branch fork shop/api fix main
```

```bash
layerfs fs edit shop/api fix /src/index.ts --old "retries = 3" --new "retries = 1" -m "one attempt"
```

### 4.6 Backups

```bash
layerfs backup connect r2-main https://<account>.r2.cloudflarestorage.com/layerfs/stores --key env:LAYERFS_R2_MAIN
```

```bash
layerfs backup create shop r2-main
```

```bash
layerfs backup list r2-main --text
```

On another computer with its own home, connect the bucket there, then
restore a new Store from that file:

```bash
layerfs backup connect r2-main https://<account>.r2.cloudflarestorage.com/layerfs/stores --key env:LAYERFS_R2_MAIN
```

```bash
layerfs backup restore shop r2-main
```

## 5. Changes from 1.1.0

| | 1.1.0 | 2.0.0 |
| --- | --- | --- |
| Packages | `@layerfs/agent`, `@layerfs/runtime`, `@layerfs/backend`, `@layerfs/cli` | one, `layerfs`: a binary, npm and PyPI |
| The command | a Node program | a native binary; no Node |
| Python | none | `layerfs` on PyPI, the same calls |
| Worker in a VM or ssh host | Node.js 22 running the SDK | the `layerfs` binary (`layerfs worker`) |
| `LayerFS.init(name, folder, timeout?)` | a bare project name; the third argument a bare number of ms | removed, no alias: `project.init(<store>/<project>, folder, { timeout })`. The name gains its Store, and the third argument becomes `{ timeout }` |
| `LayerFS.open(project)`, then calls on the client | | removed: every call takes its project |
| Settings | 56 `LAYERFS_*` names, listed in [the proposal's §3.1](host-platform-connections.md#31-all-56-settings-by-kind) | `LAYERFS_HOME`, trace and measurement settings (§2.5) |
| Families | `project`, `branch`, `fs`, `sandbox`, `workspace` | adds `commit`, `machine`, `store`, `backup` |
| Project | `--project api`, default `$LAYERFS_PROJECT`; `{ store }` when ambiguous | a first argument, always `<store>/<project>` |
| `layerfs init <name> <source-dir>` (top level) | a new Store of its own, or the one `LAYERFS_STORE` names | `project init <store>/<project> <dir>`, into an existing Store |
| `project list_branches` | | `branch list <project>` |
| `branch fork <name> [<from-ref>]` | | `branch fork <project> <name> <from-branch> [<from-commit>]` |
| `branch merge <a> <b> <name>` | | `branch merge <project> <name> <a> <b>` |
| `branch list_commits`, `diff_commits` | | `commit list`, `commit diff` |
| `branch diff_branches` | | `branch diff` |
| `fs <call> <ref> <path>` | ref: a Branch, Commit or Layer id | `<project> <branch> <path> [<commit>]` |
| `fs glob`, `grep` `<ref> <pattern> [<path>]` | | `<project> <branch> <path> <pattern> [<commit>]` |
| `fs diff_files` | | `fs diff` |
| `sandbox create` | no name: the lowest free number; `--image`, `--platform` (#529) | `<project> <name> <machine>`, `[--image]`; the machine replaces `--platform` |
| `sandbox list_workspaces <sandbox>` | | `sandbox get <name>` |
| `project list_sandboxes` | | `sandbox list [<project>]` |
| `workspace mount <sandbox> <branch>` | a Sandbox number; returns `{ sandbox, path, image }` | a Sandbox name; returns a handle, `{ ws, sandbox, branch, path }` |
| Workspace handle | `3:/workspaces/1` | `tests:/workspaces/1` |
| `workspace exec`, `commit`, `unmount` | positional `message`, `force`, `discard`, and `{ stdout, stderr }` sinks | the same as options: `{ message, stream, stdout, stderr }`, `{ message, force }`, `{ discard, force }` |
| Paging | positional `pageSize`, `pagePos` in the SDK; `cursor` on diffs | `{ page_size, page_pos }` and `cursor` as options on the same calls, plus `project.list` |
| Keys | camelCase options and results (`startLine`, `exitCode`) | snake_case in every front end |
| Exit codes | 0, 2, 125; `--stream` passes the command's code | the same; §2.6 draws the line between 2 and 125 |
| Errors | 30 codes (`CODES`) | the same 30 for the 1.1.0 families; machine, store and backup add `NotFound`, `InUse`, `Unsupported`, `Unavailable`, `Denied`, `Failed`, `Unknown` (§2.9) |
| `workspace status` | in `@layerfs/backend` (`backend.daemon.workspace.status`), not in the SDK or CLI | not in 2.0 (O14) |
| Runtime and backend calls | | §5.1 |

### 5.1 Other 1.1.0 calls

Every other 1.1.0 call and export has a fate below.

**`@layerfs/backend` (14).** None is among 2.0's 54 calls. O25 proposes
leaving them out of 2.0's surface and keeping what Connect needs behind trace
files.

| 1.1.0 | 2.0.0 |
| --- | --- |
| `store.project.get_history`, `get_storage`, `get_comparable`, `list_overwritten` | not in 2.0 (proposed, O25) |
| `store.branch.list_commit_storage` | not in 2.0 (proposed, O25) |
| `store.fs.get_storage`, `list_revisions` | not in 2.0 (proposed, O25) |
| `daemon.workspace.status` | not in 2.0 (O14) |
| `daemon.workspace.list_changes`, `get_storage` | not in 2.0 (proposed, O25) |
| `daemon.process.start`, `list`, `cancel` | not in 2.0 (proposed, O25) |
| `daemon.sandbox.get_log` | not in 2.0 (proposed, O25) |

**`@layerfs/runtime`.**

| 1.1.0 | 2.0.0 |
| --- | --- |
| `status` | `machine.list`, `store.list` |
| `worker.start`, `worker.restart` | automatic: the worker starts on demand and stops when idle (§3.7) |
| `project.list` | `project.list` |
| `project.start_init`, `get_init` | `project.init` with `on_progress` (proposed, O22) |
| `project.get_state` | `project.get` |
| `project.delete` | none; open (proposed, O24: `project.remove(project)`) |
| `store.list`, `create`, `remove` | `store.list`, `create`, `remove` |
| `store.attach_postgresql` | `store.connect` |
| `store.start_publish`, `start_push` | `backup.create` |
| `store.start_send` | `backup.create` into a second bucket |
| `store.start_pull`, `start_load` | `backup.restore` |
| `store.start_import_postgresql` | none: no import between kinds |
| `store.unlink` | `backup.disconnect` (proposed, O23) |
| `store.check` | `store.status` |
| `remote.list`, `add`, `remove`, `list_stores` | `backup.list`, `connect`, `disconnect`, `list` |
| `remote.providers` | none: `backup.connect` checks the URL |
| `postgresql.status` | `store.status` |
| `sync.get` | `on_progress` (proposed, O22) |
| `sandbox.reclaim`, `list_leftovers`, `clean_leftovers`, `list`, `sweep` | `sandbox.list` and automatic cleanup |
| `volume.list`, `remove` | `store.list`, `store.remove` |
| `vm.status`, `stop`, `delete` | `machine.status`, idle stop, `machine.remove` |
| `events.subscribe` | none: trace files under `trace/` are the feed (proposed, O26) |
| `close` | none |

**`@layerfs/agent` exports.**

| 1.1.0 | 2.0.0 |
| --- | --- |
| `LayerFSError`, `CODES`, `isRefusal` | kept |
| a client's `close()` and `Symbol.asyncDispose` | none: there are no clients |
| `LayerFS.call(method, args)` | `call(name, args)` from the call table, exported for scripts (proposed) |
| `RENAMED_METHODS`, `currentMethod` | removed |
| `INIT_TIMEOUT_MS` | kept: `project.init`'s default timeout |

## 6. Open questions

Each entry carries its number in
[the proposal's §13](host-platform-connections.md#13-open-decisions), which
holds the full question and the alternatives.

- **O1. Secret references.** `env:` and `file:`, as today, or also a
  system keychain.
- **O2. Machine idle time** before a VM stops: about 10 minutes is suggested.
- **O3. PostgreSQL on `native`, VMs and ssh:** `native` needs a build with
  the `postgresql` feature and a test; VMs and ssh also need a network path
  and the password inside the machine.
- **O4. Old settings:** removed in 2.0, with `layerfs import-env` writing
  records from the environment once.
- **O5. Windows:** the home's sqlite Stores and Init in the WSL worker, and
  Sandboxes on `docker`; nothing here was run on Windows.
- **O6. PostgreSQL Stores in other schemas:** whether `connect` opens a
  database whose one LayerFS schema has another name.
- **O7. `native` and `docker` Stores together on macOS,** as §3.8's listing
  shows.
- **O9. The old Store calls:** removed in 2.0 (§5.1), or kept one release as
  wrappers.
- **O10. `native` or `local`** as the built-in machine's name.
- **O11. `machine update`:** resizing a VM (whether it must restart), and
  setting the built-in machines' image, Docker socket and WSL distribution.
  It would be a 55th call.
- **O12. A move between two non-native machines:** through `native` in one
  call, or refused, asking for two moves.
- **O13. `fs` and Node's `fs`.** `import { fs } from "layerfs"` clashes with
  `import fs from "node:fs"` in most Node files (Python has no such clash).
  Renaming the family `file` (`file.read`, `layerfs file read`) avoids it.
- **O14. `workspace.status`** is left out to keep `workspace` at four calls;
  agents use it today to see a Workspace's changes before a commit.
- **O15. Package names:** `layerfs` was not checked on npm, PyPI or
  crates.io; `@layerfs/layerfs` is the npm fallback.
- **O16. The Linux bundle.** VMs, ssh hosts and `docker` on macOS need it;
  today it is the runtime's `bundle` setting. Suggested: fetched once per
  build into the user's cache, checked by hash, pinned by the `layerfs`
  version. The built-in machines' binaries come from the same cache.
- **O17. Firecracker's files** (binary, guest kernel, base root filesystem,
  guest key) are placed in the Lima host by hand today. The bundle holds
  neither the guest kernel nor the root filesystem, so adding them is new.
  The root filesystem must carry Node.js 22 and an sshd that accepts the
  guest key. `--nested` could provide them.
- **O18. ssh Store roots** are ext4 only today; xfs and btrfs would match the
  Linux Store rule.
- **O19. Python's API style:** synchronous `import layerfs` plus
  `layerfs.aio`, or asyncio only.
- **O20. Prebuilt targets:** Linux x64 and arm64, macOS arm64 and Windows are
  suggested; musl and macOS x64 when asked.
- **O21. The library crate.** O21a: grow `layerfs-api` in place
  (recommended), not a new crate. O21b: where the crates live; recommended,
  they stay in `engine/crates/` for 2.0.
- **O22. Progress for long calls:** one stream (§2.6, recommended) or jobs.
- **O23. Backup links:** one per Store per bucket (§3.9, recommended),
  reversing #509's one link per Store.
- **O24. Removing one project:** add `project.remove(project)`, a 55th call
  (recommended), or only `store.remove`.
- **O25. Operator and diagnostic calls** from `@layerfs/backend` (14 calls):
  dropped from 2.0's surface (recommended), or kept as an `inspect` family
  outside the 54.
- **O26. The trace feed** (`events.subscribe`): trace files under `trace/`
  are the feed, with no call (recommended).
- **O27. The Linux native backend:** remove the host-process pairing on
  Linux (recommended), so `native` on Linux always runs namespaces.
- **O28. Unset `LAYERFS_HOME`:** default `~/.layerfs` (recommended).
- **O29. `fs.diff`'s arguments:** positional, matching the shell
  (recommended, §3.4), or two `{ branch, path, commit }` objects.
- **O30. Commit numbers.** Commits are ids; per-Branch numbers (Commit 12 of
  `fix`) would be a new index.
- **O31. Forcing a Sandbox closed:** `sandbox.close(name, { force })`
  (recommended).
- **O32. MCP and folder scope (#521):** folder scope as a client option of
  every front end; MCP a thin front end over the call table, or dropped
  (recommended).
- **O33. Container names per home:** the home id from `layerfs.json`
  (recommended).
- **O34. Machine facts:** `machine.status` reports state, free disk, tool
  version and why it is down; no change event (recommended).
- **O35. A stale lease:** removed under the home lock when its process is
  gone (recommended).

## 7. Related

- [#533](https://github.com/Ephemeral-AI-Lab/layerfs/issues/533): the implementation spec and work order,
  [core/docs/issues/533/README.md](../issues/533/README.md).
- [One control center for every platform](host-platform-connections.md): the
  home layout, machines, workers, routes and the plan these calls build on.
- `api/cli/src/`, `api/agent/src/`, `api/runtime/src/`, `api/backend/src/`:
  the 1.1.0 packages; `engine/crates/layerfs-node/`: today's napi binding;
  `engine/crates/layerfs-api/`: today's Rust method table.
- #506, #509: Stores, remotes and links; #527, #529: placement and Sandbox
  platforms.
