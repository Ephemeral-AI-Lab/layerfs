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
  native.js   loads layerfs.node (the package's own, or LAYERFS_NATIVE)
  values.js   encode / decode: JSON text, whole numbers exactly,
              bytes as {"base64": ...}
  errors.js   LayerFSError { code, message, detail }
  index.d.ts  the typings, generated from the call table
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
| Values | `Uint8Array` / `Buffer` → `{ base64 }`; `bigint` → its exact digits; whole numbers back as `number` (safe integers) or `bigint`; `{ base64 }` back → `Buffer`; see [Values](#values) | [`values.js`](../../../../api/node/src/values.js) |
| Outside the table | `doctor({home})` (runs off the JS thread; may dial running workers), `close()` (synchronous; closes idle Stores, answers how many) | [`index.js`](../../../../api/node/src/index.js), [`call.rs`](../../../../api/crates/layerfs-node/src/call.rs) |

The package is an ES module named `layerfs`, version 2.0.0, MIT licensed
(the repository's [`LICENSE`](../../../../LICENSE), copied into the package),
for Node 22 or later; it ships `src` (with `index.d.ts`), `layerfs.node` and
`LICENSE` ([`package.json`](../../../../api/node/package.json)). Node 22 is
the floor because exact whole numbers use `JSON.rawJSON` and the source text
of `JSON.parse` (Node 20 lacks both and is end of life). The workspace root
[`api/package.json`](../../../../api/package.json) (`layerfs-js`, private)
links it and holds the build script.

## Values

The generated `Value` input type accepts `Uint8Array` (including `Buffer`)
inside JSON arguments, including the content of a `file.apply` change.

| Direction | Value | Behaviour |
| --- | --- | --- |
| In | `bigint` | written as its exact digits, in any position: `2n ** 60n + 1n`, `-(2n ** 63n)` arrive unchanged. The library reads a whole number as a signed 64-bit integer and refuses a larger one (`InvalidInput`, "number out of range") |
| In | `number` | a safe integer (`Number.isSafeInteger`) or a fraction is sent as is (the library refuses a fraction); `NaN`, `±Infinity` and an integer beyond `±(2^53 - 1)` are refused by the package, `InvalidInput`, before any call: send a `bigint` |
| In | `Map`, `Set`, `ArrayBuffer`, `SharedArrayBuffer`, a typed array other than `Uint8Array`, `DataView`, a function, a symbol, a cycle | refused, `InvalidInput`, before any call |
| In | text, or an object key, with a lone surrogate | refused, `InvalidInput`, before any call |
| In | `Uint8Array`, `Buffer` | `{ "base64": "…" }`; empty bytes are `{ "base64": "" }` |
| In | `null`, `undefined` | a positional or option is left out (see above); inside a JSON value, `undefined` in an object is dropped and in an array is `null`, as `JSON.stringify` does |
| Out | whole number | a `number` when it is a safe integer, else a `bigint`, exactly; a number written with a fraction or exponent is a `number` |
| Out | `{ "base64": "…" }` | a `Buffer`. The library prints bytes as a string when they are UTF-8 and as `{ "base64" }` when they are not, so exec output (`stdout`/`stderr` functions) is a `string` or a `Buffer` |

The key `base64` is reserved: a map whose only key is `base64`, with a text
that is base64, is read as bytes by the library and by the package, both
ways. A map with another key beside it, or a non-base64 text, stays a map. As
the `content` positional of `file.write` a plain object is taken for the
options object (`unknown option "base64"`); a one-key map is bytes inside a
JSON value such as the `content` of a `file.apply` change.

## Typings

[`index.d.ts`](../../../../api/node/src/index.d.ts) is generated from the
exported call table by
[`gen-node-types.mjs`](../../../../api/scripts/gen-node-types.mjs) (`npm
--prefix api run gen:types`; `--check` only compares): per family an
interface with each call's positionals typed by kind (`project`, `name`,
`text` → `string`; `int` → `number | bigint`; `size` → `number | bigint |
string`; `content` → `string | Uint8Array`; `json` → `Value`; an optional
positional also takes `null`), then an options object of the call's own
options; `on_progress`, `stdout` and `stderr` are typed where a call declares
them. `Client`, `Scope`, `LayerFSError`, `doctor`, `close`, `table` and the
nine family exports are declared. The table has no result schema, so every
call answers `Promise<Result>`, a map of snake_case keys to the loose `Value`
type (JSON plus `bigint` and `Buffer`); the call's SDK page lists the keys.
The file references `@types/node` for `Buffer`. Do not edit it by hand;
`test/package.mjs` fails when it differs from the table, and
[`test/types/sample.ts`](../../../../api/node/test/types/sample.ts) is
checked with `tsc --noEmit` (see Tests).

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
| Build | `npm --prefix api run build:native` runs `cargo +1.85.1 build --manifest-path Cargo.toml --offline --locked [--release] -p layerfs-node`; `-- --debug` builds a dev profile; `-- --features postgresql,tls` (either name or both, nothing else) passes the opt-in features on, default none | [`build-native.mjs`](../../../../api/scripts/build-native.mjs) |
| Host | `darwin` (`.dylib`) or `linux` (`.so`); any other platform, a set `CARGO_BUILD_TARGET`, or a `CARGO_TARGET_DIR` outside the worktree is refused | `build-native.mjs` |
| Output | copied to `api/node/layerfs.node` (git-ignored; the package's own addon), or to `LAYERFS_NATIVE_OUTPUT`, through a staged file and a rename | `build-native.mjs` |
| Loading | the file `LAYERFS_NATIVE` names, else `layerfs.node` beside the package's `src`; nothing else is looked at. A missing file, or one that does not load, throws one error naming the path, `platform/arch`, the Node version and how to build | [`native.js`](../../../../api/node/src/native.js) |
| Features | `layerfs-api` with `machine` always; `postgresql` and `tls` only when the crate is built with them | [`Cargo.toml`](../../../../api/crates/layerfs-node/Cargo.toml) |

`LAYERFS_NATIVE_OUTPUT` is read by the build script only (it prints the
`LAYERFS_NATIVE` to use when the output is elsewhere); `LAYERFS_NATIVE` is
read by the loader only. The package no longer probes the source tree or a
worktree bundle's root: a bundle's runner sets `LAYERFS_NATIVE` to the
bundle's `layerfs.node` (`tools/worktree_run.py`).

### Tests

Build the addon first (`node api/scripts/build-native.mjs --debug` puts it
at `api/node/layerfs.node`, where the tests find it; or set `LAYERFS_NATIVE`
to a built addon). The temporary directory must be on a filesystem with
SQLite locks (ext4, xfs, btrfs; set `TMPDIR` if `/tmp` is not). Tests that
compare with the command also need
`cargo +1.85.1 build --locked -p layerfs-cli` (they look for
`target/debug/layerfs`, or under `CARGO_TARGET_DIR`, and skip when it is
missing). Run one file at a time under a wall limit, for example
`perl -e 'alarm 100; exec @ARGV' node --test api/node/test/calls.mjs`.

| Test | Sets up | Checks | Needs |
| --- | --- | --- | --- |
| [`calls.mjs`](../../../../api/node/test/calls.mjs) | temporary homes, a folder with `README.md` | nine families and 61 functions; every call with a required argument is refused `InvalidInput` "is required"; empty lists; built-in machines `docker`, `native`; a Store, Init with progress, file and Branch calls, bytes, a folder scope | the addon |
| [`shell.mjs`](../../../../api/node/test/shell.mjs) | the `layerfs` debug binary beside the package | every call refused the same by the command and the package; exit 2 for a wrong line; a live Store read both ways; `--progress` lines | addon and command |
| [`events.mjs`](../../../../api/node/test/events.mjs) | a `layerfs worker` started on loopback | a throwing `on_progress` does not stop a `store.push`, and every event arrives before the call settles; exec output through functions and `stream: true` | command; the exec half needs `LAYERFS_DAEMON`, else skipped `NOT_RUN` |
| [`close.mjs`](../../../../api/node/test/close.mjs) | a Store and Init | `close()` drops this process's leases; a later call opens the Store again | the addon |
| [`doctor.mjs`](../../../../api/node/test/doctor.mjs) | missing and broken homes | `doctor` is outside the table, makes no home, names a record that does not parse and its fix | the addon |
| [`values.mjs`](../../../../api/node/test/values.mjs) | `encode` / `decode` alone, then a temporary home | exact `bigint` in and out (`2n ** 60n + 1n`, i64 min and max, `2 ** 53 + 1`, negatives, nested); unsafe numbers, `NaN`, `Map`, `Set`, buffers, typed arrays, functions and lone surrogates refused; empty, binary, Unicode and one-key-map bytes. No call answers a number beyond 2^53, so decoding is tested on JSON text; in a live call, i64 max reaches the library exactly | the addon |
| [`native.mjs`](../../../../api/node/test/native.mjs) | a copy of `src` with decoy addons in the old fallback places | a missing addon is one clear error naming path and platform; no decoy is tried; `LAYERFS_NATIVE` wins over the package's own | the addon |
| [`package.mjs`](../../../../api/node/test/package.mjs) | the package files | licence MIT and `LICENSE` equal to the repository's; `index.d.ts` equals what the table generates and covers every call, argument and option; `npm pack --dry-run` lists source, typings and licence, not tests; `engines` is `>=22` | the addon, `npm` |

The typings sample is checked outside the repository (TypeScript is not a
dependency): install `typescript` and `@types/node` in a temporary directory
and run `tsc --noEmit --strict --module nodenext --moduleResolution nodenext
--target es2022 --types node --typeRoots <dir>/node_modules/@types
api/node/test/types/sample.ts`; its `@ts-expect-error` lines must each be an
error. [`examples/file-history.mjs`](../../../../api/node/examples/file-history.mjs)
is a runnable mountless flow (Store, project, Branch, writes, history, diff,
publish) in a temporary home.

## Related

- [Front ends](README.md): the three front ends side by side.
- [Table](../call-table/table.md): the JSON table the package builds from.
- [Errors](../call-table/errors.md#error-kinds): the codes `LayerFSError` carries.
- [Python](python.md): the same package shape for Python.

---

← [Front ends: CLI](cli.md) · [Up: Front ends](README.md) · [Front ends: Python](python.md) →
