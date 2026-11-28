# MCP

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source. The profile table is generated and checked.

`@layerfs/mcp`: a local stdio Model Context Protocol server over the Node package, one tool per call in the selected profile, adding no retry, replay, Commit, unmount or scope check of its own, within [Front ends](README.md).

| | |
| --- | --- |
| Source | [`api/mcp/src`](../../../../api/mcp/src): [`server.js`](../../../../api/mcp/src/server.js), [`export-roots.js`](../../../../api/mcp/src/export-roots.js), [`profiles.js`](../../../../api/mcp/src/profiles.js), [`tools.js`](../../../../api/mcp/src/tools.js), [`values.js`](../../../../api/mcp/src/values.js), [`errors.js`](../../../../api/mcp/src/errors.js), [`progress.js`](../../../../api/mcp/src/progress.js); [`mcp-profiles.mjs`](../../../../api/scripts/mcp-profiles.mjs) |

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
             result text | isError {error,message,fix,...} | progress notes
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
| `--home <dir>` | the LayerFS home; default `LAYERFS_HOME`, else `~/.layerfs/home` (or a legacy `~/.layerfs` that holds `layerfs.json`) |
| `--scope-project <store>/<project>` | the folder scope's project; must be given with `--scope-folder` |
| `--scope-folder <dir>` | the folder scope inside the project; the library checks every scoped call |
| `--scope-branch <name>` | the scope's Branch namespace; needs both scope flags |
| `--export-root <dir>` | a directory `project_export` may write under; repeatable. An absolute path to an existing directory, not `/`; anything else is a usage error (exit 2) |
| `--help`, `--version` | the usage line, or the package version, on stdout, then exit 0 |

The scope flags are the command's [`--scope-project` and `--scope`](README.md#the-layerfs-command)
under this server's names. The scope is fixed for the process; a scoped call
the library refuses comes back as its own `InvalidInput` (`out of scope: …`)
([`scope.rs`](../../../../api/crates/layerfs-api/src/dispatch/guard/scope.rs)).

## Export allowlist

`project_export` writes to a path on the machine running this server, and the
client picks the path, so the operator names where it may write at launch
([`export-roots.js`](../../../../api/mcp/src/export-roots.js)).

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

Each call belongs to exactly one bucket: `work` (listed by both profiles) or
`admin` (listed only by `admin`). Every 2.0 entry names its own bucket in its
table entry (`form.profile`: `agent` is `work`, `full` is `admin`), and
[`profiles.js`](../../../../api/mcp/src/profiles.js) reads only that; it names
calls only to give some a longer line in the table below. 17 calls are in
`work` and 66 in `admin` only.

| Bucket | Calls |
| --- | --- |
| `work` | `branch.fork`; `commit.list`, `commit.diff`; `file.read`, `file.list`, `file.glob`, `file.grep`, `file.write`, `file.edit`, `file.rm`, `file.mv`; `sandbox.create`; `workspace.list`, `workspace.exec`, `workspace.cancel`, `workspace.commit`, `workspace.status` |
| `admin` | every other call, the six doctors included |

`work` lets an agent fork a Branch, read and change its files (each write is
one explicit Commit), read and compare its Commits, start a Sandbox and run,
cancel and commit in a mounted Workspace. `admin` adds the project, home,
machine, Store and backup calls, the rest of the Branch, file and Sandbox
calls, and the doctors: the calls that start workers, move whole Stores, hold
credentials, retire or compact records, or change the shape of the home. A new call with
no bucket fails `tools()`, generation and the test rather than appearing
silently. Regenerate the table below with
`node api/scripts/mcp-profiles.mjs` (`--check` compares; both need the
built addon).

<!-- BEGIN GENERATED: api/scripts/mcp-profiles.mjs -->

84 calls: 17 in `work` (the table's `agent` profile), 67 in `admin` only (`full`); `--profile admin` lists 84.

| Call | Bucket | Profiles that list it | Why |
| --- | --- | --- | --- |
| `home.init` | admin | admin | set the home up, read its state and call log, and check it; the `full` profile (administration) |
| `home.get` | admin | admin | set the home up, read its state and call log, and check it; the `full` profile (administration) |
| `home.log` | admin | admin | set the home up, read its state and call log, and check it; the `full` profile (administration) |
| `home.clear_log` | admin | admin | set the home up, read its state and call log, and check it; the `full` profile (administration) |
| `home.doctor` | admin | admin | set the home up, read its state and call log, and check it; the `full` profile (administration) |
| `home.move` | admin | admin | move this home into another folder on the same filesystem; client folders stay; the `full` profile (administration) |
| `project.init` | admin | admin | import a host folder into a Store; the `full` profile (administration) |
| `project.list` | admin | admin | import, list, read, count, export, delete or check a project, or hide a path in it; the `full` profile (administration) |
| `project.get` | admin | admin | import, list, read, count, export, delete or check a project, or hide a path in it; the `full` profile (administration) |
| `project.stats` | admin | admin | import, list, read, count, export, delete or check a project, or hide a path in it; the `full` profile (administration) |
| `project.doctor` | admin | admin | import, list, read, count, export, delete or check a project, or hide a path in it; the `full` profile (administration) |
| `project.export` | admin | admin | export a Branch snapshot to a local folder under a launch-time --export-root; a folder scope refuses it; the `full` profile (administration) |
| `project.delete` | admin | admin | retire a project in its Store and remove its home entry; a folder scope refuses it; the `full` profile (administration) |
| `project.hide_path` | admin | admin | hide one absolute path from the project's whole history; a folder scope refuses it; the `full` profile (administration) |
| `branch.list` | admin | admin | list, read, fork, merge, publish and remove Branches; the `full` profile (administration) |
| `branch.get` | admin | admin | list, read, fork, merge, publish and remove Branches; the `full` profile (administration) |
| `branch.fork` | work | work, admin | list, read, fork, merge, publish and remove Branches; the `agent` profile |
| `branch.merge` | admin | admin | list, read, fork, merge, publish and remove Branches; the `full` profile (administration) |
| `branch.publish` | admin | admin | list, read, fork, merge, publish and remove Branches; the `full` profile (administration) |
| `branch.remove` | admin | admin | retire one Branch in its Store; a folder scope refuses it; the `full` profile (administration) |
| `commit.list` | work | work, admin | read a Branch's Commit history and compare Commits; the `agent` profile |
| `commit.get` | admin | admin | read a Branch's Commit history and compare Commits; the `full` profile (administration) |
| `commit.diff` | work | work, admin | read a Branch's Commit history and compare Commits; the `agent` profile |
| `file.read` | work | work, admin | read and change files in a Branch; each write is one explicit Commit; the `agent` profile |
| `file.stat` | admin | admin | read and change files in a Branch; each write is one explicit Commit; the `full` profile (administration) |
| `file.list` | work | work, admin | read and change files in a Branch; each write is one explicit Commit; the `agent` profile |
| `file.glob` | work | work, admin | read and change files in a Branch; each write is one explicit Commit; the `agent` profile |
| `file.grep` | work | work, admin | read and change files in a Branch; each write is one explicit Commit; the `agent` profile |
| `file.history` | admin | admin | read and change files in a Branch; each write is one explicit Commit; the `full` profile (administration) |
| `file.blame` | admin | admin | read and change files in a Branch; each write is one explicit Commit; the `full` profile (administration) |
| `file.write` | work | work, admin | read and change files in a Branch; each write is one explicit Commit; the `agent` profile |
| `file.edit` | work | work, admin | read and change files in a Branch; each write is one explicit Commit; the `agent` profile |
| `file.rm` | work | work, admin | read and change files in a Branch; each write is one explicit Commit; the `agent` profile |
| `file.mkdir` | admin | admin | read and change files in a Branch; each write is one explicit Commit; the `full` profile (administration) |
| `file.mv` | work | work, admin | read and change files in a Branch; each write is one explicit Commit; the `agent` profile |
| `file.cp` | admin | admin | read and change files in a Branch; each write is one explicit Commit; the `full` profile (administration) |
| `file.apply` | admin | admin | read and change files in a Branch; each write is one explicit Commit; the `full` profile (administration) |
| `sandbox.create` | work | work, admin | start a Sandbox, and with --mount mount a Branch in it; the `agent` profile |
| `sandbox.list` | admin | admin | start, read, remove, inspect and check a Sandbox; the `full` profile (administration) |
| `sandbox.get` | admin | admin | start, read, remove, inspect and check a Sandbox; the `full` profile (administration) |
| `sandbox.remove` | admin | admin | stop a Sandbox and remove it; the `full` profile (administration) |
| `sandbox.files` | admin | admin | start, read, remove, inspect and check a Sandbox; the `full` profile (administration) |
| `sandbox.log` | admin | admin | start, read, remove, inspect and check a Sandbox; the `full` profile (administration) |
| `sandbox.doctor` | admin | admin | start, read, remove, inspect and check a Sandbox; the `full` profile (administration) |
| `workspace.list` | work | work, admin | mount, run in, cancel, commit and unmount a Workspace explicitly; nothing is implicit; the `agent` profile |
| `workspace.mount` | admin | admin | mount, run in, cancel, commit and unmount a Workspace explicitly; nothing is implicit; the `full` profile (administration) |
| `workspace.exec` | work | work, admin | mount, run in, cancel, commit and unmount a Workspace explicitly; nothing is implicit; the `agent` profile |
| `workspace.cancel` | work | work, admin | mount, run in, cancel, commit and unmount a Workspace explicitly; nothing is implicit; the `agent` profile |
| `workspace.commit` | work | work, admin | mount, run in, cancel, commit and unmount a Workspace explicitly; nothing is implicit; the `agent` profile |
| `workspace.unmount` | admin | admin | mount, run in, cancel, commit and unmount a Workspace explicitly; nothing is implicit; the `full` profile (administration) |
| `workspace.status` | work | work, admin | mount, run in, cancel, commit and unmount a Workspace explicitly; nothing is implicit; the `agent` profile |
| `machine.list` | admin | admin | machines hold Stores and workers: read, start, stop, create, change, inspect and check them; the `full` profile (administration) |
| `machine.get` | admin | admin | machines hold Stores and workers: read, start, stop, create, change, inspect and check them; the `full` profile (administration) |
| `machine.images` | admin | admin | machines hold Stores and workers: read, start, stop, create, change, inspect and check them; the `full` profile (administration) |
| `machine.load_image` | admin | admin | machines hold Stores and workers: read, start, stop, create, change, inspect and check them; the `full` profile (administration) |
| `machine.prune` | admin | admin | machines hold Stores and workers: read, start, stop, create, change, inspect and check them; the `full` profile (administration) |
| `machine.stats` | admin | admin | machines hold Stores and workers: read, start, stop, create, change, inspect and check them; the `full` profile (administration) |
| `machine.files` | admin | admin | machines hold Stores and workers: read, start, stop, create, change, inspect and check them; the `full` profile (administration) |
| `machine.log` | admin | admin | machines hold Stores and workers: read, start, stop, create, change, inspect and check them; the `full` profile (administration) |
| `machine.start` | admin | admin | machines hold Stores and workers: read, start, stop, create, change, inspect and check them; the `full` profile (administration) |
| `machine.stop` | admin | admin | machines hold Stores and workers: read, start, stop, create, change, inspect and check them; the `full` profile (administration) |
| `machine.create` | admin | admin | machines hold Stores and workers: read, start, stop, create, change, inspect and check them; the `full` profile (administration) |
| `machine.remove` | admin | admin | machines hold Stores and workers: read, start, stop, create, change, inspect and check them; the `full` profile (administration) |
| `machine.update` | admin | admin | machines hold Stores and workers: read, start, stop, create, change, inspect and check them; the `full` profile (administration) |
| `machine.doctor` | admin | admin | machines hold Stores and workers: read, start, stop, create, change, inspect and check them; the `full` profile (administration) |
| `store.list` | admin | admin | create, connect, attach, move, compact, upgrade, check and remove Stores; the `full` profile (administration) |
| `store.get` | admin | admin | create, connect, attach, move, compact, upgrade, check and remove Stores; the `full` profile (administration) |
| `store.create` | admin | admin | create, connect, attach, move, compact, upgrade, check and remove Stores; the `full` profile (administration) |
| `store.connect` | admin | admin | create, connect, attach, move, compact, upgrade, check and remove Stores; the `full` profile (administration) |
| `store.disconnect` | admin | admin | create, connect, attach, move, compact, upgrade, check and remove Stores; the `full` profile (administration) |
| `store.attach` | admin | admin | create, connect, attach, move, compact, upgrade, check and remove Stores; the `full` profile (administration) |
| `store.detach` | admin | admin | create, connect, attach, move, compact, upgrade, check and remove Stores; the `full` profile (administration) |
| `store.move` | admin | admin | create, connect, attach, move, compact, upgrade, check and remove Stores; the `full` profile (administration) |
| `store.remove` | admin | admin | create, connect, attach, move, compact, upgrade, check and remove Stores; the `full` profile (administration) |
| `store.compact` | admin | admin | reclaim one Store's dead bytes; a folder scope refuses it; the `full` profile (administration) |
| `store.upgrade` | admin | admin | bring an older-format Store to the latest format, in place; Ctrl-C cancels it before COMMIT; the `full` profile (administration) |
| `store.repair_mount_holders` | admin | admin | the operator's explicit repair of the admissions that hold one Branch: list, preview, settle, release or audit; the `full` profile (administration) |
| `store.doctor` | admin | admin | create, connect, attach, move, compact, upgrade, check and remove Stores; the `full` profile (administration) |
| `backup.connect` | admin | admin | connect buckets, back up and restore a Store, and check a bucket; the `full` profile (administration) |
| `backup.disconnect` | admin | admin | connect buckets, back up and restore a Store, and check a bucket; the `full` profile (administration) |
| `backup.list` | admin | admin | connect buckets, back up and restore a Store, and check a bucket; the `full` profile (administration) |
| `backup.create` | admin | admin | connect buckets, back up and restore a Store, and check a bucket; the `full` profile (administration) |
| `backup.restore` | admin | admin | connect buckets, back up and restore a Store, and check a bucket; the `full` profile (administration) |
| `backup.doctor` | admin | admin | connect buckets, back up and restore a Store, and check a bucket; the `full` profile (administration) |

<!-- END GENERATED -->

## Tools and schemas

One tool per call, named `<family>_<call>` (`store_create`,
`workspace_cancel`, …), generated from `layerfs.table`
([`tools.js`](../../../../api/mcp/src/tools.js)). Each tool's JSON Schema is
the entry's arguments, in order, then its options, minus the functions
(`on_progress`, `stdout`, `stderr`); `additionalProperties` is false. A
2.0 entry (every entry has a `form`) also takes `detail` (the long form)
and, when it lists sections, `with` (an array of their names). The
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
| Refusal | `isError: true` and a text block `{ "error", "message", "fix" }` with the fields beside them that the library put there, and `detail` when it has one; the library's own | [`errors.js`](../../../../api/mcp/src/errors.js) |
| Uncertain outcome | `Unknown`, `Unresolved`, or a refusal with `attempted: true` (the field beside `error`, or the long form's `detail.attempted`; the message text is not read), adds `outcome uncertain: read state before acting again; nothing was retried.` (`UNCERTAIN`) | [`errors.js`](../../../../api/mcp/src/errors.js), [`values.js`](../../../../api/mcp/src/values.js) `isUncertain` |
| Retry, replay, Commit, unmount | none of its own: one library call per tool call; Commit and unmount only through `workspace_commit` and `workspace_unmount`, never on an error, disconnect or shutdown | [`server.js`](../../../../api/mcp/src/server.js) |
| Progress | with a progress token, `on_progress` events become `notifications/progress`: the first at once, then at most one per 100 ms (`INTERVAL_MS`) carrying the latest, and the last before the result, which waits 10 ms so the note arrives first. `progress` counts 1, 2, …; `total` only when positive; `message` the event as JSON, cut at 512 characters. Without a token, no handler is passed | [`progress.js`](../../../../api/mcp/src/progress.js) |
| `workspace.exec` | output is buffered (no `stdout`/`stderr` function): each stream's text is in the result up to its last 1 MiB; a longer stream answers its last 1 MiB and `stdout_file` or `stderr_file` names the file on the machine that holds all of it; `stdout_bytes` and `stderr_bytes` (long form) give its full size | [Errors and events](../calls/errors-and-events.md#workspaceexec-output) |
| Cancel notification | acknowledged on stderr; the native call cannot be cancelled and runs to its end | [`server.js`](../../../../api/mcp/src/server.js) |
| stdin close, `SIGTERM`, `SIGINT` | stop accepting calls, wait up to 2 s (`GRACE_MS`) for in-flight ones, run `layerfs.close()` (idle Stores only), exit 0. A Workspace stays mounted; a call still running is cut off and its outcome is unknown | [`server.js`](../../../../api/mcp/src/server.js) |
| stdout, stderr | stdout carries one JSON-RPC message per line and nothing else while serving; one `layerfs-mcp: …` line per call, startup and shutdown on stderr. Argument values are never logged (a `store.connect` password, a `backup.connect` key) | [`server.js`](../../../../api/mcp/src/server.js) |

## Limits

- stdio only: no HTTP/SSE transport, no authentication (it runs as the user
  who launched it), no resources, prompts or sampling.
- One home and one fixed scope per process; no `init`/`open`/`close` tool.
- Calls run on Node's default thread pool (`UV_THREADPOOL_SIZE` 4): many long
  concurrent calls can starve each other. The server does not raise it.
- No batching or automatic paging: `limit` and `cursor` pass through, and
  `next_cursor` comes back as the call gives it.

## Tests

From the repository root, each under a 120 s wall limit, with `TMPDIR` on
ext4, xfs or btrfs, the built addon (`api/node/layerfs.node`) and
`LAYERFS_CLI`. The tests run a real `node src/server.js` with the SDK's
`Client` over `StdioClientTransport` and a temporary home.

| Test | Command | Shows |
| --- | --- | --- |
| package | `cd api/mcp && npm install --no-audit --no-fund && npm test` | the files below, one at a time |
| [`workflow.test.mjs`](../../../../api/mcp/test/protocol/workflow.test.mjs) | `node --test api/mcp/test/protocol/workflow.test.mjs` | initialize, `tools/list` counts, a Store/project/Branch/file history flow, export with progress, refusals, scope, the work profile |
| [`purity.test.mjs`](../../../../api/mcp/test/protocol/purity.test.mjs) | `node --test api/mcp/test/protocol/purity.test.mjs` | stdout is JSON-RPC only while a call logs to stderr |
| [`values.test.mjs`](../../../../api/mcp/test/tools/values.test.mjs) | `node --test api/mcp/test/tools/values.test.mjs` | bytes round-trip, the integer wrapper |
| [`concurrency.test.mjs`](../../../../api/mcp/test/protocol/concurrency.test.mjs) | `node --test api/mcp/test/protocol/concurrency.test.mjs` | 10 concurrent calls; stdin close and SIGTERM exit 0 within 5 s, no process left |
| [`export-roots.test.mjs`](../../../../api/mcp/test/tools/export-roots.test.mjs) | `node --test api/mcp/test/tools/export-roots.test.mjs` | the allowlist cases above |
| [`profiles.test.mjs`](../../../../api/mcp/test/tools/profiles.test.mjs) | `node --test api/mcp/test/tools/profiles.test.mjs` | every call in exactly one bucket; the generated table equals this page; every schema valid |
| coverage | `node api/scripts/coverage-doc.mjs --check` | the MCP column of the [coverage matrix](../calls/coverage.md) matches the same tool list |

## Related

- [Front ends](README.md); [bindings](bindings.md) (the Node package it calls).
- [Calls](../calls/README.md), [errors and events](../calls/errors-and-events.md).
