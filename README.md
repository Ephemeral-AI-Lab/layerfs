# LayerFS

**Ephemeral Workspaces. Durable Shared History.**

Fast, branchable filesystem storage for parallel AI agents.

[![CI](https://github.com/Ephemeral-AI-Lab/layerfs/actions/workflows/ci.yml/badge.svg)](https://github.com/Ephemeral-AI-Lab/layerfs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Status: Developer Preview](https://img.shields.io/badge/status-developer%20preview-orange.svg)](release-notes/0.1.0/README.md)

<p align="center">
  <img
    src="docs/assets/diagrams/layerstack-parallel-workspaces.png"
    alt="A top-down LayerStack with newer Layers above older Layers, multiple Branches per Layer, multiple Workspaces per Branch, and Branch head Commits added as new Layers"
    width="820"
  >
</p>
LayerFS gives each agent a private, disposable filesystem **Workspace** over shared immutable history. Useful results become durable, deduplicated **Commits** in one local SQLite **Store**, ready to branch, inspect, reuse, or promote into the next immutable **Layer**.

Fork stored state without duplicating its canonical content. Run ordinary Linux tools through a real FUSE filesystem. Commit only the changed content and filesystem structure.

LayerFS is a local workspace and storage engine. It is **not** an agent framework, a cloud orchestration service, or a hardened security sandbox.

> [!WARNING]
> LayerFS 0.1.0 is a release candidate and Developer Preview. It is intended for local evaluation, agent-runtime integration, and performance research. It is not production storage or a crash-durable database. Keep an independent copy of important data.

## Architecture

LayerFS separates durable shared history from ephemeral execution:

| Component         | Lifetime      | Responsibility                                               |
| ----------------- | ------------- | ------------------------------------------------------------ |
| `LayerStackStore` | Durable       | One local SQLite database containing LayerStacks, immutable Layers, named Branches, immutable Commits, and one deduplicated canonical-object namespace. |
| `Client`          | Process-bound | Binds exactly one Store, one Monitor, and one Workspace manager. A different Store uses a different Client. |
| `Workspace`       | Ephemeral     | Copy-on-write state projected through a host directory or container FUSE, with fresh-process execution and explicit Commit and End. It has no database. |

The public lifecycle is:

```text
Layer Ln → Fork Branch → Create Workspace → Commit to Branch → Add Branch head → Layer L(n+1)
```

One LayerStack is a linear history. Initialization creates the genesis Layer `L0`; every later Layer is created by adding a Branch head Commit based on the current LayerStack head. Adding a Branch reuses the Commit’s existing root and copies zero canonical objects.

Each Layer can seed many Branches, and each Branch can create many Workspaces over its lifetime. LayerFS 0.1.0 permits one active writable Workspace lease per Branch, so parallel agents normally use separate Branches. Competing Branches must reconcile onto the winning LayerStack head before `Add`.

## Storage Model: Core Storage Mechanisms

CAS, CDC, and COW make LayerFS history storage-efficient by reusing unchanged objects, file regions, and filesystem structure.

### 01 · Identity

#### Content-addressed storage

Names immutable objects from their canonical bytes, verifies reads, and reuses exact duplicates across files, LayerStacks, and agents.

### 02 · Byte locality

#### Content-defined chunking

Keeps chunk boundaries stable around localized edits, so changing a small region does not require storing an entirely new large file.

### 03 · Structural locality

#### Copy-on-write

Publishes a change in a new Layer by rebuilding only the changed file and directory path while preserving every unchanged subtree from its parent.

## LayerStacks, Branches, and Workspaces

A **LayerStack** records complete filesystem checkpoints. Agents can check out independent Workspaces from different points in that history:

```text
LayerStack:  L0 ── L1 ── L2 ── L3 ── L4
                         │          │
                    Agent A      Agent B
                   checkout L1  checkout L3
```

Agent A and Agent B receive private filesystem environments while the selected history remains shared. This supports independent experimentation, checkpointing, branching, rollback, comparison, and later promotion of useful results.[1]

The current 0.1.0 lifecycle is:

1. Create or open a local Store.
2. Initialize a LayerStack from an empty root or directory.
3. Fork a named Branch from a Layer or Commit.
4. Create a host-materialized or container-FUSE Workspace.
5. Execute fresh processes and follow bounded output.
6. Commit or discard the Workspace, then end it cleanly.
7. Add the Branch head as an immutable Layer.
8. Query, diff, inspect, or reconcile durable history as needed.

## Repository Layout

```text
crates/
├── layerfs-content           canonical objects, CDC, extents, and tree algorithms
├── layerfs-layerstack-store  SQLite schema, identities, history, and object admission
├── layerfs-workspace         ephemeral COW sessions, capture, execution, and containers
├── layerfs-materialization   portable directory materialization and capture
├── layerfs-fuse              Linux FUSE plus host/proxy adapters
├── layerfs-daemon            authenticated container mount and execution protocol
├── layerfs-monitor           operation receipts, timings, database snapshots, and dedup analysis
├── layerfs-sdk               public Rust Client and value types
└── layerfs-cli               layerfs command-line interface

containers/layerfs-fuse       managed Linux FUSE runtime image
benchmark/fs-bench            focused filesystem benchmark scripts
benchmark/fs-bench-pro        end-to-end public SDK benchmark
benchmark-results/             retained benchmark evidence
tools/layerfs-eval             Store and Branch integrity evaluator
docs/versioned/0.1.0          current versioned product manual
release-notes/0.1.0           release contract, evidence, artifacts, and limitations
```

## Quickstart

LayerFS 0.1.0 is built from source and requires macOS or Linux with Rust 1.85.1 or newer. Docker is needed only for managed-container FUSE Workspaces.

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

This creates one local Store and one LayerStack with a genesis Layer. Continue with the [complete quickstart](docs/versioned/0.1.0/quickstart.md) for Branch creation, the Workspace → Exec → Commit → End lifecycle, directory imports, managed containers, and real FUSE. See the [CLI reference](docs/versioned/0.1.0/cli.md) for every command and argument.

## Rust SDK

The CLI and benchmarks use the public Rust SDK. It is currently consumed from this repository:

```toml
[dependencies]
layerfs-sdk = { path = "/absolute/path/to/layerfs/crates/layerfs-sdk" }
```

Connect one Client to one local Store:

```rust
use layerfs_sdk::{Client, LayerStackStore};
use std::sync::Arc;

let store = Arc::new(LayerStackStore::create(".layerfs/store.sqlite")?);
let client = Client::connect(store)?;
```

The [Rust SDK reference](docs/versioned/0.1.0/sdk.md) contains a complete compilable lifecycle example plus initialization, Branch, Workspace, execution, container, query, monitoring, pagination, and cleanup APIs.

## Public Operations

LayerFS currently supports:

- initializing a LayerStack from an empty root or directory;
- forking a named Branch from a Layer or Commit;
- creating host-materialized and container-FUSE Workspaces;
- executing fresh processes and following bounded output;
- committing, discarding, or cleanly ending a Workspace;
- adding a Branch head as an immutable Layer;
- diffing supported Layer and Branch-history pairs;
- reconciling stale Workspace conflicts;
- querying durable entities and active operations;
- inspecting deduplication evidence; and
- managing resource-bounded runtime containers.

See the [CLI reference](docs/versioned/0.1.0/cli.md) and [SDK reference](docs/versioned/0.1.0/sdk.md) for the complete public surface.

## Measured End-to-End Performance

The LayerFS 0.1.0 release-candidate benchmark compares complete public SDK lifecycles against Cloudflare Computer. The matched seven-pair campaign uses real FUSE Workspaces, fresh workload processes, isolated Stores, identical acknowledgement boundaries, and no timed container provisioning.

| Complete public SDK lifecycle | LayerFS median | Cloudflare Computer median | LayerFS speedup |
| ----------------------------- | -------------: | -------------------------: | --------------: |
| Cold create 32 MiB            | **161.231 ms** |               1,660.321 ms |      **10.07×** |
| Sixteen deterministic edits   | **169.133 ms** |               2,631.062 ms |      **15.80×** |
| Prepend 10 bytes              | **232.394 ms** |               2,484.210 ms |      **10.48×** |
| Read 32 MiB                   | **119.154 ms** |                 780.946 ms |       **6.53×** |
| Registered total              | **690.196 ms** |               7,579.414 ms |      **10.76×** |

Measured incremental-storage results:

| Workload                   |        LayerFS storage reduction |
| -------------------------- | -------------------------------: |
| Prepend 10 bytes to 32 MiB | **99.92% less semantic content** |
| Sixteen-edit campaign      | **97.19% less semantic content** |

These measurements apply to a specific release-candidate source seal, environment, workload, and public operation boundary. They are not universal latency guarantees. Read the [complete benchmark report](release-notes/0.1.0/benchmark-results.md) for distributions, phase timing, storage accounting, source identity, environment custody, raw evidence, and limitations.

## Project Status and Limitations

LayerFS is ready for local evaluation, agent-runtime integration, and performance research, but it is not production-ready.

Important current limitations include:

- local one-Store operation with no cross-host synchronization;
- live-process transaction visibility with no power-loss durability guarantee;
- no published crates.io SDK package or default runtime image;
- Docker, `/dev/fuse`, and `CAP_SYS_ADMIN` are required for managed FUSE;
- the managed container is not a complete hostile-code security boundary;
- the detached CLI context owner does not forward an interactive PTY; and
- CLI JSON operation payloads remain preview-level.

Read the [complete limitations](release-notes/0.1.0/limitations.md) before using LayerFS with important data.

## Learn More

- [Quickstart](docs/versioned/0.1.0/quickstart.md) — run the current public CLI and SDK.
- [LayerFS from first principles](https://learn.layerfs.ai/) — learn CAS, CDC, copy-on-write, and the LayerStack model.
- [Simplified Chinese documentation](https://learn.layerfs.ai/zh/) — Chinese version of the learning site.
- [0.1.0 specification](docs/versioned/0.1.0/specification.md) — normative release-candidate behavior.
- [CLI reference](docs/versioned/0.1.0/cli.md) — complete command grammar.
- [Rust SDK reference](docs/versioned/0.1.0/sdk.md) — complete public Rust API.
- [Container runtime](docs/versioned/0.1.0/container-runtime.md) — managed Docker and real-FUSE setup.
- [Benchmark results](release-notes/0.1.0/benchmark-results.md) — latency, storage, methodology, and raw evidence.
- [Release-candidate record](release-notes/0.1.0/README.md) — frozen release scope, verification, artifacts, and limitations.
- [Documentation index](docs/index.md) — all maintained documentation.

The versioned repository documentation defines the current product contract. The first-principles learning site is educational and may describe future work.[1]

## Related Projects

LayerFS supplies storage mechanics. Neighboring projects focus on execution isolation and version-control workflows:

| Project                                                      | Purpose                                                      |
| ------------------------------------------------------------ | ------------------------------------------------------------ |
| [AgentsGit](https://github.com/Ephemeral-AI-Lab/agentsgit)   | Version control for agent work in motion: checkpoint, branch, compare, recover, and promote. |
| [Ephemeral Sandbox](https://github.com/Ephemeral-AI-Lab/ephemeral-sandbox) | Isolated execution environments for parallel agents.         |

## Contributing

LayerFS is under active development. Bug reports, reproducible performance evidence, documentation corrections, and focused pull requests are welcome.

See the [development guide](docs/general/development.md) for the local verification workflow.

## License

LayerFS is licensed under the [MIT License](LICENSE).

## References

[1]: https://learn.layerfs.ai/chapters/01-cas-and-cdc/index.html "Foundations: CAS + CDC + COW"

[4]: https://github.com/Ephemeral-AI-Lab/layerfs "LayerFS source repository"
