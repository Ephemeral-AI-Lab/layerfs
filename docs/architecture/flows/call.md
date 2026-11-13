# A call, end to end

[Architecture](../README.md) › [Flows](README.md) › A call, end to end

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

From a JS, Python or CLI call to its result: the call table, control, routing, and the process the call runs in.

This page follows a Store call (`branch`, `commit` or `file` family), the
most common kind. Workspace calls take the same first steps and then go to a
Sandbox daemon; see [A Workspace session](workspace.md). Terms are in the
[glossary](../glossary.md).

## The path

```text
 caller process            machine worker            Store
 (CLI, Node, Python)       (sqlite on another        (SQLite file or
                            kernel only)              PostgreSQL server)
 ------------------------  ------------------------  ----------------
 front end -> call_scoped
 table::find, check::check
 scope.apply, home::open
 family fn -> store_call
 lease::claim  [Store lock]
 place::plan(StoreRef)
   |
   +-- Executor::Local  (postgresql; sqlite on a local platform)
   |   open::run -> acquire ----------------------------------> open
   |   service::call -> methods::call -----------------------> read or
   |                                                  [write: admission
   |                                                   turn, flock,
   |                                                   BEGIN IMMEDIATE]
   +-- Executor::Worker
   =====|== process boundary: TCP 127.0.0.1:<port> ==|
       {secret} ------------>  hello
       <------------ {ready, protocol, bundle}
       {op: store.call} ---->  lease under its root
                               open::run -> methods::call --> read or write
       <------------ {event}* then {value} | {refused}
 keys::snake -> Outcome
 CLI: exit code | Node/Python: {value} | {refused}
```

| Step | Component |
| --- | --- |
| 1 | [API: Front ends](../api/front-ends/README.md) |
| 2 | [API: Call table](../api/call-table/README.md) |
| 3 | [Control: Management](../control/management/README.md) |
| 4 | [Control: Routing and workers](../control/routing/README.md) |
| 5 | [Control: Network](../control/network/README.md) |
| 6 | [Engine: Store calls](../engine/store-calls/README.md) |
| 7 | [Engine: Store](../engine/store/README.md) |

## Steps

| # | What happens | Component / topic | Source |
| --- | --- | --- | --- |
| 1 | The front end makes one library call. The CLI parses `layerfs <family> <call>` against the table and calls `call_scoped`; Node and Python pass JSON text to `call_json` (Node in an `AsyncTask`, off the JavaScript thread). | [Front ends](../api/front-ends/README.md): [CLI](../api/front-ends/cli.md), [Node](../api/front-ends/node.md), [Python](../api/front-ends/python.md) | [`run.rs`](../../../api/crates/layerfs-cli/src/run.rs), [`node call.rs`](../../../api/crates/layerfs-node/src/call.rs), [`calls/text.rs`](../../../api/crates/layerfs-api/src/calls/text.rs) |
| 2 | `table::find` looks up `family.call` in the nine families (61 calls). | [Call table: table](../api/call-table/table.md) | [`calls/dispatch.rs`](../../../api/crates/layerfs-api/src/calls/dispatch.rs), [`lib.rs`](../../../api/crates/layerfs-api/src/lib.rs) |
| 3 | `check::check` checks every argument against the entry before any effect: known names, required positionals, kinds; sizes become bytes. | [Call table: checks](../api/call-table/table.md#checks) | [`calls/check.rs`](../../../api/crates/layerfs-api/src/calls/check.rs) |
| 4 | A folder scope, when the client has one, guards and rewrites the checked arguments (`Scope::apply`), then checks ownership after the home is open (`Scope::owns`). | [Call table: scopes](../api/call-table/table.md#scopes) | [`calls/scope.rs`](../../../api/crates/layerfs-api/src/calls/scope.rs) |
| 5 | The home is the explicit path, else `LAYERFS_HOME`, else `~/.layerfs`; it must be absolute. `home::open` reads its id; with the `machine` feature the built-in `native` and `docker` machine records are made once per process. | [Management: home and records](../control/management/home-and-records.md#home-layout) | [`calls/dispatch.rs`](../../../api/crates/layerfs-api/src/calls/dispatch.rs) |
| 6 | The family function translates the call into one Store method (`branch.fork`, `branch.diff_commits`, `fs.read`, …). | [Control: families](../control/management/families.md), [SDK: branch](../api/sdk/branch/README.md) | [`calls.rs`](../../../control/crates/layerfs-store/src/calls.rs) |
| 7 | `resolve::target` reads the Store and project records; `store_call` checks the record holds no blocking intent and takes this process's lease on the Store under the Store's lock. | [Routing table: by Store kind](../control/routing/routing-table.md#by-store-kind) | [`resolve.rs`](../../../control/crates/layerfs-store/src/resolve.rs), [`call.rs`](../../../control/crates/layerfs-store/src/call.rs), [`lease.rs`](../../../control/crates/layerfs-store/src/lease.rs) |
| 8 | `place::plan` decides where the call runs, once, from the record: a PostgreSQL Store runs here; a sqlite Store runs here when its machine's platform runs calls locally on this host, else in that machine's worker. | [Routing table](../control/routing/routing-table.md) | [`place.rs`](../../../control/crates/layerfs-store/src/place.rs) |
| 9a | Local: `open::run` keeps the Store open in this process (closed after 60 s unused) and `service::acquire` opens a `StoreService`; the kernel guard runs before any file is opened, and a writable open takes the Store directory's writer lane (Linux and macOS) so two openers queue instead of meeting SQLite's busy lock. `service::call` strips the routing fields and calls `methods::call` once. | [Store calls: service](../engine/store-calls/service.md#open-and-bind), [SDK facade: dispatch](../engine/store-calls/sdk-facade.md#dispatch) | [`open.rs`](../../../control/crates/layerfs-store/src/open.rs), [`service.rs`](../../../control/crates/layerfs-store/src/service.rs) |
| 9b | Worker: one new connection to the loopback address the machine's `worker.json` records. The secret goes first, alone; the worker answers with its protocol and bundle; a worker of another build is sent nothing. The request goes once. The worker holds its own lease, sets the Store path under its root and runs step 9a. | [Network: workers](../control/network/workers.md#one-request-per-connection), [Routing: workers](../control/routing/workers.md) | [`worker/client.rs`](../../../control/crates/layerfs-machine/src/worker/client.rs), [`worker_server.rs`](../../../control/crates/layerfs-control/src/worker_server.rs) |
| 10 | The engine runs the method. Reads use the published read session and never take the lane. A write waits for the session's admission turn, then the Linux `flock` on the Store's directory, then `BEGIN IMMEDIATE` (PostgreSQL: the `store_lane` row `FOR UPDATE`). A Commit holds the publish lane across its head check and publish. | [Store: one writer](../engine/store/backends.md#one-writer), [Store: the publish lane](../engine/store/history-publication.md#the-publish-lane) | [`methods/dispatch.rs`](../../../engine/crates/layerfs-store-service/src/methods/dispatch.rs), [`backend/sqlite/lane/lane.rs`](../../../engine/crates/layerfs-persistence/src/backend/sqlite/lane/lane.rs) |
| 11 | The answer's keys become snake_case. Events (progress, command output) went to the call's event stream as they came. The CLI closes the Stores the call opened before it exits and maps a refusal to exit code 125. | [Call table: keys](../api/call-table/table.md#keys) | [`calls/dispatch.rs`](../../../api/crates/layerfs-api/src/calls/dispatch.rs), [`run.rs`](../../../api/crates/layerfs-cli/src/run.rs) |

Every hop is one attempt. Nothing in the path retries, refreshes or replays a
request; waits for a turn, a lock or a worker's hello are readiness, not
retries.

### Telemetry

| Where | What is recorded | Source |
| --- | --- | --- |
| A traced thread (`trace::run` with a scope) | `service::call` collects the method's timing tree, SQL counts and delta work; a Sandbox control connection sends its requests as traced observations | [`trace.rs`](../../../control/crates/layerfs-home/src/trace.rs), [`service.rs`](../../../control/crates/layerfs-store/src/service.rs) |
| `LAYERFS_TRACE` set | One JSON line per worker connection: `op`, `worker`, `sent`, `ms` | [`trace.rs`](../../../control/crates/layerfs-home/src/trace.rs) `connection` |

No caller in `api/` calls `trace::run` at this source, so the per-call
collection is idle in the product path. Telemetry never changes a result.
Timing trees themselves are described in [Core: Telemetry](../core/telemetry/README.md).

## Failures along the way

| Step | What can fail | What the caller sees |
| --- | --- | --- |
| 1 | CLI words do not name a call or its arguments | exit 2, nothing called |
| 1 | JSON arguments or scope do not parse | `InvalidInput` |
| 2 | No such `family.call` | `InvalidInput` "no call named …" |
| 3 | Unknown argument, missing positional, wrong kind | `InvalidInput` "`<call>`: …" |
| 4 | Path or Branch outside the scope, or a call the scope does not allow | `InvalidInput` "out of scope: …" |
| 5 | Home not absolute, or `HOME` unset | `InvalidInput` |
| 7 | Store or project not recorded | `NoSuchStore`, `NoSuchProject` |
| 7 | The Store's record holds another operation's intent | `Busy` (owner alive) or `Unresolved` (owner gone, outcome unknown) |
| 8–9b | The machine's worker is not recorded, does not answer, refuses the secret | `Unavailable`, naming `layerfs machine start`; nothing was sent |
| 9b | The worker is of another build | refusal naming how to replace it; nothing was sent |
| 9b | Request larger than a frame | `TooLarge`; nothing was sent |
| 9b | Connection lost after the request was sent | `Unknown`: the call may have run; it is not resent |
| 9a | Kernel guard refuses the Store's filesystem; Store file missing; a writable open meets SQLite's busy lock before any effect (another process holds the Store outside the lane) | `Unsupported`; `NoStore`; `Busy` |
| 10 | SQLite busy (a writer that bypasses the directory lane), PostgreSQL `lock_timeout` | `Busy` |
| 10 | The engine cannot tell whether a write happened | `Unknown`; a file write keeps its private Workspace |
| 10 | Any other engine refusal | its stable code (`NoSuchBranch`, `HeadMoved`, `Conflict`, …), see [Call table: errors](../api/call-table/errors.md) |

## Demonstration

These commands were written from source and were not run while writing this
page.

| Test | What it sets up | What it checks | Command | Needs |
| --- | --- | --- | --- | --- |
| [`calls_table.rs`](../../../api/crates/layerfs-api/tests/calls_table.rs) | A temporary home | 61 calls in nine families; unknown calls and arguments refused before any effect; each family's function reached by a valid call; JSON round trips; sizes and keys; an explicit home must be absolute | `cargo +1.85.1 test --locked -p layerfs-api --test calls_table` | Unix; no Docker or root |
| [`store_routes.rs`](../../../api/crates/layerfs-api/tests/store_routes.rs) | Homes with sqlite and PostgreSQL Store records and an in-process worker on a loopback listener | A Store call goes where its records say; a worker gets the Store record with the call; a PostgreSQL password reference is resolved per call; leases are held, counted and stale ones removed | `cargo +1.85.1 test --locked -p layerfs-api --test store_routes` | Unix; one block is `#[cfg(not(feature = "machine"))]`, so it runs only without `--features machine` |

---

← [Flows](README.md) · [Up: Flows](README.md) · [Flow: Project Init](init.md) →
