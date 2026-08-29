# LayerFS CLI and SDK contract

Status: **binding replacement for the application-facing SDK and terminal
interfaces**. This document intentionally permits destructive API changes. It
optimizes the final architecture, not compatibility with the current
`Direct`/`Stacked` façade.

## 1. Frozen user model

LayerFS exposes six terminal nouns:

```text
db          connect the physical Store graph
layer       initialize, acquire, inspect, and publish accepted Layers
stack       create, acquire, inspect, and publish intermediate Stacks
branch      create, merge, acquire, and transfer Branches
workspace   execute one ephemeral filesystem transaction
monitor     observe size, reuse, resources, transactions, and elapsed time
```

There is no `project`, `layer-history`, `stack-history`, `repository`,
`worktree`, `durable`, `cache`, or `working-store` CLI noun.

`layerfs` is the coordinating application, not another Store noun. The user
model is:

```text
Workspace final delta -> Branch Commit -> optional Stack -> Layer

Commit = BranchStore result
Stack  = optional intermediate acceptance
Layer  = authoritative acceptance
```

Push transfers a candidate; Add accepts it. Pull transfers immutable state
toward work. Merge integrates Branches only. End cleans a Workspace only.

The persistence model may retain `LayerHistoryId` and `StackHistoryId`; the CLI
groups their operations under `layer` and `stack` and derives a history from a
selected immutable Layer or Stack whenever possible.

```text
LayerStore
├── 0..N StackStores
│   └── 0..N BranchStores
└── 0..N direct BranchStores

BranchStore
└── 0..N ephemeral Workspaces
```

Every BranchStore binds to one immutable parent route for its lifetime:

```text
BranchStore -> LayerStore

or

BranchStore -> StackStore -> LayerStore
```

A Workspace is not a Store, directory binding, Project, or durable record. It
is one UUID-scoped ephemeral COW transaction forked from an exact Branch head.

## 2. Interface boundary

```text
layerfs-tui
    visual UI only
        |
        v
layerfs-cli
    fully functional standalone CLI library + binary
        |
        v
layerfs-sdk
    thin semantic composition/API
        |
        +-- LayerStore
        +-- StackStore
        +-- BranchStore
        +-- layerfs-workspace
        `-- layerfs-monitor

layerfs-workspace
    COW/spool, UUID/session registry, placement, projection,
    execution/output, Commit and End orchestration
        |
        +-- layerfs-fuse
        +-- layerfs-materialization
        +-- layerfs-storage
        `-- layerfs-content

LayerStore / StackStore / BranchStore
        `-- layerfs-storage
                `-- layerfs-content
```

The boundaries are strict:

| Package | Owns | Must not own |
|---|---|---|
| `layerfs-content` | canonical object foundation, regular-file content, directory/inode/metadata tree, and pure whole-filesystem read/apply/diff/merge | SQLite, history IDs/records, transfer, Workspace spool paths, projection/runtime policy |
| `layerfs-storage` | typed history IDs/records, Store schema/SQL, candidate scratch/spill, merge-base, admission/CAS/transfer/wire | canonical decoding/CDC/filesystem merge, Workspace runtime and UI |
| `layerfs-fuse` / `layerfs-materialization` | narrow backend ports plus concrete projection/capture mechanics and raw counters/receipts | concrete Workspace dependency, Workspace-session policy, Branch Commit/CAS, Store topology |
| `layerfs-workspace` | transient COW/tree/spool, UUID sessions, host/Docker placement, projection orchestration, create/commit/end, exec/shell/stop, bounded output | reimplemented Content/Storage algorithms, dedup formulas, monitor retention, CLI/TUI rendering |
| `layerfs-monitor` | operation envelope and monotonic spans, dedup/storage formulas, route analysis, CPU/RSS sampling, bounded receipt collection and presentation-ready snapshots | Layer/Stack/Branch/Workspace mutation, raw command output, CLI/TUI rendering, Store metric tables |
| `layerfs-sdk` | Store connection/topology composition, semantic Layer/Stack/Branch access, typed subsystem handles and results | Workspace implementation, monitor implementation, Clap, Ratatui, terminal colors, command strings |
| `layerfs-cli` | all standalone commands, Clap parsing, persisted local connection context, completion, SDK dispatch, human/JSON output, streaming CLI events | Ratatui rendering, SQLite, Store algorithms |
| `layerfs-tui` | Crossterm lifecycle/input, Ratatui state/rendering, focus, navigation, completion presentation, scrollback | SDK dependency, Store handles, Docker calls, command legality |

`layerfs-tui` depends on `layerfs-cli`, not directly on `layerfs-sdk`.
The CLI remains complete when the TUI is not installed.

`layerfs-monitor` is one crate in this workspace, not a separate repository,
daemon, generic event framework, or telemetry database. Lower components own
raw domain counters; Monitor composes and retains them. The SDK may hold and
return Workspace/Monitor handles, but it does not implement either subsystem.

## 3. Database connection model

### 3.1 Create and connect are different

The current generic `open` behavior must not remain the public contract. A path
typo must not silently create an empty authority.

```rust
LayerStore::create(location) -> Result<LayerStore>
LayerStore::connect(location) -> Result<LayerStore>

StackStore::create(location, layer_endpoint) -> Result<StackStore>
StackStore::connect(location, layer_endpoint) -> Result<StackStore>

BranchStore::create(location, parent_endpoint) -> Result<BranchStore>
BranchStore::connect(location, parent_endpoint) -> Result<BranchStore>
```

| Operation | Required behavior |
|---|---|
| `create` | Location must not already contain a Store; install the exact role schema; fail rather than replace or adopt an existing Store |
| `connect` | Location must exist and match the exact role/schema; never create; validate identity and parent compatibility |

Wrong-role, wrong-parent, missing, already-existing, and unreachable outcomes are
distinct typed errors.

### 3.2 Authority-first progressive setup

The CLI/TUI session lands in a LayerStore first, then attaches zero or more
StackStores and/or BranchStores.

```text
db create layer /data/layer.db
db connect layer /data/layer.db

db create stack /data/stack.db
db connect stack /data/stack.db

db create branch /data/branch.db
db connect branch /data/branch.db
```

The active context supplies the parent; users do not name Stores or pass
`--parent`.

```text
active Layer only + db create branch
    => BranchStore -> LayerStore

active Layer + Stack + db create branch
    => BranchStore -> active StackStore -> LayerStore
```

Connecting a new StackStore never reparents an existing BranchStore. A user
creates or connects a different BranchStore for the new route.

### 3.3 Multiple Store instances

The CLI context retains connected locations and one active route:

```rust
pub struct ConnectionContext {
    pub layer: LayerConnection,
    pub stacks: Vec<StackConnection>,
    pub branches: Vec<BranchConnection>,
    pub active_stack: Option<StackConnectionId>,
    pub active_branch: Option<BranchConnectionId>,
}
```

Locations are identities in the CLI; users do not assign redundant display
names. `db use` changes selection only.

```text
db use /data/stack-b.db
db use /data/branch-c.db
db disconnect /data/branch-c.db
db list
```

The saved location already identifies the connected role; repeating the role
on `use` or `disconnect` adds no information. `db list` includes active route,
role, schema and reachability, so a second `db status` command is unnecessary.
`disconnect` removes a Store from the current CLI context. It never deletes a
database or remote resource. The normal CLI has no `db delete` command.

### 3.4 Local CLI context

Standalone commands are separate processes, so `db connect` persists a small
generated local context outside all Store databases. It stores only Store
locations, role/route identity, selected connection IDs, runtime-state root,
host socket, and non-secret presentation defaults.

It must not store credentials or accepted filesystem state. The SDK remains
constructible entirely through Rust values; no YAML is part of the SDK
contract. The CLI context is an adapter-owned convenience, not Store truth.

One small LayerFS host runs per saved CLI context. It owns that context's Store
connection graph exactly once, the Monitor collector, and one worker per active
Workspace UUID. Each `layerfs` invocation connects through a bounded typed
local control socket, executes, renders, and exits. This host is implemented in
`layerfs-cli`; it is not a new package, generic daemon, remote API, or authority.

```text
CLI process A --\
CLI process B ----> local control socket -> context host
future TUI -----/                         |-- Store graph
                                           |-- Monitor
                                           `-- Workspace workers
```

A context-host or worker crash never implies Commit. Store truth remains in
the three Store roles; runtime descriptors, output, and receipts remain bounded
host state outside them.

Host startup is exclusive and local-user scoped:

```text
acquire one atomic context lock
    -> winner creates a 0700 runtime directory and local socket
    -> winner opens Stores and signals READY
    -> losing starter connects to the READY host
```

Where supported, the host verifies the socket peer UID; socket/runtime
permissions must exclude other users. Stale PID/socket cleanup is allowed only
while holding the context lock and after ownership/type validation. Cleanup
never attempts Workspace recovery or Commit. `db disconnect` refuses a
BranchStore with active Workspaces, a StackStore with attached BranchStores, or
a LayerStore with dependent Stack/BranchStores.

### 3.5 Read-only Store observation capability

Local and remote Store endpoints may expose two internal read-only observation
calls used by explicit Monitor queries:

```rust
StoreEndpoint::inventory_page(after: Option<ObjectId>, limit: u16)
    -> Result<InventoryPage>

StoreEndpoint::storage_snapshot()
    -> Result<StoreStorageSnapshot>
```

Inventory is sorted/paged `(ObjectId, encoded_length)` data; storage snapshot
reports DB/WAL/SHM measurements available at the Store owner. These are not
application mutation methods or raw SQL access. An endpoint without the
capability returns `ObservationUnavailable`; callers show unavailable and do
not estimate route union or remote physical bytes.

## 4. Semantic SDK surface

### 4.1 IDs and source values

```rust
pub struct BranchCommit {
    pub branch_id: BranchId,
    pub commit_id: CommitId,
}

pub enum BranchSource {
    Layer(LayerId),
    Stack(StackId),
    Commit(BranchCommit),
}

pub enum LayerSource {
    BranchCommit(BranchCommit),
    Stack(StackId),
}

pub enum LayerInitialization {
    Empty,
    Directory(PathBuf),
}
```

The public SDK derives history IDs from selected immutable records:

```text
LayerId -> LayerRecord.history_id
StackId -> StackRecord.history_id
Branch base -> target LayerHistory or StackHistory
```

The lower Store layer may retain exact history arguments for atomic validation;
the application SDK must not make the caller repeat derivable values.

### 4.2 Layer operations

```rust
LayerStore::initialize(source: LayerInitialization)
    -> Result<InitializedLayer>

StackStore::pull_layer(layer_id: LayerId)
    -> Result<RefOutcome<LayerId>>

LayerStore::add_layer(source: LayerSource)
    -> Result<AddResult<LayerId>>
```

`initialize` is the only direct bootstrap. It creates a new LayerHistory and
first Layer from an empty root or external directory. It cannot append to an
existing history. Later accepted changes use `add_layer`.

### 4.3 Stack operations

```rust
StackStore::create_stack(layer_id: LayerId)
    -> Result<CreatedStack>

StackStore::pull_stack(stack_id: StackId)
    -> Result<RefOutcome<StackId>>

StackStore::add_stack(source: BranchCommit)
    -> Result<AddResult<StackId>>

StackStore::push_stack(stack_id: StackId)
    -> Result<RefOutcome<StackId>>
```

The source Branch base determines the target StackHistory for `add_stack`.
Cross-history attempts remain rejected internally.

### 4.4 Branch operations

```rust
BranchStore::create_branch(source: BranchSource)
    -> Result<BranchRecord>

BranchStore::merge(source: BranchId, target: BranchId)
    -> Result<MergeOutcome>

BranchStore::pull_branch(source: BranchId)
    -> Result<PulledBranch>

BranchStore::push_branch(branch_id: BranchId)
    -> Result<RefOutcome<CommitId>>

BranchStore::pull_commits(branch_id: BranchId)
    -> Result<CommitId>
```

The semantic SDK pins the target Branch head for Merge. The Store primitive
still performs exact-head CAS; normal users do not type an expected head.

Branch IDs are globally unique and Pull uses the same identity locally. A
divergence returns a typed result instead of silently remapping or merging the
Branch. `pull_commits` may import the immutable candidate Commit closure, after
which the user can create a Branch from the selected Commit and Merge it
explicitly.

The direct `commit(branch, expected_head, changes)` primitive remains owned by
BranchStore but is normally invoked only by `commit_workspace_session`.

These are the twelve application-facing Store operations. The lower Store
layer retains its exact validation/CAS primitives; it does not expose raw SQL,
object admission, transfer frames, `FullStorage`, history-plus-node arguments,
or caller-supplied expected heads through the application SDK.

Navigation uses one read-model seam instead of adding `list`, `show`, and `log`
methods to every façade:

```rust
Client::query(query: Query) -> Result<QueryResult>
```

`Query` covers topology, paged Layer/LayerHistory, Stack/StackHistory,
Branch/Commit details, and Commit diff. Results are frontend-neutral semantic
models with stable IDs, parents, generation/freshness, and cursors—not raw
database rows or formatted CLI text.

## 5. Workspace subsystem API

The types and implementation in this section belong to `layerfs-workspace`,
not `layerfs-sdk`. The SDK exposes a composed `Workspaces` handle for
convenience; direct Rust consumers may construct/use the subsystem without the
SDK.

### 5.1 Placement and projection are independent

```rust
pub enum WorkspacePlacement {
    Host {
        root: PathBuf,
    },
    Container {
        container_id: ContainerId,
        root: PathBuf,
    },
}

pub enum WorkspaceProjection {
    Fuse,
    Materialize,
}

pub struct CreateWorkspaceSession {
    pub branch_id: BranchId,
    pub placement: WorkspacePlacement,
    pub projection: Option<WorkspaceProjection>,
}
```

The generated UUID is an output. `None` selects the placement/platform default;
an `Auto` enum variant adds no information. Placement already distinguishes
host from container, so separate HostFuse/ContainerFuse variants are also
redundant. APFS immutable-base/clone acceleration is deferred; portable
materialization remains the native-directory Phase One path.

No path is persistently bound to a Branch. The same Branch may have simultaneous
host and Docker/FUSE sessions at different roots.

All LayerStore, StackStore, and BranchStore databases stay on the host (or an
explicit remote Store service). A Docker Workspace contains only a thin FUSE
projection; it never receives SQLite files or Store credentials.

### 5.2 Lifecycle

```rust
Workspaces::create_workspace_session(request)
    -> Result<WorkspaceSession>

Workspaces::commit_workspace_session(session_id)
    -> Result<WorkspaceCommitResult>

Workspaces::end_workspace_session(session_id, mode)
    -> Result<WorkspaceEndResult>
```

```rust
pub enum EndWorkspaceMode {
    Clean,
    Discard,
}
```

The operations are deliberately separate:

| Operation | Capture | Commit/CAS | Cleanup |
|---|---:|---:|---:|
| `create_workspace_session` | no | no | no |
| `commit_workspace_session` | final delta only | yes | no; retain a read-only session for inspection |
| `end_workspace_session(Clean)` | no | no | yes; only clean or committed session |
| `end_workspace_session(Discard)` | discard transient state | no | yes |

Plain End rejects a dirty uncommitted session. A failed Commit preserves the
session and COW state. One Workspace session creates at most one Commit; after
success it is read-only.

```text
Created -> Active -> Quiescing -> Committed/read-only -> Ended
                    |
                    `-> HeadMoved -> Active

Active --explicit discard--> Ended
```

Each active UUID has one Workspace worker under the context host. It owns the
pinned Branch head/base root, COW tree and spool, projection handles, execution
processes, writer gate, output drain, and lifecycle serialization. It never
opens a Store database or owns Store authority; it reaches the host-owned
BranchStore through the typed subsystem boundary.

`commit_workspace_session` commits final state, not operation history:

```text
pinned base root + quiesced final Workspace view
        -> one canonical-path-ordered final delta
        -> CDC/canonical objects for changed final content only
        -> unchanged subtree reuse
        -> one candidate root
        -> at most one Commit and one exact Branch CAS
```

The Workspace may maintain dirty indexes incrementally, but the result must
equal a fresh base-to-final-view diff. Repeated writes, truncate/rewrite,
create/delete, and rename/undo steps do not become durable facts. Equivalent
final filesystem states with the same canonical metadata, hard-link graph, and
pinned base produce the same root and reachable ObjectIds. Command/tool logs,
receipts, and stdout/stderr stay outside Commit identity and Store schemas.

### 5.3 Executions

```rust
Workspaces::exec(session_id, argv: NonEmpty<Vec<OsString>>)
    -> Result<WorkspaceExecution>

Workspaces::shell(session_id)
    -> Result<WorkspaceExecution>

Workspaces::stop(execution_id) -> Result<()>
```

```rust
pub enum ExecutionEvent {
    Started(ExecutionStarted),
    Stdout(OutputChunk),
    Stderr(OutputChunk),
    Exited(ExecutionReceipt),
}
```

The Workspace subsystem passes the nonempty argv directly; it never joins
arguments into an implicit shell string. `shell` uses the selected host or
container's default interactive shell; a configurable shell request is deferred
until a real consumer requires it. Host execution uses the Workspace path as `current_dir`. Container
execution uses the selected container and Workspace mount as `WorkingDir`.

Commit refuses while tracked executions, writable handles, or backend writers
remain active. It never silently kills them.

The smallest Workspace read surface is:

```rust
Workspaces::sessions() -> Result<Vec<WorkspaceSummary>>
Workspaces::session(session_id) -> Result<WorkspaceDetail>
Workspaces::diff(session_id) -> Result<WorkspaceDiff>
Workspaces::output(execution_id) -> Result<OutputReader>
```

`session` includes execution summaries; `sessions` includes active and bounded
retained ended sessions. Separate `executions` and `history` methods would
duplicate those results.

### 5.4 Output and receipts

Noninteractive stdout/stderr stream live and are written incrementally by
`layerfs-workspace` to a host runtime-state directory. Bounded retained output survives
command exit, Workspace Commit, Workspace End, and container removal. They are
never stored in Store databases or included in a Commit unless a program
explicitly writes a log inside its Workspace.

`layerfs-monitor` retains the operation/execution measurement receipt (elapsed,
CPU/RSS, bytes emitted, result) and a pointer to the execution ID. It does not
own or duplicate stdout/stderr content.

Interactive shells use the real terminal initially; session metadata persists,
while a complete PTY transcript is opt-in rather than default.

## 6. Monitor subsystem API

`layerfs-monitor` owns observation; it never mutates Layer, Stack, Branch, or
Workspace state.

```rust
Monitor::snapshot(scope: MonitorScope) -> Result<MonitorSnapshot>
Monitor::analyze_dedup(route: BranchStoreId) -> Result<DedupAnalysis>

pub enum MonitorScope {
    Databases,
    Dedup { route: Option<BranchStoreId> },
    Workspace(Option<WorkspaceSessionId>),
    Branch(BranchId),
    Operation(Option<OperationId>),
    Process,
}
```

`None` means aggregate/list and `Some(id)` means detail. Transfer detail belongs
inside operation receipts. `analyze_dedup` remains separate because it performs
an explicitly requested exact route inventory and must not run on every frame
or operation. Recorder creation is internal instrumentation, not another user
operation.

Every semantic operation returns/emits a bounded `OperationReceipt` owned by
Monitor. Raw domain counters remain owned by the component that measures them:

```text
layerfs-storage       object/fact admission, transactions, transfer sets
workspace             COW/spool/dirty/open-handle snapshot
fuse/materialization  projection counters/receipt
workspace             session/execution identity and output byte totals
monitor               operation timing, formulas, aggregation and retention
```

The primary cohort result is byte-weighted and coverage-labelled:

```text
dedup_saved_bytes = candidate_bytes - unique_candidate_bytes
dedup_saved_rate  = dedup_saved_bytes / candidate_bytes
collapse_factor   = candidate_bytes / unique_candidate_bytes

10 equivalent installs -> 1 canonical payload set per required DB
```

Required independent Branch/Stack/Layer placements are displayed separately.
Active Workspace spool/materialized allocation is also separate and never
presented as committed CAS reuse.

## 7. Standalone CLI contract

The `layerfs` binary is complete without the TUI. The TUI consumes the
`layerfs-cli` Rust library in-process and never parses headless formatted text.

### 7.1 Database commands

```text
layerfs db create <layer|stack|branch> <location>
layerfs db connect <layer|stack|branch> <location>
layerfs db use <location>
layerfs db disconnect <location>
layerfs db list
```

### 7.2 Layer commands

```text
layerfs layer init <directory>
layerfs layer init --empty
layerfs layer pull <layer-id>
layerfs layer add --from <branch-id>@<commit-id>
layerfs layer add --from <stack-id>
layerfs layer list
layerfs layer show <layer-id|layer-history-id>
```

### 7.3 Stack commands

```text
layerfs stack create --from <layer-id>
layerfs stack pull <stack-id>
layerfs stack add --from <branch-id>@<commit-id>
layerfs stack push <stack-id>
layerfs stack list
layerfs stack show <stack-id|stack-history-id>
```

### 7.4 Branch commands

```text
layerfs branch create --from <layer-id>
layerfs branch create --from <stack-id>
layerfs branch create --from <branch-id>@<commit-id>
layerfs branch merge <source-branch-id> --into <target-branch-id>
layerfs branch pull <branch-id>
layerfs branch push <branch-id>
layerfs branch pull-commits <branch-id>
layerfs branch list
layerfs branch show <branch-id|branch-id@commit-id>
layerfs branch diff <left-commit-id> <right-commit-id>
```

### 7.5 Workspace commands

```text
layerfs workspace create <branch-id> --at <root>
    [--container <container-id>]
    [--projection fuse|materialize]

layerfs workspace shell <workspace-id>
layerfs workspace exec <workspace-id> -- <program> [arguments...]
layerfs workspace output <execution-id> [--follow]
layerfs workspace stop <execution-id>
layerfs workspace commit <workspace-id>
layerfs workspace end <workspace-id> [--discard]
layerfs workspace list
layerfs workspace show <workspace-id>
layerfs workspace diff <workspace-id>
```

### 7.6 Monitoring commands

```text
layerfs monitor db
layerfs monitor dedup [--route <branch-store-id>] [--analyze]
layerfs monitor workspace [workspace-id]
layerfs monitor branch <branch-id>
layerfs monitor operation [operation-id]
layerfs monitor process
```

Every mutating command supports structured semantic results. One global
`--json` option selects machine rendering; it is not a Store or SDK protocol.
There are no Phase One command aliases. `show` renders an object or its history,
Workspace detail includes executions, Workspace list includes bounded retained
sessions, and operation receipts include transfer detail.

## 8. CLI-to-subsystem operation mapping

| CLI | Semantic call | Implementation/persistence owner |
|---|---|---|
| `db create/connect layer` | LayerStore create/connect | LayerStore |
| `db create/connect stack` | StackStore create/connect using active Layer endpoint | StackStore |
| `db create/connect branch` | BranchStore create/connect using active Stack or Layer endpoint | BranchStore |
| `layer init` | `initialize` | LayerStore bootstrap |
| `layer pull L` | `pull_layer(L)` | active StackStore |
| `layer add --from X` | `add_layer(LayerSource)` | LayerStore |
| `stack create --from L` | `create_stack(L)` | StackStore |
| `stack pull S` | `pull_stack(S)` | StackStore |
| `stack add --from B@C` | `add_stack(BranchCommit)` | StackStore |
| `stack push S` | `push_stack(S)` | StackStore -> LayerStore transfer |
| `branch create --from X` | `create_branch(BranchSource)` | BranchStore |
| `branch merge` | semantic Merge with internally pinned target head | BranchStore |
| `branch pull/push/pull-commits` | corresponding Branch transfer | BranchStore + parent |
| `workspace create` | `Workspaces::create_workspace_session` | `layerfs-workspace`; zero Store rows |
| `workspace exec/shell` | `Workspaces::exec/shell` | `layerfs-workspace`; bounded output outside Stores |
| `workspace commit` | `Workspaces::commit_workspace_session` -> final delta -> BranchStore exact CAS | `layerfs-workspace` orchestration + BranchStore persistence |
| `workspace end` | `Workspaces::end_workspace_session` | `layerfs-workspace` projection/spool cleanup only |
| `monitor ...` | `Monitor::snapshot` or explicit `analyze_dedup` | `layerfs-monitor`; no Store metric tables |

## 9. Structured results and errors

Domain subsystems return typed semantic values and raw domain receipts;
`layerfs-monitor` composes the bounded operation receipt. CLI and TUI render
the same frontend-neutral completion independently.

```rust
pub enum WorkspaceCommitResult {
    Created {
        previous_head: CommitId,
        commit_id: CommitId,
        receipt: WorkspaceCommitReceipt,
    },
    UpToDate {
        head: CommitId,
        receipt: WorkspaceCommitReceipt,
    },
    HeadMoved {
        expected: CommitId,
        actual: CommitId,
    },
}
```

The common user-visible result vocabulary is:

```text
CREATED
FAST_FORWARDED
UP_TO_DATE
CONFLICT
HEAD_MOVED
WRONG_HISTORY
READ_ONLY
WORKSPACE_BUSY
WORKSPACE_DIRTY
INTERRUPTED
NOT_FOUND
INTEGRITY_ERROR
```

Conflict, HeadMoved, and Busy outcomes expose no partial accepted state. A
failed Workspace Commit leaves the Workspace recoverable and inspectable.

## 10. Phase One frontend contract

`layerfs-cli` supplies a reusable Rust library as well as the standalone
`layerfs` binary. Phase One freezes this concrete library seam before any TUI
code exists:

```rust
CliSession::open(context_location) -> Result<CliSession>
CliSession::parse_line(input: &str) -> Result<Command>
CliSession::plan(command: &Command) -> Result<CommandPlan>
CliSession::execute(command: Command) -> Result<OperationHandle>
CliSession::complete(input: &str, cursor: usize) -> Result<Vec<Completion>>
CliSession::snapshot(query: ViewQuery) -> Result<ViewSnapshot>

OperationHandle::interrupt() -> Result<()>
```

`CommandPlan` resolves the active route, Store/Branch targets, Workspace
placement/projection, expected effects, conflicts with saved context, and
whether confirmation is required—without mutation. `ViewQuery` covers paged
topology/history/details, Workspace/execution/output, and Monitor snapshots.
Output pages expose stable byte/line cursors and truncation markers; live follow
uses the operation event stream.

One bounded event enum serves standalone human output, JSON, and Phase Two:

```rust
pub enum CliEvent {
    Started { operation_id: OperationId, command: CommandSummary },
    Progress { operation_id: OperationId, phase: OperationPhase,
               progress: ProgressValue, elapsed_ns: u64 },
    Output { execution_id: ExecutionId, sequence: u64,
             stream: OutputStream, bytes: Vec<u8> },
    Snapshot { scope: ViewScope, snapshot: ViewSnapshot },
    Finished { operation_id: OperationId,
               result: Result<CommandResult, CliError>,
               receipt: OperationReceipt },
}
```

Progress and gauges coalesce under backpressure; stdout/stderr are continuously
drained and retain order through `sequence`. Every `Finished`, including fast
semantic validation failure, carries one queue/service `OperationReceipt`
finalized before rendering. The standalone binary separately finalizes a
`CliInvocationReceipt` after output flush for parse/plan/context/wait/render
time; a parse/plan failure has only that outer receipt. Phase One freezes typed
ID prefixes, event ordering, error/result discriminants, JSON schema version,
output-byte semantics, pagination, and interrupt/stop behavior.

`OperationHandle::interrupt` requests cancellation of that CLI operation and is
distinct from `workspace stop <execution-id>`, which stops a Workspace child
process. Once interruption is accepted, already-ordered Output may drain, then
exactly one `Finished` with `INTERRUPTED` and its receipt closes the operation;
no later event for that operation is valid. A non-interruptible atomic section
finishes before the cancellation result and never exposes partial accepted
state.

Phase Two opens this same session and never calls SDK, Workspace, Monitor,
Docker, or Stores directly; it never parses human or JSON output. Needing a new
backend query or event during ordinary Phase Two work means Phase One was not
complete.

## 11. Required destructive changes

The clean implementation should delete or demote the following current public
shape rather than wrap it indefinitely:

| Current shape | Replacement |
|---|---|
| `LayerStore::open`, `StackStore::open`, `BranchStore::open` as ambiguous public entry | explicit `create` and `connect`; private shared open helper if useful |
| global `Direct` and `Stacked` product façades | explicit progressive Store handles; retain only temporary tests/convenience if proven necessary |
| `layerfs-core` package | `layerfs-content`; reorganize as `object`, `file`, `tree`, and `filesystem`, absorbing pure change/reference/merge behavior extracted from current Storage |
| old `layerfs-storage` plus current `layerfs-storage-core` | one clean `layerfs-storage` containing only Store-domain persistence/candidate/admission/transfer behavior |
| mixed Storage `three_way.rs` / `merkle.rs` | Content merge/change/reference algorithms + Storage candidate/transfer modules + Workspace spool staging |
| current COW-only `layerfs-workspace` | retain and expand into the one complete Workspace subsystem; no second Core/Overlay package |
| SDK `workspace(branch, spool)` | `Workspaces::create_workspace_session(CreateWorkspaceSession)` |
| SDK `finalize(workspace)` | `Workspaces::commit_workspace_session(id)` |
| implicit End/finalize | `end_workspace_session(id, mode)` cleanup only; never capture/Commit |
| edit/tool-operation history as Commit input | one canonical final delta against the pinned base |
| SDK workspace/placement/execution implementation files | the one `layerfs-workspace` crate |
| SDK receipt/monitor implementation files | `layerfs-monitor` crate |
| private/mixed/dropped transfer receipt | object/fact-separated domain receipt consumed by Monitor |
| mount process exit implies finalize/discard | explicit Commit and End control; crash never implies Commit |
| three public `create_branch_from_*` SDK calls | `create_branch(BranchSource)` |
| caller supplies history ID plus Layer/Stack ID | derive history from immutable record |
| caller supplies expected Commit head | session/semantic operation pins it internally; Store CAS remains exact |
| `add_stack(history, branch, commit)` | `add_stack(BranchCommit)` |
| `add_layer(history, source)` | `add_layer(LayerSource)` |
| container mount binary accepts DB paths | thin FUSE projection accepts Workspace endpoint/capability only |
| one Store/Workspace owner per CLI process | one context-scoped local CLI host with per-UUID Workspace workers |
| no persistent command receipt model | bounded host-side Workspace/Execution receipts outside Store DBs |

No compatibility façade is required for the cold replacement unless a real
external consumer is identified and given a dated removal plan.

## 12. Acceptance gates

The CLI/subsystem contract is not complete until tests prove:

1. `create` refuses an existing Store; `connect` refuses missing/wrong-role
   Stores and never creates.
2. One LayerStore connects to multiple StackStores and direct BranchStores;
   each StackStore connects to multiple BranchStores without silent reparenting.
3. Every CLI command parses in argv and reusable command-line form to the same
   typed request and structured outcome; the Phase One frontend fixture consumes
   plan/completion/events/snapshots without Ratatui.
4. History IDs are derived and wrong-history attempts still fail before writes.
5. Expected Branch heads are hidden from ordinary CLI arguments but exact CAS
   prevents lost updates.
6. One Branch can own simultaneous host and Docker Workspaces at different
   bases; sessions never observe each other's uncommitted state.
7. Docker Workspaces access host-owned DBs only through a session-scoped FUSE
   projection and never receive DB paths or Store credentials.
8. Commit refuses active writers; distinct operation histories with the same
   pinned base/final state produce the same root/ObjectIds; success creates at
   most one Commit and leaves a read-only session; HeadMoved preserves the final
   delta; End never computes a delta or commits.
9. Live stdout/stderr remain bounded in memory, persist according to policy,
   and survive Workspace End without entering Store schemas.
10. CLI remains fully functional with no TUI crate or Ratatui/Crossterm
    dependency in Phase One; its frozen concrete frontend seam exposes every
    command, plan, completion, event, snapshot, output page, and interrupt.
11. Every CLI/SDK operation emits the timing and resource receipt defined by
    `04-monitoring-dedup-performance.md`.
12. `layerfs-content` has no SQLite/Storage/Store/Workspace/Monitor dependency;
    `layerfs-storage` depends one-way on Content and does not duplicate canonical
    parsing, CDC or filesystem merge behavior.
13. No spool `PathBuf` enters Content or persisted Store records; the one
    `layerfs-workspace` owns COW/spool and all host/container session setup;
    FUSE/materialization depend only on their narrow ports, not concrete
    Workspace; SDK owns neither Workspace nor Monitor implementation.
14. `layerfs-monitor` adds no Store table, never mutates domain state, separates
    objects from facts and local reuse from admission races, and retains all
    observation state within bounded limits.
15. Ten equivalent same-path install Commit results in one BranchStore report
    `90%` byte savings and `10 -> 1` collapse for the measured cohort, while
    direct/stacked required Store placements and active transient Workspace
    bytes remain separately labelled.
16. Separate CLI invocations reconnect to one context host; multiple Workspace
    workers share its typed Store handles without independently reopening the
    same local SQLite Store; host/worker/FUSE/container exit never commits.
