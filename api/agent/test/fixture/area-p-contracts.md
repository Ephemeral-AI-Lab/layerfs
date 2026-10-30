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
