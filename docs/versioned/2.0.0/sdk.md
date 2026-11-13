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
import layerfs, { LayerFSError, table, families } from "@layerfs/node";

await layerfs.store.create("shop");
await layerfs.project.init("shop/api", "/src/api", { on_progress: console.log });
await layerfs.branch.fork("shop/api", "agent", "main");
await layerfs.file.write("shop/api", "agent", "/README.md", "hello\n");
// a named Workspace: ws is "tests:build" (#536 C3)
const { ws } = await layerfs.workspace.mount("tests", "agent", { name: "build" });

const lfs = layerfs.client({ home: "/data/lfs", scope: { project: "shop/web", folder: "/web", branch: "agent" } });
```

- Node 22 or later. The package is an ES module.
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
    print([b["name"] for b in page["items"]])

asyncio.run(main())
```

## Export a project snapshot

See [`project.export`](../../architecture/api/sdk/project.md#projectexport)
for destination validation, fidelity, routing, refusal codes and result fields.
The table has 61 calls, five in the `project` family.

```js
// /exports exists; the destination is absent or empty on this host.
const exported = await layerfs.project.export("shop/api", "/exports/api", {
  branch: "main",
  on_progress: ({ phase, bytes }) => console.error(phase, bytes),
});
```

```python
result = layerfs.project.export("shop/api", "/exports/api", branch="main", on_progress=print)
# Inside an async function, using a separate destination:
# result = await layerfs.aio.project.export("shop/api", "/exports/api-agent", branch="agent", on_progress=print)
```

Omitting `branch` selects `main`. There is no `commit` option. The immutable
node/root is pinned once; later publications and uncommitted Workspace changes
do not enter the output. SQLite worker routes stream to the caller’s local
writer; local/remote PostgreSQL connections read through the caller’s process.
A required stopped machine is `Unavailable`.

Callbacks receive `prepare`, `export`, `finalize` with monotonic local-output
`files`, `directories`, `symlinks`, `bytes` and `done = bytes` during export; `total` is
`None`/`null`. `finalize` comes only once the folder is in place, and `export`
updates at most every 200 ms; only a successful return means success.
Omit `on_progress` when unused: the current Python shell rejects explicit
`on_progress=None`. Handler exceptions do not cancel export. Cancelling `aio`
stops callbacks while the native operation continues, so inspect the destination
before deciding on another attempt.

## Functions as options

| Option | Calls | Called with |
| --- | --- | --- |
| `on_progress` | `project.init`, `project.export`, `workspace.exec`, `store.push`, `store.pull`, `backup.create`, `backup.restore` | each progress update (an object or dict); for `workspace.exec`, once, `{exec: {id}}` before the command starts |
| `stdout`, `stderr` | `workspace.exec` | the command's output, piece by piece: a string when the piece is UTF-8, else a `Buffer` (Node) or `bytes` (Python) |

There is one progress stream and no job registry (O22). Another process
cannot watch a call it did not start; `store status` shows a push or
pull in progress. `workspace.exec` sends its output after the command ends; there
is no live output stream yet. A stream over 1 MiB comes only through these
functions (one 1 MiB piece at a time) or `stream: true`; the result then
has that stream `null`, its byte count and `streamed: true`.

- `on_progress` on `workspace.exec` gives the command's exec id while it
  runs, so another call (in this process or another) can end it:

  ```js
  const id = new Promise((resolve) => {
    lfs.workspace.exec(ws, "npm run dev", { on_progress: (p) => resolve(p.exec.id) });
  });
  await lfs.workspace.exec_cancel(ws, await id, { grace_ms: 2000 });
  ```

  Not run as written; the shell form was (`layerfs-cli/tests/exec_cancel.rs`).
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
