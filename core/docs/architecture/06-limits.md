# End-to-end flows and declared limits

> **Status:** Research; informative and not a product contract.

The issue #192 schema 8 changes, including the configured per-Store writer
budget of #216, are described in
[save ownership and publication](15-multi-writer-storage.md), based on
`152b9c3a2e8ec2536a1d63601b681e1f7ef34455` plus that change. That description supersedes the
older exclusive-save, prefix-publication, shared-private-cache and cleanup rules
below, and adds the returned-read byte bound and bounded ordinal window. The
older source-pinned sections remain historical descriptions; they do not qualify
the pending implementation or a performance change.

Part of the [replacement-core architecture](README.md) set. Source pin
`1884e3eca`; scope, method, measurement status and upkeep are stated in the
[index](README.md).
The base-less build-count correction in this page describes product commit
`64ea3ea8aa213edb8991e958829aeb87c6bfd16d`; older sections retain their
own source pin.
The Service/Bridge file-save limits below describe the #252 source in the same
commit as this note; older flow diagrams retain their historical source pins.
The keyed namespace-tree subsection of §9 describes the #256 source of phase
3's slices 3.1 and 3.2 (`bd907b5ba` + 1); older sections retain their own pins.

Chapter numbers are global to the set. This paper holds chapters 7 and 9;
**chapter 8** (module map) and **chapter 10** (what the set does not claim) are in
the [index](README.md), which also carries the [contents table](README.md#contents)
and the [source index](README.md#references).

---

## 7. End-to-end flows

### 7.1 Save: C1 constructs, C2 stores

```text
   caller
     │
     │  1. Store::create(path, policy)  or  Store::open(path)
     │        └── validates everything BEFORE any object work
     │
     │  2. let mut save = store.begin_save(scope)?        ← exclusive ownership ONCE
     │
     │  3. C1: construct_bytes(policy, capacities, bytes, &mut save, scope)
     │        │
     │        ├── policy.validated()
     │        ├── representation(len) ─┐
     │        │                        └─ Empty │ WholeFile │ Chunked
     │        ├── encode  ──► content.encode
     │        ├── identify ──► ObjectId::for_bytes  ──► content.identify
     │        └── consumer.accept(object)           ──► content.emit
     │                 │
     │                 ▼
     │       4. SaveOperation::accept(FinalizedObject)
     │              │  takes ownership of id, role, canonical,
     │              │  references AND predecessors
     │              │
     │              ├── exact CAS reuse check:  is this id already stored?
     │              │        ├── yes ──► reused += 1        (no new record)
     │              │        └── no  ──► select FULL vs PREFIX ([§6.8](05-storage.md#68-physical-representation-selection--policy-versus-failure))
     │              │
     │              ├── frame the record in its LANE (PackLane::for_role)
     │              ├── batch: 512 objects / 512 KiB / 8,191 rows / 4 MiB − 1
     │              └── early transactions MAY commit — packs stay INVISIBLE
     │
     │  5. save.finish(scope)?  ──► advances retained_pack_ceiling
     │        │
     │        └── returns SaveOutcome   ← HOLDING THIS IS THE ACKNOWLEDGEMENT
     │
     ▼
   reader (any time after) sees exactly the packs whose save was acknowledged
```

### 7.2 Read: C2 resolves, C1 assembles

```text
   caller holds a file root ObjectId
     │
     │  C1: read_all_bounded(store_provider, root, maximum, sink, scope)
     │
     ├── content.acquire ──► C2 StoreProvider::read_canonical_batch_scoped
     │        │                    │  (the SCOPED override — C2's real work becomes
     │        │                    │   a named child, not merged into C1's duration)
     │        │                    │
     │        │                    ├── Store::read_canonical_batch([root])
     │        │                    │     ├── grouped SQL lookup → locator
     │        │                    │     ├── pack body read (≤ wave ceiling)
     │        │                    │     └── decode_canonical → canonical bytes
     │        │                    └── values in demand order, exact cardinality
     │        │
     │        ▼
     ├── content::classify(canonical)  ──► WholeFile { logical_len }
     │                                 │  or Chunked(FileState)
     │        │
     │        └── logical_len > maximum ──► BoundedCapacityExceeded  (fail closed)
     │
     ├── WholeFile ──► emit the requested slice directly
     │
     └── Chunked   ──► bounded extent traversal
                          │  read mapping pages in bounded batches
                          │  one decoded leaf at a time
                          │  acquire payload objects (≤ 4,096 per wave)
                          └──► bytes to sink IN LOGICAL ORDER
```

### 7.3 A localized edit against storage

```text
   caller holds base root + a validated EditSequence
     │
     │  C1: apply_edits(policy, capacities, provider, EditRequest{..}, &mut save, scope)
     │
     ├── edit.base       ──► FileView::open ──► acquire + classify base ONCE
     ├── edit.base       ──► edit.base_read  (the acquire child)
     ├── edit.compare    ──► compare_replacements (bounded COMPARE_WINDOW_BYTES)
     │        └── all byte-identical ──► return BASE ROOT (no new objects)
     │
     ├── Chunked route:
     │        │  split old mapping at start ──► left  REUSED by ObjectId
     │        │  split tail at end           ──► right REUSED by ObjectId
     │        │  boundary extents SLICED, never re-CDC'd
     │        │  only replacement bytes ──► FastCdc ──► new chunk objects
     │        └── concat + coalesce ──► new FileState
     │
     └── every emitted object ──► save.accept(...)   (same path as [§7.1](06-limits.md#71-save-c1-constructs-c2-stores))
```

The storage side sees only ordinary finalized objects. **C2 has no notion of an
edit** — it stores FULL/PREFIX records for the objects it is handed. The
localization lives entirely in C1, which is why editing costs can be attributed to
one component rather than split across both.

### 7.4 Filesystem update

```text
   C1: update_filesystem(&mut FilesystemObjects{reader, consumer}, input, backing)
        │
        ├── validate::check           ──► CheckedInput + FilesystemTopology
        │                                   (bounded MAXIMUM_WALK_ENTRIES per walk)
        ├── directory bindings merge  ──► sorted B+tree engine (LFS6NSP)
        │        └── observes every original → final binding edge
        ├── reference reduce          ──► ReferenceReducer → FinalRows
        │        └── spill to runs when pending exceeds 4,096
        │             (the dial: `FilesystemResources.maximum_pending_records`;
        │              spill-free while `touched <= ordering_bytes / (2 x 96)`,
        │              i.e. 349,525 rows under the default 64 MiB ceiling)
        ├── release_zero_count        ──► bounded traversal of zeroed descendants
        ├── inode table rebuild       ──► sorted engine (LFS6INT), ONE pass
        └── FilesystemRoot emission   ──► new 116-byte root object
        │
        ▼
   FilesystemResult { root: FilesystemRootId, value: FilesystemRoot, counters }
        │
        └── every finalized page ──► consumer ──► (optionally) save.accept(...)
```

---

## 9. Declared limits

Every bound below is enforced by a named check. The "enforced by" column names
where, because a limit that is stated but not enforced is not a limit.

### C1 — objects and files

| Limit | Value | Enforced by |
| --- | ---: | --- |
| canonical object | 16 MiB | `codec::canonical_len`, `decode_bytes_object` |
| object value field | 8 MiB | `codec::canonical_len` |
| chunk payload | 8 / 16 / 32 KiB | `cdc::gear` (frozen profile) |
| whole-file raw | cutoff − 1 | `encode_whole_file` |
| mapping entries per page | 64 – 128 | `ExtentNode::validate` |
| mapping tree level | ≤ 31 | `ExtentNode::validate` |
| mapping node object | 8,192 B | `MAX_NODE_OBJECT_BYTES` |
| edits per operation | `MAXIMUM_EDITS_PER_OPERATION` | `edit::input` |

### C1 — filesystem

| Limit | Value | Enforced by |
| --- | ---: | --- |
| tree page | 8,192 B | `limits::MAXIMUM_PAGE_BYTES` |
| empty page | 44 B | `format::EMPTY_PAGE_BYTES` |
| node header | 31 B | `format::NODE_HEADER_BYTES` |
| inode leaf rows (non-root) | 50 – 100 | `limits`, `inode_leaf` |
| inode branch children (non-root) | 64 – 127 | `limits` |
| directory/attribute fill (non-root) | ≥ 2/5 of page | `limits::filled_page` |
| name component | 255 B | `MAXIMUM_NAME_BYTES` |
| path | 4,096 B / 256 components | `path.rs` |
| operation scratch | 4 MiB | `MAXIMUM_OPERATION_SCRATCH_BYTES` |
| validation walk | 4,096 bindings **per walk** | `MAXIMUM_WALK_ENTRIES` |
| read wave | 4,096 objects | `objects::MAXIMUM_READ_DEMANDS` |
| symlink target | 4,096 B | `MAXIMUM_SYMLINK_TARGET_BYTES` |
| attribute domain / key | 64 B / 255 B | `limits` |
| attribute value | 32 KiB | `MAXIMUM_ATTRIBUTE_VALUE_BYTES` |
| attribute key listing | 4,096 keys | `MAXIMUM_ATTRIBUTE_KEYS` |
| inode serial | 1 ..= `i64::MAX` | `InodeIdentity::new` |

### Workspace private backing — keyed namespace tree (#256)

The keyed tree holds one Workspace generation's dirty identities, inodes and
directory bindings. Its bounds are page-format bounds; the *number* of names or
dirty identities a generation may hold is not bounded by a constant, it is
charged.

| Limit | Value | Enforced by |
| --- | ---: | --- |
| private page / header | 4 KiB / 128 B | `metadata_pages::PAGE`, `HEADER` |
| cells per keyed page | 128 | `metadata_pages::MAX_CELLS` |
| declared body of a non-root page | ≥ 1 KiB | `metadata_pages::MIN_BODY`, checked on every descent |
| keyed tree level | ≤ 3 name kinds, ≤ 2 identity kinds | `key_limit` |
| distance from a page's fill to a sibling rewrite | one neighbour, same level | `keyed::delete::repair` |
| generation frontier | no count admission; the exact prepared-namespace bytes are reserved from the host budget | `State::frontier_bytes`, `Host::budget` |

A removal keeps the same invariants as an insertion. The leaf that holds the key
is copied along its own path; a page the removal leaves under 1 KiB is rewritten
with one neighbour at its own level, merged into a single page when the pair fits
one and split between two legal pages when it does not. A branch that keeps one
child is repaired one level up; a root that keeps one child is lifted out, so the
published height follows the key count and never the deletion history. One shape
is refused rather than published: a pair of pages whose cells total more than one
page's 128 cells while their bytes are fewer than two pages' minimum, which no
legal pair of pages can hold. `keyed::cursor` walks the same tree in key order
and reads each reached leaf once, `O(H + L)` page reads rather than `O(H × L)`.

### C2 — storage

| Limit | Value | Enforced by |
| --- | ---: | --- |
| pack (ordinary/native/whole-file/pooled) | 256 KiB | `PACK_LIMIT` |
| pack (singleton) | 16 MiB + 4 KiB | `SINGLETON_PACK_LIMIT` |
| group body (ordinary/native/whole-file) | 65,536 | `GROUP_LIMIT` |
| group body (pooled metadata) | 16 KiB | `METADATA_GROUP_LIMIT` |
| group target | 48 KiB framed | `GROUP_TARGET` |
| group count per pack | 256 | `GROUP_COUNT_LIMIT` |
| record count per group | 8,191 | `RECORD_COUNT_LIMIT` |
| canonical object accepted | 16 MiB | `CANONICAL_LIMIT` |
| lookup page ids | 128 | `LOOKUP_PAGE_IDS` |
| read wave | 4,096 objects | `READ_OBJECT_LIMIT` |
| batch objects | 512 | `BATCH_OBJECT_LIMIT` |
| batch canonical bytes | 512 KiB | `BATCH_CANONICAL_BYTES_LIMIT` |
| transaction rows | 8,191 | `TRANSACTION_ROW_LIMIT` |
| transaction canonical bytes | 4 MiB − 1 | `TRANSACTION_CANONICAL_BYTES_LIMIT` |
| cleanup page rows | 128 | `CLEANUP_PAGE_ROWS` |
| chain canonical / encoded | 512 KiB / 256 KiB | `CHAIN_*_LIMIT` |
| pooled metadata decoded work | 32 MiB | `METADATA_DECODED_WORK_LIMIT` |
| pooled index entries | 131,072 | `METADATA_INDEX_VALUES` |
| pooled value cache | 512 KiB | `POOLED_VALUE_CACHE_BYTES` |
| dependency pack cache | 4 MiB | `DEPENDENCY_PACK_CACHE_BYTES` |
| values per metadata group | 165 | `VALUES_PER_GROUP` |
| pooled leaf rows | 100 | `POOLED_LEAF_ROWS_LIMIT` |
| private save slots (supported space) | 64 | `MAX_CONCURRENT_WRITES_LIMIT`, `saves.active_slot` CHECK |
| concurrent writers per Store (admitted) | 2 default, 1..=64 | persisted `store_policy.max_concurrent_writes` |
| concurrent reads per service process | 2 | `MAX_READ_OPERATIONS` |
| sessions per Store | budget + 2, +1 refusal slot | `session_capacity` |
| delta depth (whole-file / chunk / metadata) | 8 / 4 / 8 defaults, ≤ 50 | policy validation |

### Service/Bridge — final file save (#252)

| Limit | Value | Enforced by |
| --- | ---: | --- |
| logical final file | 4 GiB | `MAX_FILE` and final-sequence validation |
| wire descriptor records | 24 B each; count charged by bytes, no fixed count cap | `Operation::SaveFile::input_length` |
| descriptor plus non-base byte body | 8 GiB | `MAX_SAVE_STREAM_BYTES` |
| derived edit, zero-range and byte spools | 8 GiB total | `file_stream::SPOOL_DISK_BYTES` |
| Service resident replacement window | 64 KiB | `file_stream::WINDOW_BYTES` |

### Three bounds that are easy to misread

**1. The validation walk ceiling is charged per walk, not per operation.** The
source is explicit, and the consequence is a real behavioral boundary rather than
an implementation detail:

- a base-less `build_filesystem` call walks only its supplied binding vector and
  has no independent entry-count ceiling. Its work and memory still grow with
  that input;
- an existing directory whose **effective subtree** reaches the ceiling **can
  never be renamed or relocated**, however small the change is, because the walk of
  the base tree charges the rest of the tree beside the rebound directory first.

Exceeding a work bound is an **explicit refusal** — "the same error a genuine cycle
gets, **never a claim that the tree was proven acyclic**". That last clause matters:
the failure must not be reportable as a proof.

**2. Twelve constants are named `4_096`, and three of them are not counts.**
`MAXIMUM_PATH_BYTES`, `MAXIMUM_SYMLINK_TARGET_BYTES` and `SINGLETON_FRAMING_SLACK`
are **byte** limits; only two of the twelve are read-wave batches, and none is the
write batch (512 objects / 512 KiB). The full disambiguation is
[§15.4](10-counters.md#154-the-twelve-4096s--a-disambiguation).

**3. `MAXIMUM_TREE_LEVEL` is derived, not restated.**
`limits::MAXIMUM_TREE_LEVEL = file::mapping::MAX_LEVEL` ("one owner, one name"),
and the source records that two independent `31`s used to sit in the tree — "which
is how a reader loses track of which enforcement uses which". The same discipline
appears in `MAXIMUM_ATTRIBUTE_VALUE_BYTES` (derived from the chunk maximum) and
`MAXIMUM_SCRATCH_BYTES` (one name for one figure, after a `4 MiB` and a
`4 MiB − 1` twin coexisted with no caller on the twin).

**4. Attribute values are bounded by the chunk grammar, not by a round number.**
`MAXIMUM_ATTRIBUTE_VALUE_BYTES` is derived from `cdc::MAXIMUM_CHUNK_BYTES` because
an attribute value is stored as one extent-only root over one canonical chunk
object. The source states the reason a larger constant would be wrong: "a bound
above it would be a limit no write path can reach and no read path can return."
