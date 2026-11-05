# Node

[Architecture](../../README.md) › [API](../README.md) › [Front ends](README.md) › Node

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The Node package and its native addon.

| | |
| --- | --- |
| Component | [Front ends](README.md) |
| Source | [`api/crates/layerfs-node/src`](../../../../api/crates/layerfs-node/src), [`api/node`](../../../../api/node) |

## Package

```text
api/node/src/
  index.js    table, families, client(), bind(), doctor(), close(),
              default export and the nine family exports
  native.js   finds and loads layerfs.node
  values.js   encode / decode: JSON text, bytes as {"base64": ...}
  errors.js   LayerFSError { code, message, detail }
```

One function call, start to finish
([`index.js`](../../../../api/node/src/index.js) `bind`):

```text
JS thread                          native worker thread (napi AsyncTask)
---------                          -------------------------------------
lfs.file.read(p, b, path, {..})
 | split positionals / options
 | unknown option -> throw InvalidInput
 | functions kept in `on`
 | args -> encode()
 +--- native.call(name, args, home, scope, onEvent) --->
                                    layerfs_api::call_json(...)  once
                                    each event -> threadsafe fn
 <-- onEvent(text) (own handle) ----   (counted: queued)
 |   decode; on_progress / stdout / stderr
 <-- resolve '{"events":n,"answer":{...}}' -----------
 | wait until n events delivered
 | answer.refused -> throw LayerFSError(code, message, detail)
 v answer.value
```

| Rule | Behaviour | Source |
| --- | --- | --- |
| Building | `table = decode(native.table())`; `client({home, scope})` makes one function per entry, grouped by family, and freezes the client | [`index.js`](../../../../api/node/src/index.js) |
| Defaults | the default export and the named family exports use `client()`: home from `LAYERFS_HOME`, else `~/.layerfs`, decided by the library | [`dispatch.rs`](../../../../api/crates/layerfs-control/src/calls/dispatch.rs) `home` |
| Options object | a trailing plain object is the options when there are more values than positionals, or when its slot is not a `json` positional; so an optional positional can be left off | `bind` |
| Checks before the call | options not an object, too many positionals, an unknown option, a function option that is not a function: `LayerFSError("InvalidInput")` | `bind` |
| `null` / `undefined` | a positional or option with either is left out, so the library sees it as absent | `bind` |
| `stream: true` | without `stdout` / `stderr` functions, output goes to `process.stdout` / `process.stderr` | `bind` |
| Event order | `call.rs` counts each event it queued; the package resolves only after it has delivered that many | [`call.rs`](../../../../api/crates/layerfs-node/src/call.rs), `bind` |
| A throwing function | caught; reported with `process.emitWarning(…, "LayerFSHandlerWarning")`; the call goes on | `bind` |
| Values | `Uint8Array` / `Buffer` → `{ base64 }`; `bigint` → number; `{ base64 }` back → `Buffer` | [`values.js`](../../../../api/node/src/values.js) |
| Outside the table | `doctor({home})` (runs off the JS thread; may dial running workers), `close()` (synchronous; closes idle Stores, answers how many) | [`index.js`](../../../../api/node/src/index.js), [`call.rs`](../../../../api/crates/layerfs-node/src/call.rs) |

The package is an ES module named `layerfs`, version 2.0.0, for Node 20 or
later; it ships `src` and `layerfs.node`
([`package.json`](../../../../api/node/package.json)). The workspace root
[`api/package.json`](../../../../api/package.json) (`layerfs-js`, private)
links it and holds the build script.

## Native addon build

```text
api/crates/layerfs-node (cdylib, napi 2.16, napi4)
  lib.rs   mod call; pub use call::{call, close, doctor, table, ...}
  call.rs  table()  -> layerfs_api::table_json()
           call()   -> AsyncTask Call: layerfs_api::call_json once
           close()  -> layerfs_api::close_idle(0)
           doctor() -> AsyncTask Doctor: layerfs_api::doctor_json
```

| Step | What happens | Source |
| --- | --- | --- |
| Build | `npm --prefix api run build:native` runs `cargo +1.85.1 build --manifest-path Cargo.toml --offline --locked [--release] -p layerfs-node`; `-- --debug` builds a dev profile | [`build-native.mjs`](../../../../api/scripts/build-native.mjs) |
| Host | `darwin` (`.dylib`) or `linux` (`.so`); any other platform, a set `CARGO_BUILD_TARGET`, or a `CARGO_TARGET_DIR` outside the worktree is refused | `build-native.mjs` |
| Output | copied to `api/crates/layerfs-node/layerfs.node`, or to `LAYERFS_NATIVE_OUTPUT`, through a staged file and a rename | `build-native.mjs` |
| Loading | first found of `api/node/layerfs.node`, a worktree bundle's root `layerfs.node`, then `api/crates/layerfs-node/layerfs.node`; none found throws | [`native.js`](../../../../api/node/src/native.js) |
| Features | `layerfs-api` with `machine` always; `postgresql` and `tls` only when the crate is built with them | [`Cargo.toml`](../../../../api/crates/layerfs-node/Cargo.toml) |

`LAYERFS_NATIVE_OUTPUT` is read by the build script only; the library
reads no other setting for the addon.

### Tests

Commands are written from source and were not run while writing this page.
Build the addon first; tests that compare with the command also need
`cargo +1.85.1 build --locked -p layerfs-cli` (they look for
`target/debug/layerfs`, or under `CARGO_TARGET_DIR`, and skip when it is
missing). Run one file at a time under a wall limit, for example
`perl -e 'alarm 100; exec @ARGV' node --test api/node/test/calls.mjs`.

| Test | Sets up | Checks | Needs |
| --- | --- | --- | --- |
| [`calls.mjs`](../../../../api/node/test/calls.mjs) | temporary homes, a folder with `README.md` | nine families and 62 functions; every call with a required argument is refused `InvalidInput` "is required"; empty lists; built-in machines `docker`, `native`; a Store, Init with progress, file and Branch calls, bytes, a folder scope | the addon |
| [`shell.mjs`](../../../../api/node/test/shell.mjs) | the `layerfs` debug binary beside the package | every call refused the same by the command and the package; exit 2 for a wrong line; a live Store read both ways; `--progress` lines | addon and command |
| [`events.mjs`](../../../../api/node/test/events.mjs) | a `layerfs worker` started on loopback | a throwing `on_progress` does not stop a `store.push`, and every event arrives before the call settles; exec output through functions and `stream: true` | command; the exec half needs `LAYERFS_DAEMON`, else skipped `NOT_RUN` |
| [`close.mjs`](../../../../api/node/test/close.mjs) | a Store and Init | `close()` drops this process's leases; a later call opens the Store again | the addon |
| [`doctor.mjs`](../../../../api/node/test/doctor.mjs) | missing and broken homes | `doctor` is outside the table, makes no home, names a record that does not parse and its fix | the addon |

## Related

- [Front ends](README.md): the three front ends side by side.
- [Table](../call-table/table.md): the JSON table the package builds from.
- [Errors](../call-table/errors.md#error-kinds): the codes `LayerFSError` carries.
- [Python](python.md): the same package shape for Python.

---

← [Front ends: CLI](cli.md) · [Up: Front ends](README.md) · [Front ends: Python](python.md) →
