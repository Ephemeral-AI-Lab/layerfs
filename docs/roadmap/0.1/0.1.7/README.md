# LayerFS v0.1.7: Architecture Refactor Migration

> **Status:** Current planning checklist; no release candidate exists.

## Goal

v0.1.7 is the **architecture refactor migration** release: move the internal
architecture onto the shape the [v0.2.0 model](../../0.2/README.md) assumes, so
that 0.2.0 adds new semantics on top of prepared structure instead of replacing
it, and set the tone for that release.

## Direction from 0.2.0

The [0.2 roadmap](../../0.2/README.md) defines the target collaboration model:

```text
LayerStack = main, the globally integrated checkpoint history
Branch     = a rapidly iterating node or pod shared by cooperating agents
Workspace  = one isolated tool-call attempt
```

The [agent Branch reconciliation task](../../0.2/agent-branch-reconciliation/README.md)
records the exact differences between the implemented 0.1 model and that target.
Those differences are 0.2.0's work, not this release's: they change what a
Branch, a Workspace, and an accepted Commit mean. v0.1.7 prepares the
architecture that will carry them.

## Boundary

- v0.1.7 is a 0.1.x release, so the
  [release-policy](../../../general/release-policy.md) promise for the patch line
  applies: public SDK and CLI behavior, daemon protocol, canonical bytes and
  identities, and the Store format are preserved while the internal architecture
  changes. Any exception needs an explicit owner decision recorded here first.
- No 0.2 public semantics, and no 0.2 mechanism, are pre-approved by this
  checklist.
- A refactor that cannot fit this boundary moves to 0.2.0.

## Plan status

Initialization only (owner direction, 2026-09-16). This checklist states the goal
and the direction; it does not yet specify the architecture moves, their order,
their proofs, or their measurement plan. Those are written here before
implementation begins.

Tracking issue:
[#155](https://github.com/Ephemeral-AI-Lab/layerfs/issues/155).
