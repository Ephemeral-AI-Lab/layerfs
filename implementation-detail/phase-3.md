# Phase 3 — Copy-on-Write Trees and Authenticated Deltas

Status: retrospective implementation record. This document describes the
existing Phase 3 behavior introduced by commit
`cb80edb950ac538e122d6496e8ecedbff4d53a95` (`Phase 3: implement COW and
authenticated deltas`). It does not introduce a new format, semantic contract,
performance claim, or implementation milestone.

The original Phase 3 plan remains in the repository-level
[implementation plan](../IMPLEMENTATION_PLAN.md#phase-3--copy-on-write-trees-and-deltas).

## 1. Phase boundary

Phase 3 owns the backend-independent semantic layer:

```text
immutable tree nodes
  -> content-derived node and root identities
  -> copy-on-write mutation
  -> unchanged-subtree reuse
  -> exact parent/child delta
  -> authenticated delta application
```

Phase 3 does not own SQLite rows, durable transition framing, visible-head
publication, journal behavior, COMMIT, physical allocation, or ambiguous
outcome reconciliation. Those are Phase 4 responsibilities.

## 2. Implementation provenance

The original implementation commit added:

- [`crates/layerfs-core/src/cow/mod.rs`](../crates/layerfs-core/src/cow/mod.rs)
- [`crates/layerfs-core/src/cow/tree.rs`](../crates/layerfs-core/src/cow/tree.rs)
- [`crates/layerfs-core/src/cow/mutate.rs`](../crates/layerfs-core/src/cow/mutate.rs)
- [`crates/layerfs-core/src/delta/mod.rs`](../crates/layerfs-core/src/delta/mod.rs)

It also extended core errors and identity domains and added Phase 3 evaluation
coverage in [`tools/layerfs-eval/src/main.rs`](../tools/layerfs-eval/src/main.rs).

Later Phase 4 work added durable mapping support such as
[`cow/persistence.rs`](../crates/layerfs-core/src/cow/persistence.rs) and
[`delta/codec.rs`](../crates/layerfs-core/src/delta/codec.rs). Those files
consume Phase 3 semantics; they are not retroactively part of the original
Phase 3 implementation.

## 3. Core types

### Immutable tree and root

[`cow/tree.rs`](../crates/layerfs-core/src/cow/tree.rs) defines:

- `Metadata`;
- `NodeKind`;
- immutable `TreeNode` values;
- content-derived `NodeId`;
- `RootHandle`; and
- `RootId`, represented by the existing content-derived `ObjectId` domain.

Directory entries use canonical names and deterministic ordering. A root
contains an immutable directory tree; mutable workspace state is not part of
root identity.

### Copy-on-write mutation

[`cow/mutate.rs`](../crates/layerfs-core/src/cow/mutate.rs) defines:

- `Mutation`;
- `MutationResult`;
- add, remove, replace, rename, and metadata mutation operations; and
- authenticated application of individual delta entries.

A successful mutation returns a new root and its exact semantic delta. The
parent root remains usable and unchanged.

### Delta

[`delta/mod.rs`](../crates/layerfs-core/src/delta/mod.rs) defines:

- `DeltaEntry`;
- `Delta`;
- exact parent and child root binding;
- deterministic delta construction between roots; and
- authenticated delta application.

Delta entries cover additions, removals, replacements, and metadata changes.
A rename is represented by the exact ordered semantic operations required by
the existing contract.

## 4. Frozen semantic contracts

- A mutation creates a new immutable root.
- The parent root remains valid and unchanged.
- Unchanged files, directories, chunks, and subtrees retain their identities.
- Only changed nodes and affected ancestor spines are recreated.
- Root identity is content-derived and excludes workspace/backend state.
- Delta entries bind exact paths and before/after semantic values.
- A delta binds its expected parent and child roots.
- Applying a delta to another parent fails with `DeltaParentMismatch`.
- Producing a root other than the bound child fails with
  `DeltaChildMismatch`.
- Conflicting add/remove/replace/metadata operations fail with typed errors
  rather than silently retargeting another tree.
- Failed mutation or delta application does not mutate the parent root.
- Replaying a delta against its resulting child is rejected by the parent
  binding rather than corrupting the tree.
- Canonical path and directory ordering remain deterministic.

Later phases may persist or optimize these values but may not silently redefine
their identities, parent/child meaning, operation ordering, or replay behavior.

## 5. Algorithmic expectation

For a path-local mutation, Phase 3 intends:

```text
work
  = changed path/leaf
  + affected ancestor spine
  + exact delta entries

unchanged subtrees
  = identity reuse
  + no payload reconstruction
```

The phase fails its design goal if a small mutation rebuilds an entire
unchanged directory tree or materializes all unchanged file payloads.

This is a core semantic/algorithmic expectation, not a durable-latency claim.
Phase 4 later adds storage authentication, transaction authority, publication,
COMMIT, reconciliation, and physical I/O.

## 6. Load-bearing tests

The current core tests cover:

- mutation rebuilding only the changed ancestor spine;
- unchanged subtree identity and pointer reuse;
- add, remove, replace, rename, and metadata delta construction;
- applying a delta to its exact parent and obtaining the exact child;
- wrong-parent and wrong-child rejection;
- replay protection;
- conflict detection; and
- failed mutations leaving the parent unchanged.

The tests live with the implementation in
[`cow/mutate.rs`](../crates/layerfs-core/src/cow/mutate.rs),
[`cow/tree.rs`](../crates/layerfs-core/src/cow/tree.rs), and
[`delta/mod.rs`](../crates/layerfs-core/src/delta/mod.rs).

## 7. Phase 4 handoff

Phase 4 consumes the Phase 3 contracts through:

- [logical persistence mapping](phase-4/mapping/logical-persistence.md);
- [durable algorithm specification](phase-4/algorithm/spec.md);
- [SQLite visible-head publication](phase-4/storage/sqlite/visible-head.md);
  and
- [WP4-M optimization evidence](phase-4/wp4m/progress.md).

Phase 4 may add canonical durable codecs, mapping pages, receipts, generations,
and storage authority. Physical row IDs, offsets, filenames, SQLite details,
receipts, and publication generations must remain outside Phase 3 content
identity.
