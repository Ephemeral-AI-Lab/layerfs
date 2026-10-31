# Area P public contracts

Each slice writes its precise public contract here before implementation,
beside the shared P6 cases (`cases.mjs`, `cases-area-p.mjs`). The contract is
the test oracle: the JS cases and the Rust `area_p*` suites check these
shapes and refusals, not the implementation's own wording.

## P10 — per-call results (`client.results`)

**Surface**: `@layerfs/agent`. `client.results(after?)` on every client made
by `LayerFS.open` / `LayerFS.init`. Synchronous and local: it never dials the
binding, adds no binding call, and stays readable after `client.close()`. The
`layerfs` command has no results command (one process per command, like the
event stream). P4/P5 map the same shape onto Python and MCP.

**What is recorded**: exactly the calls a trace file would record — every
agent and backend operation through the client (including `client.call`),
plus the client's own `project.init`, its open-time `project.list_branches`
lookup, and `events.unsubscribe`. The untraced internal
`workspace.exec_release` follow-up and the Store open/close plumbing record
nothing. Recording is always on, independent of `LAYERFS_TRACE`.

**Record shape** (one per recorded call, in completion order):

```js
{
  seq: 7,                        // per-client, starts 1, +1 per record, never reused
  method: "fs.write",
  start: "2026-10-30T04:05:06.789Z", // ISO, when the call was sent
  ms: 12.345,                    // client-side duration to settle, 3 decimals
  ok: true,
  // only when ok === false:
  code: "NoSuchPath",            // the refusal code; null for a non-refusal throw
  message: "...",                // the refusal message
  // the answer's outcome kind, whenever it names one (Committed, UpToDate,
  // Merged, Added, Forked, Initialized, ...):
  kind: "Committed",
  // only for a successful publishing write (fs.write/edit/apply/mkdir/rm/cp/mv,
  // workspace.commit, branch.merge, branch.publish):
  commit: "c0ffee",              // the new Commit; UpToDate/publish name their head/source
  layer: "lay1",                 // branch.publish only: the published Layer
  sizes: { logical: 120, newCanonical: 40, newStored: 30, newMetadata: 8 },
}
```

No arguments, result ids, queue attribution or daemon telemetry: those stay
in the trace file (`LAYERFS_TRACE`). `sizes` is present only when the answer
carries the Commit record; `sizes.newStored` is the bytes added to the Store.
A successful call whose answer names no outcome kind carries no `kind`;
`commit`/`layer`/`sizes` appear only on successful publishing writes.
An `ok: true` record carries no `code`/`message`.

**Paging and bound**: the client keeps its most recent **1024** records.
Overflow drops oldest records without reusing sequence numbers.

`client.results(after = 0)`:

- `after` must be null/undefined or a nonnegative safe integer, else
  `InvalidInput` "after must be a nonnegative safe integer".
- Returns `{ items, next, evicted, stale? }`:
  - `items`: kept records with `seq > after`, oldest first.
  - `next`: the last returned record's `seq`, or `after` itself when nothing
    was returned; pass it as the next `after`.
  - `evicted`: total records dropped so far (0 at first, only grows).
  - `stale`: `true` when records after `after` were dropped (`after + 1` is
    older than the oldest kept record); absent otherwise. `items` still
    carries every kept record after `after`.

**Ordering**: records are appended when a call settles, so parallel calls
appear in completion order and `start` may be out of order. `seq` order is
the completion order.

**Agreement with the trace**: with `LAYERFS_TRACE` on, each record and its
trace line agree on method, start, ms, ok, code, message, kind, commit, layer
and sizes. Recording changes no call's value or refusal.

## P13 — folder-scoped client (`LayerFS.open` option)

**Surface**: `@layerfs/agent` only in this slice. Python mirroring is deferred
to P4. The CLI has no scope flags in P13. `LayerFS.init` does not accept
`scope`; this option scopes operations on an existing project.

**Construction**: `await LayerFS.open(project, { scope: { folder, branch } })`.
`scope` must be a plain object with required `folder` and optional `branch`,
with no unknown keys. `folder` is a canonical project-root absolute path,
e.g. `/packages/editor`: it starts with `/`, has no empty, `.` or `..`
component, and has no trailing slash except `/`. `/` scopes the full file tree.
`branch`, when given, is a namespace prefix that satisfies the Store's
HistoryName grammar: 1–63 bytes, lowercase ASCII letters, digits, `.`, `_`,
or `-`, starting and ending with a letter or digit. Malformed scope values
refuse `InvalidInput` at construction.

**Path confinement**: API file paths remain project-root absolute paths; the
client does not rewrite them relative to `folder`. For `/packages/editor`, the
inclusive scope is that exact path and descendants on component boundaries.
A path is in scope only if it is canonical, absolute, and beneath the folder;
relative paths, empty components, `.` and `..` components are out of scope.
Every path-bearing Store operation checks before dispatch: `fs.list`, `glob`,
`grep`, `stat`, `read`, `blame`, `write`, `edit`, `apply`, `mkdir`, `rm`, `cp`,
`mv`, and `diff_files`. `apply` checks every path in every change. `cp` and
`mv` check both source and destination; `diff_files` checks both paths. No
partial call is sent when any path is out of scope. A scoped listing/search
starts only at an in-scope directory and can return only entries within it.
In-scope operations keep today's argument and result shapes.

**History and Branch namespace**: `fs.blame` and `fs.diff_files` are available
with their file paths checked above. The path-returning history operations
`branch.list_commits`, `graph`, `changed_paths`, `diff_commits`, and
`diff_branches` refuse for scoped clients with `InvalidInput`, message class
`out of scope: history`, detail `null`; P13 does not filter history results.
Scoped `project.list_branches` lists only names beneath the configured Branch
namespace and returns names relative to that namespace. Branch operations that
name Branches are permitted only when `scope.branch` exists. Each supplied
relative Branch name is composed with the namespace as `<prefix>.<name>`; names
outside it or invalid after composition refuse before dispatch. The prefix,
relative names, and every composed name must satisfy HistoryName, including
its 63-byte limit. With no `scope.branch`, Branch-namespace operations refuse
`InvalidInput` with message class `out of scope: branch`.

**Workspace and Exec**: `workspace.mount(sandbox)` takes only the Sandbox for
a scoped client and mounts `<scope.branch>.main`; `scope.branch` is required
for mount. That Branch must already exist; otherwise normal `NoSuchBranch`
behavior applies. Supplying a Branch or extra argument to scoped mount refuses
`InvalidInput` with message class `out of scope: branch`. The mounted Workspace
still exposes the complete tree; P13 makes no claim that mount is a filtered
filesystem. `workspace.exec` starts with its working directory set to
`scope.folder`, keeps its existing result shape, and cannot override that
folder through the public API. Shell code can still access the complete mounted
tree under the underlying Workspace contract: scope is API-call namespace
guarding, not filesystem isolation (`command` can still `cd` anywhere in the
complete mounted tree). `workspace.commit` commits the
Workspace frontier as today, attributed to the Branch selected at mount.
Workspace mount, exec, commit and unmount preserve their unscoped result shapes.

**Other operations and refusals**: `sandbox.create`, `sandbox.close`,
`project.list_sandboxes`, local `results()`, and `close()` retain current
behavior. Scoped clients refuse `events.subscribe` with `InvalidInput`, message
class `out of scope: events`, detail `null`, because event payloads can contain
paths and P13 does not filter them. A scoped client has no `backend` property;
`LayerFSBackend.of(scopedClient)` refuses `InvalidInput`, message class
`out of scope: backend`, detail `null`, because `backend.store.fs.get_storage`
and `list_revisions` take `path` and other backend calls can return
unfiltered Workspace change paths and history/branch data. `client.call()` uses the same guards as
named methods; unknown methods retain existing refusal behavior. Every
out-of-scope path, Branch, history, event, or backend attempt refuses before
binding dispatch with `LayerFSError` code `InvalidInput`, message class starting
exactly `out of scope: `, and `detail: null`. Without `scope`, every existing
operation behaves exactly as today.

**Composition**: `scope.branch` is a namespace prefix, not a selected leaf.
For `{ folder: "/packages/editor", branch: "team.editor" }`, a relative name
`main` is sent as `team.editor.main`; returned Branch names strip
`team.editor.`. Workspace mount uses `team.editor.main`. Scope applies within
each project and never changes project selection. The mounted Workspace remains
a complete tree even though direct file operations are folder-scoped.

## P11 — Store statistics (`backend.store.project.get_storage` extended)

**Surface**: `@layerfs/backend` `backend.store.project.get_storage(options?)`,
on the existing call. The CLI has no backend commands; P4/P5 map the same
shape onto Python and MCP. Builds on today's totals; no new call, no second
Store implementation, no Store schema change.

**Call**: `get_storage()` or `get_storage({ commits: true })`. Any other
option key, a non-object options argument, or a non-boolean `commits` refuses
`InvalidInput` (validated in the JS wrapper and again at the operator
boundary).

**Reply**: the existing fields (`logical`, `canonical`, `stored`, `metadata`,
`file`, `free`, `dedupe`, `compression`, `init`) unchanged, plus:

```js
{
  // always present:
  disk: { main, wal, total },
  // only when commits: true:
  commits: {
    Workspace: { count, newCanonical, newStored, newMetadata },
    Files:     { count, newCanonical, newStored, newMetadata },
    Merge:     { count, newCanonical, newStored, newMetadata },
  },
}
```

- `disk.main`: byte length of the Store's main SQLite file. `disk.wal`: byte
  length of its `<store>-wal` file, `0` when no WAL file exists. `disk.total`:
  `main + wal`. Physical file lengths, unlike `file` (logical DB pages). The
  `-shm` file is a transient shared-memory index, not data, and is excluded.
- `commits.<kind>`: over the whole commit catalog, grouped by `CommitVia`:
  `count` commits of that kind and the sums of each commit's own
  `newCanonical`, `newStored`, `newMetadata` — the same counters
  `backend.store.branch.list_commit_storage` reports per commit. All three
  kinds are always present, zero-filled when a kind has no commits. Per-commit
  snapshot `logical` is deliberately NOT summed: a snapshot size is not an
  additive storage statistic. `raw`/`unique`/`compressed` bytes are the
  existing `logical`/`canonical`/`stored` fields.
- Every figure is computed fresh from the real Store on each call: no
  estimation, no sampling, no caching across calls. If the Store's files
  cannot be measured the call fails (`Failed` with the cause) instead of
  returning partial or estimated statistics.

**Cost**: the default call reads maintained totals, page pragmas and two file
stats — no scan of the object catalog or the commit catalog (unchanged from
today, plus two stats). `commits: true` adds ONE full scan of the commit
catalog, O(number of commits), grouped in SQL; this is not cheap and is never
presented as cheap. The reply is bounded: `disk` plus at most three small
kind maps, so no paging is needed.

## P12 — cheap Store summary (`backend.store.project.get_summary`)

**Surface**: `@layerfs/backend` adds the no-argument
`backend.store.project.get_summary()` call for issue layerfs-connect#63. It is
additive and does not alter P11's complete `get_storage()` contract. The call
is computed fresh from the real open Store every time: no cached, sampled or
estimated values.

**Reply (exact JS shape)**:

```js
{
  logical: 123,                 // logical bytes, integer
  branches: 2,                  // exact Branch count for this Store
  commits: 5,                   // exact Commit count for this Store
  mainLayer: "lay1",            // main Branch's head Layer id; null if absent
  lastCommit: {                  // main Branch's head Commit; null if absent
    id: "c0ffee",
    createdMs: 1761800000000,    // Unix epoch milliseconds, integer
  },
}
```

- `logical` is the maintained logical-byte total from `store_totals` (O(1));
  it is not the SQLite file length, unique canonical bytes or compressed bytes.
- `branches` is the exact number of Branch rows belonging to this Store's
  LayerStack. `commits` is the exact number of Commit rows belonging to that
  LayerStack. Both are nonnegative integer counts.
- `mainLayer` is the main Branch's head Layer identity. `lastCommit` is the
  main Branch's current head Commit identity and its creation timestamp. They
  are both `null` when main has no head Commit; IDs are checked lowercase hex
  strings and `createdMs` is an integer number of milliseconds since Unix epoch.
  The `lastCommit` value is a Commit record, not the globally newest Commit on
  another Branch.
- The object has exactly these five keys. P12 omits P11's disk/WAL sizes,
  canonical/stored/metadata totals, per-Commit-kind accounting and full history.

**Cost — SummaryBound**: exactly five fixed SQL statements, each returning at
most one row: maintained totals; Branch count; Commit count; main Branch
lookup; main head Commit lookup. No object-catalog scan, no per-Commit column
reads, and no cache. The Branch and Commit counts are index-only `COUNT(*)`
aggregates using their LayerStack-leading unique indexes, with EXPLAIN plans
showing no table scan. Statement and returned-row counts are fixed for every
store size: the five queries run in one read snapshot, whose surrounding
`BEGIN`/`COMMIT` are the only additional statements. The runtime fixture
therefore observes 7 statements and 5 returned rows at every size, and the
test asserts exactly those fixed numbers. The database still visits O(B) Branch index entries and O(C) Commit
index entries for B Branches and C Commits; P12 does not claim O(1) page work.
The remaining three reads are maintained/point lookups with bounded result
rows. Runtime evidence compares two sizes using only `Fixture::work` statement
and row counters, not VM-step counters, and SQL page-work growth is explained
from the index-only EXPLAIN plans.

A maintained Branch/Commit counter schema was considered and rejected for this
slice: it changes Store format identity and migration chains, while this
contract can state and test the honest index-work bound without a schema change.

**Refusals**: non-object arguments, options, or unknown option keys refuse
`InvalidInput`. Store-open and query failures preserve the existing exact
Store/History refusal code and cause; no partial summary is returned.

## P4 — Python SDK (D10)

**Package and constructors**: `layerfs` is a Python package. `LayerFS.open(name, options=None)` opens an installed project; `LayerFS.init(name, folder, timeout=300, options=None)` initializes then opens it. Both return an async context manager/client with explicit `close()`. Options mirror Node (`home`, `trace`, `deploy`, `storePolicy`, `storePath`); explicit values override `LAYERFS_*` once at construction. Store calls use the calling process on Linux and the existing authenticated fixed-length JSON worker protocol on macOS/Windows. No Linux worker route or alternate Store exists.

**Client groups**: `client.fs`, `client.project`, `client.branch`, `client.sandbox`, and `client.workspace` expose the Node agent methods with the same argument order, defaults, returned JSON-compatible shapes, and behavior. Python uses snake_case names. `client.backend` exposes the Node backend methods under `store` and `daemon`; methods and options retain the same shapes. `client.results(after=0)` synchronously returns the exact P10 page and remains readable after close. `client.call(method, args)` provides the same agent dispatch, rejecting backend/runtime names as `UnknownMethod`. Client `close()` releases the Store once; it leaves mounted Workspaces intact. `LayerFSError` has `.code`, `.message`, and `.detail`. As in Node, `.message` is exactly `${code}: ${message}` (including the prefix), while `.detail` is the unchanged structured detail; the unprefixed refusal text is not exposed as `.message`. Non-refusal failures remain ordinary exceptions with their original cause where available.

**Operation mapping**: all Node agent and backend operations are present/equivalent, listed below. Runtime-management APIs are explicitly deferred: they own host installation, Docker/WSL worker lifecycle, project removal, volume/container cleanup, and init-job orchestration, rather than a project client; Python uses the already-defined worker endpoint for Store calls and does not create a second runtime manager. Node `workspace.exec` has equivalent async Python behavior returning exit code/stdout/stderr; long-running stream callbacks and Node-specific stream objects are deferred pending a language-neutral stream contract. `workspace.exec` does not make filesystem state depend on command registration.

| Node namespace/method | Python parity | Notes |
| --- | --- | --- |
| `project.init` / `LayerFS.init` | equivalent | async init then open; same timeout and Store configuration |
| `project.list_branches` | present | same paging/result |
| `project.list_sandboxes` | present | same paging/result |
| `fs.apply`, `blame`, `cp`, `diff_files`, `edit`, `glob`, `grep`, `list`, `mkdir`, `mv`, `read`, `rm`, `stat`, `write` | present | same arguments, normalization, result/refusals; binary read uses bytes in Python |
| `branch.changed_paths`, `diff_branches`, `diff_commits`, `fork`, `graph`, `list_commits`, `merge`, `publish` | present | same arguments and paged shapes |
| `sandbox.create`, `list_workspaces`, `close` | present | same lifecycle/result |
| `workspace.mount`, `exec`, `commit`, `unmount` | equivalent | execution result normalized to Python values; callback streaming deferred as described above |
| `events.subscribe`, `events.poll`, `events.unsubscribe`, async iteration | deferred | `layerfs-api` exposes no events surface; its only `event` occurrence is a Sandbox listener wait in `sandbox/create.rs`. Node's queues are binding-private, so Python cannot reuse them through the public Rust API. No Python `client.events` is provided. |
| `client.results(after)` (P10) | present | local synchronous accessor; exact 1024-record cursor/eviction contract |
| `backend.store.project.get_history`, `get_storage`, `get_comparable`, `list_overwritten` | present | `get_storage(options=None)` accepts only `{commits: bool}` per P11; default omits `commits` |
| `backend.store.branch.list_commit_storage` | present | same paging/result |
| `backend.store.fs.get_storage`, `list_revisions` | present | same args/result |
| `backend.daemon.workspace.status`, `list_changes`, `get_storage` | present | same args/result |
| `backend.daemon.process.start`, `list`, `cancel` | present | same args/result; process lifecycle stays caller-owned |
| `backend.daemon.sandbox.get_log` | present | same args/result |
| `LayerFSRuntime.open` | deferred | Host/runtime lifecycle API, outside project SDK. Python does not take ownership of deployment policy. |
| `runtime.status` | deferred | Host status and sweep report are outside the project client. |
| `runtime.worker.start` | deferred | Host owns worker startup. Python Store calls only consume the existing authenticated worker endpoint. |
| `runtime.worker.restart` | deferred | Host owns worker restart and its explicit restart confirmation. |
| `runtime.project.list` | deferred | Host project inventory is outside the project client. |
| `runtime.project.start_init` | deferred | Host async init-job orchestration is outside the project client. |
| `runtime.project.get_init` | deferred | Host init-job observation is outside the project client. |
| `runtime.project.subscribe_init` | deferred | Host init-job subscription is outside the project client. |
| `runtime.project.get_state` | deferred | Host project-state observation is outside the project client. |
| `runtime.project.remove` | deferred | Host destructive project removal is outside the project client. |
| `runtime.sandbox.reclaim` | deferred | Host container reclamation policy is outside the project client. |
| `runtime.sandbox.list_leftovers` | deferred | Host leftover-container inventory is outside the project client. |
| `runtime.sandbox.clean_leftovers` | deferred | Host leftover-container cleanup is outside the project client. |
| `runtime.sandbox.list` | deferred | Host-wide Sandbox inventory is outside the project client. |
| `runtime.sandbox.sweep` | deferred | Host Sandbox sweep policy is outside the project client. |
| `runtime.volume.list` | deferred | Host volume inventory is outside the project client. |
| `runtime.volume.remove` | deferred | Host destructive volume removal is outside the project client. |
| `runtime.events.subscribe` | deferred | Runtime-wide event stream is outside the project client's deferred events surface. |
| `LayerFSRuntime.close` | deferred | Host/runtime lifecycle cleanup is outside the project client. |
| Push/pull | deferred, blocked | R7 #42 is unresolved; no method, stub, local simulation, or successful-looking transfer result is provided. |
| P12 cheap project summary | pending reconciliation at merge | Implemented by parallel slice; reconcile Python contract against its landed public shape. |
| P13 folder-scoped client | pending reconciliation at merge | Implemented by parallel slice; do not invent names/semantics before merging its contract. |

**Refusal/error mapping**: every typed Node refusal becomes `LayerFSError` with the identical `.code`, prefixed `.message` (`${code}: ${message}`), and structured `.detail`: `Conflict`, `HeadMoved`, `Dirty`, `Busy`, `NoSuchPath`, `NoSuchBranch`, `NoSuchCommit`, `NoSuchProject`, `NoCommit`, `NoMergeBase`, `IsDirectory`, `NotDirectory`, `NotFile`, `Binary`, `NotText`, `NoMatch`, `AmbiguousMatch`, `Exists`, `NotEmpty`, `NameTaken`, `ReservedName`, `ReadOnly`, `MountsActive`, `TooLarge`, `InvalidInput`, `NotMounted`, `NoSuchSandbox`, `NoSuchProcess`, `Capacity`, `NotComparable`, plus existing codes not in `CODES` such as `UnknownMethod`, `Unsupported`, `Failed`, `NotFound`, and `Unknown`. Never infer a refusal from message text. Transport/protocol/authentication failures remain errors, not success-shaped values; an uncertain mutation is surfaced without retry or replay.

**Shared tests**: Python drives the existing P6 cases (`cases.mjs`, `cases-area-p.mjs`, P1/P7/P8 cases and refusal payloads) through the Python client and compares operation name, sent arguments, result/refusal code/message/detail. It also mirrors P10 results and P11 `get_storage({commits: true})` options and invalid-option refusals. Linux real-Store tests use fresh Disposable fixtures only and cover init/open, filesystem read/write/list, Branch/Commit/history, and storage stats. Until those real-Store checks pass, real-Store behavior and cost claims are target behavior, not verified Python behavior; P11 cost claims remain owned by the operator implementation and its Rust tests, not by the stand-in parity suite.

## P5 — MCP server (D11, stdio)

**Package**: `@layerfs/mcp`, a Node ESM package in the `api/` workspace.
It uses `@modelcontextprotocol/server` 2.x, Zod 4, and stdio only. One
server process owns one opened `@layerfs/agent` client; it closes that client
on stdin EOF/SIGINT and unmounts Workspaces it created before closing. It
creates no Store implementation and does not persist client state. The server
writes protocol frames only to stdout; diagnostics use stderr.

**Grouping**: one MCP tool per SDK operation gives each call its own exact
Zod input object and avoids a generic method dispatcher that weakens schema
validation and tool descriptions. `layerfs_init` and `layerfs_open` establish
the single server client; every operation tool uses that client. `layerfs_close`
closes it. `layerfs_results` reads its P10 ring. The `events.subscribe`
stream is exposed as `layerfs_events` returning the initial event snapshot and
then MCP notifications until cancellation, close, or a stale/error termination
(the SDK throws `InvalidInput: stream went stale; resubscribe`); no
reconnect/replay guarantee is added. MCP cannot preserve binary process stdout
as bytes, so Exec returns the SDK's current text/exit/truncation shape.

### Tool names and exact input shapes

Fields that name a Branch (`branch`, plus `layerfs_branch_fork.name` and
`layerfs_branch_merge.{branch,with,name}`) use the project-name grammar:
lowercase ASCII alphanumerics plus `.`, `_` and `-`, first/last alphanumeric,
at most 63 bytes; `/` is invalid. Fields named `ref`, `from`, `to`, `commit`,
`leftRef` or `rightRef` are references, not constrained to that grammar.

All fields below use JSON Schema/Zod equivalents: `string`, `boolean`,
`integer` (safe integer), `number`, `array<T>`, and `object`. Unless marked
required, fields are optional. Object schemas reject unknown keys. Optional
nullable fields accept `null` as well as omission. Every call returns the
Node SDK's JSON-compatible result unchanged (Buffer/Uint8Array values are
encoded by the MCP JSON result serializer as `{ "$layerfs": "bytes",
"base64": string }`, recursively); `layerfs_fs_read` returns the complete SDK
result unchanged — its text or bytes body plus whatever read metadata the SDK
answers (`nextLine`, `nextOffset`), never a fixed subset.

#### Client lifecycle and observation

| Tool | Input object | Output |
| --- | --- | --- |
| `layerfs_init` | `name:string` required, `path:string` required, `timeoutMs:integer` optional positive; `options:{home?,trace?,storePolicy?,storePath?,deploy?}` optional | `{created,client:{projectName}}` |
| `layerfs_open` | `name:string` required, `options` same as init | `{client:{projectName}}` |
| `layerfs_close` | `{}` | `{closed:true}`; idempotent |
| `layerfs_results` | `after:integer` optional, default 0, nonnegative | P10 `{items,next,evicted,stale?}` |
| `layerfs_events` | `{}` | `notifications/layerfs_snapshot` with `{cursor,snapshot}`, then `notifications/layerfs_event` with `{event}`; the call stays in flight and streams until cancellation, close, or stale/error termination (the SDK throws `InvalidInput: stream went stale; resubscribe`); cancellation unsubscribes and MCP sends no result for a cancelled call; there is no replay |

`options.deploy` permits optional `docker,image,volume,daemon,containerPrefix,
daemonHost` strings. Settings are passed once to the Node SDK and are resolved
using the SDK's existing environment behavior. No credentials are accepted by
MCP tools.

#### Project and Branch history

| Tool | Input object |
| --- | --- |
| `layerfs_project_list_branches` | `pageSize:integer` 1..1000, `pagePos:integer` >=0; defaults 50,0 |
| `layerfs_project_list_sandboxes` | same paging fields/defaults |
| `layerfs_branch_list_commits` | `branch:string` required, same paging fields/defaults |
| `layerfs_branch_graph` | `branch:string` required, same paging fields/defaults |
| `layerfs_branch_changed_paths` | `commit:string` required, same paging fields/defaults, `cursor:string|null` optional |
| `layerfs_branch_diff_commits` | `from:string`, `to:string` required, paging fields, `cursor:string|null`, `hunks:boolean` optional |
| `layerfs_branch_diff_branches` | `from:string`, `to:string` required, paging fields, `cursor:string|null`, `hunks:boolean` optional |
| `layerfs_branch_fork` | `name:string` required, `from:string` optional default `main` |
| `layerfs_branch_merge` | `branch:string`, `with:string`, `name:string` required, `message:string`, `units:array<string | {name:string,patterns:array<string>}>` optional |
| `layerfs_branch_publish` | `branch:string` required, `commit:string` optional |

#### Files

| Tool | Input object |
| --- | --- |
| `layerfs_fs_read` | `ref:string`, `path:string` required, `startLine:integer` >=1 default 1, `endLine:integer` optional >=1, `binary:boolean` default false, `offset:string|null` optional opaque continuation from a previous answer |
| `layerfs_fs_list` | `ref:string` required, `path:string` optional default `/`, paging fields |
| `layerfs_fs_stat` | `ref:string`, `path:string` required |
| `layerfs_fs_grep` | `ref:string`, `pattern:string` required, `path:string` optional `/`, paging fields |
| `layerfs_fs_glob` | `ref:string`, `pattern:string` required, `path:string` optional `/`, paging fields |
| `layerfs_fs_diff_files` | `leftRef:string`, `leftPath:string`, `rightRef:string`, `rightPath:string` required |
| `layerfs_fs_blame` | `ref:string`, `path:string` required, `fromLine:integer` >=1 optional default 1 |
| `layerfs_fs_write` | `branch:string`, `path:string`, `content:string` required; `base64:string` alternative to content; `message:string`, `mode:integer` optional |
| `layerfs_fs_edit` | `branch:string`, `path:string`, `oldText:string`, `newText:string` required; `message:string` optional |
| `layerfs_fs_apply` | `branch:string` required, `changes:array<change>` required, `message:string` optional; each change is exactly one of the SDK's seven: `{op:"write",path,content}` or `{op:"write",path,base64}` (exactly one body) with `mode:integer` optional, `{op:"edit",path,oldText,newText}`, `{op:"mkdir",path,mode?:integer}`, `{op:"chmod",path,mode:integer}`, `{op:"rm",path}`, `{op:"cp",from,to}`, `{op:"mv",from,to}` |
| `layerfs_fs_mkdir` | `branch:string`, `path:string` required; `message:string`, `mode:integer` optional |
| `layerfs_fs_rm` | `branch:string`, `path:string` required; `message:string` optional |
| `layerfs_fs_cp` | `branch:string`, `from:string`, `to:string` required; `message:string` optional |
| `layerfs_fs_mv` | same as cp |

`content` and `base64` are mutually exclusive. A `write` uses UTF-8 text
or explicit base64 bytes; no host path is read by the server.

#### Sandbox, Workspace, and Bash

| Tool | Input object |
| --- | --- |
| `layerfs_sandbox_create` | `image:string` optional (the client's already-resolved deployment supplies the other settings) |
| `layerfs_sandbox_list_workspaces` | `sandbox:string` required, paging fields |
| `layerfs_sandbox_close` | `sandbox:string` required |
| `layerfs_workspace_mount` | `sandbox:string`, `branch:string` required |
| `layerfs_workspace_exec` | `workspace:{sandbox:string,path:string}` required, `command:string` required, `message:string` optional |
| `layerfs_workspace_commit` | `workspace:{sandbox:string,path:string}` required; `message:string`, `force:boolean` optional; `options:{author:string,labels:array<string>}` optional |
| `layerfs_workspace_unmount` | `workspace` required; `discard:boolean`, `force:boolean` optional |

`layerfs_workspace_exec` is the Bash operation; the command executes only in
the selected LayerFS Sandbox Workspace. The server does not expose host shell.
Workspace handles are accepted only from a successful `layerfs_workspace_mount`
result or the corresponding list result. Created Workspaces are unmounted on
server shutdown (default no discard), then the client closes; cleanup failures
are reported to stderr without forging success.

#### Existing backend calls (Node SDK parity)

The backend SDK calls are also exposed because they are existing callable
operations, not separate MCP semantics:

| Tool | Input object |
| --- | --- |
| `layerfs_backend_get_history` | `since:string` optional (the previous answer's `version`) |
| `layerfs_backend_get_storage` | `options:{commits:boolean}` optional |
| `layerfs_backend_get_comparable` | `ref:string` required (a saved Commit or Layer id) |
| `layerfs_backend_list_overwritten` | `layer:string` required, paging fields |
| `layerfs_backend_list_commit_storage` | `branch:string` required, paging fields |
| `layerfs_backend_fs_get_storage` | `ref:string`, `path:string` required |
| `layerfs_backend_fs_list_revisions` | `ref:string`, `path:string` required, paging fields |
| `layerfs_backend_workspace_status` | `workspace` required |
| `layerfs_backend_workspace_list_changes` | `workspace` required, paging fields |
| `layerfs_backend_workspace_get_storage` | `workspace` required |
| `layerfs_backend_process_start` | `workspace` required, `command:string` required |
| `layerfs_backend_process_list` | paging fields |
| `layerfs_backend_process_cancel` | `process:string` required |
| `layerfs_backend_sandbox_get_log` | `sandbox:string` required, `lines:integer` 1..2000 default 200 |

P12 and P13 calls are **pending reconciliation at merge**. The MCP package
will not invent `project.list_summary` or scoped-client tool names in this
contract. After P12/P13 land, add their Node SDK operations and schemas here
only if present in the reconciled SDK surface.

### Refusal and protocol error mapping

- Zod/schema-invalid tool arguments are rejected by the MCP SDK before the
  Node SDK is invoked. SDK 2.4.0 returns an `isError:true` tool result whose
  text starts `Input validation error: Invalid arguments for tool ...` (the
  validation failure is an `InvalidParams` error internally, projected by the
  SDK onto the tool result).
- A `LayerFSError` becomes an MCP tool result with `isError: true` and a
  single JSON text content block `{ "code": <stable code>, "message": <message>,
  "detail": <structured detail or null> }`. Its stable code is unchanged.
  This includes every existing code in `@layerfs/agent` `CODES`: `Conflict`,
  `HeadMoved`, `Dirty`, `Busy`, `NoSuchPath`, `NoSuchBranch`, `NoSuchCommit`,
  `NoSuchProject`, `NoCommit`, `NoMergeBase`, `IsDirectory`, `NotDirectory`,
  `NotFile`, `Binary`, `NotText`, `NoMatch`, `AmbiguousMatch`, `Exists`,
  `NotEmpty`, `NameTaken`, `ReservedName`, `ReadOnly`, `MountsActive`,
  `TooLarge`, `InvalidInput`, `NotMounted`, `NoSuchSandbox`,
  `NoSuchProcess`, `Capacity`, and `NotComparable`.
- Non-refusal exceptions map to `{code:"Failed",message,detail:null}` with
  `isError:true`; stack traces are never returned to the caller.
- Push/pull are blocked by R7 at this base tip. They have no MCP tools, no
  simulated calls, and no success-shaped responses. If Node gains those
  operations before merge, reconcile this contract and preserve the blocker.

### Parity matrix at P5 base commit

Parity means a distinct MCP tool invokes the matching public SDK call and
returns the result/refusal described above; lifecycle-local operations map to
the client API. “Pending reconciliation” means no speculative tool is added.

| Node SDK operation | MCP tool / status |
| --- | --- |
| `LayerFS.init` / `LayerFS.open` / `client.close` | `layerfs_init` / `layerfs_open` / `layerfs_close` |
| `client.results` / `client.events.subscribe` | `layerfs_results` / `layerfs_events` |
| `project.list_branches`, `project.list_sandboxes` | respective `layerfs_project_*` tools |
| `fs.read`, `list`, `stat`, `grep`, `glob`, `diff_files`, `blame`, `write`, `edit`, `apply`, `mkdir`, `rm`, `cp`, `mv` | one `layerfs_fs_<method>` tool each |
| `branch.list_commits`, `graph`, `changed_paths`, `diff_commits`, `diff_branches`, `fork`, `merge`, `publish` | one `layerfs_branch_<method>` tool each |
| `sandbox.create`, `list_workspaces`, `close` | one `layerfs_sandbox_<method>` tool each |
| `workspace.mount`, `exec`, `commit`, `unmount` | one `layerfs_workspace_<method>` tool each |
| `LayerFS.call(method,args)` | Not separately exposed; every supported public operation above has a typed tool. Unsupported/unknown names retain SDK `UnknownMethod` behavior through no tool (MCP `InvalidParams`/unknown tool). |
| `backend.store.project.get_history`, `get_storage`, `get_comparable`, `list_overwritten` | one `layerfs_backend_<method>` tool each |
| `backend.store.branch.list_commit_storage` | `layerfs_backend_list_commit_storage` |
| `backend.store.fs.get_storage`, `list_revisions` | respective `layerfs_backend_fs_*` tools |
| `backend.daemon.workspace.status`, `list_changes`, `get_storage` | respective `layerfs_backend_workspace_*` tools |
| `backend.daemon.process.start`, `list`, `cancel` | respective `layerfs_backend_process_*` tools |
| `backend.daemon.sandbox.get_log` | `layerfs_backend_sandbox_get_log` |
| P12 project summary / P13 scoped client | **pending reconciliation at merge** |
| Push/pull | blocked by R7; absent from current SDK parity |
