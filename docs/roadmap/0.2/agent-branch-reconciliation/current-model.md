# Current model and target definition

> **Status:** Source-backed 0.1 model plus required 0.2 product semantics.
>
> This document separates implemented behavior from the proposed mechanism in
> [proposal.md](proposal.md).

## Scope

This document answers two questions:

1. How do LayerFS Branches, Commits, Workspaces, reconciliation, and
   LayerStack Add work in the current source?
2. What do those concepts mean when one Branch is a fast collaboration pod for
   many agents and one Workspace is one agent tool call?

The first answer is descriptive. The second is the 0.2 requirement. Neither
section selects a Store schema or final API.

## Implemented 0.1 entity model

The current durable graph has one LayerStack head, immutable Layers, named
Branches, and immutable Commits. A Workspace is ephemeral and holds a stable
snapshot plus private copy-on-write state.

```text
                           LayerStack S
                                |
                                | head_layer_id
                                v
        L0 --------------------> L1
        ^                        ^
        |                        |
        | base_layer_id          | next published Layer
        |                        |
     Branch B                    |
        |                        |
        | head_commit_id         |
        v                        |
       C0 ----> C1 ----> C2 -----+
                         root reused by Add

     Workspace W
        |
        +-- branch_id = B
        +-- expected_head = C2
        +-- expected_base = L0 or L1
        +-- private COW tree and spool
```

A Branch record identifies a LayerStack, a base Layer, and an optional head
Commit. Its effective filesystem root is the head Commit root when present;
otherwise it is the base Layer root.

```text
Branch effective root

                     +-----------------------+
head Commit exists? -| yes -> Commit.root    |
                     | no  -> BaseLayer.root |
                     +-----------------------+
```

Layers and Commits are immutable after insertion. Visibility changes only when
the Branch or LayerStack head pointer advances.

## Implemented Workspace creation

Workspace creation reads the Branch, its base, its head, and its effective root
from one Store snapshot. It also acquires an in-process writable lease for that
Branch before exposing the requested projection.

```text
create_workspace(branch B)
          |
          v
  acquire Branch lease
          |
     +----+----+
     |         |
   failed    acquired
     |         |
     v         v
   Busy     pin snapshot
               |
               +-- expected_head
               +-- expected_base
               +-- base_root
               |
               v
        create COW Workspace
               |
               v
       attach projection
               |
               v
         return ready
```

The lease means that one `Workspaces` owner does not admit two writable
Workspaces for the same Branch at once:

```text
                 Branch B lease
                       |
             +---------+---------+
             |                   |
       Workspace A           Workspace B
          admitted              refused
```

The database compare-and-swap still protects publication against direct Store
users or other owners that do not share the in-memory lease set.

Source: [Workspace creation and lifecycle](../../../../crates/layerfs-workspace/src/lifecycle.rs).

## Implemented Workspace Commit

A normal Workspace Commit is capture plus optimistic publication, not a merge
with a newer Branch head.

```text
Workspace Commit
      |
      v
active execution? -- yes --> Busy
      |
      no
      v
pause projection
      |
wait for writers and quiesce
      |
capture final Workspace state
      |
      v
Branch head/base still equal expected values?
      |
  +---+---+
  |       |
 no      yes
  |       |
  v       v
HeadMoved mutation_generation == 0?
          |
      +---+---+
      |       |
     yes      no
      |       |
      v       v
  UpToDate  build candidate
                 |
                 v
          admit missing objects
                 |
                 v
          insert immutable Commit
                 |
                 v
       CAS Branch head and base
                 |
             +---+---+
             |       |
           loses    wins
             |       |
             v       v
         HeadMoved Created
                     |
                     v
           rebase active Workspace
                     |
                     v
                  resume
```

The public result is one of:

```text
Created   new immutable Commit and Branch head
UpToDate  no logical Workspace mutation
Busy      active execution or writer prevented capture
HeadMoved expected Branch position no longer current
```

Both the pre-build check and final SQL compare-and-swap require the expected
Branch head and base. The Branch pointer is the last visibility update. Earlier
object-admission batches can leave unreachable immutable objects after a lost
race, but cannot expose an incomplete Branch root.

Sources:

- [Workspace Commit orchestration](../../../../crates/layerfs-workspace/src/lifecycle.rs)
- [Store Commit publication](../../../../crates/layerfs-layerstack-store/src/workspace.rs)
- [Workspace Commit outcomes](../../../../crates/layerfs-workspace/src/session.rs)

## Implemented stale-Workspace behavior

If an external writer advances the same Branch while a Workspace is running,
the Workspace keeps its private state but Commit does not reconcile it:

```text
time ------------------------------------------------------------>

Branch:       C0 ---------------- C1 ---------------- C2
               \                                      ^
                \                                     |
Workspace W:     +-------- private changes -------- commit
                                                       |
                                                       v
                                                   HeadMoved

No C3 is created.
No conflict list is created.
No automatic rebase occurs.
```

Repeating Commit without changing the Workspace's expected Branch position
returns the same stale outcome. The caller needs a separate reconciliation
lifecycle.

## Implemented LayerStack Add

`layerstack add` publishes a Branch head Commit as the next immutable Layer. It
does not merge the Branch with a newer LayerStack head.

```text
Branch B
  base = L1
  head = C4
             \
              +-- C4.root
                     |
                     v
               candidate Layer L2
                     |
                     v
LayerStack head still L1?
        |
    +---+---+
    |       |
   no      yes
    |       |
    v       v
HeadMoved  CAS head L1 -> L2
                     |
                     v
                   Added
```

The current outcomes are:

```text
Added      Branch Commit became a new Layer
UpToDate   this Branch/Commit source was already added
NoChanges  Commit root equals its base Layer root
HeadMoved  Branch base is not the current LayerStack head, or CAS lost
```

Add copies no canonical filesystem objects and never silently pulls, rebases,
or merges.

Source: [LayerStack Add](../../../../crates/layerfs-layerstack-store/src/layerstack.rs).

## Implemented reconciliation

The existing three-root filesystem engine can combine a Branch Commit with a
newer Layer of the same LayerStack:

```text
                  old Branch base Layer
                           |
                           v
                         Base
                        /    \
                       /      \
                      v        v
            Branch Commit    Current Layer
                      \        /
                       \      /
                        v    v
                     reconcile
                        |
               +--------+--------+
               |                 |
         combined root      typed conflicts
```

It automatically accepts unchanged sides, identical results, and structurally
compatible changes. Remaining conflicts are classified as:

```text
Content    incompatible file state
Type       incompatible inode kinds
Directory  incompatible namespace/subtree state
HardLink   incompatible linked inode topology
```

The current Workspace-facing choices are:

```text
Branch       exact affected paths from the Branch snapshot
Layer        exact affected paths from the Layer snapshot
WorkingTree  preserve the resolution Workspace version
```

Important current behavior:

```text
resolve --layer
      |
      v
record choice and fingerprint
      |
      v
mounted view may still show Branch content
      |
      v
Workspace Commit applies Layer choice
      |
      v
projection refresh shows final content
```

An unresolved conflict blocks Commit. A later Workspace mutation intersecting
an affected path, ancestor, or descendant invalidates its recorded choice.
Conflict state is held by an active resolution Workspace; no conflict row or
marker is written to durable Store history.

The lower-level `Workspaces` API can create a reconciliation Workspace, but the
current public SDK `Client` and CLI expose only list and resolve operations for
an already-existing one. `workspace commit` and `layerstack add` do not create
one automatically.

Sources:

- [Filesystem three-root reconciliation](../../../../crates/layerfs-content/src/filesystem/reconcile.rs)
- [Workspace reconciliation state](../../../../crates/layerfs-workspace/src/reconcile.rs)
- [Reconciliation choice application](../../../../crates/layerfs-layerstack-store/src/objects.rs)
- [Public SDK Client](../../../../crates/layerfs-sdk/src/client.rs)
- [CLI operations](../../../../crates/layerfs-cli/src/lib.rs)

## 0.1 properties to preserve

The 0.2 design may change public and storage contracts deliberately, but it
must preserve these correctness properties unless a separately reviewed
replacement is stronger:

- immutable Layer and Commit records;
- authenticated content-addressed objects;
- stable canonical filesystem identity;
- bounded candidate construction and object admission;
- visibility-last Branch and LayerStack compare-and-swap;
- no visible head pointing to incomplete closure;
- no silent overwrite after a lost race;
- exact historical reads and fresh reconnect;
- explicit Workspace Commit and End;
- explicit Branch-to-LayerStack publication.

## Required 0.2 collaboration model

For 0.2, the product definition is:

```text
                         LayerStack
                      globally integrated main
                              |
             +----------------+----------------+
             |                                 |
             v                                 v
        Branch / pod A                    Branch / pod B
       fast shared state                 fast shared state
        /      |      \                    /          \
       /       |       \                  /            \
      v        v        v                v              v
 Agent A1   Agent A2   Agent A3       Agent B1       Agent B2
 Workspace  Workspace  Workspace      Workspace      Workspace
 tool call  tool call  tool call      tool call      tool call
```

### LayerStack

The LayerStack is main: a slower, globally integrated sequence of immutable
pod/task checkpoints. It is not updated after every agent tool call.

### Branch

A Branch is one collaboration node or pod. Multiple agents share it and must
observe a fast linear sequence of accepted results. A Branch is not owned by
one agent and is not a Git pull-request branch.

### Workspace

A Workspace is isolated execution state for one agent tool call. It starts
from a stable Branch Commit, may run concurrently with other Workspaces, and
ends by producing a candidate, being discarded, or failing explicitly.

### Commit

A Branch Commit is an accepted result in the pod's shared linear history. A
private Workspace result is not accepted merely because candidate construction
succeeded.

### Proposal

`Proposal` is the current working name for the immutable handoff between a
finished tool-call Workspace and Branch acceptance. The 0.2 design may choose a
different public name, but it needs this state boundary if Workspaces are truly
disposable per call.

```text
Workspace result               Branch history
     private                        shared
        |                              ^
        v                              |
    Proposal -- reconcile/validate ----+
```

## Required multi-agent behavior

Assume three Workspaces start at `C2`:

```text
                            +---- Workspace A ---- result A
                            |
C0 -------- C1 -------- C2 -+---- Workspace B ---- result B
                            |
                            +---- Workspace C ---- result C
```

All three may execute concurrently. Acceptance remains linear:

```text
C0 -> C1 -> C2 -> C3(A) -> C4(A+B) -> C5(A+B+C)
```

For each older result, reconciliation uses cumulative roots:

```text
Base      = Commit from which that result was derived
Incoming  = tool-call result or already-reconciled candidate
Current   = latest accepted Branch head
```

Clean stale results integrate automatically. Genuine incompatible changes
become structured work. Changed inputs on which a tool depended require
revalidation even when written paths are disjoint.

## Gap summary

| Concern | Implemented 0.1 | Required 0.2 |
| --- | --- | --- |
| Workspaces per Branch | one writable lease per owner | many concurrent tool-call Workspaces |
| Stale Workspace Commit | typed `HeadMoved` | automatic cumulative reconcile or actionable outcome |
| Accepted history | linear when writes do not race | deterministic linear acceptance under routine concurrency |
| Conflict context | Branch versus Layer | Workspace versus latest Branch and Branch versus LayerStack |
| Conflict lifetime | active Workspace memory | resumable beyond originating tool call |
| Resolver | records a later Commit choice | visible exact candidate before validation |
| Dependency safety | filesystem result comparison | changed-read revalidation plus write reconciliation |
| Public initiation | incomplete SDK/CLI path | complete typed lifecycle |
| Progress | conflict lifecycle is caller-managed | conflicted result does not block unrelated acceptance |
| Global publication | explicit Add | remains an explicit pod-to-main checkpoint |

## Definition boundary

LayerFS owns filesystem state, immutable candidate evidence, reconciliation,
conflict state, validation identity, Branch acceptance, and LayerStack
publication. An agent orchestrator owns agent selection, task intent, semantic
review, and deciding which authorized agent resolves a ticket.

The target is not Git with faster commands. It is a filesystem-aware optimistic
transaction system for agent tool calls:

```text
stable snapshot
  + observed mutations and dependencies
  + cumulative current state
  + deterministic acceptance
  = next shared Branch Commit or structured work item
```
