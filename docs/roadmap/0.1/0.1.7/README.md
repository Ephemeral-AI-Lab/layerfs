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
- Projection design focuses on FUSE. APFS-specific projection and clonefile
  acceleration are out of scope for v0.1.7.
- A refactor that cannot fit this boundary moves to 0.2.0.

## Plan status

Design planning (owner direction, 2026-09-16). The
[component-decoupling discussion index](component-decoupling/README.md) organizes
the proposed clusters and their future design documents. The
[shared decoupling proposal](component-decoupling/proposal.md) sets the
repository-wide scope, boundary principles and independent-measurement
requirements for the full internal refactor. CAS/delta/CDC/COW/FUSE is one
worked example within that scope.
The design workstream is tracked in
[#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).
The [proposed repository layout](component-decoupling/repository-layout.md)
places the replacement product in core/ and future application adapters in
adapters/, retaining existing crates as a reference until replacement qualification.
Only layerfs-telemetry is agreed as a candidate crate so far; the remaining crate
inventory and boundaries will follow component-design decisions.

Time-only parent/child measurement is specified in
[layerfs-telemetry](component-decoupling/telemetry.md) and was implemented for
[#161](https://github.com/Ephemeral-AI-Lab/layerfs/issues/161) as the first
component of the replacement workspace:
[`core/crates/layerfs-telemetry/`](../../../../core/crates/layerfs-telemetry/README.md).
The standalone crate provides environment-independent trees, injected
parent/child scopes, attachment of independently completed reports and
caller-owned text/JSON output. Adapter integration, async behavior and overhead
qualification are still future work and are not claimed here.

The source-linked inventory, exact interfaces and architecture moves, ordered
implementation slices, proofs and measurement plan remain design deliverables.
They are recorded or linked here before implementation begins.

Tracking issue:
[#155](https://github.com/Ephemeral-AI-Lab/layerfs/issues/155).
