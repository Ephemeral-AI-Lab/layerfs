# The `layerfs` Python package

> **Status:** Current implementation guide for LayerFS 2.0 (#533 step 16);
> not a release receipt.

The same nine families as the [JavaScript package](../README.md), from the
same call table, through the PyO3 module `layerfs._native`
(`api/crates/layerfs-python`). Calls are synchronous; `layerfs.aio` has
the same calls as coroutines (O19).

```python
import layerfs

layerfs.store.create("shop")
layerfs.project.init("shop/api", "/src/api", on_progress=print)
layerfs.branch.fork("shop/api", "agent", "main")
layerfs.file.write("shop/api", "agent", "/README.md", "hello\n", message="docs")
text = layerfs.file.read("shop/api", "agent", "/README.md")["text"]

lfs = layerfs.client(home="/data/lfs", scope={"project": "shop/api", "folder": "/web", "branch": "agent"})
```

Positionals follow the table's order; optional positionals and options may
be keywords. Results are JSON values (dicts with snake_case keys for most
calls); file content is `str` or `bytes`. A refusal raises `layerfs.LayerFSError` with `.code`, `.message`
and `.detail`. Function options (`on_progress`, `stdout`, `stderr`) receive
the call's events.

A value JSON cannot hold (NaN, Infinity, a lone surrogate, a map with a
non-text key, a type with no JSON form) raises `LayerFSError("InvalidInput")`
before any call, like a number past 64 bits or a fraction.

## Types

The package ships `py.typed` and `.pyi` stubs for `layerfs` and `layerfs.aio`:
every call has typed keyword arguments (`on_progress`, `stdout` and `stderr`
where the call declares them) and `aio` has the coroutine variants. The call
table has argument kinds and no result schema, so every call returns the loose
`Result` (a JSON value: a dict for most calls). `Scope`, `DoctorReport`,
`Progress`, `JSON` and `Result` are for annotations only (import them under
`TYPE_CHECKING`; they are not defined at run time). The stubs of the families
are generated from the table; after the table changes, rewrite them with the
module built:

```bash
python api/python/tools/gen_stubs.py          # --check fails on drift
```

## Build and test

```bash
cd api/python && maturin build --locked
```

For the tests, either install the module in a virtualenv (`maturin develop
--locked --offline` puts `_native.abi3.so` beside the package) or build it
with Cargo and put it there:

```bash
cargo +1.85.1 build -p layerfs-python -p layerfs-cli
cp target/debug/lib_native.dylib api/python/src/layerfs/_native.abi3.so
perl -e 'alarm 100; exec @ARGV' python3 -m unittest discover -s api/python/test
```

(`lib_native.so` on Linux.) The tests compare every call with the
`layerfs` command, which `api/node/test/shell.mjs` compares with Node. The
temp homes need a filesystem with SQLite locks (ext4, xfs, btrfs; not
tmpfs): point `TMPDIR` at one. `test_stubs.py` runs `mypy --strict` over
`test/typed_samples` when mypy is installed.

The test command runs under a wall limit of at most 120 s (here 100 s;
macOS has no `timeout`, so `perl`'s `alarm` ends it).
