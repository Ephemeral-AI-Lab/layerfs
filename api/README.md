# LayerFS JavaScript packages

> **Status:** Current implementation guide for issues #327 and #428; not a
> release receipt. The #428 package split is a Proposal
> ([core/docs/issues/428](../core/docs/issues/428/README.md)) implemented on
> its branch. Historical results retain their source identities in the
> Phase 2–4 records.

`api/` holds the JavaScript packages and the native addon:

- `agent/sdk/`: `@layerfs/agent`, the agents' SDK (the client, its
  transports and the agent methods).
- `agent/cli/`: the `layerfs` command, one `@layerfs/agent` call per command.
- `backend/`: `@layerfs/backend`, per-project Store and daemon reads of an
  open client, for UIs and tools.
- `runtime/`: `@layerfs/runtime`, installation management on a LayerFS home,
  for integrations.
- `binding/`: the Rust Node addon crate `layerfs-node` over core; it builds
  `binding/layerfs.node`.

`npm install` in `api` links `agent/sdk`, `backend` and `runtime` to each
other; it downloads nothing.

`@layerfs/agent/internal` is not a public API. It exists only for
`@layerfs/backend` and `@layerfs/runtime` at the same exact version as
`@layerfs/agent`, and may change in any release; don't add new uses. It
exports exactly `STATE`, `invokeBackend`, `invokeClient`,
`invokeClientBackend`, `live`, `outcome`, `page`, `pageOf`, `projectArgs`,
`sandboxArg`, `storeArgs`, `workspaceArgs`, `deployment` (call helpers),
`decode`, `encode`, `frame`, `readFrames` (worker frames), `connection`,
`endpointOf` (worker client), `addonPath` (in-process addon) and `useWorker`
(the runtime's worker hand-over).

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

A client is made by `LayerFS.open(name)` (an existing project) or
`LayerFS.init(name, folder, timeout?)` (Init, then the same open); both are
async and return a ready client, and `new LayerFS()` is refused
`InvalidInput`. **There is no options argument:** every setting comes from the
environment, read once when the client is made (each `LAYERFS_` name else its
`DSH_LAYERFS_` twin): `LAYERFS_HOME`, `_STORE_PATH`, `_DOCKER`, `_IMAGE`,
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

Store calls (`fs`, `branch`, `project.list_branches`) run in this process
against the installed Store file at `storePath` (else `LAYERFS_STORE_PATH`),
which the factory opened; no Sandbox is involved. That
file is `global/store.sqlite` in the project's volume, so the process runs where
that volume is mounted (`/layerfs-store` in a Sandbox). This is Linux only, with
the Store on a local filesystem the daemons share (ext4, xfs, btrfs); anywhere
else opening fails `Unsupported`, and without a `storePath` it fails
`InvalidInput`. On macOS and Windows the dsh adapter alone may send the calls
to its LayerFS home's worker instead (`LAYERFS_WORKER_ENDPOINT` with
`LAYERFS_WORKER_ADAPTER=dsh`, the AGENTS.md exceptions); any other adapter is
refused `Unsupported`. A client keeps the binding it was made with: the
in-process addon, or the worker if one was set when it was made, so a client
made before `@layerfs/runtime` hands over its worker stays in-process and
every later one uses the worker. Mount does not scan or materialize the complete tree.
A project is its home entry, `LAYERFS_HOME/<name>/`; the binding keeps no other
record of which names it initialized. Init refuses a name whose entry exists
(`Exists`). Once that entry is deleted, after the project's Sandboxes are
removed, any process, including the one that made it, can initialize the name again.
The SDK has no remove method; a host that removes a project deletes the entry
and the project's volume itself. `node api/agent/sdk/test/reinit.mjs` checks this with
the real binding: Init, `Exists`, delete the entry, Init again in one process.

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

| Environment | Meaning |
| --- | --- |
| `LAYERFS_HOME` | Absolute private directory for discovery records (projects, Sandboxes) and Init's temporary sealed Store, deleted once its install is recorded. Created as 0700; existing directories must be owned by the current effective user with no group/other access. Records are 0600, not symlinks. |
| `LAYERFS_DOCKER` | Local Engine Unix socket; `/var/run/docker.sock` is the default. |
| `LAYERFS_IMAGE` | Exact immutable runtime `sha256:...` digest a new Sandbox runs. A running Sandbox keeps the image it started with. |
| `LAYERFS_VOLUME` | Existing named local VM volume shared by the project's daemons. |
| `LAYERFS_DAEMON` | Host path to the built Linux daemon executable. |
| `LAYERFS_NATIVE` | Optional path of the real addon; defaults to `api/binding/layerfs.node`. |
| `LAYERFS_WORKER_ENDPOINT`, `LAYERFS_WORKER_ADAPTER` | macOS and Windows, dsh only: the endpoint file of the LayerFS home's worker, which then makes every binding call; the adapter must be `dsh`. |
| `LAYERFS_STORE_PATH` | The installed Store file this client's Store calls open in this process. It is `global/store.sqlite` in the project's volume, `/layerfs-store/global/store.sqlite` where that volume is mounted at `/layerfs-store`. Linux only, on a local filesystem the daemons share; elsewhere Store calls fail `Unsupported`. Unset, they fail `InvalidInput`. |
| `LAYERFS_CONTAINER_PREFIX` | Optional. Names every container this client creates `<prefix><project>-<Sandbox>` (default prefix `layerfs-`); another LayerFS home on the same Docker engine sets its own, so names never collide. Docker's name rule applies (an ASCII letter or digit, then letters, digits, `_`, `.`, `-`, at most 128). A name another container holds refuses the create before any effect. |
| `LAYERFS_DAEMON_HOST` | Optional. The host this process dials for a daemon's port. Each daemon publishes its port on the Docker host's `127.0.0.1` only, and that stays so. A client inside a container does not share that loopback, so it names the host that reaches it, `host.docker.internal` on Docker Desktop. One resolution, its first address, one connect; nothing is tried again. Unset, the client dials `127.0.0.1`. |
| `LAYERFS_TRACE` | Optional per-call trace file (see [Call trace](#call-trace)); unset means no trace. |
| `LAYERFS_TRACE_STEP` | Optional step label put on each trace line as `step`; read at every call. |
| `LAYERFS_TRACE_START` | `1` adds a start line before each traced call; read at every call. |
| `DSH_LAYERFS_HOME`, `DSH_LAYERFS_DOCKER`, `DSH_LAYERFS_IMAGE`, `DSH_LAYERFS_VOLUME`, `DSH_LAYERFS_DAEMON`, `DSH_LAYERFS_STORE_PATH`, `DSH_LAYERFS_TRACE`, `DSH_LAYERFS_TRACE_STEP`, `DSH_LAYERFS_TRACE_START` | Each is used when its `LAYERFS_` name is unset or empty. A host that may only set `DSH_*` names in a command's environment (deepseek-harness) configures and traces the SDK through these. |

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

| Group | Positional methods |
| --- | --- |
| project (2) | `init(sourcePath)`, `list_branches(pageSize?, pagePos?)` |
| fs reads (7) | `read(ref, path, startLine?, endLine?)`, `list(ref, path?, pageSize?, pagePos?)`, `stat(ref, path)`, `grep(ref, pattern, path?, pageSize?, pagePos?)`, `glob(ref, pattern, path?, pageSize?, pagePos?)`, `diff_files(leftRef, leftPath, rightRef, rightPath)`, `blame(ref, path, fromLine?)` |
| fs writes (7) | `write(branch, path, content, message?)`, `edit(branch, path, oldText, newText, message?)`, `apply(branch, changes, message?)`, `mkdir(branch, path, message?)`, `rm(branch, path, message?)`, `cp(branch, from, to, message?)`, `mv(branch, from, to, message?)` |
| branch (6) | `fork(name, fromRef?)`, `list_commits(branch, pageSize?, pagePos?)`, `diff_commits(from, to, pageSize?, pagePos?, { cursor?, hunks? }?)`, `diff_branches(from, to, pageSize?, pagePos?, { cursor?, hunks? }?)`, `merge(a, b, name, message?, units?)`, `publish(branch, commit?)` |
| workspace (5) | `mount(sandbox, branch)` → `{ sandbox, path, image }`, `exec(ws, command, message?, { stdout?, stderr? }?)`, `status(ws)`, `commit(ws, message?, force?)`, `unmount(ws, discard?)` |

Every grouped method returns a Promise and takes positional arguments only; no
method takes an options object. A ref is `main`, a Branch name, a Commit
id or a Layer id. Filesystem paths are absolute in that view. Main changes only
through publish. There is no `remove` alias, harness export, agent/call attribution,
public inspect or public process-control method.

Messages are optional UTF-8, at most 4096 encoded bytes; newline/tab are allowed,
other controls and unpaired JS surrogates are refused before work. Omitted messages
get operation descriptions; empty messages are allowed. Commit messages participate
in `layerfs/commit/v3` identity; time, byte counters and LWW details are persisted
facts outside that identity. An Exec message describes that execution and creates
no history. Unchanged state creates no Commit solely for a message.

`apply` takes ordered `{op, ...}` changes (`write`, `edit`, `mkdir`, `rm`, `cp`,
`mv`) and publishes once. A refusal identifies its zero-based `detail.index`.
Each call uses a private indexed Workspace, the ordinary Workspace semantic ports
and the shared captured Content constructor. Failed validation closes only that
private overlay. Uncertain construction/publication keeps original custody.
Recursive work uses bounded directory windows and a backed hardlink index; it
has no resident whole-tree draft or total-path admission cap. Copies preserve
modes, copied-set hardlinks and opaque symlinks; timestamps are new. Moves retain
identity/timestamps. Destinations are exact and must not exist. Same-path/subtree
moves or copies are InvalidInput. Mkdir creates parents and an existing directory
is a no-op. Edit requires exactly one match and streams large files.

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
`Binary`, and `fs.stat` reports its size. `node api/agent/sdk/test/binary_write.mjs`
checks these forms and refusals on Linux.

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
Blame uses bounded LCS and exact common edges; an oversized changed middle is a
removed/added block and `comparisonCoarse` discloses that Content diff policy.
Diff hunks can be omitted explicitly when their bounded comparison/reply cannot
represent them; equal content is decided by immutable content identity.

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

## @layerfs/backend

```js
import { LayerFSBackend } from "@layerfs/backend";
const backend = LayerFSBackend.of(layerfs); // opens nothing; refused once layerfs is closed
// backend.store: the project's Store, no Sandbox needed
await backend.store.project.get_history(since);           // Branches, newest Commits, Layers, version
await backend.store.project.get_storage();
await backend.store.project.get_comparable(id);           // { commit, root, exact } or NotComparable
await backend.store.project.list_overwritten(layer, 50, 0);
await backend.store.branch.list_commit_storage("feature", 20, 0);
await backend.store.fs.get_storage("feature", "/README.md");
await backend.store.fs.list_revisions("feature", "/README.md", 20, 0);
// backend.daemon: the Sandbox must be running, else NoSuchSandbox
await backend.daemon.workspace.status(ws);
await backend.daemon.workspace.list_changes(ws, 20, 0);
await backend.daemon.workspace.get_storage(ws);
const process = await backend.daemon.process.start(ws, "npm run dev"); // "proc_1_…"
await backend.daemon.process.list(50, 0);                 // every Sandbox's, each with its `sandbox`
await backend.daemon.process.cancel(process.process);
await backend.daemon.sandbox.get_log("1", { lines: 200 }); // { sandbox, container, text, bytes }
```

Backend names (`backend.*`) are refused by the agent's public call and by
public native, Rust and daemon dispatch; they use the binding's backend table.
Background processes run through the Sandbox's ordinary command launcher; the
daemon supervises no command. The names before #428 (`operator.*`) map to
these through `RENAMED_METHODS` in `@layerfs/agent`, for old traces.

`get_comparable(id)` takes a saved Commit or Layer id and answers the Commit
with the same files, so a Layer can be compared through `branch.diff_commits`.
A Layer whose listed Commit has its root answers `exact: true`; one published
from a Commit `get_history` does not list answers that Commit with
`exact: false`, and the caller checks the diff answer's root equals `root`.
Init's Layer, a publish that merged and an id `get_history` does not list are
refused `NotComparable`. What it learns is kept per client.

`get_log` reads the Sandbox's daemon log once through the Docker Engine where
the binding runs (the worker on macOS and Windows): the last `lines` lines
(1 to 2000, default 200), at most 64 KiB. A stopped Sandbox's log is readable
until its records are reclaimed. There is no `since`.

`runtime.project.get_init` (native; `@layerfs/runtime`) reads the project's Init while `project.init` runs in the
same process, without a Sandbox: `{ step, entries, files, bytes }`, or null when
no Init runs. `step` is `scan` (reading the source), `files` (constructing each
regular file), `tree` (building the namespace), `publish` (genesis Layer) or
`seal` (first Branch and sealing). `entries` counts source entries scanned,
including the root; `files` and `bytes` count regular-file identities
constructed and their bytes, a hardlinked file once. The counts say how far
Init has come, not its totals; a caller that wants a fraction counts the source
itself. Init then installs the Store into the volume in the same process.

Store `logical` sums regular-file sizes across immutable Commit and Layer history
records, counting each hardlink path. `canonical` counts distinct registered object
bytes, `stored` complete pack bodies, and `metadata` metadata-domain pack bodies
plus occupied SQLite bytes outside pack bodies. `file` is committed logical DB
pages (including free pages and pages currently in WAL), not main-file length or
extra WAL-file allocation; `free` is reusable DB pages in bytes. Ratios are
logical/canonical and canonical/stored, null for a zero denominator. Per-Commit
new byte counts describe that producer's acknowledged Save, not exclusive SQL
or per-path physical allocation. Per-file physical attribution remains null;
revision diagnostics explicitly disclose their 1000-revision count bound.
Overlay payload counts stored cell bytes including validity masks; metadata row
counts are reported without inventing exclusive byte attribution.

## @layerfs/runtime

```js
import { LayerFSRuntime } from "@layerfs/runtime";
const runtime = await LayerFSRuntime.open({ home, image, daemon, bundle, docker, containerPrefix });
try {
  await runtime.status();                  // { platform, missing, docker, worker, oldStore, disk, sweep }
  const job = await runtime.project.start_init("shop", "/src/shop", { timeout }); // timeout optional: LayerFS.init's 300 s default
  await runtime.project.get_init(job.id);  // { step, scanned, totals, imported, project, error, ended }
  await runtime.project.list();            // [{ name, volume, image, source, created, initialized }]
  await runtime.project.get_state("shop", since);
  await runtime.sandbox.reclaim("shop");   // { reclaimed: [ids] }
  await runtime.sandbox.list_leftovers();
  await runtime.sandbox.clean_leftovers(["layerfs-old-1"]);
  await runtime.sandbox.list();            // [{ name, project, sandbox, state, running, startedAt, finishedAt, image, ports, scratch, owned }]
  await runtime.sandbox.sweep();           // { reclaimed: { project: [ids] }, failed: [{ project, code, message }] }
  await runtime.volume.list();             // [{ name, size, containers, owner, project, live, removable }]
  await runtime.volume.remove(["layerfs-shop-store-k2"]); // { results: [{ name, ok, message? }] }
  for await (const event of runtime.events.subscribe({ project: "shop" })) { /* a trace line + actor */ }
  await runtime.project.remove("shop", { confirm: "shop", stopSandboxes: false });
} finally {
  await runtime.close();
}
```

The runtime opens on a LayerFS home, not a project, so it works before the
first project exists, after one is removed and while the worker is down.
Agents don't import it. Settings (`settings.js`): `home` (required), `image`,
`daemon` (default the bundle's), `bundle`, `docker`, `volumes` (Linux),
`traceDir` (default `<home>/traces`), `containerPrefix`, `stateDir` (default
`<home>/.runtime`, the runtime's project records), `adapter`, `sweep`
(default true) and on Windows `wsl: { distribution, user, node, home }`. A
missing setting does not refuse `open`: `status().missing` lists it.

Disk the home holds in Docker (#423):

- `sandbox.list` lists this home's Sandbox containers, running or not, with
  `scratch`, the bytes of the container's writable layer (what commands wrote
  outside every Workspace), its ports and whether a project of this home owns
  it. One `docker ps` and one `docker inspect --size`; it changes nothing.
- `sandbox.sweep` runs `sandbox.reclaim` for every project of the home that
  has an exited or dead Sandbox container. The project's records and lock
  decide what goes, so a container of another home, or one starting now, is
  never touched; containers no project records stay for `list_leftovers`.
  `open` runs it once in the background unless `sweep: false`;
  `status().sweep` is `null`, `{ state: "running" }` or
  `{ state: "done" | "failed", at, … }`, and `close` waits for it.
- Init labels a project's volume `layerfs.role=store`, `layerfs.home=<home>`
  and `layerfs.project=<name>` (Docker sets labels only at create).
  `volume.list` lists every volume on the engine with Docker's size, the
  containers that use it and its `owner`: `"this"` (labelled with this home
  or named by a project record), `"other"` (another home's label) or
  `"unknown"` (no label and no record: made before labels or by another
  tool). `live` is true while this home has the owning project; an uncertain
  Init, which keeps a home entry and no record, counts as live. Docker's own
  "reclaimable" figure counts every idle project's Store and is not used.
- `volume.remove(names)` removes only volumes that `volume.list` marks
  `removable`: this home's, no longer a project's, and used by no container.
  Anything else is refused by name; one `docker volume rm` each, no retry.

Each platform folder implements one interface (`endpoint`, `status`,
`start`, `restart`, `init`, `path` and the Docker, home and disk helpers):

- **Linux** runs Store calls in this process. There is no worker:
  `worker.start` and `worker.restart` are refused `InvalidInput`. Init
  installs the Store under Docker's volumes directory, so the process must be
  able to write it (`init` checks first).
- **macOS** runs the worker as one container per home, `<prefix>worker`
  (label `layerfs.role=worker`), with the bundle's `worker/server.js`; Init
  runs in a container of its own with the source folder mounted read-only.
  The older dsh Store container, `<prefix>store`, is listed by
  `status().oldStore` while it exists. While it runs, `worker.start` and
  `worker.restart` are refused `InvalidInput` unless given
  `{ confirm: "restart" }`; then they stop it and remove it (one attempt
  each; a failed step stops there) and start `<prefix>worker`, which cuts off
  any live run on the old container. The worker's secret reaches `docker
  run` through its environment, never its command line.
- **Windows** runs the worker as a Node process in a WSL2 distribution
  (`server.js --stdin`); Init runs in the worker, which maps the folder with
  `wslpath`. This process owns the worker through its stdin: `close()` drains
  and ends it.

The worker route is open only for the dsh adapter (AGENTS.md exceptions):
with `adapter: "dsh"` the runtime hands its endpoint to `@layerfs/agent`
(`useWorker`, without changing the environment), so every client made after
`open` in its process uses the worker; a client made before keeps its
in-process binding. One worker per process: a second runtime of another home
is refused. The dsh adapter gives agents' bash calls `LAYERFS_WORKER_ENDPOINT`
and `LAYERFS_WORKER_ADAPTER=dsh`. Without it, `worker.start` is refused
`Unsupported`. The endpoint is `<home>/worker/endpoint.json`, `{ home, port,
secret, build, daemonHost?, init? }`, readable by its user only. The worker
checks every file of the bundle's `manifest.json` before it listens.

`start_init(name, folder, { image, timeout })` refuses a bad name, folder,
image or timeout and a taken name before anything changes; then it counts the
source, makes the volume and runs one Init. `timeout` (milliseconds, a positive
whole number) is passed unchanged to `LayerFS.init(name, folder, timeout)` and
bounds the Init itself, not the count or volume steps; the runtime has no
default of its own, so an omitted one gets `LayerFS.init`'s 300 s. A Deadline
is a failed Init like any other. A failed Init removes the volume and the home entry it made; an
uncertain one (`Unknown`, or any failure on Windows) keeps everything and is
recorded in `<stateDir>/init-failures.jsonl`. `remove` needs `confirm` equal
to the project's name and refuses `Busy` while a running container uses the
volume, unless `stopSandboxes`. `events.subscribe` reads the trace folder's
`*.jsonl` files every `pollMs`: lines from now on, or from `since`, each with
`actor`, its trace file's name.

The runtime calls the `docker` CLI, as the dsh Host did. It has no command
line of its own: the `lfs` command it carried is retired (owner 2026-10-23).
Agents use the `layerfs` command in the repository's `api/agent/cli/` folder, one
`@layerfs/agent` call per command with JSON output and no short forms (for
example `layerfs sandbox create`, `layerfs workspace mount <sandbox>
<branch>`); see [agent/cli/sdk-to-cli.md](agent/cli/sdk-to-cli.md).

## Call trace

`LAYERFS_TRACE=<file>` (else `DSH_LAYERFS_TRACE`), as set when the client is
made, appends one JSON line to `file` for every agent and backend call that
reaches the native binding.
Unset, nothing is written and nothing changes. A failed trace write is dropped;
it never changes a value, a refusal or the call count. Argument checks the
JavaScript methods make before the call (for example `page` bounds) are refused
without a trace line.

```json
{"layerfs_trace":1,"id":"9f3c21aa-12","step":"add product page","method":"fs.write","args":{"branch":"feature","path":"/README.md","contentBytes":6,"message":"Update introduction"},"start":"2026-10-15T09:12:01.120Z","scope":"…","phase":"end","ms":41.733,"ok":true,"commit":"…","sizes":{"logical":120,"newCanonical":40,"newStored":30,"newMetadata":8},"daemon":[{"timing":[{"name":"files.bind","ms":1.2,"children":[…]},{"name":"commit","ms":30.4,"children":[…]}],"storage":{…},"construction":{…}}]}
```

| Field | Meaning |
| --- | --- |
| `layerfs_trace` | Format version, `1`. |
| `id` | `<run>-<n>`: a random per-process run prefix and the call's sequence number. |
| `step` | The step label, cut to 200 characters; absent when none is set. |
| `method` | The binding method, for example `fs.read`, `workspace.commit`, `backend.store.project.get_storage`. |
| `project` | The project the call names, when it names one. |
| `args` | Short summary: text arguments (paths, refs, names, Sandbox id, command, message) cut to 200 characters; numbers and flags; `content`, `bytes`, `oldText` and `newText` as `<name>Bytes` byte counts; `apply` changes as `changes` (count) and `changeBytes`. Never file contents; `project`, `home` and `deploy` are left out. |
| `start`, `ms` | Wall-clock start (ISO 8601) and elapsed wall milliseconds measured in the calling process, including the native queue and the daemon round trip. |
| `queue` | The part of `ms` the call spent waiting before its own work, in milliseconds: `thread`, the wait for a native worker thread (the call's time outside its run in the binding), `connection`, the wait for a free control connection of its Sandbox, and their sum `ms`. Present on every line of a call that reached the binding. |
| `ok` | `true` for a value; `false` with `code` and `message` for a refusal. |
| `phase` | `end` on the line written when the call returns; `start` on the optional line written before it (see below). |
| `branch`, `base`, `head`, `sandbox`, `path`, `baseLayer`, `process`, `exitCode` | Result ids, on a successful end line only. `branch.fork`: the new `branch`, its `base` Layer and its `head` (`null` when forked from main). `branch.merge`: the new `branch` and its `head`. `workspace.mount`: the `sandbox`, the Workspace `path`, its `branch`, its `base` Commit (`null` on a Branch with no Commit) and `baseLayer`. `backend.daemon.process.start`: the `process` id. A foreground `workspace.exec`: its `exitCode`; its output is never traced. |
| `kind`, `commit`, `layer`, `sizes` | The value's `kind`. Every publishing call that succeeds (`fs` write, edit, apply, mkdir, rm, cp, mv, `workspace.commit`, `branch.merge`, `branch.publish`) names a `commit`: its new Commit, or the head it was `UpToDate` with; `branch.publish` names the published source Commit and the new `layer`. `sizes` (`logical`, `newCanonical`, `newStored`, `newMetadata`) appear only when the reply carries the Commit record (`fs` writes and `workspace.commit` that commit); join `commit` to `backend.store.branch.list_commit_storage` for the rest. No call is added to fetch them. |
| `scope` | 64 hex characters: the fresh daemon observation scope this call's control exchanges carried. |
| `daemon` | One entry per traced control exchange the call made (a mount makes at least three: Branch lookup, bind, attach). Absent when the call reached no daemon (for example `project.init`). |

Each `daemon` entry holds:

| Field | Meaning |
| --- | --- |
| `timing` | Phase trees the daemon recorded for that exchange: `{ name, ms, children }`, with `outcome` (`error`, `unknown`) and `incomplete` only when they apply. Roots include `commit` (phases `capture`, `producer`, `begin`, `construct` › `captured.namespace`, `head`, `merge`, `logical.known`, `finish`, `logical.count`, `validate`, `publish`, `install`), `sandbox.mount.bind` › `store.bind`, `sandbox.mount.attach` › `kernel.attach`, `files.bind` (the private Workspace of an `fs` write), `sdk.read`, `sdk.lines`, `branch.merge` and `blame.compare`. |
| `omitted` | Timing nodes left out beyond the 256-node reply bound. |
| `sql` | The exchange's Store SQL work: `writer` (the Store's writer session) and `readers` (every reader session, live and closed, summed), each the session totals after the exchange minus before it, with the 27 `SqlWork` counters in snake_case (`statements`, `vm_steps`, `fullscan_steps`, `sorts`, `autoindex_rows`, `reprepares`, `returned_rows`, … `_ns` walls). Work of concurrent calls on the same sessions is included. A group whose counters were unavailable is left out. `call` (#367) has the same counters for the work the calling thread ran on any Store session: an in-process Store call's own work, without concurrent calls'. |
| `delta` | On exchanges that finished a Save: the daemon's delta selection work, after minus before. `payload`: whole-file and chunk counts (`prepared_full`, `trials`, `prefix_selected`, `full_losses`, `no_candidate`, `absent_candidates`, `ineligible_candidates`, `depth_capped`, `work_exceeded`) and depth histograms `whole_file_depths`, `chunk_depths` (slot d = objects stored at chain depth d; 0 is FULL). `metadata`: pooled inode leaves (`leaves`, `delta_leaves`, `full_leaves`, `trials`, `depth_capped`, `work_exceeded`, `depths`). |
| `fuse` | On `workspace.status` and `workspace.unmount` exchanges of an attached Workspace (Linux): its kernel opcode counts since attach (39 opcodes such as `lookup`, `getattr`, `readdirplus`, `read`, `write`), frame dispositions (`served`, `inline`, `refused`, `terminal`), `forget_units`, `store_units`, `largest_read` and `largest_write`. Per-exec counts are the difference between two such readings. |
| `storage` | When the exchange committed: the Save's storage diagnostics (42 counters such as `pack_write_bytes`, `locator_hits`, `pack_misses`), daemon names in snake_case. |
| `construction` | When the exchange committed: the captured namespace construction work (31 counters such as `files_constructed`, `inode_rows`). |
| `diagnosticError` | `{ kind, message }` when the daemon could not write its numeric records, or the telemetry did not fit the reply (`Oversized`). The call's result and connection are unaffected. |

Tracing makes each control exchange an observed one: the daemon also writes
fixed-schema numeric records (SQL work, owner lock wait/hold, Store reader and
cache work, FUSE kernel counters on Linux, Workspace resources) to its stderr.
Decode them and join them to the trace by `scope`:

```sh
docker logs <container> 2>&1 | python3 tools/decode_observations.py --trace trace.jsonl
```

Each output line is one daemon call: `daemon`, `scope`, `call`, `operation`,
`complete`, the joined trace `id` as `trace`, and named `sections` (unavailable
sections are `null`, never zero). Values are cumulative daemon counters at that
call, not per-call deltas. The container is `cat $LAYERFS_HOME/<project>/sandboxes/<n>/container`.

A step label groups lines by the caller's own unit of work, such as an agent's
step. `LAYERFS_TRACE_STEP` is read at each call, so a long-lived process can
change it between calls. Unset or empty, lines
have no `step`. A label alone traces nothing.

With `LAYERFS_TRACE_START=1` (or `DSH_LAYERFS_TRACE_START=1`), each traced
call first writes a start line: the same `layerfs_trace`, `id`, `step`, `method`,
`args`, `start` and `scope` as its end line, with `phase: "start"` and no
outcome fields. A viewer can show a call while it runs; a start line with no
end line is a call that has not returned (or a process that ended first).
Any other value leaves start lines off.

Concurrent calls append whole lines; order follows completion, so join by `id`.
The trace is diagnostic timing, not a benchmark measurement. Off means no scope,
no observed exchange, no daemon timing and byte-identical replies.

Checks: `node api/agent/sdk/test/trace.mjs <case>` runs one case (`on`, `off`,
`failure`, `dsh-env`, `start`, `ids`, `no-content`) over a stand-in binding;
`node api/agent/sdk/test/verify/trace_real.mjs <file>` traces a fork, mount, exec,
process, commit and merge on a fresh Disposable Store with only the `DSH_`
names set.

## Builds and compatibility

Run `npm --prefix api run build:native` on the macOS/Linux host that loads the
addon. It uses Cargo +1.85.1, `--offline --locked --release`, and the repository
ARM64 inputs. `-- --debug` selects dev. The artifact is atomically renamed into
place, avoiding stale macOS executable-page signature state. `CARGO_TARGET_DIR`
must stay inside this worktree; `LAYERFS_NATIVE_OUTPUT` can select a separate
platform artifact. Keep Linux addons separate from the host addon.

New Store schema versions are 16–18 (pack layouts) and 22–24 (with acquisition),
with history identity format 3. Old Disposable Stores are rejected without
migration. Select a fresh project/Store for the new format.

Disposable/WAL/synchronous=OFF only. Durable execution is NOT_RUN — disabled by
owner until explicit reauthorization. One attempted operation; no retry or replay.
Zombie reaping and the existing replaced-inode FUSE policy remain owner-deferred.

## Tests

The stand-in tests need no Docker, Store or daemon: they load the real
JavaScript over one stand-in native binding,
`agent/sdk/test/fixture/fake-native.cjs`, which the `layerfs` CLI tests
use too.

- `node api/agent/sdk/test/calls.mjs` checks every agent call for
  correctness against the shared case table
  `agent/sdk/test/fixture/cases.mjs`. For each call it checks the binding
  call made, the arguments sent, and the result returned or the
  `LayerFSError` thrown. There is a case for every call file under
  `agent/sdk/src/`.
- `node --test api/agent/cli/test/cli.test.mjs` runs the same cases through the SDK and
  through the `layerfs` command and requires them to agree (see
  [agent/cli/sdk-to-cli.md](agent/cli/sdk-to-cli.md)).
- `packaging.mjs`, `trace.mjs`, `exec_output.mjs` and the backend and runtime
  tests each check one case per command (`node <file> <case>`).

Against a real Store, Docker and daemon, `test/verify/scenario.mjs` makes
every agent call once, in two parts: `store` (Projects, Branches, files and
history) and `workspace` (a Sandbox, one Workspace, Exec, Commit and close).
Run each part under the E2E launcher (`tools/worktree.py run`, see
`docs/general/parallel-worktrees.md`):

- `node api/agent/sdk/test/verify/calls_real.mjs inner <part>` runs the
  part through the SDK and checks each answer.
- `node api/agent/cli/test/verify/cli_real.mjs inner <part>` runs it through the SDK and
  then as `layerfs` commands, and requires the same answers. Values that differ
  between any two runs are masked before the comparison.
