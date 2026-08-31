<p align="center">
  <img src="docs/assets/branding/layerfs-icon.png" alt="LayerFS icon" width="128">
</p>

# LayerFS

**Branchable, content-addressed workspaces for local AI-agent workloads.**

[![CI](https://github.com/Ephemeral-AI-Lab/layerfs/actions/workflows/ci.yml/badge.svg)](https://github.com/Ephemeral-AI-Lab/layerfs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Status: Developer Preview](https://img.shields.io/badge/status-developer%20preview-orange.svg)](release-notes/0.1.0/README.md)

<p align="center">
  <img
    src="docs/assets/diagrams/layerstack-parallel-workspaces.png"
    alt="LayerStack history with branches and parallel workspaces"
    width="820"
  >
</p>

LayerFS is a **local filesystem storage engine** for creating disposable workspaces over shared, immutable history. Agents work in ordinary Linux filesystems, keep their changes isolated, and publish useful results as deduplicated commits that can be branched, inspected, compared, or promoted into a new layer.

LayerFS is deliberately narrow in scope. It provides storage, workspace, execution, and history primitives; it is **not** an agent framework, cloud orchestration service, or hardened security sandbox.

> [!WARNING]
> LayerFS 0.1.0 is a developer preview and release candidate. It is intended for local evaluation, agent-runtime integration, and performance research—not production storage. It does not provide crash- or power-loss-durability guarantees. Keep an independent copy of important data.

## Why LayerFS?

Traditional copies of an agent workspace duplicate unchanged bytes and directory structure. LayerFS instead stores immutable content once and creates new state by reusing everything that did not change.

| Mechanism | What it does |
| --- | --- |
| **Content-addressed storage** | Names immutable objects by their canonical bytes, verifies reads, and reuses exact duplicates. |
| **Content-defined chunking** | Keeps chunk boundaries stable around localized edits, reducing the amount of new file content required. |
| **Copy-on-write trees** | Rebuilds only changed files and directory paths while reusing unchanged subtrees. |
| **Branches and layers** | Makes alternate filesystem states cheap to create, inspect, compare, and promote. |

## Core model

A single local SQLite **Store** contains durable history and canonical objects. A **LayerStack** is an ordered history of immutable **Layers**, with the newest Layer at the top and the genesis Layer at the bottom. Each Layer can fork multiple **Branches**; each Branch can create multiple ephemeral **Workspaces**. A Workspace can be committed back to its Branch, and the Branch head can then be added as a new Layer on the LayerStack.

| Concept | Lifetime | Role |
| --- | --- | --- |
| **LayerStack** | Durable | A linear sequence of immutable filesystem checkpoints. |
| **Layer** | Durable | A checkpoint created by adding a branch-head commit to a LayerStack. |
| **Branch** | Durable | A named line of work rooted at a layer or commit. |
| **Commit** | Durable | An immutable snapshot of a workspace’s filesystem state. |
| **Workspace** | Ephemeral | A private copy-on-write working environment projected onto a host directory or through container FUSE. |
| **Client** | Process-bound | Binds one Store, one monitor, and one workspace manager. |

The mental model is:

```text
                                      LayerStack
                              newest Layer at the top
                                      oldest at bottom

                              ┌────────────────────────┐
                              │ L2                     │
                              │ from Branch D @ D2     │
                              ├────────────────────────┤
                              │ L1                     │
                              │ from Branch B @ B2     │
                              ├────────────────────────┤
                              │ L0  genesis            │
                              └────────────────────────┘

L2 ──┬── fork ──▶ Branch E ──┬── create ──▶ Workspace E1
     │                       └── create ──▶ Workspace E2
     │
     └── fork ──▶ Branch F ──┬── create ──▶ Workspace F1
                             └── create ──▶ Workspace F2

L1 ──┬── fork ──▶ Branch C ──┬── create ──▶ Workspace C1
     │                       └── create ──▶ Workspace C2
     │
     └── fork ──▶ Branch D ──┬── create ──▶ Workspace D1
                             └── create ──▶ Workspace D2
                                                   │
                                                   └── commit ──▶ Commit D2
                                                                    │
                                                                    └── Add ──▶ L2

L0 ──┬── fork ──▶ Branch A ──┬── create ──▶ Workspace A1
     │                       └── create ──▶ Workspace A2
     │
     └── fork ──▶ Branch B ──┬── create ──▶ Workspace B1
                             └── create ──▶ Workspace B2
                                                   │
                                                   └── commit ──▶ Commit B2
                                                                    │
                                                                    └── Add ──▶ L1
```

A LayerStack is the durable sequence; Branches are lines of work rooted at a Layer; Workspaces are disposable executions created from a Branch. `commit` publishes a Workspace’s changes to its Branch, while `Add` promotes the Branch head into a new immutable Layer. Each `workspace exec` starts a fresh process, and `end` removes the ephemeral projection without committing implicitly.

## Quickstart

The current release is built from source. You need **macOS or Linux** and **Rust 1.85 or newer**. Docker, `/dev/fuse`, and `CAP_SYS_ADMIN` are needed only for managed container-FUSE workspaces. Packages are not published to crates.io in 0.1.0.

From the repository root:

```bash
git clone https://github.com/Ephemeral-AI-Lab/layerfs.git
cd layerfs

cargo build --release -p layerfs-cli
export LAYERFS_BIN="$PWD/target/release/layerfs"
export LAYERFS_CONTEXT="$PWD/.layerfs/context"
mkdir -p "$PWD/.layerfs"

"$LAYERFS_BIN" db create "$PWD/.layerfs/store.sqlite"
"$LAYERFS_BIN" context use --store "$PWD/.layerfs/store.sqlite"
"$LAYERFS_BIN" layerstack init --name demo --empty
"$LAYERFS_BIN" query layerstacks
```

This creates a local Store and an empty LayerStack with a genesis layer. To continue through branch creation, workspace execution, commit, and cleanup, follow the [complete quickstart](docs/versioned/0.1.0/quickstart.md).

If you initialize from an existing directory, keep the Store file **outside** the directory being imported or projected:

```bash
mkdir -p "$PWD/import-root"
printf 'hello\n' > "$PWD/import-root/hello.txt"
"$LAYERFS_BIN" layerstack init --name imported "$PWD/import-root"
```

## What is included

The 0.1.0 public surface includes the following capabilities:

- local LayerStack initialization from an empty root or an existing directory;
- named Branch creation from a layer or commit;
- host-materialized and container-FUSE Workspaces;
- fresh-process execution with bounded output streaming;
- explicit workspace commit, discard, and cleanup;
- immutable layer creation from a branch head;
- history queries, supported diffs, and stale-workspace reconciliation;
- monitoring, operation receipts, database snapshots, and deduplication analysis; and
- resource-bounded managed runtime containers.

Use the [CLI reference](docs/versioned/0.1.0/cli.md) for the complete command surface. For programmatic integration, use the public Rust SDK:

```toml
[dependencies]
layerfs-sdk = { path = "/absolute/path/to/layerfs/crates/layerfs-sdk" }
```

```rust
use layerfs_sdk::{Client, LayerStackStore};
use std::sync::Arc;

let store = Arc::new(LayerStackStore::create(".layerfs/store.sqlite")?);
let client = Client::connect(store)?;
```

See the [Rust SDK reference](docs/versioned/0.1.0/sdk.md) for a complete lifecycle example and API details.

## Repository layout

```text
crates/
├── layerfs-content           content-addressed objects, chunking, extents, trees
├── layerfs-layerstack-store  SQLite schema, history, identities, object admission
├── layerfs-workspace         ephemeral workspaces, capture, execution, containers
├── layerfs-materialization   directory materialization and capture
├── layerfs-fuse              Linux FUSE and host/proxy adapters
├── layerfs-daemon            authenticated container mount/execution protocol
├── layerfs-monitor           receipts, timings, snapshots, dedup analysis
├── layerfs-sdk               public Rust client and value types
└── layerfs-cli               `layerfs` command-line interface

tools/layerfs-eval             Store and Branch integrity evaluator
benchmark/                     filesystem and end-to-end benchmarks
containers/layerfs-fuse        managed Linux FUSE runtime image
docs/versioned/0.1.0          current versioned product manual
release-notes/0.1.0            release contract, evidence, and limitations
```

## Current limitations

LayerFS is suitable for evaluation and integration work, but the preview boundary matters:

- operation is local to one Store; there is no cross-host synchronization;
- live-process transaction visibility does not imply crash or power-loss durability;
- the SDK is consumed from this repository; there is no published crates.io package or default runtime image;
- managed FUSE requires Docker, `/dev/fuse`, and `CAP_SYS_ADMIN`;
- the managed container is not a complete hostile-code security boundary;
- the detached CLI context owner does not forward an interactive PTY; and
- CLI JSON output is a preview text envelope, not a stable machine API.

Read the full [limitations](docs/versioned/0.1.0/limitations.md) before using LayerFS with important data.

## Documentation

Start with the [documentation index](docs/index.md), or choose a focused guide:

| Need | Documentation |
| --- | --- |
| Learn the concepts | [Core concepts](docs/general/concepts.md) |
| Run the public CLI and SDK | [Quickstart](docs/versioned/0.1.0/quickstart.md) |
| Find a CLI command | [CLI reference](docs/versioned/0.1.0/cli.md) |
| Integrate with Rust | [Rust SDK reference](docs/versioned/0.1.0/sdk.md) |
| Configure container FUSE | [Container runtime](docs/versioned/0.1.0/container-runtime.md) |
| Understand the storage format | [Storage format](docs/versioned/0.1.0/storage-format.md) |
| Review measured performance | [Benchmark results](release-notes/0.1.0/benchmark-results.md) |
| Contribute changes | [Development guide](docs/general/development.md) |

The [first-principles learning site](https://learn.layerfs.ai/) is educational material and may describe future work; the versioned repository manual defines the current product contract.

## Related projects

LayerFS provides storage mechanics for a broader agent-workflow stack:

| Project | Focus |
| --- | --- |
| [AgentsGit](https://github.com/Ephemeral-AI-Lab/agentsgit) | Version control for agent work in motion: checkpoint, branch, compare, recover, and promote. |
| [Ephemeral Sandbox](https://github.com/Ephemeral-AI-Lab/ephemeral-sandbox) | Isolated execution environments for parallel agents. |

## Contributing

Bug reports, reproducible performance evidence, documentation corrections, and focused pull requests are welcome. Before opening a change, review the [development guide](docs/general/development.md) and run the repository’s relevant verification commands.

## License

LayerFS is licensed under the [MIT License](LICENSE).
