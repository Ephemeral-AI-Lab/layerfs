# Node and Python

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Part of the [2.0.0 manual](README.md). Sources:
[`api/node/src/index.js`](../../../api/node/src/index.js),
[`api/python/src/layerfs/`](../../../api/python/src/layerfs/__init__.py),
[api/README.md](../../../api/README.md) and
[api/python/README.md](../../../api/python/README.md).

Both packages are named `layerfs` (O15). Each is a thin front end over the
same Rust library. Each builds its nine families from the library's call
table, so neither names a call in its own code, and both take the same
arguments as the `layerfs` command. Every call is one call into the
library, made once.

Neither package is published. `@layerfs/layerfs` is the npm fallback name
if `layerfs` is not available (O15); this was not checked against the
registries.

## Node

```js
import layerfs, { LayerFSError, table, families } from "layerfs";

await layerfs.store.create("shop");
await layerfs.project.init("shop/api", "/src/api", { on_progress: console.log });
await layerfs.branch.fork("shop/api", "agent", "main");
await layerfs.file.write("shop/api", "agent", "/README.md", "hello\n");
// a named Workspace: ws is "tests:build" (#536 C3)
const { ws } = await layerfs.workspace.mount("tests", "agent", { name: "build" });

const lfs = layerfs.client({ home: "/data/lfs", scope: { project: "shop/web", folder: "/web", branch: "agent" } });
```

- Node 20 or later. The package is an ES module.
- The default export and the named exports `project`, `branch`, `commit`,
  `file`, `sandbox`, `workspace`, `machine`, `store` and `backup` use the
  default home (`LAYERFS_HOME`, else `~/.layerfs`).
- `layerfs.client({ home, scope })` makes a client of another home, with an
  optional [folder scope](calls.md#folder-scope) `{ project, folder, branch? }`.
- Every call is `async`. Positionals come in the table's order, then an
  options object. A trailing plain object is read as the options unless its
  slot takes JSON, so an optional positional can be left off:
  `file.read(p, b, path, { start_line: 2 })`.
- An unknown option, or too many positionals, throws `InvalidInput` before
  any call.
- Results are plain objects with snake_case keys. Bytes are `Buffer`s;
  content to write may be a string or a `Buffer`.
- A refusal throws `LayerFSError` with `code`, `message` and `detail`.
- `table` is the call table, `[{ family, call, args, options, help }]`;
  `families` lists the nine families.
- `layerfs.doctor({ home })` (also a named export) is
  [`layerfs doctor`](calls.md#layerfs-doctor), outside the table: it
  resolves to `{ ok, findings: [{ severity, entity: { kind, name }, message,
  fix }] }` and changes nothing; `home` defaults as above.
- The native addon is looked for beside the package (`layerfs.node`), at a
  worktree build bundle's root, then beside its crate
  (`engine/crates/layerfs-node/layerfs.node`). Build it with
  `npm --prefix api run build:native` (Cargo `+1.85.1`, `--offline
  --locked --release`; `-- --debug` for a dev build).

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
  built by PyO3 from `engine/crates/layerfs-python`.
- Calls are synchronous (O19). `layerfs.aio` has the same families and
  calls as coroutines; each runs the synchronous call on a thread, and the
  native call releases the interpreter. Its function options run on the
  event loop's thread, in order, all before the coroutine returns.
  Cancelling the coroutine stops its functions at once, but the native call
  cannot be cancelled: it runs to its end and its outcome is dropped, so
  read the state again after a cancel.
- Positionals come in the table's order. Optional positionals and options
  may be keywords: `file.read(p, b, path, start_line=2)`. A positional
  given twice, an unknown keyword or too many positionals raises
  `InvalidInput`.
- Results are dicts with snake_case keys. File content is `str` or
  `bytes`.
- A refusal raises `layerfs.LayerFSError` with `.code`, `.message` and
  `.detail`.
- `layerfs.TABLE` is the call table and `layerfs.FAMILIES` the nine
  families.
- `layerfs.doctor(home=None)` is [`layerfs doctor`](calls.md#layerfs-doctor),
  outside the table: a dict `{"ok", "findings"}` as in Node, changing
  nothing.
- Build: `cd api/python && maturin build --locked` (maturin 1.9.6 or a
  later 1.x, as `pyproject.toml` requires since `2793ee114c`). Only a macOS
  arm64 wheel was built on this branch; the Linux x64 and arm64 glibc
  wheels (O20) are `NOT_RUN`.

```python
import asyncio
from layerfs import aio

async def main():
    lfs = aio.client(home="/data/lfs")
    page = await lfs.branch.list("shop/api")
    print([b["name"] for b in page["items"]])

asyncio.run(main())
```

## Functions as options

| Option | Calls | Called with |
| --- | --- | --- |
| `on_progress` | `project.init`, `store.push`, `store.pull`, `backup.create`, `backup.restore` | each progress update (an object or dict) |
| `stdout`, `stderr` | `workspace.exec` | the command's output, piece by piece: a string when the piece is UTF-8, else a `Buffer` (Node) or `bytes` (Python) |

There is one progress stream and no job registry (O22). Another process
cannot watch a call it did not start; `store status` shows a push or
pull in progress. `workspace.exec` sends its output after the command ends; there
is no live output stream yet. A stream over 1 MiB comes only through these
functions (one 1 MiB piece at a time) or `stream: true`; the result then
has that stream `null`, its byte count and `streamed: true`.

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

The first call of this build on a layout 2 home converts it to layout 3,
or is refused `InUse` while a Sandbox, lease or Store move in it is live; the
refusal's `detail.related` names each (`{kind, name, why}`). See
[the home](home.md#a-layout-2-home).

## Checks on this branch

From the step commits, not repeated for this manual:

- `node --test api/node/test/shell.mjs api/node/test/calls.mjs`: 5/5. All
  57 calls are refused identically by the command and the package when
  their arguments are missing; a live Store read both ways gives identical
  JSON.
- `python3 api/python/test/test_calls.py`: 4/4. All 57 calls are refused
  as the command refuses them; a live Store read gives the same result,
  with bytes, a keyword Commit and a folder scope; `aio`.
- Later, with the review fixes: `api/node/test/events.mjs` (a throwing
  `on_progress` during a Store move; live exec output with `stream`) 2/2,
  and `test_calls.py` 7/7 with the native daemon (`94671c974c`).

Run each under a wall limit of at most 120 s, for example
`perl -e 'alarm 100; exec @ARGV' node --test api/node/test/calls.mjs`.

For this manual, short Node and Python flows were run on macOS against a
private home (Store, Init, Branch, file write and read, a refusal, paging,
`aio`); see the [quickstart](quickstart.md).
