# LayerFS V2 SDK and CLI operation families

This document records the implemented V2 public surface after the pull-through
and naming refinements. It is subordinate to `spec.md` and intentionally
contains no compatibility aliases.

One `Client` binds exactly one LayerStackStore endpoint, one BranchStore, the
BranchStore's immutable parent route, one Monitor, and one Workspace worker per
active Workspace UUID. A different database pair requires a different Client.

## Shared value types

```rust
EntityName                         // validated immutable name
LayerStackId, LayerId
BranchId, CommitId
RemotePlacement::{Reference, Replica}

LayerStackInitialization::{
    Empty,
    Directory(PathBuf),
}

LocalForkSource::{
    Layer { layer_id },
    Branch { branch_id, commit_id },
}

DiffRequest::{
    BranchCommits { branch_id, from_commit_id, to_commit_id },
    BranchLayer { branch_id, layer_id },
    Layers { from_layer_id, to_layer_id },
}

ResolveChoice::{Branch, Layer, WorkingTree}
EndWorkspaceMode::{Clean, Discard}
```

`EntityName` is 1–63 ASCII bytes, starts and ends with `[a-z0-9]`, and
otherwise permits `[a-z0-9._-]`. Names are immutable presentation and
selection metadata. They do not participate in any content-derived ID.

`LocalForkSource` deliberately has no endpoint or placement field. Remote
acquisition belongs only to Pull.

## Store and context family

```rust
LayerStackStore::create(path)
LayerStackStore::connect(path)
BranchStore::create(path, parent_store_id)
BranchStore::connect(path, expected_parent_store_id)
Client::connect(ConnectionContext { layerstack, branches })
```

`create` refuses an existing target. `connect` never creates. Context binding
verifies the immutable BranchStore parent identity and never reparents a Store.

## LayerStack and Layer family

```rust
Client::initialize_layerstack(
    name: EntityName,
    source: LayerStackInitialization,
) -> InitializeLayerStackResult

Client::pull_layer(
    through_layer_id: LayerId,
    placement: RemotePlacement,
) -> PullLayerResult

Client::add_layer(branch_id: BranchId) -> AddLayerResult
```

Initialization creates one named LayerStack and its genesis Layer atomically.

Layer Pull always acquires the complete LayerStack prefix through the exact
boundary. Reference imports every fact and reads missing objects local-first
from the parent. Replica additionally completes and receipts the union of every
Layer root in the prefix for offline reads.

Add derives the target LayerStack and pushed Branch head from `BranchId`. It
never pushes implicitly.

## Branch family

```rust
Client::pull_branch(
    branch_id: BranchId,
    through_commit_id: CommitId,
    placement: RemotePlacement,
) -> PullBranchResult

Client::fork_branch(
    name: EntityName,
    source: LocalForkSource,
) -> BranchId

Client::push_branch(branch_id: BranchId) -> PushResult
```

Branch Pull preserves the authority `BranchId`, name, LayerStack ownership, and
immutable origin. It imports complete visible inherited history through the
exact Commit plus every required LayerStack prefix. The resulting scope is a
read-only remote placement.

Fork is local-only, requires a new name, performs no Pull or object copy, and
always mints a new `BranchId`.

Push accepts only a local Branch and transfers only its locally owned Commit
suffix after the immutable fork boundary or already acknowledged authority
head. Pulled ancestry is never retransmitted.

Pull outcomes distinguish:

```rust
Created
Advanced
ModeChanged
UpToDate
AlreadyContained
HeadMoved
```

The result always calls the requested ID a `through_*` boundary.

## Diff family

```rust
Client::diff(request: DiffRequest) -> OperationHandle
```

The three supported comparisons share one read-only paged path-diff
implementation. Branch-to-Branch Diff is intentionally unrepresentable.
Inherited Branch ancestry is valid input; owned-lane traversal is not used for
Diff.

## Workspace family

```rust
Client::create_workspace_session(request) -> WorkspaceId
Client::workspace_conflicts(workspace_id, cursor) -> ConflictPage
Client::resolve_workspace_conflict(workspace_id, conflict_id, choice)
Client::commit_workspace_session(workspace_id) -> WorkspaceCommitResult
Client::end_workspace_session(workspace_id, mode)
Client::exec_workspace_session(workspace_id, argv) -> WorkspaceExecution
Client::shell_workspace_session(workspace_id) -> WorkspaceExecution
Client::workspace_output(execution_id) -> OutputReader
Client::stop_workspace_execution(execution_id)
```

A Workspace is ephemeral and has no database. A remote Branch may be mounted
and read, but Commit returns typed `ReadOnlyBranch`. A local Fork is required
before writing remote history.

Conflict choices remain distinct:

- `Branch` restores the exact Branch-side state for all affected paths.
- `Layer` selects the pulled current-Layer state.
- `WorkingTree` preserves the validated current Workspace state.

Only a later mutation intersecting an affected path invalidates that choice.
An unrelated mutation does not.

`Clean` end refuses dirty uncommitted state; `Discard` explicitly abandons it.
Exec and Shell run only inside an active Workspace. Output is a bounded reader
over typed stdout/stderr chunks and a terminal execution receipt; Stop targets
the exact `ExecutionId`.

## Query and Monitor family

```rust
Client::query(Query) -> QueryPage
Client::monitor_snapshot() -> MonitorSnapshot
Client::analyze_dedup() -> DedupAnalysis
```

Query kinds are:

```rust
LayerStacks
AuthorityLayerStacks
Layers
AuthorityLayers
Branches
AuthorityBranches
Commits
AuthorityCommits
Workspaces
Monitor
```

`Query::after`, `Query::limit`, and `Query::in_layer_stack` provide typed,
bounded keyset pagination. Limits are 1–512. Receiver LayerStack and Branch
items include exact scope, through boundary, serving mode, and immutable name.
Workspace rows use immutable LayerStack/Branch identity cached on the ephemeral
Workspace worker, so page decoration does not issue per-row Store or authority
lookups. Unscoped facts are not visible as placements.

Passive Monitor snapshots execute zero Store SQL. Explicit dedup analysis
reports exact physical CAS bytes, union CAS bytes, placement factor, local CAS
reuse, and transfer avoidance when the required denominators are present.

## CLI grammar

The CLI has seven top-level families and a global `--json` flag:

```text
layerfs
├── db
├── context
├── layerstack
├── branch
├── workspace
├── monitor
└── query
```

### Database and context

```text
layerfs db create layerstack <path>
layerfs db create branch <path> --parent <layerstack-location>
layerfs db connect layerstack <location>
layerfs db connect branch <path> --parent <layerstack-location>

layerfs context use \
  --layerstack <layerstack-location> \
  --branch <branch-store-path>
layerfs context show
```

### LayerStack

```text
layerfs layerstack init --name <name> --empty
layerfs layerstack init --name <name> <directory>

layerfs layerstack pull --through <layer-id> --reference
layerfs layerstack pull --through <layer-id> --replica

layerfs layerstack diff --from <layer-id> --to <layer-id>
layerfs layerstack add <branch-id>
```

### Branch

```text
layerfs branch pull <branch-id> --through <commit-id> --reference
layerfs branch pull <branch-id> --through <commit-id> --replica

layerfs branch fork --name <name> --layer <layer-id>
layerfs branch fork --name <name> \
  --branch <branch-id> --commit <commit-id>

layerfs branch diff --branch <branch-id> \
  --from <commit-id> --to <commit-id>
layerfs branch diff --branch <branch-id> --layer <layer-id>
layerfs branch push <branch-id>
```

Exactly one placement flag is required for Pull and forbidden for Fork.

### Workspace

```text
layerfs workspace create <branch-id> \
  --at <mount-path> \
  [--container <container-id>] \
  [--projection fuse|materialize]

layerfs workspace exec <workspace-id> -- <program> [arguments...]
layerfs workspace shell <workspace-id>
layerfs workspace output <execution-id> [--follow]
layerfs workspace stop <execution-id>
layerfs workspace conflicts <workspace-id> [--after <cursor>]
layerfs workspace resolve <workspace-id> <conflict-id> \
  (--branch | --layer | --working-tree)
layerfs workspace commit <workspace-id>
layerfs workspace end <workspace-id> [--discard]
```

### Monitor and query

```text
layerfs monitor snapshot
layerfs monitor analyze-dedup

layerfs query layerstacks
layerfs query authority-layerstacks
layerfs query layers
layerfs query authority-layers
layerfs query branches
layerfs query authority-branches
layerfs query commits
layerfs query authority-commits
layerfs query workspaces
layerfs query monitor
```

Completion presents qualified `layerstack-name/branch-name (BranchId)` labels
but substitutes the exact typed ID. JSON schema version 3 emits structured
query fields, including names, scope, serving mode, and through boundary; it
does not serialize Rust `Debug` output as the query value.

## Deleted and forbidden operation families

V2 has no generic merge, Layer push, Branch advance, Branch-to-Branch Diff,
Project type/table, rename operation, hidden Pull inside Fork, placement-bearing
Fork, active Store selector, connection vector, TUI, Ratatui, Crossterm, or
OverlayFS implementation. FUSE and explicit materialization are the current
Workspace projections. OverlayFS remains deferred.
