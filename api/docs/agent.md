# Using the agent SDK and the `layerfs` command

> **Status:** Current implementation guide for `@layerfs/agent` and the
> [`layerfs` command](#the-layerfs-command) in LayerFS 1.1.0; not a release
> receipt. Part of the [JavaScript packages wiki](../README.md).

`@layerfs/agent` is what works on a project: it reads and writes files, forks
and merges Branches, and mounts Workspaces in Sandboxes to run commands. Agents
use it, and so does any tool that does an agent's kind of work. There are two
ways in, with the same calls and the same answers:

- [**The SDK**](#the-sdk): a Node script that imports `@layerfs/agent` and
  makes many calls with one open client.
- [**The `layerfs` command**](#the-layerfs-command): one call per command, for
  an agent whose only tool is Bash.

## The SDK

### A first script

```js
import { LayerFS } from "@layerfs/agent";
// Settings come from the environment: LAYERFS_STORE_PATH is the Store file Store
// calls open (here the project's volume is mounted at /layerfs-store).
process.env.LAYERFS_STORE_PATH ??= "/layerfs-store/global/store.sqlite";
const layerfs = await LayerFS.init("website", "./website"); // optional 3rd argument: timeout in ms
// another process opens the project instead: await LayerFS.open("website")
await layerfs.branch.fork("feature");
await layerfs.fs.write("feature", "/README.md", "Hello\n", "Update introduction");
const page = await layerfs.branch.list_commits("feature", 20, 0);
const text = await layerfs.fs.read("feature", "/README.md", 1, 20);
const { sandbox } = await layerfs.sandbox.create();  // "1": the only call that starts a Sandbox
const ws = await layerfs.workspace.mount(sandbox, "feature"); // { sandbox: "1", path: "/workspaces/1" }
await layerfs.workspace.exec(ws, "npm test", "Run tests");
await layerfs.workspace.commit(ws, "Save changes");
await layerfs.workspace.unmount(ws);
await layerfs.close();
```

### Opening a project

A client is made by `LayerFS.open(name)` (an existing project) or
`LayerFS.init(name, folder, timeout?)` (Init, then the same open); both are
async and return a ready client, and `new LayerFS()` is refused
`InvalidInput`. **There is no options argument:** every setting comes from the
environment, read once when the client is made, by its `LAYERFS_` name only:
`LAYERFS_HOME`, `_STORE_PATH`, `_DOCKER`, `_IMAGE`,
`_VOLUME`, `_DAEMON`, `_CONTAINER_PREFIX`, `_DAEMON_HOST`, `_TRACE`,
`_STORE_POLICY` (and `_TRACE_STEP`, read at each call). An object passed where
the options went (`LayerFS.open(name, {…})`, or an object as `LayerFS.init`'s
third argument) is refused `InvalidInput`, never ignored. `open` opens the project's Store and makes one project lookup
on it, so a missing project is `NoSuchProject` at once; if either fails,
nothing stays open. `close()` releases the Store (the process's last client of
it closes it), leaves Workspaces mounted, and makes every later call refuse
`InvalidInput`; a second `close()` does nothing. `await using layerfs = await
LayerFS.open(...)` closes it at the end of the block. One copy of
`@layerfs/agent` loads per process: a second copy at another path refuses to
load, so one Store is never opened twice. `LayerFS.init`'s client carries
Init's answer as `created`: `{ kind: "Initialized", project, layer, root, store }`,
where `store` is the installed Store file (`LAYERFS_STORE_PATH`). Init seals
the Store as `<LAYERFS_HOME>/<project>/sealed.sqlite`, installs it at the Store
path and records that, then deletes the sealed copy (owner 2026-10-23: nothing
reads it again). A failed or uncertain install keeps it for diagnosis. If the
delete itself fails, the project is still usable and the answer carries
`leftover: { path, message }`; it is not retried.

### Init

`LayerFS.init(name, folder, timeout?)` takes an optional `timeout` in
milliseconds, a positive whole number. **Default 300 s (`INIT_TIMEOUT_MS`,
exported); there is no unlimited option** (#431; the addon's own fixed 60 s is
gone). The agent always sends it as `timeoutMs`, which the native code requires
(it has no default of its own) and counts from where Init runs, on every route
(in process, the macOS Init container, the Windows worker). An Init that runs
longer fails `Failed` with a `Deadline` message, once, keeping its entry as any
failure that is not a source refusal does. Any other `timeout` is refused
`InvalidInput` before anything is made.

`LAYERFS_STORE_POLICY`, a JSON object
`{ smallFileThreshold, wholeFileDepth, chunkDepth, metadataDepth }`, selects the
Store policy Init records instead of the default, each field defaulting to the
default policy's value (131072 bytes, depths 16, 8 and 8). Text that is not
such an object, or an invalid field, is refused as `InvalidInput` naming
`LAYERFS_STORE_POLICY`, before the project entry is made.
A source entry Init cannot store is refused as `InvalidInput` whose `detail`
names it: `{ reason, path }`, plus `mode` (octal text) for `Mode`. The reasons
are `NotDirectory` (the source is a symlink or not a directory), `Mode` (a
set-user-ID or set-group-ID bit on any entry, or the sticky bit on a file),
`Mtime` (a modification time out of range) and `Changed` (the entry changed
while Init read it). Nothing is published and the project entry is removed, so
the name can be initialized again once the source is fixed.
The recorded policy governs the Store for its life. Project names use 1–63 lowercase ASCII
letters, digits, `_`, `-`, `.` and start/end alphanumeric. Init is explicit;
subsequent clients discover the existing project without reacquiring its source
or reinstalling over live state.

### Where calls run

Store calls (`fs`, `branch`, `project.list_branches`) run in this process
against the installed Store file at `storePath` (else `LAYERFS_STORE_PATH`),
which the factory opened; no Sandbox is involved. That
file is `global/store.sqlite` in the project's volume, so the process runs where
that volume is mounted (`/layerfs-store` in a Sandbox). This is Linux only, with
the Store on a local filesystem the daemons share (ext4, xfs, btrfs); anywhere
else opening fails `Unsupported`, and without a `storePath` it fails
`InvalidInput`. On macOS and Windows, any process that names its LayerFS
home's worker (`LAYERFS_WORKER_ENDPOINT`, or the endpoint `@layerfs/runtime`
hands over with `useWorker`) sends the calls there instead. The route is open to any integration (owner,
2026-10-24); LayerFS names none. A client keeps the binding it was made with: the
in-process addon, or the worker if one was set when it was made, so a client
made before `@layerfs/runtime` hands over its worker stays in-process and
every later one uses the worker. Mount does not scan or materialize the complete tree.
A project is its home entry, `LAYERFS_HOME/<name>/`; the binding keeps no other
record of which names it initialized. Init refuses a name whose entry exists
(`Exists`). Once that entry is deleted, after the project's Sandboxes are
removed, any process, including the one that made it, can initialize the name again.
The SDK has no remove method; a host that removes a project deletes the entry
and the project's volume itself. `node api/agent/test/reinit.mjs` checks this with
the real binding: Init, `Exists`, delete the entry, Init again in one process.

### Calls in parallel

Parallel agents each mount their own Workspaces, each in a Sandbox the
mount names (see below); there is no placement. A mount takes only its
Sandbox's own lock; `sandbox.create` and `sandbox.close` also take the
project's. Exec, status, Commit and unmount take neither.
Concurrent calls to one Sandbox from one process use separate control
connections, up to six (a Sandbox admits eight), so a read does not wait behind
another call's Commit; each call's exchanges stay on one connection and are sent
once. The binding sets `UV_THREADPOOL_SIZE` to 64 when the process has not set
it, because every call, a long `exec` included, holds a Node worker thread.
Keep each agent's operations on a Workspace sequential.

### Settings

| Environment | Meaning |
| --- | --- |
| `LAYERFS_HOME` | Absolute private directory for discovery records (projects, Sandboxes) and Init's temporary sealed Store, deleted once its install is recorded. Created as 0700; existing directories must be owned by the current effective user with no group/other access. Records are 0600, not symlinks. |
| `LAYERFS_DOCKER` | Local Engine Unix socket; `/var/run/docker.sock` is the default. On the macOS worker route the worker and Init's container use the engine's socket inside its VM instead (the endpoint's `docker`). |
| `LAYERFS_IMAGE` | Exact immutable runtime `sha256:...` digest a new Sandbox runs. A running Sandbox keeps the image it started with. |
| `LAYERFS_VOLUME` | Existing named local VM volume shared by the project's daemons. |
| `LAYERFS_DAEMON` | Host path to the built Linux daemon executable. |
| `LAYERFS_NATIVE` | Optional path of the real addon; defaults to `engine/crates/layerfs-node/layerfs.node`. |
| `LAYERFS_WORKER_ENDPOINT` | macOS and Windows: the endpoint file of the LayerFS home's worker, which then makes every binding call. Any integration may set it. |
| `LAYERFS_STORE_PATH` | The installed Store file this client's Store calls open: `global/store.sqlite` in the project's volume. On Linux this process opens it, so it must be on a local filesystem the daemons share, such as `/layerfs-store/global/store.sqlite` where the volume is mounted at `/layerfs-store`; otherwise Store calls fail `Unsupported`. On macOS and Windows the worker opens it, so it is the path inside the worker's Linux, `/var/lib/docker/volumes/<volume>/_data/global/store.sqlite`. On any other platform Store calls fail `Unsupported`. Unset, they fail `InvalidInput` everywhere. |
| `LAYERFS_CONTAINER_PREFIX` | Optional. Names every container this client creates `<prefix><project>-<Sandbox>` (default prefix `layerfs-`); another LayerFS home on the same Docker engine sets its own, so names never collide. Docker's name rule applies (an ASCII letter or digit, then letters, digits, `_`, `.`, `-`, at most 128). A name another container holds refuses the create before any effect. |
| `LAYERFS_DAEMON_HOST` | Optional. The host this process dials for a daemon's port. Each daemon publishes its port on the Docker host's `127.0.0.1` only, and that stays so. A client inside a container does not share that loopback, so it names the host that reaches it, `host.docker.internal` on Docker Desktop. One resolution, its first address, one connect; nothing is tried again. Unset, the client dials `127.0.0.1`. |
| `LAYERFS_TRACE` | Optional per-call trace file (see [the call trace](backend.md#the-call-trace)); unset means no trace. |
| `LAYERFS_TRACE_STEP` | Optional step label put on each trace line as `step`; read at every call. |
| `LAYERFS_TRACE_START` | `1` adds a start line before each traced call; read at every call. |

The Docker Engine server selected by `LAYERFS_DOCKER` must speak Engine API
v1.48 (Docker 28.0) or newer. Before its first request the SDK reads the
server's API version once with an unversioned `GET /version` and uses
`/v1.<minor>/...` paths at that version, up to v1.54 (a newer server is
addressed as v1.54). A server older than v1.48 is refused before any other
request, with an error naming the server's version and the supported range:
earlier Engines fail to upload the daemon. If `GET /version` cannot be read,
the SDK uses v1.54, and the server's own refusal of it, if any, is the error. A
failed request is never resent at another version. Check the server's API range
with `docker version`.

The SDK and the `layerfs` command read only these `LAYERFS_` names. An
integration that can set only names with its own prefix maps them to these in
its own wrapper.

### Sandboxes and Workspaces

A Sandbox is a container running one daemon on the project's Store. The
project has numbered Sandboxes, `1`, `2`, …, each on one image and holding up
to 16 Workspaces. **There is no placement** (owner 2026-10-23):
`sandbox.create({ image }?)` is the only call that starts one. Under the
project's lock it first reclaims Sandboxes that closed themselves, then
starts the lowest free number on `image` (default `LAYERFS_IMAGE`) and
returns `{ sandbox, image }`. One project's Sandboxes may run different
images; the mount result and `project.list_sandboxes()` give each one's
`image`. `workspace.mount(sandbox, branch)` (both required, no options)
mounts in that Sandbox only, on its image; the native call gets
`{ sandbox, branch }` with the client's project, home, deploy and Store
path. It is refused `NoSuchSandbox` when the Sandbox is not running (or
closed itself), with a message that says to create one with
`sandbox.create()` and mount there, and `Capacity` when it holds 16
Workspaces, with a message that says to create a new Sandbox with
`sandbox.create()`, or list its Workspaces with
`sandbox.list_workspaces(sandbox)` and unmount the ones that are done with
`workspace.unmount`. `sandbox.close(sandbox)` stops one now (stop, then
delete, each once), refused `MountsActive` while a Workspace is mounted in it
and `NoSuchSandbox` when it is not running. A Sandbox still closes itself two
minutes after its last Workspace is unmounted, with no call in flight.
`project.list_sandboxes(pageSize, pagePos)` lists the running ones, each
`{ sandbox, name, image, workspaces }`, reading records and starting and
changing nothing; `sandbox.list_workspaces(sandbox, pageSize, pagePos)` asks
that Sandbox's daemon for its Workspaces, each `{ sandbox, path, branch,
base, activity, changed }` (`activity` Idle or Busy) plus the daemon's record. A Workspace is
`{ sandbox, path }`, its Sandbox's number and its path in that Sandbox
(`/workspaces/1`, `/workspaces/2`, … numbered per Sandbox); `exec`, `status`,
`commit` and `unmount` take it. Every Workspace of a project sees the same
Store, so which Sandbox holds it changes nothing a command sees. A Sandbox's
container is `<prefix><project>-<n>`, labelled `layerfs.role=sandbox`,
`layerfs.project` and `layerfs.sandbox=<n>`; its only mount is the project's
Store volume. Its records are `LAYERFS_HOME/<project>/sandboxes/<n>/`, with
the container id in the plain-text file `container` there (for
`docker logs`). A Sandbox starts with the image the deployment names then and
keeps it for its life.

A Sandbox closes when idle: its daemon ends itself after 2 minutes with no
Workspace and no call, and its container stops. Open connections do not keep
it. `project.list_sandboxes()` skips a stopped Sandbox and keeps its
records; the next mount or `runtime.sandbox.reclaim` that meets a stopped or
removed container reclaims the Sandbox: it deletes a stopped container
(unforced, once) and removes the Sandbox's records, so the number is used
again. `runtime.sandbox.sweep`, which the runtime runs once when it opens,
does that for every project of the home, so a stopped Sandbox's scratch does
not wait for its project's next mount.

What commands write outside a Workspace (`/tmp`, which is also `HOME`, and
the npm, node and cargo caches there) stays in the container's own writable
layer until the container is deleted; `runtime.sandbox.list` reports it as
`scratch` bytes (#423). The daemon's log is capped at two 10 MB files. A Workspace call to a Sandbox whose container no longer runs is
refused as `NoSuchSandbox` with `detail.containerGone`: Docker no longer has
the container (removed outside LayerFS), or has it stopped
(`detail.containerStopped`, after Docker restarted, for instance), which lost
its daemon and every mount in it.

The same authorized home permits a fresh Node process to use a Workspace
directly. Reopening validates the full container id, deployment, volume,
topology, loopback endpoint, Noise peer and saved daemon incarnation.
Discovery changes take OS locks and replace records atomically; a lock
another process holds is waited for up to 60 seconds, then refused as
`Busy`. A start intent is written only after the request's inputs are read.
A start that fails before anything reaches the Store volume (the container
never became ready, for example a daemon that exits at startup) stops and
deletes the container it made, once each, removes its intent and frees its
number, then reports the original failure as `Failed` with `detail.container`
and `detail.removed`. Before the delete it reads the container's last 20 log
lines once, at most 2048 bytes, into `detail.log` and the message; a log it
could not read leaves `detail.log` null and the message says so. One whose
effect is unknown (an interrupted process, an Engine create whose outcome was
lost, or a container that could not be removed) keeps the intent, which
records what is known. Every later start in that project is then `Unknown`,
and its message says what to remove (`docker ps -a --filter volume=<volume>`)
before deleting `<home>/<project>/create.intent`.
Daemon/container restart recovery and replay of uncertain operations are excluded.
Background process ownership is in the native executor, including persisted
process identity; the filesystem daemon launches and supervises no commands.

### Exec output

`exec` returns `{ kind, exitCode, stdout, stderr, message, truncated }` when the
command ends, and no output is cut at any size (`truncated` is always false).
Each stream is kept inline up to 1 MiB; a result whose streams fit is exactly
the one older releases returned. A larger stream is spooled by the binding to
an unlinked file in the host temp directory, read back into one reused 1 MiB
page, then released; the result then also carries `stdoutBytes` and
`stderrBytes`. Text is decoded as UTF-8 (invalid bytes become U+FFFD), so a
text result is bounded only by V8's longest string (2^29−24 UTF-16 units). For
bounded memory pass Writable sinks as the fourth argument: their streams' raw
bytes are written to them a page at a time, each after the sink's callback for
the last, so the call holds one page however long the output is; their result
fields are null. A sink must not keep a chunk after its write callback (file,
socket and hash streams do not) and is not ended. The Sandbox runtime reads the
command's output only as fast as the spool takes it (an 8 KiB window), and a
spool is bounded by free disk space. Page reads are not traced; the call's
trace line covers the command's run.

### Methods

| Group | Positional methods |
| --- | --- |
| `LayerFS` (2) | `LayerFS.open(name)`, `LayerFS.init(name, folder, timeout?)`: static, each returns a ready client |
| project (2) | `list_branches(pageSize?, pagePos?)`, `list_sandboxes(pageSize?, pagePos?)` |
| fs reads (7) | `read(ref, path, startLine?, endLine?)`, `list(ref, path?, pageSize?, pagePos?)`, `stat(ref, path)`, `grep(ref, pattern, path?, pageSize?, pagePos?)`, `glob(ref, pattern, path?, pageSize?, pagePos?)`, `diff_files(leftRef, leftPath, rightRef, rightPath)`, `blame(ref, path, fromLine?)` |
| fs writes (7) | `write(branch, path, content, message? \| { message?, mode? })`, `edit(branch, path, oldText, newText, message?)`, `apply(branch, changes, message?)`, `mkdir(branch, path, message? \| { message?, mode? })`, `rm(branch, path, message?)`, `cp(branch, from, to, message?)`, `mv(branch, from, to, message?)` |
| branch (6) | `fork(name, fromRef?)`, `list_commits(branch, pageSize?, pagePos?)`, `diff_commits(from, to, pageSize?, pagePos?, { cursor?, hunks? }?)`, `diff_branches(from, to, pageSize?, pagePos?, { cursor?, hunks? }?)`, `merge(a, b, name, message?, units?)`, `publish(branch, commit?)` |
| sandbox (3) | `create({ image }?)` → `{ sandbox, image }`, `list_workspaces(sandbox, pageSize?, pagePos?)`, `close(sandbox)` |
| workspace (4) | `mount(sandbox, branch)` → `{ sandbox, path, image }`, `exec(ws, command, message?, { stdout?, stderr? }?)`, `commit(ws, message?, force?)`, `unmount(ws, discard?, force?)` |

A Workspace's status is not an agent call: it is `backend.daemon.workspace.status(ws)`
in [`@layerfs/backend`](backend.md).

Every grouped method returns a Promise and takes positional arguments; only
the trailing `{ ... }?` arguments shown above are options objects. A ref is `main`, a Branch name, a Commit
id or a Layer id. Filesystem paths are absolute in that view. Main changes only
through publish. There is no `remove` alias, harness export, agent/call attribution,
public inspect or public process-control method.

### Messages

Messages are optional UTF-8, at most 4096 encoded bytes; newline/tab are allowed,
other controls and unpaired JS surrogates are refused before work. Omitted messages
get operation descriptions; empty messages are allowed. Commit messages participate
in `layerfs/commit/v3` identity; time, byte counters and LWW details are persisted
facts outside that identity. An Exec message describes that execution and creates
no history. Unchanged state creates no Commit solely for a message.

### Writes

`apply` takes ordered `{op, ...}` changes (`write`, `edit`, `mkdir`, `chmod`,
`rm`, `cp`, `mv`) and publishes once. A refusal identifies its zero-based `detail.index`.
Each call uses a private indexed Workspace, the ordinary Workspace semantic ports
and the shared captured Content constructor. Failed validation closes only that
private overlay. Uncertain construction/publication keeps original custody.
Recursive work uses bounded directory windows and a backed hardlink index; it
has no resident whole-tree draft or total-path admission cap. Copies preserve
modes, copied-set hardlinks and opaque symlinks; timestamps are new. Moves retain
identity/timestamps. Destinations are exact and must not exist. Same-path/subtree
moves or copies are InvalidInput. Mkdir creates parents and an existing directory
is a no-op. Edit requires exactly one match and streams large files.

A file's or folder's permission bits are an integer `mode` such as `0o755`. A
`write` (in `fs.write`'s options or an `fs.apply` change) or `mkdir` may carry
one: a new file gets `mode`, else 0o644; an existing file keeps its mode unless
`mode` is given; a new folder gets `mode`, else 0o755, while parents it creates
get 0o755 and an existing folder keeps its mode, as `mkdir -p -m` does.
`{ op: "chmod", path, mode }` sets the mode of an existing file or folder.
`fs.stat` reports `mode`. A mode that is not an integer from 0 to 0o7777 is
`InvalidInput`; one LayerFS cannot store (set-user-ID or set-group-ID on any
entry, the sticky bit on a file) is `InvalidInput` with `detail`
`{ reason: "Mode", path, mode }` (octal text), as in `project.init`. A symbolic
link has no mode of its own: `chmod` of one is `NotFile`.

Branch writes, like `workspace.commit`, read the Branch head, merge with it and
publish inside the Store's short publication lane, so concurrent writers never
meet a moved head. A call whose private Workspace was bound before another
publication merges with it once: different paths are `Committed`; a path both
changed is refused as `Conflict` (`detail.conflicts`, `detail.head`) with nothing
committed. Nothing is resent or merged again.

A write's file body (`write` content or bytes, in `fs.write` and `fs.apply`)
travels in the call itself: Store calls run in this process, so no control
record bounds it. The engine writes it into the call's private Workspace a
record at a time, so no file size is capped. Calls are never split or replayed
automatically.

A body is text (a string, written as UTF-8) or bytes: a `Buffer`, any typed
array (`Uint8Array`, `Uint16Array`, ...), a `DataView` or an `ArrayBuffer`. A
typed array or `DataView` writes exactly the bytes it views, in the machine's
byte order, as Node's `fs.writeFile` does. In `fs.write` a non-string body is
sent as bytes; in an `fs.apply` write, give `content` (text) or `bytes`, and
binary `content` is sent as `bytes`. Bytes in a `SharedArrayBuffer` or a
detached `ArrayBuffer` are refused as InvalidInput, and so is anything else,
such as an array of numbers. The bytes are copied once the call starts, after
the client's Store is open, so do not change them until the call settles.
Reads are text only: `fs.read` refuses a file that is not UTF-8 text as
`Binary`, and `fs.stat` reports its size. `node api/agent/test/binary_write.mjs`
checks these forms and refusals on Linux.

### Pages and reads

Pages have `items` and `hasMore`; advance by `pagePos + items.length`.
`diff_commits` and `diff_branches` also answer `nextCursor`, the last path the
page listed: passing it back as `cursor` resumes the walk after that path, so a
page costs its own changes instead of re-walking earlier pages. `hunks: false`
lists the changes without line hunks (`hunks` is `null`), so a page fills by
paths rather than by file contents. Defaults
are 50/0, max pageSize 1000, with safe integers only. Byte bounds can shorten a
page; they never mean EOF when `hasMore` is true. Deep offsets skip through bounded
windows linearly. Names use byte order; recursive paths use deterministic
component-by-component byte order, then line number for grep. Commit history is
newest first along first parents. Every call resolves each live ref once; separate
calls against live heads can shift numeric positions.

Read/blame use 1-based inclusive line positions, no byte offsets or byte-read
method. Responses fit the authenticated control frame: at most 2000 lines and
48 KiB of text/value payload, below the selected 256 KiB ceiling. LF separates
lines; CRLF text is preserved, and a final LF creates no phantom line. Read reports
`startLine`, `endLine`, `truncated`, `lineTruncated` and `nextLine`. Huge lines return
a UTF-8 prefix, with no within-line continuation; the remainder is explicitly
omitted and the next window starts at the following line. NUL/invalid UTF-8 in the
scanned text is Binary. Grep scans chunks and matches across chunk boundaries.
Blame uses bounded LCS and exact common edges. In coarse comparisons, a line
is inherited from the common suffix only when it starts at a corresponding
line boundary in both versions, and from the common prefix only when it ends at
one in both. An oversized changed middle is a removed/added
block and `comparisonCoarse` discloses that Content diff policy.
Diff hunks can be omitted explicitly when their bounded comparison/reply cannot
represent them; equal content is decided by immutable content identity.

### Merge diagnostics and conflicts

Merge diagnostics also use byte windows within the unchanged control frame.
`branch.merge` returns `lwwTruncated`, `conflictsTruncated` and `unitsTruncated` alongside its
diagnostic lists (including in `Conflict.detail`); `workspace.commit` reports
`lwwTruncated` and its conflict detail reports both flags. Each conflict keeps
its original `blocks` count and reports `hunksOmitted` if whole hunks were left
out. These flags limit the reply only: every path still participates in the
merge, and conflict/publication semantics are unchanged. Source text is never
cut to make a diagnostic fit.

For an incomplete Branch conflict summary, preserve the original input Branches
and `detail.sources`/`mergeBase`. Page `branch.diff_branches(a, b)` to inspect all
candidate paths, then read each source and its merge-base version with `fs.read`
or `fs.diff_files`. Resolve on working forks of both inputs. A Workspace conflict
keeps its markers in that Workspace; inspect them through ordinary bounded
`exec` commands and Commit each command. An omitted diagnostic is not a clean
merge, and a failed/unknown operation must be inspected before a new operation.

## The `layerfs` command

`layerfs` runs one `@layerfs/agent` call per command, so an agent whose only
tool is Bash can use LayerFS without writing a Node script for each call.

It is only a wrapper. Each command opens the project with `LayerFS.open`,
makes exactly one SDK call through the SDK's public API, prints the result and
closes the client. It adds no capability, check or retry of its own: the SDK
checks the inputs and makes its one attempt, and the command reports what it
answered. Settings come from the environment exactly as for the SDK, and the
same platform rules apply (Store calls run in this process on Linux, through
the LayerFS home's worker on macOS and Windows, and fail `Unsupported` anywhere else).

### Install and run

The command is the `api/cli/` folder of a LayerFS checkout; it isn't published to
npm. It needs Node 20 or newer and the SDK's native addon (`npm run
build:native` in `api`, or `LAYERFS_NATIVE`).

```bash
npm --prefix "$LAYERFS_REPO/api" install
npm --prefix "$LAYERFS_REPO/api/cli" install
"$LAYERFS_REPO/api/cli/src/layerfs.js" help
```

The package's bin is `layerfs`; link it onto `PATH` (for example `npm link` in
`api/cli/`) to type `layerfs` alone. A short session:

```bash
export LAYERFS_PROJECT=shop                        # or --project shop on each command
layerfs branch fork cart
printf '# Shop\n' | layerfs fs write cart /README.md -m readme
layerfs sandbox create                             # {"sandbox":"1","image":"sha256:…"}
layerfs workspace mount 1 cart                     # {…,"ws":"1:/workspaces/3"}
layerfs workspace exec 1:/workspaces/3 "npm test" --stream
layerfs workspace commit 1:/workspaces/3 -m "step 1: npm test"
layerfs workspace unmount 1:/workspaces/3
layerfs sandbox close 1
```

### How a call becomes a command

- **Names.** A call `<family>.<call>` is `layerfs <family> <call>`, with the
  SDK's names (`branch list_commits`); a dashed name is the same call
  (`branch list-commits`). `LayerFS.init` is `layerfs init`. `LayerFS.open`
  and `close` happen inside every command; the project is `--project <name>`,
  else `LAYERFS_PROJECT`. The command reads no other prefix.
- **Positional arguments** are the SDK's, in the SDK's order. An optional one
  that is left out is not sent, so the SDK applies its own default (`fs list
  main` lists `/`, `branch fork cart` forks from main).
- **Options** are the SDK's remaining arguments as flags: `--page-size <n>`,
  `--start-line <n>`, `--no-hunks` (sends `hunks: false`), `--image <digest>`. A value is `--name value`
  or `--name=value`; `-m <text>` is the Commit message, the only short option.
  A whole number must be digits only, `--units` is JSON, a flag takes no value,
  and an option given twice or not known is refused. `--` ends the options, so
  `layerfs workspace exec 1:/workspaces/3 -- --version` runs `--version`.
- **A Workspace** is written `<sandbox>:<path>`, such as `1:/workspaces/3`.
  `workspace mount` prints it as `ws` beside the SDK's `{ sandbox, path, image }`.
- **Bodies.** `fs write` reads the file's bytes from stdin, or from `--from
  <file>`, and sends them exactly. `fs apply` reads a JSON array of changes the
  same way. `fs edit` takes `--old <text>` or `--old-from <file>`, and `--new`
  or `--new-from`. Input is read before anything opens.
- **Global options**, anywhere before `--`: `--project <name>`, `--text`
  (readable output instead of JSON), `-h`/`--help`, `--version`.
- **exec.** `workspace exec` prints the SDK's result as JSON, with the
  command's `exitCode` inside, and exits 0. With `--stream` it writes the
  command's stdout and stderr through as they arrive, prints nothing else and
  exits with the command's own code.

### Output and exit codes

| What | Where |
| --- | --- |
| The call's result | stdout, one line of JSON; `--text` prints it as readable `key: value` lines. Bytes print as `{ "base64": … }` |
| A refusal or failure | stderr, `{"code", "message", "detail"}` as the SDK's `LayerFSError` gave them; `--text` prints `Code: message` |
| A wrong command line | stderr, `layerfs: <what is wrong>` and the command's usage line |

| Exit | Meaning |
| --- | --- |
| 0 | The call succeeded |
| 2 | The command line is wrong (unknown command, option or argument, a bad value or input, no project); nothing was opened or called |
| 125 | LayerFS refused or failed the call. The SDK's own input checks land here too, because they run inside the call |
| the command's code | `workspace exec --stream` only; 125 still means LayerFS failed |

### Every call

Each command is one file, `api/cli/src/<family>/<call>.js`, beside the SDK's own
`api/agent/src/<family>/<call>.js` (`layerfs init` is
`api/cli/src/project/init.js`, where the SDK keeps Init). Each family's `index.js`
lists its commands in help order, and `api/cli/src/calls.js` assembles them into
the call table. The table below is written from it by
`node api/cli/test/doc-tables.mjs --write`; `api/cli/test/cli.test.mjs` fails when the
table differs, and when a family's files stop matching the SDK's.

<!-- calls:start -->
| SDK call | Command |
| --- | --- |
| `LayerFS.init(name, folder, timeout?)` | `layerfs init <name> <source-dir> [--timeout <n>]` |
| `project.list_branches` | `layerfs project list_branches [--page-size <n>] [--page-pos <n>]` |
| `project.list_sandboxes` | `layerfs project list_sandboxes [--page-size <n>] [--page-pos <n>]` |
| `branch.fork` | `layerfs branch fork <name> [<from-ref>]` |
| `branch.list_commits` | `layerfs branch list_commits <branch> [--page-size <n>] [--page-pos <n>]` |
| `branch.diff_commits` | `layerfs branch diff_commits <from-commit> <to-commit> [--page-size <n>] [--page-pos <n>] [--cursor <text>] [--no-hunks]` |
| `branch.diff_branches` | `layerfs branch diff_branches <from-branch> <to-branch> [--page-size <n>] [--page-pos <n>] [--cursor <text>] [--no-hunks]` |
| `branch.merge` | `layerfs branch merge <a> <b> <name> [-m <text>] [--units <json>]` |
| `branch.publish` | `layerfs branch publish <branch> [<commit>]` |
| `fs.read` | `layerfs fs read <ref> <path> [--start-line <n>] [--end-line <n>]` |
| `fs.list` | `layerfs fs list <ref> [<path>] [--page-size <n>] [--page-pos <n>]` |
| `fs.stat` | `layerfs fs stat <ref> <path>` |
| `fs.grep` | `layerfs fs grep <ref> <pattern> [<path>] [--page-size <n>] [--page-pos <n>]` |
| `fs.glob` | `layerfs fs glob <ref> <pattern> [<path>] [--page-size <n>] [--page-pos <n>]` |
| `fs.diff_files` | `layerfs fs diff_files <left-ref> <left-path> <right-ref> <right-path>` |
| `fs.blame` | `layerfs fs blame <ref> <path> [--from-line <n>]` |
| `fs.write` | `layerfs fs write <branch> <path> [-m <text>] [--mode <octal>] [--from <file>]` |
| `fs.edit` | `layerfs fs edit <branch> <path> [-m <text>] [--old <text>] [--old-from <file>] [--new <text>] [--new-from <file>]` |
| `fs.apply` | `layerfs fs apply <branch> [-m <text>] [--from <file>]` |
| `fs.mkdir` | `layerfs fs mkdir <branch> <path> [-m <text>] [--mode <octal>]` |
| `fs.rm` | `layerfs fs rm <branch> <path> [-m <text>]` |
| `fs.cp` | `layerfs fs cp <branch> <from-path> <to-path> [-m <text>]` |
| `fs.mv` | `layerfs fs mv <branch> <from-path> <to-path> [-m <text>]` |
| `workspace.mount` | `layerfs workspace mount <sandbox> <branch>` |
| `workspace.exec` | `layerfs workspace exec <ws> <command> [-m <text>] [--stream]` |
| `workspace.commit` | `layerfs workspace commit <ws> [-m <text>] [--force]` |
| `workspace.unmount` | `layerfs workspace unmount <ws> [--discard] [--force]` |
| `sandbox.create` | `layerfs sandbox create [--image <text>]` |
| `sandbox.list_workspaces` | `layerfs sandbox list_workspaces <sandbox> [--page-size <n>] [--page-pos <n>]` |
| `sandbox.close` | `layerfs sandbox close <sandbox>` |
<!-- calls:end -->

### What the command leaves out

- **`@layerfs/backend` and `@layerfs/runtime`.** Their calls serve integration
  hosts. A separate admin command for the runtime (`layerfs-admin`) is a
  possible follow-up; it isn't built.
- **Short forms.** The command has only the SDK calls above. The older `lfs`
  command in `@layerfs/runtime` (`status`, `log`, `ls`, `cat`, `grep`, `diff`,
  `fork`, `write`, `sandbox`, `sandboxes`, `run`) is retired (owner
  2026-10-23); `layerfs` is the only command. An integration tells its
  agents how to run it.

### Cost per command

Each command is a new Node process. Before its one call it loads the SDK and
the native addon and opens the project's Store (`LayerFS.open` also makes one
`project.list_branches` call to check the project exists); a Workspace call
also connects to the Sandbox's daemon. An SDK script pays this once for many
calls. This cost has not been measured. For a loop of many calls, a script is
the cheaper route until it is.
