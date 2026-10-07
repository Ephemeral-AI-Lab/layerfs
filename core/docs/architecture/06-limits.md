# End-to-end flows and declared limits

> **Current-source correction:** the changes committed with this paragraph,
> based on `7499d6d56`, replace the fixed 4,096-entry validation walk ceiling
> with a cumulative allowance derived from the operation's declared ordering
> memory. The historical tables below are updated at their former entry.

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

Known-edit memo correction, 2026-10-07, following parent `a41d131f2`:
[EditObjects::load_node](../../crates/layerfs-content/src/file/edit/objects.rs) now
checks a fetched node's grammar/context and actual summary before retaining it,
uses the existing `PageCache::make_room_for(1)` eviction and moves the canonical
allocation into the memo. The default limit is unchanged at 64 pages of at most
8,192 canonical bytes: at most 524,288 canonical bytes retained in that memo.
A miss additionally holds its candidate page and decoded node until validation
and insertion finish. Hash-map allocations, recursive decoded pages, deferred
edit state, provider/Save windows, SQL and OS caches remain separate domains.
This is a correction of this caller's missing enforcement, not a whole resident
bound or removal of K1's deferred-state refusal. See
[the source and public regression scope](03-files.md#known-edit-memo-enforcement-correction-2026-10-07).

The subsequent lazy small-result correction after parent `4090cb9a2` removes
the resident `Vec<Segment>` from WholeFile assembly. A scalar `Plan`, one current
segment and one persistent retained-range cursor feed the cutoff-bounded final
canonical allocation. Object ownership/emission moved to `file/edit/objects.rs`;
the existing split/concat algorithms remain in `file/edit/tree.rs`, with unchanged
public exports. This relocation and segment-container removal do not remove the
8MiB memory-profile draft refusal or growing reference/detached/resolved/emitted
state, and do not establish aggregate resident bounds. See
[the separate lazy/source-organization scope](03-files.md#lazy-small-result-assembly-and-edit-ownership-2026-10-07).
The backed-editor addition following `889836c44` introduces
`apply_edits_backed` without changing canonical file bytes. The existing memory
route retains its 8MiB deferred-state limit and original charge formula. The
backed route places growing draft/reference/detached/resolved/emission records
behind a caller-owned indexed port. Its bounded windows and record codec are
separate from total editor/provider/SQL/process residency; neither a heap charge
nor copy counters qualify a whole-operation bound. See
[backed file edit state](50-backed-file-edit-state.md) and the
[Workspace owning adapter](52-workspace-edit-backing-port.md).

The additive streamed filesystem input avoids materializing all changed names
through legacy directory rows. It retains the existing growing demanded,
addition, topology, count and frontier domains and their resource refusals;
those domains are the next backed-state scope. See
[streamed directory input](51-streamed-directory-input.md).

The subsequent [sparse serial/progress checkpoint](../issues/307/SPARSE-SERIAL-PROGRESS-20261007.md)
after `dcdf52758` records source and functional scope. Selected new-parent/held
membership, rebuilt directory roots and initial counts use the same opaque
construction-record protocol. Sealed header/fresh cursors supply iteration,
membership and final rows without a complete resident count/root/row domain.
Every raw batch still accounts for descriptor and nested Vec capacities within
65,536 bytes. The resident API and remaining validator/reducer/touched/zero/
release refusals remain. See
[backed filesystem serial state](53-backed-filesystem-serial-state.md).

Run-aware replacement spans reuse the frozen Scanner/ExtentBuilder and a separate
positive immutable evidence memo of at most 64 entries. Full referenced byte
domains must be proven before memoization, and summary/context checks apply to
hits. Distinct or evicted identities pay actual demand work; this is no universal
logarithmic bound. See [run-aware localized edits](54-run-aware-localized-file-edits.md).
The [captured input port](55-captured-sparse-run-cursor.md) binds exact retained
inode points and a forward metadata cursor to the original reader/root/floor.
At most four existing layers contribute metadata; a Window is limited to one
4,096-byte cell and its inherited mask. Daemon admission charges the boxed input,
reply and original window, while success clones an independently returned window
with the original credited Completion still held. Those domains exclude transient
composition, caller-retained copies, SQL/pager/OS caches and aggregate residency.
End/drop releases no reader and an absent local row establishes no immutable zero.

[Supervisor observations and parking](45-runtime-supervision.md) retain one
selected phase and at most one original provider unit. The fixed wake latch adds
its own allocation and ownership beyond existing registry gauges. The application
checks bounded control work each turn and after a borrowed park permit before
waiting. Pending actual worker joins prevent parking; wait bounds change no
operation, Save, Workspace or Bash lifetime. Captured normalization, complete
root qualification, original release/Commit composition and E/Q numerical/
resource gates remain open. Component functional checks qualify none of those
aggregate limits or performance gates.

The subsequent [captured-file/reducer checkpoint](../issues/307/CAPTURED-FILE-REDUCER-20261007.md)
implements [captured file normalization](56-captured-file-normalization.md) and
the selected [P13 reference/release state](53-backed-filesystem-serial-state.md).
The file adapter retains one captured cursor, one pending Gap or Window and one
decoded Edit cache. A returned Window must have data length and capacity at most
4,096 bytes, and inherited-mask capacity at most 512 bytes with the exact mask
length for its data. Every returned raw construction-record allocation must fit
65,536 bytes before decode. At most 64 numbered edit descriptors are accumulated
for a guarded batch, within that same capacity allowance; the complete edit
sequence is backed. These are local retained/transient allocation boundaries,
separate from the Content engine, provider copies, SQL/pager/OS state and process
residency. The [borrowed FileView entry point](54-run-aware-localized-file-edits.md)
avoids another root classification and does not add a length vector or replay.

Captured Continue transitions must advance actual metadata probes while retaining
the original reader/serial/size/layer context. Unchanged, restarted or retreating
continuations fail before another demand; there is no iteration cap substituted
for progress. Inherited positions beyond authenticated base EOF become logical
Zero, with no pending masked-byte credit. Disjoint file scopes can normalize
multiple files under one retained capture and operation owner. Reader release
does not resolve that capture, and retained read-after-close supplies no permission
for new scratch writes. Such writes retain the actual Closed completion.

P13 places count/effect rows, touched membership, FIFO release work and cursor
frames behind the same indexed protocol. Each full-key window contains at most
64 keys and each raw batch pays its actual nested capacities. One current cursor
frame and bounded directory/base windows remain local; backing rows grow with
actual work. `ReleaseWork::peak_depth` counts simultaneous cursor frames,
including independent seed siblings, and is not path depth or a total resident
bound. Required row/membership/queue records refuse when missing. Positive count
overflow is typed LengthOverflow, and a final-row error reaches the fallible inode
engine immediately rather than allowing EOF finalization. Original accepted
children, failed Completion/command custody and record owners remain retained;
there is no failed-operation replay or implicit release.

The resident route retains its prior reducer/release allowances. Grouped
validator/addition/graph containers, ordering-derived validator limits and
whole-base/rebound alias walks remain open, as do namespace normalization, P14
qualification and actual same-Save/Save/history/install/unknown integration.
These component bounds and functional checks supply no E/Q numerical gate,
eligible phase residency, physical allocation/reservation/high-water/freelist,
device I/O, sustainable service rate or whole-operation resource acceptance.

The current checkpoint's retained functional bodies total 408 host and 373
Linux. Linux executes only three of the selected SDK bodies; the other 35 remain
macOS-only. The captured fourteen pass on both platforms, with the host's unchanged
thirteen passes retained from failed receipt 16 and only its corrected failed
fixture added by receipt 18. Earlier build failures 07/12 and the actual
CaptureInFlight fixture failure remain. No capacity, clock or physical gate is
inferred from these body counts; final lint/native/source/LOC evidence is separate.

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
        │                                   (ordering bytes / 1024, per operation)
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
| validation walk | `floor(ordering_bytes / 1024)` entries across rebound directories | `validate::walk_limit` |
| read wave | 4,096 objects | `objects::MAXIMUM_READ_DEMANDS` |
| symlink target | 4,096 B | `MAXIMUM_SYMLINK_TARGET_BYTES` |
| attribute domain / key | 64 B / 255 B | `limits` |
| attribute value | 32 KiB | `MAXIMUM_ATTRIBUTE_VALUE_BYTES` |
| attribute key listing | 4,096 keys | `MAXIMUM_ATTRIBUTE_KEYS` |
| inode serial | 1 ..= `i64::MAX` | `InodeIdentity::new` |

### Workspace private backing — keyed namespace tree (#256)

The #264 Phase 4.5 mounted route, based on `6115dfcd2` plus the source in this
commit, validates each component but does not assemble an aggregate path.
Resident directories retain charged ancestor Nodes by serial; rename checks
that live chain under the final state lock. It edits one or two private parent
deltas and the moved resident parent/name edge. Inherited descendants are not
scanned or copied up by mounted rename. Direct C1 `LogicalPath`, path Inspect,
source import and the caller's one-syscall pathname still have their separate
path bounds; component traversal can reach deeper descendants.

The post-review #264 correction in the same commit as this paragraph removes
rename/remove's eager extra-Node reservation and changes the resident
replacement/held-owner lookups to the existing serial index. This prevents an
unrelated `O(P)` Node-table growth or scan from entering directory rename;
the attached-chain and constant-count keyed-page work are its remaining
namespace terms. Backing maintenance remains separately accounted work.

The #264 telemetry follow-up in the same commit as this paragraph exposes
`BackingStatus.metadata_writes`: successful immutable private page-file writes
across the host's arenas. It counts candidate pages even when a later cleanup
retires them; `MetadataStatus.allocated_pages` instead counts pages still
allocated at observation. The two numbers need not match.

C1's current `validate::check_parent_aliases` reads the full base tree when a
stored directory gains a changed binding, and `check_effective_cycles` reads the
effective moved subtree. Those are Commit-time costs on the frozen generation;
the local rename publication is atomic, but the current Commit does not have a
proved path-local complexity bound.

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
legal pair of pages can hold. A page of short removal cells is that shape: a
five-byte name gives an eleven-byte removal cell, so a page holding 128 of them
cannot be split - both halves would be under the 1,024-byte minimum body - and
the next removal in that directory is refused rather than published.

`keyed::cursor` walks the same tree in key order and reads each reached leaf
once, `O(H + L)` page reads rather than `O(H × L)`.

### Frozen namespace lowering and the prepared stream (#256)

One Commit lowers the captured generation once. The frontier walk, the namespace
records and the inode records are three ordered passes over that one keyed tree,
and a maintained directory's final bindings are the ordered merge of its entry
leaves and its removal leaves. A pass reads each reached leaf once, so its page
visits follow the tree's height and leaves rather than the number of names.

The #258 source in the same commit as this paragraph keeps one lookahead record
for each serial-ordered namespace, inode and saved-result cursor. A miss for a
dirty serial no longer consumes the first record for a later serial, which is
required when directory and file identities alternate in the frontier. The
lookahead stays bounded to one fixed record per cursor and preserves the
ordered `O(H + L)` traversal.

The lowered rows do not travel inside the request. A prepared command carries a
*declaration* - the identity of the update plus the exact totals of the ordered
body that follows it - and the rows themselves are written into that body one at
a time: the changed directories by parent serial, then one identity row per dirty
serial (an existing file or symlink with the roots this Commit saved, a fresh one,
a maintained directory's portable patch, or a fresh directory's declaration). The
Workspace measures the declaration by lowering the frozen frontier once without
encoding anything and then lowers it again to write the body, so the figure the
contract admits is a figure both passes produced rather than an estimate over an
earlier capture.

| Limit | Value | Enforced by |
| --- | ---: | --- |
| prepared rows one generation may carry | no count; the rows are the charged frontier | `Workspace::prepared_directories`, `State::frontier_bytes` |
| directories, names or inodes in one prepared update | no count | `prepared_stream::PreparedTotals::check` |
| bytes one prepared request occupies | one metadata frame; it grows with the declaration, not with the rows | `bridge::contract::check_prepared`, `METADATA_BYTES` |
| bytes one prepared body may carry | 256 MiB, charged | `MAX_PREPARED_STREAM_BYTES` |
| resident rows while lowering | one row, plus one declared batch in the directory phase | `commit::stream::PreparedStream` |
| resident rows while receiving | one row, and one charged slot per row on disk | `RowSpool`, `service::save::prepared` |
| prepared rows C1 holds at once | one row per open pass, whatever the row count | `filesystem::rows::RowSource` cursors |

The declaration is exact and it is checked twice: the contract recomputes the body
length from the counts and refuses a declaration whose counts cannot describe one
stream (`Capacity`), the transport streams exactly that many bytes, and the reader
refuses a body that ends anywhere else. A generation wider than one metadata frame
is therefore *carried* - the frame bounds the request, and the request is the
declaration - and what refuses a generation is the charged stream bound and the
spool the service receives it into. The rows a frame used to have to hold are now
on both sides of the wire in files rather than in memory: the workspace pulls them
from its frozen cursors, and the service writes each row into a spool before C1
reads it back. C1 reads them through `RowSource` cursors rather than a resident
slice (`04-filesystem.md` §5.8).

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
