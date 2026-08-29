# LayerFS CLI, TUI, and Workspace-session architecture

This directory defines the proposed application-facing LayerFS architecture.
It is a clean redesign and may require destructive replacement of current SDK,
mount, package, and public-operation shapes. The goal is the simplest complete
architecture that remains flexible across local, remote, host, and container
execution—not the smallest patch against current code.

## Documents

Read in this order:

1. [Topology and source tree](01-topology-and-source-tree.md) — Store
   cardinalities, dependency boundaries, packages, files, keep/create/rename/
   delete decisions, and host/container projection ownership.
2. [CLI and SDK contract](02-cli-sdk-contract.md) — standalone CLI grammar,
   progressive database connection, semantic Rust APIs, Workspace lifecycle,
   execution streaming, and destructive public-API changes.
3. [TUI design](03-tui-design.md) — information architecture, navigation,
   Grok Build-inspired visual language, command interaction, topology/history/
   monitor views, output handling, and terminal accessibility.
4. [Monitoring, deduplication, and performance](04-monitoring-dedup-performance.md)
   — honest two-/three-database storage and reuse metrics, CPU/memory accounting,
   transfer/transaction measurement, and elapsed time for every operation.
5. [Phase One backend implementation plan](05-implementation-plan.md) — cold
   implementation of every backend, FUSE, monitoring, SDK, and standalone CLI
   responsibility, with no TUI code.
6. [Phase Two TUI implementation plan](06-tui-implementation-plan.md) — the
   Ratatui/Crossterm client built only after the Phase One frontend contract and
   terminal proof are frozen.
7. [Phase One handoff prompt](07-phase-one-handoff-prompt.md) — reusable
   execution mandate for the implementation owner; the binding Phase One plan
   remains authoritative.

The canonical TUI icon/avatar is [layerfs.png](../tui/layerfs.png). Preserve its
geometry and transparent background exactly; derive size/platform variants from
that source when a consuming surface exists.

## One-sentence model

LayerFS coordinates durable Layer/Stack/Branch Stores on their configured
hosts, lets each Branch spawn zero or more UUID-scoped ephemeral Workspaces,
and turns a Workspace's final delta into progressively more accepted immutable
history without making LayerFS itself another database.

## Responsibility model

LayerFS is the coordinator, not a fifth Store:

| Role | Owns | Produces | Does not own |
|---|---|---|---|
| LayerFS context host | Store graph, credentials, Monitor, Workspace workers, command/event routing | coordinated operations | another history or Store database |
| LayerStore | authoritative LayerHistory and accepted canonical data | immutable Layer | Workspace/tool activity, Branch or Stack head authority |
| StackStore | optional StackHistory, selected pulled Layer data, integrated candidates | immutable Stack | LayerHistory authority or transparent cache semantics |
| BranchStore | Branch heads, immutable Commits, locally created objects | immutable Commit | inherited parent payload copies or Workspace runtime state |
| Workspace | one pinned ephemeral COW view and final delta | candidate for one Commit | database tables, durable operation history, Push/Add authority |

The durable state ladder is:

```text
Workspace final delta
        |
        v
Branch Commit
        |
        +---------------------------> Layer       # direct route
        |
        `-> Stack -> Layer                         # stacked route
```

`Commit`, `Stack`, and `Layer` are immutable filesystem results at increasing
acceptance boundaries. The mutable references are their Branch, StackHistory,
and LayerHistory heads, each protected by exact CAS.

## Topology

```text
LayerStore (1)
├── StackStore (0..N)
│   └── BranchStore (0..N per StackStore)
└── BranchStore (0..N direct)

BranchStore (1)
└── Workspace (0..N ephemeral sessions)
    ├── host/materialized directory
    ├── host/FUSE
    └── Docker/container FUSE projection
```

Execution direction is always:

```text
Workspace -> BranchStore -> optional StackStore -> LayerStore
```

Authority and construction direction is:

```text
LayerStore
    accepted Layer histories

StackStore
    optional pulled Layer state and intermediate Stack construction

BranchStore
    mutable Branch refs and immutable Commits

Workspace
    ephemeral COW transaction; no database tables
```

Those downward arrows mean ownership/navigation. Publication runs upward. A
StackStore is optional and is an intermediate history builder, not a renamed
LayerStore cache.

## Durable versus ephemeral state

| State | Durable? | Owner |
|---|---:|---|
| canonical objects | yes | each physical Store's CAS table, at most once per ObjectId in that DB |
| LayerHistory/Layer | yes | LayerStore; copied Full records may exist in StackStore where explicitly pulled |
| StackHistory/Stack | yes | StackStore; verified copy may exist in LayerStore after Push |
| Branch/Commit | yes | BranchStore; accepted/received provenance may exist upstream |
| Workspace COW, spool, open handles | no | `layerfs-workspace` |
| container FUSE proxy | no | one thin projection per container Workspace |
| execution process and retained output | no/bounded runtime state | `layerfs-workspace`, never Store truth |
| operation receipt and CPU/memory/dedup snapshot | observation only | `layerfs-monitor`, never Store tables |

## Layered Workspace view

BranchStore does not clone every object inherited from its parent. The full
filesystem presented to a Workspace is resolved by identity across layers:

```text
direct view
    = Layer base + Branch Commit changes + Workspace delta

stacked view
    = Layer base + Stack state + Branch Commit changes + Workspace delta
```

Lookup order is:

```text
Workspace COW
    -> BranchStore
        -> optional StackStore
            -> LayerStore
```

The first owner of the requested canonical object answers. Uncommitted state
never leaks between Workspaces, even when several sessions pin the same Branch
head.

## Small verb grammar

| Verb | Boundary | Meaning |
|---|---|---|
| `commit_workspace_session` | Workspace -> BranchStore | Collapse the final delta into at most one Commit and exact-CAS the Branch |
| `merge` | Branch -> Branch | Three-way integrate Branch histories and exact-CAS the target Branch |
| `push_branch` | BranchStore -> configured parent | Transfer only missing immutable Branch/Commit/object data |
| `add_stack` | Branch Commit -> StackHistory | Conflict-check, create one Stack, exact-CAS StackHistory |
| `push_stack` | StackStore -> LayerStore | Transfer only missing Stack/provenance/object data |
| `add_layer` | Branch Commit or Stack -> LayerHistory | Conflict-check, create one Layer, exact-CAS LayerHistory |
| `pull_*` | authority -> work tier | Transfer selected immutable history toward work without hidden Merge |
| `end_workspace_session` | Workspace runtime | Clean or discard only; never Commit |

The shortest rule is:

```text
Commit = Workspace -> Branch result
Merge  = Branch -> Branch integration
Push   = transfer toward authority
Pull   = transfer toward work
Add    = accept a Stack or Layer
End    = cleanup only
```

## Workspace session lifecycle

```text
create
  pin Branch head
  allocate UUID
  create host COW/spool
  start one Workspace worker under the local LayerFS host
  expose host/container projection
        |
        v
active
  shell/exec/tool operations
        |
        +-- commit
        |     quiesce the final filesystem view
        |     collapse it to one final delta against the pinned base
        |     canonicalize changed final content
        |     create one Commit
        |     exact-head CAS
        |     retain read-only projection
        |           |
        |           v
        |        committed
        |           |
        |           `-- end -> unmount/cleanup
        |
        `-- end --discard -> explicit cleanup without Commit
```

Plain End never captures or commits. It rejects a dirty uncommitted Workspace.
A failed Commit preserves the Workspace and changes. One Workspace session
creates at most one Commit.

The Commit input is the final Workspace delta, not the sequence of shell,
tool, FUSE, or materialization operations that produced it. Equivalent final
filesystem states against the same pinned base produce the same canonical root
and reachable ObjectIds. Operation receipts and command output remain bounded
runtime observations outside Commit identity and every Store schema.

## Host versus Docker

All configured Store databases stay on their host or explicit Store service.
They are never copied or bind-mounted into Docker merely to run a Workspace.

```text
HOST
  Layer/Stack/Branch DBs
  high-level Workspace sessions
  Workspace overlay COW and spool
  Commit/CAS authority
  execution logs
  Monitor receipts and resource snapshots
        |
        | session-scoped filesystem connection
        v
DOCKER
  thin FUSE projection
  /workspaces/<uuid>
  tool/agent processes
  no SQLite or Store credentials
```

Multiple Workspace UUID mounts may coexist in one FUSE-ready trusted container.
Use one container per Workspace when agents require a security/resource
isolation boundary.

## Terminal products

```text
layerfs-cli
    complete standalone `layerfs` command application
    every db/layer/stack/branch/workspace/monitor operation
    human and structured output
    works without TUI

layerfs-tui
    optional Ratatui/Crossterm visual interface
    depends on and invokes layerfs-cli in-process
    never bypasses CLI to call SDK/Stores directly
```

Implementation is deliberately split:

```text
Phase One: Content + Storage + Stores + Workspace + FUSE/materialization
           + Monitor + SDK + standalone/reusable CLI + benchmarks

Phase Two: layerfs-tui only
```

Phase One must be a complete headless product. It freezes typed commands,
plans, completion, events, snapshots, output paging, and results so Phase Two
does not redesign a backend API while drawing the interface.

Strict dependency:

```text
layerfs-tui -> layerfs-cli -> layerfs-sdk
                                  |
                                  +-- Store crates
                                  +-- layerfs-workspace
                                  `-- layerfs-monitor

layerfs-workspace
    +-- layerfs-fuse
    `-- layerfs-materialization

layerfs-layer-store / layerfs-stack-store / layerfs-branch-store
    `-- layerfs-storage
            `-- layerfs-content
```

`layerfs-content` defines canonical filesystem content and pure transformations;
`layerfs-storage` persists, admits, queries and transfers that content plus
LayerFS history records. `layerfs-sdk` is a thin composition and semantic-access
layer. Workspace COW/session/placement/execution/output are implemented by one
cohesive `layerfs-workspace` crate, and receipts/timing/deduplication/resource
aggregation are implemented by `layerfs-monitor`. Neither subsystem is
implemented inside the SDK.

The TUI contains no permanent allowed-operation button grid. The bottom command
line is the universal action surface; the UI supplies selection, completion,
preview, progress, output, navigation, and monitoring.

## Design principles

1. **One authority per route.** A BranchStore binds to exactly one LayerStore
   directly or one StackStore that binds to that LayerStore. It is never
   silently reparented.
2. **Progressive connection.** Users land in/create a LayerStore, then attach
   zero or more StackStores and/or BranchStores. No user-defined Store names or
   `--parent` flags are required.
3. **No Project entity.** `LayerHistory` already represents accepted filesystem
   history. There is no Project table, class, package, or CLI noun.
4. **Content is meaning; Storage is mechanism.** `layerfs-content` owns the
   canonical filesystem model and pure transformations with no SQLite/Store/
   Workspace dependency. `layerfs-storage` owns typed history persistence,
   admission, CAS and transfer and depends one-way on Content.
5. **Derive redundant context.** The SDK derives history from immutable
   Layer/Stack records and exact expected heads from pinned operations.
6. **Explicit lifecycle.** Workspace Create, Commit, and End have different
   ownership. End cannot imply Commit, and crash cannot imply publication.
7. **Storage stays out of execution containers.** Containers receive only a
   session-scoped FUSE view and execution environment.
8. **One semantic command path.** Standalone CLI and TUI commands use the same
   parser, dispatcher, SDK requests, events, and results.
9. **Deduplication is the flagship result.** For an explicitly measured cohort,
   show saved bytes, byte-weighted reuse, collapse factor, and an intuitive
   statement such as `10 equivalent installs -> 1 canonical payload set per
   required DB` before secondary statistics.
10. **Metrics must be honest.** Report per-physical-DB uniqueness and required
   copies separately; never call two/three required Store placements a dedup
   failure and never mix active transient Workspace bytes into committed CAS
   reuse.
11. **Bounded observation.** Execution output belongs to the Workspace
    subsystem; operation receipts, monitoring, and timing belong to Monitor.
    Both stay outside Store schemas and cannot grow unboundedly.
12. **Cold replacement over compatibility.** Delete redundant modes, wrappers,
    parameters, packages, and duplicate implementations unless a current
    consumer proves a migration need.
13. **Final state over operation history.** Workspace writes, renames, tools,
    and executions are transient ways to reach a filesystem state. Only the
    collapsed final delta against the pinned base enters a Branch Commit.
14. **Two terminal phases.** Phase One contains everything except TUI and must
    pass independently. Phase Two consumes the frozen CLI library and contains
    presentation code only.

## Binding status

These documents are a proposed replacement for conflicting application-facing
sections in existing `docs/source-tree.md`, `docs/rule.md`,
`docs/implementation-plan.md`, and the current SDK/mount implementation. Before
production implementation, reconcile those earlier documents to this set or
explicitly supersede them; do not leave two normative architectures active.
