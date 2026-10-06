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

### 2.5 Runtime semantic admission — #307 implementation checkpoint

This section describes the implementation after `6a0dbe003`; it does not advance
the earlier sections' source pin or historical measurements.
[FinalizedObject::admit](../../crates/layerfs-content/src/object/output.rs) is the
untrusted-input boundary. The selected Store construction policy and expected
inode scope are required inputs. The existing `new` checks only the envelope and
does not establish semantic admission.

[Admission](../../crates/layerfs-content/src/object/admission.rs) first validates
policy and envelope/object/field bounds, hashes once under the canonical domain,
then calls the owning decoder for each of the thirteen roles. Leaf/branch role
confusion, invalid grammar and impossible local summaries fail explicitly. A
filesystem root must have the expected allocation scope. Whole-file bounds derive
from the selected policy; this imposes no total file/Save/Workspace cap. Directory
decode now rejects an impossible declared row count before reserving decoded rows.

References come from decoded bytes in canonical order, retaining repeats: extent
payloads/children, file mapping root, inode content then metadata for each row,
branch children, attribute values and filesystem inode table. Directory serials,
profile and scope identities are not stored-object references. Payload/symlink
roles have no children. No sandbox reference list or predecessor provenance claim
is accepted. The canonical allocation transfers intact; bounded decoded page
state and reference vectors are temporary per-object work. Hashing costs O(B),
decoding/reference extraction O(B+R); R is bounded by that role's page grammar.
There is no Save-sized resident set or graph traversal here.

Runtime authority, reference savedness, child role/level/summary/fill context,
inode-root placement and complete namespace topology remain separate obligations.
Admission permits a locally valid short root page; closure must check whether it
actually appears in a non-root position. The real Store still rejects a derived
missing inode dependency at Save completion. Public tests cover all roles,
cross-role confusion, rehashed malformed input, allocation ownership, policy/
scope and real macOS Store reconstruction/closure. These small proofs do not
complete P1 or qualify the authenticated runtime.

### 2.6 Direct child context — #307 R2 component checkpoint

The additive public [FinalizedObject::validate_context](../../crates/layerfs-content/src/object/output.rs)
uses the owning canonical decoders with an authenticated provider, the persisted
Store construction policy, expected inode scope/root serial and a timing scope.
The [context modules](../../crates/layerfs-content/src/object/context/mod.rs) own
these checks. This describes component source; authenticated runtime assembly,
R2 completion and integrated qualification have separate evidence obligations.

Validation first checks policy and root-serial representability, then re-derives
local references and requires exact agreement with the object's retained list.
It does not rehash canonical bytes: `new`/`admit` establish identity, private
canonical/ID fields have no mutation API, and `with_references` changes only the
list checked here. Providers authenticate demanded child bytes. The object is
borrowed, so validation neither copies nor transfers its canonical allocation.

| Role context | Owning checks |
| --- | --- |
| Extent leaf | Actual Chunk grammar and every slice end within that Chunk's decoded payload |
| Extent branch | Child non-root fill, level and exact cumulative byte/extent summaries |
| FileState | Actual mapping root under root fill rules; exact level/logical-byte/extent totals |
| Inode leaf | Every serial representable in the expected scope; root placement from `serial == root_serial`; kind-specific content root and required portable mode/mtime |
| Inode branch | Child fill/level/maximum key and exact summed subtree count; local child serial representability and visible sibling key ordering |
| Directory | Local serial representability and no root binding; branch child fill/level/maximum key and exact count/encoded-row-byte sums |
| Attribute | Nonempty bounded extent-only value roots with exact mapping-root summaries; branch child fill/level/maximum key and exact count/encoded-row-byte sums |
| Filesystem root | Supported profile, expected scope/serial and demand lookup of its required zero-count Directory root inode, content kind and portable metadata |

Regular-file WholeFile capacity follows the selected policy. A valid FileState
below the fresh-construction cutoff remains accepted: a no-op localized edit can
retain that representation. Generic attribute values use extent-only roots;
portable metadata is read through the existing fixed mode/mtime key and value
interfaces and validated against the owning inode kind.

The contextual [provider adapter](../../crates/layerfs-content/src/object/context/read.rs)
splits every underlying demand into one-ID windows, checks exact cardinality and
forwards the actual provider work under `content.context.read`. Direct roots are
released before the next inode/content demand; an inode leaf's one hundred roots
never become a single request exceeding the Reader's 32 MiB window. Existing
portable reads retain only their fixed key/path/value windows. A failed read
returns its exact Content error once. A runtime that uses a Save must recover its
original typed storage failure through the owning Save adapter; this API supplies
no retry, refresh, replay or failed-Save continuation.

Let B be the local canonical bytes inspected, C the total demanded canonical
bytes and D the actual bounded canonical tree depth. Direct branch/leaf work is
O(B+C+R), with R bounded by the page grammar. An inode leaf has at most 100 rows;
its required portable fields and a filesystem root's single inode demand add
O(I*D) path work plus C, where I is the local inode count. Decode/reference state
is page-bounded; one content root is held at a time and portable values have
fixed 4/12-byte logical widths. Equal child IDs can be demanded again; there is
no cross-object memo or claimed amortized cache saving. Across accepted objects,
cost is the sum of these actual demands and provider authentication/Save work.
No file-, Save- or Workspace-sized graph is collected and no whole-root scan
is moved into binding.

These are direct context checks. A branch child's first summary key is not the
minimum of every descendant, so visible sibling ordering does not establish
complete descendant range order. Serial membership, exact reverse bindings,
alias counts, cycles, predecessor provenance, authority and complete saved
closure remain their owning caller/K2 obligations. The external
[object_context tests](../../crates/layerfs-content/tests/object_context.rs) cover
all thirteen roles, malformed direct contexts, early refusals, one-ID demand
windows and exact original read failures; their source presence is not a passing
test receipt or complete R2 acceptance.
