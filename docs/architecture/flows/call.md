# A call, end to end

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

From a JS, Python or CLI call to its result: the call table, control, and the process the call runs in.

This page follows a Store call (`branch`, `commit` or `file` family), the
most common kind. Workspace calls take the same first steps and then go to a
Sandbox daemon; see [A Workspace session](workspace.md). Where every call
runs is the single table in
[Control: where each call runs](../control/README.md#where-each-call-runs).

## The path

```text
 caller process            machine worker            Store
 (CLI, Node, Python)       (sqlite on another        (SQLite file or
                            kernel only)              PostgreSQL schema)
 ------------------------  ------------------------  ----------------
 front end -> call table
 check args, folder scope
 open home; family fn
 Store record; lease
 plan: where it runs
   |
   +-- local (postgresql; sqlite on a local machine)
   |   open Store service --------------------------------> open
   |   method, once ----------------------------------------> read, or
   |                                                  write: admission
   |                                                  turn, writer lock
   +-- worker
   =====|== process boundary: TCP 127.0.0.1:<port> ==|
       secret ------------->  hello
       <-------------- protocol, bundle
       store.call (once) -->  lease under its root
                              open, method --------------> read or write
       <-------------- events, then value | refusal
 snake_case keys -> Outcome
 CLI: exit code | Node/Python: {value} | {refused}
```

Every hop is one attempt. Nothing in the path retries, refreshes or replays a
request; waits for a turn, a lock or a worker's hello are readiness, not
retries. A lost answer after the request was sent is `Unknown`, never resent.

## Hops

| # | What happens | Owning topic |
| --- | --- | --- |
| 1 | The front end makes one library call: the CLI parses `layerfs <family> <call>`; Node and Python pass JSON text. | [Front ends](../api/front-ends/README.md), [Bindings](../api/front-ends/bindings.md) |
| 2 | The call table finds `family.call` (73 calls in 10 families) and checks every argument before any effect. | [Calls](../api/calls/README.md#what-it-does), [SDK](../api/sdk/README.md) |
| 3 | A folder scope, when the client has one, guards and rewrites the checked arguments. | [Calls: folder scope](../api/calls/README.md#folder-scope) |
| 4 | The home is opened (explicit path, else `LAYERFS_HOME`, else `~/.layerfs`). | [Home](../control/home/README.md) |
| 5 | The family function reads the Store and project records, refuses a blocking intent and takes this process's lease on the Store. | [Stores and projects](../control/stores-and-projects/README.md#what-it-does), [Home: the `op` intent](../control/home/README.md#the-op-intent) |
| 6 | Where the call runs is decided once from the record: the caller's process, or the Store machine's worker. | [Control: where each call runs](../control/README.md#where-each-call-runs) |
| 7a | Local: the process keeps the Store open and runs the method once through the Store service; the kernel guard runs before any file is opened, and a writable open takes the Store directory's writer lane (Linux and macOS), so two openers queue instead of meeting SQLite's busy lock. | [Store calls: opening the service](../engine/store-calls/README.md#opening-the-service), [Store calls: method table](../engine/store-calls/README.md#method-table) |
| 7b | Worker: one loopback connection, the secret first, then one request; the worker holds its own lease and runs step 7a. | [Workers and reach](../control/machines/workers-and-reach.md), [Bridge](../engine/bridge/README.md) |
| 8 | Reads use the published read session and never take the lane; a write takes the session's admission turn and the Store's writer (a directory `flock` on Linux and macOS, or PostgreSQL's `store_lane` row). | [Store calls: read sessions](../engine/store-calls/README.md#read-sessions), [Store: invariants](../engine/store/README.md#invariants) |
| 9 | The answer's keys become snake_case; events went out as they came; a refusal carries its stable code. | [API: calling conventions](../api/README.md#calling-conventions), [Errors and events](../api/calls/errors-and-events.md) |

## Related

- [Calls](../api/calls/README.md), [SDK](../api/sdk/README.md)
- [Control: where each call runs](../control/README.md#where-each-call-runs)
- [Store calls](../engine/store-calls/README.md), [Store](../engine/store/README.md)
- [Core: Telemetry](../core/telemetry/README.md): the timing tree a traced call records.
