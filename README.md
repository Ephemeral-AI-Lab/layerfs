# LayerFS V2

LayerFS is a content-addressed, copy-on-write filesystem for branchable agent
workspaces. The binding architecture and verification contract is
[`docs/v2/spec.md`](docs/v2/spec.md). Older architecture and PoC documents are
historical unless that specification explicitly cites them as evidence.

## Architecture

V2 has exactly two durable SQLite databases and no Workspace database:

```text
LayerStackStore
  authoritative LayerStacks and immutable Layers
  pushed Branch candidates and immutable Commits
  complete canonical objects

BranchStore
  exact pulled Layer facts
  local Branch rollout trees and immutable Commits
  locally created or replicated canonical objects
  complete-root receipts

Workspace
  ephemeral COW state, execution, output, mount, and container lifecycle
```

One SDK context binds exactly one `LayerStackStore` endpoint, one
`BranchStore`, the BranchStore's immutable parent StoreId, one `Monitor`, and
one worker per active Workspace UUID. A different database pair uses a
different context.

The only runtime read path is root-keyed:

```text
Workspace COW -> BranchStore -> exact-missing LayerStackStore
```

A receipted complete root never falls through to the authority. A Reference
root may fall through only after an exact local `MissingObject`; corruption
and every other local error are returned directly. Reference reads never
create durable cache rows.

## Operations

- Initialize one LayerStack from an empty root or native directory.
- Pull the complete LayerStack prefix through an exact Layer as `Reference` or
  `Replica`.
- Fork a fresh Branch from a pulled Layer, local Branch Commit, or exact remote
  Branch Commit.
- Diff Branch commits, Branch versus Layer, or two Layers through one paged,
  read-only path-diff implementation.
- Create an ephemeral materialized or FUSE Workspace for one Branch.
- Commit a Workspace with one writable lease and exact Branch head/base CAS.
- Push a Branch to the authority with objects and facts first, head CAS last.
- Add a pushed Branch as the next immutable Layer; stale bases require an
  explicit Pull and typed reconciliation Workspace.
- End a Workspace explicitly; End never commits and Add never pushes.

`Reference` and `Replica` change placement and acquisition latency only. They
do not change ObjectId, CommitId, roots, Diff, Push, or Add results.

## Source layout

```text
crates/layerfs-content           canonical filesystem and reconciliation
crates/layerfs-storage           IDs, exact schemas, admission, transfer, wire
crates/layerfs-layerstack-store  authority database
crates/layerfs-branch-store      pulled Layers and local Branch rollout trees
crates/layerfs-workspace         ephemeral Workspace workers and projections
crates/layerfs-fuse              host and thin-container FUSE adapters
crates/layerfs-materialization   portable copy-out and capture
crates/layerfs-monitor           passive counters and explicit dedup analysis
crates/layerfs-sdk               one-pair public client
crates/layerfs-cli               standalone V2 CLI
tools/layerfs-eval               Store consistency evaluator
```

There is no StackStore, LayerStore, generic Branch Merge, Direct/Stacked
facade, TUI crate, Ratatui dependency, or Crossterm dependency.

## Verification

The terminal source gates are:

```sh
cargo fmt --all -- --check
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
git diff --check
```

V2 is terminal only with fresh source-bound evidence for the exact schemas and
query plans, transfer receipts and inventories, deduplication, real host and
Docker FUSE for both placements, multiple isolated mounts in one prepared
container, the frozen `fs-bench.sh` populations, executable/image identity,
checksums, and cleanup. See section 14 of the binding specification.
