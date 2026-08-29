# LayerFS topology and resulting source tree

Status: **binding target for the CLI/TUI redesign**.

This is a clean target, not a compatibility-preserving refactor. Existing
types, files, and packages may be deleted or renamed when they do not fit this
model. The goal is the smallest coherent final architecture, not the smallest
patch against the current checkout.

## 1. Frozen mental model

LayerFS has three durable Store roles and one ephemeral execution role:

```text
LayerStore  -> authoritative Layer histories and complete accepted data
StackStore  -> optional intermediate Stack construction and selected copies
BranchStore -> Branch/Commit work and changed objects
Workspace   -> one ephemeral COW session against one Branch head
```

LayerFS itself is the coordinator, not a Store role, SQLite database, or
durable history. The context host owns connections, credentials, monitoring,
Workspace workers, and command/event routing; it creates no fifth history.

The immutable result types form an acceptance ladder:

```text
Workspace final delta -> Commit -> optional Stack -> Layer
```

| Result | Owning durable boundary | Mutable reference |
|---|---|---|
| Commit | BranchStore | Branch head |
| Stack | StackStore | StackHistory head |
| Layer | LayerStore | LayerHistory head |

StackStore is an optional intermediate history authority. It is not a cache:
it owns unpublished StackHistory, is not freely discardable while that history
exists, does not mirror all LayerStore data, and never advances LayerHistory.

There is no `Project` entity, `ProjectId`, project table, project package, or
`project` CLI group. A Layer history already identifies the authoritative
filesystem lineage. A Workspace is not a project directory and is not durable
history.

The controller host owns every Store handle, credential, Workspace session,
COW state, Commit/CAS decision, execution observation, and monitor. A Store
may ultimately be backed by a host-local database or a
configured remote Store endpoint, but the host is always the client and
authority boundary visible to a Workspace. Containers receive only a
session-scoped filesystem projection.

The standalone CLI runs one small user/context-scoped local LayerFS host so
separate terminal invocations reconnect to the same Store handles, Monitor,
and active Workspace sessions. It is not a public service or generic daemon.
Inside it, each Workspace UUID has one worker that serializes Commit/End while
serving bounded filesystem and execution work.

```text
terminal commands / future TUI
              |
              v
      local typed control socket
              |
              v
one LayerFS host per saved CLI context
    |-- one Store connection graph
    |-- one Monitor collector
    `-- Workspace worker w1..wN
```

A host, worker, FUSE helper, CLI, or container exit never implies Commit.

The two foundational packages have a strict one-way boundary:

```text
layerfs-content
    canonical objects, ObjectId, CDC, filesystem structure and pure content
    read/change/diff/merge/reference algorithms

layerfs-storage
    typed LayerFS history records/IDs, SQLite, admission, exact CAS,
    candidate spill, missing-only transfer and Store wire protocol
```

`layerfs-storage -> layerfs-content`; the reverse dependency is forbidden.
Host spool paths and uncommitted runtime staging belong to the single cohesive
`layerfs-workspace` crate, not Content or Storage.

`layerfs-monitor` is a separate monorepo crate. It owns operation receipts,
aggregation, resource sampling, persisted observation output, and bounded
retention. None of those responsibilities belongs to `layerfs-sdk`.

## 2. Cardinalities

### 2.1 Authority-first view

```text
LayerStore L1
│
├── StackStore S1
│   ├── BranchStore B1
│   │   ├── Branch br-1
│   │   │   ├── Workspace w-1  -> host:/tmp/layerfs/w-1
│   │   │   └── Workspace w-2  -> docker:agent-a:/workspaces/w-2
│   │   └── Branch br-2
│   │       └── Workspace w-3  -> docker:agent-a:/workspaces/w-3
│   └── BranchStore B2
│
├── StackStore S2
│   └── BranchStore B3
│
├── BranchStore B4             # direct, no StackStore
│   └── Branch br-4
│       └── Workspace w-4      -> host:/var/tmp/layerfs/w-4
│
└── BranchStore B5             # another direct BranchStore
```

The binding cardinalities are:

| Parent | Child | Cardinality | Rule |
|---|---|---:|---|
| LayerStore | StackStore | `0..N` | Every StackStore is connected to exactly one LayerStore. |
| LayerStore | direct BranchStore | `0..N` | A direct BranchStore has no hidden StackStore. |
| StackStore | BranchStore | `0..N` | Each such BranchStore has exactly one StackStore parent. |
| BranchStore | Branch | `0..N` | One database may serve many Branches; one database per Branch also remains valid. |
| Branch | active Workspace | `0..N` | Each Workspace pins one exact Branch head at creation. |
| container | projected Workspace | `0..N` | One prepared container may host many independent UUID mounts. |

Each BranchStore has exactly one immutable parent route:

```text
Direct:  BranchStore -> LayerStore
Stacked: BranchStore -> StackStore -> LayerStore
```

Attaching another Store must never silently reparent an existing BranchStore.
Create or connect a different BranchStore instead.

### 2.2 Data and publication direction

The setup UI is authority-first, but normal work flows toward authority:

```text
setup/navigation: LayerStore -> optional StackStore -> BranchStore -> Workspace

publication:      Workspace -> BranchStore -> optional StackStore -> LayerStore
```

The distinction is deliberate. It prevents the UI tree from being mistaken
for a write path and prevents a Workspace from receiving Store authority.

### 2.3 Layered filesystem resolution

A BranchStore stores locally created Branch payload rather than copying every
object inherited from its parent. The full logical Workspace view is:

```text
direct:
    Layer base + Branch Commit changes + Workspace final COW state

stacked:
    Layer base + Stack state + Branch Commit changes + Workspace final COW state
```

Object reads resolve in this fixed order:

```text
Workspace COW
    -> BranchStore
        -> optional StackStore
            -> LayerStore
```

A local integrity error is never treated as a cache miss; fallback occurs only
for an exact missing-object result. Each physical Store deduplicates by
ObjectId independently. Required copies in two or three Store databases are
placement, not failed deduplication.

### 2.4 Operation grammar

| Operation | Source -> destination | Binding effect |
|---|---|---|
| `commit_workspace_session` | final Workspace delta -> BranchStore | Create at most one Commit and exact-CAS Branch |
| `merge` | Branch -> Branch | Three-way integrate and exact-CAS target Branch |
| `push_branch` | BranchStore -> configured LayerStore/StackStore parent | Transfer missing immutable data; no Add or Merge |
| `add_stack` | Branch Commit -> StackHistory | Conflict-check, create one Stack, exact-CAS StackHistory |
| `push_stack` | StackStore -> LayerStore | Transfer missing immutable Stack/provenance data; no Layer creation |
| `add_layer` | Branch Commit or Stack -> LayerHistory | Conflict-check, create one Layer, exact-CAS LayerHistory |
| `pull_layer` / `pull_stack` / `pull_branch` / `pull_commits` | authority -> work tier | Transfer selected immutable state; no hidden Merge |
| `end_workspace_session` | Workspace runtime | Clean or discard only; no delta construction, Commit, Push, or Add |

`Push` and `Add` are intentionally separate. Availability of a complete
candidate upstream does not imply acceptance into StackHistory or LayerHistory.
`Merge` is Branch-to-Branch only; Branch-to-Stack and Branch/Stack-to-Layer
acceptance use `add_stack` and `add_layer`.

## 3. Store placement and connection graph

The SDK must replace fixed product-wide `Direct` and `Stacked` composition with
explicit Store handles and a host-side connection graph:

```text
Host LayerFS process
├── LayerConnection L1
├── StackConnection S1 -> L1
├── StackConnection S2 -> L1
├── BranchConnection B1 -> S1
├── BranchConnection B2 -> S1
├── BranchConnection B3 -> S2
├── BranchConnection B4 -> L1
└── Workspace registry
```

Connection metadata is application configuration, not filesystem truth. The
standalone CLI persists only enough local configuration to reopen Store
locations between process invocations. It must not add topology tables to any
Store or persist credentials in plain text.

The SDK exposes explicit `create` and `connect` behavior:

```text
create  -> location must be empty; create the exact Store-role schema
connect -> Store must already exist; reject missing or wrong-role schema
```

It must never silently create a database while executing `connect`, and it
must never silently open an existing database while executing `create`.

Store crates remain independent. A BranchStore or StackStore talks to its
parent through the existing typed endpoint/transport contract; it does not link
the parent Store crate. The SDK composes the handles.

## 4. Workspace placement and projection

A Workspace is a UUID-scoped, host-controlled COW transaction:

```text
create Workspace
    -> pin Branch head
    -> allocate host COW/spool state
    -> choose one presentation
    -> expose one UUID path

commit Workspace
    -> quiesce presentation and executions
    -> compare the final view with the pinned base
    -> collapse transient edits into one canonical final delta
    -> create one Commit and exact-CAS the Branch head
    -> keep the session available read-only

end Workspace
    -> no capture and no Commit
    -> unmount/close/remove transient state
```

The single `layerfs-workspace` subsystem owns this entire state machine:
transient COW tree, spool, UUID registry, placement, projection orchestration,
execution and output. Focused files preserve SRP; a second Workspace/Core/
Overlay package is unnecessary.

Tool calls, FUSE requests, shell commands, and intermediate file mutations are
not durable Branch history. A Workspace worker may track dirty state
incrementally for speed, but `commit_workspace_session` must be semantically
equivalent to a fresh base-to-final-view diff. Cancelled writes, create/delete,
and rename/undo sequences disappear. Only the canonical final root and minimal
Commit provenance enter the BranchStore.

Placement is chosen per Workspace, never stored on the Branch:

```text
Branch br-1
├── w-host    -> host materialized directory
├── w-linux   -> host directory, FUSE presentation
├── w-docker1 -> container A, thin FUSE projection
└── w-docker2 -> container A, another independent thin FUSE projection
```

### 4.1 Host presentation

```text
macOS host:
    host Workspace -> portable materialized directory -> tool process

Linux host:
    host Workspace -> FUSE mount or materialized directory -> tool process
```

`layerfs-materialization` owns portable native-directory exposure and capture.
`layerfs-fuse` owns FUSE presentation. High-level `layerfs-workspace` selects
and orchestrates them. Neither projection package owns Branch Commit policy.

### 4.2 Docker presentation

The canonical flexible Docker path is one thin FUSE proxy per Workspace:

```text
HOST
┌───────────────────────────────────────────────────────────────┐
│ layer.db   stack.db   branch.db                              │
│ Workspace session + COW/spool + Commit/CAS                   │
│                                                               │
│ Workspace w-a endpoint/token ─────────────────────────────┐    │
│ Workspace w-b endpoint/token ────────────────────────┐    │    │
└──────────────────────────────────────────────────────┼────┼────┘
                                                       │    │
DOCKER CONTAINER                                       │    │
┌──────────────────────────────────────────────────────┼────┼────┐
│ thin FUSE proxy A -> /workspaces/w-a <───────────────┘    │    │
│ thin FUSE proxy B -> /workspaces/w-b <────────────────────┘    │
│ tool/agent processes                                            │
└─────────────────────────────────────────────────────────────────┘
```

The container receives only:

```text
Workspace UUID
session endpoint
ephemeral per-Workspace capability
mount path
thin FUSE projection binary injected at runtime
```

The container must not receive:

```text
SQLite files or database paths
Store credentials
Store implementations
Branch/Stack/Layer authority
Commit or CAS authority
another Workspace's capability
```

Docker setup, helper injection, Docker exec/shell/stop, host directory setup,
projection selection, COW state and spool live only in `layerfs-workspace`.

The thin binary may be mounted read-only or copied into a runtime directory;
it need not be installed in the workload image. A FUSE projection requires
the container to be prepared with `/dev/fuse` and the required mount
capability. Host FUSE plus a Docker bind mount may remain a measured native
Linux optimization, but it is not the only product path and must not leak into
Store or Workspace semantics.

## 5. Package decisions

### 5.1 Resulting packages

| Package | Action | Sole responsibility |
|---|---|---|
| `layerfs-content` | **rename/reorganize from `layerfs-core` plus pure algorithms extracted from Storage** | Canonical objects, file content, filesystem-tree state, and pure whole-filesystem read/apply/diff/merge algorithms. |
| `layerfs-storage` | **replace old `layerfs-storage`; rename/reorganize current `layerfs-storage-core`** | Typed LayerFS history IDs/records, Store schemas/SQL, bounded candidate staging, merge-base selection, admission, exact CAS, missing-only transfer and byte protocol. |
| `layerfs-layer-store` | keep | Authoritative Layer persistence and Layer operations. |
| `layerfs-stack-store` | keep | Optional Stack construction, copies, and Stack operations. |
| `layerfs-branch-store` | keep | Branch/Commit persistence, merge, Commit CAS and Branch transfer. |
| `layerfs-workspace` | **keep and expand as one cohesive subsystem** | Transient COW/tree/spool, UUID sessions, Branch binding, placement/projection orchestration, create/commit/end, exec/shell/stop and bounded live/retained execution output. |
| `layerfs-materialization` | keep/refocus | Portable native-directory presentation and capture; APFS clone acceleration is deferred. |
| `layerfs-fuse` | **rename from `layerfs-mount`** | Host FUSE and thin container projection only. |
| `layerfs-monitor` | **create inside this monorepo** | Operation receipts/timing, Store/Workspace/execution aggregation, CPU/memory/storage/dedup sampling, and bounded operation-receipt retention. |
| `layerfs-sdk` | **destructively thin** | Public semantic Layer/Stack/Branch façade and composition of topology, Workspace, and monitor subsystems. |
| `layerfs-cli` | **create** | Complete standalone CLI plus reusable in-process command API for the TUI. |
| `layerfs-tui` | **create in Phase Two only** | Ratatui/Crossterm UI only; navigation, interaction and rendering. |
| `layerfs-eval` | keep | Benchmarks and acceptance verification, including mixed host/container Workspaces. |

The target has 12 production crates plus the existing `layerfs-eval` tool.
Content and Storage are the only foundational packages. Workspace remains one
cohesive package; FUSE/materialization define narrow backend ports and never
depend back on Workspace, so no second COW/Core/Overlay crate is needed.
Monitor remains downstream of all observed domains to keep observation out of
SDK and product truth. No additional runtime package is justified.

Do not create these packages now:

```text
layerfs-project
layerfs-terminal
layerfs-server
layerfs-sync
layerfs-transfer
layerfs-observability
layerfs-runtime
layerfs-workspace-session
layerfs-workspace-core
layerfs-overlay
layerfs-docker
layerfs-api
layerfs-web
layerfs-common
layerfs-utils
```

None has an independent current consumer or release boundary. Extract only
after a second real consumer forces duplication that cannot remain in the
existing responsible package.

The implementation has two terminal phases:

```text
Phase One = every package above except layerfs-tui
Phase Two = layerfs-tui only
```

Phase One does not scaffold, compile, test, or use Ratatui/Crossterm. It freezes
the typed `layerfs-cli` command, plan, event, snapshot, completion, and output
contracts before Phase Two begins. APFS immutable-base/clone acceleration
remains intentionally deferred beyond these phases.

### 5.2 Dependency graph

```text
layerfs-tui
    |
    v
layerfs-cli
    |
    v
layerfs-sdk
    |-- layerfs-layer-store
    |-- layerfs-stack-store
    |-- layerfs-branch-store
    |-- layerfs-workspace
    `-- layerfs-monitor

layerfs-monitor
    |-- layerfs-workspace
    |-- layerfs-layer-store
    |-- layerfs-stack-store
    `-- layerfs-branch-store

layerfs-workspace
    |-- layerfs-branch-store
    |-- layerfs-fuse
    |-- layerfs-materialization
    |-- layerfs-storage
    `-- layerfs-content

layerfs-fuse -------------------> layerfs-content
layerfs-materialization --------> layerfs-content
layerfs-storage ----------------> layerfs-content

three Store crates ------------> layerfs-storage
```

`layerfs-cli` is a fully usable product by itself. `layerfs-tui` adds a visual
interface by calling the CLI package's Rust library in-process; it must not
spawn a new CLI process for each action or parse formatted CLI output.

### 5.3 Why this graph has no cycle

Dependency direction always moves from composition toward mechanism:

```text
TUI -> CLI -> SDK -> {Monitor, Workspace, Stores}
                          |          |
                          |          +-> {Fuse, Materialization, Storage, Content}
                          +-> consumes Workspace/Store snapshots

Stores -> Storage -> Content
```

The critical backend-port rule is:

```text
FUSE defines the narrow filesystem port it consumes.
Materialization defines the narrow source/capture port it consumes.
Workspace implements/supplies those ports and owns all COW/session state.
```

Therefore `layerfs-workspace` may depend on FUSE and materialization without a
cycle; neither backend depends on the concrete Workspace crate. Content remains
pure and Storage depends on it. Workspace/Store crates never depend on Monitor.

Monitor is also downstream-only. Domain packages expose immutable outcome and
snapshot values; `layerfs-monitor` consumes them, samples external resources,
composes/persists receipts, and applies retention. Workspace and Store crates
must not call Monitor. The thin SDK invokes the two subsystems and passes
domain outcomes to Monitor; it does not implement monitoring itself.

The runtime adapter direction is:

```text
CLI starts the outer Monitor operation before parsing, or a direct SDK caller
starts it at SDK entry
    -> SDK opens/reuses the SDK child span and calls Store or high-level Workspace
    -> domain returns outcomes/snapshots or streams Workspace events
    -> SDK forwards those values to Monitor and to the caller
    -> Monitor closes/persists the receipt and applies retention
```

The SDK owns only that wiring. It owns no receipt schema, sampler, aggregation
formula, output file, retention decision, Workspace registry, placement
adapter, or execution process.

This is a directed acyclic graph. A future change that makes FUSE or
materialization depend on the concrete Workspace crate, Content depend on
Storage, Workspace depend on Monitor, or a Store depend on SDK would create an
ownership inversion and is forbidden.

## 6. Exact target production source tree

Files marked **keep** retain their current responsibility. A file may be
rewritten completely while retaining its name. `lib.rs` files contain module
declarations and deliberate re-exports only. Binary `main` files contain
bootstrap only.

```text
crates/
├── layerfs-content/                           # RENAME/REORGANIZE current core + pure storage algorithms
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── error.rs
│       ├── limits.rs
│       ├── object/                           # canonical CAS object foundation
│       │   ├── mod.rs
│       │   ├── id.rs                       # ObjectId and typed content-root IDs
│       │   ├── digest.rs                   # canonical digest implementation
│       │   ├── canonical.rs                # CanonicalObject + identity validation
│       │   ├── codec.rs                    # object envelope encoding/decoding
│       │   ├── references.rs               # canonical child-reference decoding
│       │   └── access.rs                   # ObjectRead/ObjectStore ports
│       ├── file/                             # regular-file content only
│       │   ├── mod.rs
│       │   ├── extent.rs
│       │   ├── extent_codec.rs
│       │   ├── cdc/
│       │   │   ├── mod.rs
│       │   │   └── gear.rs
│       │   └── rope/
│       │       ├── mod.rs
│       │       ├── state.rs
│       │       ├── build.rs
│       │       ├── edit.rs
│       │       ├── read.rs
│       │       ├── diff.rs
│       │       └── validate.rs
│       ├── tree/                             # directory/inode/metadata tree only
│       │   ├── mod.rs
│       │   ├── path.rs                     # CanonicalPath and CanonicalName
│       │   ├── root.rs                     # tree-root model and linkage
│       │   ├── directory/
│       │   │   ├── mod.rs
│       │   │   ├── node.rs
│       │   │   ├── codec.rs
│       │   │   ├── read.rs
│       │   │   ├── edit.rs
│       │   │   ├── diff.rs
│       │   │   ├── merge.rs
│       │   │   └── validate.rs
│       │   ├── inode/
│       │   │   ├── mod.rs
│       │   │   ├── record.rs
│       │   │   ├── codec.rs
│       │   │   ├── table.rs
│       │   │   ├── cursor.rs
│       │   │   └── merge.rs
│       │   └── metadata/
│       │       ├── mod.rs
│       │       ├── portable.rs
│       │       ├── apple_acl.rs
│       │       ├── codec.rs
│       │       ├── tree.rs
│       │       └── merge.rs
│       └── filesystem/                       # whole-filesystem operations
│           ├── mod.rs
│           ├── change.rs                      # ContentChange/ContentConflict/counters
│           ├── root.rs                        # empty-root construction/validation
│           ├── resolve.rs                     # canonical path resolution
│           ├── read.rs
│           ├── apply.rs
│           ├── diff.rs
│           └── merge.rs                       # pure three-way filesystem merge
│
├── layerfs-storage/                          # NEW implementation replaces old storage package
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── error.rs                      # DB/authority/transport errors + Content wrapper
│       ├── ids.rs
│       ├── records.rs
│       ├── contract.rs                   # StoreEndpoint and semantic transfer contracts
│       ├── schema.rs
│       ├── sql.rs
│       ├── admission.rs                  # membership/validation/idempotent DB admission
│       ├── candidate.rs                  # bounded object staging + scratch spill + Content adapter
│       ├── merge_base.rs                 # Commit/Stack/Layer history-base selection
│       ├── transfer.rs                   # closure traversal, batching, pipeline and raw receipt
│       └── wire.rs
│
├── layerfs-layer-store/                     # KEEP
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── layer_store.rs
│       ├── provision.rs
│       ├── add_layer.rs
│       ├── transfer.rs
│       ├── remote.rs
│       └── bin/layerfs-layer-store.rs
│
├── layerfs-stack-store/                     # KEEP
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── stack_store.rs
│       ├── writer.rs
│       ├── create_history.rs
│       ├── add_stack.rs
│       ├── history_pull.rs
│       ├── commit_pull.rs
│       ├── branch_transfer.rs
│       ├── push_stack.rs
│       ├── remote.rs
│       └── bin/layerfs-stack-store.rs
│
├── layerfs-branch-store/                    # KEEP
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── branch_store.rs
│       ├── create_branch.rs
│       ├── commit.rs
│       ├── merge.rs
│       ├── branch_transfer.rs
│       ├── layered_read.rs
│       └── snapshot.rs
│
├── layerfs-workspace/                       # KEEP/EXPAND as the one complete Workspace subsystem
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── cow_tree.rs                     # transient nodes, base refs, dirty namespace state
│       ├── file_io.rs                      # reads/writes/ranges/truncate/spool access
│       ├── changes.rs                      # WorkspaceChange + spool readers -> Content editor
│       ├── limits.rs                       # bounded memory/spool/finalize policy
│       ├── session.rs                      # UUID, Branch/base, state and public session values
│       ├── registry.rs                     # active session ownership and lookup
│       ├── worker.rs                       # one serialized worker per active Workspace UUID
│       ├── lifecycle.rs                    # explicit create/commit/end session state machine
│       ├── placement.rs                    # host/container placement validation
│       ├── projection.rs                   # implements FUSE/materialization ports and owns handles
│       ├── docker.rs                       # helper injection and container lifecycle/exec plumbing
│       ├── execution.rs                    # exec/shell/stop; host and Docker dispatch
│       └── output.rs                       # bounded live/retained stdout/stderr writer and lookup
│
├── layerfs-materialization/                 # REFOCUS
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── port.rs                        # source/capture contract implemented by Workspace
│       ├── materialize.rs                  # portable exact-root directory exposure
│       └── capture.rs                      # native directory -> bounded Workspace changes
│
├── layerfs-fuse/                            # RENAME from layerfs-mount
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── port.rs                        # filesystem contract implemented by Workspace
│       ├── adapter.rs                      # FUSE attributes, errno and request translation
│       ├── filesystem.rs                   # fuser::Filesystem callbacks only
│       ├── inode_table.rs                  # kernel inode mapping only
│       ├── handles.rs                      # open file/directory handles only
│       ├── host_mount.rs                   # host Workspace mount lifecycle
│       ├── protocol.rs                     # bounded session filesystem messages
│       ├── proxy_host.rs                   # host endpoint for one Workspace capability
│       ├── proxy_client.rs                 # thin in-container request client
│       └── bin/layerfs-fuse.rs             # thin/helper bootstrap only
│
├── layerfs-monitor/                         # NEW observation subsystem
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── operation.rs                    # operation envelope, IDs, outcomes and domain receipts
│       ├── timing.rs                       # monotonic spans and fixed histograms
│       ├── dedup.rs                        # exact covered-byte/set formulas and hero snapshot
│       ├── route.rs                        # streaming DB inventory union/placement analysis
│       ├── resource.rs                     # process/container CPU/RSS sampling
│       ├── collector.rs                    # bounded receipts/gauges and Branch aggregation
│       ├── retention.rs                    # bounded operation JSONL rotation
│       └── snapshot.rs                     # presentation-ready frontend-neutral values
│
├── layerfs-sdk/                             # DESTRUCTIVELY THIN COMPOSITION
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── client.rs                       # thin public handle composing topology/workspace/monitor
│       ├── location.rs                     # local DB/remote endpoint locations
│       ├── connection.rs                   # create/connect validation for each Store role
│       ├── topology.rs                     # one-to-many Store graph and route lookup
│       ├── layer.rs                        # semantic Layer façade
│       ├── stack.rs                        # semantic Stack façade
│       ├── branch.rs                       # semantic Branch façade
│       └── result.rs                       # frontend-neutral semantic results/errors
│
├── layerfs-cli/                             # NEW standalone CLI + Rust library
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── command.rs                      # complete db/layer/stack/branch/workspace/monitor grammar
│       ├── parse.rs                        # argv and TUI command-line parsing
│       ├── context.rs                      # context location, host socket and Store locations
│       ├── host.rs                         # one local owner for Store graph/Monitor/Workspace workers
│       ├── control.rs                      # bounded typed local command/event IPC
│       ├── plan.rs                         # resolved route/effect preview without mutation
│       ├── query.rs                        # frontend-neutral paged snapshots/read model
│       ├── execute.rs                      # typed command -> SDK call/events
│       ├── completion.rs                   # static grammar + dynamic ID candidates
│       ├── event.rs                        # progress/output/completion event model
│       ├── output.rs                       # standalone human/JSON rendering
│       └── bin/layerfs.rs                  # thin command/client or hidden local-host bootstrap
│
└── layerfs-tui/                             # PHASE TWO only; absent during Phase One
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        ├── app.rs                          # UI state only
        ├── event.rs                        # Crossterm input/resize/frame timing
        ├── input.rs                        # line editing and completion selection
        ├── navigation.rs                   # Store/Branch/Workspace tree selection
        ├── render.rs                       # frame layout; delegates concrete panes
        ├── topology_view.rs                # Layer -> Stack/direct Branch tree
        ├── history_view.rs                 # Layer/Stack/Commit lineage
        ├── workspace_view.rs               # placement, execution and lifecycle UI
        ├── monitor_view.rs                 # CPU/memory/storage/dedup/timing UI
        ├── output_view.rs                  # live and retained command output
        ├── theme.rs                        # colors/symbols derived from the approved art direction
        └── bin/layerfs-tui.rs              # terminal enter/restore and run only

tools/
└── layerfs-eval/                            # KEEP
    ├── Cargo.toml
    └── src/main.rs

docs/tui/
└── layerfs.png                              # canonical TUI identity image
```

The internal Content dependency direction is fixed:

```text
             filesystem
              /      \
           tree      file
              \      /
               object
```

`object` defines identity, canonical envelopes, references, and access ports.
`file` defines byte content. `tree` defines paths, directories, inodes, and
metadata. `filesystem` composes those domains into root-level operations.
Dependencies never point upward, and no algorithm may be duplicated across
the four domains. Every `mod.rs` is declarations/re-exports only.

The FUSE crate uses optional features rather than a second client crate:

```text
default/host feature -> FUSE over the port implemented by in-process Workspace
proxy feature        -> thin client/protocol only; no Store or SQLite dependency
```

The injected container binary is built with the proxy-only feature. Do not
split out `layerfs-fuse-client` unless another independent product consumes the
protocol and the single package causes proven dependency or release problems.

Domain packages may return narrow measurement fragments beside their semantic
outcomes—for example transfer counts from Storage or content-build/COW/spool
counters from Content/Workspace. They do not own clocks, aggregation, persisted receipts,
CPU/RSS sampling, operation-receipt retention, or deduplication presentation.
Those are the exclusive responsibility of `layerfs-monitor`. Raw execution
stdout/stderr retention remains the exclusive responsibility of the high-level
Workspace subsystem.

## 7. File and package ownership

| Concern | Sole owner | Explicit non-owner |
|---|---|---|
| Canonical object identity/model, CDC, references and pure filesystem transformations | `layerfs-content` | Storage policy, SQLite, Store history, Workspace spool, FUSE, SDK, CLI, TUI |
| Typed history records/IDs, Store DDL/SQL/candidate/admission/CAS/transfer mechanics | `layerfs-storage` | Content format/merge semantics, CLI/TUI and Workspace runtime |
| Layer authority | `layerfs-layer-store` | SDK and Workspace |
| Stack authority | `layerfs-stack-store` | SDK and Workspace |
| Branch/Commit authority | `layerfs-branch-store` | Workspace presentation and TUI |
| COW/spool plus Workspace UUID/session/placement/execution/output lifecycle | `layerfs-workspace` | Content algorithms, Storage internals, Monitor receipt retention and UI state |
| FUSE request/projection mechanics | `layerfs-fuse` | Store access, Commit policy, Docker command execution/setup |
| Portable materialized directory mechanics | `layerfs-materialization` | Store access, Branch CAS, and deferred APFS clone policy |
| Operation receipts, timing, aggregation, sampling and receipt retention | `layerfs-monitor` | Store truth, Workspace lifecycle, raw execution output and UI rendering |
| Store graph and thin semantic composition | `layerfs-sdk` | Workspace implementation, monitoring implementation, CLI syntax and UI state |
| Standalone commands and reusable command API | `layerfs-cli` | Ratatui rendering and Store algorithms |
| Visual navigation and interaction | `layerfs-tui` | SDK calls, SQLite, Docker calls, command semantics |
| Benchmarks/acceptance proof | `layerfs-eval` | production policy |

Monitoring is a separate monorepo concern because Store inventory, route-level
deduplication, Workspace execution measurements, OS/container sampling, and
persisted operation receipts form one coherent observation subsystem used by
both standalone CLI and TUI. Raw execution stdout/stderr and its retention
policy remain in `layerfs-workspace`. Monitor still adds no metrics table or
monitoring database to any Store. TUI graph rendering and CLI text/JSON
rendering remain presentation-specific.

## 8. Forbidden dependencies and shortcuts

| Package | Must not depend on or do |
|---|---|
| Store crates | Depend on another Store crate; know Workspace UUID/path/container; call TUI/CLI; copy shared merge, admission or transfer algorithms. |
| `layerfs-content` | Depend on SQLite/Storage/Store/Workspace/Monitor/SDK; know Branch/Commit/Stack/Layer IDs, spool `PathBuf`, transfer pages or wire framing. |
| `layerfs-storage` | Reimplement canonical decoding/references/CDC/filesystem merge; know Workspace UUID/path/container; render CLI/TUI. |
| `layerfs-workspace` | Open SQLite directly; Push/Add; reimplement Content algorithms or Storage admission; persist Monitor receipts; depend on Monitor; know terminal UI. |
| `layerfs-fuse` | Depend on concrete `layerfs-workspace`; open Store DBs; construct topology; Commit a Workspace implicitly on unmount or crash. |
| `layerfs-materialization` | Depend on concrete `layerfs-workspace`; open Store DBs; own Branch CAS; define canonical identity. |
| `layerfs-monitor` | Mutate Store/Workspace truth; become a required Store write-path dependency; implement lifecycle; render CLI/TUI; add metrics tables. |
| `layerfs-sdk` | Implement Workspace placement/execution; implement receipt/retention/sampling/aggregation; parse Clap syntax; render terminal output; add hidden databases. |
| `layerfs-cli` | Implement storage algorithms; query SQLite directly; depend on Ratatui/Crossterm. |
| `layerfs-tui` | Depend directly on `layerfs-sdk`; open Stores; call Docker; parse formatted CLI output; duplicate command grammar. |
| Docker proxy | Receive DB path/credential; create Commit; access more than its one Workspace capability. |

Global forbidden shortcuts:

```text
no hidden StackStore in direct mode
no raw FullStorage product API
no Workspace table in any Store
no monitoring/receipt table in any Store
no Project entity or package
no automatic Commit on Workspace end/unmount/process exit
no permanent Branch -> workspace-directory binding
no tool/FUSE/command operation log in Commit identity or Store schemas
no implicit capture during End, unmount, host exit or worker exit
no global public daemon or one Store owner per Workspace process
no one global Direct/Stacked SDK object that prevents N children
no TUI-specific LayerFS implementation
no second Workspace/Core/Overlay package
no spool path or Store-domain ID in layerfs-content
no canonical reference decoder or filesystem merge implementation in layerfs-storage
no Workspace or Store crate depending back on layerfs-monitor
no compatibility wrapper retained without a proven external consumer
no layerfs-tui crate, Ratatui or Crossterm during Phase One
```

## 9. Destructive migration manifest

### Create

```text
crates/layerfs-monitor/
crates/layerfs-cli/
crates/layerfs-tui/                         # Phase Two only
```

`layerfs-workspace` already exists; retain its COW implementation and add the
focused session/placement/projection/execution/output files rather than
creating a second Workspace package.

### Rename

```text
crates/layerfs-core/                        -> crates/layerfs-content/
package layerfs-core                        -> layerfs-content
Rust crate layerfs_core                     -> layerfs_content
crates/layerfs-storage-core/                -> crates/layerfs-storage/
package layerfs-storage-core                -> layerfs-storage
Rust crate layerfs_storage_core             -> layerfs_storage
crates/layerfs-content/src/content/extent*  -> crates/layerfs-content/src/file/
crates/layerfs-content/src/content/rope.rs  -> crates/layerfs-content/src/file/rope/
crates/layerfs-content/src/cdc/             -> crates/layerfs-content/src/file/cdc/
crates/layerfs-content/src/identity/        -> crates/layerfs-content/src/object/{id,digest}.rs
crates/layerfs-content/src/object/model.rs  -> crates/layerfs-content/src/object/canonical.rs
crates/layerfs-content/src/format/path.rs   -> crates/layerfs-content/src/tree/path.rs
crates/layerfs-content/src/namespace.rs     -> crates/layerfs-content/src/tree/{root,directory}/
namespace_codec.rs directory portions       -> crates/layerfs-content/src/tree/directory/{node,codec}.rs
namespace_codec.rs inode portions           -> crates/layerfs-content/src/tree/inode/codec.rs
crates/layerfs-content/src/inode.rs         -> crates/layerfs-content/src/tree/inode/
crates/layerfs-content/src/metadata.rs      -> crates/layerfs-content/src/tree/metadata/
crates/layerfs-content/src/logical/         -> crates/layerfs-content/src/filesystem/
layerfs-workspace/src/overlay.rs            -> cow_tree.rs
layerfs-workspace/src/resource.rs           -> limits.rs
crates/layerfs-mount/                       -> crates/layerfs-fuse/
package layerfs-mount                       -> layerfs-fuse
binary layerfs-mount                        -> layerfs-fuse
mount_session.rs                            -> host_mount.rs
```

Update the workspace manifest, container build, benchmark commands, package
references and dependency names in one atomic responsibility move.

Reorganize the mixed current Storage files in the same move:

```text
storage-core/three_way.rs pure filesystem merge -> content/filesystem/merge.rs
storage-core/merkle.rs content changes -> content/filesystem/change.rs
storage-core/merkle.rs child references -> content/object/references.rs
storage-core/merkle.rs candidate scratch/spill -> storage/candidate.rs
storage-core/merkle.rs transfer closure/order -> storage/transfer.rs
storage-core/admission.rs TransferPipeline -> storage/transfer.rs
storage-core/contract.rs Change/Conflict -> content/filesystem/change.rs
storage-core/contract.rs StagedChange/spool PathBuf -> workspace/changes.rs
storage-core/wire.rs CanonicalObject -> content/object/canonical.rs
```

### Delete or replace

```text
layerfs-sdk/src/direct.rs                   delete after semantic route parity
layerfs-sdk/src/stacked.rs                  delete after semantic route parity
layerfs-sdk/src/binding.rs                  delete; high-level Workspace owns session binding
layerfs-sdk/src/endpoint.rs                 replace with location/connection/topology ownership
layerfs-sdk/src/workspace.rs                do not create; high-level Workspace owns it
layerfs-sdk/src/placement.rs                do not create; high-level Workspace owns it
layerfs-sdk/src/execution.rs                do not create; high-level Workspace owns it
layerfs-sdk/src/receipt.rs                  do not create; Monitor owns it
layerfs-sdk/src/monitor.rs                  do not create; Monitor owns it
layerfs-storage/src/three_way.rs            do not create; Content owns filesystem merge
layerfs-storage/src/merkle.rs               do not create; split by candidate/transfer/content ownership
```

Do not preserve deprecated façade types merely to keep old tests compiling.
Port tests to the semantic SDK and delete compatibility tests once parity is
proved. Existing Store primitives may keep their precise internal operation
names even when the public SDK and CLI present a smaller source-based method.

### Keep

```text
layerfs-layer-store
layerfs-stack-store
layerfs-branch-store
layerfs-workspace
layerfs-materialization
layerfs-eval
```

Keep means preserve the responsibility, not freeze every current line.
Duplicate algorithms, repeated protocol/session code, placeholders and stale
fixed-topology adapters should still be deleted.

## 10. SRP and size gates

- One file owns one named responsibility from this document.
- `lib.rs` is declarations/re-exports only.
- Binary `main` files bootstrap only.
- No `common.rs`, `utils.rs`, `manager.rs`, `product.rs`, or equivalent catch-all.
- No implementation-bearing module file may become a disguised god file.
- A handwritten production file at 350 lines triggers a responsibility review,
  not an automatic split.
- A handwritten production file must not exceed 1,500 lines, except
  `sql.rs` and `schema.rs`; declarative SQL/schema length is not a structural
  gate and must not cause mechanical splitting.
- Package and aggregate LOC estimates are soft review signals only.
- Minimize final duplication, dependencies, transactions and round trips; do
  not compress cohesive correct code to satisfy a cosmetic package budget.

The terminal architecture is complete only when the standalone CLI can perform
all supported operations without the TUI, the TUI performs them solely through
the CLI Rust library, and a Docker Workspace can operate through the thin FUSE
projection while every database and Commit decision remains outside the
container. Canonical content behavior must exist only in `layerfs-content`;
Store persistence/transfer behavior only in `layerfs-storage`; all COW/session/
placement/execution behavior in the one `layerfs-workspace`; and observation/
retention behavior only in `layerfs-monitor`. `layerfs-sdk` merely composes them.
