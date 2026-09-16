# Cluster 1: logical content and CAS contract

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Read with the [joint co-design review](content-storage-co-design.md),
[physical storage review](object-storage.md) and [shared proposal](proposal.md).
Tracking issue: [#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).

## Purpose and review verdict

Own canonical bytes and identity, logical file/namespace representation, CDC,
persistent COW, and pure Diff/reconciliation algorithms. Own the semantic CAS
object-access contract; cluster 2 supplies physical storage implementations.
Live Workspace ownership, runtime transport, persistence and history publication
are external responsibilities.

Read-only review at `8357b1e336d1e16eae349a3313c5f3dbdc777b82` found a strong
existing independent library and a full candidate builder that still crosses
the proposed boundaries. Extraction is feasible; complete isolation is not yet
implemented or proven. No builds, benchmarks or implementation changes were made.

## Existing foundations

- [Content dependencies](../../../../../crates/layerfs-content/Cargo.toml)
  contain only BLAKE3; no Workspace, FUSE, daemon, Docker or SQLite dependency.
- [Object access contracts](../../../../../crates/layerfs-content/src/object/access.rs)
  already support generic canonical reads/writes and authenticated batches.
- [Canonical encoding and identity](../../../../../crates/layerfs-content/src/object/codec.rs)
  provide `encode_bytes_object` and `identify_canonical`.
- [File construction](../../../../../crates/layerfs-content/src/file/content.rs)
  and [rope construction](../../../../../crates/layerfs-content/src/file/rope/build.rs)
  use those object interfaces.
- [Compact namespace tests](../../../../../crates/layerfs-content/tests/compact_namespace.rs)
  provide a no-SQL memory implementation with explicit representation and
  inode-allocation behavior. This is useful proof material, not a production
  unbounded-memory design.

## Couplings to remove or make explicit

1. **Concrete candidate context.**
   [CandidateInputs, changes.rs:594](../../../../../crates/layerfs-workspace/src/changes.rs#L594)
   carries frozen Workspace types, Store, SnapshotReader, Workspace identity and
   a spool path. Adapt these into neutral semantic changes and explicit content
   readers. Moving `FrozenWorkspaceChanges` into the content crate unchanged
   would invert its existing dependency on content and retain Workspace coupling.
2. **Hidden identity writes.**
   [prepare_inode_serials, changes.rs:606](../../../../../crates/layerfs-workspace/src/changes.rs#L606)
   runs before both Commit and Preview; it reserves through the Store when new
   nodes need a range. [Reservation, schema.rs:198](../../../../../crates/layerfs-layerstack-store/src/schema.rs#L198)
   commits before IDs escape and deliberately burns ranges on failed attempts.
   Supply authorized identity context explicitly. Production operations still
   pay reservation cost; construction diagnostics declare supplied context.
3. **Broad object interface.**
   [ObjectStore, access.rs:64](../../../../../crates/layerfs-content/src/object/access.rs#L64)
   mixes byte access with format flags, allocation, physical hints and accounting.
   Simplify internal capabilities around actual consumers. No-op defaults can
   change canonical representation and are not an equivalent diagnostic provider.
4. **Read-your-writes and scratch.**
   [ObjectBuffer, objects.rs:3297](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L3297)
   reads owned candidate objects before its immutable source. Its
   [scratch index, spill.rs:687](../../../../../crates/layerfs-layerstack-store/src/objects/spill.rs#L687)
   can use private SQLite. A canonical-Store-write-free mode does not establish
   an entirely SQLite-free mode. Preserve bounded scratch and lifetime ownership.
5. **Physical work inside the producer.**
   [put_file_payload, objects.rs:880](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L880)
   can precompute a signature for physical delta selection. Preserve useful
   single-pass cooperation through an explicit optional preparation hook/data
   path or attribute the fused work honestly. Do not duplicate a scan to force
   the code into a diagram. Physical predecessor handling also currently depends
   on a concrete SnapshotReader at objects.rs:3311.
6. **Store-owned observation.**
   [note_commit_phase, changes.rs:3284](../../../../../crates/layerfs-workspace/src/changes.rs#L3284)
   routes through Store telemetry. Return or update plain component-local
   counters owned by the operation; the outer workflow translates these into
   product receipts without a new shared telemetry runtime.

## Proposed contract

```text
stable semantic changes
  + explicit canonical format and reserved identity context
  + immutable authenticated object reader
  + bounded read-your-writes scratch
  + bounded canonical output destination
    -> candidate root, logical counters and completion correlations
```

The builder has no mount, process, daemon connection, live Workspace, publication
handle or writable canonical Store. A supplied reader may perform I/O, which is
part of the declared measurement scope. Checkpoint installation and history
acknowledgement stay with the owning workflow.

Public API/format compatibility remains a constraint, but private types, wrappers
and module layouts are disposable. Prefer removing broad context and duplicate
paths over wrapping them in additional interfaces. Cluster membership does not
prescribe a new crate.

## Independent proofs and measurements

1. **Object construction:** run production encoding and identity functions from
   raw input in a content-only executable; consume output; separately verify
   canonical fixture bytes/ID, round-trip decoding and corrupt-ID rejection.
2. **File/tree construction:** fixed base objects, changes and identity/profile
   context with bounded providers; check bytes/ranges, root, object closure and
   the frozen CDC profile. Reuse the
   [canonical fixture oracle](../../../../../crates/layerfs-content/tests/canonical_v2_fixture_oracle.rs)
   and [shifted-stream oracle](../../../../../crates/layerfs-content/tests/fastcdc_shifted_stream.rs).
3. **Full candidate extraction:** the same producer algorithm and neutral input
   run with isolated and production destinations. Compare canonical output under
   identical namespace identities, metadata, profiles and ordering; prove no
   canonical Store writes in the isolated construction scope.
4. **Stronger no-SQL mode:** separately qualify input and scratch providers,
   including declared capacity and resource limits. A finite memory fixture does
   not prove a bounded large-candidate implementation.

Include intrinsic hashing, reads, copies, scratch and required output handling
inside their declared scopes. Producer elapsed time includes backpressure unless
the waiting interval is independently identified; it is not construction CPU.
The joint review defines measurement ownership and performance acceptance.
