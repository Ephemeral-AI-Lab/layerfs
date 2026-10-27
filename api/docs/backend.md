# Using `@layerfs/backend`

> **Status:** Current implementation guide for `@layerfs/backend` in LayerFS
> 1.1.0; not a release receipt. Part of the
> [JavaScript packages wiki](../README.md).

`@layerfs/backend` reads what an agent's client cannot: a project's history and
storage, a Workspace's state, background processes and a Sandbox's daemon log.
UIs and tools use it to show a project; it changes no file. It wraps an open
[`@layerfs/agent`](agent.md) client and opens nothing of its own. The
[call trace](#the-call-trace), the other way to watch a run, is described here
too.

## Calls

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

## Comparing a Layer

`get_comparable(id)` takes a saved Commit or Layer id and answers the Commit
with the same files, so a Layer can be compared through `branch.diff_commits`.
A Layer whose listed Commit has its root answers `exact: true`; one published
from a Commit `get_history` does not list answers that Commit with
`exact: false`, and the caller checks the diff answer's root equals `root`.
Init's Layer, a publish that merged and an id `get_history` does not list are
refused `NotComparable`. What it learns is kept per client.

## A Sandbox's log

`get_log` reads the Sandbox's daemon log once through the Docker Engine where
the binding runs (the worker on macOS and Windows): the last `lines` lines
(1 to 2000, default 200), at most 64 KiB. A stopped Sandbox's log is readable
until its records are reclaimed. There is no `since`.

## Storage figures

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

## The call trace

`LAYERFS_TRACE=<file>`, as set when the client is
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

With `LAYERFS_TRACE_START=1`, each traced
call first writes a start line: the same `layerfs_trace`, `id`, `step`, `method`,
`args`, `start` and `scope` as its end line, with `phase: "start"` and no
outcome fields. A viewer can show a call while it runs; a start line with no
end line is a call that has not returned (or a process that ended first).
Any other value leaves start lines off.

Concurrent calls append whole lines; order follows completion, so join by `id`.
The trace is diagnostic timing, not a benchmark measurement. Off means no scope,
no observed exchange, no daemon timing and byte-identical replies.

Checks: `node api/agent/test/trace.mjs <case>` runs one case (`on`, `off`,
`failure`, `layerfs-env`, `start`, `ids`, `no-content`) over a stand-in
binding; `layerfs-env` checks that an integration's own prefixed names are not
settings. `node api/agent/test/verify/trace_real.mjs inner` traces a fork,
mount, exec, process, commit and merge on a fresh Disposable Store with the
`LAYERFS_TRACE` names set.
