# LayerFS JavaScript packages

> **Status:** Current implementation guide for LayerFS 1.1.0 (issues #327 and
> #428); not a release receipt. The package split is #428's
> ([core/docs/issues/428](../core/docs/issues/428/README.md)); the release is
> recorded in [docs/release-notes/1.1.0](../docs/release-notes/1.1.0/README.md).
> Historical results retain their source identities in the Phase 2–4 records.

This is the wiki for `api/`, LayerFS's JavaScript packages: what each one is
for, which one you need, and how they are built and tested. How to use each
one is in [`docs/`](docs):

| Guide | Package | What it does | Who uses it |
| --- | --- | --- | --- |
| [agent.md](docs/agent.md) | `@layerfs/agent` and the `layerfs` command | Works on a project: files, Branches, Sandboxes, Workspaces, Exec and Commit | Agents, through a Node script (the SDK) or Bash (the command) |
| [backend.md](docs/backend.md) | `@layerfs/backend` | Reads history, storage, Workspace state, background processes and daemon logs; the call trace | UIs and tools that show a project |
| [runtime.md](docs/runtime.md) | `@layerfs/runtime` | Runs LayerFS on a machine: Init, project removal, the macOS and Windows worker, Sandbox and volume cleanup | Hosts that run projects for their users |

## Which package do I need?

Every integration starts with [`@layerfs/agent`](docs/agent.md) and adds
the others as it grows:

- **A tool or skill for agents**, which works on a project's files and
  Branches: `@layerfs/agent`. Give agents that have only Bash
  [the `layerfs` command](docs/agent.md#the-layerfs-command).
- **Something that shows a project**, such as a history view or a dashboard:
  add [`@layerfs/backend`](docs/backend.md), and read the
  [call trace](docs/backend.md#the-call-trace) to follow a run as it
  happens.
- **A host that runs projects for its users**, which initializes and removes
  them, and on macOS and Windows starts the worker: add
  [`@layerfs/runtime`](docs/runtime.md).

Store calls run in the calling process on Linux only. On macOS and Windows they
go through the LayerFS home's worker, which `@layerfs/runtime` starts; see
[where calls run](docs/agent.md#where-calls-run) and
[platforms and the worker](docs/runtime.md#platforms-and-the-worker).
On Windows there is also the native direct route (owner direction
2026-10-25): the binding built from this tree opens a local NTFS Store
itself — fs, branch, project and operator calls in this process, no
worker and no daemon; workspace, exec and daemon calls refuse
`Unsupported`. See
[the Windows-native route](../engine/docs/issues/352/DIRECT_STORE.md).

## The folder

`api/` holds the published JavaScript packages, one folder each, and no Rust:

- `agent/`: `@layerfs/agent`, the agents' SDK (the client, its
  transports and the agent methods).
- `cli/`: `@layerfs/cli`, the `layerfs` command, one `@layerfs/agent` call per command.
- `backend/`: `@layerfs/backend`, per-project Store and daemon reads of an
  open client, for UIs and tools.
- `runtime/`: `@layerfs/runtime`, installation management on a LayerFS home,
  for integrations.
- `docs/`: the three guides above.
- `scripts/`: `build-native.mjs` and `recover-records.mjs`.

The Rust behind them lives in the engine: the SDK method surface
`engine/crates/layerfs-api` and the Node addon crate
`engine/crates/layerfs-node`, which builds `engine/crates/layerfs-node/layerfs.node`.

`npm install` in `api` links `agent`, `backend` and `runtime` to each
other; it downloads nothing.

`@layerfs/agent/internal` is not a public API. It exists only for
`@layerfs/backend` and `@layerfs/runtime` at the same exact version as
`@layerfs/agent`, and may change in any release; don't add new uses. It
exports exactly `STATE`, `invokeBackend`, `invokeClient`,
`invokeClientBackend`, `live`, `outcome`, `page`, `pageOf`, `projectArgs`,
`sandboxArg`, `storeArgs`, `workspaceArgs`, `deployment` (call helpers),
`decode`, `encode`, `frame`, `readFrames` (worker frames), `connection`,
`endpointOf` (worker client), `addonPath` (in-process addon) and `useWorker`
(the runtime's worker hand-over).

## Builds and compatibility

Run `npm --prefix api run build:native` on the macOS/Linux host that loads the
addon. It uses Cargo +1.85.1, `--offline --locked --release`, and the repository
ARM64 inputs. `-- --debug` selects dev. The artifact is atomically renamed into
place, avoiding stale macOS executable-page signature state. `CARGO_TARGET_DIR`
must stay inside this worktree; `LAYERFS_NATIVE_OUTPUT` can select a separate
platform artifact. Keep Linux addons separate from the host addon. On
Windows the script refuses to run; build the addon directly with
`cargo build --manifest-path engine/Cargo.toml -p layerfs-node` (locked,
`CARGO_TARGET_DIR` inside this worktree) and rename
`target/<profile>/layerfs_node.dll` to the addon path.

At runtime, the Docker Engine server selected by `LAYERFS_DOCKER` must speak
Engine API v1.48 (Docker 28.0) or newer. Before its first request the SDK reads
the server's version once (`GET /version`) and uses `/v1.<minor>/...` paths at
that version, up to v1.54; an older server is refused up front, naming both
versions. See the [agent settings](docs/agent.md#settings).

New Store schema versions are 16–18 (pack layouts) and 22–24 (with acquisition),
with history identity format 3. Old Disposable Stores are rejected without
migration. Select a fresh project/Store for the new format.

Disposable/WAL/synchronous=OFF only. Durable execution is NOT_RUN — disabled by
owner until explicit reauthorization. One attempted operation; no retry or replay.
Zombie reaping and the existing replaced-inode FUSE policy remain owner-deferred.

## Tests

The stand-in tests need no Docker, Store or daemon: they load the real
JavaScript over one stand-in native binding,
`agent/test/fixture/fake-native.cjs`, which the `layerfs` CLI tests
use too.

- `node api/agent/test/calls.mjs` checks every agent call for
  correctness against the shared case table
  `agent/test/fixture/cases.mjs`. For each call it checks the binding
  call made, the arguments sent, and the result returned or the
  `LayerFSError` thrown. There is a case for every call file under
  `agent/src/`.
- `node --test api/cli/test/cli.test.mjs` runs the same cases through the SDK and
  through the `layerfs` command and requires them to agree (see
  [the `layerfs` command](docs/agent.md#the-layerfs-command)).
- `packaging.mjs`, `trace.mjs`, `exec_output.mjs` and the backend and runtime
  tests each check one case per command (`node <file> <case>`).

Against a real Store, Docker and daemon, `test/verify/scenario.mjs` makes
every agent call once, in two parts: `store` (Projects, Branches, files and
history) and `workspace` (a Sandbox, one Workspace, Exec, Commit and close).
Run each part under the E2E launcher (`tools/worktree.py run`, see
`docs/general/parallel-worktrees.md`):

- `node api/agent/test/verify/calls_real.mjs inner <part>` runs the
  part through the SDK and checks each answer.
- `node api/cli/test/verify/cli_real.mjs inner <part>` runs it through the SDK and
  then as `layerfs` commands, and requires the same answers. Values that differ
  between any two runs are masked before the comparison.

### The `layerfs` command's tests

```bash
npm --prefix api/cli install
node --test api/cli/test/cli.test.mjs
```

The tests run the real `@layerfs/agent` JavaScript over the stand-in native
binding that the SDK's own tests use
(`api/agent/test/fixture/fake-native.cjs`). The binding records
every call, so the tests need no Docker, Store or daemon. The SDK's
`calls.mjs` checks the SDK for correctness against the shared case table
(`api/agent/test/fixture/cases.mjs`). These tests check that the
command is consistent with the SDK. Each case runs through the SDK in the
test's own process and through `layerfs`. The command must make the same
binding call with the same arguments, then either print the SDK's result as
JSON (`workspace mount` adds `ws`) and exit 0, or exit 125 with the SDK's
refusal. The tests also check parsing, bodies, `--text`, `--stream` and exit
codes, that every call in the table has a case, and that `api/cli/src` has a
command file for every SDK call file.

`api/cli/test/verify/cli_real.mjs inner <store|workspace>` checks the command
against a real Store, Docker and daemon, under the E2E launcher. It runs the
SDK's real-Store scenario (`api/agent/test/verify/scenario.mjs`,
every call once) through the SDK and then as `layerfs` commands, one process
per call. Each command must give the SDK's answer. The comparison masks only
what differs between any two runs: the project name, object ids, times, inode
numbers and a Commit's stored bytes.
