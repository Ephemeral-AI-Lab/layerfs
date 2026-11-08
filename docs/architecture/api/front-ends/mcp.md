# MCP

[Architecture](../../README.md) › [API](../README.md) › Front ends › MCP

> **Status:** Current architecture guide for LayerFS 2.0.0; the tool and profile table is generated and checked.

`@layerfs/mcp`: a local stdio Model Context Protocol server over the Node package, one tool per call in the
selected profile. It is a thin adapter, not a fourth call surface: the [table](../call-table/table.md) is the
contract, and this server adds no retry, replay, Commit, unmount or scope check of its own.

| | |
| --- | --- |
| Layer | [API](../README.md) |
| Package | `api/mcp` (`@layerfs/mcp`, **private**) |
| Source | [`src/server.js`](../../../../api/mcp/src/server.js), [`src/tools.js`](../../../../api/mcp/src/tools.js), [`src/values.js`](../../../../api/mcp/src/values.js), [`src/profiles.js`](../../../../api/mcp/src/profiles.js), [`src/errors.js`](../../../../api/mcp/src/errors.js), [`src/progress.js`](../../../../api/mcp/src/progress.js) |
| Uses | [Node](node.md), [`layerfs`](../../../../api/node), [Call table](../call-table/table.md), [`@modelcontextprotocol/sdk`](https://www.npmjs.com/package/@modelcontextprotocol/sdk) 1.32.2 (MIT) |
| Used by | MCP clients that launch a local stdio server |

**MCP history.** O32 dropped MCP from 2.0; it is reinstated by the assignee's instruction of 2026-11-07, to be
confirmed by the maintainers. The old #521 server is not reused: it had a single-project session, hand-written
backend tools and an automatic close/unmount on shutdown. This package replaces all three.

## Launch

Build the Node addon first ([Node](node.md#native-addon-build)), install this package once, then point a stdio
client at the entry file. No `npx`, no registry, no network:

```bash
cd api/mcp
npm install --no-audit --no-fund    # links layerfs from ../node and installs the SDK
node src/server.js --help           # the entry is src/server.js; api/node/layerfs.node must be built
```

A stdio client configuration with local paths (the environment is the server's, not the client's):

```json
{
  "mcpServers": {
    "layerfs": {
      "command": "node",
      "args": [
        "/opt/layerfs/api/mcp/src/server.js",
        "--home", "/data/layerfs",
        "--profile", "work"
      ],
      "env": { "LAYERFS_CLI": "/opt/layerfs/target/debug/layerfs" }
    }
  }
}
```

The same server from a shell, which also runs the tests:

```bash
node api/mcp/src/server.js --home /data/layerfs --profile work
node api/mcp/src/server.js --home /data/layerfs --profile admin
node api/mcp/src/server.js --help
```

| Flag | Meaning |
| --- | --- |
| `--profile work` | the default; the `work` calls, below |
| `--profile admin` | the `work` calls plus every administrative call: project create/remove, Sandbox create/close, machine, Store and backup |
| `--home <dir>` | the LayerFS home; default `LAYERFS_HOME`, else `~/.layerfs` |
| `--scope-project <store>/<project>` | the folder scope's project; must be given with `--scope-folder` |
| `--scope-folder <dir>` | the folder scope inside the project; the library checks every scoped call |
| `--scope-branch <name>` | the scope's Branch namespace; needs both scope flags |
| `--help`, `--version` | the usage line, or the package version, on stdout, then exit 0 |

`--scope-project` and `--scope-folder` are the command's [`--scope-project` and `--scope`](cli.md#parsing),
under this server's names. The scope is fixed for the process: a tool call cannot change it, and the adapter
does not reimplement the check ([`scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs)). A scoped
call the library refuses comes back as the library's own `InvalidInput` (`out of scope: …`).

## Profiles

A call belongs to exactly one bucket: `work` (listed by both profiles) or `admin` (listed only by `admin`). No
call is excluded. The rule is in [`src/profiles.js`](../../../../api/mcp/src/profiles.js): the `branch`,
`commit`, `file` and `workspace` families plus read-only `project` and `sandbox` calls are `work`; project
create/remove, Sandbox create/close, and the `machine`, `store` and `backup` families are `admin`. The admin
profile lists the union, so it can do the work calls too. A new call with no bucket fails generation and the
test, rather than appearing silently.

Why: `work` lets an agent read a project, work on Branches and files, run in a mounted Workspace and export a
snapshot, without the calls that create or destroy authoritative state. `machine`, `store` and `backup` calls
start workers, move whole Stores, hold credentials and delete records; `project.init/remove` and
`sandbox.create/close` change the shape of the home. Those need an operator's explicit `--profile admin`.

<!-- BEGIN GENERATED: api/mcp/scripts/gen-profiles.mjs -->

61 calls: 34 in `work`, 27 in `admin` only; `--profile admin` lists all 61.

| Call | Bucket | Profiles that list it | Why |
| --- | --- | --- | --- |
| `project.init` | admin | admin | import a host folder into a Store; administration |
| `project.list` | work | work, admin | read a project's record or export one (work); importing and removing a project are administration |
| `project.get` | work | work, admin | read a project's record or export one (work); importing and removing a project are administration |
| `project.export` | work | work, admin | export a Branch snapshot to a local folder; a folder scope refuses it |
| `project.remove` | admin | admin | remove a project from its Store; administration |
| `branch.list` | work | work, admin | work on Branches: list, read, fork, merge, publish and diff |
| `branch.get` | work | work, admin | work on Branches: list, read, fork, merge, publish and diff |
| `branch.fork` | work | work, admin | work on Branches: list, read, fork, merge, publish and diff |
| `branch.merge` | work | work, admin | work on Branches: list, read, fork, merge, publish and diff |
| `branch.publish` | work | work, admin | work on Branches: list, read, fork, merge, publish and diff |
| `branch.diff` | work | work, admin | work on Branches: list, read, fork, merge, publish and diff |
| `commit.list` | work | work, admin | read a Branch's Commit history |
| `commit.get` | work | work, admin | read a Branch's Commit history |
| `commit.diff` | work | work, admin | read a Branch's Commit history |
| `file.read` | work | work, admin | read and change files in a Branch; each write is one explicit Commit |
| `file.stat` | work | work, admin | read and change files in a Branch; each write is one explicit Commit |
| `file.list` | work | work, admin | read and change files in a Branch; each write is one explicit Commit |
| `file.glob` | work | work, admin | read and change files in a Branch; each write is one explicit Commit |
| `file.grep` | work | work, admin | read and change files in a Branch; each write is one explicit Commit |
| `file.blame` | work | work, admin | read and change files in a Branch; each write is one explicit Commit |
| `file.diff` | work | work, admin | read and change files in a Branch; each write is one explicit Commit |
| `file.write` | work | work, admin | read and change files in a Branch; each write is one explicit Commit |
| `file.edit` | work | work, admin | read and change files in a Branch; each write is one explicit Commit |
| `file.rm` | work | work, admin | read and change files in a Branch; each write is one explicit Commit |
| `file.mkdir` | work | work, admin | read and change files in a Branch; each write is one explicit Commit |
| `file.mv` | work | work, admin | read and change files in a Branch; each write is one explicit Commit |
| `file.cp` | work | work, admin | read and change files in a Branch; each write is one explicit Commit |
| `file.apply` | work | work, admin | read and change files in a Branch; each write is one explicit Commit |
| `sandbox.create` | admin | admin | start a Sandbox process; administration |
| `sandbox.list` | work | work, admin | read a Sandbox and its Workspaces (work); starting and closing one is administration |
| `sandbox.get` | work | work, admin | read a Sandbox and its Workspaces (work); starting and closing one is administration |
| `sandbox.close` | admin | admin | stop a Sandbox; administration |
| `workspace.mount` | work | work, admin | mount, run in, commit and unmount a Workspace explicitly; nothing is implicit |
| `workspace.exec` | work | work, admin | mount, run in, commit and unmount a Workspace explicitly; nothing is implicit |
| `workspace.exec_cancel` | work | work, admin | mount, run in, commit and unmount a Workspace explicitly; nothing is implicit |
| `workspace.commit` | work | work, admin | mount, run in, commit and unmount a Workspace explicitly; nothing is implicit |
| `workspace.unmount` | work | work, admin | mount, run in, commit and unmount a Workspace explicitly; nothing is implicit |
| `workspace.status` | work | work, admin | mount, run in, commit and unmount a Workspace explicitly; nothing is implicit |
| `machine.list` | admin | admin | machines hold Stores and workers; creating, changing and removing them is administration |
| `machine.platforms` | admin | admin | machines hold Stores and workers; creating, changing and removing them is administration |
| `machine.get` | admin | admin | machines hold Stores and workers; creating, changing and removing them is administration |
| `machine.status` | admin | admin | machines hold Stores and workers; creating, changing and removing them is administration |
| `machine.start` | admin | admin | machines hold Stores and workers; creating, changing and removing them is administration |
| `machine.stop` | admin | admin | machines hold Stores and workers; creating, changing and removing them is administration |
| `machine.create` | admin | admin | machines hold Stores and workers; creating, changing and removing them is administration |
| `machine.remove` | admin | admin | machines hold Stores and workers; creating, changing and removing them is administration |
| `machine.update` | admin | admin | machines hold Stores and workers; creating, changing and removing them is administration |
| `store.list` | admin | admin | create, connect, transfer and remove Stores; administration |
| `store.get` | admin | admin | create, connect, transfer and remove Stores; administration |
| `store.status` | admin | admin | create, connect, transfer and remove Stores; administration |
| `store.create` | admin | admin | create, connect, transfer and remove Stores; administration |
| `store.connect` | admin | admin | create, connect, transfer and remove Stores; administration |
| `store.push` | admin | admin | create, connect, transfer and remove Stores; administration |
| `store.pull` | admin | admin | create, connect, transfer and remove Stores; administration |
| `store.remove` | admin | admin | create, connect, transfer and remove Stores; administration |
| `store.disconnect` | admin | admin | create, connect, transfer and remove Stores; administration |
| `backup.connect` | admin | admin | upload and restore a Store on a bucket; administration |
| `backup.disconnect` | admin | admin | upload and restore a Store on a bucket; administration |
| `backup.list` | admin | admin | upload and restore a Store on a bucket; administration |
| `backup.create` | admin | admin | upload and restore a Store on a bucket; administration |
| `backup.restore` | admin | admin | upload and restore a Store on a bucket; administration |

<!-- END GENERATED -->

## Tools and schemas

One tool per call, named `<family>_<call>` (`store_create`, `workspace_exec_cancel`, …), generated from
`layerfs.table`. Each tool's JSON Schema is the table entry's arguments, in order, then its options, minus the
functions (`on_progress`, `stdout`, `stderr`); `additionalProperties` is false, and a name the entry does not
have is a protocol error. The `stream` option of `workspace.exec` is deliberately not exposed: `stream: true`
writes the command's output to this process's stdout, which carries the protocol.

| Kind | Accepted | Notes |
| --- | --- | --- |
| `Project` | string | `<store>/<project>` |
| `Name`, `Text` | string | |
| `Int` | integer, or decimal string | a string keeps a whole number beyond ±(2^53−1) exact; a JSON number outside it is refused, not rounded |
| `Size` | integer, or string | `512M`, `8G` |
| `Bool` | boolean | |
| `Content` | string, or `{ "base64": "..." }` | bytes on the wire |
| `Json` | any JSON value | nested `{ "base64": … }` content is read as bytes by the library |

Results are the call's answer as one JSON text block: `Buffer`s become `{ "base64": "..." }`, and a whole
number outside ±(2^53−1) becomes an explicit wrapper, `{ "$layerfs": "int", "value": "9223372036854775807" }`.
A rounded JSON number is never returned. An argument that does not fit its schema (unknown name, missing
required name, wrong kind, a non-safe JSON number where a whole number is expected) and an unknown tool are
JSON-RPC protocol errors (`-32602`), not tool results; the call does not run.

## Semantics

| Rule | Behaviour |
| --- | --- |
| Result | the call's answer as JSON, exactly once |
| Refusal | `isError: true` and a text block with `{ "code", "message", "detail" }`; `detail` is the library's |
| Uncertain outcome | `Unknown`, or `Failed` carrying `attempted: true`, adds the text `outcome uncertain: read state before acting again; nothing was retried.` Read `store status`, `machine status`, `sandbox get` or the Workspace's state before acting again |
| Retry / replay | none: every tool call is one library call, made once |
| Commit / unmount | only the explicit `workspace_commit` and `workspace_unmount` tools; neither is called on an error, a disconnect or shutdown |
| Disconnect or SIGTERM | the client is closed, its idle Stores are released and the process exits 0; a mounted Workspace stays mounted and an in-flight native call runs to its end (it cannot be cancelled), then the process ends |
| Cancel notification | acknowledged and logged; the native call is not cancellable and runs to its end; nothing is retried |
| Progress | when the request has a progress token, the call's `on_progress` events become `notifications/progress`: the first event at once, then at most one every 100 ms carrying the latest, and the last event before the result. `progress` is a strictly increasing counter (1, 2, …), `total` only when the event has a positive total, `message` the coalesced event as JSON (cut at 512 characters). Without a token, no handler is passed and no notification is sent |
| `workspace.exec` | the Node binding's current behaviour: output is buffered and arrives after the command ends; inline text up to 1 MiB per stream is in the result, a larger stream is not delivered and `stdout_bytes`, `stderr_bytes` and `streamed: true` report it. There is no live streaming |
| `project.export` | `destination` is a path on the machine running this MCP server, not the client's; under a folder scope the library refuses the call |
| Scope | the client's `scope` option, fixed at launch; restricted calls return the library's refusal |
| stdout / stderr | stdout carries the protocol only; one log line per call, startup and shutdown notes go to stderr |

## Process hygiene

- While serving, one line-delimited JSON-RPC message per line on stdout; nothing else, ever. `--help` and
  `--version` are the only modes that write to stdout without serving, then exit 0.
- Every log line is `layerfs-mcp: …` on stderr. Argument values are not logged (a `store.connect` password or a
  `backup.connect` key must not reach a log).
- Closing stdin, `SIGTERM` or `SIGINT` stops accepting calls, waits up to 2 s for in-flight calls, runs
  `layerfs.close()` (idle Stores only) and exits 0. A call still running then ends with the process.
- An in-flight native call cannot be cancelled in any binding; a `notifications/cancelled` is acknowledged on
  stderr and the call runs to its end.

## Limits

- stdio only. No HTTP/SSE transport, no authentication (the server runs as the user who launched it), no
  resources, prompts or sampling.
- One home and one fixed scope per process. There is no `init`/`open`/`close` tool: every call names its
  `<store>/<project>`, and the home is a launch flag.
- The package is private and unpublished; it needs the built `layerfs.node` addon and Node 22 or later.
- `UV_THREADPOOL_SIZE` is Node's default (4), and the Node package runs calls on that pool: many long,
  concurrent calls can starve each other. The server does not raise it.
- A call that sends a progress notification yields 10 ms before its result: the SDK client dispatches notifications in a microtask but responses at once, so this keeps the notification ahead of the response.
- A tool call is one call: no batching, no automatic paging. `page_size`, `page_pos` and `cursor` are passed
  through, and `next_page_pos` is returned as the call gives it.

## Verification

Commands from the repository root, each with an external wall limit of at most 120 s and `TMPDIR` on ext4:

| Test | Command | Shows |
| --- | --- | --- |
| package | `cd api/mcp && npm install --no-audit --no-fund && npm test` | the tests below, one file at a time |
| workflow | `node --test api/mcp/test/workflow.test.mjs` | initialize, `tools/list` counts, a real Store/project/Branch/file history flow, export with progress, refusals, scope, the work profile |
| purity | `node --test api/mcp/test/purity.test.mjs` | stdout is JSON-RPC only while a call logs to stderr |
| values | `node --test api/mcp/test/values.test.mjs` | bytes round-trip, the integer boundary wrapper |
| concurrency and exit | `node --test api/mcp/test/concurrency.test.mjs` | 10 concurrent calls; stdin close and SIGTERM exit 0 within 5 s with no process left |
| profiles and docs | `node --test api/mcp/test/profiles.test.mjs` | every call in exactly one bucket; the generated table equals the page; every schema valid |
| coverage | `node api/scripts/gen-coverage.mjs --check` | the MCP column matches the same generated tool list |

Tests run against a real `node src/server.js`, a real SDK `Client` and `StdioClientTransport`, a Disposable
temp home, the built addon (`api/node/layerfs.node`) and `LAYERFS_CLI`.

## NOT RUN

- macOS and Windows hosts; only Linux (WSL Ubuntu) was run.
- Third-party MCP clients (a desktop app, an IDE); only the SDK's `Client` over `StdioClientTransport` was run.
- HTTP/SSE transports, authentication and any published artifact: out of scope.
- A Sandbox, Workspace mount and `workspace.exec`: these need a daemon and a machine; this package's tests do
  not start one. `workspace` and `sandbox` tools are listed and refused by the library as it would refuse them.
- `store.push`/`pull`, PostgreSQL Stores, backups and machines: they need a second machine, a server or a
  bucket. Not run; the tools are generated from the table and the tests do not call them.
- Cancellation of an in-flight native call: documented as impossible; not run.
- A long-running call cut off by stdin close: the 2 s bounded wait is in the code; only the no-in-flight case
  was measured.

## Earlier material

- [O32 and the 2.0 decisions](../../../../core/docs/issues/533/DECISIONS.md) (historical; MCP is reinstated by
  the instruction above, not by an edit there).
- [The #521 MCP review](../../../../api/mcp) is not retained: hand-written tools, a single-project session and
  automatic close/unmount are gone.

---

← [Front ends](README.md) · [Up: API](../README.md) · [Node](node.md) ›
