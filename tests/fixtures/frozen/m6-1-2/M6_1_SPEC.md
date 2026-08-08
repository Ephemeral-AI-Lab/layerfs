# M6.1 implementation specification — portable identity, canonical codec, and typed records

```text
DOCUMENT_ROLE: NORMATIVE_M6_1_IMPLEMENTATION_SPEC
SPEC_REVISION: 2026-08-06.1
M6_0_STATE: FINAL_M6_0_COMPLETE
M6_1_IMPLEMENTATION_STATE: NOT_STARTED
M6_1_START_GATE: FRESH_CUSTODY_RECEIPT_REQUIRED
PRODUCT_SOURCE_MODE_BEFORE_START_GATE: READ_ONLY
M6_2_AND_LATER: UNAUTHORIZED
M7: REMOVED
M8_M9_QUALIFICATION: NOT_STARTED
PHASE_2: STOP
PERFORMANCE_CLAIM: NONE
```

## 1. Required outcome

M6.1 creates the dependency-free portable core for:

- strict canonical path and byte codecs;
- domain-separated structural logical identity;
- distinct typed physical identity and immutable object-record schemas;
- bounded, effect-free tree/root construction and traversal;
- complete typed strong-edge enumeration;
- exact canonical-object comparison as a pure state machine; and
- deterministic tests against the M6.0 golden vectors and hostile corpus.

M6.1 does **not** create storage authority. It cannot mint
`DurabilityReceipt`, `AcceptedVersion`, or `AcceptedBinding`; open a carrier;
write a pack; publish a reference; materialize a Workspace; run a benchmark;
or claim memory, RSS, durability, or performance conformance for the product.

The resulting core is the semantic foundation for the already selected
CAS + exact CDC + structural-reuse + direct-to-pack architecture. M6.1 does
not reopen that architecture and does not implement the later layers.

## 2. Authority and direct consistency links

An implementor MUST read the following documents before creating the M6.1
start receipt. If two documents appear inconsistent, stop and record the
conflict; do not choose a new interpretation in code.

| Concern | Governing document | M6.1 use |
|---|---|---|
| M6.0 seal and custody | [M6.0 final receipt](M6_0_START_RECEIPT.md) | Proves the predecessor is sealed; supplies the canonical product source and the fresh-custody requirement. |
| Phase 1 contract | [Phase 1 SPEC](SPEC.md) | OD-01, OD-05, OD-06, limits, equality, dependency, and portability rules. |
| M6 implementation boundary | [M6 SPEC](M6_SPEC.md) | S1 ownership, LOC envelope, M6.1 acceptance, and later-milestone exclusions. |
| Logical identity decision | [ADR-003](../../design/decisions/ADR-003-structural-version-identity.md) | Exact structural domains, root sentinel, logical/physical separation, and collision semantics. |
| Identity algorithm | [A1 structural Version identity](../../algorithm/01-identity-and-admission/01-canonical-version-identity.md) | Canonical construction, exact occupied-ID comparison, export distinction, and complexity shape. |
| Identity model | [state identity](../../design/02-state-identity.md) | Typed logical hierarchy and the rule that identity is not authority. |
| Canonical vectors and root-sentinel correction | [M6.0 golden vectors](M6_0_GOLDEN_VECTORS.md) | Exact accepted bytes, IDs, hostile mutations, corrected root-sentinel cases, expected outcomes, and seals. |
| Receipt grammar | [ChangeReceiptV1 contract](M6_0_CHANGE_RECEIPT_V1.md) | Type naming and boundary awareness only; receipt admission is not implemented in M6.1. |
| Source ownership | [source/ownership receipt](M6_0_SOURCE_OWNERSHIP_AND_POLICY_RECEIPT.md) | Fresh-V1 provenance, new-core ownership, Cargo surfaces, and forbidden dependency scan. |
| Native runtime model | [native Workspace model](M6_NATIVE_WORKSPACE_MATERIALIZATION_MODEL.md) | Boundary only: M6.1 never materializes or mounts a Workspace. |
| Memory contract | [bounded-memory SPEC](../../memory/SPEC.md) | M6.1 must be allocation-safe; the shared `StorageResourceLedger` remains M6.3 work. |
| Public API boundary | [API index](../../api/README.md) | No raw ID becomes accepted/public authority. |
| Benchmark boundary | [Phase 1 benchmark contract](benchmark.md) | No benchmark, target selection, historical control, or speed claim belongs to M6.1. |
| Pack boundary | [pack algorithm index](../../algorithm/07-physical-pack-storage/README.md) and [pack ASCII diagram index](../../diagrams/storage/pack/README.md) | Consume only physical object grammar; pack writing, indexes, journals, caches, and durability remain later work. |

`recommendation.md`, `recommendation-verification.md`, Stage 04.6 MPLA, and
`upgrade-2.0-phase-1` are not implementation authority and are not source or
fixture donors.

## 3. Canonical product custody

The only product checkout permitted for M6.1 is:

```text
PATH:   /Users/yifanxu/Ephemeral-AI-Lab/ephemeral-sandbox-v2-worktrees/ephemeral-sandbox-v2
BRANCH: ephemeral-sandbox-v2
REMOTE: https://github.com/Ephemeral-AI-Lab/ephemeral-sandbox.git
BASE:   official V1 v0.1.4
BASE_COMMIT: b22862550e0a7cb4fe61ce581831e9244cc492b5
```

Before the first product edit, create `M6_1_START_RECEIPT.md` in this Phase 1
documentation directory. It MUST record:

1. absolute checkout path, branch, remote, `HEAD`, `git status --short`, and
   whether `HEAD` still descends from the sealed base;
2. the M6.0 receipt state and SHA-256 seals of this specification, the tracker,
   `SPEC.md`, `M6_SPEC.md`, ADR-003, and the golden-vector documents;
3. the exact M6.1 product allowlist from section 6;
4. the exact protected/non-authorized scope from sections 4 and 5;
5. toolchain and Cargo dependency facts; and
6. explicit `M6_1_EDIT_AUTHORIZATION: GO` only if the checkout is clean or all
   pre-existing changes are attributable, non-overlapping, and owner-approved.

If any value differs from the sealed custody facts, the receipt remains
`STOP` and no product edit is allowed. A second product worktree, temporary
implementation clone, or historical branch is forbidden.

## 4. Exact M6.1 scope

### 4.1 In scope

- New `sandbox-runtime-layerstack-core` crate and the mechanical Cargo wiring
  required to compile it.
- Borrowed/caller-supplied, capacity-first canonical encoding and decoding.
- Portable path/component validation and unsigned-byte ordering.
- Logical types and IDs:
  `LogicalChunkIdV1`, `LogicalFileIdV1`, `FileNodeIdV1`,
  `SymlinkNodeIdV1`, `DirectoryNodeIdV1`, and `VersionIdV1`.
- Physical/profile types and IDs:
  `ChunkerSpecV1`, `DigestSpecV1`, `ProfileSpecV1`, `ChunkerSpecId`,
  `DigestSpecId`, `ProfileId`, `ChunkId`,
  `VersionRecordId`, `TreeRecordId`, `FileRecordId`, and
  `SymlinkRecordId`.
- Exact `ELSOBJ01` envelope and the five immutable physical object kinds:
  `VersionRecord`, `Tree`, `File`, `Symlink`, and `Chunk`.
- Typed, complete strong-edge visiting without source-sized collections.
- Structural root semantics, including the one implicit root sentinel.
- Canonical tree paging/grouping as a pure bounded transformation over
  caller-owned or borrowed storage.
- A narrow streaming digest port. Core code frames exact logical and physical
  preimages; the port supplies BLAKE3-256 execution later.
- Pure exact comparison/validation of two canonical typed objects using
  caller-owned bounded windows: absent a fatal, both reach simultaneous exact
  EOF; after a fatal, the conditionally activated amendment's bounded
  fatal-terminal/equal-frontier collection law applies.
- Deterministic golden, hostile, permutation, mutation, and limit tests.
- Cargo/source-policy proof that the core has no forbidden dependency or host
  effect and no historical POC code was imported.

### 4.2 Explicitly deferred

The following may be named as downstream consumers but MUST NOT be implemented,
wired, tested live, or silently approximated in M6.1:

- M6.2 exact FastCDC boundary implementation, CDC-facing runtime table
  constants/lookups, streaming ring, or range-resynchronization; M6.1 may only
  encode/decode and validate the already frozen table field supplied as a
  borrowed `ChunkerSpecV1` value. The owner-authorized fixed-profile ruling
  permits one immutable OD-05 table solely inside the M6.1 fixed-profile codec
  for field-local compatibility validation; it is not exposed to or used by a
  CDC boundary iterator;
- M6.3 `StorageResourceLedger`, runtime BLAKE3 adapter, queues, caches, spill,
  mappings, FDs, admission, custody, or source I/O;
- M6.4+ pack writing, embedded index, catalog, journal, quarantine, durability,
  publication, recovery, GC/repack, references, or lifecycle work;
- M8/M9 workload execution, qualification, memory/RSS measurement, benchmarks,
  target selection, or comparison with Stage 04.6;
- Workspace construction, full native-base materialization, OverlayFS mount,
  Docker/runtime changes, Firecracker, WASM/WASI, or Phase 2; and
- any M7 or `whole_version.rs` product/control path. M7 is removed.

## 5. Non-negotiable prohibitions

M6.1 product source, tests, fixtures, build scripts, and qualification helpers
MUST contain no:

- reflink, clone, copy-offload, `FICLONE`, `FICLONERANGE`, `clonefile`,
  `COPYFILE_CLONE`, `FSCTL_DUPLICATE_EXTENTS_TO_FILE`, semantic equivalent, or
  “try clone then copy” fallback;
- FUSE-family filesystem, `virtiofs`, `9p`, or alternate mounted storage view;
- SQLite/RocksDB/LMDB/other database truth, loose-object truth, sidecar-index
  truth, second catalog, second whole-Version store, or mutable accepted object;
- payload cache, native-base cache, hidden warm carrier, whole-pack residency,
  source-sized `Vec`, unbounded map/set/queue/tree, or implicit global state;
- filesystem, path-to-host conversion, provider, Docker, OverlayFS, async
  runtime, serde, FFI, `unsafe`, or hash-library dependency inside the portable
  core crate; or
- copy/import of product code, tests, fixtures, benchmark setup, configuration,
  or algorithms from Stage 04.6, `upgrade-2.0-phase-1`, or any POC branch.

The crate root MUST use `#![forbid(unsafe_code)]`. The core crate has no normal
or development dependency on a hash library. M6.1 validates exact digest
preimages against sealed BLAKE3-256 outputs through a deterministic test port;
the real pinned BLAKE3 adapter remains later host-layer work. This preserves
both the frozen BLAKE3-256 contract and the sealed no-hash-library core rule.

M6.1 may hash a caller-supplied, already delimited logical chunk and validate
an ordered logical chunk-reference stream. It MUST NOT discover CDC boundaries
or treat a convenience chunker as an oracle. The exact cutter and its bounded
ring are M6.2 work.

## 6. Product file/folder plan and LOC envelope

All paths below are relative to the canonical product checkout. `NEW` means
absent at the M6.0 source seal. LOC ranges are review envelopes, not quotas and
exclude blank/comment-only padding.

```text
ephemeral-sandbox-v2/
|-- Cargo.toml                                           MOD  2-8 changed lines
|-- Cargo.lock                                           MOD  generated delta only
`-- crates/sandbox-runtime/
    |-- layerstack/
    |   `-- Cargo.toml                                   MOD  1-5 changed lines
    `-- layerstack-core/                                 NEW
        |-- Cargo.toml                                   NEW  15-35 lines
        |-- src/
        |   |-- lib.rs                                   NEW  30-50 Rust LOC
        |   |-- codec.rs                                 NEW  250-450 Rust LOC
        |   |-- error.rs                                 NEW  50-100 Rust LOC
        |   |-- identity.rs                              NEW  200-400 Rust LOC
        |   |-- object.rs                                NEW  300-600 Rust LOC
        |   |-- path.rs                                  NEW  50-100 Rust LOC
        |   |-- port.rs                                  NEW  100-200 Rust LOC
        |   |-- root.rs                                  NEW  150-300 Rust LOC
        |   `-- tree.rs                                  NEW  270-400 Rust LOC
        `-- tests/
            |-- golden_vectors.rs                        NEW  350-700 Rust LOC
            `-- hostile_and_properties.rs                NEW  400-800 Rust LOC
```

Production S1 target: **1,400-2,600 Rust LOC**, exactly matching the sealed M6
allocation. M6.1 test target: **750-1,500 Rust LOC**. Expected total new Rust
for this milestone: **2,150-4,100 LOC**.

File responsibilities are exclusive:

| File | Sole responsibility | Forbidden responsibility |
|---|---|---|
| `lib.rs` | Export the frozen portable M6.1 surface; forbid unsafe. | Store/runtime composition or broad re-exports. |
| `codec.rs` | Checked integer/framing codec, bounded streaming cursor/writer, exact EOF. | Filesystem I/O, allocation policy, hashing algorithm. |
| `error.rs` | Portable structural/codec/port error taxonomy and vector outcome mapping. | Host I/O errors, store authority, quarantine side effects. |
| `identity.rs` | Distinct typed IDs, domains, exact logical/physical hash framing, exact comparison state. | Digest implementation, locator, existence, acceptance. |
| `object.rs` | `ELSOBJ01` header, immutable payload schemas, typed strong-edge visitor. | Pack layout, catalog, journal, mutable carrier state. |
| `path.rs` | OD-01 relative path/component validation and byte ordering. | Host path canonicalization or filesystem lookup. |
| `port.rs` | Effect-free borrowed byte source/sink/digest/result interfaces. | Concrete hash, runtime, file, provider, cache, ledger implementation. |
| `root.rs` | Logical root construction/validation and sentinel law. | Accepted binding, publication, materialization. |
| `tree.rs` | Canonical logical/physical tree ordering, paging, traversal, and bounds. | Directory enumeration or unbounded resident trees. |
| `golden_vectors.rs` | Exact M6.0 accepted bytes/IDs/outcomes using an independent deterministic test port. | Product hash implementation or copied POC fixtures. |
| `hostile_and_properties.rs` | Hostile decoding, permutations, mutation, limits, typed edges, dependency-independent properties. | Live storage, E2E, benchmark, or host effects. |

No other product path is editable in M6.1. If compile evidence appears to
require another path, stop, record the exact dependency chain and proposed
LOC, and obtain owner approval before editing it.

## 7. Architecture and flow — ASCII only

### 7.1 Milestone boundary

```text
             SEALED M6.0 CONTRACT BYTES
        SPEC + ADR-003 + vectors + source receipt
                         |
                         v
     +-------------------------------------------+
     | sandbox-runtime-layerstack-core (M6.1)    |
     |-------------------------------------------|
     | path | codec | identity | object          |
     | root | tree  | errors   | narrow ports    |
     +-------------------------------------------+
          | canonical bytes        | typed edges
          | logical/physical IDs   | pure results
          v                        v
     deterministic tests      later host/store consumers
     (no host effects)         (NOT AUTHORIZED IN M6.1)

     X FastCDC implementation       -> M6.2
     X ledger/hash host adapter     -> M6.3+
     X packs/catalog/durability     -> M6.4+
     X materialization/runtime      -> later M6
     X benchmark/qualification      -> M8/M9
     X Phase 2                      -> STOP
```

### 7.2 Identity is not authority

```text
portable facts
    |
    +-- canonical structural preimages -- DigestPortV1 --> typed logical IDs
    |                                                          |
    |                                                          v
    |                                                    VersionIdV1
    |
    +-- canonical ELSOBJ01 bytes -------- DigestPortV1 --> typed physical IDs

typed ID == expected canonical content
typed ID != existence
typed ID != durability
typed ID != accepted closure
typed ID != Head/Root membership
typed ID != runtime authorization
```

### 7.3 Canonical logical hierarchy

```text
VersionIdV1
  `-- implicit root DirectoryNodeIdV1 (mode field == 0x1000)
       |-- explicit DirectoryNodeIdV1 (mode 0x0000..0x0fff)
       |-- FileNodeIdV1
       |    `-- LogicalFileIdV1
       |         `-- ordered (LogicalChunkIdV1, chunk_len)
       `-- SymlinkNodeIdV1

directory names: unique, unsigned-byte sorted, valid path components
holes: logical zero bytes before LOGICAL_CDC_V1
locators/packs/providers/host paths: never members of this hierarchy
```

### 7.4 Physical immutable record hierarchy

```text
VersionRecordId --strong--> root TreeRecordId
TreeRecordId    --strong--> Tree/FileRecordId/SymlinkRecordId children
FileRecordId    --strong--> ordered ChunkId references
SymlinkRecordId ----------> no object edge
ChunkId ------------------> no object edge

every record = ELSOBJ01 header + exact typed payload + exact EOF
record identity = BLAKE3-256 framed over the complete ELSOBJ01 object
carrier/pack/catalog location = absent from M6.1 object truth
```

## 8. Frozen types, fields, and naming

### 8.1 Naming laws

- Rust type names use the exact suffix `V1` where the schema/domain is
  versioned. Do not use generic `Digest32`, `StateStore`, `SnapshotId`,
  `ManifestId`, or `RootRecordV2/V3` as aliases for the new types.
- Logical IDs and physical IDs are distinct newtypes with private raw-byte
  constructors. Cross-domain comparison/conversion is not implemented.
- `VersionIdV1` names logical structural identity. `VersionRecordId` names the
  physical immutable closure record. `FlatVersionDigestV1` names export-only
  verification. None is an alias for another.
- `DirectoryNodeV1` remains one type. Root versus explicit directory is
  validated by context; do not invent `RootDirectoryNodeV1` as a second wire
  type.
- `ROOT_DIRECTORY_MODE_SENTINEL_V1` is exactly `0x1000`. It is identity-only,
  never an OS mode, and requires no field, allocation, traversal, payload I/O,
  or materialization branch beyond validating the existing `u16` mode.
- Physical object kind values and directory child-kind values are different
  enums and MUST NOT be cast or inferred from each other.

### 8.2 Logical grammar

All logical integers are fixed-width unsigned little-endian. Domains and field
order are exactly:

```text
LogicalChunkIdV1 := BLAKE3-256(
  "ESV2-LCHUNK" 00 | u16-LE(1) | u64-LE(payload_len) | payload)

LogicalFileIdV1 := BLAKE3-256(
  "ESV2-LFILE" 00 | u16-LE(1) | u64-LE(logical_len) |
  u32-LE(chunk_count) |
  ordered [LogicalChunkIdV1[32] | u64-LE(chunk_len)])

FileNodeIdV1 := BLAKE3-256(
  "ESV2-FNODE" 00 | u16-LE(1) | portable_mode:u16-LE |
  LogicalFileIdV1[32] | u64-LE(logical_len))

SymlinkNodeIdV1 := BLAKE3-256(
  "ESV2-SNODE" 00 | u16-LE(1) |
  u32-LE(target_len) | target)

DirectoryNodeIdV1 := BLAKE3-256(
  "ESV2-DNODE" 00 | u16-LE(1) | portable_mode:u16-LE |
  u32-LE(child_count) |
  ordered [u32-LE(name_len) | name | child_kind:u8 | child_id[32]])

VersionIdV1 := BLAKE3-256(
  "ESV2-VROOT" 00 | u16-LE(1) | root DirectoryNodeIdV1[32])
```

Logical directory child kinds are `0x01 file`, `0x02 directory`, and
`0x03 symlink`. Files and explicit directories accept modes
`0x0000..0x0fff`; the implicit root accepts only `0x1000`. An explicit child
rejects `0x1000` and a Version root edge rejects an explicit-directory mode.

### 8.3 Frozen physical profile-record grammar

Before physical objects, M6.1 implements exact fixed profile-record codecs:

```text
DigestSpecV1:  16 bytes, frozen unkeyed BLAKE3-256/hash-frame declaration
ChunkerSpecV1: 2,116 bytes, profile fields including borrowed GEAR bytes
ProfileSpecV1: 136 bytes, frozen schema/count/size/fanout/profile fields
```

Their exact fields, big-endian widths, reserved bytes, table values, seals,
and `ELSHASH1` tags `0x01`, `0x02`, and `0x03` are OD-05 in
[SPEC.md](SPEC.md) and the [M6.0 golden vectors](M6_0_GOLDEN_VECTORS.md).
Implementing these deterministic records does not authorize the M6.2 CDC
iterator, a CDC-facing runtime GEAR lookup, or the later concrete BLAKE3 host
adapter. Under the owner-authorized fixed-profile projection, the codec owns
one immutable OD-05 GEAR validation table solely to compare each serialized
`GEAR[i]` at its field-local wire ordinal; it cannot be exported to or called
by a boundary iterator, and product code must not regenerate it through MD5 or
another hash dependency. Golden tests supply the sealed `ChunkerSpecV1` bytes
as input and assert exact pass-through encoding, field-local validation, and
typed ID.

### 8.4 Physical object grammar

Physical integers are fixed-width unsigned big-endian. Each object is:

```text
"ELSOBJ01"[8]
schema:u16-BE = 1
kind:u8
flags:u8 = 0
ProfileId[32]
payload_len:u64-BE
payload[payload_len]
exact EOF
```

Physical object kinds are `0x01 VersionRecord`, `0x02 Tree`, `0x03 File`,
`0x04 Symlink`, and `0x05 Chunk`. The exact Version, Tree directory/leaf/index,
File extent, Symlink, and Chunk payload fields are OD-06 in
[SPEC.md](SPEC.md); M6.1 MUST implement those bytes verbatim and MUST NOT
create an alternate abbreviated in-memory or wire schema.

Every immutable object exposes a complete typed strong-edge visitor:

| Record | Strong edges |
|---|---|
| `VersionRecordV1` | exactly one `root_tree_id: TreeRecordId` |
| Tree directory | optional `root_page_id: TreeRecordId` iff nonempty |
| Tree leaf | one typed `TreeRecordId`, `FileRecordId`, or `SymlinkRecordId` per ordered entry |
| Tree index | one `TreeRecordId` per canonical page child |
| File | every ordered `ChunkId` referenced by every data extent |
| Symlink | none |
| Chunk | none |

The visitor must stream edges and cannot return a source-sized `Vec`.

### 8.5 Bounds required before allocation or iteration expansion

At minimum, M6.1 enforces:

- name/component bytes `1..=255`, path bytes `1..=4096`, path depth `<=256`;
- nonempty UTF-8 symlink target `<=4,096` bytes with no NUL;
- logical/file/canonical bytes `<=8,589,934,592`;
- logical/physical chunk bytes `<=32,768`;
- entries `<=1,000,000`, tree objects `<=4,000,001`, chunk objects
  `<=2,310,720`, total objects `<=7,310,722`;
- extents per file `<=262,144`, chunk refs per file `<=1,310,720`, extents
  per Version `<=1,262,144`, and chunk refs per Version `<=2,310,720`;
- Tree leaf fanout `<=192`, Tree index fanout `<=96`, page depth `<=2`;
- maximum physical object bytes `50,593,858`; and
- every count/length multiplication, sum, offset, and cast checked before read,
  seek request, loop expansion, scratch request, or allocation by a caller.

## 9. Required algorithms

### 9.1 Capacity-first strict decode

```text
decode_typed_value(source, limits, visitor):
    read fixed prefix into caller-owned fixed scratch
    validate domain/magic, schema, kind, flags and reserved fields
    parse every count and length with checked arithmetic
    reject a per-field, coupled, aggregate or encoded-size limit violation
        before requesting variable bytes or expanding an iteration
    for each canonical element:
        borrow at most the next bounded window
        validate UTF-8/kind/mode/order/uniqueness/typed edge
        emit a borrowed value or edge to the caller-supplied visitor
    validate declared counts and reconstructed lengths
    require exact EOF
    return a validated typed view/result; never return a partial value
```

Unknown fields, unknown kinds, padding, extensions, trailing bytes, duplicate
names, noncanonical order, malformed UTF-8, incomplete bytes, arithmetic
overflow, and ambiguous encodings fail deterministically.

### 9.2 Canonical encode and digest

```text
derive_typed_id(validated_value, digest_port, borrowed_window):
    checked-plan the exact canonical length, domain, schema, fields, and caps
    stream only to DigestPortV1 in deterministic <=65,536-byte blocks
    finish exactly once and wrap exactly 32 bytes in the private typed newtype
    call no sink and return no accepted/existence/durability capability

encode_and_derive(validated_value, digest_port,
                  PrivateCanonicalSinkV1, borrowed_window):
    checked-plan the exact canonical length, domain, schema, fields, and caps
    begin one private exact-length session using <=65,536 borrowed bytes
    tee each canonical block once to digest then private write
    on any pre-finish failure abort/Drop with zero visibility and no ID
    finalize and validate digest before one finish_exact visibility point
    after successful finish perform only infallible return
```

The core owns all domain separators and physical `ELSHASH1` framing. The
digest port cannot select domains, schemas, field order, or output width. The
only allowed algorithm for these V1 contracts is unkeyed BLAKE3-256. M6.1
does not implement the algorithm; its deterministic test port checks complete
preimages and returns the already sealed expected output.

`PrivateCanonicalSinkV1` is a bounded caller-memory transaction port, not a
whole-output reservation: `begin_exact(total_len, borrowed_window)` creates a
private session, `write_private(segment)` remains invisible, and
`finish_exact()` is the sole visibility point. `abort`/Drop discards. The
session retains at most the borrowed 65,536-byte window plus O(1) scalar state
and never allocates `total_len`. Its completed bytes convey no durability,
existence, local-resource acceptance, custody, publication, or storage
authority. M6.1 supplies deterministic memory/discarding fakes only.

`ChunkerSpecV1` is only a frozen canonical profile value in M6.1. Logical
chunk identity accepts one already bounded chunk payload, and logical-file
identity accepts an already ordered `(LogicalChunkIdV1, chunk_len)` stream.
No M6.1 function scans bytes for CDC boundaries.

### 9.3 Canonical directory construction

```text
construct_directory(mode_context, ordered_entry_source, output, digest):
    validate root/explicit mode context
    last_name := NONE
    count := 0
    for each borrowed entry:
        validate one OD-01 name component and typed child kind/ID
        require last_name < name by unsigned byte ordering
        checked_increment count; enforce all bounds
        stream canonical entry bytes and remember only last bounded name
    reject duplicate/non-increasing names and declared-count mismatch
    finalize DirectoryNodeIdV1 through the digest port
```

An unordered caller must sort under a later charged/bounded owner before
calling the canonical constructor. M6.1 does not allocate an entry-sized sort
buffer or inspect a host directory.

### 9.4 Canonical physical Tree grouping

```text
group_tree_pages(sorted_entries, caller_scratch):
    fill Leaf pages with exactly 192 entries except the final Leaf
    if one Leaf is sufficient, use the minimum root depth
    otherwise group exactly 96 children per Index page except the final page
    use the minimum depth that represents the entries; reject depth > 2
    derive exact first/last names, subtree counts and typed child IDs
    validate every range is ordered, nonempty, nonoverlapping and child-exact
    emit each completed page immediately; retain no complete tree
```

### 9.5 Exact canonical object comparison

This subsection is conditionally replaced by the exact family-partitioned
algorithm in `M6_1_CONTRACT_AMENDMENT_001.md` only when a separate
`M6_1_CONTRACT_AMENDMENT_001_SEAL_RECEIPT.md` names an independently dual-
reviewed effective-document-set manifest containing the exact SHA-256 of both
this `M6_1_SPEC.md` and that amendment. Until that receipt exists, the linked
amendment is non-authoritative review material, `M61-BLOCK-001` remains open,
and this subsection grants no product-edit authorization. The frozen M6.0
fatal-over-inequality rule remains unchanged while the deterministic
multi-fatal algorithm is gated.

```text
compare_occupied_typed_object(expected_key, family, left, right, windows,
                              supplied_authenticated_edge_facts):
    require two caller-owned windows, each with exactly 65,536-byte capacity
    select exactly one sealed logical, fixed-profile, or physical schedule
    initialize two strict incremental validators for that exact family/type
    remembered_difference := false
    best_fatal := NONE

    while a live side can produce a FatalKey <= best_fatal, or no fatal exists:
        expose both live next canonical semantic ordinals
        choose the minimum global frontier; at equality evaluate left first
        acquire and validate each canonical item exactly once
        feed validated contiguous canonical blocks at sealed <=65,536 boundaries
        remember any byte or length difference; do not return early
        retain the smaller of best_fatal and any observed typed fatal key
        make a fatal side terminal; never call that side again
        never skip an equal-frontier operation

    if best_fatal exists and all live lower bounds are strictly greater:
        return Fatal(best_fatal.typed_failure)
    require simultaneous exact EOF and two complete valid typed values
    require both recomputed typed IDs equal expected_key
    if both valid and all canonical bytes equal: return Identical
    if both valid but any complete byte differs: return DifferentValidated
```

The activated amendment mechanically enumerates the schedules, checkpoints,
predicate applicability, and fatal key for all six logical identity types, all
three fixed profile records, and all five `ELSOBJ01` physical object kinds. It
does not apply an `ELSOBJ01` envelope to logical or profile formats. No
implementation may return immediately on the first discovered fatal. The
activated amendment's O(1) left-first frontier, equal-frontier evaluation,
and strict-greater lower-bound rule exclusively determine when collection is
complete and which fatal wins.

M6.1 returns a pure result only. A later store owner decides that
`DifferentValidated` under one occupied typed key is collision/corruption and
performs quarantine/publication blocking. M6.1 does not open an occupied
carrier or create quarantine state.

### 9.6 Flat export separation

`FlatVersionExportV1` is a separately named deterministic complete-stream
export/verification codec. `FlatVersionDigestV1` is its separately framed
digest. It MUST NOT share a constructor, newtype, text prefix, or return type
with `VersionIdV1`, and neither can construct authority.

## 10. Failure semantics

The core returns typed errors; it does not panic on input, silently normalize,
fall back, retry, allocate a larger representation, or convert failure into a
different path. The immutable 15-code M6.0 base corpus remains normative. The
exact 17-code extension, complete 32-row `CoreError` -> `OutcomeCode` ->
exact-string table, contextual length/bound classification, and deterministic
fatal tie-break in
[M6.1 contract amendment 001](M6_1_CONTRACT_AMENDMENT_001.md) are
**conditional and non-authoritative** until the seal receipt described in
section 9.5 activates one exact independently dual-reviewed effective document
set. At activation they become mechanically normative together and replace
the conflicting pre-amendment first-fatal-return reading of section 9.5. The
three frozen M6.0 vector artifacts remain byte-for-byte unchanged. One explicit
bijective mapping is required, exhaustively tested, and implemented with no
wildcard or catch-all.

Failure families are:

| Family | Examples | Required result |
|---|---|---|
| Structural framing | domain/schema/kind/flags/reserved/length/count/EOF | Deterministic reject before returning a value. |
| Canonical value | path/name/UTF-8/mode/root/order/duplicate/target/extent/tree grouping | Deterministic reject; no normalization. |
| Bound/arithmetic | per-field, aggregate, coupled bound, overflow, cast, offset | Refuse before variable read, loop expansion, scratch request, or caller allocation. |
| Typed edge | wrong child domain, missing required relationship in supplied closure view, length mismatch | Reject the parent result. |
| Digest port | unavailable/failure/wrong output width or completion protocol | Typed fatal result; never substitute SHA-256 or a generic digest. |
| Source/sink port | truncation, source failure, sink refusal, cancellation/deadline | Typed fatal result; no partial value. |
| Exact comparison | identical, validated different, or fatal | Fatal structural/port/resource outcome outranks remembered inequality. |

No M6.1 error grants permission to full-enumerate, implement CDC, copy a file,
use a cache, select a database, or continue with an untyped value.

## 11. Bounded memory and resource contract

M6.1 lands before the shared M6.3 `StorageResourceLedger`, so it MUST remain
effect-free and allocation-safe by construction:

- variable input is borrowed or exposed as bounded windows;
- output and scratch capacity are caller supplied;
- the core requests capacity before consuming the corresponding variable
  count/bytes and rejects refusal;
- no source-sized `Vec`, `String`, `HashMap`, `BTreeMap`, edge list, path list,
  object list, complete tree, complete file, or complete physical object is
  retained by the core;
- directory and Tree transforms retain only fixed fanout/bounded-name state;
- strong edges are visited one at a time;
- two exact-comparison windows have exactly 65,536-byte capacity each and are
  caller-owned (the final used slice may be shorter);
- integer arithmetic is checked before memory/iteration expansion;
- cancellation/error drops borrowed state and leaves no retained global data;
  and
- no static mutable state, thread-local cache, process cache, session cache,
  mmap, FD, queue, spill, pin, or background task exists.

M6.1 therefore proves only its local finite-allocation shape. It does **not**
prove the product's 32/48/72 MiB profiles, the aggregate `B_s` ceiling,
garbage collection, RSS reclamation, or multi-session behavior. Those require
later ledger/store implementation and M8/M9 measurement.

## 12. Durability and crash boundary

M6.1 performs no durable or runtime effect. It creates no file, directory,
pack, index, catalog, selector, journal, cache, quarantine artifact, reference,
Workspace, mount, or background worker. Consequently:

```text
M6.1 product failure/cancellation/crash -> no M6.1 durable state transition
M6.1 typed ID/result                 -> no accepted authority
M6.1 test artifact                   -> no product storage truth
```

Pack durability, occupied-carrier I/O, crash cuts, recovery, collision
quarantine, acceptance, and publication are later milestones. Tests may use
in-memory borrowed byte arrays only.

## 13. Submilestones and binary acceptance

The progress source of truth is [M6.1 progress tracker](M6_1_PROGRESS.md).

### M6.1.0 — fresh custody and start gate

**Depends on:** sealed M6.0.
**Deliverable:** `M6_1_START_RECEIPT.md` with the exact fields in section 3.

Acceptance:

- Receipt identifies the sole canonical product checkout, branch, base, HEAD,
  status, doc seals, allowlist, protected scope, and dependency baseline.
- No product file changed before `M6_1_EDIT_AUTHORIZATION: GO`.
- No P0/P1 custody or contract conflict remains.

### M6.1.1 — crate and dependency boundary

**Depends on:** M6.1.0.
**Deliverable:** mechanical Cargo wiring and empty-compiling core boundary.

Acceptance:

- Only the four authorized Cargo surfaces change.
- Core builds with Rust 1.85/edition 2021, forbids unsafe, and imports no
  forbidden dependency or host effect.
- No BLAKE3 implementation, M6.2 CDC, or later store module lands.

### M6.1.2 — strict codec, paths, bounds, and errors

**Depends on:** M6.1.1.
**Deliverable:** `codec.rs`, `path.rs`, and `error.rs` contract.

Acceptance:

- LE logical and BE physical fields match frozen grammar byte-for-byte.
- Capacity-first checks precede variable read/iteration/allocation requests.
- Exact EOF, path/name/target, root/explicit mode, ordering, duplicate,
  unknown, malformed, incomplete, trailing, and one-over-limit tests pass.

### M6.1.3 — typed identity and digest framing

**Depends on:** M6.1.2.
**Deliverable:** `identity.rs` plus narrow digest/stream ports.

Acceptance:

- Every logical/physical/profile ID is a distinct private-constructor newtype.
- All exact structural domains, three profile records/IDs, and physical
  `ELSHASH1` frames match vectors.
- The core depends on no hash library; deterministic test port proves the
  complete preimage-to-sealed-BLAKE3-output mapping.
- `VersionIdV1`, `VersionRecordId`, and `FlatVersionDigestV1` cannot alias.

### M6.1.4 — immutable records and strong edges

**Depends on:** M6.1.2 and M6.1.3.
**Deliverable:** `object.rs` physical object codec and edge visitor.

Acceptance:

- All five `ELSOBJ01` kinds and exact payload grammars validate.
- Complete typed strong edges are emitted once, in canonical order, without a
  source-sized collection.
- Extent, chunk-ref, reconstructed-length, sparse-hole, tree-page, and graph
  limits fail closed.

### M6.1.5 — root and tree semantics

**Depends on:** M6.1.3 and M6.1.4.
**Deliverable:** `root.rs` and `tree.rs`.

Acceptance:

- Equivalent logical trees produce the same structural `VersionIdV1` across
  input permutation after the caller provides canonical order.
- The implicit root accepts only `0x1000`; explicit directories reject it.
- Physical Tree pages use exact full-then-final grouping, minimum depth,
  fanout, range, count, and typed-child rules.
- No complete tree or source-sized registry is retained.

### M6.1.6 — exact comparison and export separation

**Depends on:** M6.1.3 through M6.1.5.
**Deliverable:** pure exact-comparison state machine and separate flat export.

Acceptance:

- Absent a fatal, comparison validates both inputs through simultaneous exact
  EOF and remembers inequality. After a fatal, it applies the activated
  amendment's fatal-terminal, equal-frontier, and strict-greater collection
  rule; fatal failure always outranks remembered inequality.
- Same bytes are `Identical`; fully valid different bytes are
  `DifferentValidated`; malformed/cancelled/resource-failed input is fatal.
- Flat export/digest cannot become `VersionIdV1` or authority.

### M6.1.7 — golden, hostile, property, and policy evidence

**Depends on:** M6.1.1 through M6.1.6.
**Deliverable:** two test targets and captured command evidence.

Acceptance:

- Every M6.0 M6.1-relevant accepted vector, hostile mutation, expected `S_*`
  code, sentinel case, and typed-object case passes.
- Permutation/backend-neutral fake-port/process-repeat, semantic mutation,
  logical/physical separation, exact EOF, and boundary/one-over properties
  pass deterministically.
- Format, test, clippy, core dependency tree, workspace check, allowlist diff,
  forbidden-symbol/dependency scan, and POC-provenance scan pass.
- No benchmark, E2E, live Docker, M6.2+, M7/control, or Phase 2 work ran.

### M6.1.8 — adversarial closure and finish receipt

**Depends on:** all prior M6.1 submilestones.
**Deliverable:** independent review evidence and `M6_1_FINISH_RECEIPT.md`.

Acceptance:

- At least two independent non-author reviews report exact `P0=0` and `P1=0`
  against the same final product/doc seals.
- Finish receipt records final HEAD/status/diff, exact changed files and LOC,
  commands/results, vector seals, review findings, and scope audit.
- All M6.1 tracker acceptance items are checked with evidence.
- M6.2+ remains `STOP`; completion does not auto-authorize another milestone.

## 14. Required verification commands

The implementor may adapt package names only if the start receipt proves a
mechanical naming conflict. Evidence must include full command, exit code, and
artifact/log path.

```text
cargo fmt --all -- --check
cargo check -p sandbox-runtime-layerstack-core
cargo test -p sandbox-runtime-layerstack-core
cargo clippy -p sandbox-runtime-layerstack-core --all-targets -- -D warnings
cargo check --workspace
cargo tree -p sandbox-runtime-layerstack-core --edges normal,build,dev
cargo metadata --format-version 1 --locked
git diff --check
git status --short
git diff --stat
```

Policy evidence must additionally prove:

```text
CORE_HASH_LIBRARY_DEPENDENCIES: 0
CORE_FILESYSTEM_RUNTIME_PROVIDER_SERDE_FFI_UNSAFE_IMPORTS: 0
REFLINK_CLONE_COPY_OFFLOAD_SYMBOLS_IN_M6_1_DIFF: 0
FUSE_VIRTIOFS_9P_SYMBOLS_OR_PACKAGES_IN_M6_1_DIFF: 0
DB_LOOSE_SIDECAR_CACHE_TRUTH_IN_M6_1_DIFF: 0
HISTORICAL_POC_IMPORTS_OR_FIXTURE_COPIES: 0
OUT_OF_ALLOWLIST_PRODUCT_FILES: 0
BENCHMARK_E2E_LIVE_RUNTIME_RUNS: 0
```

If a word appears only in a denylist test, record the semantic review so a
text hit is not confused with an implementation dependency.

## 15. Global M6.1 acceptance checklist

M6.1 is complete only when every item below is evidenced in the tracker:

- Equivalent logical trees yield the same structural `VersionIdV1`; every
  semantic mutation changes the affected identity frontier.
- Enumeration order, locale, host paths, process, runtime, provider, pack,
  locator, and backend do not affect logical canonical bytes.
- Duplicate, ambiguous, unknown, malformed, trailing, oversized, incomplete,
  or arithmetically unsafe values fail before unsafe work.
- Logical, physical, profile, export, and authority concepts remain distinct.
- Every immutable record enumerates complete typed strong edges.
- Sparse-hole and portable metadata rules match OD-01/OD-06 exactly.
- Absent a fatal, exact comparison processes both canonical objects through
  validated exact EOF. After a fatal, the activated amendment's fatal-terminal,
  equal-frontier, and strict-greater lower-bound law exclusively determines
  safe collection completion and the frozen failure precedence.
- Core memory shape is borrowed/caller-owned and bounded; no source-sized or
  hidden retained state exists.
- Forbidden-dependency, source-ownership, allowlist, and provenance scans pass.
- Golden/hostile/property tests, workspace compile, format, clippy, and diff
  hygiene pass.
- Final independent review has `P0=0/P1=0` and the finish receipt is complete.

## 16. Stop/go handoff

### Immediate stop conditions

Stop M6.1 and leave the tracker `BLOCKED` or `STOPPED` if any of these occurs:

- custody, branch, base, or source drift;
- an edit outside the exact allowlist;
- canonical-byte ambiguity or conflict among sealed authorities;
- need for a hash library in core, host I/O, unsafe, unbounded allocation, or
  hidden state;
- temptation to implement CDC, ledger, pack/catalog/journal/cache, durability,
  materialization, benchmark, M7/control, or Phase 2 work;
- vector mismatch not explained by an implementation defect;
- P0/P1 review finding; or
- test evidence that requires weakening a frozen bound, EOF check, typed edge,
  root sentinel, identity domain, or failure priority.

### Completion does not grant downstream authority

An M6.1 `COMPLETE` receipt means only that the portable identity/codec/record
foundation passed its local contract. It does not prove performance, storage
efficiency, bounded aggregate sandbox memory, RSS reclamation, durability,
publication, GC, native Workspace behavior, or qualification. The owner must
issue a separate explicit start receipt for any subsequent milestone.
