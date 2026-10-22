# @layerfs/sdk

> **Status:** Current implementation guide for issue #327; not a release receipt.
> The public contract is the [29-method #328 standard](https://github.com/Ephemeral-AI-Lab/layerfs/blob/5321d6de320dcbd41402db9cb41aa36bceabdf4c/core/docs/issues/327/SDK-STANDARD.md).
> Historical results retain their source identities in the Phase 2–4 records.

```js
import { LayerFS } from "@layerfs/sdk";
// Lazy: no filesystem/addon/network I/O. Store calls open this file in this
// process; here the project's volume is mounted at /layerfs-store.
const layerfs = new LayerFS("website", { storePath: "/layerfs-store/global/store.sqlite" });
await layerfs.project.init("./website");
await layerfs.branch.fork("feature");
await layerfs.fs.write("feature", "/README.md", "Hello\n", "Update introduction");
const page = await layerfs.branch.list_commits("feature", 20, 0);
const text = await layerfs.fs.read("feature", "/README.md", 1, 20);
const ws = await layerfs.workspace.mount("feature"); // { sandbox: "1", path: "/workspaces/1" }
await layerfs.workspace.exec(ws, "npm test", "Run tests");
await layerfs.workspace.commit(ws, "Save changes");
await layerfs.workspace.unmount(ws);
```

Construction captures environment strings. `new LayerFS(project, { storePolicy })`
selects the Store policy `project.init` records instead of the default:
`{ smallFileThreshold, wholeFileDepth, chunkDepth, metadataDepth }`, each
defaulting to the default policy's value (131072 bytes, depths 16, 8 and 8). An
invalid policy is refused as `InvalidInput` before the project entry is made.
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

Store calls (`fs`, `branch`, `project.list_branches`) run in this process: the
first one opens the installed Store file at `storePath` (else
`LAYERFS_STORE_PATH`) and `close()` releases it; no Sandbox is involved. That
file is `global/store.sqlite` in the project's volume, so the process runs where
that volume is mounted (`/layerfs-store` in a Sandbox). This is Linux only, with
the Store on a local filesystem the daemons share (ext4, xfs, btrfs); anywhere
else Store calls fail `Unsupported`, and without a `storePath` they fail
`InvalidInput`. Mount does not scan or materialize the complete tree.
A project is its home entry, `LAYERFS_HOME/<name>/`; the binding keeps no other
record of which names it initialized. Init refuses a name whose entry exists
(`Exists`). Once that entry is deleted, after the project's Sandboxes are
removed, any process, including the one that made it, can initialize the name again.
The SDK has no remove method; a host that removes a project deletes the entry
and the project's volume itself. `node sdk/js/test/reinit.mjs` checks this with
the real binding: Init, `Exists`, delete the entry, Init again in one process.

Parallel agents each mount their own Workspaces; LayerFS places them in the
project's Sandboxes (see below), so no agent names, creates or closes one.
Mounts of one project take its placement lock one at a time, each for its
own placement and mount; Exec, status, Commit and unmount do not take it.
Concurrent calls to one Sandbox from one process use separate control
connections, up to six (a Sandbox admits eight), so a read does not wait behind
another call's Commit; each call's exchanges stay on one connection and are sent
once. The binding sets `UV_THREADPOOL_SIZE` to 64 when the process has not set
it, because every call, a long `exec` included, holds a Node worker thread.
Keep each agent's operations on a Workspace sequential.

| Environment | Meaning |
| --- | --- |
| `LAYERFS_HOME` | Absolute private directory for sealed provisioning and discovery. Created as 0700; existing directories must be owned by the current effective user with no group/other access. Records are 0600, not symlinks. |
| `LAYERFS_DOCKER` | Local Engine Unix socket; `/var/run/docker.sock` is the default. |
| `LAYERFS_IMAGE` | Exact immutable runtime `sha256:...` digest a new Sandbox runs. A running Sandbox keeps the image it started with. |
| `LAYERFS_VOLUME` | Existing named local VM volume shared by the project's daemons. |
| `LAYERFS_DAEMON` | Host path to the built Linux daemon executable. |
| `LAYERFS_NATIVE` | Optional addon path; defaults to `native/layerfs.node`. |
| `LAYERFS_STORE_PATH` | The installed Store file this client's Store calls open in this process; the client option `storePath` wins. It is `global/store.sqlite` in the project's volume, `/layerfs-store/global/store.sqlite` where that volume is mounted at `/layerfs-store`. Linux only, on a local filesystem the daemons share; elsewhere Store calls fail `Unsupported`. Unset, they fail `InvalidInput`. |
| `LAYERFS_CONTAINER_PREFIX` | Optional. Names every container this client creates `<prefix><project>-<Sandbox>` (default prefix `layerfs-`); another LayerFS home on the same Docker engine sets its own, so names never collide. Docker's name rule applies (an ASCII letter or digit, then letters, digits, `_`, `.`, `-`, at most 128). A name another container holds refuses the create before any effect. |
| `LAYERFS_DAEMON_HOST` | Optional; the client option `daemonHost` wins. The host this process dials for a daemon's port. Each daemon publishes its port on the Docker host's `127.0.0.1` only, and that stays so. A client inside a container does not share that loopback, so it names the host that reaches it, `host.docker.internal` on Docker Desktop. One resolution, its first address, one connect; nothing is tried again. Unset, the client dials `127.0.0.1`. |
| `LAYERFS_SANDBOX` | Optional; the client option `sandbox` wins. Unset or `docker`: a Sandbox is a Docker container, exactly as described here. `local`: a Sandbox is a daemon process on this machine, with no container and no Docker setting read (see [Local Sandboxes](#local-sandboxes-no-docker)). `native`: the same on macOS without root: Workspaces are served over loopback NFS and mounted by this machine's own NFS client, and commands run as this user (see [Native Sandboxes](#native-sandboxes-macos)). Any other value is refused as `InvalidInput` before anything is made. |
| `LAYERFS_LOCAL_DIR` | With `LAYERFS_SANDBOX=local`, required: an existing absolute directory, owned by the running user, not group/other writable and traversable by others (0755). Each local Sandbox gets its own directory under it. |
| `LAYERFS_TRACE` | Optional per-call trace file (see [Call trace](#call-trace)); unset means no trace. |
| `LAYERFS_TRACE_STEP` | Optional step label put on each trace line as `step`; read at every call. |
| `LAYERFS_TRACE_START` | `1` adds a start line before each traced call; read at every call. |
| `DSH_LAYERFS_HOME`, `DSH_LAYERFS_DOCKER`, `DSH_LAYERFS_IMAGE`, `DSH_LAYERFS_VOLUME`, `DSH_LAYERFS_DAEMON`, `DSH_LAYERFS_STORE_PATH`, `DSH_LAYERFS_SANDBOX`, `DSH_LAYERFS_LOCAL_DIR`, `DSH_LAYERFS_TRACE`, `DSH_LAYERFS_TRACE_STEP`, `DSH_LAYERFS_TRACE_START` | Each is used when its `LAYERFS_` name is unset or empty. A host that may only set `DSH_*` names in a command's environment (deepseek-harness) configures and traces the SDK through these. |

A Sandbox is a container running one daemon on the project's Store. The
project has numbered Sandboxes, `1`, `2`, …, each on one image:
`workspace.mount(branch, { image }?)` takes the lowest-numbered running
Sandbox of that image (default `LAYERFS_IMAGE`) with fewer than 16 Workspaces
and starts the next number, on that image, when every such one is full. One
project's Sandboxes may run different images; the mount result and
`operator.sandbox.list()` give each one's `image`. A Workspace is
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
it. The next mount or `operator.sandbox.list()` that meets a stopped or
removed container reclaims the Sandbox: it deletes a stopped container
(unforced, once) and removes the Sandbox's records, so the number is used
again. A Workspace call to a Sandbox whose container no longer runs is
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
| workspace (5) | `mount(branch)` → `{ sandbox, path }`, `exec(ws, command, message?, { stdout?, stderr? }?)`, `status(ws)`, `commit(ws, message?, force?)`, `unmount(ws, discard?)` |

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
`Binary`, and `fs.stat` reports its size. `node sdk/js/test/binary_write.mjs`
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

## Local Sandboxes (no Docker)

`LAYERFS_SANDBOX=local` runs each Sandbox's daemon as a process of this
machine instead of in a container, for a Linux VM or CI job that has no Docker
and is itself the isolation boundary (a microVM, for instance). **It isolates
nothing**: the daemon runs as root in this machine's namespaces, and commands
share its processes, network and filesystem; they only run as their own uid
501 and gid 20, with no supplementary groups. Use it only where everything on
the machine belongs to the same trust domain. Docker stays the default.

It needs root (the daemon mounts FUSE and commands run as another identity;
otherwise `workspace.mount` is refused `Unsupported`), `/dev/fuse` (the
daemon narrows it to 0600, as it does in a container), the daemon executable
(`LAYERFS_DAEMON`), `LAYERFS_LOCAL_DIR` and the client's `storePath`, which
the daemon opens directly: the Store calls and the daemons use the same file,
on ext4, xfs or btrfs. `LAYERFS_DOCKER`, `_IMAGE`, `_VOLUME` and
`_DAEMON_HOST` are not read. Everything else above holds: numbered
Sandboxes of up to 16 Workspaces, placement, idle close after 2 minutes,
reclaim, records under `LAYERFS_HOME/<project>/sandboxes/<n>/`, background
processes, start intents and their refusals.

Sandbox `n` lives in `LAYERFS_LOCAL_DIR/<prefix><project>-<n>/`:
`config/daemon.setup` (its daemon setup, 0600 in a 0700 directory),
`overlay/` (0700), `workspaces/<k>` (the Workspace mounts, so a Workspace's
`path` is under this directory), `tmp/` (1777, the commands' `HOME` and the
background processes' logs) and `daemon.log` (the daemon's output, what
`docker logs` shows for a container; a failed start's report carries its last
20 lines). The daemon listens on 127.0.0.1 only. Its records name the daemon
by process id and start time, so a reused id is never taken for it. A mount
result's `image` is `local`; local and Docker Sandboxes of one project are
never mixed. A daemon that ends (idle close, crash, kill) is the stopped
observation: Workspace calls to it are refused, and the next mount or
`operator.sandbox.list()` reclaims it, detaching any mount the dead daemon
left before deleting its directory. Nothing outside that directory is
removed; the Store is never touched.

## Native Sandboxes (macOS)

`LAYERFS_SANDBOX=native` is the local runtime for macOS, with no root, no
Docker, no kernel extension and no macFUSE or FUSE-T. Each Sandbox's daemon
runs as a process of this user and serves its Workspaces over NFSv3 on
127.0.0.1 (`LAYERFS_KERNEL=nfs`); macOS's own client mounts them
(`/sbin/mount_nfs`, as this user) under the Sandbox's `workspaces/`. Commands
run as this user, in this machine's namespaces: like `local`, **it isolates
nothing** beyond what the caller's own sandbox (for instance Seatbelt)
imposes. A command starts with this process's `PATH` (so `node`, Homebrew's
tools and the like are found as in the caller's own shell) and its own
environment, nothing else of this process's; a `local` Sandbox on Linux keeps
a container's fixed search path.

It needs the daemon executable (`LAYERFS_DAEMON`), `LAYERFS_LOCAL_DIR` (owned
by this user, 0755; the directories above it need not be traversable by
others) and the client's `storePath` on a local apfs or hfs volume; a Store on
any other filesystem is refused `Unsupported`. Directory layout, records, idle
close and reclaim are those of local Sandboxes; a record's `runtime` is
`native`, and native, local and Docker Sandboxes of one project are never
mixed. Before a Commit the daemon writes back what the client buffered; after
a Commit or merge installs another root, it makes the client drop the replaced
files' cached attributes at once.

`LAYERFS_NFS_MOUNT`, `_UNMOUNT`, `_FORCE` and `_FLUSH` replace the daemon's
helper commands (`{port}` and `{dir}` are filled in). The default mount is
`/sbin/mount_nfs -o vers=3,tcp,port={port},mountport={port},locallocks,nfc,actimeo=60,rsize=131072,wsize=131072,soft,intr,timeo=20,retrans=6,nobrowse 127.0.0.1:/ {dir}`.
`LAYERFS_NFS_REFRESH=0` turns the refresh after an install off (the client
then revalidates within `actimeo`, 60 s).

Limits: background processes (`operator.process.*`) still read `/proc` and are
not available on macOS; many small writes cost about six times APFS's time;
checked on Apple silicon with macOS 27.2. On Linux the Store install needs
root, which a native Sandbox's commands may not be: use `local` there.

## Internal operator

```js
import { LayerFSOperator } from "@layerfs/sdk/operator";
const operator = new LayerFSOperator(layerfs);
await operator.store.stats();
await operator.store.commits("feature", 20, 0);
await operator.store.overwritten(layer, 50, 0); // every Layer a publish overwrote
await operator.files.stats("feature", "/README.md");
await operator.files.history("feature", "/README.md", 20, 0);
await operator.workspace.overlay(ws);
await operator.workspace.changes(ws, 20, 0);
await operator.sandbox.list(); // the project's running Sandboxes; reclaims stopped ones
await operator.inspect.snapshot();
await operator.project.progress(); // while layerfs.project.init runs in this process
const process = await operator.process.start(ws, "npm run dev"); // "proc_1_…"
await operator.process.list(); // every Sandbox's, each with its `sandbox`
await operator.process.cancel(process.process);
```

Operator names are refused by public JS/native/Rust/daemon dispatch. This subpath
uses its own method table; process control remains exclusively in the binding.
HTTP exists only in the local demo server, which maps GET reads and POST
`{args:[...]}` writes at `/api/{sdk|operator}/{group}/{method}`.

`project.progress()` reads the project's Init while `project.init` runs in the
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

## Call trace

`LAYERFS_TRACE=<file>` (else `DSH_LAYERFS_TRACE`), or
`new LayerFS(project, { trace: file })` (the option wins), appends one JSON line to `file` for every SDK and operator call that
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
| `method` | The binding method, for example `fs.read`, `workspace.commit`, `operator.store.stats`. |
| `args` | Short summary: text arguments (paths, refs, names, Sandbox id, command, message) cut to 200 characters; numbers and flags; `content`, `bytes`, `oldText` and `newText` as `<name>Bytes` byte counts; `apply` changes as `changes` (count) and `changeBytes`. Never file contents; `project`, `home` and `deploy` are left out. |
| `start`, `ms` | Wall-clock start (ISO 8601) and elapsed wall milliseconds measured in the calling process, including the native queue and the daemon round trip. |
| `queue` | The part of `ms` the call spent waiting before its own work, in milliseconds: `thread`, the wait for a native worker thread (the call's time outside its run in the binding), `connection`, the wait for a free control connection of its Sandbox, and their sum `ms`. Present on every line of a call that reached the binding. |
| `ok` | `true` for a value; `false` with `code` and `message` for a refusal. |
| `phase` | `end` on the line written when the call returns; `start` on the optional line written before it (see below). |
| `branch`, `base`, `head`, `sandbox`, `path`, `baseLayer`, `process`, `exitCode` | Result ids, on a successful end line only. `branch.fork`: the new `branch`, its `base` Layer and its `head` (`null` when forked from main). `branch.merge`: the new `branch` and its `head`. `workspace.mount`: the `sandbox`, the Workspace `path`, its `branch`, its `base` Commit (`null` on a Branch with no Commit) and `baseLayer`. `operator.process.start`: the `process` id. A foreground `workspace.exec`: its `exitCode`; its output is never traced. |
| `kind`, `commit`, `layer`, `sizes` | The value's `kind`. Every publishing call that succeeds (`fs` write, edit, apply, mkdir, rm, cp, mv, `workspace.commit`, `branch.merge`, `branch.publish`) names a `commit`: its new Commit, or the head it was `UpToDate` with; `branch.publish` names the published source Commit and the new `layer`. `sizes` (`logical`, `newCanonical`, `newStored`, `newMetadata`) appear only when the reply carries the Commit record (`fs` writes and `workspace.commit` that commit); join `commit` to `operator.store.commits` for the rest. No call is added to fetch them. |
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
change it between calls; `new LayerFS(project, { trace, traceStep })`, or
setting `layerfs.options.traceStep` later, overrides it. Unset or empty, lines
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

Checks: `node sdk/js/test/trace.mjs <case>` runs one case (`on`, `off`,
`failure`, `dsh-env`, `start`, `ids`, `no-content`) over a stand-in binding;
`node sdk/js/test/verify/trace_real.mjs <file>` traces a fork, mount, exec,
process, commit and merge on a fresh Disposable Store with only the `DSH_`
names set.

## Builds and compatibility

Run `npm --prefix sdk/js run build:native` on the macOS/Linux host that loads the
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
