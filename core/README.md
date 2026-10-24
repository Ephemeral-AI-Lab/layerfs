# LayerFS core

> **Status:** Current general guide. Crate list and production line counts are
> pinned to `664d02864e`. Implementation rules live in [AGENTS.md](AGENTS.md).

`core/` is the LayerFS product: one Rust Cargo workspace with fourteen crates,
plus the JavaScript SDK's Node binding at [`../api/binding`](../api/binding).
It is the only product at HEAD. The v0.1.6 reference implementation that lived
under root `crates/` was retired on 2026-10-10 (R9c) and can be read at local tag
`reference-v0.1.6-final`; nothing in `core/` depends on it.

## How the pieces fit

- **One global Store.** All committed state (content objects, metadata and
  history) lives in one SQLite Store file, opened with the Disposable profile
  (WAL, `synchronous=OFF`). Project Init builds and seals it; it is then
  installed on a volume the daemons share.
- **Store calls run in the SDK's process.** The JavaScript SDK
  ([`api`](../api/README.md)) calls `layerfs-api` through the Node
  binding. Its fs, branch, project and operator methods run on
  `layerfs-store-service` against the Store the SDK process opened directly.
  This is the only route, and it is Linux only; elsewhere these calls fail
  `Unsupported`. See [direct Store access](docs/issues/352/DIRECT_STORE.md).
- **Workspace calls go to the daemon.** `layerfs-daemon` runs inside a Sandbox
  container that `layerfs-api` creates through `layerfs-sandbox` (Docker) and
  reaches over an authenticated `layerfs-bridge` channel. It serves mount, exec,
  Commit, status and unmount.
- **A Workspace is a full mutable view.** Mount binds a FUSE filesystem
  (`layerfs-fuse`) over an immutable committed root (`layerfs-workspace`). Each
  Workspace has its own overlay SQLite file (`layerfs-overlay`), copied from a
  template at mount and deleted at unmount. Any process that can see the mount
  uses it through ordinary filesystem calls; the daemon does not supervise
  commands.
- **Commit publishes the Workspace's changes.** It captures what the Workspace
  has published locally, builds new objects with the same content, storage and
  persistence libraries Project Init uses, and publishes in one short step.
  Writers take turns: heavy work happens first, and only the final publish holds
  the single write lane. See the
  [Store concurrency model](docs/architecture/83-store-concurrency-model.md).

## Crates

Production lines from `tools/production_loc.py` at `664d02864e`. Only the crate
directories are shown; the total is all of `core/crates/`.

```
core/crates/                       [96,473]
  layerfs-content/                 [25,942]  (canonical objects, file construction, bounded reads)
  layerfs-storage/                  [8,993]  (content-addressed encoding and packs)
  layerfs-history/                    [990]  (LayerStacks, Branches, Commits, Layers)
  layerfs-persistence/              [8,398]  (shared SQLite adapter and profiles)
  layerfs-project/                  [1,667]  (namespace Init over the libraries above)
  layerfs-telemetry/                [2,280]  (parent/child timing trees)
  layerfs-overlay/                  [9,730]  (one Workspace's disposable overlay database)
  layerfs-workspace/                [8,991]  (Workspace filesystem semantics)
  layerfs-fuse/                     [4,523]  (kernel connection and request service)
  layerfs-daemon/                   [6,019]  (Workspaces, Commit, the layerfs-daemon binary)
  layerfs-store-service/            [9,821]  (Store engine: reads, file changes, history)
  layerfs-api/                      [2,291]  (SDK surface, one function per method)
  layerfs-bridge/                   [3,517]  (authenticated bounded channels)
  layerfs-sandbox/                  [3,311]  (container commands, streams, status)
```

The first six crates are the content, storage, history, persistence, Project Init
and telemetry libraries (cluster one; see the
[cluster-one handbook](../docs/general/cluster_one_handbook.md)). The rest are the
Workspace runtime, the Store engine and the SDK surface (cluster two; see the
[design index](docs/issues/303/README.md)). [Cargo.toml](Cargo.toml) decides what
is built.

## Other folders

- [`docs/architecture/`](docs/architecture/README.md): source-backed descriptions
  of what the crates do, including the
  [global SQLite Store](docs/architecture/84-global-sqlite-store.md) and the
  [SDK call channel](docs/architecture/82-sdk-call-channel.md).
- [`docs/issues/`](docs/issues/): per-issue designs, fix records and evidence.
  Closed issues are historical records.
- [`tools/`](tools/): the product boundary guard, the fuser integrity check and
  telemetry report tools, each with its tests.
- `patches/` and `vendor/`: fuser 0.18.0 with the owner-authorized patches; see
  [fuser provenance](AGENTS.md#fuser-provenance-and-authorized-patch-checks).

## Checks

Run from the repository root with the pinned toolchain. Build tests first with
`--no-run`, then run each test command under an explicit timeout of at most
120 s, selecting by package when the whole suite does not fit:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p <package> --all-targets --no-run
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p <package> --all-targets
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
python3 -B core/tools/check_product_boundary.py
python3 -B -m unittest discover -s core/tools -p 'test_*.py'
```

There is no CI or aggregate pre-push gate. The full check contract, including
the aarch64 build flags, is in [AGENTS.md](AGENTS.md#checks-and-completion).
