# Canonical objects: identity, encoding and access

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Component 1 within [Cluster 1: canonical content](canonical-content.md).
Tracking: [#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).
Read with the [component map](cluster-1-2-components.md),
[finalized-object handoff](finalized-object-handoff.md), [content I/O contract](content-io.md) and
[ordered co-design decisions](content-storage-co-design.md#remaining-co-design-decisions).

## 1. Purpose and decision

Define the exact canonical bytes, object identity and common access contract used
by file content, filesystem trees and physical storage. Reuse the existing format
and hashing primitives; simplify the interfaces and repeated work around them.

Canonical means that identical fields, ordering and canonical profile produce
identical bytes and identity. It does not mean that every representation of the
same file contents has the same root. Physical compression, delta selection and
pack placement must reconstruct the same canonical object and preserve its ID.

Keep structural checks inside checked decoding and construction. There is no
separate role-validation component, registry, service or mandatory validation pass.
This document defines responsibility and removal targets, not new public APIs,
a new crate, a changed on-disk format or a measured optimization.

## 2. Responsibility boundary

```text
File content                         Filesystem tree and metadata
  chunks / extents / file roots        directories / inodes / metadata
  checked role codecs                 checked role codecs
                 \                    /
                  +------------------+
                           |
                  Canonical object primitives
                  +--------------------------+
                  | common framing           |
                  | ObjectId and hashing     |
                  | framing/identity checks  |
                  | shared reference types   |
                  | read/output contract     |
                  +------------+-------------+
                               |
                  Physical object store / admission
                  membership -> encoding -> packs -> DB
```

| Responsibility | Owner |
| --- | --- |
| Common envelope, object identity and framing checks | Canonical objects |
| Chunk, file-state and extent structure | File content |
| Directory, inode and metadata structure | Filesystem tree and metadata |
| Meaning of child references | The corresponding file/tree format; use shared ID/reference types |
| Membership, exact reuse, collision/dependency checks and persistence | Object store/admission |
| Physical delta bases, compression and pack locations | C2 encoding/packing |

The existing outer Bytes kind can contain several structured roles. An outer-kind
check does not prove the inner structure is valid. Logical child references also
differ from physical delta dependencies; the latter do not redefine the canonical
object graph. Keep each role's codec with its owning structure. A shared reference
entry point may dispatch to those codecs without duplicating their parsers or
collecting the entire graph.

## 3. Checked construction and decoding

### Construct

```text
role fields / payload
        |
check required invariants while building/encoding
        |
write final canonical allocation -> compute ObjectId
        |
move immutable output + needed role and direct references to bounded consumer
```

Do not encode and immediately decode an object merely to rediscover its known
length, role or references. Preserve required encoder checks, particularly after
mutating decoded nodes. A trusted constructor must actually validate the fields
it passes; caller-asserted metadata is not proof.

### Read

```text
requested ObjectId
        |
C2: bounded acquisition / reconstruction
        |
authenticate identity and common framing
        |
file/tree decoder: parse and check the required structure
        |
use borrowed payload or decoded value within its valid lifetime
```

| Check | Where it belongs |
| --- | --- |
| Expected identity and complete outer framing | The authenticated acquisition boundary |
| Tags, supported lengths, counts, ordering and field arithmetic | The corresponding checked decoder/constructor |
| Relationships, such as an extent fitting its referenced chunk | The algorithm that resolves those relationships, with required dependencies acquired in bounded batches |

The [chunk decoder](../../../../../crates/layerfs-content/src/file/extent_codec.rs#L48)
already checks its marker and length as it decodes. The
[inode-table decoder](../../../../../crates/layerfs-content/src/tree/inode/codec.rs#L65)
similarly checks fields and ordering. Reuse these patterns. Authentication alone
does not prove structural or relationship validity.

Validation can travel with unchanged privately owned bytes; a wrapper or ownership
move need not trigger another hash or decode. Reacquisition, mutation, a different
expected role, or different root/non-root context may require checks again. Do not
add a global validated-object cache. Full graph traversal is not required merely
to decode one node; retain any closure checks required by the actual operation.

## 4. Existing code to reuse

| Existing primitive | Reuse decision |
| --- | --- |
| [ObjectId](../../../../../crates/layerfs-content/src/object/id.rs#L9) | Keep the fixed identity representation and conversions |
| [Object hashing](../../../../../crates/layerfs-content/src/object/digest.rs#L6) | Keep BLAKE3, frozen domain separation and streaming support; do not replace identity with a raw-content digest |
| [Canonical codec](../../../../../crates/layerfs-content/src/object/codec.rs#L22) | Preserve exact encoding, overflow/length/trailing-byte checks and supported format semantics |
| [Writer encoding](../../../../../crates/layerfs-content/src/object/codec.rs#L30) | Reuse existing writer-based primitives when they avoid temporary materialization |
| [Borrowed payload decoding](../../../../../crates/layerfs-content/src/object/codec.rs#L64) | Retain views into existing bytes; decoding need not allocate another payload |
| [Authentication](../../../../../crates/layerfs-content/src/object/codec.rs#L163) | Keep identity/framing checks, with role checks owned by the relevant decoder |
| [Reference extraction](../../../../../crates/layerfs-content/src/object/references.rs#L10) | Preserve logical reference semantics and supported formats; reuse role codecs and references already established by construction |
| [Canonical fixture oracle](../../../../../crates/layerfs-content/tests/canonical_v2_fixture_oracle.rs) | Reuse exact-byte/identity evidence; replacement tests remain outside product source |

## 5. Simplifications to make

| Current coupling or repeated work | Target |
| --- | --- |
| [ObjectStore](../../../../../crates/layerfs-content/src/object/access.rs#L64) mixes reads/writes, format booleans, identity allocation, physical hints and accounting | Explicit construction profile/authorized identities, one semantic read boundary and a bounded output consumer; no trait per field |
| [ObjectRead](../../../../../crates/layerfs-content/src/object/access.rs#L3) and [ObjectSource/CoreReader](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L923) overlap | Consolidate contracts while retaining real grouped batch reads, cardinality, demand order and error semantics |
| Batch defaults loop over point operations | Production storage provides actual bounded batching; a point read can be a one-ID batch |
| [File length access](../../../../../crates/layerfs-content/src/file/content.rs#L60) rebuilds an envelope to decode a known payload | Decode that payload directly or return the already-established length |
| [Small-file encoding](../../../../../crates/layerfs-content/src/file/content.rs#L69) builds an inner allocation then copies into the outer one | Write identical bytes into the final canonical allocation |
| [Mixed reads](../../../../../crates/layerfs-layerstack-store/src/objects.rs#L3692) copy authenticated bytes and authenticate them again | Move authenticated ownership into bounded demand-order slots; retain checks when ownership does not establish validity |
| Separate structural validation and parsing repeat the same traversal | Perform required field checks during decoding; reuse the decoded result and its fields |

Some cuts occur at file/storage call sites. Do not pull their algorithms into this
component simply to make the change local. Reuse inexpensive checks when clearer;
prioritize whole-payload copies, repeat hashing/decoding and real I/O amplification.
Removal of generic candidate scratch is coordinated with the file/tree algorithms
and admission, under the [finalized-output design](content-io.md#finalized-output-replaces-candidate-staging).

## 6. Minimum boundary and memory ownership

The [handoff contract](finalized-object-handoff.md#2-object-fields-and-optional-hints)
owns the detailed output shape: immutable owned bytes, ObjectId, established role
and bounded direct references where needed; predecessor hints remain separate.
Derive canonical length from bytes.len(). Keep file/tree summaries in C1's result
or frontier. Exact Rust layout and reference ownership remain open; no duplicate
serialized descriptor or separately allocated wrapper is required per handoff.

Read/output batches need count and byte-capacity limits, demand-order semantics,
missing-object behavior and backpressure. Identity proof covers the actual bytes
for its lifetime; it does not excuse collision or external-input checks. Repeated
IDs are normal and do not imply unique construction output.

C1 establishes finality and returns the root and known length/count summaries. C2 owns bounded acceptance
and storage completion. Their [different guarantees](finalized-object-handoff.md#6-completion-and-failure)
do not require another validation layer or extra transaction. Allocation release,
backpressure and failure ownership are specified once in that shared contract.

There is no per-file, per-workspace or whole-graph cache in these primitives.
Construct one bounded object or batch, move/borrow it, then release it according
to consumer lifetime. Configured whole-file size affects required capacity; the
[policy design](content-storage-policy-and-tables.md) owns supported overrides.
The [I/O contract](content-io.md#whole-operation-bounds-including-workspace-scale-workloads)
owns operation-wide limits, many-file batching and the unresolved ordering proofs.

No Workspace, mount, daemon, container, history entity, SQL handle or transport
type is required here. The same canonical bytes and checks apply in every placement.

## 7. Independent evidence and timing

Use the same product functions to construct, identify, authenticate and decode
objects with explicit inputs and a bounded consuming destination. Saving to SQLite
is a separate measurable storage operation. A complete-operation comparison still
includes all required acquisition, construction and persistence work.

The existing [timer](../../../../../core/crates/layerfs-telemetry/README.md) can wrap
coarse calls or batches, with labels such as canonical.construct,
canonical.authenticate and canonical.decode. These are illustrative labels, not
new APIs. Do not record a trace node per object across an unbounded workload or
change scheduling to fit the recorder. Preserve disabled behavior and reporting
limits; actual nesting determines whether durations overlap.

Qualification must preserve exact bytes/IDs under matching profiles, corrupt-input
rejection, supported formats, reference semantics and ownership bounds. Measure
the targeted copying/hash/decode reductions and source-matched performance under
the existing [I/O qualification contract](content-io.md#7-measurement-and-completion).
No build, implementation or benchmark result is supplied by this document.

## 8. Remaining decisions

1. Exact Rust representation of the agreed output fields and bounded reference
   ownership, preserving constructor proofs without copying bytes or retaining
   every object's references for the whole operation.
2. Minimal authenticated batch-read and output signatures, including errors,
   demand order and byte-capacity/backpressure rules.
3. Placement of reference dispatch and checked payload helpers, keeping role
   parsers with file/tree components and preserving supported-format behavior.
4. The first extraction's equivalence oracle and legitimate reference access for
   measurement. Package/module layout follows those boundaries; no crate is fixed.

Start with one complete-file finalized-output route through real storage and
authenticated readback. The broader [co-design checklist](content-storage-co-design.md#remaining-co-design-decisions)
owns multi-edit finality, filesystem reference ordering, packing and remote SQL.
