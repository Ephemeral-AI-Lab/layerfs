# Bindings

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The Node and Python bindings: a native library per language that passes JSON text to `layerfs_api::call_json` once per call, and a package that builds one function per table entry over it, within [Front ends](README.md).

| | |
| --- | --- |
| Source | [`api/crates/layerfs-node/src`](../../../../api/crates/layerfs-node/src), [`api/node`](../../../../api/node), [`api/crates/layerfs-python/src`](../../../../api/crates/layerfs-python/src), [`api/python`](../../../../api/python), [`node-types.mjs`](../../../../api/scripts/node-types.mjs), [`native-addon.mjs`](../../../../api/scripts/native-addon.mjs), [`python_stubs.py`](../../../../api/scripts/python_stubs.py) |

Both bindings share these rules: the client is built from the table
(`table_json`), so a call is added only in `layerfs-api`; arguments are split
into positionals and options, checked lightly (too many positionals, unknown
option, a function option that is not a function → `InvalidInput` before any
native call) and checked again by the library; `null`/`None` leaves a value
out; bytes cross as `{ "base64": "…" }` and come back as `Buffer`/`bytes`;
every event reaches its function before the call settles; a refusal becomes
`LayerFSError` (`code`, `message`, `fix`, the next-call fields as properties,
`detail` only when the call asked for the long form); `log_tag` names the call-log line, with
client `node` or `python`. The home defaults to `LAYERFS_HOME`, else
`~/.layerfs/home` (or a legacy `~/.layerfs` that holds `layerfs.json`),
decided by the library.

## layerfs-node

```text
api/crates/layerfs-node (cdylib, napi 2.16, napi4)
  call.rs  table()  -> layerfs_api::table_json()
           call()   -> AsyncTask: call_json once; events -> threadsafe fn
           close()  -> layerfs_api::close(): close_idle(0), end_idle_sessions(250 ms)
api/node/src
  index.js    table, families, client(), bind(), close(),
              default export (all ten families) and named family exports
  native.js   loads layerfs.node (the package's own, or LAYERFS_NATIVE)
  values.js   encode / decode: exact whole numbers, {"base64": ...}
  errors.js   LayerFSError { code, message, fix, detail, ...next-call fields }
  index.d.ts  generated from the table
```

```text
JS thread                           napi worker thread
lfs.file.read(p, b, path, {..})
 | split positionals / options; functions kept in `on`
 +--- native.call(name, args, home, scope, onEvent) --->  call_json once
 <-- onEvent(text) (own handle, counted) -------------- each event
 <-- '{"events":n,"answer":{...}}' ---------------------
 | wait until n events delivered
 v answer.value, or throw LayerFSError
```

| Rule | Behaviour | Source |
| --- | --- | --- |
| Client | `client({ home, scope })` makes one function per entry, grouped by family, frozen; the default export and the family exports use `client()`. The named exports are nine families; `home` is on the default export and on a client only (`layerfs.home.get()`) | [`index.js`](../../../../api/node/src/index.js) |
| Options object | a trailing plain object is the options when there are more values than positionals, or when its slot is not a `json` positional, so an optional positional can be left off | `bind` |
| `stream: true` | without `stdout`/`stderr` functions, output goes to `process.stdout`/`process.stderr` | `bind` |
| Event order | the addon counts each event it queued; the call settles only after that many were delivered. Events wait in an unbounded queue (`create_threadsafe_function(0, …)`, `Blocking`), so a slow handler does not slow the call | [`call.rs`](../../../../api/crates/layerfs-node/src/call.rs) |
| A throwing handler | a synchronous throw becomes `process.emitWarning(…, "LayerFSHandlerWarning")` and the call goes on; an `async` handler's rejection is not awaited | `bind` |
| Values in | `bigint` as exact digits (the library refuses past signed 64 bits); `number` only when safe or a fraction; `NaN`, `±Infinity`, unsafe integers, `Map`, `Set`, `ArrayBuffer`, typed arrays other than `Uint8Array`, `DataView`, functions, symbols, cycles and lone surrogates refused `InvalidInput` before any call; `Uint8Array`/`Buffer` → `{ base64 }` | [`values.js`](../../../../api/node/src/values.js) |
| Values out | a whole number as `number` when safe, else `bigint`; `{ base64 }` → `Buffer`, so exec output is a `string` or a `Buffer` per event. A map whose only key is `base64` with base64 text is bytes both ways | [`values.js`](../../../../api/node/src/values.js) |
| Outside the table | `close()` only, synchronous, answering how many Stores it closed. The six doctors are table calls (`layerfs.home.doctor()`, …); the 1.x `doctor()` function and `doctor.fix` were removed with no alias | [`index.js`](../../../../api/node/src/index.js) |
| Loading | the file `LAYERFS_NATIVE` names, else `layerfs.node` beside `src`; nothing else is tried. A missing or unloadable file throws one error naming the path, `platform/arch`, the Node version and how to build | [`native.js`](../../../../api/node/src/native.js) |
| Build | `npm --prefix api run build:native` runs `cargo +1.85.1 build --offline --locked [--release] -p layerfs-node` (`--debug` for dev; `--features tls` opt-in; PostgreSQL is always built) and copies the result to `api/node/layerfs.node`, or `LAYERFS_NATIVE_OUTPUT`, through a staged rename. Only `darwin` and `linux`; a set `CARGO_BUILD_TARGET` or a target outside the worktree is refused | [`native-addon.mjs`](../../../../api/scripts/native-addon.mjs) |
| Typings | `index.d.ts` from the table (`npm --prefix api run gen:types`; `--check` compares): per family an interface, positionals typed by kind (`int` → `number \| bigint`, `size` → `number \| bigint \| string`, `content` → `string \| Uint8Array`, `json` → `Value`), options extending `CallOptions` (`detail`, `with`, `log_tag`; an entry with sections narrows `with` to their names); every call answers `Promise<Result>`, since the table has no result schema | [`node-types.mjs`](../../../../api/scripts/node-types.mjs) |

The package is an ES module, MIT, for Node 22 or later (exact whole numbers
use `JSON.rawJSON` and the source text of `JSON.parse`)
([`package.json`](../../../../api/node/package.json)). The workspace root
[`api/package.json`](../../../../api/package.json) (`layerfs-js`, private)
links it and holds the build and type scripts.

| Test | Checks |
| --- | --- |
| [`every-call.test.mjs`](../../../../api/node/test/calls/every-call.test.mjs) | ten families and 84 calls, every function; required-argument refusals; a Store, Init with progress, file and Branch calls, bytes, a folder scope; the call log's `client` and `log_tag` |
| [`shell.test.mjs`](../../../../api/node/test/contract/shell.test.mjs) | the command and the package refuse every call alike; a live Store read both ways (needs `target/debug/layerfs`) |
| [`events.test.mjs`](../../../../api/node/test/calls/events.test.mjs), [`progress.test.mjs`](../../../../api/node/test/calls/progress.test.mjs) | handler failures, delivery before settling, exec output through functions and `stream: true` (exec half needs `LAYERFS_DAEMON`) |
| [`values.test.mjs`](../../../../api/node/test/native/values.test.mjs) | exact `bigint`, refused values, bytes and the one-key map |
| [`native.test.mjs`](../../../../api/node/test/native/native.test.mjs) | a missing addon is one clear error; `LAYERFS_NATIVE` wins; no fallback is tried |
| [`package.test.mjs`](../../../../api/node/test/drift/package.test.mjs) | licence, file list, `engines`, and `index.d.ts` equal to what the table generates |
| [`close.test.mjs`](../../../../api/node/test/calls/close.test.mjs) | `close()`, the one function outside the table |
| [`doctor.test.mjs`](../../../../api/node/test/calls/doctor.test.mjs) | `home.doctor` and its siblings as table calls that change nothing; `home.leftovers` and `home.prune` are gone |
| [`coverage.test.mjs`](../../../../api/node/test/drift/coverage.test.mjs), [`front-ends.test.mjs`](../../../../api/node/test/contract/front-ends.test.mjs) | the [coverage matrix](../calls/coverage.md) and cross-front-end answers |

Build the addon first; run one file at a time, for example
`perl -e 'alarm 120; exec @ARGV' node --test api/node/test/calls/every-call.test.mjs`, with
`TMPDIR` on ext4, xfs or btrfs. The typings sample
[`test/drift/types-sample.ts`](../../../../api/node/test/drift/types-sample.ts) is checked
with `tsc --noEmit --strict` outside the repository (TypeScript is not a
dependency).

## layerfs-python

```text
api/crates/layerfs-python (cdylib "_native", PyO3, abi3-py310)
  lib.rs   #[pymodule] _native: table, call, close
  call.rs  call(name, args, home=None, scope=None, on_event=None)
             py.detach: call_json once; each event: attach, on_event(text)
api/python/src/layerfs
  __init__.py   close(), and the ten families of the default client
  _client.py    TABLE, FAMILIES, Client, client(), _bind
  aio.py        AsyncClient: the same calls as coroutines
  _values.py    encode / decode
  _errors.py    LayerFSError(code, message, detail, fix, fields)
  py.typed, *.pyi   stubs; three generated by api/scripts/python_stubs.py
```

| Rule | Behaviour | Source |
| --- | --- | --- |
| Interpreter | released (`py.detach`) for the whole library call; re-taken per event, so a slow function slows the call. An exception a function raises goes to `sys.unraisablehook` and the call goes on | [`call.rs`](../../../../api/crates/layerfs-python/src/call.rs) |
| Client | `Client(home, scope)` sets one `Family` attribute per family and one function per entry, with the entry's `help` as docstring; module-level `layerfs.home` … `layerfs.backup` belong to `Client()` | [`_client.py`](../../../../api/python/src/layerfs/_client.py), [`__init__.py`](../../../../api/python/src/layerfs/__init__.py) |
| Keywords | optional positionals and options may be keywords: `file.read(p, b, path, line=2, limit=50)`; a Python keyword takes a trailing underscore, `with_` and `from_` (`branch.fork(p, "agent", from_="main")`); a positional given twice is refused | `_bind` |
| `stream=True` | without functions, output goes to `sys.stdout`/`sys.stderr` (bytes to the buffer) | `_streamed` |
| Values | `bytes`, `bytearray`, `memoryview` → `{ base64 }`; NaN, Infinity, lone surrogates (also in a home or scope), non-text map keys and types with no JSON form raise `InvalidInput` before any call; `int` exact both ways, the library refusing past signed 64 bits; exec output arrives as `str` when UTF-8, else `bytes` | [`_values.py`](../../../../api/python/src/layerfs/_values.py) |
| `layerfs.aio` | each call runs in `asyncio.to_thread`; functions are queued to the loop, and the coroutine awaits a barrier behind them before returning. Cancelling it stops the functions; the native call runs to its end and its outcome is dropped | [`aio.py`](../../../../api/python/src/layerfs/aio.py) |
| Outside the table | `close()` only; the six doctors are table calls (`layerfs.home.doctor()`, …), and the 1.x `doctor()` function was removed with no alias | [`__init__.py`](../../../../api/python/src/layerfs/__init__.py) |
| Stubs | `_client.pyi`, `aio.pyi`, `__init__.pyi` generated from the table: positionals as listed (`X \| None = None` when optional), options keyword-only (every call also takes `detail`, `with_` and `log_tag`), kinds `int` → `int`, `size` → `int \| str`, `content` → `str \| bytes \| bytearray \| memoryview`, `json` → `JSON`; every call returns `Result` (`Any`). `_values.pyi` and `_errors.pyi` are hand written | [`python_stubs.py`](../../../../api/scripts/python_stubs.py) |
| Build | `cd api/python && maturin build --locked` (maturin 1.9.6 or a later 1.x) builds `layerfs._native`; on macOS `pyo3-build-config` links with `-undefined dynamic_lookup`, so a plain Cargo build also works | [`pyproject.toml`](../../../../api/python/pyproject.toml), [`Cargo.toml`](../../../../api/crates/layerfs-python/Cargo.toml) |

The distribution is `layerfs` 2.0.0, MIT (`api/python/LICENSE`, a copy of
the repository's, ships in the wheel and the source distribution), Python
3.10 or later.

| Test | Checks |
| --- | --- |
| [`test_calls.py`](../../../../api/python/test/calls/test_calls.py) | the table as functions; refusals equal the command's; a live Store read both ways; `aio` delivery and cancellation; exec output (needs `LAYERFS_DAEMON`) |
| [`test_values.py`](../../../../api/python/test/native/test_values.py) | refused values never reach the library; 64-bit bounds; bytes and the one-key map; malformed scopes |
| [`test_flow.py`](../../../../api/python/test/calls/test_flow.py) | a mountless file and history flow through `aio`, read through both packages alike |
| [`test_progress.py`](../../../../api/python/test/calls/test_progress.py), [`test_project_export.py`](../../../../api/python/test/native/test_project_export.py) | progress delivery and handler failures; export events |
| [`test_stubs.py`](../../../../api/python/test/stubs/test_stubs.py) | committed stubs equal `python_stubs.py`'s output; licence; `mypy --strict` on `typed_samples/` (skipped without mypy) |
| [`test_native_missing.py`](../../../../api/python/test/native/test_native_missing.py), [`test_doctor.py`](../../../../api/python/test/calls/test_doctor.py), [`test_disabled_platforms.py`](../../../../api/python/test/calls/test_disabled_platforms.py) | a clear `ImportError` without `_native`; the doctors as table calls; `home.leftovers` and `home.prune` gone; removed platforms refused |

Build with `cargo +1.85.1 build --locked -p layerfs-python -p layerfs-cli`,
copy `target/debug/lib_native.dylib` (`.so` on Linux) to
`api/python/src/layerfs/_native.abi3.so` (or `maturin develop --locked
--offline`), then
`perl -e 'alarm 120; exec @ARGV' python3 -m unittest discover -s api/python/test`
with `TMPDIR` on ext4, xfs or btrfs.

## Related

- [Front ends](README.md): the command and packaging.
- [Calls](../calls/README.md): the table the clients are built from.
- [Errors and events](../calls/errors-and-events.md): codes and event delivery.
- [MCP](mcp.md): the server over the Node binding.
