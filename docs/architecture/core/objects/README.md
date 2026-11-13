# Objects

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Canonical immutable objects and their IDs: the units every other topic names,
stores and compares, and the two ports through which they are read and written.

| | |
| --- | --- |
| Layer | [Core](../README.md) |
| Crates | `layerfs-content` |
| Source | [`object/`](../../../../core/crates/layerfs-content/src/object), [`contract/`](../../../../core/crates/layerfs-content/src/contract) |
| Uses | — |
| Used by | [Files](../files/README.md), [Filesystem trees](../filesystem-trees/README.md), [Compare and merge](../compare-and-merge/README.md), [Physical storage](../physical-storage/README.md), [History](../history/README.md) |
| Flows | [A Commit](../../flows/commit.md), [Project Init](../../flows/init.md) |

## Picture

```text
        Files, Filesystem trees, Compare and merge (constructors)
             |                                    ^
 value bytes |                                    | decoded values
 + role      v                                    |
 +------------------------------------------------------------------+
 | layerfs-content::object, ::contract                              |
 |  ObjectRole (13)      LFSO envelope         FinalizedObject      |
 |  inode leaf grammar   BLAKE3 ID over   ->   id, role, bytes,     |
 |  ConstructionPolicy   domain + bytes        references, preds    |
 +------------------------------------------------------------------+
             |                                    ^
 FinalizedConsumer::accept         AuthenticatedObjects::read_canonical_batch
             v                                    |
 +------------------------------------------------------------------+
 | caller: Physical storage Save / Reader, engine Store calls       |
 +------------------------------------------------------------------+
```

## What it does

### Identity and the envelope

```text
 offset 0        4      5              9              13
        +--------+------+--------------+--------------+-----------------+
        | "LFSO" | kind | payload_len  | value_len    | value           |
        | magic  | 0x01 | u32 BE       | u32 BE       | value_len bytes |
        +--------+------+--------------+--------------+-----------------+
 payload_len = value_len + 4        value = role magic + role grammar + body

 ObjectId = BLAKE3( "layerfs/object/v2\0" || canonical bytes )   32 bytes
```

| Behaviour | Detail | Source |
| --- | --- | --- |
| ID | BLAKE3 over `OBJECT_DOMAIN` then every canonical byte, envelope included; 32 bytes, 64 hex characters in text | [`id.rs`](../../../../core/crates/layerfs-content/src/object/id.rs) |
| Streaming ID | `ObjectId::stream()` hashes parts in order without keeping them; chunk construction hashes while it scans | [`id.rs`](../../../../core/crates/layerfs-content/src/object/id.rs), [`mapping/codec.rs`](../../../../core/crates/layerfs-content/src/file/mapping/codec.rs) |
| Envelope | `LFSO`, kind `1`, two lengths; `decode_bytes_object` returns the value as a borrowed slice and checks, in order, total ceiling, length, magic, kind, payload and value ceilings, truncation and trailing bytes | [`codec.rs`](../../../../core/crates/layerfs-content/src/object/codec.rs) |
| Equal logical content | Same ID for whole files, chunks, symlink targets and fresh file construction; a chunked edit result keeps the base's chunks and need not match; a filesystem root depends on its scope and serials | [Files](../files/README.md), [Filesystem trees](../filesystem-trees/README.md) |
| Pack key | Physical storage names a pack by `ObjectKey`, SHA-256 of the sealed pack bytes, a separate digest | [`port/pack.rs`](../../../../core/crates/layerfs-storage/src/port/pack.rs) |

### Object kinds

```text
 FilesystemRoot --> InodeBranch ..> InodeLeaf   one row per inode
                                       | content_root      | metadata_root
       +-------------------------------+--------+          v (root form)
       v                v                       v    AttributeBranch
 WholeFile or     DirectoryBranch ..>       Symlink    ..> AttributeLeaf
 FileState        DirectoryLeaf                              --> FileState
   |              (name -> serial rows)
   v
 ExtentBranch ..> ExtentLeaf --> Chunk          "..>" = zero or more levels
```

| Code | `ObjectRole` | Value magic, role byte | Direct references | Owner |
| --- | --- | --- | --- | --- |
| 1 | `WholeFile` | `LFS5SML\0` v1 | none | [Files](../files/README.md) |
| 2 | `Chunk` | `LFS4CHK\0` | none | [Files](../files/README.md) |
| 3 | `ExtentLeaf` | `LFS4MAP\0` v3, `0x08` | chunk IDs | [Files: Edits](../files/edits.md) |
| 4 | `ExtentBranch` | `LFS4MAP\0` v3, `0x09` | child pages | [Files: Edits](../files/edits.md) |
| 5 | `FileState` | `LFS4MAP\0` v3, `0x0a` | mapping root | [Files: Edits](../files/edits.md) |
| 6 | `InodeLeaf` | `LFS6INT\0` v1, `7` | each row's content and metadata root | [below](#inode-leaves) |
| 7 | `DirectoryLeaf` | `LFS6NSP\0` v1, `1` | none (serials are not IDs) | [Filesystem trees](../filesystem-trees/README.md) |
| 8 | `DirectoryBranch` | `LFS6NSP\0` v1, `2` | child pages | [Filesystem trees](../filesystem-trees/README.md) |
| 9 | `InodeBranch` | `LFS6INT\0` v1, `8` | child pages | [Filesystem trees](../filesystem-trees/README.md) |
| 10 | `FilesystemRoot` | `LFS6FSR\0` v1, `6` | inode-table root | [Filesystem trees](../filesystem-trees/README.md) |
| 11 | `AttributeLeaf` | `LFS4MET\0` v1, `9` | each value root | [Filesystem trees](../filesystem-trees/README.md) |
| 12 | `AttributeBranch` | `LFS4MET\0` v1, `10` | child pages | [Filesystem trees](../filesystem-trees/README.md) |
| 13 | `Symlink` | `LFS4LNK\0` v1, `5` | none | [Filesystem trees](../filesystem-trees/README.md) |

Codes and `ObjectRole::from_code` are in
[`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs).
The role code and the role byte inside a value are separate numbers. History
records (Commits, Branches, Layers) are not objects; see
[History](../history/README.md).

### Inode leaves

[`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs)
owns the 73-byte inode value and its leaf page, shared by Content and
Physical storage, which pools the values.

```text
 InodeLeaf = envelope 13 B | node header 31 B | count x row 81 B
             |<------ POOLED_PREFIX_BYTES = 44 ----->|
 row:   serial u64 BE (> 0, strictly increasing) | inode value 73 B
 value: 0 kind (1 file, 2 directory, 3 symlink; bit 0x80 = inline metadata)
        1..9 namespace_ref_count | 9..41 content_root
        41..73 attribute-tree root, or inline mode u32, mtime s i64,
               mtime ns u32, 16 zero bytes
 pooled body  = 44 B prefix + count x (serial u64 | ordinal u32)
 pooled value = envelope + "LFSIVL1\0" + 73 B value = 94 B
```

| Rule | Source |
| --- | --- |
| 1 to `MAXIMUM_LEAF_ROWS` (100) rows, at most 8,192 canonical bytes | [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| `InodeValue::validate(is_root)`: the root is a directory with count 0; other inodes have count ≥ 1, and only files and symlinks may exceed 1 | same |
| Unknown kind, nonzero inline padding or invalid portable metadata is refused, so one value has one encoding | same |
| `rebuild_leaf` re-encodes from rows, never from the supplied prefix, so a rebuilt leaf keeps its ID | same |

### Admission and context checks

```text
 FinalizedObject::admit(id, role, canonical, policy, inode_scope, timing)
   1 policy.validated()                          UnsupportedPolicy
   2 decode_bytes_object, ceilings first         framing and size errors
   3 ObjectId::for_bytes(canonical) == id        IdentityMismatch
   4 decode_references(role): role grammar,      WrongLogicalRole,
     local summaries, root scope                 InvalidRecord, ScopeMismatch
   5 references derived here; no predecessors
```

| Check | Detail | Source |
| --- | --- | --- |
| `admit` | One object alone; trusts no supplied reference list or predecessor. Per role: nonempty whole file under the cutoff, extent levels, file-state counts, inode placement, branch summaries, root scope | [`admission.rs`](../../../../core/crates/layerfs-content/src/object/admission.rs) |
| `validate_context` | Re-derives references, then reads each direct child as its own one-ID demand and checks levels, fills, keys, summaries, content kinds and portable metadata; refusals run before any demand | [`context/`](../../../../core/crates/layerfs-content/src/object/context) |
| Not checked | Peer authority, saved closure, serial membership, reverse bindings, aliases, cycles, full descendant topology: the caller's job ([Filesystem trees: Validation](../filesystem-trees/validation.md)) | [`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs) |
| Callers | Product constructors use `FinalizedObject::new` (checks the envelope, hashes once). `admit` runs in the [Store diagnostic](../../engine/store/diagnostics.md) scan; `validate_context` only in tests | [`diagnostic/scan.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/scan.rs), [`object_context.rs`](../../../../core/crates/layerfs-content/tests/object_context.rs) |

### Reading and writing objects

```text
         caller (Storage, engine)                    Content
 +--------------------------------------+    +---------------------------+
 | AuthenticatedObjects                 |<---| read_canonical_batch(ids) |
 |   bytes whose ID is ids[i], in order |--->| decode, read, construct   |
 | FinalizedConsumer                    |<---| accept(FinalizedObject)   |
 |   stores or forwards; owns the bytes |    | children before parents   |
 +--------------------------------------+    +---------------------------+
```

| Port | Contract | Source |
| --- | --- | --- |
| `read_canonical_batch(ids)` | One grouped demand, exactly one value per ID, in demand order; scoped and single-ID forms default to it; `decoded_pages()` may share decoded tree pages (skips a decode, never a check) | [`access.rs`](../../../../core/crates/layerfs-content/src/object/access.rs) |
| Provider answers | Bytes already authenticated; `MissingObject` (absent), `ProviderFailure` (held but unservable), `IdentityMismatch`; `PathNotFound` is a tree answer, not a provider one | [`contract/error.rs`](../../../../core/crates/layerfs-content/src/contract/error.rs) |
| Providers | Storage `Reader` and an open Save; engine Store ports and cached reads; the compare memo | [`read/provider.rs`](../../../../core/crates/layerfs-storage/src/read/provider.rs), [`save/provider.rs`](../../../../core/crates/layerfs-storage/src/save/provider.rs), [`read/cached.rs`](../../../../engine/crates/layerfs-store-service/src/read/cached.rs), [`compare/memo.rs`](../../../../core/crates/layerfs-content/src/compare/memo.rs) |
| `FinalizedObject` | `id`, `role`, owned `canonical` bytes, direct `references` in canonical order, up to four advisory `predecessors`; `into_parts` moves them all | [`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs) |
| `FinalizedConsumer::accept` | Emitted only when later input cannot change the object; an error ends construction, with no resend | [`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs) |
| Consumers | Storage Save sink, engine Commit staging; `DiscardingConsumer` only counts | [`save/operation.rs`](../../../../core/crates/layerfs-storage/src/save/operation.rs), [`construction/driver.rs`](../../../../engine/crates/layerfs-workspace/src/construction/driver.rs) |

| Advisory predecessor | Meaning | Proposed by |
| --- | --- | --- |
| `OriginalBase` | The base object an edit declared | Whole-file edit ([`edit/apply.rs`](../../../../core/crates/layerfs-content/src/file/edit/apply.rs)) |
| `UnchangedPrefix` | The object holding the bytes just before a replaced range | Mapping build, sorted tree pages ([`mapping/build.rs`](../../../../core/crates/layerfs-content/src/file/mapping/build.rs), [`sorted/page.rs`](../../../../core/crates/layerfs-content/src/filesystem/sorted/page.rs)) |
| `ReusedRange` | A stored object covering a reused range | Chunk runs ([`chunk_runs.rs`](../../../../core/crates/layerfs-content/src/file/construction/chunk_runs.rs)) |

### Construction policy

`ConstructionPolicy` is the profile a Store records;
[`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs)
validates it before any work.

| Field | Accepted | Default |
| --- | --- | --- |
| `small_file_threshold_bytes` (cutoff) | power of two, 131,072 to 1,048,576 | 131,072 |
| `whole_file_delta_max_depth` | 0 to 50; 0 disables whole-file delta | 16 |
| `chunk_delta_max_depth` | 0 to 50; 0 disables chunk delta | 8 |

`capacities()` derives the bounds by checked arithmetic: whole-file raw limit
`cutoff − 1`, frame limit `max(135,168, raw + raw/128 + 1024)`, canonical
limit `raw + 23`, chunk limits from the chunker, `stream_flush_entries` 192,
and a whole-file codec window log of 18 up to a 256 KiB cutoff, else 20. Content
records the depths; only Physical storage runs a delta codec.

## Invariants

| Rule | Enforced by |
| --- | --- |
| An object never changes after it is named: no setter for bytes or ID; the ID is computed once from the owned bytes | [`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs) |
| The canonical form decides the ID; compression, delta and pack layout are outside it | [`id.rs`](../../../../core/crates/layerfs-content/src/object/id.rs) |
| IDs are domain-separated, never a raw digest of the payload | [`id.rs`](../../../../core/crates/layerfs-content/src/object/id.rs) |
| One canonical encoding per value: decoders refuse trailing bytes, padding and non-canonical partitions instead of repairing them | [`codec.rs`](../../../../core/crates/layerfs-content/src/object/codec.rs), [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| References are derived from the bytes, never trusted from a caller | [`admission.rs`](../../../../core/crates/layerfs-content/src/object/admission.rs) |
| Predecessors are hints: at most four, outside the ID, chosen or ignored by Physical storage | [`predecessor.rs`](../../../../core/crates/layerfs-content/src/object/predecessor.rs) |
| Content opens no database, pack or file, and forbids unsafe code | [`lib.rs`](../../../../core/crates/layerfs-content/src/lib.rs) (`#![forbid(unsafe_code)]`) |
| Absence (`MissingObject`) and failure (`ProviderFailure`) stay distinct; a provider failure ends the operation once | [`contract/error.rs`](../../../../core/crates/layerfs-content/src/contract/error.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Canonical object, any role | `MAX_CANONICAL_OBJECT_BYTES` 16 MiB | [`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |
| One value field | `MAX_OBJECT_FIELD_BYTES` 8 MiB | same |
| Largest produced objects | whole file < cutoff + 23, chunk 32,789 B, tree page 8,192 B; the 16 MiB ceiling never binds on them | [`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs), [`limits.rs`](../../../../core/crates/layerfs-content/src/filesystem/limits.rs) |
| Advisory predecessors | 4; a fifth is `BoundedCapacityExceeded { what: "object.predecessors" }`; a repeated ID is ignored | [`predecessor.rs`](../../../../core/crates/layerfs-content/src/object/predecessor.rs) |
| Cutoff or depth out of range | `UnsupportedPolicy` | [`policy.rs`](../../../../core/crates/layerfs-content/src/contract/policy.rs) |
| ID width or text | `InvalidIdentityLength`, `InvalidIdentityText` | [`id.rs`](../../../../core/crates/layerfs-content/src/object/id.rs) |
| Wrong magic; wrong kind or role | `UnsupportedFraming`; `WrongLogicalRole` | [`codec.rs`](../../../../core/crates/layerfs-content/src/object/codec.rs), [`admission.rs`](../../../../core/crates/layerfs-content/src/object/admission.rs) |
| Short or long input | `UnexpectedEof`, `TrailingBytes` | [`codec.rs`](../../../../core/crates/layerfs-content/src/object/codec.rs) |
| Bytes that do not hash to the ID | `IdentityMismatch` | [`admission.rs`](../../../../core/crates/layerfs-content/src/object/admission.rs) |
| Unknown role or kind code, broken summary | `InvalidRecord(label)` | [`output.rs`](../../../../core/crates/layerfs-content/src/object/output.rs), [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| Unsupported grammar version | `UnsupportedMappingVersion { version }` | [`inode_leaf.rs`](../../../../core/crates/layerfs-content/src/object/inode_leaf.rs) |
| Provider returns the wrong count | `BatchCardinality` | [`access.rs`](../../../../core/crates/layerfs-content/src/object/access.rs) |

Nothing malformed is repaired, converted or re-read.

## Demonstration

In-memory tests with no platform gate, feature, Docker or environment.
Give each command a wall timeout of at most 120 s.

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`object_identity.rs`](../../../../core/crates/layerfs-content/tests/object_identity.rs) | `cargo +1.85.1 test --locked -p layerfs-content --test object_identity` | any | Frozen domain and fixture IDs, a damaged byte changes the ID, every envelope malformation, the 8 MiB and 16 MiB ceilings at ±1 |
| [`object_admission.rs`](../../../../core/crates/layerfs-content/tests/object_admission.rs) | `cargo +1.85.1 test --locked -p layerfs-content --test object_admission` | any | `admit` on all 13 roles: exact references; role confusion, truncation, wrong scope and impossible summaries refused |
| [`object_context.rs`](../../../../core/crates/layerfs-content/tests/object_context.rs) | `cargo +1.85.1 test --locked -p layerfs-content --test object_context` | any | `validate_context` with one-ID demands; refusals before demands; a provider failure ends without retry |
| [`inode_leaf.rs`](../../../../core/crates/layerfs-content/tests/inode_leaf.rs) | `cargo +1.85.1 test --locked -p layerfs-content --test inode_leaf` | any | Leaf layout, both metadata forms, the 94-byte pooled value and pooled round trip |
| [`object_key.rs`](../../../../core/crates/layerfs-storage/tests/object_key.rs) | `cargo +1.85.1 test --locked -p layerfs-storage --test object_key` | any | `ObjectKey` is plain SHA-256 of sealed pack bytes, separate from `ObjectId` |
