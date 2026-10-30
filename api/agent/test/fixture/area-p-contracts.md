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
