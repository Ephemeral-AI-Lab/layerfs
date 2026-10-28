# End-to-end flows and declared limits

> **Current-source correction:** the changes committed with this paragraph,
> based on `7499d6d56`, replace the fixed 4,096-entry validation walk ceiling
> with a cumulative allowance derived from the operation's declared ordering
> memory. The historical tables below are updated at their former entry.

> **Current-source correction (2026-10-18, #352 limits audit, source
> `efcb6d880`):** §9 now marks every row whose constant no longer exists as
> retired, corrects the dependency pack cache and delta depth defaults, and adds
> the history, merge, depth and time limits that were enforced but undeclared.
> The classified list of every product limit is
> [the #352 limits audit](../issues/352/LIMITS_AUDIT.md). The keyed
> namespace-tree, prepared-stream and Service/Bridge file-save subsections
> describe routes retired by the per-Workspace overlay (#321) and the one-route
> Store (#352 F-020); they are kept as history.

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
operation, Save, Workspace or Bash lifetime. Captured namespace normalization
was added in R4 ([captured namespace construction](78-captured-namespace-construction.md)); complete
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

The resident route retains its prior reducer/release allowances. R4 removed the
whole-base and rebound alias walks, the grouped addition and graph containers
and the ordering-derived validator limits from the backed route, and added
namespace normalization; see
[captured namespace construction](78-captured-namespace-construction.md). P14
qualification and product same-Save/Save/history/install/unknown integration
remain open.
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
     │              ├── batch: 512 objects / 8,191 rows / 4 MiB − 1
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

> **Updated by R4.** `validate::check` now takes the reader, the input and the work counters only, and `CheckedInput` no longer carries `additions`. Validation on the backed route is windowed and keeps its topology evidence in indexed records; see [captured namespace construction](78-captured-namespace-construction.md). The diagram below still shows the call order.

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
| edits per operation | no count cap (retired; not in source at `efcb6d880`: `MAXIMUM_EDITS_PER_OPERATION`); the product edit route is backed | `apply_indexed_edits_view_backed` |

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
| path | 4,096 B / 256 components (the component cap is not a platform limit; audit A4) | `path.rs` |
| operation scratch | 4 MiB | `MAXIMUM_OPERATION_SCRATCH_BYTES` |
| validation topology containers (resident routes only) | `floor(ordering_bytes / 1024)` entries each; the backed route has no such limit | `validate/backed.rs` |
| classification window (backed route) | 64 rows | `validate::CLASSIFICATION_WINDOW_ROWS` |
| read wave | 4,096 objects | `objects::MAXIMUM_READ_DEMANDS` |
| symlink target | 4,096 B | `MAXIMUM_SYMLINK_TARGET_BYTES` |
| attribute domain / key | 64 B / 255 B | `limits` |
| attribute value | 32 KiB | `MAXIMUM_ATTRIBUTE_VALUE_BYTES` |
| attribute key listing | 4,096 keys | `MAXIMUM_ATTRIBUTE_KEYS` |
| inode serial | 1 ..= `i64::MAX` | `InodeIdentity::new` |

### Workspace private backing — keyed namespace tree (#256)

> **Retired route (2026-10-18 correction).** The keyed private-backing tree was replaced by each Workspace's own overlay SQLite file (#321). None of the constants named in this subsection exists at `efcb6d880`; the text is kept as history.

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

Since R4, C1's validation reads no base tree for aliases, and lists a moved
subtree only when a stored directory moves across parents under a stored
directory other than the root
([captured namespace construction](78-captured-namespace-construction.md#validation)).
Before R4, `validate::check_parent_aliases` read the full base tree when a
stored directory gained a changed binding, and `check_effective_cycles` read the
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

> **Retired route (2026-10-18 correction).** Commit no longer sends a prepared stream over the bridge; it constructs in the daemon from the captured overlay (#321) and SDK Store calls run in process (#352 F-020). None of the constants named in this subsection exists at `efcb6d880`; the text is kept as history.

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
| group body (pooled metadata) | retired; not in source at `efcb6d880` | `METADATA_GROUP_LIMIT` |
| group target | 48 KiB framed | `GROUP_TARGET` |
| group count per pack | 256 | `GROUP_COUNT_LIMIT` |
| record count per group | 8,191 | `RECORD_COUNT_LIMIT` |
| canonical object accepted | 16 MiB | `CANONICAL_LIMIT` |
| lookup page ids | retired; not in source at `efcb6d880`; pages follow SQLite's variable and length limits | `Transaction::input_limit` |
| read wave | 4,096 objects | `READ_OBJECT_LIMIT` |
| batch objects | 512 | `BATCH_OBJECT_LIMIT` |
| wave canonical bytes | 4 MiB − 1 (was a 512 KiB batch bound) | `WAVE_CANONICAL_BYTES_LIMIT` |
| group canonical bytes | 512 KiB | `GROUP_CANONICAL_BYTES_LIMIT` |
| transaction rows | 8,191 | `TRANSACTION_ROW_LIMIT` |
| transaction canonical bytes | 4 MiB − 1 | `TRANSACTION_CANONICAL_BYTES_LIMIT` |
| cleanup page rows | retired; not in source at `efcb6d880` | `CLEANUP_PAGE_ROWS` |
| chain canonical / encoded | 512 KiB / 256 KiB | `CHAIN_*_LIMIT` |
| pooled metadata decoded work | retired; not in source at `efcb6d880` | `METADATA_DECODED_WORK_LIMIT` |
| pooled index entries | 131,072 | `METADATA_INDEX_VALUES` |
| pooled value cache | 512 KiB | `POOLED_VALUE_CACHE_BYTES` |
| dependency pack cache | 2 MiB | `DEPENDENCY_PACK_CACHE_BYTES` |
| canonical bytes one Save keeps of objects it resolved (#364) | 4 MiB | `SAVE_RESOLVED_CACHE_BYTES` |
| values per metadata group | retired; not in source at `efcb6d880` | `VALUES_PER_GROUP` |
| pooled leaf rows | 100 | `inode_leaf::MAXIMUM_LEAF_ROWS` |
| private save slots | retired; not in source at `efcb6d880`; one writer at a time (architecture 83) | `MAX_CONCURRENT_WRITES_LIMIT` |
| concurrent writers per Store | one; queued writers wait their turn with no time limit (F-040, record 038; audit A8 and A11 removed) | admission queue, Linux directory lock |
| concurrent reads per service process | retired; not in source at `efcb6d880` | `MAX_READ_OPERATIONS` |
| sessions per Store | retired; not in source at `efcb6d880` | `session_capacity` |
| delta depth (whole-file / chunk / metadata) | 16 / 8 / 8 defaults, ≤ 50 | policy validation |

### C5 — history walks (#353, 2026-10-18)

History has no count limit. `MAXIMUM_LINEAGE_ROWS` (4,096), which refused
`is_ancestor`, `merge_base`, publish's skipped-Layer walk and fork-from-Commit
with `Capacity("lineage work")` past 4,096 rows, is deleted (#352 F-033). The
walks read lineage records instead
([16-history](16-history.md), [#353 design](../issues/353/DESIGN.md)):

| Limit | Value | Enforced by |
| --- | ---: | --- |
| Commits since a Branch's base Layer | none | `lineage.rs` runs and absorption |
| Layers between two sides | none | `layer_lineage.height` |
| first-parent depth of a fork or history start | none | `commit_lineage` run and depth |
| overwritten Layers per publication | none; paged, 128 per catalog page | `overwritten`, `MAXIMUM_PAGE_RECORDS` |

What a walk still costs: a run segment and a merge at a time over the local
Commits no Layer has absorbed; a fork from a Commit, one step per run on the
chain. Disk and memory as admitted are the only bounds.

### C1 — merge (#355, 2026-10-18)

A merge has no count limit. `MAXIMUM_MERGE_PATHS` (262,144 decided paths),
which refused a merge past it with `BoundedCapacityExceeded`, is deleted. The
in-memory build budget (65,536 topology entries) no longer applies to a merge:
since #355 the merge result is built on the backed route
(`update_filesystem_streamed_backed` with the plan's spill records,
`merge/build.rs`), which uses the indexed reducer, and the resident caps
(`ordering_bytes/1024` topology entries, `ordering_bytes/16` touched serials)
are checked only when no backing holds the state (`validate.rs`, `update.rs`). The merge's
working state lives in the global Store's merge scratch rows
([81-compare-and-merge](81-compare-and-merge.md)). Since M1 the merge walks
changed serials, not candidate paths:

| Limit | Value | Enforced by |
| --- | ---: | --- |
| changed serials per merge | none | a run and point records in `merge_record` |
| merge state held in memory before it spills | 8 MiB | `merge/spill.rs` `HELD_BYTES` |
| run chunk | 60 KiB | `CHUNK_BYTES` |
| merge records held resident before any is written to the file (#364) | 8 MiB | `storage/merge_records.rs` `RESIDENT_BYTES` |
| record write-back buffer, once past the resident bound | 256 KiB | `storage/merge_records.rs` `BUFFER_BYTES` |
| changed serials the in-lane Commit step decides | 1,024; more refuses the Commit `LaneMerge` | `LANE_MERGE_PATHS` |
| written paths a conflicted Commit lists | 4,096, `truncated` when cut | `WRITTEN_PATHS` |

The in-lane bound keeps the lane's work independent of the tree; it is a
refusal of that Commit, never a partial merge. Disk as admitted is the only
other bound.

### History, merge, depth and time (enforced, undeclared until 2026-10-18)

These refuse work on a count, depth or time rather than a resource. They are
candidate findings (#352 F-033 and [audit](../issues/352/LIMITS_AUDIT.md) §1),
not declared product contracts.

| Limit | Value | Enforced by |
| --- | ---: | --- |
| Commits since the base Layer, Layers between sides, first-parent depth | none; the 4,096-row cap is deleted (#353, C5 above) | `MAXIMUM_LINEAGE_ROWS` (F-033), not in source |
| paths one merge may decide | none; the 262,144 cap is deleted (#355, C1 above) | `MAXIMUM_MERGE_PATHS`, not in source |
| merge result topology | none on the merge route since #355: the build is backed (C1 above); the 65,536-entry resident cap applies only to an unbacked build. `merge/rows.rs`'s `FilesystemResources::default()` only supplies figures the backed route does not check (audit A3) | `validate.rs` `resident_limit`, only when `!state.backed()` |
| Commit message | 4,096 B, no control character but LF and TAB | `MAXIMUM_MESSAGE_BYTES`, `CommitAttribution::check` |
| directory moved under a parent | at most 256 levels deep | `kernel_mutation::ANCESTRY_STEPS` |
| LWW rule include files | 16; later includes are ignored | `lww::MAXIMUM_RULE_FILES` |
| writer wait | none since F-040: queued writers wait their turn with no time limit ([record 038](../issues/352/fixes/038-f040-untimed-writer-admission.md); audit A8 and A11 removed) | admission queue, Linux directory lock |
| SDK `project.init` | the caller's required `timeoutMs`; `@layerfs/agent` sends 300 s unless given one (`INIT_TIMEOUT_MS`); no unlimited option (#431; the fixed 60 s `INIT_DEADLINE` is deleted) | `engine/crates/layerfs-node/src/project.rs` `deadline`, `api/agent/src/project/init.js` |
| mounts per SDK Sandbox / control connections | 16 / 8 (fixed by the SDK) | `sandbox.rs` `DaemonLimits` |

### Service/Bridge — final file save (#252)

> **Retired route (2026-10-18 correction).** The bridge file-save stream and its spools are gone: SDK file calls run in process (#352 F-020) and mounted writes go through the overlay. None of the constants named in this subsection exists at `efcb6d880`; the text is kept as history.

| Limit | Value | Enforced by |
| --- | ---: | --- |
| logical final file | retired; not in source at `efcb6d880`; file size is bounded by `i64::MAX` | `operations/file/write.rs` |
| wire descriptor records | 24 B each; count charged by bytes, no fixed count cap | `Operation::SaveFile::input_length` |
| descriptor plus non-base byte body | 8 GiB | `MAX_SAVE_STREAM_BYTES` |
| derived edit, zero-range and byte spools | 8 GiB total | `file_stream::SPOOL_DISK_BYTES` |
| Service resident replacement window | 64 KiB | `file_stream::WINDOW_BYTES` |

### Three bounds that are easy to misread

> **Updated by R4.** This first bound no longer exists. There is no validation walk ceiling: a directory of any size can be renamed or relocated, a rename inside its own parent lists nothing, and a move across parents under a stored directory lists the moved subtree once without a limit. `InvalidRecord("cycle check work limit")` is retired. See [captured namespace construction](78-captured-namespace-construction.md). The text of bound 1 is kept as history.

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

**2. Twenty-six constants are `4_096`, and eight of them are not counts.**
`MAXIMUM_PATH_BYTES`, `MAXIMUM_SYMLINK_TARGET_BYTES`, `SINGLETON_FRAMING_SLACK`
and five others are **byte** sizes; only two are read-wave batches, one is the
history lineage cap, and none is the write batch (512 objects / 4 MiB − 1). The full disambiguation is
[§15.4](10-counters.md#154-the-4096s--a-disambiguation).

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
