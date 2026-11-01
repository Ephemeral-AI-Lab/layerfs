# The `layerfs` JavaScript package

> **Status:** Current implementation guide for LayerFS 2.0 (issue #533); not a
> release receipt. It replaces 1.1.0's four packages (`@layerfs/agent`,
> `@layerfs/cli`, `@layerfs/backend` and `@layerfs/runtime`), which were
> removed in #533 step 14. Their last source is in the 1.1.0 release tag.

`api/` holds one JavaScript package, `layerfs` (in [`node/`](node)), and the
script that builds its native addon. The package is the library's call table
as nine families of functions:

| Family | Calls |
| --- | --- |
| `project` | `init`, `list`, `get`, `remove` |
| `branch` | `list`, `get`, `fork`, `merge`, `publish`, `diff` |
| `commit` | `list`, `get`, `diff` |
| `file` | `read`, `stat`, `list`, `glob`, `grep`, `blame`, `diff`, `write`, `edit`, `rm`, `mkdir`, `mv`, `cp`, `apply` |
| `sandbox` | `create`, `list`, `get`, `close` |
| `workspace` | `mount`, `exec`, `commit`, `unmount`, `status` |
| `machine` | `list`, `get`, `status`, `create`, `connect`, `remove`, `disconnect`, `update` |
| `store` | `list`, `get`, `status`, `create`, `connect`, `move`, `remove`, `disconnect` |
| `backup` | `connect`, `disconnect`, `list`, `create`, `restore` |

`layerfs.table` lists every call with its arguments and options; it is the
source for the list above, for the `layerfs` command (`layerfs help`) and for
the Python package.

```js
import layerfs from "layerfs";

await layerfs.store.create("shop");
await layerfs.project.init("shop/api", "/src/api", { on_progress: console.log });
await layerfs.branch.fork("shop/api", "agent", "main");
await layerfs.file.write("shop/api", "agent", "/README.md", "hello\n");

// another home, and a folder scope: paths and Branches are relative to it
const lfs = layerfs.client({ home: "/data/lfs", scope: { folder: "/web", branch: "agent" } });
```

Each function takes its positional arguments in the table's order and then an
options object. Function options (`on_progress`, `stdout`, `stderr`) receive
the call's events. A refusal throws `LayerFSError` with `code`, `message` and
`detail`; results are plain objects with snake_case keys, and bytes are
`Buffer`s. Where a call runs (in this process, or in a machine's worker) is
decided by the home's records, not by the package.

## Builds

Run `npm --prefix api run build:native` on the macOS/Linux host that loads the
addon. It uses Cargo +1.85.1, `--offline --locked --release`, and the repository
ARM64 inputs. `-- --debug` selects dev. The artifact is atomically renamed into
place. `CARGO_TARGET_DIR` must stay inside this worktree; `LAYERFS_NATIVE_OUTPUT`
can select a separate platform artifact. The package looks for the addon beside
itself (`node/layerfs.node`, when installed), at a worktree build bundle's root,
and then beside its crate.

Disposable/WAL/synchronous=OFF only. Durable execution is NOT_RUN — disabled by
owner until explicit reauthorization. One attempted operation; no retry or replay.

## Tests

```bash
node --test api/node/test/calls.mjs
```

They run over the real addon (build it first) in a private temporary home:
the table, every call refused with `InvalidInput` when its arguments are
missing, and one live flow (a Store, Init, files, bytes and a folder scope).
