# LayerFS API packages

> **Status:** Current implementation guide for LayerFS 2.0.0. The calling
> conventions, every call and the front ends are documented in
> [docs/architecture/api](../docs/architecture/api/README.md).

`api/` holds the JavaScript package `@layerfs/node` (in [`node/`](node)), the
CLI package `@layerfs/cli` (in [`cli/`](cli)), the Python package (in
[`python/`](python/README.md)), the scripts that build and generate them, and
the private stdio MCP server [`mcp/`](mcp) (`@layerfs/mcp`: one generated tool
per call, `work` and `admin` profiles). The design is in
[docs/architecture/api](../docs/architecture/api/README.md). The package is the
library's call table as ten families of functions, 73 calls in all:

| Family | Calls |
| --- | --- |
| [`project`](../docs/architecture/api/sdk/project.md) | `init`, `list`, `get`, `stats`, `export`, `remove` |
| [`branch`](../docs/architecture/api/sdk/branch.md) | `list`, `get`, `commits`, `get_commit`, `fork`, `merge`, `publish`, `diff` |
| [`commit`](../docs/architecture/api/sdk/commit.md) | `list`, `get`, `diff` |
| [`file`](../docs/architecture/api/sdk/file.md) | `read`, `stat`, `list`, `glob`, `grep`, `revision`, `blame`, `diff`, `write`, `edit`, `rm`, `mkdir`, `mv`, `cp`, `apply` |
| [`sandbox`](../docs/architecture/api/sdk/sandbox.md) | `create`, `list`, `get`, `close` |
| [`workspace`](../docs/architecture/api/sdk/workspace.md) | `mount`, `exec`, `exec_cancel`, `commit`, `unmount`, `status` |
| [`machine`](../docs/architecture/api/sdk/machine.md) | `list`, `platforms`, `get`, `status`, `start`, `stop`, `create`, `remove`, `update` |
| [`store`](../docs/architecture/api/sdk/store.md) | `list`, `get`, `status`, `stats`, `create`, `connect`, `attach`, `detach`, `push`, `pull`, `remove`, `disconnect` |
| [`backup`](../docs/architecture/api/sdk/backup.md) | `connect`, `disconnect`, `list`, `create`, `restore` |
| [`logs`](../docs/architecture/api/sdk/logs.md) | `list`, `reset` |

`layerfs.table` lists every call with its arguments and options; it is the
source for the list above, for the `layerfs` command (`layerfs help`) and for
the Python package, which has the same families. Each call's arguments,
answer and errors are in the [SDK reference](../docs/architecture/api/sdk/README.md).

```js
import layerfs from "@layerfs/node";

await layerfs.store.create("shop");
await layerfs.project.init("shop/api", "/src/api", { on_progress: console.log });
await layerfs.branch.fork("shop/api", "agent", "main");
await layerfs.file.write("shop/api", "agent", "/README.md", "hello\n");

// another home, and a folder scope bound to one project: paths stay in
// its folder and Branches are relative to its namespace
const lfs = layerfs.client({ home: "/data/lfs", scope: { project: "shop/api", folder: "/web", branch: "agent" } });
```

Each function takes its positional arguments in the table's order and then an
options object. Function options (`on_progress`, `stdout`, `stderr`) receive
the call's events. A refusal throws `LayerFSError` with `code`, `message` and
`detail`; results are plain objects with snake_case keys, and bytes are
`Buffer`s. Whole numbers are exact: a `bigint` is sent as its digits, and a
result is a `number` when it is a safe integer and a `bigint` when it is not;
an unsafe `number`, `NaN`, a `Map`, a `Set` or text with a lone surrogate is
refused (`InvalidInput`) before the call. The package needs Node 22 and ships
TypeScript typings generated from the call table. Where a call runs (in this
process, or in a machine's worker) is decided by the home's records, not by
the package. A runnable flow is
[`node/examples/file-history.mjs`](node/examples/file-history.mjs).

## Builds

Run `npm --prefix api run build:native` on the macOS/Linux host that loads the
addon. It uses Cargo +1.85.1, `--offline --locked --release`, and the repository
ARM64 inputs. `-- --debug` selects dev. The artifact is atomically renamed into
place at `node/layerfs.node`, beside the package's `src`. `CARGO_TARGET_DIR` must
stay inside this worktree; `LAYERFS_NATIVE_OUTPUT` can select a separate platform
artifact. The package loads that file, or the one `LAYERFS_NATIVE` names, and
nothing else: it does not look in the source tree, and a missing addon is an
error that says how to build it. `npm --prefix api run gen:types` regenerates
`node/src/index.d.ts` after a change to the call table.

### Package and install

Each artifact is for the OS, CPU and C library it was built on (a Linux build
needs at least the glibc of its build host). Install the files you build, here
into `$OUT`; publishing is in
[front ends: packaging and release](../docs/architecture/api/front-ends/README.md#packaging-and-release):

```bash
npm --prefix api run build:native                           # node/layerfs.node
(cd api/node && npm pack --pack-destination "$OUT")         # layerfs-node-2.0.0-rc.1.tgz
npm install "$OUT/layerfs-node-2.0.0-rc.1.tgz"                   # in an empty project

(cd api/python && maturin build --release --locked -o "$OUT")   # layerfs-2.0.0-*.whl
pip install "$OUT"/layerfs-2.0.0-*.whl                          # in a virtualenv

cargo +1.85.1 build --release --locked -p layerfs-cli       # target/release/layerfs, one file
```

`npm pack` fails when `node/layerfs.node` is missing, so a tarball never lacks
its addon. `node/package.json` declares `os: ["linux"]`, `cpu: ["x64"]` and
`libc: ["glibc"]`, so npm refuses another platform with `EBADPLATFORM`; npm
cannot check the glibc version, so an older glibc installs and fails to load.
macOS, Windows, Linux arm64 and musl need a per-platform prebuild scheme that
is not built; [`node/README.md`](node/README.md) is the npm page. The default
addon, wheel and command have no PostgreSQL and no TLS:
a call that needs them is refused `Unsupported` ("this LayerFS build has no
PostgreSQL engine"). Both are opt-in at build time (`tls` uses the system's
TLS library through `native-tls`):

```bash
npm --prefix api run build:native -- --features postgresql     # or tls, or postgresql,tls
(cd api/python && maturin build --release --locked --features postgresql -o "$OUT")
cargo +1.85.1 build --release --locked -p layerfs-cli --features postgresql
```


Disposable/WAL/synchronous=OFF only. Durable execution is NOT_RUN — disabled by
owner until explicit reauthorization. One attempted operation; no retry or replay.

## Tests

```bash
perl -e 'alarm 120; exec @ARGV' node --test api/node/test/calls.mjs
```

They run over the real addon (build it first) in a private temporary home:
the table, every call refused with `InvalidInput` when its arguments are
missing, and one live flow (a Store, Init, files, bytes and a folder scope).

Every test command runs under a wall limit of at most 120 s (macOS has no
`timeout`, so `perl`'s `alarm` ends it). The other test files are listed in
[bindings](../docs/architecture/api/front-ends/bindings.md).
