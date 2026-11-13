# MCP

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source. The profile table is generated and checked.

`@layerfs/mcp`: a local stdio Model Context Protocol server over the Node package, one tool per call in the selected profile, adding no retry, replay, Commit, unmount or scope check of its own, within [Front ends](README.md).

| | |
| --- | --- |
| Source | [`api/mcp/src`](../../../../api/mcp/src): [`server.js`](../../../../api/mcp/src/server.js), [`exportroots.js`](../../../../api/mcp/src/exportroots.js), [`profiles.js`](../../../../api/mcp/src/profiles.js), [`tools.js`](../../../../api/mcp/src/tools.js), [`values.js`](../../../../api/mcp/src/values.js), [`errors.js`](../../../../api/mcp/src/errors.js), [`progress.js`](../../../../api/mcp/src/progress.js); [`gen-profiles.mjs`](../../../../api/mcp/scripts/gen-profiles.mjs) |

The package is private and unpublished (`api/mcp`, version 2.0.0), Node 22 or
later, over the [Node binding](bindings.md#layerfs-node) and
[`@modelcontextprotocol/sdk`](https://www.npmjs.com/package/@modelcontextprotocol/sdk)
1.32.2 (MIT). The [call table](../calls/README.md) is the contract; the
server's one check of its own is the [export allowlist](#export-allowlist).

```text
 MCP client --stdio JSON-RPC--> server.js (profile, home, scope fixed)
                                   | tools/list: tools(table, profile)
                                   | tools/call: schema check (tools.js)
                                   |   project_export: export roots
                                   v
                          layerfs (Node package), one call, once
                                   |
             result text | isError {code,message,detail} | progress notes
```

## Launch

Build the Node addon first ([bindings](bindings.md#layerfs-node)), install
this package once, then point a stdio client at the entry file. No `npx`, no
registry, no network:

```bash
cd api/mcp
npm install --no-audit --no-fund   # links ../node, installs the SDK
node src/server.js --help
```

```json
{
  "mcpServers": {
    "layerfs": {
      "command": "node",
      "args": ["/opt/layerfs/api/mcp/src/server.js",
               "--home", "/data/layerfs", "--profile", "work"],
      "env": { "LAYERFS_CLI": "/opt/layerfs/target/debug/layerfs" }
    }
  }
}
```

| Flag | Meaning |
| --- | --- |
| `--profile work` | the default; the `work` calls, below |
| `--profile admin` | the `work` calls plus every administrative call |
| `--home <dir>` | the LayerFS home; default `LAYERFS_HOME`, else `~/.layerfs` |
| `--scope-project <store>/<project>` | the folder scope's project; must be given with `--scope-folder` |
| `--scope-folder <dir>` | the folder scope inside the project; the library checks every scoped call |
| `--scope-branch <name>` | the scope's Branch namespace; needs both scope flags |
| `--export-root <dir>` | a directory `project_export` may write under; repeatable. An absolute path to an existing directory, not `/`; anything else is a usage error (exit 2) |
| `--help`, `--version` | the usage line, or the package version, on stdout, then exit 0 |

The scope flags are the command's [`--scope-project` and `--scope`](README.md#the-layerfs-command)
under this server's names. The scope is fixed for the process; a scoped call
the library refuses comes back as its own `InvalidInput` (`out of scope: …`)
([`scope.rs`](../../../../api/crates/layerfs-api/src/calls/scope.rs)).

## Export allowlist

`project_export` writes to a path on the machine running this server, and the
client picks the path, so the operator names where it may write at launch
([`exportroots.js`](../../../../api/mcp/src/exportroots.js)).

| Destination | Result |
| --- | --- |
| no `--export-root` given | refused, `InvalidInput`, naming `--export-root` |
| a root itself, or beneath one, after resolving the parent through symlinks | passed to the library |
| a link inside a root that leads out; a sibling sharing a root's prefix (`<root>-x`) | refused |
| a relative path, a `..` path, a missing parent, a trailing separator, a NUL byte | refused |

A refusal names the allowed roots, is a tool result with `isError`, and comes
before the library is called, so nothing is created or uncertain. The
library's own destination rules and the folder scope still apply after it.
The check is made once, before the call; it does not defend against a local
process rewriting directories under a root while an export runs. The list
covers `project_export` only: the other host-naming tools are `admin`
(`project_init` reads a host folder; `machine_update` names a daemon file and
a Docker socket), and `workspace_exec` (in `work`) runs commands in a
Workspace, confined by its Sandbox backend, not by this list.

## Profiles

Each call belongs to exactly one bucket, `work` (listed by both profiles) or
`admin` (listed only by `admin`); none is excluded
([`profiles.js`](../../../../api/mcp/src/profiles.js)):

| Bucket | Calls |
| --- | --- |
| `work` | the `branch`, `commit`, `file` and `workspace` families; `project.list`, `project.get`, `project.export`, `sandbox.list`, `sandbox.get` |
| `admin` | `project.init`, `project.remove`, `sandbox.create`, `sandbox.close`; the `machine`, `store`, `backup` and `logs` families |

`work` lets an agent read a project, work on Branches and files (each write
is one explicit Commit), run in a mounted Workspace and export a snapshot.
`admin` adds the calls that start workers, move whole Stores, hold
credentials, delete records or change the shape of the home. A new call with
no bucket fails `tools()`, generation and the test rather than appearing
silently. Regenerate the table below with
`node api/mcp/scripts/gen-profiles.mjs` (`--check` compares; both need the
built addon).

<!-- BEGIN GENERATED: api/mcp/scripts/gen-profiles.mjs -->

61 calls: 34 in `work`, 27 in `admin` only; `--profile admin` lists all 61.

| Call | Bucket | Profiles that list it | Why |
| --- | --- | --- | --- |
| `project.init` | admin | admin | import a host folder into a Store; administration |
| `project.list` | work | work, admin | read a project's record or export one (work); importing and removing a project are administration |
| `project.get` | work | work, admin | read a project's record or export one (work); importing and removing a project are administration |
| `project.export` | work | work, admin | export a Branch snapshot to a local folder under a launch-time --export-root; a folder scope refuses it |
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

One tool per call, named `<family>_<call>` (`store_create`,
`workspace_exec_cancel`, …), generated from `layerfs.table`
([`tools.js`](../../../../api/mcp/src/tools.js)). Each tool's JSON Schema is
the entry's arguments, in order, then its options, minus the functions
(`on_progress`, `stdout`, `stderr`); `additionalProperties` is false. The
`stream` option of `workspace.exec` is not exposed: it would write the
command's output to this process's stdout, which carries the protocol.

| Kind | Accepted | Notes |
| --- | --- | --- |
| `Project` | string | `<store>/<project>` |
| `Name`, `Text` | string | |
| `Int` | integer, or decimal string | a string keeps a whole number beyond ±(2^53−1) exact; a JSON number outside it is refused, not rounded |
| `Size` | integer, or string | `512M`, `8G` |
| `Bool` | boolean | |
| `Content` | string, or `{ "base64": "..." }` | bytes on the wire |
| `Json` | any JSON value | nested `{ "base64": … }` content is read as bytes by the library |

Results are the answer as one JSON text block: `Buffer`s become
`{ "base64": "..." }`, and a whole number outside ±(2^53−1) becomes
`{ "$layerfs": "int", "value": "9223372036854775807" }`; a rounded number is
never returned ([`values.js`](../../../../api/mcp/src/values.js)). An argument
that does not fit its schema and an unknown tool are JSON-RPC protocol errors
(`-32602`), not tool results; a non-object `arguments` is refused earlier by
the SDK's request schema (`-32603`). The call does not run.

## Semantics

| Rule | Behaviour | Source |
| --- | --- | --- |
| Result | the call's answer as JSON, exactly once | [`server.js`](../../../../api/mcp/src/server.js) |
| Refusal | `isError: true` and a text block `{ "code", "message", "detail" }`, the library's own | [`errors.js`](../../../../api/mcp/src/errors.js) |
| Uncertain outcome | `Unknown`, `Unresolved`, or `Failed` with `attempted: true`, adds `outcome uncertain: read state before acting again; nothing was retried.` (`UNCERTAIN`) | [`errors.js`](../../../../api/mcp/src/errors.js) |
| Retry, replay, Commit, unmount | none of its own: one library call per tool call; Commit and unmount only through `workspace_commit` and `workspace_unmount`, never on an error, disconnect or shutdown | [`server.js`](../../../../api/mcp/src/server.js) |
| Progress | with a progress token, `on_progress` events become `notifications/progress`: the first at once, then at most one per 100 ms (`INTERVAL_MS`) carrying the latest, and the last before the result, which waits 10 ms so the note arrives first. `progress` counts 1, 2, …; `total` only when positive; `message` the event as JSON, cut at 512 characters. Without a token, no handler is passed | [`progress.js`](../../../../api/mcp/src/progress.js) |
| `workspace.exec` | output is buffered: inline text up to 1 MiB per stream in the result; a larger stream is not delivered (no `stdout`/`stderr` function), and `stdout_bytes`, `stderr_bytes`, `streamed: true` report it | [Errors and events](../calls/errors-and-events.md#workspaceexec-output) |
| Cancel notification | acknowledged on stderr; the native call cannot be cancelled and runs to its end | [`server.js`](../../../../api/mcp/src/server.js) |
| stdin close, `SIGTERM`, `SIGINT` | stop accepting calls, wait up to 2 s (`GRACE_MS`) for in-flight ones, run `layerfs.close()` (idle Stores only), exit 0. A Workspace stays mounted; a call still running is cut off and its outcome is unknown | [`server.js`](../../../../api/mcp/src/server.js) |
| stdout, stderr | stdout carries one JSON-RPC message per line and nothing else while serving; one `layerfs-mcp: …` line per call, startup and shutdown on stderr. Argument values are never logged (a `store.connect` password, a `backup.connect` key) | [`server.js`](../../../../api/mcp/src/server.js) |

## Limits

- stdio only: no HTTP/SSE transport, no authentication (it runs as the user
  who launched it), no resources, prompts or sampling.
- One home and one fixed scope per process; no `init`/`open`/`close` tool.
- Calls run on Node's default thread pool (`UV_THREADPOOL_SIZE` 4): many long
  concurrent calls can starve each other. The server does not raise it.
- No batching or automatic paging: `page_size`, `page_pos` and `cursor` pass
  through, and `next_page_pos` comes back as the call gives it.

## Tests

From the repository root, each under a 120 s wall limit, with `TMPDIR` on
ext4, xfs or btrfs, the built addon (`api/node/layerfs.node`) and
`LAYERFS_CLI`. The tests run a real `node src/server.js` with the SDK's
`Client` over `StdioClientTransport` and a Disposable temp home.

| Test | Command | Shows |
| --- | --- | --- |
| package | `cd api/mcp && npm install --no-audit --no-fund && npm test` | the files below, one at a time |
| [`workflow.test.mjs`](../../../../api/mcp/test/workflow.test.mjs) | `node --test api/mcp/test/workflow.test.mjs` | initialize, `tools/list` counts, a Store/project/Branch/file history flow, export with progress, refusals, scope, the work profile |
| [`purity.test.mjs`](../../../../api/mcp/test/purity.test.mjs) | `node --test api/mcp/test/purity.test.mjs` | stdout is JSON-RPC only while a call logs to stderr |
| [`values.test.mjs`](../../../../api/mcp/test/values.test.mjs) | `node --test api/mcp/test/values.test.mjs` | bytes round-trip, the integer wrapper |
| [`concurrency.test.mjs`](../../../../api/mcp/test/concurrency.test.mjs) | `node --test api/mcp/test/concurrency.test.mjs` | 10 concurrent calls; stdin close and SIGTERM exit 0 within 5 s, no process left |
| [`export-roots.test.mjs`](../../../../api/mcp/test/export-roots.test.mjs) | `node --test api/mcp/test/export-roots.test.mjs` | the allowlist cases above |
| [`profiles.test.mjs`](../../../../api/mcp/test/profiles.test.mjs) | `node --test api/mcp/test/profiles.test.mjs` | every call in exactly one bucket; the generated table equals this page; every schema valid |
| coverage | `node api/scripts/gen-coverage.mjs --check` | the MCP column of the [coverage matrix](../calls/coverage.md) matches the same tool list |

## Related

- [Front ends](README.md); [bindings](bindings.md) (the Node package it calls).
- [Calls](../calls/README.md), [errors and events](../calls/errors-and-events.md).
