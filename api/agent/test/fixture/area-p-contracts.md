# Area P public contracts

Each slice writes its precise public contract here before implementation,
beside the shared P6 cases (`cases.mjs`, `cases-area-p.mjs`). The contract is
the test oracle: the JS cases and the Rust `area_p*` suites check these
shapes and refusals, not the implementation's own wording.

## P10 — per-call results (`client.results`)

**Surface**: `@layerfs/agent`. `client.results(after?)` on every client made
by `LayerFS.open` / `LayerFS.init`. Synchronous and local: it never dials the
binding, adds no binding call, and stays readable after `client.close()`.
The `layerfs` command has no results command (one process per command, like
the event stream). P4/P5 map the same shape onto Python and MCP.

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

