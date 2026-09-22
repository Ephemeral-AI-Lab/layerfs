# Pair 2 — commit and history

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
>
> **Implementation status (2026-09-21):** C5 history is implemented through the
> service, including the reviewed remediation and bridge consolidation at
> `72d95323205e1352f1310a81a9e287414c7c9957`. The
> [remediation specification](commit-history/remediation-20260921.md) and
> [repaired boundary](commit-history/remediation-contract-20260921.md) record
> the corrections to the original specification. [Paper 16](../16-history.md)
> describes the implementation. H04/H06/H08/H14 qualification gaps remain open;
> see the [verification handoff](../../../../docs/roadmap/0.1/0.1.7/evidence/issue210-bridge-consolidation-20260921/validation.md).

Design parent: [#180](https://github.com/Ephemeral-AI-Lab/layerfs/issues/180).
The [implementation specification](commit-history/implementation.md) is the current
pair-2 handoff. It replaces the earlier initialized questions and supersedes
conflicting assumptions in the [older operational sketch](02-init-commit-and-concurrency.md).
Source basis: `a02168adbb1b02571941654919cefca12dbc1f42`, including the landed
schema-7/two-writer C2 implementation. History is not implemented by this document.

Implement a service-side C5 capability independently of Workspace/FUSE. The
existing pair-3 service composes public C1/C2 operations with a separate history
catalog; later pair 1 consumes that service contract. No C1/C2 private SQL, packs,
save IDs or algorithm details enter history.

```text
Test client now / Workspace later
                 |
        existing service + bridge
            /             \
           v               v
       C1 + C2         C5 history
       schema 7        catalog schema 1
       six tables      seven tables
```

The specification contains:

- [Entity relationships and ASCII diagrams](commit-history/implementation.md#3-history-entities-and-relationships).
- [Operations](commit-history/implementation.md#4-supported-operations): init,
  fork, staged/convenience Commit, add-layer, exact discard, allocation and queries.
- [Tables and constraints](commit-history/implementation.md#5-catalog-schema-1).
- [Multi-writer integration](commit-history/implementation.md#6-multi-writer-visibility-and-persistence).
- [Allocation and reopen support](commit-history/implementation.md#7-allocation-continuity-and-reopen-support).
- [Service/bridge and pair-1 boundary](commit-history/implementation.md#8-servicebridge-contract-and-later-pair-1).
- [File layout and estimated LOC](commit-history/implementation.md#9-files-folders-and-production-loc-plan).
- [Milestones and acceptance](commit-history/implementation.md#11-milestones-and-acceptance).

Retain the reference's LayerStack/Branch/Commit/Layer model, zero-copy forks and
separate Branch/stack publication. Enrich stages with frozen expected context and
exact tokens. A saved root is not a Commit; a Commit is not a published Layer.
Both disjoint and overlapping changes can be saved by C2, but one state-changing
history transition wins against a fixed expected head. No automatic rebase/retry,
GC, conflict resolution or stronger durability is introduced.

Native writable authority initially lasts for one continuing service authority;
read-only catalog reopen is supported, and arbitrary writable recovery is refused.
This explicit limitation and the initial bounded logical-manifest bootstrap are
part of the implementation scope, not hidden work deferred to a FUSE consumer.

The earlier expanded local investigation is retained verbatim in the
[81ace2778 checkpoint](https://github.com/Ephemeral-AI-Lab/layerfs/blob/81ace2778201036e9b1ca3040c63949e595f5971/core/docs/architecture/proposal/03-history.md).
It records its older inspection and design decisions; this overview and the
linked implementation/remediation contracts supersede its implementation status.
