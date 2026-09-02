# Agent Branch Workspace reconciliation and conflict policy

> **Status:** Release-defining main-task folder for LayerFS 0.2.
>
> The documents describe a target contract. They do not change the released
> 0.1 behavior.

## Fixed premise

```text
LayerStack
  role: main
  rate: deliberate pod/task checkpoints
  history: globally integrated immutable Layers

Branch
  role: one agent collaboration node or pod
  rate: fast, many accepted tool-call results
  history: linear immutable Commits

Workspace
  role: isolated execution state for exactly one agent tool call
  rate: highly concurrent and disposable
  result: Commit candidate, discard, or explicit failure
```

The Branch is not a Git-style personal fork. Multiple agents collaborate on
one Branch and may run Workspaces concurrently from different Branch Commits.
The reconciliation mechanism must be cheaper, more structured, and more
automatable than a manual Git pull/rebase/conflict-marker workflow.

## Documents

1. [Current model and target definition](current-model.md)

   Records the implemented 0.1 Branch, Commit, Workspace, Add, and
   reconciliation behavior; then defines the 0.2 multi-agent collaboration
   model and the exact gaps between them.

2. [Proposal](proposal.md)

   Proposes Workspace result capture, Branch integration sequencing,
   cumulative reconciliation, dependency-aware revalidation, durable conflict
   tickets, visible resolution, validation, failure handling, public outcomes,
   implementation slices, and proof gates.

## Reading order

```text
current-model.md
      |
      | establishes facts and required semantics
      v
proposal.md
      |
      | proposes one design that must satisfy them
      v
implementation issue/spec
      |
      | freezes names, schema, migration, tests, and measurements
      v
code
```

Do not treat a proposed type name or transaction layout as accepted merely
because it appears in `proposal.md`. Freeze those choices only after the
current-model gaps, correctness matrix, and compatibility consequences are
reviewed.

## Main-task completion

- [ ] The implemented current model and its limitations remain source-linked.
- [ ] The multi-agent Branch/pod definition is accepted.
- [ ] The Proposal lifecycle and ownership boundary are accepted or replaced.
- [ ] Public outcomes, persistence, recovery, and migration are specified.
- [ ] Deterministic correctness cases fail before implementation and pass
  afterward.
- [ ] Multiple agents can use one Branch without long-lived Workspace
  exclusion, lost updates, Git-style manual rebases, or unvalidated publication.
- [ ] Branch-to-LayerStack publication remains a separate explicit checkpoint.
