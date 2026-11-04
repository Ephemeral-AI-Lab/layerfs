# Objects

[Architecture](../../README.md) › [Core](../README.md) › Objects

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Canonical immutable objects and their IDs: the units every other component names, stores and compares.

| | |
| --- | --- |
| Layer | [Core](../README.md) |
| Crates | `layerfs-content` |
| Source | [`core/crates/layerfs-content/src/object`](../../../../core/crates/layerfs-content/src/object), [`core/crates/layerfs-content/src/contract`](../../../../core/crates/layerfs-content/src/contract) |
| Uses | — |
| Used by | [Files](../files/README.md), [Filesystem trees](../filesystem-trees/README.md), [Physical storage](../physical-storage/README.md), [History model](../history-model/README.md) |
| Flows | — |

## Topics

| Topic | What it covers |
| --- | --- |
| [Identity](identity.md) | How an object's ID is derived, and why equal content gives an equal ID. |
| [Kinds](kinds.md) | The object kinds, inode leaves, admission and the content policy. |
| [Access](access.md) | Reading an object by ID: access context and output. |

## The picture

```text
            Files, Filesystem trees, Compare and merge (constructors)
                 |                                  ^
  value bytes +  |                                  | decoded values
  ObjectRole     v                                  |
 +----------------------------------------------------------------------+
 | Objects (layerfs-content::object, ::contract)                        |
 |                                                                      |
 |  Kinds                      Identity                Access           |
 |  13 ObjectRoles,     -->    LFSO envelope    -->    FinalizedObject  |
 |  inode leaf grammar,        BLAKE3 ID over          (id, role, bytes,|
 |  ConstructionPolicy         domain + bytes,         refs, preds)     |
 |  admission                  predecessors            provider port    |
 +----------------------------------------------------------------------+
                 |                                  ^
  FinalizedConsumer::accept                         | AuthenticatedObjects::
                 v                                  |   read_canonical_batch
 +----------------------------------------------------------------------+
 | Physical storage (layerfs-storage) and Engine Store                  |
 +----------------------------------------------------------------------+
```

## What it does

Everything LayerFS stores as history content is a canonical object: immutable
bytes in one envelope, named by a 32-byte ID. Content builds them; Physical
storage stores them; History model points at roots of them.

| Kind group | `ObjectRole`s | Names |
| --- | --- | --- |
| File content | `WholeFile`, `Chunk`, `ExtentLeaf`, `ExtentBranch`, `FileState` | The bytes of one regular file ([Files](../files/README.md)) |
| Namespace | `FilesystemRoot`, `InodeLeaf`, `InodeBranch`, `DirectoryLeaf`, `DirectoryBranch` | The whole tree: inodes and directory bindings ([Filesystem trees](../filesystem-trees/README.md)) |
| Metadata and links | `AttributeLeaf`, `AttributeBranch`, `Symlink` | Kept attributes and symlink targets |

An ID is `BLAKE3("layerfs/object/v2\0" || canonical bytes)`. The canonical bytes
are fully determined by the value, so equal canonical bytes give an equal ID,
and Physical storage can reuse an object by ID alone. See
[Identity](identity.md) for when equal *logical* content gives equal canonical
bytes.

| Term | Meaning here |
| --- | --- |
| Canonical bytes | The one accepted byte form of an object: envelope plus role value |
| Role | What an object means (`ObjectRole`), not how it is stored |
| Direct references | Child IDs an object names; derived from its bytes |
| Advisory predecessor | A hint for a delta base; never a dependency |

Other terms are in the [glossary](../../glossary.md).

## Invariants

| Rule | Where it is enforced |
| --- | --- |
| Objects never change after they are named: `FinalizedObject` has no setter for bytes or ID, and the ID is computed once from the owned bytes. | [`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs) |
| The canonical form, not the physical one, decides the ID: compression, delta and pack layout are outside the hashed bytes. | [`id.rs`](../../../../core/crates/layerfs-content/src/object/id.rs), [Physical storage](../physical-storage/README.md) |
| IDs are domain-separated, never a raw digest of the payload. | [`id.rs`](../../../../core/crates/layerfs-content/src/object/id.rs) |
| Each value has one canonical encoding: decoders reject trailing bytes, padding and non-canonical partitions instead of repairing them. | [`codec.rs`](../../../../core/crates/layerfs-content/src/object/codec.rs), [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| References are derived from the bytes; a supplied list or predecessor is never trusted on admission. | [`admission.rs`](../../../../core/crates/layerfs-content/src/object/admission.rs) |
| Predecessors are hints: at most four, outside the ID, chosen or ignored by Physical storage. | [`predecessor.rs`](../../../../core/crates/layerfs-content/src/object/predecessor.rs) |
| Content opens no database, pack or file; it reads through `AuthenticatedObjects` and writes through `FinalizedConsumer`. | [`lib.rs`](../../../../core/crates/layerfs-content/src/lib.rs), [`access.rs`](../../../../core/crates/layerfs-content/src/object/access.rs) |
| A provider absence (`MissingObject`) and a provider failure (`ProviderFailure`) stay distinct. | [`contract/error.rs`](../../../../core/crates/layerfs-content/src/contract/error.rs) |
| Content forbids unsafe code. | [`lib.rs`](../../../../core/crates/layerfs-content/src/lib.rs) (`#![forbid(unsafe_code)]`) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Canonical object, any role | `MAX_CANONICAL_OBJECT_BYTES` = 16 MiB | [`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |
| One value field | `MAX_OBJECT_FIELD_BYTES` = 8 MiB | [`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |
| Tree page (extent, inode, directory, attribute) | 8,192 canonical bytes | [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs), [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Inode leaf rows | 1 to 100 | [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| Advisory predecessors | 4 per object | [`predecessor.rs`](../../../../core/crates/layerfs-content/src/object/predecessor.rs) |
| Construction cutoff | Power of two, 128 KiB to 1 MiB; else `UnsupportedPolicy` | [`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |
| Delta depth | 0 to 50 per role; else `UnsupportedPolicy` | [`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |
| Wrong magic | `UnsupportedFraming` | [`codec.rs`](../../../../core/crates/layerfs-content/src/object/codec.rs) |
| Wrong envelope kind, or bytes of another role | `WrongLogicalRole` | [`codec.rs`](../../../../core/crates/layerfs-content/src/object/codec.rs), [`admission.rs`](../../../../core/crates/layerfs-content/src/object/admission.rs) |
| Short or long input | `UnexpectedEof`, `TrailingBytes` | [`codec.rs`](../../../../core/crates/layerfs-content/src/object/codec.rs) |
| Bytes that do not hash to the ID | `IdentityMismatch` | [`admission.rs`](../../../../core/crates/layerfs-content/src/object/admission.rs) |
| Unknown role code, kind code or broken summary | `InvalidRecord(label)` | [`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs), [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| Unsupported grammar version | `UnsupportedMappingVersion { version }` | [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| Provider returns the wrong count | `BatchCardinality` | [`access.rs`](../../../../core/crates/layerfs-content/src/object/access.rs) |

An unknown or malformed object is refused with one of these errors. Nothing is
repaired, converted or re-read.

## Demonstration

These commands come from source and were not run while writing this page. None
of these tests has a platform gate, `#[ignore]`, feature flag, Docker or
environment requirement.

```text
 object_identity                         object_admission
 +-----------------------------+         +--------------------------------+
 | frozen hex fixture          |         | one valid object per role (13) |
 | -> encode == fixture bytes  |         | -> admit: ID, role, refs exact |
 | -> ID == recorded hex       |         | -> each bytes as every other   |
 | damaged byte -> new ID      |         |    role: refused               |
 +-----------------------------+         +--------------------------------+
```

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`object_identity.rs`](../../../../core/crates/layerfs-content/tests/object_identity.rs) | `layerfs-content` | Frozen domain probe, whole-file and chunk fixtures, CDC and mapping profile IDs, every envelope malformation, the 8 MiB field and 16 MiB envelope ceilings at ±1, and that `into_parts` keeps predecessors |
| [`object_admission.rs`](../../../../core/crates/layerfs-content/tests/object_admission.rs) | `layerfs-content` | `admit` on all 13 roles: exact references, no allocation copy; role confusion, truncation, wrong scope or policy, and rehashed impossible summaries are refused |
| [`object_context.rs`](../../../../core/crates/layerfs-content/tests/object_context.rs) | `layerfs-content` | `validate_context` on all 13 roles with one-ID demands; refusals before any demand; branch fill, level and summary checks; provider failure ends without retry |
| [`inode_leaf.rs`](../../../../core/crates/layerfs-content/tests/inode_leaf.rs) | `layerfs-content` | Exact leaf layout, both metadata forms, the 94-byte pooled value, pooled body round trip, every rejected malformation |
| [`object_key.rs`](../../../../core/crates/layerfs-storage/tests/object_key.rs) | `layerfs-storage` | The pack key `ObjectKey` is plain SHA-256 of sealed bytes, a separate digest from `ObjectId` |

```sh
cargo +1.85.1 test --locked -p layerfs-content --test object_identity
cargo +1.85.1 test --locked -p layerfs-content --test object_admission
cargo +1.85.1 test --locked -p layerfs-content --test object_context
cargo +1.85.1 test --locked -p layerfs-content --test inode_leaf
cargo +1.85.1 test --locked -p layerfs-storage --test object_key
```

The skeleton listed `physical_formats.rs`; it tests pack framings, so it now
belongs to [Physical storage](../physical-storage/README.md) and is replaced
here by the Content tests above.

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`core/docs/architecture/01-boundary.md`](../../../../core/docs/architecture/01-boundary.md) — the provider/consumer boundary is covered in [Access](access.md); its crate and provider descriptions are historical only.
- [`core/docs/architecture/02-objects.md`](../../../../core/docs/architecture/02-objects.md) — covered in [Identity](identity.md), [Kinds](kinds.md) and [Access](access.md).
- [`core/docs/architecture/08-representations.md`](../../../../core/docs/architecture/08-representations.md) — CDC is covered in [Files: Chunking](../files/chunking.md); CAS reuse and delta belong to [Physical storage: Encoding](../physical-storage/encoding.md); its v0.1.6 comparison is historical only.

---

← [Core](../README.md) · [Up: Core](../README.md) · [Objects: Identity](identity.md) →
