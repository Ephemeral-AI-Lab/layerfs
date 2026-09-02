# Proposal: agent Branch reconciliation and conflict policy

> **Status:** Proposed 0.2 design; not an accepted public or storage contract.
>
> Read [Current model and target definition](current-model.md) first. Public
> names, Store schema, wire frames, and implementation details remain
> unfrozen.

## Problem statement

A Branch is not a Git-style developer fork. It is a rapidly advancing node or
pod shared by cooperating agents. Each tool call receives a private Workspace,
but many Workspaces may start from different Commits of the same Branch and run
concurrently.

The 0.1 behavior admits one writable Workspace lease per Branch and returns
`HeadMoved` when a Workspace Commit observes a newer Branch position. Its
three-way reconciliation path addresses Branch-versus-Layer divergence, not
routine Workspace-versus-latest-Branch integration. Conflict state is tied to
an active Workspace, and the public Client and CLI cannot initiate the complete
reconciliation lifecycle.

Those constraints turn normal agent concurrency into manual retry work. LayerFS
0.2 needs a reconciliation mechanism designed for high-frequency agent
transactions rather than a faster imitation of Git.

## Fixed model

```text
LayerStack = main, the globally integrated checkpoint history
Branch     = a multi-agent node or pod with fast linear accepted history
Workspace  = one isolated tool-call attempt from a stable Branch snapshot
Proposal   = the immutable candidate produced by that attempt
```

`Proposal` is a working term until the public contract is frozen.

Agents execute concurrently. Only the short acceptance path into one Branch is
ordered:

```text
agent A Workspace -> Proposal A --+
agent B Workspace -> Proposal B --+-> Branch integration -> C1 -> C2 -> C3
agent C Workspace -> Proposal C --+
```

The complete proposed ownership path is:

```text
                      AGENT ORCHESTRATOR
        chooses tool call, resolver, and validation policy
                              |
       +----------------------+----------------------+
       |                      |                      |
       v                      v                      v
  Workspace A            Workspace B            Workspace C
 stable snapshot         stable snapshot         stable snapshot
 private writes          private writes          private writes
       |                      |                      |
       v                      v                      v
  Proposal A             Proposal B             Proposal C
       |                      |                      |
       +----------------------+----------------------+
                              |
                              v
                    BRANCH INTEGRATOR
             changed-frontier and dependency checks
              three-root reconciliation when needed
                   /          |           \
                  /           |            \
                 v            v             v
             accepted     revalidate     conflict ticket
                 |            |             |
                 v            +-------------+
        linear Branch Commit history
                 |
                 | explicit pod checkpoint
                 v
         LAYERSTACK / MAIN INTEGRATION
```

LayerFS owns the state and exact publication rules inside the lower half. The
orchestrator owns which agent acts and what semantic validation is appropriate.

## Required history rule

If a Workspace starts at `C2` while the Branch advances to `C5`, its result is
reconciled once from cumulative snapshots:

```text
Base      = C2 root
Incoming  = Workspace Proposal root
Current   = C5 root
```

The implementation must not replay `C3`, `C4`, and `C5` one by one. A clean
result becomes `C6` with parent `C5`, preserving one linear accepted history:

```text
C1 -> C2 -> C3 -> C4 -> C5 -> C6
      \                         ^
       Workspace Proposal -----+
```

After a Proposal is reconciled onto `C5`, `C5` becomes its next operational
merge base. If the Branch advances to `C7` before publication, reconcile:

```text
Base      = C5
Incoming  = candidate already reconciled onto C5
Current   = C7
```

The original checkout Commit remains provenance; it is not reused as the
operational merge base after later state has been incorporated.

### Multiple candidates from one old head

Three agents may all start from `C2` and finish in a different order:

```text
time -------------------------------------------------------------------->

Agent A:       [ work from C2 -------------------- ] Proposal A
Agent B:       [ work from C2 -------- ] Proposal B
Agent C:       [ work from C2 --------------------------- ] Proposal C

Completion:                              B       A              C
Integration:                             |       |              |
                                         v       v              v
Branch:       C0 -> C1 -> C2 ----------> C3(B) -> C4(B+A) ----> C5(B+A+C)
```

Completion or queue order determines parent order, but not whether concurrent
work is silently discarded. Each accepted candidate is reconciled against the
latest accepted root before it receives a Commit.

## Proposal evidence

The integration path needs enough factual evidence to outperform whole-file
Git reconciliation. A Proposal should retain bounded forms of:

- Base Commit and root;
- candidate root;
- changed paths and inode identities;
- create, remove, rename, type, metadata, and link operations;
- dirty file ranges where capture already knows them;
- object-version preconditions;
- read dependencies that affected the tool result;
- validation identity and outcome.

Do not freeze a large new record before measuring what the existing Workspace
mutation and capture state can supply. The required semantic distinctions are:

```text
no dependency overlap -> clean transplant
write/write overlap    -> reconcile or conflict
read/write overlap     -> revalidate or regenerate
```

The minimum logical Proposal shape is:

```text
Proposal
+-- identity
+-- Branch identity
+-- original checkout Commit/root          provenance
+-- operational merge Base Commit/root     advances after reconcile
+-- candidate root
+-- changed frontier
|   +-- paths and inode identities
|   +-- namespace/type/link operations
|   `-- dirty file ranges when known
+-- dependencies
|   +-- object/path versions read
|   `-- object/path versions expected before write
+-- validation receipt or required validation class
`-- state: pending | revalidate | conflict | accepted | rejected
```

This is a semantic inventory, not a requirement to create one unbounded row or
serialize every syscall. The implementation should reuse bounded capture state
and content-addressed identities before adding another operation log.

### Dependency decision matrix

```text
Changes since Base
        |
        v
+----------------------+----------------------+----------------------+
| no Proposal          | intersects Proposal  | intersects Proposal  |
| dependency overlap   | reads only           | writes               |
+----------------------+----------------------+----------------------+
| transplant/merge     | NeedsRevalidation    | structural or full   |
| onto Current         | or regenerate        | reconciliation       |
+----------------------+----------------------+----------------------+
                                                  |
                                          +-------+-------+
                                          |               |
                                       clean           conflict
                                          |               |
                                       accept      resolution ticket
```

Example of a false clean merge avoided by read tracking:

```text
Proposal reads:   schema.json @ object S1
Proposal writes:  generated/client.rs

Current changes:  schema.json S1 -> S2

write sets are disjoint
read dependency is stale
result = NeedsRevalidation, not automatic acceptance
```

## Reconciliation ladder

Stop at the first correct result:

1. If Incoming already equals Current, return accepted/up-to-date.
2. If changed dependencies do not intersect changes since Base, apply the
   Incoming frontier to Current.
3. If operations are identical, independent, or structurally commuting, merge
   them deterministically.
4. If known dirty ranges of one file are disjoint and the file invariants hold,
   merge the ranges.
5. Run complete three-root filesystem reconciliation.
6. Emit structured conflicts only for the remaining incompatible units.

Range-aware merging is not required before the path/inode fast paths are
correct and measured. A format-aware text or manifest merge belongs above the
storage core and must submit its result as `WorkingTree` content.

The ladder as a decision path:

```text
Incoming candidate
        |
        v
Incoming == Current? -- yes --> UpToDate/AlreadyAccepted
        |
        no
        v
dependency frontier disjoint? -- yes --> transplant onto Current
        |
        no
        v
operations commute or are identical? -- yes --> structural merge
        |
        no
        v
same-file known ranges disjoint and valid? -- yes --> range merge
        |
        no
        v
complete three-root filesystem reconcile
        |
    +---+---+
    |       |
  clean   conflict
    |       |
    v       v
validate  durable resolution ticket
```

## Branch integration sequencer

Each Branch has one logical integration stream. It serializes candidate
acceptance, not agent execution or Workspace mutation.

For each Proposal:

1. Read the current Branch head.
2. Compare changes and dependencies since the Proposal Base.
3. Produce a clean reconciled candidate, `NeedsRevalidation`, or a structured
   resolution ticket.
4. Present the exact reconciled candidate for validation.
5. Publish an immutable Commit whose parent is the current Branch head.
6. Advance the Branch head with compare-and-swap as the final visibility write.
7. Retry internally if the head races and the new state still reconciles.

Routine stale-head movement is not a user-visible conflict. Public outcomes
must distinguish:

```text
Accepted
AutomaticallyReconciled
NeedsRevalidation
NeedsResolution
Busy or RetryLater
Failed
```

The Branch remains available while a conflicted Proposal is resolved. Move the
Proposal out of the active queue and let unrelated candidates continue.

### Sequencer state machine

```text
                     +---------+
        submit ----> | Pending |
                     +----+----+
                          |
                          v
                    +-----------+
                    | Comparing |
                    +-----+-----+
                          |
          +---------------+----------------+
          |               |                |
          v               v                v
   +------------+   +------------+   +------------+
   | Reconciling|   | Revalidate |   | Conflicted |
   +------+-----+   +------+-----+   +------+-----+
          |                |                |
          | clean          | passed         | resolved
          +----------------+----------------+
                           |
                           v
                     +-----------+
                     | Ready     |
                     +-----+-----+
                           |
                           v
                     +-----------+
                     | Publishing|
                     +-----+-----+
                           |
                    +------+------+
                    |             |
                  CAS win       CAS loss
                    |             |
                    v             +----> Comparing against newer head
               +----------+
               | Accepted |
               +----------+

Any state may enter Rejected or Cancelled before publication.
Only Accepted changes the visible Branch head.
```

A `Conflicted` Proposal is removed from the active acceptance slot. The next
pending Proposal may proceed. When resolved, the earlier Proposal re-enters at
`Comparing` against the then-current head.

### Public outcome meanings

| Outcome | Meaning | Caller action |
| --- | --- | --- |
| `Accepted` | Candidate was current and published | continue |
| `AutomaticallyReconciled` | Stale candidate merged and published | continue with merged Commit |
| `NeedsRevalidation` | A read or semantic dependency changed | rerun the declared check or regenerate |
| `NeedsResolution` | Incompatible filesystem changes remain | open or assign the ticket |
| `Busy` / `RetryLater` | Bounded service or resource admission stopped progress | retry with backoff |
| `Rejected` | Validation, cancellation, or policy rejected the candidate | inspect reason; Branch is unchanged |

`HeadMoved` remains an internal compare-and-swap observation during Branch
integration. It stays public for the distinct Branch-to-LayerStack checkpoint
until that higher-level lifecycle has an equally explicit integration result.

## Structured conflict policy

A conflict is bounded evidence, not text inserted into user files. Each entry
must identify:

- conflict kind;
- affected paths and inode identities;
- Base, Incoming, and Current versions;
- relevant namespace operations or dirty ranges;
- the Branch head against which it was prepared;
- any prior resolution and the dependency fingerprint that keeps it valid.

Intra-Branch choices are:

```text
Incoming     keep the tool-call Proposal version
Current      keep the latest accepted Branch version
WorkingTree  keep an agent-created combined result
```

Branch-to-LayerStack presentation may label the same roles as `Branch`,
`Layer`, and `WorkingTree`. The storage core should not hard-code those
context-specific names into a generic three-root algorithm.

Selecting a snapshot must update the visible resolution candidate before
validation. No choice may be substituted after the validated root is computed.

Conflict anatomy:

```text
Conflict
+-- stable ticket-local conflict ID
+-- kind: Content | Type | Directory | HardLink | Dependency
+-- affected paths and inode identities
+-- Base
|   `-- version/object/namespace facts at operational merge Base
+-- Incoming
|   `-- Proposal version and relevant operations
+-- Current
|   `-- latest accepted Branch version and relevant operations
+-- WorkingTree
|   `-- resolver-editable combined candidate
+-- resolution choice and dependency fingerprint
`-- status: unresolved | resolved | invalidated
```

The `Dependency` label is provisional. A stale read may be a ticket-level
`NeedsRevalidation` reason rather than a filesystem conflict kind; Phase A must
choose one public representation without conflating it with write conflict.

## Resolution tickets

One Workspace per tool call means resolution cannot depend on the original
process or mount surviving. A conflicted Proposal needs a resumable ticket or
an equally explicit durable boundary containing:

```text
ticket identity
Branch and source Proposal identity
operational merge Base and target head
incoming and partially reconciled roots
conflicts and affected dependencies
recorded choices and fingerprints
```

An authorized agent may open a new resolution Workspace, inspect the three
variants, edit a combined result, validate it, and submit a resolved Proposal.
The agent orchestrator chooses the resolver; LayerFS owns the exact state and
publication rules.

If the Branch advances during resolution:

- preserve choices whose affected dependencies did not change;
- invalidate only choices intersected by new path, inode, range, or dependency
  changes;
- rebase the resolved candidate from its latest incorporated head onto the new
  current head;
- emit a smaller updated ticket when conflicts remain.

### Ticket lifecycle

```text
Proposal P based on C2
        |
        | reconcile against C5
        v
Ticket T5
+-- operational Base = C2
+-- target = C5
+-- partial merged root
`-- unresolved conflicts
        |
        | agent opens a new resolution Workspace
        v
WorkingTree R5
        |
        | resolve + validate
        v
Resolved Proposal P5, operational Base = C5
        |
        | Branch advanced meanwhile
        v
reconcile P5 against C7 using Base C5
        |
    +---+--------------------------+
    |                              |
new changes disjoint        affected conflict changed
    |                              |
preserve decisions          invalidate affected decisions only
    |                              |
    v                              v
publish child of C7          smaller Ticket T7
```

The original checkout `C2` remains provenance throughout. It is not the next
operational Base after `C5` has been incorporated.

## Validation rule

Filesystem-clean does not mean semantically safe. A Proposal may write one file
from data it read in another. If another Commit changes that read dependency,
the result needs revalidation or regeneration even when write paths are
disjoint.

The candidate validated by the tool or configured check must be the exact root
published as the next Branch Commit. If another Commit changes that root before
publication, reconcile and validate again, or hold the Branch integration slot
through the bounded validation step.

Heavy pod-level validation may remain at the explicit LayerStack checkpoint,
but Branch acceptance must never claim that a tree different from the tested
tree passed.

### Validation and publication race

```text
merge candidate against C5
        |
        v
install exact candidate in validation Workspace
        |
        v
run declared validation -> receipt(root = R5)
        |
        v
CAS Branch head C5 -> new Commit(root = R5)
        |
    +---+---+
    |       |
  wins    loses to C6
    |       |
    v       v
Accepted  reconcile R5 from Base C5 onto Current C6
            |
            v
          candidate root changed?
            |
        +---+---+
        |       |
       no      yes
        |       |
 reuse exact  revalidate exact new root
 receipt if
 policy allows
```

The simplest first implementation may hold the per-Branch integration slot
through one bounded validation command. If measurements show that blocks the
queue materially, introduce optimistic revalidation without weakening the
root-to-receipt binding.

## LayerStack checkpoint boundary

Fast Branch Commits do not imply one Layer per tool call:

```text
many Workspace Proposals
  -> many accepted Branch Commits
  -> one meaningful pod checkpoint
  -> reconcile Branch head with current LayerStack head
  -> Add one Layer
```

The LayerStack remains main. Branch-to-LayerStack Add stays atomic and refuses
silent overwrite. Its higher-level integration path may use the same generic
three-root reconciliation engine, but it is a distinct, less frequent lifecycle.

```text
FAST POD LOOP                                  MAIN CHECKPOINT LOOP

Workspace -> Proposal -> Branch Commit --+
Workspace -> Proposal -> Branch Commit --+--> selected Branch head
Workspace -> Proposal -> Branch Commit --+            |
                                                       v
                                             reconcile with LayerStack head
                                                       |
                                                       v
                                                validate checkpoint
                                                       |
                                                       v
                                                  Add one Layer
```

## Provisional public lifecycle

The public surface needs operations equivalent to these semantics; the names
are deliberately not frozen:

```text
create_workspace(branch, expected_head?) -> Workspace
finish_workspace(workspace)               -> Proposal | UpToDate
submit_proposal(proposal)                  -> IntegrationHandle
integration_status(handle)                 -> IntegrationOutcome
open_resolution(ticket)                    -> Workspace
conflicts(ticket, cursor)                  -> bounded ConflictPage
resolve(workspace, conflict, choice)        -> remaining count
finish_resolution(workspace)               -> resolved Proposal
cancel_proposal(proposal_or_ticket)         -> Cancelled
checkpoint_branch(branch)                   -> Layer integration outcome
```

The ordinary SDK path must not require callers to construct internal readers,
call private Store reconciliation, parse debug strings, or retain the original
tool process after candidate capture.

## Failure and recovery contract

Visibility remains last:

```text
capture candidate
      |
admit immutable objects and Proposal facts
      |
reconcile and validate
      |
BEGIN final publication transaction
      |
insert immutable accepted Commit
      |
CAS Branch head from expected Current to new Commit
      |
COMMIT
```

Required crash outcomes:

| Failure point | Visible Branch result | Recovery requirement |
| --- | --- | --- |
| Before Proposal admission | unchanged | Workspace may retry or discard |
| After object admission, before Proposal fact | unchanged | unreachable objects may deduplicate later |
| After Proposal admission, before integration | unchanged | Proposal is resumable or cancellable |
| During reconciliation | unchanged | deterministic retry from immutable inputs |
| While ticket is unresolved | unchanged by that Proposal | ticket and choices reconnect exactly |
| During final transaction before Commit | unchanged | transaction rollback |
| CAS loss | winner remains visible | re-enter comparison against winner |
| After successful CAS/commit | new Commit visible and complete | reconnect returns exact accepted state |

No recovery path may guess whether publication happened; immutable IDs and the
Branch head must make the answer observable.

## Implementation slices

### Slice 1: freeze semantics and failing checks

- Specify Proposal lifetime, operational Base advancement, queue ordering,
  cancellation, and typed outcomes.
- Add deterministic failing cases for two concurrent Workspaces on one Branch,
  clean stale reconciliation, and exact linear parents.

### Slice 2: concurrent Workspace admission and clean integration

- Remove the long-lived one-Workspace-per-Branch exclusion.
- Reuse existing candidate building and three-root reconciliation.
- Add one per-Branch logical integrator and internal CAS retry.
- Support independent and identical changes before durable tickets.

### Slice 3: dependency revalidation

- Reuse existing read and mutation observations where they identify exact
  object or path dependencies.
- Add the minimum bounded dependency representation proved necessary by the
  matrix.
- Bind validation receipts to exact candidate roots.

### Slice 4: resumable structured resolution

- Persist or otherwise prove the Proposal/ticket boundary across Workspace and
  process teardown.
- Expose bounded Base, Incoming, Current, and WorkingTree access.
- Apply choices visibly, preserve unaffected choices, and invalidate only
  intersecting ones.

### Slice 5: pod-to-main integration and release closure

- Reuse the generic reconcile roles for Branch versus LayerStack without
  conflating the two lifecycles.
- Complete typed CLI/SDK coverage, crash recovery, resource evidence, and the
  accumulated regression campaign.

Do not begin with range merging, format adapters, generalized CRDT machinery,
or a distributed queue. Add them only after the smaller local Branch
integrator is correct and a retained case proves the need.

## Current source areas

- [Workspace lease and lifecycle](../../../../crates/layerfs-workspace/src/lifecycle.rs)
- [Workspace reconciliation](../../../../crates/layerfs-workspace/src/reconcile.rs)
- [Workspace candidate construction](../../../../crates/layerfs-workspace/src/changes.rs)
- [Store Commit publication](../../../../crates/layerfs-layerstack-store/src/workspace.rs)
- [LayerStack Add](../../../../crates/layerfs-layerstack-store/src/layerstack.rs)
- [Filesystem reconciliation](../../../../crates/layerfs-content/src/filesystem/reconcile.rs)
- [Public SDK Client](../../../../crates/layerfs-sdk/src/client.rs)
- [CLI operations](../../../../crates/layerfs-cli/src/lib.rs)

## Deterministic correctness matrix

The first matrix should prove behavior, not compete with Git performance:

| Case | Required result |
| --- | --- |
| Two Workspaces from one head, disjoint files | both accepted automatically in linear history |
| Two Workspaces from one head, identical result | one accepted and one accepted/up-to-date without conflict |
| Older Workspace, several intervening Commits | one cumulative reconciliation onto the latest head |
| Same file, incompatible content | bounded `NeedsResolution` ticket |
| Create/delete and file/directory collision | typed namespace or type conflict |
| Hard-link topology conflict | complete affected identity set and exact selected topology |
| Incoming write depends on changed read | `NeedsRevalidation`, not false clean merge |
| Branch advances during resolution, unrelated paths | prior choice preserved and new changes incorporated |
| Branch advances during resolution, affected path | only intersecting choice invalidated |
| One conflicted Proposal plus later clean Proposal | clean Proposal advances the Branch |
| Concurrent clean submissions | deterministic linear parents and exact final root |
| Crash after candidate admission but before publication | no incomplete visible head and resumable or safely repeatable work |
| Fresh reconnect with pending resolution | exact ticket, roots, choices, and continuation behavior |

Each case verifies final bytes, canonical root, Commit parents, Branch head,
historical immutability, conflict evidence, Store integrity, reconnect, and
Workspace cleanup.

## Performance and resource evidence

Measure separately:

- Workspace execution and candidate construction;
- Branch queue wait and service time;
- changed-frontier comparison;
- cumulative three-root reconciliation;
- validation;
- object admission and publication;
- retries, preserved and invalidated choices, and conflict count;
- CPU, peak RSS, spill, Store growth, and object reuse.

Start with bounded concurrency and history profiles such as 1, 2, 4, and 8
simultaneous Workspaces. Increase only when a smaller profile identifies a
specific scaling question. Do not add a public Git comparison until the
correctness and acknowledgement boundaries are equivalent and useful.

## Non-goals

- Git compatibility or replacement.
- General-purpose source-code semantic merging in the LayerFS core.
- A conflict marker protocol.
- A global CRDT filesystem.
- An unbounded operation log per Workspace.
- Agent scheduling or selection.
- Making a Branch conflict freeze every other agent.
- Publishing every tool-call Commit as a Layer.

## Acceptance criteria

- [ ] Multiple Workspaces can run concurrently from one Branch without a
  long-lived exclusive writable lease.
- [ ] Clean stale Proposals reconcile automatically against the latest Branch
  head and publish as its direct child.
- [ ] The operational merge Base advances after each incorporated head.
- [ ] Intervening Commits are reconciled cumulatively rather than replayed one
  at a time.
- [ ] Independent and identical changes avoid user-visible conflict.
- [ ] Changed read dependencies produce explicit revalidation.
- [ ] Genuine conflicts expose exact bounded Base, Incoming, Current, and
  WorkingTree evidence.
- [ ] Resolution is resumable across tool-call and Workspace teardown.
- [ ] Another authorized agent can complete a resolution.
- [ ] Resolution choices affect the visible candidate before validation.
- [ ] Unaffected choices survive later Branch movement and intersecting choices
  are invalidated precisely.
- [ ] A conflicted Proposal does not block unrelated Branch acceptance.
- [ ] The exact validated root becomes the accepted Commit root.
- [ ] Accepted Branch history is deterministic, immutable, and linear.
- [ ] Compare-and-swap remains the final visibility write and never exposes
  incomplete state.
- [ ] LayerStack checkpoint reconciliation remains explicit and separate.
- [ ] Focused failure, reconnect, resource, public SDK, CLI, and integrity
  checks pass without private shortcuts.
