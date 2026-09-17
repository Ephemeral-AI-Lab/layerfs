# Canonical objects

> **Status:** Research; informative and not a product contract.

Part of the [replacement-core architecture](README.md) set. Source pin
`1884e3eca`; scope, method, measurement status and upkeep are stated in the
[index](README.md).

---

## 2. Canonical object model (C1)

### 2.1 Identity

`core/crates/layerfs-content/src/object/id.rs`

```text
   ObjectId = BLAKE3( "layerfs/object/v2\0" ‖ canonical object bytes )
              └── frozen domain separator ──┘

   32 bytes wide (DIGEST_BYTES)
```

Identity is a digest over the frozen domain **plus the complete canonical bytes**,
so it is not a raw content digest of the payload. Identical canonical bytes always
produce the same ID, and — because the digest covers the framing — **structure and
length are authenticated, not assumed**.

### 2.2 The `LFSO` envelope

`core/crates/layerfs-content/src/object/codec.rs`

```text
   byte:   0        1        2        3        4     5..8        9..12     13..
        ┌────────┬────────┬──────────────────────────────┬──────────────┬────────┐
        │ "LFSO" │  kind  │        payload_len           │  value_len   │ value  │
        │  4 B   │  1 B   │          BE u32              │   BE u32     │  var   │
        └────────┴────────┴──────────────────────────────┴──────────────┴────────┘
          magic    role        header = 9 bytes total      │
                                                           └─ value begins at HEADER_LEN + 4

   canonical length = 9 + 4 + value_len          (HEADER_LEN + VALUE_LEN_BYTES + value_len)
```

The kind byte for a bytes-role object is `BYTES_KIND = 1`. Two ceilings apply:

| Constant | Value | Checked at |
| --- | ---: | --- |
| `MAX_CANONICAL_OBJECT_BYTES` | 16 MiB | `canonical_len`, `decode_bytes_object` |
| `MAX_OBJECT_FIELD_BYTES` | 8 MiB | `canonical_len` (on `value_len`) |
| `MAX_PAYLOAD_BYTES` | 16 MiB − 9 | `decode_bytes_object` |

Encoding writes the final allocation **once** (`encode_bytes_object_to`), so no
intermediate copy of the same bytes exists. Decoding **borrows** the value out of
the supplied buffer, so a caller holding authenticated ownership does not copy the
payload to decode it.

`decode_bytes_object` rejects, in order: an over-long object, a short header,
wrong magic, wrong kind, an over-long payload, an over-long value, a value length
that overruns the payload, a truncated body, and trailing bytes. Every length,
kind and trailing-byte violation it finds is refused.

### 2.3 The thirteen roles

`core/crates/layerfs-content/src/object/output.rs`

A role is **logical meaning, not a physical FULL/DELTA choice** — the source says
this explicitly, and the physical choice belongs to C2.

| Code | `ObjectRole` | Produced by |
| ---: | --- | --- |
| 1 | `WholeFile` | complete-file construction below the cutoff |
| 2 | `Chunk` | the frozen CDC profile |
| 3 | `ExtentLeaf` | extent-tree leaf page |
| 4 | `ExtentBranch` | extent-tree branch page |
| 5 | `FileState` | logical root of a chunked file |
| 6 | `InodeLeaf` | compact inode-value leaf (also the pooled input grammar) |
| 7 | `DirectoryLeaf` | directory page, rows of `name → inode serial` |
| 8 | `DirectoryBranch` | directory tree child summaries |
| 9 | `InodeBranch` | inline inode-table child summaries |
| 10 | `FilesystemRoot` | scoped filesystem root |
| 11 | `AttributeLeaf` | generic `domain + key → value root` entries |
| 12 | `AttributeBranch` | attribute-tree child summaries |
| 13 | `Symlink` | symbolic-link target |

`ObjectRole::code()` / `from_code()` are the stable persisted encoding. The
persisted ceiling is enforced in SQL, not only in Rust — see [§6.3](05-storage.md#63-what-validate-refuses).

### 2.4 `FinalizedObject` and the four things it carries

```text
   FinalizedObject {
       id           : ObjectId            ── computed once from the canonical bytes
       role         : ObjectRole          ── logical meaning
       canonical    : Vec<u8>             ── the owned allocation, envelope included
       references   : Vec<ObjectId>       ── direct logical children, canonical order
       predecessors : AdvisoryPredecessors ── bounded advisory hints, preference order
   }
```

`into_parts()` moves **all five** pieces to the consumer. The source comment
records why the last one is not optional:

> The pieces are the identity, the role, the canonical bytes, the direct references
> **and the advisory predecessors**. A predecessor is an input the production save
> path consumes to choose a physical representation, so a consumer that takes
> ownership through this call has to receive it: **a form that dropped it silently
> discarded an input the object was built with.**

A predecessor is **advisory**: it is a hint about which existing object might make
a good physical delta base, bounded by `MAXIMUM_ADVISORY_PREDECESSORS`, and it
carries a `PredecessorProvenance` tag. It does not affect canonical identity — two
objects with identical canonical bytes have identical IDs regardless of which
predecessors were attached.

`FinalizedObject::new` re-decodes the supplied bytes as a canonical object before
accepting them, so a leaf or whole-file object must still be decodable canonical
framing.
