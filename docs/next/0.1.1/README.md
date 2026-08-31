# LayerFS 0.1.1 proposed work

> **Status:** Proposal
>
> Target: LayerFS 0.1.1
>
> This directory is not part of the LayerFS 0.1.0 contract.

This directory contains evidence-backed candidates for the first patch release.
Inclusion here is not a commitment that a candidate will ship.

LayerFS 0.1.1 is a compatibility-preserving patch line. A candidate belongs
here only if it preserves all of the following 0.1.0 contracts:

- the five-table Store schema;
- canonical object bytes and identity domains;
- LayerStack, Layer, Branch, Commit, Workspace, Execution, and Object IDs;
- the CDC profile;
- existing Store visibility and transaction semantics;
- documented CLI grammar and exit behavior;
- documented public Rust SDK behavior;
- compatibility with the released container-daemon protocol;
- existing Workspace correctness, memory, and resource bounds.

A proposal that cannot meet those constraints must target LayerFS 0.2.0.

## Candidate areas

- [Large and mixed-edit capture resilience](capture-large-mixed-edit-resilience.md)
- [Extent-aware `copy_file_range` and prepend](copy-file-range-prepend.md)

## Admission requirements

Before a candidate becomes release work, it needs:

1. a measured defect in a public 0.1.0 operation;
2. an explicit compatibility analysis;
3. a bounded CPU and memory model;
4. focused correctness tests that fail before the change;
5. public SDK and real-FUSE proof when relevant;
6. a current-source benchmark showing no regression elsewhere;
7. a decision to accept, defer, or retarget it.

The release notes for 0.1.1 will be created only after an actual release
candidate exists. Until then, this directory remains non-binding design work.
