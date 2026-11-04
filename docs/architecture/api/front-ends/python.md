# Python

[Architecture](../../README.md) › [API](../README.md) › [Front ends](README.md) › Python

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The Python package through PyO3.

| | |
| --- | --- |
| Component | [Front ends](README.md) |
| Source | [`api/crates/layerfs-python/src`](../../../../api/crates/layerfs-python/src), [`api/python`](../../../../api/python) |

## PyO3 module

```text
api/crates/layerfs-python (cdylib "_native", PyO3 0.30, abi3-py310)
  lib.rs   #[pymodule] _native: table, call, close, doctor
  call.rs
    table()                         -> layerfs_api::table_json()
    call(name, args, home=None,     -> py.detach: layerfs_api::call_json
         scope=None, on_event=None)     once; each event:
                                        Python::attach -> on_event(text)
    close()                         -> py.detach: close_idle(0)
    doctor(home=None)               -> py.detach: doctor_json(home)
```

| Rule | Behaviour | Source |
| --- | --- | --- |
| Interpreter | released (`py.detach`) for the whole library call, so other Python threads run | [`call.rs`](../../../../api/crates/layerfs-python/src/call.rs) |
| Events | each event re-takes the interpreter to call `on_event` with JSON text; an exception it raises is written as unraisable and the call goes on | `call.rs` |
| Answer | JSON text `{ value }` or `{ refused: { code, message, detail } }` | [`calls/text.rs`](../../../../api/crates/layerfs-control/src/calls/text.rs) |
| Features | `machine` by default; `postgresql` and `tls` only when built with them | [`Cargo.toml`](../../../../api/crates/layerfs-python/Cargo.toml) |
| Linking | `pyo3-build-config` gives `-undefined dynamic_lookup` on macOS, so a plain Cargo build links like maturin | `Cargo.toml` |

## Package

```text
api/python/src/layerfs/
  __init__.py   doctor(), close(), the nine families of the default client
  _client.py    TABLE, FAMILIES, Client, client(), _bind (one call)
  aio.py        AsyncClient: the same calls as coroutines
  _values.py    encode / decode: JSON text, bytes as {"base64": ...}
  _errors.py    LayerFSError(code, message, detail)
```

One function call ([`_client.py`](../../../../api/python/src/layerfs/_client.py) `_bind`):

```text
lfs.file.read(p, b, path, start_line=2)
  |  too many positionals, a positional given twice, an unknown
  |  keyword, a function option that is not callable
  |       --> raise LayerFSError("InvalidInput")   (no native call)
  |  None values left out; functions kept as handlers
  v
_native.call(name, encode(args), home, encode(scope), on_event)
  |  events: progress -> on_progress; stream -> stdout / stderr
  v
decode(answer): refused -> raise LayerFSError ; else answer["value"]
```

| Rule | Behaviour | Source |
| --- | --- | --- |
| Building | `TABLE = decode(_native.table())`; `Client(home, scope)` sets one `Family` attribute per family and one function per entry, with the entry's `help` as its docstring | `_client.py` |
| Defaults | module-level `layerfs.project` … `layerfs.backup` belong to `Client()`: the library chooses the home (`LAYERFS_HOME`, else `~/.layerfs`) | [`__init__.py`](../../../../api/python/src/layerfs/__init__.py) |
| Keywords | optional positionals and options may be keywords: `file.read(p, b, path, start_line=2)` | `_client.py` |
| `stream=True` | without `stdout` / `stderr` functions, output goes to `sys.stdout` / `sys.stderr` (text to the text stream, bytes to its buffer) | `_client.py` `_streamed` |
| Values | `bytes`, `bytearray`, `memoryview` → `{ base64 }`; `{ base64 }` back → `bytes` | [`_values.py`](../../../../api/python/src/layerfs/_values.py) |
| Outside the table | `doctor(home=None)` and `close()` | `__init__.py` |

`layerfs.aio` ([`aio.py`](../../../../api/python/src/layerfs/aio.py)):

```text
event loop thread                    worker thread (asyncio.to_thread)
-----------------                    ---------------------------------
await aio.store.push("s", "m",
        on_progress=f)
  wrap f: call_soon_threadsafe ---->  sync call -> _native.call (detached)
  <------------- f(progress) queued      each event -> marshal(f)
  <------------- result
  live = False (functions stop)
```

| Case | Behaviour |
| --- | --- |
| normal return | every queued function call runs on the loop before the coroutine returns |
| coroutine cancelled | the functions stop at once; the native call cannot be cancelled, runs to its end and its outcome is dropped |
| loop closed | delivery stops; nothing is left to deliver to |

The distribution is named `layerfs`, version 2.0.0, Python 3.10 or later,
built by maturin 1.9.6 or a later 1.x from the crate's manifest as
`layerfs._native` ([`pyproject.toml`](../../../../api/python/pyproject.toml)).
Its build command, from [`api/python/README.md`](../../../../api/python/README.md),
is `cd api/python && maturin build --locked`.

### Tests

Commands are written from source and were not run while writing this page.
Build first: `cargo +1.85.1 build --locked -p layerfs-python -p layerfs-cli`,
then copy `target/debug/lib_native.dylib` (`.so` on Linux) to
`api/python/src/layerfs/_native.abi3.so`. Run each file under a wall limit:
`perl -e 'alarm 100; exec @ARGV' python3 api/python/test/test_calls.py`.

| Test | Checks | Needs |
| --- | --- | --- |
| [`test_calls.py`](../../../../api/python/test/test_calls.py) `test_the_table` | nine families and 61 calls as functions; an unknown keyword is `InvalidInput` | the module |
| `test_every_call_is_refused_as_the_command_refuses_it` | every call gives the same refusal as the `layerfs` command | the command, else skipped |
| `test_a_live_store_reads_the_same_both_ways` | a Store read through the package and the command gives the same JSON | the command, else skipped |
| `test_aio_makes_the_same_calls_as_coroutines`, `test_aio_functions_run_on_the_loop_before_the_call_returns`, `test_a_cancelled_aio_call_stops_its_functions_and_the_call_runs_to_its_end` | the `aio` rules above | the module |
| `test_exec_output_through_functions_and_stream_without_them` | exec output through `stdout`/`stderr` and `stream=True` | `LAYERFS_DAEMON` and the command, else skipped `NOT_RUN` |
| [`test_doctor.py`](../../../../api/python/test/test_doctor.py) | `close()` is outside the table; `doctor` on a missing home is ok and makes none; a record that does not parse is an error with its fix | the module |

## Related

- [Front ends](README.md): the three front ends side by side.
- [Node](node.md): the same package shape for Node.
- [Table](../call-table/table.md): the JSON table the package builds from.
- [Errors](../call-table/errors.md#error-kinds): the codes `LayerFSError` carries.

---

← [Front ends: Node](node.md) · [Up: Front ends](README.md) · [Flows](../../flows/README.md) →
