# Node and Python

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Part of the [2.0.0 manual](README.md). Sources:
[`api/node/src/index.js`](../../../api/node/src/index.js),
[`api/python/src/layerfs/`](../../../api/python/src/layerfs/__init__.py),
[api/README.md](../../../api/README.md) and
[api/python/README.md](../../../api/python/README.md).

The Node package is `@layerfs/node` (`api/node`) and the Python package
`layerfs` (`api/python`). Each is a thin front end over the same Rust
library. Each builds its ten families from the library's call table, so
neither names a call in its own code, and both take the same arguments as
the `layerfs` command. Every call is one call into the library, made once.
Neither package is published.

## Node

```js
import layerfs, { LayerFSError, table, families } from "@layerfs/node";

await layerfs.store.create("shop");
await layerfs.project.init("shop/api", "/src/api", { on_progress: console.log });
await layerfs.branch.fork("shop/api", "agent", "main");
await layerfs.file.write("shop/api", "agent", "/README.md", "hello\n");
// a named Workspace: workspace is "tests:build"
const { workspace } = await layerfs.workspace.mount("tests", "agent", { name: "build" });

const lfs = layerfs.client({ home: "/data/lfs", scope: { project: "shop/web", folder: "/web", branch: "agent" } });
```

- Node 22 or later. The package is an ES module.
- The default export's ten families and the named exports `project`,
  `branch`, `commit`, `file`, `sandbox`, `workspace`, `machine`, `store` and
  `backup` use the default home (`LAYERFS_HOME`, else `~/.layerfs`). The
  `home` family is on the default export and on a client
  (`layerfs.home.get()`), not a named export.
- `layerfs.client({ home, scope })` makes a client of another home, with an
  optional [folder scope](calls.md#folder-scope) `{ project, folder, branch? }`.
- Every call is `async`. Positionals come in the table's order, then an
  options object. A trailing plain object is read as the options unless its
  slot takes JSON, so an optional positional can be left off:
  `file.glob(p, b, "**/*.md", { limit: 10 })`.
- An unknown option, or too many positionals, throws `InvalidInput` before
  any call.
- Results are plain objects with snake_case keys. Bytes are `Buffer`s;
  content to write may be a string or a `Buffer`.
- A refusal throws `LayerFSError` with `error` (also readable as `code`),
  `message`, `fix` and the fields the next call needs; `detail` is set when
  the call asked the long form (`{ detail: true }`). See
  [the form of an answer](calls.md#the-form-of-an-answer).
- `table` is the call table, `[{ family, call, args, options, help, form }]`
  (`form` as `layerfs help --json` prints it); `families` lists the ten
  families.
- The doctors are table calls of their families (`layerfs.home.doctor()`,
  `layerfs.machine.doctor(machine)` and so on; [the doctors](calls.md#the-doctors)),
  and there is no `layerfs.doctor`. There is no call for leftovers: LayerFS
  removes what interrupted operations left itself ([`home`](calls.md#home-5)).
- `layerfs.close()` (also a named export), outside the table, closes every
  Store this process holds open with no call running, which drops its
  leases; it answers how many it closed. A process calls it before it
  exits, so it leaves no stale lease; a later call opens its Store again.
- The native addon is the `layerfs.node` beside the package's `src`, or the
  file `LAYERFS_NATIVE` names; nothing else is looked at, and a missing one
  is an error naming the platform and how to build it. Build it with
  `npm --prefix api run build:native` (Cargo `+1.85.1`, `--offline
  --locked --release`; `-- --debug` for a dev build), which writes it beside
  the package.
- Whole numbers are exact: a `bigint` is sent as its digits, and a result is a
  `number` when it is a safe integer and a `bigint` otherwise. A `number`
  beyond ±(2^53 - 1), `NaN`, a `Map`, a `Set`, a typed array other than
  `Uint8Array` or text with a lone surrogate is refused (`InvalidInput`)
  before the call. The package needs Node 22, is MIT licensed, and ships
  generated typings (`index.d.ts`).

## Python

```python
import layerfs

layerfs.store.create("shop")
layerfs.project.init("shop/api", "/src/api", on_progress=print)
layerfs.branch.fork("shop/api", "agent", "main")
layerfs.file.write("shop/api", "agent", "/README.md", "hello\n", message="docs")
text = layerfs.file.read("shop/api", "agent", "/README.md")["text"]

lfs = layerfs.client(home="/data/lfs", scope={"project": "shop/web", "folder": "/web", "branch": "agent"})
```

- Python 3.10 or later (abi3). The native module is `layerfs._native`,
  built by PyO3 from `api/crates/layerfs-python`.
- Calls are synchronous (O19). `layerfs.aio` has the same families and
  calls as coroutines; each runs the synchronous call on a thread, and the
  native call releases the interpreter. Its function options run on the
  event loop's thread, in order, all before the coroutine returns.
  Cancelling the coroutine stops its functions at once, but the native call
  cannot be cancelled: it runs to its end and its outcome is dropped, so
  read the state again after a cancel.
- Positionals come in the table's order. Optional positionals and options
  may be keywords: `file.read(p, b, path, line=2)`. A positional
  given twice, an unknown keyword or too many positionals raises
  `InvalidInput`.
- Results are dicts with snake_case keys. File content is `str` or
  `bytes`.
- A refusal raises `layerfs.LayerFSError` with `.error` (also `.code`),
  `.message`, `.fix` and the fields the next call needs (attributes, and
  the `.fields` dict); `.detail` is set when the call asked the long form
  (`detail=True`). `with` and `from` are Python keywords, so the option is
  `with_` and the argument `from_` (`branch.fork(p, "agent", from_="main")`,
  `commit.diff(p, "agent", from_="main")`).
- `layerfs.TABLE` is the call table and `layerfs.FAMILIES` the ten
  families; each family, `home` included, is a module attribute.
- As in Node, the doctors are their families' `doctor` calls and there is
  no `layerfs.doctor`, and no leftover calls.
- `layerfs.close()` is Node's `close()`: it closes this process's open
  Stores, dropping their leases, and returns how many.
- Build: `cd api/python && maturin build --locked` (maturin 1.9.6 or a
  later 1.x, as `pyproject.toml` requires since `2793ee114c`); `--release`
  for the wheel to install. A macOS arm64 wheel and a Linux x64 glibc wheel
  were built and installed on this branch; the Linux arm64 wheel (O20) is
  `NOT_RUN`. The installed artifacts were qualified in
  [artifact-qualification-2.0.0.md](../../general/artifact-qualification-2.0.0.md).

```python
import asyncio
from layerfs import aio

async def main():
    lfs = aio.client(home="/data/lfs")
    page = await lfs.branch.list("shop/api")
    print([b["branch"] for b in page["items"]])

asyncio.run(main())
```

## Export a project snapshot

See [`project.export`](../../architecture/api/sdk/project.md#projectexport)
for destination validation, fidelity, routing, refusal codes and result fields.
The table has 83 calls, eight in the `project` family.

```js
// /exports exists; the destination is absent or empty on this host.
const exported = await layerfs.project.export("shop/api", "/exports/api", {
  branch: "agent",
  on_progress: ({ phase, done }) => console.error(phase, done),
});
// exported is {"file_count":3,"size":17} in the run below
```

```python
result = layerfs.project.export("shop/api", "/exports/api", branch="main", on_progress=print)
# Inside an async function, using a separate destination:
# result = await layerfs.aio.project.export("shop/api", "/exports/api-agent", branch="agent", on_progress=print)
```

Omitting `branch` selects `main`; `at` pins one Commit of its history. The
immutable node/root is pinned once; later publications and uncommitted
Workspace changes do not enter the output. SQLite worker routes stream to
the caller's local writer; local/remote PostgreSQL connections read through
the caller's process. A required stopped machine is `Unavailable`.

The function receives `{ phase, done, total }`: `prepare`, `export` (`done`
the bytes written so far, at most every 200 ms) and `finalize`, which comes
only once the folder is in place; `total` is `null`/`None`. Only a
successful return means success. Run in Node on macOS (a three-file
project):

```text
{"phase":"prepare","done":null,"total":null}
{"phase":"export","done":0,"total":null}
{"phase":"export","done":17,"total":null}
{"phase":"finalize","done":17,"total":null}
{"file_count":3,"size":17}
```

Omit `on_progress` when unused: the Python shell rejects an explicit
`on_progress=None`. Handler exceptions do not cancel export. Cancelling
`aio` stops callbacks while the native operation continues, so inspect the
destination before deciding on another attempt.

## Functions as options

| Option | Calls | Called with |
| --- | --- | --- |
| `on_progress` | the 19 calls whose row says progress: `project.init`, `project.export`, `sandbox.create`, `sandbox.remove`, `workspace.exec`, `machine.load_image`, `machine.start`, `machine.stop`, `machine.create`, `machine.remove`, `store.move`, `store.compact`, `backup.create`, `backup.restore`, and the doctors `home.doctor`, `project.doctor`, `sandbox.doctor`, `machine.doctor`, `store.doctor` | each event, `{ phase, done, total }` (an object or dict); a doctor adds `node` (the finished thing's tree node); `workspace.exec` sends one `{ phase: "start", done: 0, total: 1, exec }` before the command starts |
| `stdout`, `stderr` | `workspace.exec` | the command's output, piece by piece: a string when the piece is UTF-8, else a `Buffer` (Node) or `bytes` (Python) |

There is one progress stream and no job registry (O22). Another process
cannot watch a call it did not start; a running operation shows as the
`busy` state with its `op` (`store get`, `machine get`, `sandbox get`).
`workspace.exec` sends its output after the command ends; there is no live
output stream. A stream of at most 1 MiB is answered whole; a longer one
answers its last 1 MiB and names the file that holds all of it
(`stdout_file`, `stderr_file`), which `sandbox.files` reads.

- `on_progress` on `workspace.exec` gives the command's exec id while it
  runs, so another call (in this process or another) can end it:

  ```js
  const id = new Promise((resolve) => {
    lfs.workspace.exec(workspace, "npm run dev", { on_progress: (p) => resolve(p.exec) });
  });
  await lfs.workspace.cancel(workspace, { exec: await id });
  ```

  Not run as written. The `start` event with `exec` was seen in Node
  ([the examples](examples/README.md#the-workspace-route)); `workspace
  cancel` ends every running command by default, SIGTERM then SIGKILL after
  5 s, and answers `{"already_ended": true}` when none ran.
- `stream: true` on `workspace.exec` passes the output to the `stdout` and
  `stderr` functions, or, without them, to this process's own stdout and
  stderr (Python writes text to the text stream and bytes to its buffer).
- Every function's events are passed on before the call settles: Node
  counts the events the native call queued and resolves only once it has
  delivered that many.
- A function that throws does not stop the call. Node reports the error
  with `process.emitWarning` (`LayerFSHandlerWarning`); Python reports it
  as an unraisable exception.

## Where a call runs

The package does not decide. The home's records do: a call on a sqlite
Store on `native` or on a PostgreSQL Store runs in the calling process; a
call on a Store on another machine goes to that machine's worker. See
[routes](machines-and-stores.md#routes).

A call of this build on a folder that is not a LayerFS 2.0 home (a
`layerfs.json` of another layout, or files and no `layerfs.json`) is
refused `Unsupported` with the fix
`mv <home> <home>.old && layerfs home init`; the home is not converted or
written. See [the home](home.md#an-older-home).

## Checks on this branch

For this version of the manual (branch `cs20-pkg-z` from `call-surface-2-0`
at `61960e3f5f`, macOS arm64, debug builds): the Node and Python
[examples](examples/README.md), the [quickstart](quickstart.md) blocks and
the export run above were run against scratch homes. The Node, Python and
MCP test suites (`api/node/test`, `api/python/test`, `api/mcp/test`) run the
table, refusals and a live Store read through each front end.

Run each test under a wall limit of at most 120 s, for example
`perl -e 'alarm 100; exec @ARGV' node --test api/node/test/calls.mjs`.
