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
