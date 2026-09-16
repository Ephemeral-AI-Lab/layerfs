# Cluster 2: physical object storage

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Read with the [joint co-design review](content-storage-co-design.md),
[logical content review](canonical-content.md) and [shared proposal](proposal.md).
Tracking issue: [#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).

## Purpose and review verdict

Implement the canonical-object contract: membership and exact reuse,
authentication, physical base selection, delta/compression, packs, locators,
SQLite persistence and authenticated reconstruction. Cluster 1 owns canonical
meaning and identity. History owns publication semantics; storage owns the
transaction mechanism required to enforce them.

Read-only review at `8357b1e336d1e16eae349a3313c5f3dbdc777b82` found no Cargo
dependency on FUSE, daemon, Docker or the Workspace service. Storage isolation
from their runtimes is feasible. The current package still contains history
and Workspace responsibilities, and prepare/persist stages are not independent
data artifacts. No extraction, build, test or measurement was executed.

## Existing foundations

- [Store dependencies](../../../../../crates/layerfs-layerstack-store/Cargo.toml)
  are content, BLAKE3, rusqlite and zstd-sys.
- [NativeEncoder::compress, pack.rs:456](../../../../../crates/layerfs-layerstack-store/src/objects/pack.rs#L456)
  takes raw bytes and an optional prefix without SQLite.
- [Delta reconstruction, delta.rs:109](../../../../../crates/layerfs-layerstack-store/src/objects/delta.rs#L109)
  takes a record and supplied base and reconstructs canonical content.
- [Native admission tests](../../../../../crates/layerfs-layerstack-store/src/objects/admission/native_tests.rs)
  exercise a real SQLite Store and canonical objects without a live Workspace.
- [Read authentication, read.rs:2040](../../../../../crates/layerfs-layerstack-store/src/objects/read.rs#L2040)
  reconstructs physical content and applies the content library's identity rules.

## Extraction obstacles

| Existing coupling | Source | Required separation |
| --- | --- | --- |
| Construction creates WorkspaceAdmission and streams directly into it | [objects.rs:4492](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L4492) | Supply the bounded output consumer explicitly; keep one construction algorithm |
| Store package exports history, snapshots, WorkspaceAdmission and construction alongside physical storage | [lib.rs:19](../../../../../crates/layerfs-layerstack-store/src/lib.rs#L19) | Separate responsibilities internally; retain only externally required compatibility facades |
| PreparedAdmission owns an admission session and validates Store identity/activity | [admission.rs:162](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L162) | Distinguish encoded data from effectful same-Store session ownership |
| Preparation performs predecessor/base/membership access under schema-selected policies | [admission.rs:415](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L415) | Explicit profile and lookup/base providers; count actual dependency reads |
| Publication assembles retained pack tails and insertion can merge into an open pack | [admission.rs:1268](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1268), [1474](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1474) | Attribute pack placement/assembly honestly or relocate computation without duplicate work |

Do not turn a same-Store admission handle into a generic serializable plan merely
to benchmark it. A portable pack artifact would need format, base closure,
provenance, logical locators and revalidation contracts beyond this requirement.

## Internal responsibility sequence

```text
canonical batches and optional hints
  -> membership, collision and dependency checks
  -> base selection and encoding
  -> bounded pack groups
  -> pack placement and SQLite mutation
  -> operation-specific acknowledgement/publication
```

These can remain ordinary functions/modules. Pure codecs can run without a DB;
storage preparation can use declared lookup/base providers; persistence uses
real SQLite. Keep finality, batches and base dependencies explicit. Delta and
compression may share a codec invocation and need not become artificial passes.

## Invariants across the boundary

- [Dependency closure, objects.rs:4071](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L4071)
  and [collision checks, objects.rs:4089](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L4089)
  remain required. Presence alone cannot replace an integrity check.
- [Publication, admission.rs:1295](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1295)
  rechecks relevant membership state; [base chronology, admission.rs:1447](../../../../../crates/layerfs-layerstack-store/src/objects/admission.rs#L1447)
  constrains physical references. A stale preparation must not bypass these rules.
- [AdmissionSession, objects.rs:2238](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2238)
  owns writer serialization and rollback scope; [resolve/rollback, objects.rs:2493](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L2493)
  can quarantine a Store after failed cleanup. Any simpler replacement must carry
  the same required safety obligations.
- [Direct candidate publication, workspace.rs:326](../../../../../crates/layerfs-layerstack-store/src/workspace.rs#L326)
  combines final admission and history update in a transaction callback.
  [Workspace publication, workspace.rs:450](../../../../../crates/layerfs-layerstack-store/src/workspace.rs#L450)
  finishes admission, stages/retains the root, then performs a separate
  conditional history transaction. Preserve each route's observable contract;
  a common internal implementation must account for their different ownership.

Private state machines and wrappers may be deleted or replaced. Review them by
the guarantees above, not by their existing names or source locations.

## Independent measurement and proof

- Codec: target/base bytes to actual encoded result; verify decode and identity
  separately. Include the declared codec initialization/reuse policy.
- Pack construction: encoded records/groups to framed packs and valid locations.
- Storage preparation: canonical input plus readers to bounded prepared output;
  include lookup, base reads, comparisons, encoding and packing.
- Object save: real writable Store plus canonical input through admission and
  final acknowledgement; separately reopen/read/authenticate and check reuse.
- Persistence phase: valid prepared/session state through actual remaining pack
  work, checks, SQL/index work and commit. Do not label it SQL-only prematurely.
- Read: IDs through lookup, base reconstruction, decompression and authentication
  under a declared cache state.

A one-object diagnostic must finish its real transaction/acknowledgement. Bulk
production retains shared packs and transactions. Dividing batch time by count
is an average, not observed latency for each individual object; do not force one
transaction per object to produce convenient trace spans.

Prove isolated/integrated canonical equivalence, full/delta/duplicate paths,
missing/corrupt bases, stale membership, wrong session/Store, final transaction
failure, abandonment and cleanup. Verify bounded scratch/batches and unchanged
worker policy. The joint review records remaining isolation and measurement gates.
