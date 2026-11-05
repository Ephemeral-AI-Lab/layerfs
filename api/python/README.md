# The `layerfs` Python package

> **Status:** Current implementation guide for LayerFS 2.0 (#533 step 16);
> not a release receipt.

The same ten families as the [JavaScript package](../README.md), from the
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
be keywords. Results are dicts with snake_case keys; file content is `str`
or `bytes`. A refusal raises `layerfs.LayerFSError` with `.code`, `.message`
and `.detail`. Function options (`on_progress`, `stdout`, `stderr`) receive
the call's events.

## Build and test

```bash
cd api/python && maturin build --locked
```

For the tests, build the module and the command with Cargo and put the
module beside the package:

```bash
cargo +1.85.1 build -p layerfs-python -p layerfs-cli
cp target/debug/lib_native.dylib api/python/src/layerfs/_native.abi3.so
perl -e 'alarm 100; exec @ARGV' python3 api/python/test/test_calls.py
```

(`lib_native.so` on Linux.) The tests compare every call with the
`layerfs` command, which `api/node/test/shell.mjs` compares with Node.

The test command runs under a wall limit of at most 120 s (here 100 s;
macOS has no `timeout`, so `perl`'s `alarm` ends it).
