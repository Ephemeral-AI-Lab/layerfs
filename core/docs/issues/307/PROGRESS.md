# Cluster-two implementation progress

> **Status:** Current planning checklist; no release candidate exists.

Owner: [tracker #307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307).
Design baseline: `c9861bc878583822a468e78dbc0f3740eacbecbe` on local main.
The [implementation plan](../303/07-implementation-validation.md) owns milestone
dependencies and exits. All S0–S13 items remain unchecked until their complete
implementation and required evidence exist.

## Current checkpoint

Initial overlay/prerequisite checkpoint `f2a381119` is committed locally, with
all required host checks passing. S3 now has initial public content-built root
binding/read APIs and immutable caching; its dormant predecessor is temporarily
relocated intact. See [base architecture](../../architecture/20-workspace-base.md).
This remains partial implementation, with all milestone exits outstanding.

S0 in progress. Public-API proof in
[interleaved_saves.rs](../../../crates/layerfs-persistence/tests/interleaved_saves.rs)
demonstrates an initialized provider with independently initialized Storage
handles, a stack-owned registry of borrowed Saves, same-Save pending reads,
interleaved finish and demand read, abort, slot reuse and exact CAS reuse.
No lifetime extension, self-referential owner or whole-Commit provider lock is
needed. This establishes a usable lifetime topology, not an authenticated runtime.

The [Linux platform proof](../../../benchmark/cluster2-platform/README.md)
compiled content, bundled rusqlite and fuser 0.18.0 with Rust 1.85.1 on ARM64.
Actual native FUSE mount, file read, detach and worker join passed with `/dev/fuse`
and `CAP_SYS_ADMIN`. This is prerequisite evidence, not product FUSE qualification.

## Concrete next work

The initial active overlay now provides typed namespace/metadata/cell/scratch/
ownership primitives and publication/reply-attempt/capture routing. Paired EXPLAIN
and actual statement profiles caught and corrected a residual-generation scan.
See [implemented architecture](../../architecture/19-daemon-overlay.md).
The remaining list below includes initial interfaces already established; complete
S0/S1/S2 acceptance requires the unimplemented service/lifetime/resource work.

1. Implement S1 as an active `layerfs-overlay` crate: one owner connection,
   startup/schema/readback, typed namespaced metadata, binary cell payload,
   operation scratch and ownership; observe real VM/sort/full-scan/reprepare
   counters and retain paired EXPLAIN. Preserve existing dependency pins.
2. Establish minimal S2 frontier interfaces: checked incarnations, publication
   tickets and reply-send-attempt custody, generation-selective keyset pages and
   fixed EOF. Defer no long computation/IO under the owner transaction.
3. Establish S3 against genuine content-built roots and current public APIs.
   No mount-wide materialization or special empty-root substitute.
4. Close S0 algorithm gates for truncate/regrow, repeated failed capture and
   orphan composition, physical pressure and fair service; specify all R1–R8
   adversarial bounds and P1–P14 implementation ownership. These remain open,
   so prerequisite smoke proofs do not complete S0.
5. Progress the embedded runtime and canonical backed-construction/import
   prerequisites as their public boundaries become ready.

## Preserved state and publication

Untracked `core/docs/issues/301/`, the existing extent-normalization research and
`output/` are unrelated and preserved. Root `crates/` remains the reference until
S12 qualification permits S13. Checkpoint hashes are local-only; no remote push,
release or deployment is authorized. Append-only tracker texts are retained in
[checkpoints](checkpoints/initial.md). Rust checks/LOC/evidence are recorded in
each checkpoint commit and its tracker comment; no completed milestone is claimed.
