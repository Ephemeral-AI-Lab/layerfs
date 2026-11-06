# Front ends

[Architecture](../../README.md) › [API](../README.md) › Front ends

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The `layerfs` command, the Node addon and the Python module: one call each, over the call table.

| | |
| --- | --- |
| Layer | [API](../README.md) |
| Crates | `layerfs-cli`, `layerfs-node`, `layerfs-python` |
| Source | [`api/crates/layerfs-cli/src`](../../../../api/crates/layerfs-cli/src), [`api/crates/layerfs-node/src`](../../../../api/crates/layerfs-node/src), [`api/crates/layerfs-python/src`](../../../../api/crates/layerfs-python/src), [`api/node`](../../../../api/node), [`api/python`](../../../../api/python) |
| Uses | [Call table](../call-table/README.md) |
| Used by | — |
| Flows | [A call, end to end](../../flows/call.md) |

## Topics

| Topic | What it covers |
| --- | --- |
| [CLI](cli.md) | The `layerfs` command. |
| [Node](node.md) | The Node package and its native addon. |
| [Python](python.md) | The Python package through PyO3. |

## The picture

```text
  shell words           JS function call         Python function call
       |                      |                         |
       v                      v                         v
+--------------+     +-----------------+       +------------------+
| CLI          |     | Node package    |       | Python package   |
| parse.rs     |     | index.js bind() |       | _client.py _bind |
| run.rs one() |     +-----------------+       +------------------+
+--------------+        | JSON text               | JSON text
       |                v                         v
       |         +-----------------+       +------------------+
       |         | layerfs-node    |       | layerfs-python   |
       |         | call.rs (napi)  |       | call.rs (PyO3)   |
       |         +-----------------+       +------------------+
       | SdkValue       | call_json               | call_json
       v                v                         v
+----------------------------------------------------------------+
| layerfs-api: call_scoped / call_json  (one call, made once)    |
|   find -> check -> scope -> home -> family run -> snake keys   |
+----------------------------------------------------------------+
       |                |                         |
       v events         v events (JSON)           v events (JSON)
  stderr / stdout   on_progress, stdout,     on_progress, stdout,
                    stderr functions         stderr functions
```

Every front end builds its functions or sub-commands from the same table
([`layerfs_api::families`](../../../../api/crates/layerfs-api/src/lib.rs),
or its JSON form `table_json`) and makes exactly one library call per
function call. Functions given as options never cross into the library:
the front end keeps them and feeds them the call's events. The steps
inside the library are on [Table](../call-table/table.md).

## What it does

| Front end | Turns its input into | Library entry | Answer | Source |
| --- | --- | --- | --- | --- |
| `layerfs` command | positionals in table order, then `--options`; content and JSON positionals from stdin or `--from` | `layerfs_api::call_scoped` | one JSON line on stdout, or the refusal as JSON on stderr | [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs), [`parse.rs`](../../../../api/crates/layerfs-cli/src/parse.rs) |
| Node `layerfs` | positionals in table order, then an options object | `native.call` → `layerfs_api::call_json` | a resolved value, or a thrown `LayerFSError` | [`index.js`](../../../../api/node/src/index.js), [`call.rs`](../../../../api/crates/layerfs-node/src/call.rs) |
| Python `layerfs` | positionals in table order; optional positionals and options as keywords | `_native.call` → `layerfs_api::call_json` | a dict, or a raised `LayerFSError`; `layerfs.aio` as coroutines | [`_client.py`](../../../../api/python/src/layerfs/_client.py), [`call.rs`](../../../../api/crates/layerfs-python/src/call.rs) |

| Event | Shape | CLI | Node and Python |
| --- | --- | --- | --- |
| progress (`project.init`, `store.push`, `store.pull`, `backup.create`, `backup.restore`) | `{ progress: … }` | `--progress`: one JSON line per event on stderr | the `on_progress` function |
| exec output (`workspace.exec`) | `{ stream, bytes }` | `--stream`: bytes to stdout or stderr | the `stdout` and `stderr` functions; `stream: true` without them writes to the process's own streams |

Events come from the call's one event stream
([`Context`](../../../../api/crates/layerfs-api/src/calls/context.rs)).
`workspace.exec` sends its output after the command ends: an inline stream
as one event ([`workspace/calls.rs`](../../../../control/crates/layerfs-sandbox/src/workspace/calls.rs)),
a stream over 1 MiB as 1 MiB pages from the host's spool, with the result
marked `streamed` ([`route.rs`](../../../../control/crates/layerfs-sandbox/src/route.rs) `drain`).

Entries outside the table: `doctor` in every front end (report-only, see
[Doctor](../../control/management/doctor.md)); `close` in Node and Python, which runs
`close_idle(0)` to close this process's idle Stores and drop their leases
(the CLI does the same after every call); and, in the CLI only, `worker`,
`import-env`, `help` and `--version`.

## Invariants

| Rule | Enforced by |
| --- | --- |
| Front ends add no behaviour of their own beyond argument and output shape. | Each function is built from the table and calls `call_json` or `call_scoped` once: [`index.js`](../../../../api/node/src/index.js) `bind`, [`_client.py`](../../../../api/python/src/layerfs/_client.py) `_bind`, [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs) `one` |
| No call, argument or result name is written in a front end. | [`api/AGENTS.md`](../../../../api/AGENTS.md): change the entry in `layerfs-api/src/<family>/<call>.rs` |
| Nothing is retried in a front end. | [`layerfs-node/src/lib.rs`](../../../../api/crates/layerfs-node/src/lib.rs), [`calls/text.rs`](../../../../api/crates/layerfs-api/src/calls/text.rs) |
| Argument checks are the library's, so a wrong call is refused the same way everywhere. | [`calls/check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs); compared in `shell.mjs` and `test_calls.py` |
| Functions (`on_progress`, `stdout`, `stderr`) stay in the front end. | `check` refuses a `Function` argument as unknown ([`check.rs`](../../../../api/crates/layerfs-api/src/calls/check.rs)) |
| Result keys are snake_case in every front end. | [`calls/keys.rs`](../../../../api/crates/layerfs-api/src/calls/keys.rs) |
| Bytes cross a language boundary as `{ "base64": … }`. | [`values.js`](../../../../api/node/src/values.js), [`_values.py`](../../../../api/python/src/layerfs/_values.py) |
| LayerFS reads `LAYERFS_*` settings only and names no plugin. | `LAYERFS_HOME` in [`dispatch.rs`](../../../../api/crates/layerfs-api/src/calls/dispatch.rs) `home`; `LAYERFS_WORKER_*` in [`worker.rs`](../../../../api/crates/layerfs-cli/src/worker.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| CLI command line wrong | exit 2, nothing called | [`run.rs`](../../../../api/crates/layerfs-cli/src/run.rs) `USAGE` |
| Call refused or failed | exit 125 and `{"code","message","detail"}` on stderr | [`output.rs`](../../../../api/crates/layerfs-cli/src/output.rs) |
| Node or Python: unknown option, too many positionals | `LayerFSError("InvalidInput")` before the native call | [`index.js`](../../../../api/node/src/index.js), [`_client.py`](../../../../api/python/src/layerfs/_client.py) |
| Node addon host | macOS or Linux only; `build-native.mjs` refuses other platforms and a set `CARGO_BUILD_TARGET` | [`build-native.mjs`](../../../../api/scripts/build-native.mjs) |
| Python | 3.10 or later (abi3), built by maturin | [`pyproject.toml`](../../../../api/python/pyproject.toml), [`Cargo.toml`](../../../../api/crates/layerfs-python/Cargo.toml) |
| Cargo features | Node: `machine` always on, `postgresql` and `tls` off by default. Python and CLI: `machine` by default, `postgresql` and `tls` off. Guest worker: `--no-default-features --features postgresql` | the three `Cargo.toml` files, [`tools/linux_bundle.py`](../../../../tools/linux_bundle.py) |
| Event delivery | every event reaches its function before the call settles; a function that throws does not stop the call | [`index.js`](../../../../api/node/src/index.js), [`call.rs`](../../../../api/crates/layerfs-python/src/call.rs) |
| Python `aio` cancel | stops the functions; the native call runs to its end and its outcome is dropped | [`aio.py`](../../../../api/python/src/layerfs/aio.py) |

Error codes are listed in [Errors](../call-table/errors.md#error-kinds).

## Demonstration

Commands are written from source and were not run while writing this page.
Give each a wall limit of at most 120 s.

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`stream.rs`](../../../../api/crates/layerfs-cli/tests/stream.rs) | `layerfs-cli` | `workspace exec --stream` passes output through and exits with the command's code |
| [`doctor.rs`](../../../../api/crates/layerfs-cli/tests/doctor.rs) | `layerfs-cli` | `layerfs doctor` reports, exits 0 and makes no home |
| [`test`](../../../../api/node/test) | `layerfs` (npm package) | the table as functions, library checks, events, `close`, `doctor`, same JSON as the command |
| [`test`](../../../../api/python/test) | `layerfs` (Python package) | the same for Python, plus `aio` |

- **`stream.rs`**: a refusal with `--stream` is still exit 125. With a
  native daemon it makes a Store, Init, a Branch, a Sandbox on `native` and
  a Workspace, then checks that `exit 3` exits 3 with stdout and stderr
  passed through, `true` exits 0, and without `--stream` the exit is 0 with
  `"exit_code":4` in the JSON. Command:
  `cargo +1.85.1 test --locked -p layerfs-cli --test stream`. Needs Unix;
  the live half needs `LAYERFS_DAEMON` (a daemon binary), else it prints
  `NOT_RUN` and passes.
- **`doctor.rs`**: `layerfs doctor` on a missing home prints
  `{"ok":true,"findings":[]}` and does not create it; a Store record that
  is not JSON gives `1 error(s), 0 warning(s)` and its fix with `--text`;
  `doctor --all` is exit 2. Command:
  `cargo +1.85.1 test --locked -p layerfs-cli --test doctor`. Needs Unix.
- **Node tests**: `calls.mjs`, `shell.mjs`, `events.mjs`, `close.mjs`,
  `doctor.mjs`; see [Node](node.md#native-addon-build). Command:
  `perl -e 'alarm 100; exec @ARGV' npm --prefix api/node test`
  (`node --test test/*.mjs`). Needs the addon built first.
- **Python tests**: `test_calls.py`, `test_doctor.py`; see
  [Python](python.md#package). Command:
  `perl -e 'alarm 100; exec @ARGV' python3 api/python/test/test_calls.py`.
  Needs the module built and copied beside the package.

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`api/README.md`](../../../../api/README.md): covered in [Node](node.md) and [CLI](cli.md).
- [`api/python/README.md`](../../../../api/python/README.md): covered in [Python](python.md).
- [`docs/versioned/2.0.0/sdk.md`](../../../versioned/2.0.0/sdk.md): the user manual for the packages; its design points are covered in [Node](node.md) and [Python](python.md).

---

← [backup.restore](../sdk/backup/restore.md) · [Up: API](../README.md) · [Front ends: CLI](cli.md) →
