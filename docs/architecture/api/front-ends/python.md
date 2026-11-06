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
| Answer | JSON text `{ value }` or `{ refused: { code, message, detail } }` | [`calls/text.rs`](../../../../control/crates/layerfs-control/src/calls/text.rs) |
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
  py.typed, *.pyi   the type stubs (see Types)
api/python/tools/gen_stubs.py   writes _client.pyi, aio.pyi and __init__.pyi from the table
```

One function call ([`_client.py`](../../../../api/python/src/layerfs/_client.py) `_bind`):

```text
lfs.file.read(p, b, path, start_line=2)
  |  too many positionals, a positional given twice, an unknown
  |  keyword, a function option that is not callable, a value JSON
  |  cannot hold (NaN, Infinity, a lone surrogate, a non-text map
  |  key, a type with no JSON form), a home or scope with a lone surrogate
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
| Values | `bytes`, `bytearray`, `memoryview` → `{ base64 }`; a one-key `{ base64 }` map with base64 text back → `bytes` (any other map stays a `dict`) | [`_values.py`](../../../../api/python/src/layerfs/_values.py) |
| Refused before any call | `encode` raises `LayerFSError("InvalidInput")` for NaN and Infinity, a lone surrogate (also in a home or scope), a map key that is not text and a type JSON has no form for (a set, an object, a cycle); before this, a lone surrogate raised `UnicodeEncodeError` and a non-JSON value `TypeError` | `_values.py` `encode`, `path_text` |
| Integers | Python `int` is exact both ways. The library reads a number as a signed 64-bit integer: past that (`2**63`, `-2**63 - 1`) it refuses with `InvalidInput` "number out of range"; a fraction is refused "only whole numbers are accepted"; `True` is not a whole number. A call may be stricter than the parser: the line and page options refuse anything above 2^53 - 1 ("safe integer"), though `2**63 - 1` and `-2**63` reach the call as written | [`json.rs`](../../../../control/crates/layerfs-home/src/json.rs) |
| Bytes | an empty, binary, non-UTF-8 or UTF-8 `bytes` is stored as given (`file.write`); `file.read` returns text only and refuses NUL or invalid UTF-8 content. The reserved form is ambiguous with a map: `{"base64": "AP8B"}` passed as `content` is the bytes `00 ff 01`, not a map, and the text `"AP8B"` stays text; a one-key map whose text is not base64 stays a map (`file.write` refuses it as not text or bytes). Exec output (events `stdout`, `stderr`) arrives as `str` when it is UTF-8 and as `bytes` when it is not | `_values.py`, [`json.rs`](../../../../control/crates/layerfs-home/src/json.rs) |
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

### Types

`py.typed` and the `.pyi` stubs ship in the wheel and the source distribution
(maturin includes every file of the package directory). Hand written:
`_values.pyi` (`JSON`, `Content`, `Size`, `Progress`, `Output`, `Result`,
`Scope`, `DoctorReport`) and `_errors.pyi`. Generated from the call table by
[`tools/gen_stubs.py`](../../../../api/python/tools/gen_stubs.py):

| Stub | Content |
| --- | --- |
| `_client.pyi` | one class per family (`ProjectFamily` ...), one method per call: positionals as the table lists them (an optional one `X \| None = None`), options keyword-only; `Client`, `client()`, `TABLE`, `FAMILIES` |
| `aio.pyi` | the same as `async def`; `AsyncClient`, `client()`, the default families |
| `__init__.pyi` | the default client's families, `doctor() -> DoctorReport`, `close() -> int`, `LayerFSError` |

By argument kind: `name`, `project`, `text` → `str`; `int` → `int`; `bool` →
`bool`; `size` → `int | str`; `content` → `str | bytes | bytearray | memoryview`;
`json` → `JSON`; function options may be omitted, but an explicitly supplied `None` is refused;
the function options are `on_progress: Callable[[Progress],
object]` and `stdout`, `stderr: Callable[[str | bytes], object]`. **Results are
loose**: the table has argument kinds and no result schema (and the Rust table
is not extended here), so every call returns `Result`, an alias of `Any` for the
JSON value the call answers (some calls answer `null` or a list). `Progress` is
`Mapping[str, Any]`: the progress payload has more than one shape. The types
exist only for type checkers; import them under `TYPE_CHECKING`.

The distribution is named `layerfs`, version 2.0.0, Python 3.10 or later,
built by maturin 1.9.6 or a later 1.x from the crate's manifest as
`layerfs._native` ([`pyproject.toml`](../../../../api/python/pyproject.toml)).
Its licence is MIT (`license = "MIT"`, metadata `License-Expression: MIT`)
and `api/python/LICENSE`, a copy of the repository's, is in the wheel
(`dist-info/licenses/LICENSE`) and the source distribution
(`license-files = ["LICENSE"]`; maturin refuses a path above the project
directory, so the file is copied). Its build command, from
[`api/python/README.md`](../../../../api/python/README.md),
is `cd api/python && maturin build --locked`.

### Tests

Build first: `cargo +1.85.1 build --locked -p layerfs-python -p layerfs-cli`,
then copy `target/debug/lib_native.dylib` (`.so` on Linux) to
`api/python/src/layerfs/_native.abi3.so` (or `maturin develop --locked
--offline` in a virtualenv). Run under a wall limit:
`perl -e 'alarm 100; exec @ARGV' python3 -m unittest discover -s api/python/test`.
The temp homes need a filesystem with SQLite locks (ext4, xfs, btrfs, not
tmpfs): point `TMPDIR` at one.

| Test | Checks | Needs |
| --- | --- | --- |
| [`test_calls.py`](../../../../api/python/test/test_calls.py) `test_the_table` | nine families and 61 calls as functions; an unknown keyword is `InvalidInput` | the module |
| `test_every_call_is_refused_as_the_command_refuses_it` | every call gives the same refusal as the `layerfs` command | the command, else skipped |
| `test_a_live_store_reads_the_same_both_ways` | a Store read through the package and the command gives the same JSON | the command, else skipped |
| `test_aio_makes_the_same_calls_as_coroutines`, `test_aio_functions_run_on_the_loop_before_the_call_returns`, `test_a_cancelled_aio_call_stops_its_functions_and_the_call_runs_to_its_end` | the `aio` rules above | the module |
| `test_exec_output_through_functions_and_stream_without_them` | exec output through `stdout`/`stderr` and `stream=True` | `LAYERFS_DAEMON` and the command, else skipped `NOT_RUN` |
| [`test_values.py`](../../../../api/python/test/test_values.py) | a lone surrogate (in an argument, an option, a home, a scope), NaN, Infinity and non-JSON values are `InvalidInput` and never reach the library (`_native.call` replaced by a function that fails); integers past 64 bits, `i64` min and max, a bool; bytes (empty, binary, Unicode, `bytearray`, `memoryview`) and the one-key `{"base64"}` map; ten malformed scopes | the module |
| [`test_flow.py`](../../../../api/python/test/test_flow.py) | a mountless file and history flow (init, fork, mkdir, write, edit, cp, mv, rm, then reads, a page, three scoped refusals) written through `aio` and read through both packages gives equal answers and equal refusals | the module |
| [`test_stubs.py`](../../../../api/python/test/test_stubs.py) | `py.typed` exists; the committed stubs equal what `gen_stubs.py` renders from the table (61 sync methods, 61 `async` methods); `api/python/LICENSE` equals the repository's; `mypy --strict` accepts `typed_samples/ok.py` (sync and async) and reports exactly the lines of `typed_samples/bad.py` marked `# E:` | the module; mypy, else the last two are skipped |
| [`test_doctor.py`](../../../../api/python/test/test_doctor.py) | `close()` is outside the table; `doctor` on a missing home is ok and makes none; a record that does not parse is an error with its fix | the module |

## Related

- [Front ends](README.md): the three front ends side by side.
- [Node](node.md): the same package shape for Node.
- [Table](../call-table/table.md): the JSON table the package builds from.
- [Errors](../call-table/errors.md#error-kinds): the codes `LayerFSError` carries.

---

← [Front ends: Node](node.md) · [Up: Front ends](README.md) · [Flows](../../flows/README.md) →
